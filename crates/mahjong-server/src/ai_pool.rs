//! Bounded Rayon AI worker pool construction and asynchronous Tokio dispatch.

use std::sync::Arc;

use mahjong_ai::DecisionContext;
use mahjong_core::{PlayerView, Seat};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::sync::Mutex;

use crate::pacing::{overlapped_wait, AiTurnTrigger, ReleaseTicket};
use crate::server::ServerError;
use crate::session::TableSession;

/// Computes the default worker count for the dedicated Rayon AI thinking pool:
/// `(available_parallelism / 2).clamp(1, 4)` (or `1` if parallelism cannot be queried).
#[must_use]
pub fn default_ai_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4))
}

/// Builds the dedicated bounded Rayon AI thread pool (`majjj-ai-{i}`).
///
/// # Errors
///
/// Returns [`ServerError::InvalidAiThreads`] if `ai_threads == 0`, or
/// [`ServerError::AiPool`] if the Rayon thread pool cannot be constructed.
pub(crate) fn build_ai_pool(ai_threads: usize) -> Result<Arc<rayon::ThreadPool>, ServerError> {
    if ai_threads == 0 {
        return Err(ServerError::InvalidAiThreads(0));
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(ai_threads)
        .thread_name(|i| format!("majjj-ai-{i}"))
        .build()
        .map_err(|err| ServerError::AiPool(err.to_string()))?;
    Ok(Arc::new(pool))
}

pub(crate) struct PendingAiJob {
    pub match_id: u64,
    pub seq: u64,
    pub seat: Seat,
    pub view: PlayerView,
    pub ctx: DecisionContext,
    pub step_seed: u64,
    pub started_at: tokio::time::Instant,
    pub trigger: AiTurnTrigger,
}

#[derive(Default)]
pub(crate) struct ScheduledWork {
    pub ai_jobs: Vec<PendingAiJob>,
    pub pending_release: Option<ReleaseTicket>,
}

impl ScheduledWork {
    pub(crate) fn is_empty(&self) -> bool {
        self.ai_jobs.is_empty() && self.pending_release.is_none()
    }
}

pub(crate) fn spawn_scheduled_work(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    work: ScheduledWork,
) {
    if let Some(ticket) = work.pending_release {
        let runtime = Arc::clone(&runtime);
        let ai_pool = Arc::clone(&ai_pool);
        tokio::spawn(async move {
            let now = tokio::time::Instant::now();
            if ticket.release_at > now {
                tokio::time::sleep_until(ticket.release_at).await;
            }
            let next_work = {
                let mut session = runtime.lock().await;
                session.complete_pending_release(ticket)
            };
            if !next_work.is_empty() {
                spawn_scheduled_work(runtime, ai_pool, next_work);
            }
        });
    }

    for job in work.ai_jobs {
        let runtime = Arc::clone(&runtime);
        let ai_pool_for_next = Arc::clone(&ai_pool);
        let ai_pool_for_job = Arc::clone(&ai_pool);
        tokio::spawn(async move {
            let PendingAiJob {
                match_id,
                seq,
                seat,
                view,
                ctx,
                step_seed,
                started_at,
                trigger,
            } = job;
            let (tx, rx) = tokio::sync::oneshot::channel();
            ai_pool_for_job.spawn(move || {
                let mut rng = ChaCha8Rng::seed_from_u64(step_seed);
                let action = mahjong_ai::decide(&view, &ctx, &mut rng);
                let _ = tx.send(action);
            });

            let Ok(action) = rx.await else {
                let mut session = runtime.lock().await;
                session.cancel_ai_job(match_id, seq, seat);
                return;
            };

            let planned_delay = {
                let mut session = runtime.lock().await;
                session.plan_ai_action_delay(match_id, seq, seat, action, trigger, step_seed)
            };
            let Some(target_delay) = planned_delay else {
                return;
            };

            let wait = overlapped_wait(started_at, tokio::time::Instant::now(), target_delay);
            if !wait.is_zero() {
                tokio::time::sleep_until(started_at + target_delay).await;
            }

            let next_work = {
                let mut session = runtime.lock().await;
                session.complete_ai_job(match_id, seq, seat, action)
            };
            if !next_work.is_empty() {
                spawn_scheduled_work(runtime, ai_pool_for_next, next_work);
            }
        });
    }
}

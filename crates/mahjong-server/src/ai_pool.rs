//! Bounded Rayon AI worker pool construction and asynchronous Tokio dispatch.

use std::sync::Arc;

use mahjong_ai::DecisionContext;
use mahjong_core::{PlayerView, Seat};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::sync::Mutex;

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
}

pub(crate) fn spawn_ai_jobs(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    jobs: Vec<PendingAiJob>,
) {
    for job in jobs {
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

            let next_jobs = {
                let mut session = runtime.lock().await;
                session.complete_ai_job(match_id, seq, seat, action)
            };
            if !next_jobs.is_empty() {
                spawn_ai_jobs(runtime, ai_pool_for_next, next_jobs);
            }
        });
    }
}

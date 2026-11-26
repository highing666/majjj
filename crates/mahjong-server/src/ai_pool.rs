//! Bounded Rayon AI worker pool construction and asynchronous Tokio dispatch.

use std::sync::Arc;

use mahjong_ai::DecisionContext;
use mahjong_core::{PlayerView, Seat};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::sync::Mutex;

use crate::banter::{PendingBanterJob, ASYNC_BANTER_TIMEOUT, HIGHLIGHT_SYNC_CAP};
use crate::llm::{LlmClient, LlmError};
use crate::pacing::{overlapped_wait, AiTurnTrigger, ReleaseTicket};
use crate::persona_memory::MemoryNoteJob;
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

/// Unique key identifying an in-flight AI decision job bound to match, sequence, seat, and generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AiJobKey {
    pub match_id: u64,
    pub seq: u64,
    pub seat: Seat,
    pub seat_generation: u64,
}

pub(crate) struct PendingAiJob {
    pub key: AiJobKey,
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
    pub sync_banter: Vec<PendingBanterJob>,
    pub async_banter: Vec<PendingBanterJob>,
    pub memory_notes: Vec<MemoryNoteJob>,
}

impl ScheduledWork {
    pub(crate) fn is_empty(&self) -> bool {
        self.ai_jobs.is_empty()
            && self.pending_release.is_none()
            && self.sync_banter.is_empty()
            && self.async_banter.is_empty()
            && self.memory_notes.is_empty()
    }
}

async fn run_banter_job_with_timeout(
    llm: &Arc<dyn LlmClient>,
    job: PendingBanterJob,
    timeout: std::time::Duration,
) -> (
    crate::banter::BanterGuard,
    Result<crate::llm::BanterOutput, LlmError>,
) {
    let endpoint = llm.endpoint().to_string();
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    let res = match tokio::time::timeout(timeout, llm.generate_banter(job.request)).await {
        Ok(inner) => inner,
        Err(_) => Err(LlmError::Timeout {
            endpoint,
            timeout_ms,
        }),
    };
    (job.guard, res)
}

fn spawn_gameplay_work(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    llm_client: Option<Arc<dyn LlmClient>>,
    pending_release: Option<ReleaseTicket>,
    ai_jobs: Vec<PendingAiJob>,
) {
    if let Some(ticket) = pending_release {
        let runtime = Arc::clone(&runtime);
        let ai_pool = Arc::clone(&ai_pool);
        let llm_client = llm_client.clone();
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
                spawn_scheduled_work(runtime, ai_pool, llm_client, next_work);
            }
        });
    }

    for job in ai_jobs {
        let runtime = Arc::clone(&runtime);
        let ai_pool_for_next = Arc::clone(&ai_pool);
        let ai_pool_for_job = Arc::clone(&ai_pool);
        let llm_client = llm_client.clone();
        tokio::spawn(async move {
            let PendingAiJob {
                key,
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
                session.cancel_ai_job(key);
                return;
            };

            let planned_delay = {
                let mut session = runtime.lock().await;
                session.plan_ai_action_delay(key, action, trigger, step_seed)
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
                session.complete_ai_job(key, action)
            };
            if !next_work.is_empty() {
                spawn_scheduled_work(runtime, ai_pool_for_next, llm_client, next_work);
            }
        });
    }
}

pub(crate) fn spawn_scheduled_work(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    llm_client: Option<Arc<dyn LlmClient>>,
    work: ScheduledWork,
) {
    let ScheduledWork {
        ai_jobs,
        pending_release,
        sync_banter,
        async_banter,
        memory_notes,
    } = work;

    if let Some(llm) = llm_client.as_ref() {
        for job in async_banter {
            let runtime = Arc::clone(&runtime);
            let llm = Arc::clone(llm);
            tokio::spawn(async move {
                let (guard, result) =
                    run_banter_job_with_timeout(&llm, job, ASYNC_BANTER_TIMEOUT).await;
                let mut session = runtime.lock().await;
                session.commit_banter_result(guard, result);
            });
        }
        for job in memory_notes {
            let runtime = Arc::clone(&runtime);
            let llm = Arc::clone(llm);
            tokio::spawn(async move {
                let endpoint = llm.endpoint().to_string();
                let timeout_ms =
                    u64::try_from(ASYNC_BANTER_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
                let result = match tokio::time::timeout(
                    ASYNC_BANTER_TIMEOUT,
                    llm.generate_memory_note(job.request),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_) => Err(LlmError::Timeout {
                        endpoint,
                        timeout_ms,
                    }),
                };
                let mut session = runtime.lock().await;
                session.commit_memory_note_result(
                    job.persona_id,
                    &job.player_nickname,
                    job.trigger,
                    result,
                );
            });
        }
    }

    if !sync_banter.is_empty() {
        if let Some(llm) = llm_client.clone() {
            tokio::spawn(async move {
                let mut handles = Vec::with_capacity(sync_banter.len());
                for job in sync_banter {
                    let cap = job.sync_cap.unwrap_or(HIGHLIGHT_SYNC_CAP);
                    let llm = Arc::clone(&llm);
                    handles.push(tokio::spawn(async move {
                        run_banter_job_with_timeout(&llm, job, cap).await
                    }));
                }

                for handle in handles {
                    if let Ok((guard, result)) = handle.await {
                        let mut session = runtime.lock().await;
                        session.commit_banter_result(guard, result);
                    }
                }

                if pending_release.is_some() || !ai_jobs.is_empty() {
                    spawn_gameplay_work(runtime, ai_pool, Some(llm), pending_release, ai_jobs);
                }
            });
            return;
        }
    }

    spawn_gameplay_work(runtime, ai_pool, llm_client, pending_release, ai_jobs);
}

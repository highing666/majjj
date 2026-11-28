//! Bounded Rayon AI worker pool construction and asynchronous Tokio dispatch.

use std::sync::{Arc, Weak};

use futures_util::StreamExt;
use mahjong_ai::DecisionContext;
use mahjong_core::{PlayerView, Seat};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use tokio::sync::Mutex;

use crate::banter::{
    BanterChainContext, BanterGuard, DelayedBanterFollowup, PendingBanterJob, SlowThinkTicket,
    ASYNC_BANTER_TIMEOUT,
};
use crate::banter_audio::{BeginTtsStreamOutcome, PcmEvenAligner, PendingTtsJob};
use crate::llm::{LlmClient, LlmError};
use crate::pacing::{overlapped_wait, AiTurnTrigger, ReleaseTicket};
use crate::persona_memory::MemoryNoteJob;
use crate::server::ServerError;
use crate::session::TableSession;
use crate::tts::{TtsClient, TtsError};

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
    pub async_banter: Vec<PendingBanterJob>,
    pub slow_think_nudge: Option<SlowThinkTicket>,
    pub memory_notes: Vec<MemoryNoteJob>,
}

impl ScheduledWork {
    pub(crate) fn is_empty(&self) -> bool {
        self.ai_jobs.is_empty()
            && self.pending_release.is_none()
            && self.async_banter.is_empty()
            && self.slow_think_nudge.is_none()
            && self.memory_notes.is_empty()
    }
}

async fn run_banter_job_with_timeout(
    llm: &Arc<dyn LlmClient>,
    job: PendingBanterJob,
    timeout: std::time::Duration,
) -> (
    BanterGuard,
    Option<BanterChainContext>,
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
    (job.guard, job.chain, res)
}

pub(crate) fn spawn_tts_health_probe(
    runtime: Weak<Mutex<TableSession>>,
    tts: Arc<dyn TtsClient>,
    initial_probe_at: Option<tokio::time::Instant>,
) {
    tokio::spawn(async move {
        let mut next_probe_at = initial_probe_at;
        loop {
            if let Some(target) = next_probe_at {
                let now = tokio::time::Instant::now();
                if target > now {
                    tokio::time::sleep_until(target).await;
                }
            }
            if runtime.upgrade().is_none() {
                return;
            }
            let probe_result = tts
                .health_check()
                .await
                .map(|_| ())
                .map_err(|err| err.to_string());
            let Some(strong) = runtime.upgrade() else {
                return;
            };
            let reschedule_at = {
                let mut session = strong.lock().await;
                session.commit_tts_health_probe(probe_result)
            };
            match reschedule_at {
                Some(at) => next_probe_at = Some(at),
                None => return,
            }
        }
    });
}

fn spawn_tts_synthesis_task(
    runtime: Arc<Mutex<TableSession>>,
    tts: Arc<dyn TtsClient>,
    job: PendingTtsJob,
) {
    tokio::spawn(async move {
        let mut current = job;
        loop {
            let mut aligner = PcmEvenAligner::default();
            let stream_start = match tts.synthesize_stream(current.request).await {
                Ok(mut audio_stream) => {
                    let mut first_result: Result<hyper::body::Bytes, TtsError> =
                        Err(TtsError::InvalidResponse {
                            reason: "流式语音响应未返回任何有效 PCM 分片".to_string(),
                        });
                    while let Some(item) = audio_stream.chunks.next().await {
                        match item {
                            Ok(raw_chunk) => {
                                if let Some(even_chunk) = aligner.push(raw_chunk) {
                                    first_result = Ok(even_chunk);
                                    break;
                                }
                            }
                            Err(err) => {
                                first_result = Err(err);
                                break;
                            }
                        }
                    }
                    first_result.map(|first_chunk| (audio_stream, first_chunk))
                }
                Err(err) => Err(err),
            };

            let (mut audio_stream, first_chunk) = match stream_start {
                Ok(pair) => pair,
                Err(err) => {
                    let outcome = {
                        let mut session = runtime.lock().await;
                        session.commit_tts_pre_first_chunk_error(current.guard, err)
                    };
                    if let Some(probe_at) = outcome.schedule_probe_at {
                        spawn_tts_health_probe(
                            Arc::downgrade(&runtime),
                            Arc::clone(&tts),
                            Some(probe_at),
                        );
                    }
                    match outcome.next_job {
                        Some(next) => {
                            current = next;
                            continue;
                        }
                        None => break,
                    }
                }
            };

            let begin_outcome = {
                let mut session = runtime.lock().await;
                session.begin_tts_audio_stream(
                    current.guard,
                    current.dispatched_at,
                    current.deadline_ms,
                    audio_stream.sample_rate,
                    audio_stream.channels,
                    first_chunk,
                )
            };

            let (banter_id, cache) = match begin_outcome {
                BeginTtsStreamOutcome::Streaming { banter_id, cache } => (banter_id, cache),
                BeginTtsStreamOutcome::Aborted { next_job } => {
                    drop(audio_stream);
                    match next_job {
                        Some(next) => {
                            current = next;
                            continue;
                        }
                        None => break,
                    }
                }
            };

            while let Some(item) = audio_stream.chunks.next().await {
                match item {
                    Ok(raw_chunk) => {
                        if let Some(even_chunk) = aligner.push(raw_chunk) {
                            let appended = cache
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .append_stream_chunk(
                                    &banter_id,
                                    even_chunk,
                                    tokio::time::Instant::now(),
                                );
                            if !appended {
                                eprintln!(
                                    "warning: TTS stream for {banter_id} hit cache byte budget; closing stream"
                                );
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("warning: TTS mid-stream error for {banter_id}: {err}");
                        break;
                    }
                }
            }

            cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .finish_stream(&banter_id, true, tokio::time::Instant::now());

            let next_job = {
                let mut session = runtime.lock().await;
                session.finish_tts_audio_stream()
            };
            match next_job {
                Some(next) => current = next,
                None => break,
            }
        }
    });
}

fn spawn_delayed_followup_task(
    runtime: Arc<Mutex<TableSession>>,
    llm: Arc<dyn LlmClient>,
    tts_client: Option<Arc<dyn TtsClient>>,
    followup: DelayedBanterFollowup,
) {
    tokio::spawn(async move {
        if !followup.delay.is_zero() {
            tokio::time::sleep(followup.delay).await;
        }
        let next_job = {
            let mut session = runtime.lock().await;
            session.begin_delayed_followup(followup)
        };
        if let Some(job) = next_job {
            spawn_async_banter_task(runtime, llm, tts_client, job);
        }
    });
}

fn spawn_async_banter_task(
    runtime: Arc<Mutex<TableSession>>,
    llm: Arc<dyn LlmClient>,
    tts_client: Option<Arc<dyn TtsClient>>,
    job: PendingBanterJob,
) {
    tokio::spawn(async move {
        let (guard, chain, result) =
            run_banter_job_with_timeout(&llm, job, ASYNC_BANTER_TIMEOUT).await;
        let commit = {
            let mut session = runtime.lock().await;
            session.commit_banter_result(guard, chain, result)
        };
        if let (Some(tts_job), Some(tts)) = (commit.tts_job, tts_client.clone()) {
            spawn_tts_synthesis_task(Arc::clone(&runtime), tts, tts_job);
        }
        if let Some(followup) = commit.followup {
            spawn_delayed_followup_task(runtime, llm, tts_client, followup);
        }
    });
}

fn spawn_gameplay_work(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    llm_client: Option<Arc<dyn LlmClient>>,
    tts_client: Option<Arc<dyn TtsClient>>,
    pending_release: Option<ReleaseTicket>,
    ai_jobs: Vec<PendingAiJob>,
) {
    if let Some(ticket) = pending_release {
        let runtime = Arc::clone(&runtime);
        let ai_pool = Arc::clone(&ai_pool);
        let llm_client = llm_client.clone();
        let tts_client = tts_client.clone();
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
                spawn_scheduled_work(runtime, ai_pool, llm_client, tts_client, next_work);
            }
        });
    }

    for job in ai_jobs {
        let runtime = Arc::clone(&runtime);
        let ai_pool_for_next = Arc::clone(&ai_pool);
        let ai_pool_for_job = Arc::clone(&ai_pool);
        let llm_client = llm_client.clone();
        let tts_client = tts_client.clone();
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
                spawn_scheduled_work(runtime, ai_pool_for_next, llm_client, tts_client, next_work);
            }
        });
    }
}

pub(crate) fn spawn_scheduled_work(
    runtime: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    llm_client: Option<Arc<dyn LlmClient>>,
    tts_client: Option<Arc<dyn TtsClient>>,
    work: ScheduledWork,
) {
    let ScheduledWork {
        ai_jobs,
        pending_release,
        async_banter,
        slow_think_nudge,
        memory_notes,
    } = work;

    if let (Some(ticket), Some(llm)) = (slow_think_nudge, llm_client.as_ref()) {
        let runtime = Arc::clone(&runtime);
        let llm = Arc::clone(llm);
        let tts_client = tts_client.clone();
        tokio::spawn(async move {
            let now = tokio::time::Instant::now();
            if ticket.fire_at > now {
                tokio::time::sleep_until(ticket.fire_at).await;
            }
            let job = {
                let mut session = runtime.lock().await;
                session.fire_slow_think_nudge(ticket)
            };
            if let Some(job) = job {
                spawn_async_banter_task(runtime, llm, tts_client, job);
            }
        });
    }

    if let Some(llm) = llm_client.as_ref() {
        for job in async_banter {
            spawn_async_banter_task(
                Arc::clone(&runtime),
                Arc::clone(llm),
                tts_client.clone(),
                job,
            );
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

    spawn_gameplay_work(
        runtime,
        ai_pool,
        llm_client,
        tts_client,
        pending_release,
        ai_jobs,
    );
}

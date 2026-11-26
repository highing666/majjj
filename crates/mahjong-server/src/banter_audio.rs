//! Cohesive `BanterVoice` (`台词语音`) bounded LRU audio cache, stale-guard tracking,
//! and 15-second cooldown backoff / `/health` recovery state machine.

use std::collections::HashMap;
use std::time::Duration;

use mahjong_core::Seat;
use tokio::time::Instant;

use crate::persona::PersonaId;
use crate::protocol::{SeatKind, ServerMessage, TablePhase};
use crate::session::TableSession;
use crate::tts::{TtsAudioOutput, TtsError, TtsSynthesisRequest, TTS_COOLDOWN_DURATION};

/// Maximum cached synthesized WAV clips in memory (Spec #81: 24 entries).
pub const BANTER_AUDIO_CACHE_MAX_ENTRIES: usize = 24;

/// Time-to-live for cached synthesized WAV clips (Spec #81: 60 seconds).
pub const BANTER_AUDIO_CACHE_TTL: Duration = Duration::from_secs(60);

/// Exclusive upper bound on total cached WAV bytes (Spec #81: strictly `< 5 MiB`).
pub const BANTER_AUDIO_CACHE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Consecutive synthesis failures required to trip the 15-second TTS cooldown circuit breaker.
pub const TTS_FAILURE_THRESHOLD: u32 = 2;

/// Validates that a URL `banter_id` path token is safe (alphanumeric, `-`, `_`, non-empty, `<= 64`).
#[must_use]
pub fn is_valid_banter_id(banter_id: &str) -> bool {
    !banter_id.is_empty()
        && banter_id.len() <= 64
        && banter_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

struct CachedAudioEntry {
    wav_bytes: Vec<u8>,
    inserted_at: Instant,
    last_used_tick: u64,
}

/// Bounded in-memory LRU + TTL cache storing single-synthesized `BanterVoice` WAV payloads.
///
/// Enforces all three Spec #81 bounds simultaneously:
/// - `len() <= 24` entries
/// - TTL `60 s` (virtual-clock compatible via `tokio::time::Instant`)
/// - `total_bytes() < 5 MiB` (`5_242_880` bytes)
pub struct BanterAudioCache {
    max_entries: usize,
    ttl: Duration,
    max_bytes_exclusive: usize,
    entries: HashMap<String, CachedAudioEntry>,
    total_bytes: usize,
    access_tick: u64,
}

impl std::fmt::Debug for BanterAudioCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BanterAudioCache")
            .field("entries", &self.entries.len())
            .field("total_bytes", &self.total_bytes)
            .field("max_entries", &self.max_entries)
            .field("ttl", &self.ttl)
            .field("max_bytes_exclusive", &self.max_bytes_exclusive)
            .finish()
    }
}

impl Default for BanterAudioCache {
    fn default() -> Self {
        Self::new()
    }
}

impl BanterAudioCache {
    /// Creates a cache using the Spec #81 production limits (24 entries, 60 s TTL, `< 5 MiB`).
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(
            BANTER_AUDIO_CACHE_MAX_ENTRIES,
            BANTER_AUDIO_CACHE_TTL,
            BANTER_AUDIO_CACHE_MAX_BYTES,
        )
    }

    /// Creates a cache with custom bounds (for deterministic testing).
    #[must_use]
    pub fn with_limits(max_entries: usize, ttl: Duration, max_bytes_exclusive: usize) -> Self {
        Self {
            max_entries,
            ttl,
            max_bytes_exclusive,
            entries: HashMap::new(),
            total_bytes: 0,
            access_tick: 0,
        }
    }

    /// Returns the number of currently stored (not yet evicted) entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the cache has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the total WAV byte count currently resident in memory.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Purges all entries whose age at `now` is at least `ttl`.
    pub fn evict_expired(&mut self, now: Instant) {
        let ttl = self.ttl;
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| now.saturating_duration_since(entry.inserted_at) >= ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired_keys {
            if let Some(removed) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.wav_bytes.len());
            }
        }
    }

    fn evict_lru_one(&mut self) -> bool {
        let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used_tick)
            .map(|(k, _)| k.clone())
        else {
            return false;
        };
        if let Some(removed) = self.entries.remove(&oldest_key) {
            self.total_bytes = self.total_bytes.saturating_sub(removed.wav_bytes.len());
            return true;
        }
        false
    }

    /// Inserts `wav_bytes` under `banter_id`, evicting expired and LRU entries as needed.
    ///
    /// Returns `false` if `banter_id` is invalid, `max_entries == 0`, or the payload alone
    /// cannot fit strictly below `max_bytes_exclusive`.
    pub fn insert(&mut self, banter_id: String, wav_bytes: Vec<u8>, now: Instant) -> bool {
        if !is_valid_banter_id(&banter_id)
            || self.max_entries == 0
            || wav_bytes.len() >= self.max_bytes_exclusive
        {
            return false;
        }

        self.evict_expired(now);

        if let Some(existing) = self.entries.remove(&banter_id) {
            self.total_bytes = self.total_bytes.saturating_sub(existing.wav_bytes.len());
        }

        while self.entries.len() >= self.max_entries
            || self.total_bytes.saturating_add(wav_bytes.len()) >= self.max_bytes_exclusive
        {
            if !self.evict_lru_one() {
                return false;
            }
        }

        self.access_tick = self.access_tick.wrapping_add(1);
        let tick = self.access_tick;
        self.total_bytes = self.total_bytes.saturating_add(wav_bytes.len());
        self.entries.insert(
            banter_id,
            CachedAudioEntry {
                wav_bytes,
                inserted_at: now,
                last_used_tick: tick,
            },
        );
        true
    }

    /// Retrieves cached WAV bytes if present and unexpired, refreshing LRU recency.
    pub fn get(&mut self, banter_id: &str, now: Instant) -> Option<Vec<u8>> {
        if !is_valid_banter_id(banter_id) {
            return None;
        }
        let is_expired = self
            .entries
            .get(banter_id)
            .is_some_and(|e| now.saturating_duration_since(e.inserted_at) >= self.ttl);
        if is_expired {
            if let Some(removed) = self.entries.remove(banter_id) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.wav_bytes.len());
            }
            return None;
        }

        let entry = self.entries.get_mut(banter_id)?;
        self.access_tick = self.access_tick.wrapping_add(1);
        entry.last_used_tick = self.access_tick;
        Some(entry.wav_bytes.clone())
    }
}

/// Monotonic freshness guard attached to every in-flight background TTS synthesis task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TtsGuard {
    pub banter_id: String,
    pub match_id: u64,
    pub hand_generation: u64,
    pub seat: Seat,
    pub seat_generation: u64,
    pub seat_banter_generation: u64,
    pub persona_id: PersonaId,
    pub allow_finished_phase: bool,
}

/// Scheduled asynchronous TTS synthesis task dispatched right after `TableBanter` broadcast.
#[derive(Debug, Clone)]
pub(crate) struct PendingTtsJob {
    pub guard: TtsGuard,
    pub request: TtsSynthesisRequest,
    pub dispatched_at: Instant,
    pub deadline_ms: u32,
}

/// Non-blocking status notice emitted on `Hello` when `--no-tts` explicitly disables LAN TTS,
/// ensuring clients with `hide_banter_bubble_with_voice` always auto-restore comic banter bubbles.
pub(crate) const TTS_DISABLED_MESSAGE: &str =
    "局域网语音合成已关闭，当前仅显示漫画气泡与离线角色语音包";

fn format_tts_unavailable_message(err_display: &str) -> String {
    format!("局域网台词语音服务暂不可用（{err_display}），已自动退回漫画气泡与角色语音包")
}

/// Cohesive TTS availability, monotonic `banter_id`, and 15 s cooldown backoff coordinator
/// owned by `TableSession`.
pub(crate) struct TtsSessionState {
    enabled: bool,
    available: bool,
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    probe_scheduled: bool,
    last_error: Option<String>,
    next_banter_seq: u64,
    seat_banter_generation: [u64; 4],
}

impl TtsSessionState {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            available: enabled,
            consecutive_failures: 0,
            cooldown_until: None,
            probe_scheduled: false,
            last_error: if enabled {
                None
            } else {
                Some(TTS_DISABLED_MESSAGE.to_string())
            },
            next_banter_seq: 1,
            seat_banter_generation: [0; 4],
        }
    }

    pub(crate) fn with_startup_error(err_display: &str) -> Self {
        Self {
            enabled: false,
            available: false,
            consecutive_failures: 0,
            cooldown_until: None,
            probe_scheduled: false,
            last_error: Some(format_tts_unavailable_message(err_display)),
            next_banter_seq: 1,
            seat_banter_generation: [0; 4],
        }
    }

    /// Allocates a globally monotonic unique `banter_id` (`"b-1"`, `"b-2"`, ...) and bumps
    /// the per-seat banter generation so any older in-flight TTS for the same seat is invalidated.
    pub(crate) fn allocate_banter_id(&mut self, seat: Seat) -> (String, u64) {
        let seq = self.next_banter_seq;
        self.next_banter_seq += 1;
        let seat_gen = &mut self.seat_banter_generation[seat.index()];
        *seat_gen += 1;
        (format!("b-{seq}"), *seat_gen)
    }

    pub(crate) fn seat_banter_generation(&self, seat: Seat) -> u64 {
        self.seat_banter_generation[seat.index()]
    }

    pub(crate) fn bump_all_seat_banter_generations(&mut self) {
        for gen in &mut self.seat_banter_generation {
            *gen += 1;
        }
    }

    pub(crate) fn current_status_notice(&self) -> Option<ServerMessage> {
        if self.available {
            return None;
        }
        self.last_error
            .as_ref()
            .map(|message| ServerMessage::TtsStatusNotice {
                available: false,
                message: message.clone(),
            })
    }

    /// Returns `true` when TTS is enabled, healthy, and not in 15-second cooldown backoff.
    pub(crate) fn should_dispatch(&self, now: Instant) -> bool {
        self.enabled && self.available && self.cooldown_until.is_none_or(|until| now >= until)
    }

    pub(crate) fn record_synthesis_success(&mut self) {
        if self.enabled && self.available {
            self.consecutive_failures = 0;
        }
    }

    pub(crate) fn record_synthesis_failure(
        &mut self,
        err_display: &str,
        now: Instant,
    ) -> (Option<ServerMessage>, Option<Instant>) {
        if !self.enabled {
            return (None, None);
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if !self.available {
            return (None, None);
        }
        if self.consecutive_failures < TTS_FAILURE_THRESHOLD {
            return (None, None);
        }

        self.available = false;
        self.consecutive_failures = 0;
        let next_probe = now + TTS_COOLDOWN_DURATION;
        self.cooldown_until = Some(next_probe);
        let message = format_tts_unavailable_message(err_display);
        self.last_error = Some(message.clone());

        let schedule_at = if self.probe_scheduled {
            None
        } else {
            self.probe_scheduled = true;
            Some(next_probe)
        };
        (
            Some(ServerMessage::TtsStatusNotice {
                available: false,
                message,
            }),
            schedule_at,
        )
    }

    pub(crate) fn record_health_probe_result(
        &mut self,
        result: Result<(), String>,
        now: Instant,
    ) -> (Option<ServerMessage>, Option<Instant>) {
        if !self.enabled {
            self.probe_scheduled = false;
            return (None, None);
        }
        match result {
            Ok(()) => {
                self.probe_scheduled = false;
                self.cooldown_until = None;
                self.consecutive_failures = 0;
                if !self.available {
                    self.available = true;
                    self.last_error = None;
                    return (
                        Some(ServerMessage::TtsStatusNotice {
                            available: true,
                            message: "局域网台词语音服务已恢复连接".to_string(),
                        }),
                        None,
                    );
                }
                (None, None)
            }
            Err(err_display) => {
                let next_probe = now + TTS_COOLDOWN_DURATION;
                self.cooldown_until = Some(next_probe);
                self.probe_scheduled = true;
                let message = format_tts_unavailable_message(&err_display);
                self.last_error = Some(message.clone());
                if self.available {
                    self.available = false;
                    (
                        Some(ServerMessage::TtsStatusNotice {
                            available: false,
                            message,
                        }),
                        Some(next_probe),
                    )
                } else {
                    (None, Some(next_probe))
                }
            }
        }
    }
}

impl TableSession {
    fn is_tts_guard_fresh(&self, guard: &TtsGuard) -> bool {
        let phase_ok = self.phase == TablePhase::InMatch
            || (guard.allow_finished_phase && self.phase == TablePhase::Finished);
        if !phase_ok {
            return false;
        }
        let seat_slot = &self.seats[guard.seat.index()];
        if seat_slot.info.kind != SeatKind::Ai
            || seat_slot.info.persona_id != Some(guard.persona_id)
            || self.seat_generation[guard.seat.index()] != guard.seat_generation
            || self.tts.seat_banter_generation(guard.seat) != guard.seat_banter_generation
        {
            return false;
        }
        self.active_match.as_ref().is_some_and(|active| {
            active.match_id == guard.match_id && active.hand_generation == guard.hand_generation
        })
    }

    pub(crate) fn commit_tts_result(
        &mut self,
        guard: TtsGuard,
        dispatched_at: Instant,
        deadline_ms: u32,
        result: Result<TtsAudioOutput, TtsError>,
    ) -> Option<Instant> {
        let now = Instant::now();
        match result {
            Err(err) => {
                eprintln!(
                    "warning: TTS synthesis failed for {}: {err}",
                    guard.banter_id
                );
                let (notice, probe_at) = self.tts.record_synthesis_failure(&err.to_string(), now);
                if let Some(notice) = notice {
                    self.broadcast_all(notice);
                }
                probe_at
            }
            Ok(audio) => {
                self.tts.record_synthesis_success();
                let within_deadline = now.saturating_duration_since(dispatched_at)
                    <= Duration::from_millis(u64::from(deadline_ms));
                if !within_deadline || !self.is_tts_guard_fresh(&guard) {
                    return None;
                }
                let inserted = self
                    .banter_audio_cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(guard.banter_id.clone(), audio.wav_bytes, now);
                if inserted {
                    self.broadcast_all(ServerMessage::TableBanterAudioReady {
                        audio_url: format!("/api/banter-audio/{}", guard.banter_id),
                        banter_id: guard.banter_id,
                        seat: guard.seat,
                        duration_ms: audio.duration_ms,
                    });
                }
                None
            }
        }
    }

    pub(crate) fn commit_tts_health_probe(
        &mut self,
        result: Result<(), String>,
    ) -> Option<Instant> {
        if let Err(ref err) = result {
            eprintln!("warning: LAN TTS /health probe failed: {err}");
        }
        let now = Instant::now();
        let (notice, next_probe_at) = self.tts.record_health_probe_result(result, now);
        if let Some(notice) = notice {
            self.broadcast_all(notice);
        }
        next_probe_at
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::{
        is_valid_banter_id, BanterAudioCache, TtsSessionState, BANTER_AUDIO_CACHE_MAX_BYTES,
        BANTER_AUDIO_CACHE_MAX_ENTRIES, BANTER_AUDIO_CACHE_TTL, TTS_DISABLED_MESSAGE,
    };
    use crate::protocol::ServerMessage;
    use crate::tts::TTS_COOLDOWN_DURATION;

    #[test]
    fn valid_banter_id_accepts_simple_tokens_and_rejects_traversal() {
        assert!(is_valid_banter_id("b-1"));
        assert!(is_valid_banter_id("banter_42"));
        assert!(!is_valid_banter_id(""));
        assert!(!is_valid_banter_id("../secret"));
        assert!(!is_valid_banter_id("b/1"));
        assert!(!is_valid_banter_id("b 1"));
    }

    #[tokio::test(start_paused = true)]
    async fn lru_cache_enforces_24_entry_cap_and_updates_recency_on_get() {
        let mut cache = BanterAudioCache::new();
        let t0 = Instant::now();

        for i in 1..=BANTER_AUDIO_CACHE_MAX_ENTRIES {
            assert!(cache.insert(format!("b-{i}"), vec![i as u8; 1024], t0));
        }
        assert_eq!(cache.len(), 24);

        // Touch b-1 so b-2 becomes the least-recently-used entry.
        assert!(cache.get("b-1", t0).is_some());

        // Inserting b-25 must evict b-2, keeping b-1 alive.
        assert!(cache.insert("b-25".to_string(), vec![25; 1024], t0));
        assert_eq!(cache.len(), 24);
        assert!(cache.get("b-2", t0).is_none());
        assert!(cache.get("b-1", t0).is_some());
        assert!(cache.get("b-25", t0).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn lru_cache_evicts_entries_past_60s_ttl() {
        let mut cache = BanterAudioCache::new();
        let t0 = Instant::now();
        assert!(cache.insert("b-1".to_string(), vec![1; 512], t0));

        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(cache.get("b-1", Instant::now()).is_some());

        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            Instant::now().saturating_duration_since(t0),
            BANTER_AUDIO_CACHE_TTL
        );
        assert!(cache.get("b-1", Instant::now()).is_none());
        assert_eq!(cache.total_bytes(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn lru_cache_enforces_strict_under_5_mib_byte_budget() {
        let mut cache = BanterAudioCache::new();
        let now = Instant::now();

        // Single entry >= 5 MiB is rejected without disturbing cache.
        assert!(!cache.insert(
            "b-huge".to_string(),
            vec![0; BANTER_AUDIO_CACHE_MAX_BYTES],
            now
        ));

        // Insert 2 MiB clips: 2 fit (4 MiB < 5 MiB); inserting a 3rd evicts the oldest to stay < 5 MiB.
        let two_mib = 2 * 1024 * 1024;
        assert!(cache.insert("b-1".to_string(), vec![1; two_mib], now));
        assert!(cache.insert("b-2".to_string(), vec![2; two_mib], now));
        assert_eq!(cache.len(), 2);
        assert!(cache.total_bytes() < BANTER_AUDIO_CACHE_MAX_BYTES);

        assert!(cache.insert("b-3".to_string(), vec![3; two_mib], now));
        assert_eq!(cache.len(), 2);
        assert!(cache.total_bytes() < BANTER_AUDIO_CACHE_MAX_BYTES);
        assert!(cache.get("b-1", now).is_none());
        assert!(cache.get("b-2", now).is_some());
        assert!(cache.get("b-3", now).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn tts_session_state_trips_after_consecutive_failures_backs_off_15s_and_recovers() {
        let mut state = TtsSessionState::new(true);
        let t0 = Instant::now();
        assert!(state.should_dispatch(t0));

        // 1st failure does not trip breaker yet.
        let (notice1, probe1) = state.record_synthesis_failure("timeout", t0);
        assert!(notice1.is_none());
        assert!(probe1.is_none());
        assert!(state.should_dispatch(t0));

        // 2nd consecutive failure trips breaker, emits TtsStatusNotice(false), schedules 15s probe.
        let (notice2, probe2) = state.record_synthesis_failure("connection refused", t0);
        assert!(matches!(
            notice2,
            Some(ServerMessage::TtsStatusNotice {
                available: false,
                ..
            })
        ));
        assert_eq!(probe2, Some(t0 + TTS_COOLDOWN_DURATION));
        assert!(!state.should_dispatch(t0));
        assert!(!state.should_dispatch(t0 + Duration::from_secs(14)));

        // Failed probe at +15s keeps degraded without spamming duplicate notice and reschedules +15s.
        let t15 = t0 + TTS_COOLDOWN_DURATION;
        let (probe_fail_notice, next_probe) =
            state.record_health_probe_result(Err("still down".to_string()), t15);
        assert!(probe_fail_notice.is_none());
        assert_eq!(next_probe, Some(t15 + TTS_COOLDOWN_DURATION));
        assert!(!state.should_dispatch(t15));

        // Successful probe at +30s restores availability and emits TtsStatusNotice(true).
        let t30 = t15 + TTS_COOLDOWN_DURATION;
        let (recovery_notice, next_probe_ok) = state.record_health_probe_result(Ok(()), t30);
        assert!(matches!(
            recovery_notice,
            Some(ServerMessage::TtsStatusNotice {
                available: true,
                ..
            })
        ));
        assert!(next_probe_ok.is_none());
        assert!(state.should_dispatch(t30));
    }

    #[test]
    fn disabled_tts_session_state_reports_unavailable_status_notice() {
        let state = TtsSessionState::new(false);
        assert!(matches!(
            state.current_status_notice(),
            Some(ServerMessage::TtsStatusNotice {
                available: false,
                message,
            }) if message == TTS_DISABLED_MESSAGE
        ));
    }
}

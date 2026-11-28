//! Cohesive `BanterVoice` (`台词语音`) real-time streaming fan-out buffer, bounded LRU audio cache,
//! stale-guard tracking, and 15-second cooldown backoff / `/health` recovery state machine.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{self, Stream};
use hyper::body::Bytes;
use mahjong_core::Seat;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::persona::PersonaId;
use crate::protocol::{SeatKind, ServerMessage, TablePhase};
use crate::session::TableSession;
use crate::tts::{
    TtsError, TtsSynthesisRequest, DEFAULT_TTS_CHANNELS, DEFAULT_TTS_SAMPLE_RATE,
    TTS_COOLDOWN_DURATION,
};

/// Maximum cached synthesized `BanterVoice` entries in memory (Spec #81 / #112: 24 entries).
pub const BANTER_AUDIO_CACHE_MAX_ENTRIES: usize = 24;

/// Time-to-live for cached synthesized `BanterVoice` clips (Spec #81 / #112: 60 seconds).
pub const BANTER_AUDIO_CACHE_TTL: Duration = Duration::from_secs(60);

/// Exclusive upper bound on total resident audio bytes including in-flight streams (strictly `< 5 MiB`).
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

/// Preserves 2-byte PCM16 sample alignment across arbitrary upstream HTTP chunk boundaries.
#[derive(Debug, Default)]
pub(crate) struct PcmEvenAligner {
    pending_byte: Option<u8>,
}

impl PcmEvenAligner {
    /// Consumes a raw byte chunk and yields the longest even-byte-aligned prefix, carrying
    /// any trailing single odd byte into the next chunk.
    pub(crate) fn push(&mut self, chunk: Bytes) -> Option<Bytes> {
        if chunk.is_empty() {
            return None;
        }
        match self.pending_byte.take() {
            None => {
                if chunk.len() < 2 {
                    self.pending_byte = Some(chunk[0]);
                    return None;
                }
                if (chunk.len() & 1) == 1 {
                    let split = chunk.len() - 1;
                    self.pending_byte = Some(chunk[split]);
                    Some(chunk.slice(..split))
                } else {
                    Some(chunk)
                }
            }
            Some(carry) => {
                let mut buf = Vec::with_capacity(chunk.len() + 1);
                buf.push(carry);
                buf.extend_from_slice(&chunk);
                if (buf.len() & 1) == 1 {
                    self.pending_byte = buf.pop();
                }
                if buf.is_empty() {
                    None
                } else {
                    Some(Bytes::from(buf))
                }
            }
        }
    }
}

struct ActiveStreamState {
    chunks: Vec<Bytes>,
    total_bytes: usize,
    finished: bool,
}

/// Live in-flight PCM16 audio stream supporting single-synthesis real-time fan-out to
/// multiple concurrent same-table clients (including late-joining catch-up from chunk 0).
pub struct ActiveBanterStream {
    sample_rate: u32,
    channels: u8,
    state: std::sync::Mutex<ActiveStreamState>,
    version_tx: watch::Sender<usize>,
}

impl std::fmt::Debug for ActiveBanterStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (chunks, total_bytes, finished) = {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (state.chunks.len(), state.total_bytes, state.finished)
        };
        f.debug_struct("ActiveBanterStream")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("chunks", &chunks)
            .field("total_bytes", &total_bytes)
            .field("finished", &finished)
            .finish()
    }
}

impl ActiveBanterStream {
    fn new(sample_rate: u32, channels: u8, first_chunk: Bytes) -> Self {
        let len = first_chunk.len();
        let (version_tx, _) = watch::channel(0usize);
        Self {
            sample_rate,
            channels,
            state: std::sync::Mutex::new(ActiveStreamState {
                chunks: vec![first_chunk],
                total_bytes: len,
                finished: false,
            }),
            version_tx,
        }
    }

    /// Sample rate in Hz for this PCM16 stream.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channel count for this PCM16 stream (mono = 1).
    #[must_use]
    pub const fn channels(&self) -> u8 {
        self.channels
    }

    fn push_chunk(&self, chunk: Bytes) {
        if chunk.is_empty() {
            return;
        }
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.finished {
                return;
            }
            state.total_bytes = state.total_bytes.saturating_add(chunk.len());
            state.chunks.push(chunk);
        }
        self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
    }

    fn mark_finished(&self) -> Vec<u8> {
        let combined = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.finished = true;
            let mut out = Vec::with_capacity(state.total_bytes);
            for chunk in &state.chunks {
                out.extend_from_slice(chunk);
            }
            out
        };
        self.version_tx.send_modify(|v| *v = v.wrapping_add(1));
        combined
    }

    /// Returns a contiguous snapshot of all PCM bytes buffered so far.
    #[must_use]
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = Vec::with_capacity(state.total_bytes);
        for chunk in &state.chunks {
            out.extend_from_slice(chunk);
        }
        out
    }

    /// Subscribes to this stream starting from chunk 0, yielding already-buffered chunks
    /// immediately and then awaiting live chunks until EOF.
    pub fn into_chunk_stream(
        self: Arc<Self>,
    ) -> impl Stream<Item = Result<Bytes, Infallible>> + Send + 'static {
        let rx = self.version_tx.subscribe();
        stream::unfold((self, rx, 0usize), |(stream, mut rx, mut idx)| async move {
            loop {
                rx.borrow_and_update();
                let next_chunk = {
                    let state = stream
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if idx < state.chunks.len() {
                        Some(Some(state.chunks[idx].clone()))
                    } else if state.finished {
                        Some(None)
                    } else {
                        None
                    }
                };
                match next_chunk {
                    Some(Some(chunk)) => {
                        idx += 1;
                        return Some((Ok(chunk), (stream, rx, idx)));
                    }
                    Some(None) => return None,
                    None => {
                        if rx.changed().await.is_err() {
                            let remaining = {
                                let state = stream
                                    .state
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                state.chunks.get(idx).cloned()
                            };
                            if let Some(chunk) = remaining {
                                idx += 1;
                                return Some((Ok(chunk), (stream, rx, idx)));
                            }
                            return None;
                        }
                    }
                }
            }
        })
    }
}

/// Lookup result for `GET /api/banter-audio/{banter_id}`: either an in-flight fan-out
/// stream or a completed bounded-LRU cached PCM16 clip.
#[derive(Debug, Clone)]
pub enum BanterAudioSource {
    Active {
        sample_rate: u32,
        channels: u8,
        stream: Arc<ActiveBanterStream>,
    },
    Cached {
        sample_rate: u32,
        channels: u8,
        pcm_bytes: Bytes,
    },
}

struct CachedAudioEntry {
    sample_rate: u32,
    channels: u8,
    pcm_bytes: Bytes,
    inserted_at: Instant,
    last_used_tick: u64,
}

struct ActiveStreamEntry {
    stream: Arc<ActiveBanterStream>,
}

/// Bounded in-memory real-time fan-out buffer + LRU/TTL cache storing single-synthesized
/// `BanterVoice` Raw PCM16 streams and completed clips.
///
/// Enforces all three Spec #81 / #112 bounds simultaneously across both active in-flight
/// stream buffers and completed cache entries:
/// - `len() <= 24` entries
/// - TTL `60 s` for completed entries (virtual-clock compatible via `tokio::time::Instant`)
/// - `total_bytes() < 5 MiB` (`5_242_880` bytes)
pub struct BanterAudioCache {
    max_entries: usize,
    ttl: Duration,
    max_bytes_exclusive: usize,
    entries: HashMap<String, CachedAudioEntry>,
    active_streams: HashMap<String, ActiveStreamEntry>,
    total_bytes: usize,
    access_tick: u64,
}

impl std::fmt::Debug for BanterAudioCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BanterAudioCache")
            .field("entries", &self.entries.len())
            .field("active_streams", &self.active_streams.len())
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
    /// Creates a cache using the Spec #81 / #112 production limits (24 entries, 60 s TTL, `< 5 MiB`).
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
            active_streams: HashMap::new(),
            total_bytes: 0,
            access_tick: 0,
        }
    }

    /// Returns the total number of resident entries (completed LRU entries + active in-flight streams).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() + self.active_streams.len()
    }

    /// Returns `true` when the cache has neither completed entries nor active streams.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.active_streams.is_empty()
    }

    /// Returns the total PCM byte count currently resident in memory (completed + active streams).
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Purges all completed entries whose age at `now` is at least `ttl`.
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
                self.total_bytes = self.total_bytes.saturating_sub(removed.pcm_bytes.len());
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
            self.total_bytes = self.total_bytes.saturating_sub(removed.pcm_bytes.len());
            return true;
        }
        false
    }

    /// Registers a new live fan-out stream seeded with its first non-empty even-aligned PCM chunk.
    ///
    /// Returns `false` if `banter_id` is invalid, `first_chunk` is empty or odd-length,
    /// `max_entries == 0`, or the stream cannot fit within entry / `< 5 MiB` budgets.
    pub fn begin_stream(
        &mut self,
        banter_id: String,
        sample_rate: u32,
        channels: u8,
        first_chunk: Bytes,
        now: Instant,
    ) -> bool {
        if !is_valid_banter_id(&banter_id)
            || first_chunk.is_empty()
            || (first_chunk.len() & 1) == 1
            || self.max_entries == 0
            || first_chunk.len() >= self.max_bytes_exclusive
        {
            return false;
        }

        self.evict_expired(now);

        if let Some(existing) = self.entries.remove(&banter_id) {
            self.total_bytes = self.total_bytes.saturating_sub(existing.pcm_bytes.len());
        }
        if let Some(existing_active) = self.active_streams.remove(&banter_id) {
            let drained = existing_active.stream.mark_finished();
            self.total_bytes = self.total_bytes.saturating_sub(drained.len());
        }

        while self.len() >= self.max_entries
            || self.total_bytes.saturating_add(first_chunk.len()) >= self.max_bytes_exclusive
        {
            if !self.evict_lru_one() {
                return false;
            }
        }

        let chunk_len = first_chunk.len();
        let stream = Arc::new(ActiveBanterStream::new(sample_rate, channels, first_chunk));
        self.total_bytes = self.total_bytes.saturating_add(chunk_len);
        self.active_streams
            .insert(banter_id, ActiveStreamEntry { stream });
        true
    }

    /// Appends an even-aligned PCM chunk to an active fan-out stream, evicting LRU completed
    /// entries as needed to preserve the strict `< 5 MiB` total memory budget.
    ///
    /// Returns `false` if the stream does not exist or cannot grow without breaching the byte cap.
    pub fn append_stream_chunk(&mut self, banter_id: &str, chunk: Bytes, now: Instant) -> bool {
        if chunk.is_empty() {
            return self.active_streams.contains_key(banter_id);
        }
        if (chunk.len() & 1) == 1 || !self.active_streams.contains_key(banter_id) {
            return false;
        }

        self.evict_expired(now);

        while self.total_bytes.saturating_add(chunk.len()) >= self.max_bytes_exclusive {
            if !self.evict_lru_one() {
                return false;
            }
        }

        let Some(entry) = self.active_streams.get(banter_id) else {
            return false;
        };
        let chunk_len = chunk.len();
        entry.stream.push_chunk(chunk);
        self.total_bytes = self.total_bytes.saturating_add(chunk_len);
        true
    }

    /// Marks an active stream finished (waking all subscribers with clean EOF) and optionally
    /// transitions its accumulated bytes into the bounded LRU cache.
    pub fn finish_stream(&mut self, banter_id: &str, keep_in_cache: bool, now: Instant) {
        let Some(active) = self.active_streams.remove(banter_id) else {
            return;
        };
        let sample_rate = active.stream.sample_rate();
        let channels = active.stream.channels();
        let combined = active.stream.mark_finished();
        self.total_bytes = self.total_bytes.saturating_sub(combined.len());
        if keep_in_cache && !combined.is_empty() {
            let _ = self.insert_with_format(
                banter_id.to_string(),
                sample_rate,
                channels,
                combined,
                now,
            );
        }
    }

    /// Inserts completed PCM16 bytes under `banter_id` with default 24 kHz mono metadata,
    /// evicting expired and LRU entries as needed.
    pub fn insert(&mut self, banter_id: String, pcm_bytes: Vec<u8>, now: Instant) -> bool {
        self.insert_with_format(
            banter_id,
            DEFAULT_TTS_SAMPLE_RATE,
            DEFAULT_TTS_CHANNELS,
            pcm_bytes,
            now,
        )
    }

    /// Inserts completed PCM16 bytes under `banter_id` with explicit sample rate and channels,
    /// evicting expired and LRU entries as needed.
    ///
    /// Returns `false` if `banter_id` is invalid, `max_entries == 0`, or the payload alone
    /// cannot fit strictly below `max_bytes_exclusive`.
    pub fn insert_with_format(
        &mut self,
        banter_id: String,
        sample_rate: u32,
        channels: u8,
        pcm_bytes: Vec<u8>,
        now: Instant,
    ) -> bool {
        if !is_valid_banter_id(&banter_id)
            || self.max_entries == 0
            || pcm_bytes.len() >= self.max_bytes_exclusive
        {
            return false;
        }

        self.evict_expired(now);

        if let Some(existing) = self.entries.remove(&banter_id) {
            self.total_bytes = self.total_bytes.saturating_sub(existing.pcm_bytes.len());
        }

        while self.len() >= self.max_entries
            || self.total_bytes.saturating_add(pcm_bytes.len()) >= self.max_bytes_exclusive
        {
            if !self.evict_lru_one() {
                return false;
            }
        }

        self.access_tick = self.access_tick.wrapping_add(1);
        let tick = self.access_tick;
        let byte_len = pcm_bytes.len();
        self.total_bytes = self.total_bytes.saturating_add(byte_len);
        self.entries.insert(
            banter_id,
            CachedAudioEntry {
                sample_rate,
                channels,
                pcm_bytes: Bytes::from(pcm_bytes),
                inserted_at: now,
                last_used_tick: tick,
            },
        );
        true
    }

    /// Opens an active fan-out stream or unexpired cached clip for HTTP delivery.
    pub fn open(&mut self, banter_id: &str, now: Instant) -> Option<BanterAudioSource> {
        if !is_valid_banter_id(banter_id) {
            return None;
        }
        if let Some(active) = self.active_streams.get(banter_id) {
            return Some(BanterAudioSource::Active {
                sample_rate: active.stream.sample_rate(),
                channels: active.stream.channels(),
                stream: Arc::clone(&active.stream),
            });
        }

        let is_expired = self
            .entries
            .get(banter_id)
            .is_some_and(|e| now.saturating_duration_since(e.inserted_at) >= self.ttl);
        if is_expired {
            if let Some(removed) = self.entries.remove(banter_id) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.pcm_bytes.len());
            }
            return None;
        }

        let entry = self.entries.get_mut(banter_id)?;
        self.access_tick = self.access_tick.wrapping_add(1);
        entry.last_used_tick = self.access_tick;
        Some(BanterAudioSource::Cached {
            sample_rate: entry.sample_rate,
            channels: entry.channels,
            pcm_bytes: entry.pcm_bytes.clone(),
        })
    }

    /// Retrieves cached or currently buffered PCM bytes if present and unexpired, refreshing LRU recency.
    pub fn get(&mut self, banter_id: &str, now: Instant) -> Option<Vec<u8>> {
        match self.open(banter_id, now)? {
            BanterAudioSource::Active { stream, .. } => Some(stream.snapshot_bytes()),
            BanterAudioSource::Cached { pcm_bytes, .. } => Some(pcm_bytes.to_vec()),
        }
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
/// ensuring voice-first clients always auto-restore comic banter bubbles.
pub(crate) const TTS_DISABLED_MESSAGE: &str =
    "局域网语音合成已关闭，当前仅显示漫画气泡与离线角色语音包";

fn format_tts_unavailable_message(err_display: &str) -> String {
    format!("局域网台词语音服务暂不可用（{err_display}），已自动退回漫画气泡与角色语音包")
}

/// Outcome returned when a pre-first-chunk TTS error finishes an in-flight synthesis job.
#[derive(Debug)]
pub(crate) struct TtsCommitOutcome {
    pub schedule_probe_at: Option<Instant>,
    pub next_job: Option<PendingTtsJob>,
}

/// Outcome returned when registering the first non-empty PCM chunk of an in-flight TTS stream.
#[derive(Debug)]
pub(crate) enum BeginTtsStreamOutcome {
    /// Freshness guard and deadline passed; `TableBanterAudioReady` has been broadcast and
    /// subsequent chunks should be pushed into `cache` under `banter_id`.
    Streaming {
        banter_id: String,
        cache: Arc<std::sync::Mutex<BanterAudioCache>>,
    },
    /// Stream was stale, past deadline, or exceeded cache limits; upstream stream should be dropped.
    Aborted { next_job: Option<PendingTtsJob> },
}

/// Outcome returned when enqueueing a new TTS job into the single-in-flight + 1-slot scheduler.
#[derive(Debug)]
pub(crate) struct TtsEnqueueOutcome {
    pub dispatch_now: Option<PendingTtsJob>,
    pub superseded: Option<PendingTtsJob>,
}

/// Cohesive TTS availability, monotonic `banter_id`, single-in-flight + 1-slot latest-wins
/// serialization, and 15 s cooldown backoff coordinator owned by `TableSession`.
pub(crate) struct TtsSessionState {
    enabled: bool,
    available: bool,
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    probe_scheduled: bool,
    last_error: Option<String>,
    next_banter_seq: u64,
    seat_banter_generation: [u64; 4],
    in_flight: bool,
    pending_slot: Option<PendingTtsJob>,
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
            in_flight: false,
            pending_slot: None,
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
            in_flight: false,
            pending_slot: None,
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
        self.pending_slot = None;
    }

    pub(crate) fn clear_pending_slot(&mut self) {
        self.pending_slot = None;
    }

    pub(crate) fn clear_pending_slot_for_seat(&mut self, seat: Seat) {
        if self
            .pending_slot
            .as_ref()
            .is_some_and(|job| job.guard.seat == seat)
        {
            self.pending_slot = None;
        }
    }

    pub(crate) fn enqueue_or_dispatch(
        &mut self,
        job: PendingTtsJob,
        now: Instant,
    ) -> TtsEnqueueOutcome {
        if !self.should_dispatch(now) {
            return TtsEnqueueOutcome {
                dispatch_now: None,
                superseded: None,
            };
        }
        if !self.in_flight {
            self.in_flight = true;
            return TtsEnqueueOutcome {
                dispatch_now: Some(job),
                superseded: None,
            };
        }
        let superseded = self.pending_slot.replace(job);
        TtsEnqueueOutcome {
            dispatch_now: None,
            superseded,
        }
    }

    pub(crate) fn finish_in_flight(&mut self) {
        self.in_flight = false;
    }

    pub(crate) fn mark_in_flight(&mut self) {
        self.in_flight = true;
    }

    pub(crate) fn take_pending_slot(&mut self) -> Option<PendingTtsJob> {
        self.pending_slot.take()
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
        self.pending_slot = None;
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

    fn emit_tts_fallback_if_fresh(&self, guard: &TtsGuard, reason: &str) {
        if self.is_tts_guard_fresh(guard) {
            eprintln!("warning: TTS fallback for {}: {reason}", guard.banter_id);
            self.broadcast_all(ServerMessage::TableBanterAudioFallback {
                banter_id: guard.banter_id.clone(),
                seat: guard.seat,
            });
        } else {
            eprintln!(
                "warning: TTS dropped stale banter {}: {reason}",
                guard.banter_id
            );
        }
    }

    pub(crate) fn enqueue_tts_job(
        &mut self,
        job: PendingTtsJob,
        now: Instant,
    ) -> Option<PendingTtsJob> {
        let outcome = self.tts.enqueue_or_dispatch(job, now);
        if let Some(superseded) = outcome.superseded {
            self.emit_tts_fallback_if_fresh(&superseded.guard, "superseded in single pending slot");
        }
        outcome.dispatch_now
    }

    fn promote_pending_tts_job(&mut self, now: Instant) -> Option<PendingTtsJob> {
        let mut next = self.tts.take_pending_slot()?;
        if !self.is_tts_guard_fresh(&next.guard) {
            eprintln!(
                "warning: TTS dropped stale queued banter {}",
                next.guard.banter_id
            );
            return None;
        }
        if !self.tts.should_dispatch(now) {
            return None;
        }
        let elapsed_ms = u32::try_from(
            now.saturating_duration_since(next.dispatched_at)
                .as_millis(),
        )
        .unwrap_or(u32::MAX);
        let remaining_ms = next.deadline_ms.saturating_sub(elapsed_ms);
        if remaining_ms == 0 {
            self.emit_tts_fallback_if_fresh(&next.guard, "expired while waiting in pending slot");
            return None;
        }
        next.request.deadline_ms = remaining_ms;
        self.tts.mark_in_flight();
        Some(next)
    }

    pub(crate) fn commit_tts_pre_first_chunk_error(
        &mut self,
        guard: TtsGuard,
        err: TtsError,
    ) -> TtsCommitOutcome {
        let now = Instant::now();
        self.tts.finish_in_flight();
        let schedule_probe_at = if err.is_service_outage() {
            eprintln!(
                "warning: TTS synthesis outage error for {}: {err}",
                guard.banter_id
            );
            let (notice, probe_at) = self.tts.record_synthesis_failure(&err.to_string(), now);
            if let Some(notice) = notice {
                self.broadcast_all(notice);
            } else {
                self.emit_tts_fallback_if_fresh(&guard, &err.to_string());
            }
            probe_at
        } else {
            self.emit_tts_fallback_if_fresh(&guard, &err.to_string());
            None
        };
        let next_job = self.promote_pending_tts_job(now);
        TtsCommitOutcome {
            schedule_probe_at,
            next_job,
        }
    }

    pub(crate) fn begin_tts_audio_stream(
        &mut self,
        guard: TtsGuard,
        dispatched_at: Instant,
        deadline_ms: u32,
        sample_rate: u32,
        channels: u8,
        first_chunk: Bytes,
    ) -> BeginTtsStreamOutcome {
        let now = Instant::now();
        self.tts.record_synthesis_success();

        if !self.is_tts_guard_fresh(&guard) {
            eprintln!(
                "warning: TTS dropped stale streaming banter {}",
                guard.banter_id
            );
            self.tts.finish_in_flight();
            let next_job = self.promote_pending_tts_job(now);
            return BeginTtsStreamOutcome::Aborted { next_job };
        }

        let within_deadline = now.saturating_duration_since(dispatched_at)
            <= Duration::from_millis(u64::from(deadline_ms));
        if !within_deadline {
            self.emit_tts_fallback_if_fresh(
                &guard,
                &format!("first chunk arrived past {deadline_ms}ms deadline"),
            );
            self.tts.finish_in_flight();
            let next_job = self.promote_pending_tts_job(now);
            return BeginTtsStreamOutcome::Aborted { next_job };
        }

        let registered = self
            .banter_audio_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .begin_stream(
                guard.banter_id.clone(),
                sample_rate,
                channels,
                first_chunk,
                now,
            );
        if !registered {
            self.emit_tts_fallback_if_fresh(&guard, "streaming audio exceeded cache budget");
            self.tts.finish_in_flight();
            let next_job = self.promote_pending_tts_job(now);
            return BeginTtsStreamOutcome::Aborted { next_job };
        }

        self.broadcast_all(ServerMessage::TableBanterAudioReady {
            audio_url: format!("/api/banter-audio/{}", guard.banter_id),
            banter_id: guard.banter_id.clone(),
            seat: guard.seat,
            sample_rate,
        });
        BeginTtsStreamOutcome::Streaming {
            banter_id: guard.banter_id,
            cache: Arc::clone(&self.banter_audio_cache),
        }
    }

    pub(crate) fn finish_tts_audio_stream(&mut self) -> Option<PendingTtsJob> {
        let now = Instant::now();
        self.tts.finish_in_flight();
        self.promote_pending_tts_job(now)
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

    use futures_util::StreamExt;
    use hyper::body::Bytes;
    use tokio::time::Instant;

    use super::{
        is_valid_banter_id, BanterAudioCache, BanterAudioSource, PcmEvenAligner, TtsSessionState,
        BANTER_AUDIO_CACHE_MAX_BYTES, BANTER_AUDIO_CACHE_MAX_ENTRIES, BANTER_AUDIO_CACHE_TTL,
        TTS_DISABLED_MESSAGE,
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

    #[test]
    fn pcm_even_aligner_preserves_sample_alignment_across_odd_chunks() {
        let mut aligner = PcmEvenAligner::default();
        // Single byte waits for pair.
        assert_eq!(aligner.push(Bytes::from_static(&[0x01])), None);
        // 2 more bytes -> emits [0x01, 0x02], carries 0x03.
        assert_eq!(
            aligner.push(Bytes::from_static(&[0x02, 0x03])),
            Some(Bytes::from_static(&[0x01, 0x02]))
        );
        // 3 more bytes -> emits [0x03, 0x04, 0x05, 0x06], no carry.
        assert_eq!(
            aligner.push(Bytes::from_static(&[0x04, 0x05, 0x06])),
            Some(Bytes::from_static(&[0x03, 0x04, 0x05, 0x06]))
        );
        // Even chunk passes through cleanly.
        assert_eq!(
            aligner.push(Bytes::from_static(&[0x07, 0x08])),
            Some(Bytes::from_static(&[0x07, 0x08]))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn active_stream_fans_out_to_early_and_late_readers_then_transitions_to_lru() {
        let mut cache = BanterAudioCache::new();
        let t0 = Instant::now();

        assert!(cache.begin_stream(
            "b-live".to_string(),
            24_000,
            1,
            Bytes::from_static(&[1, 2, 3, 4]),
            t0,
        ));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.total_bytes(), 4);

        // Early subscriber opens while only chunk 1 exists.
        let BanterAudioSource::Active {
            sample_rate,
            channels,
            stream: early_stream,
        } = cache.open("b-live", t0).expect("open early active stream")
        else {
            panic!("expected active stream for early reader");
        };
        assert_eq!(sample_rate, 24_000);
        assert_eq!(channels, 1);

        let mut early_rx = Box::pin(early_stream.into_chunk_stream());
        assert_eq!(
            early_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[1, 2, 3, 4])
        );

        // Push chunk 2 before late reader joins.
        assert!(cache.append_stream_chunk("b-live", Bytes::from_static(&[5, 6]), t0));
        assert_eq!(cache.total_bytes(), 6);

        // Late subscriber opens after chunk 2 and must catch up from byte 0.
        let BanterAudioSource::Active {
            stream: late_stream,
            ..
        } = cache.open("b-live", t0).expect("open late active stream")
        else {
            panic!("expected active stream for late reader");
        };
        let mut late_rx = Box::pin(late_stream.into_chunk_stream());

        assert_eq!(
            early_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[5, 6])
        );
        assert_eq!(
            late_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[1, 2, 3, 4])
        );
        assert_eq!(
            late_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[5, 6])
        );

        // Push chunk 3 and finish stream.
        assert!(cache.append_stream_chunk("b-live", Bytes::from_static(&[7, 8, 9, 10]), t0));
        cache.finish_stream("b-live", true, t0);

        assert_eq!(
            early_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[7, 8, 9, 10])
        );
        assert!(early_rx.next().await.is_none());

        assert_eq!(
            late_rx.next().await.unwrap().unwrap(),
            Bytes::from_static(&[7, 8, 9, 10])
        );
        assert!(late_rx.next().await.is_none());

        // Post-completion lookup serves cached entry with full concatenated bytes.
        let BanterAudioSource::Cached {
            sample_rate: cached_sr,
            channels: cached_ch,
            pcm_bytes,
        } = cache
            .open("b-live", t0)
            .expect("open completed cache entry")
        else {
            panic!("expected cached entry after finish_stream");
        };
        assert_eq!(cached_sr, 24_000);
        assert_eq!(cached_ch, 1);
        assert_eq!(pcm_bytes.as_ref(), &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.total_bytes(), 10);
    }

    #[tokio::test(start_paused = true)]
    async fn active_stream_bytes_count_toward_under_5_mib_budget_and_evict_lru_completed() {
        let mut cache = BanterAudioCache::new();
        let now = Instant::now();
        let two_mib = 2 * 1024 * 1024;

        // Two completed 2 MiB clips (4 MiB total).
        assert!(cache.insert("b-old".to_string(), vec![1; two_mib], now));
        assert!(cache.insert("b-mid".to_string(), vec![2; two_mib], now));
        assert_eq!(cache.total_bytes(), 4 * 1024 * 1024);

        // Start a 512 KiB active stream -> fits (4.5 MiB < 5 MiB) without evicting yet.
        let half_mib = 512 * 1024;
        assert!(cache.begin_stream(
            "b-stream".to_string(),
            24_000,
            1,
            Bytes::from(vec![3; half_mib]),
            now,
        ));
        assert_eq!(cache.total_bytes(), 4 * 1024 * 1024 + half_mib);
        assert!(cache.get("b-old", now).is_some());

        // Refreshing b-old makes b-mid the LRU completed entry.
        // Growing b-stream by 1 MiB would exceed 5 MiB, so b-mid must be evicted!
        let one_mib = 1024 * 1024;
        assert!(cache.append_stream_chunk("b-stream", Bytes::from(vec![4; one_mib]), now));
        assert!(cache.total_bytes() < BANTER_AUDIO_CACHE_MAX_BYTES);
        assert!(cache.get("b-mid", now).is_none());
        assert!(cache.get("b-old", now).is_some());

        // Appending 4 MiB more would exceed 5 MiB even after evicting b-old -> rejected cleanly!
        assert!(!cache.append_stream_chunk("b-stream", Bytes::from(vec![5; 4 * 1024 * 1024]), now));
        assert!(cache.total_bytes() < BANTER_AUDIO_CACHE_MAX_BYTES);
    }
}

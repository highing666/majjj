//! Single active `TableSession` state and full match WebSocket coordination.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mahjong_ai::{AiStyle, DecisionBudget, DecisionContext, HandFacts};
use mahjong_core::{
    AnyRuleset, HandOutcome, HandSettlement, Match, MatchMode, PlayerAction, PlayerEvent,
    PlayerEventKind, PlayerView, PublicPhase, RecordedEvent, Seat, SeededWallProvider,
    TuidaohuRuleset, WallProvider,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use tokio::sync::{mpsc, watch, Mutex};
use uuid::Uuid;

use crate::ai_pool::{
    build_ai_pool, default_ai_threads, spawn_scheduled_work, spawn_tts_health_probe, AiJobKey,
    PendingAiJob, ScheduledWork,
};
use crate::banter::{
    initialize_seat_hand_facts, is_seat_off_cooldown, plan_banter_for_events, plan_chain_followup,
    plan_initial_hand_banter, plan_quick_emote_replies, plan_slow_think_nudge, BanterChainContext,
    BanterGuard, BanterPlanContext, BanterSessionState, ChainPrevLine, NudgeTurnKey,
    PendingBanterJob, SlowThinkTicket, BANTER_CHAIN_REPLY_DELAY, HIGHLIGHT_SYNC_CAP,
    SEAT_BANTER_COOLDOWN,
};
use crate::banter_audio::{BanterAudioCache, PendingTtsJob, TtsGuard, TtsSessionState};
use crate::history_dto::{
    ClearHistoryResponse, HandRecord, HandSeatRecord, HistoryExportResponse, MatchRecord,
    MatchSeatRecord, PreferencesData, StatsResponse, HISTORY_SCHEMA_VERSION,
};
use crate::lan::{discover_lan_urls, read_local_hostname, read_system_interfaces};
use crate::llm::{BanterOutput, LlmClient, LlmError};
use crate::pacing::{
    apply_discard_to_view, compute_ai_turn_trigger, discard_release_delay, target_ai_delay,
    AiTurnTrigger, PacingMode, ReleaseTicket,
};
use crate::persona::{is_valid_human_voice_pack_id, PersonaId};
use crate::persona_memory::{
    plan_match_finished_note_jobs, plan_memorable_hand_note_jobs, ClearPersonaMemoryResponse,
    PersonaMemoryNoteTrigger, PersonaMemoryResponse, PersonaMemoryStore,
};
use crate::protocol::{
    ClientMessage, DestructiveOpKind, LanUrlInfo, LobbyConfig, PendingDestructiveOp, QuickEmote,
    SeatInfo, SeatKind, SeatingRule, ServerMessage, TablePhase, TableSnapshot,
};
use crate::seat_control::HandAttribution;
use crate::security::ClientTrustTier;
use crate::server::ServerError;
use crate::storage::{current_unix_ms, normalize_nickname, HistoryStore, PublicUrlSaveError};
use crate::tts::{
    TtsClient, TtsSynthesisRequest, TTS_IN_HAND_DEADLINE_MS, TTS_SETTLEMENT_DEADLINE_MS,
};

/// `Send`-safe wrapper around a boxed [`WallProvider`] for injecting deterministic walls in tests.
pub struct BoxedWallProvider(pub Box<dyn WallProvider + Send>);

impl WallProvider for BoxedWallProvider {
    fn initial_dealer(&mut self) -> Seat {
        self.0.initial_dealer()
    }

    fn next_wall(&mut self, deck: &[mahjong_core::Tile]) -> Vec<mahjong_core::Tile> {
        self.0.next_wall(deck)
    }
}

/// Factory returning a fresh [`BoxedWallProvider`] whenever a new match starts.
pub type WallProviderFactory = Arc<dyn Fn() -> BoxedWallProvider + Send + Sync>;

/// Injectable configuration hooks for deterministic testing of walls, seating, pacing, AI RNG, LLM, and TTS.
#[derive(Clone)]
pub struct TableOptions {
    wall_factory: WallProviderFactory,
    rng_seed: Option<u64>,
    ai_budget: Option<DecisionBudget>,
    pacing: PacingMode,
    data_dir: Option<PathBuf>,
    llm_client: Option<Arc<dyn LlmClient>>,
    tts_client: Option<Arc<dyn TtsClient>>,
    tts_startup_error: Option<String>,
    banter_audio_cache: Option<Arc<std::sync::Mutex<BanterAudioCache>>>,
    banter_sync_cap: Duration,
}

impl std::fmt::Debug for TableOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableOptions")
            .field("rng_seed", &self.rng_seed)
            .field("ai_budget", &self.ai_budget)
            .field("pacing", &self.pacing)
            .field("data_dir", &self.data_dir)
            .field("has_llm_client", &self.llm_client.is_some())
            .field("has_tts_client", &self.tts_client.is_some())
            .field("banter_sync_cap", &self.banter_sync_cap)
            .finish_non_exhaustive()
    }
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            wall_factory: Arc::new(|| {
                BoxedWallProvider(Box::new(SeededWallProvider::new(random_u64())))
            }),
            rng_seed: None,
            ai_budget: None,
            pacing: PacingMode::Humanized,
            data_dir: None,
            llm_client: None,
            tts_client: None,
            tts_startup_error: None,
            banter_audio_cache: None,
            banter_sync_cap: HIGHLIGHT_SYNC_CAP,
        }
    }
}

impl TableOptions {
    /// Creates deterministic options where walls, seating shuffle, and AI RNG all derive from `seed`.
    #[must_use]
    pub fn seeded(seed: u64) -> Self {
        let match_counter = Arc::new(AtomicU64::new(0));
        let wall_factory: WallProviderFactory = Arc::new(move || {
            let idx = match_counter.fetch_add(1, Ordering::Relaxed);
            BoxedWallProvider(Box::new(SeededWallProvider::new(seed.wrapping_add(idx))))
        });
        Self {
            wall_factory,
            rng_seed: Some(seed),
            ai_budget: None,
            pacing: PacingMode::Humanized,
            data_dir: None,
            llm_client: None,
            tts_client: None,
            tts_startup_error: None,
            banter_audio_cache: None,
            banter_sync_cap: HIGHLIGHT_SYNC_CAP,
        }
    }

    /// Creates deterministic options with a custom [`WallProviderFactory`] and explicit RNG `seed`.
    #[must_use]
    pub fn with_wall_factory(seed: u64, wall_factory: WallProviderFactory) -> Self {
        Self {
            wall_factory,
            rng_seed: Some(seed),
            ai_budget: None,
            pacing: PacingMode::Humanized,
            data_dir: None,
            llm_client: None,
            tts_client: None,
            tts_startup_error: None,
            banter_audio_cache: None,
            banter_sync_cap: HIGHLIGHT_SYNC_CAP,
        }
    }

    /// Overrides the AI [`DecisionBudget`] used for `mahjong_ai::decide` (defaults to `Realtime`).
    #[must_use]
    pub fn with_ai_budget(mut self, budget: DecisionBudget) -> Self {
        self.ai_budget = Some(budget);
        self
    }

    /// Overrides the server pacing mode (defaults to [`PacingMode::Humanized`]).
    #[must_use]
    pub fn with_pacing(mut self, pacing: PacingMode) -> Self {
        self.pacing = pacing;
        self
    }

    /// Configures a persistent directory for `hands.jsonl`, `matches.jsonl`, and `preferences.json`.
    #[must_use]
    pub fn with_data_dir(mut self, data_dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(data_dir.into());
        self
    }

    /// Configures the [`LlmClient`] driving persona banter generation for this table runtime.
    #[must_use]
    pub fn with_llm_client(mut self, llm_client: Arc<dyn LlmClient>) -> Self {
        self.llm_client = Some(llm_client);
        self
    }

    /// Configures the optional [`TtsClient`] driving `BanterVoice` WAV synthesis for this table runtime.
    #[must_use]
    pub fn with_tts_client(mut self, tts_client: Arc<dyn TtsClient>) -> Self {
        self.tts_client = Some(tts_client);
        self
    }

    /// Injects a custom [`BanterAudioCache`] instance (primarily for bounded-cache tests).
    #[must_use]
    pub fn with_banter_audio_cache(
        mut self,
        cache: Arc<std::sync::Mutex<BanterAudioCache>>,
    ) -> Self {
        self.banter_audio_cache = Some(cache);
        self
    }

    pub(crate) fn with_tts_startup_error(mut self, message: String) -> Self {
        self.tts_startup_error = Some(message);
        self
    }

    /// Overrides the bounded short-sync timeout for major highlight banter (defaults to `1500ms`).
    #[must_use]
    pub fn with_banter_sync_cap(mut self, cap: Duration) -> Self {
        self.banter_sync_cap = cap;
        self
    }

    /// Returns the configured [`PacingMode`].
    #[must_use]
    pub fn pacing(&self) -> PacingMode {
        self.pacing
    }

    /// Returns the configured persistent data directory, if any.
    #[must_use]
    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }

    /// Returns the configured [`LlmClient`], if any.
    #[must_use]
    pub fn llm_client(&self) -> Option<&Arc<dyn LlmClient>> {
        self.llm_client.as_ref()
    }

    /// Returns the configured [`TtsClient`], if any.
    #[must_use]
    pub fn tts_client(&self) -> Option<&Arc<dyn TtsClient>> {
        self.tts_client.as_ref()
    }
}

fn random_u64() -> u64 {
    let bits = Uuid::new_v4().as_u128();
    ((bits >> 64) ^ bits) as u64
}

fn derive_ai_step_seed(base: u64, match_id: u64, seq: u64, seat: Seat) -> u64 {
    let mut x = base
        ^ match_id.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ seq.wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (seat.index() as u64)
            .wrapping_add(1)
            .wrapping_mul(0x94D0_49BB_1331_11EB);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn shuffle_slice<T>(slice: &mut [T], rng: &mut ChaCha8Rng) {
    for i in (1..slice.len()).rev() {
        let j = (rng.next_u32() as usize) % (i + 1);
        slice.swap(i, j);
    }
}

fn assign_ai_personas(
    ai_target_seats: &[Seat],
    previous_seat_personas: &[Option<PersonaId>; 4],
    keep_lobby_seats: bool,
    rng: &mut ChaCha8Rng,
) -> Vec<(Seat, PersonaId)> {
    let mut assigned: Vec<(Seat, Option<PersonaId>)> = ai_target_seats
        .iter()
        .copied()
        .map(|seat| (seat, None))
        .collect();
    let mut used = HashSet::new();

    // 1. Preserve exact-seat manual persona picks first.
    for (seat, slot_persona) in &mut assigned {
        if let Some(pid) = previous_seat_personas[seat.index()] {
            if used.insert(pid) {
                *slot_persona = Some(pid);
            }
        }
    }

    // 2. When seating rules remap seats, carry over remaining manual lobby picks.
    if !keep_lobby_seats {
        let remaining_manual: Vec<PersonaId> = previous_seat_personas
            .iter()
            .flatten()
            .copied()
            .filter(|pid| used.insert(*pid))
            .collect();
        let mut manual_iter = remaining_manual.into_iter();
        for (_, slot_persona) in &mut assigned {
            if slot_persona.is_none() {
                if let Some(pid) = manual_iter.next() {
                    *slot_persona = Some(pid);
                }
            }
        }
    }

    // 3. Randomly backfill any remaining unspecified AI seats without duplicates.
    let mut available: Vec<PersonaId> = PersonaId::ALL
        .into_iter()
        .filter(|pid| !used.contains(pid))
        .collect();
    shuffle_slice(&mut available, rng);
    let mut avail_iter = available.into_iter();

    assigned
        .into_iter()
        .map(|(seat, maybe_pid)| {
            let pid = maybe_pid.unwrap_or_else(|| {
                avail_iter
                    .next()
                    .expect("5 built-in personas always suffice for up to 3 AI seats")
            });
            (seat, pid)
        })
        .collect()
}

/// Runtime wrapper keeping type compatibility with `Match` for Slice 2 match driving.
pub(crate) type ActiveMatch = Match<AnyRuleset, BoxedWallProvider>;

/// Signals sent from `TableSession` to a connected WebSocket writer loop.
#[derive(Debug, Clone)]
pub enum OutboundSignal {
    /// Normal outbound protocol frame.
    Message(Box<ServerMessage>),
    /// Notify the older connection that a newer tab replaced it, then close the socket.
    ReplaceAndClose,
}

/// Result of handling an inbound WebSocket message.
#[derive(Debug, Clone, Copy, Default)]
pub struct HandleOutcome {
    /// Whether the WebSocket loop should close the connection after processing this message.
    pub close_socket: bool,
    /// Whether the inbound text frame parsed as a valid [`ClientMessage`] (refreshes liveness).
    pub valid_inbound_message: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SeatRecord {
    pub(crate) info: SeatInfo,
    pub(crate) client_id: Option<String>,
    pub(crate) seat_token: Option<String>,
}

impl SeatRecord {
    fn empty(seat: Seat) -> Self {
        Self {
            info: SeatInfo {
                seat,
                kind: SeatKind::Empty,
                nickname: String::new(),
                connected: false,
                ai_assisted: false,
                ai_difficulty: None,
                ai_style: None,
                persona_id: None,
                mood: None,
                voice_pack_id: None,
            },
            client_id: None,
            seat_token: None,
        }
    }

    fn clear_to_empty(&mut self) {
        let seat = self.info.seat;
        *self = Self::empty(seat);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ClientConnection {
    pub(crate) client_id: String,
    pub(crate) seat_token: String,
    pub(crate) nickname: String,
    pub(crate) seat: Option<Seat>,
    pub(crate) trust_tier: ClientTrustTier,
    pub(crate) tx: mpsc::UnboundedSender<OutboundSignal>,
}

pub(crate) struct PendingRelease {
    pub(crate) ticket: ReleaseTicket,
    public_views: [PlayerView; 4],
    deferred_events: Vec<RecordedEvent>,
    scheduled: bool,
}

struct ActiveDiscardWindow {
    discard_seq: u64,
    emitted_at: tokio::time::Instant,
    had_human_reactor: bool,
}

pub(crate) struct ActiveMatchState {
    pub(crate) match_id: u64,
    pub(crate) match_uuid: String,
    pub(crate) game: ActiveMatch,
    accepted_action_ids: HashSet<(Seat, String)>,
    pub(crate) pending_release: Option<PendingRelease>,
    active_discard_window: Option<ActiveDiscardWindow>,
    ai_turn_trigger: AiTurnTrigger,
    pub(crate) hand_attribution: HandAttribution,
    pub(crate) hands_settled: u32,
    pub(crate) hand_generation: u64,
    pub(crate) any_ai_assisted: [bool; 4],
    pub(crate) seat_hand_facts: [Option<HandFacts>; 4],
    pub(crate) seat_last_banter_at: [Option<tokio::time::Instant>; 4],
}

impl ActiveMatchState {
    fn public_seq(&self) -> u64 {
        self.pending_release
            .as_ref()
            .map_or_else(|| self.game.seq(), |pending| pending.ticket.public_seq)
    }

    fn public_view(&self, seat: Seat) -> PlayerView {
        match &self.pending_release {
            Some(pending) => pending.public_views[seat.index()].clone(),
            None => self.game.project_view(seat),
        }
    }
}

/// Active single-table session state.
pub struct TableSession {
    table_id: String,
    pub(crate) phase: TablePhase,
    pub(crate) config: LobbyConfig,
    pub(crate) seats: [SeatRecord; 4],
    pub(crate) seat_generation: [u64; 4],
    seq: u64,
    pub(crate) pending_destructive_op: Option<PendingDestructiveOp>,
    pub(crate) active_match: Option<ActiveMatchState>,
    next_match_id: u64,
    next_release_generation: u64,
    pub(crate) in_flight_ai: HashSet<AiJobKey>,
    options: TableOptions,
    rng: ChaCha8Rng,
    ai_base_seed: u64,
    pub(crate) issued_tokens: HashMap<String, String>,
    pub(crate) connections: HashMap<u64, ClientConnection>,
    pub(crate) token_to_conn: HashMap<String, u64>,
    next_conn_id: u64,
    port: u16,
    local_hostname: Option<String>,
    pub(crate) history: HistoryStore,
    pub(crate) persona_memory: PersonaMemoryStore,
    phase_tx: watch::Sender<TablePhase>,
    banter: BanterSessionState,
    pub(crate) tts: TtsSessionState,
    pub(crate) banter_audio_cache: Arc<std::sync::Mutex<BanterAudioCache>>,
}

impl TableSession {
    /// Creates a new `TableSession` initialized in `TablePhase::Lobby`.
    #[must_use]
    pub fn new(port: u16, local_hostname: Option<String>) -> Self {
        Self::with_options(port, local_hostname, TableOptions::default())
    }

    /// Creates a new `TableSession` with custom [`TableOptions`].
    ///
    /// # Panics
    ///
    /// Panics if [`TableOptions::data_dir`] cannot be opened.
    #[must_use]
    pub fn with_options(port: u16, local_hostname: Option<String>, options: TableOptions) -> Self {
        Self::try_with_options(port, local_hostname, options)
            .expect("table session options must be valid")
    }

    /// Creates a new `TableSession` with custom [`TableOptions`], loading persisted history & preferences.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::DataDir`] if [`TableOptions::data_dir`] cannot be initialized.
    pub fn try_with_options(
        port: u16,
        local_hostname: Option<String>,
        options: TableOptions,
    ) -> Result<Self, ServerError> {
        let history = match options.data_dir() {
            Some(dir) => HistoryStore::open(dir).map_err(|source| ServerError::DataDir {
                path: dir.to_path_buf(),
                source,
            })?,
            None => HistoryStore::in_memory(),
        };
        let persona_memory = match options.data_dir() {
            Some(dir) => PersonaMemoryStore::open(dir).map_err(|source| ServerError::DataDir {
                path: dir.to_path_buf(),
                source,
            })?,
            None => PersonaMemoryStore::in_memory(),
        };
        let config = LobbyConfig::from(history.preferences());
        let slow_think_nudge_enabled = history.preferences().slow_think_nudge_enabled;
        let base_seed = options.rng_seed.unwrap_or_else(random_u64);
        let (phase_tx, _) = watch::channel(TablePhase::Lobby);
        let tts = match &options.tts_startup_error {
            Some(err) => TtsSessionState::with_startup_error(err),
            None => TtsSessionState::new(options.tts_client.is_some()),
        };
        let banter_audio_cache = options
            .banter_audio_cache
            .clone()
            .unwrap_or_else(|| Arc::new(std::sync::Mutex::new(BanterAudioCache::new())));
        Ok(Self {
            table_id: Uuid::new_v4().to_string(),
            phase: TablePhase::Lobby,
            config,
            seats: Seat::ALL.map(SeatRecord::empty),
            seat_generation: [0; 4],
            seq: 0,
            pending_destructive_op: None,
            active_match: None,
            next_match_id: 1,
            next_release_generation: 1,
            in_flight_ai: HashSet::new(),
            options,
            rng: ChaCha8Rng::seed_from_u64(base_seed),
            ai_base_seed: base_seed,
            issued_tokens: HashMap::new(),
            connections: HashMap::new(),
            token_to_conn: HashMap::new(),
            next_conn_id: 1,
            port,
            local_hostname,
            history,
            persona_memory,
            phase_tx,
            banter: BanterSessionState::new(slow_think_nudge_enabled),
            tts,
            banter_audio_cache,
        })
    }

    fn set_phase(&mut self, phase: TablePhase) {
        self.phase = phase;
        self.phase_tx.send_if_modified(|current| {
            if *current != phase {
                *current = phase;
                true
            } else {
                false
            }
        });
    }

    fn bump_release_generation(&mut self) -> u64 {
        let gen = self.next_release_generation;
        self.next_release_generation += 1;
        gen
    }

    /// Returns the current lifecycle phase.
    #[must_use]
    pub fn phase(&self) -> TablePhase {
        self.phase
    }

    /// Returns the count of currently connected human seats.
    #[must_use]
    pub fn online_humans(&self) -> u8 {
        self.seats
            .iter()
            .filter(|slot| slot.info.kind == SeatKind::Human && slot.info.connected)
            .count() as u8
    }

    pub(crate) fn allocate_conn_id(&mut self) -> u64 {
        let id = self.next_conn_id;
        self.next_conn_id += 1;
        id
    }

    #[cfg(test)]
    pub(crate) fn handle_hello_for_test(
        &mut self,
        conn_id: u64,
        client_id: &str,
        nickname: &str,
        tx: &mpsc::UnboundedSender<OutboundSignal>,
    ) {
        self.handle_hello(
            conn_id,
            ClientTrustTier::Loopback,
            tx,
            HelloParams {
                client_id: client_id.to_string(),
                nickname: nickname.to_string(),
                table_id: None,
                seat_token: None,
            },
        );
    }

    fn seat_infos(&self) -> [SeatInfo; 4] {
        [
            self.seats[0].info.clone(),
            self.seats[1].info.clone(),
            self.seats[2].info.clone(),
            self.seats[3].info.clone(),
        ]
    }

    fn lan_urls(&self) -> LanUrlInfo {
        let interfaces = read_system_interfaces();
        discover_lan_urls(&interfaces, self.local_hostname.as_deref(), self.port)
    }

    fn build_banter_plan_context<'a>(
        active: &'a ActiveMatchState,
        seats: &'a [SeatInfo; 4],
        seat_generation: &'a [u64; 4],
        sync_cap: Duration,
        now: tokio::time::Instant,
        persona_memory: &'a PersonaMemoryStore,
    ) -> BanterPlanContext<'a> {
        BanterPlanContext {
            match_id: active.match_id,
            hand_generation: active.hand_generation,
            game: &active.game,
            seats,
            seat_generation,
            sync_cap,
            cooldown: SEAT_BANTER_COOLDOWN,
            now,
            seat_last_banter_at: &active.seat_last_banter_at,
            persona_memory,
        }
    }

    pub(crate) fn build_snapshot(&self, conn: &ClientConnection) -> TableSnapshot {
        let player_view = match (self.phase, conn.seat, &self.active_match) {
            (TablePhase::InMatch | TablePhase::Finished, Some(seat), Some(active)) => {
                Some(active.public_view(seat))
            }
            _ => None,
        };
        let host_is_loopback = conn.trust_tier.is_loopback();
        let client_is_remote = conn.trust_tier.is_remote();
        let lan_urls = if client_is_remote {
            LanUrlInfo::default()
        } else {
            self.lan_urls()
        };
        TableSnapshot {
            table_id: self.table_id.clone(),
            phase: self.phase,
            config: self.config,
            pace: self.config.pace,
            slow_think_nudge_enabled: self.banter.slow_think_nudge_enabled,
            seats: self.seat_infos(),
            your_seat: conn.seat,
            seat_token: Some(conn.seat_token.clone()),
            player_view,
            pending_destructive_op: self.pending_destructive_op.clone(),
            seq: self.seq,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            lan_urls,
            host_is_loopback,
            client_is_remote,
        }
    }

    fn broadcast_all_snapshots(&self) {
        self.broadcast_snapshots_excluding(None);
    }

    pub(crate) fn broadcast_snapshots_excluding(&self, exclude_conn_id: Option<u64>) {
        for (&id, conn) in &self.connections {
            if exclude_conn_id == Some(id) {
                continue;
            }
            let snapshot = self.build_snapshot(conn);
            Self::send_to(&conn.tx, ServerMessage::TableSnapshot(snapshot));
        }
    }

    pub(crate) fn broadcast_lobby_updated(&self, exclude_conn_id: Option<u64>) {
        let msg = ServerMessage::LobbyUpdated {
            phase: self.phase,
            config: self.config,
            seats: self.seat_infos(),
            slow_think_nudge_enabled: self.banter.slow_think_nudge_enabled,
        };
        for (&id, conn) in &self.connections {
            if exclude_conn_id == Some(id) {
                continue;
            }
            Self::send_to(&conn.tx, msg.clone());
        }
    }

    pub(crate) fn require_seated_human(
        &self,
        conn_id: u64,
        action_desc: &str,
    ) -> Option<(Seat, mpsc::UnboundedSender<OutboundSignal>)> {
        let conn = self.connections.get(&conn_id)?;
        let Some(seat) = conn.seat else {
            Self::send_error(&conn.tx, format!("must join a seat before {action_desc}"));
            return None;
        };
        if self.seats[seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn.tx, format!("only a seated human can {action_desc}"));
            return None;
        }
        Some((seat, conn.tx.clone()))
    }

    pub(crate) fn send_to(tx: &mpsc::UnboundedSender<OutboundSignal>, msg: ServerMessage) {
        let _ = tx.send(OutboundSignal::Message(Box::new(msg)));
    }

    pub(crate) fn send_error(
        tx: &mpsc::UnboundedSender<OutboundSignal>,
        message: impl Into<String>,
    ) {
        Self::send_to(
            tx,
            ServerMessage::ErrorNotice {
                message: message.into(),
            },
        );
    }

    fn handle_hello(
        &mut self,
        conn_id: u64,
        trust_tier: ClientTrustTier,
        tx: &mpsc::UnboundedSender<OutboundSignal>,
        params: HelloParams,
    ) {
        let client_id = params.client_id.trim().to_string();
        if client_id.is_empty() {
            Self::send_error(tx, "client_id must not be empty");
            return;
        }
        let nickname = params.nickname.trim().to_string();
        if !nickname.is_empty() {
            self.history.record_nickname(&nickname);
        }

        // Check if this connection is reusing an existing credential for this table.
        let valid_restore_token = match (params.table_id.as_deref(), params.seat_token.as_deref()) {
            (Some(tid), Some(tok)) if tid == self.table_id => self
                .issued_tokens
                .get(tok)
                .is_some_and(|owner_client| owner_client == &client_id)
                .then(|| tok.to_string()),
            _ => None,
        };

        let seat_token = valid_restore_token.unwrap_or_else(|| {
            let new_token = Uuid::new_v4().to_string();
            self.issued_tokens
                .insert(new_token.clone(), client_id.clone());
            new_token
        });

        // If another active connection currently holds this token, replace it (Last-Connection-Wins).
        if let Some(old_conn_id) = self.token_to_conn.get(&seat_token).copied() {
            if old_conn_id != conn_id {
                if let Some(old_conn) = self.connections.remove(&old_conn_id) {
                    let _ = old_conn.tx.send(OutboundSignal::ReplaceAndClose);
                }
            }
        }

        // Restore seat ownership if this credential already occupies a human seat.
        let mut restored_seat = None;
        let mut lobby_changed = false;
        for slot in &mut self.seats {
            if slot.info.kind == SeatKind::Human
                && slot.seat_token.as_deref() == Some(&seat_token)
                && slot.client_id.as_deref() == Some(&client_id)
            {
                restored_seat = Some(slot.info.seat);
                if !slot.info.connected {
                    slot.info.connected = true;
                    lobby_changed = true;
                }
                if self.phase == TablePhase::Lobby
                    && !nickname.is_empty()
                    && slot.info.nickname != nickname
                {
                    slot.info.nickname.clone_from(&nickname);
                    lobby_changed = true;
                }
                break;
            }
        }

        let mut pending_op_updated = false;
        if let (Some(seat), Some(op)) = (restored_seat, self.pending_destructive_op.as_mut()) {
            if !op.required_seats.contains(&seat) {
                op.required_seats.push(seat);
                pending_op_updated = true;
            }
        }

        let conn = ClientConnection {
            client_id,
            seat_token: seat_token.clone(),
            nickname,
            seat: restored_seat,
            trust_tier,
            tx: tx.clone(),
        };
        let snapshot = self.build_snapshot(&conn);
        self.connections.insert(conn_id, conn);
        self.token_to_conn.insert(seat_token, conn_id);

        Self::send_to(tx, ServerMessage::TableSnapshot(snapshot));
        if let Some(notice) = self.banter.current_outage_notice() {
            Self::send_to(tx, notice);
        }
        if let Some(notice) = self.tts.current_status_notice() {
            Self::send_to(tx, notice);
        }
        if lobby_changed {
            self.broadcast_lobby_updated(Some(conn_id));
        }
        if pending_op_updated {
            self.broadcast_snapshots_excluding(Some(conn_id));
        }
    }

    pub(crate) fn handle_join_seat(&mut self, conn_id: u64, requested_seat: Option<Seat>) {
        let Some(existing_conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        if self.phase != TablePhase::Lobby {
            Self::send_error(&existing_conn.tx, "can only join a seat in Lobby phase");
            return;
        }

        let target_seat = match requested_seat {
            Some(target) => {
                if existing_conn.seat == Some(target) {
                    let snapshot = self.build_snapshot(&existing_conn);
                    Self::send_to(&existing_conn.tx, ServerMessage::TableSnapshot(snapshot));
                    return;
                }
                if self.seats[target.index()].info.kind != SeatKind::Empty {
                    Self::send_error(
                        &existing_conn.tx,
                        format!("seat {:?} is already occupied", target),
                    );
                    return;
                }
                target
            }
            None => {
                if existing_conn.seat.is_some() {
                    let snapshot = self.build_snapshot(&existing_conn);
                    Self::send_to(&existing_conn.tx, ServerMessage::TableSnapshot(snapshot));
                    return;
                }
                let Some(empty) = self
                    .seats
                    .iter()
                    .find(|slot| slot.info.kind == SeatKind::Empty)
                    .map(|slot| slot.info.seat)
                else {
                    Self::send_error(&existing_conn.tx, "no empty seats available");
                    return;
                };
                empty
            }
        };

        let preserved_voice_pack = existing_conn.seat.and_then(|old_seat| {
            self.seats[old_seat.index()]
                .info
                .voice_pack_id
                .clone()
                .filter(|id| is_valid_human_voice_pack_id(id))
        });
        if let Some(old_seat) = existing_conn.seat {
            self.seats[old_seat.index()].clear_to_empty();
        }
        let voice_pack_id =
            preserved_voice_pack.unwrap_or_else(|| self.default_human_voice_pack(None));

        let effective_nickname = if existing_conn.nickname.is_empty() {
            format!("玩家{}", target_seat.index() + 1)
        } else {
            self.history.record_nickname(&existing_conn.nickname);
            existing_conn.nickname.clone()
        };

        let slot = &mut self.seats[target_seat.index()];
        slot.info.kind = SeatKind::Human;
        slot.info.nickname = effective_nickname;
        slot.info.connected = true;
        slot.info.ai_assisted = false;
        slot.info.ai_difficulty = None;
        slot.info.ai_style = None;
        slot.info.persona_id = None;
        slot.info.mood = None;
        slot.info.voice_pack_id = Some(voice_pack_id);
        slot.client_id = Some(existing_conn.client_id.clone());
        slot.seat_token = Some(existing_conn.seat_token.clone());

        if let Some(conn_mut) = self.connections.get_mut(&conn_id) {
            conn_mut.seat = Some(target_seat);
        }
        let updated_conn = self
            .connections
            .get(&conn_id)
            .expect("connection must exist")
            .clone();
        let snapshot = self.build_snapshot(&updated_conn);

        self.broadcast_lobby_updated(None);
        Self::send_to(&updated_conn.tx, ServerMessage::TableSnapshot(snapshot));
    }

    fn handle_leave_seat(&mut self, conn_id: u64) {
        let Some(existing_conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        if self.phase != TablePhase::Lobby {
            Self::send_error(&existing_conn.tx, "can only leave seat in Lobby phase");
            return;
        }
        let Some(seat) = existing_conn.seat else {
            Self::send_error(&existing_conn.tx, "not currently seated");
            return;
        };

        self.seats[seat.index()].clear_to_empty();
        if let Some(conn_mut) = self.connections.get_mut(&conn_id) {
            conn_mut.seat = None;
        }
        let updated_conn = self
            .connections
            .get(&conn_id)
            .expect("connection must exist")
            .clone();
        let snapshot = self.build_snapshot(&updated_conn);

        self.broadcast_lobby_updated(None);
        Self::send_to(&updated_conn.tx, ServerMessage::TableSnapshot(snapshot));
    }

    pub(crate) fn handle_update_lobby_config(&mut self, conn_id: u64, next_config: LobbyConfig) {
        let Some((_, tx)) = self.require_seated_human(conn_id, "updating lobby config") else {
            return;
        };
        if self.phase == TablePhase::InMatch {
            Self::send_error(
                &tx,
                "cannot update lobby config while a match is in progress",
            );
            return;
        }
        self.config = next_config;
        self.history.record_lobby_config(next_config);
        self.broadcast_lobby_updated(None);
    }

    pub(crate) fn handle_set_seat_persona(
        &mut self,
        conn_id: u64,
        seat: Seat,
        persona_id: Option<PersonaId>,
    ) {
        let Some((_, tx)) = self.require_seated_human(conn_id, "assigning a seat persona") else {
            return;
        };
        if self.phase == TablePhase::InMatch {
            Self::send_error(
                &tx,
                "cannot change seat persona while a match is in progress",
            );
            return;
        }
        if self.seats[seat.index()].info.kind == SeatKind::Human {
            Self::send_error(&tx, "cannot assign a persona to a human seat");
            return;
        }
        if let Some(pid) = persona_id {
            let duplicate = self.seats.iter().any(|slot| {
                slot.info.seat != seat
                    && slot.info.kind != SeatKind::Human
                    && slot.info.persona_id == Some(pid)
            });
            if duplicate {
                Self::send_error(&tx, "persona is already assigned to another seat");
                return;
            }
        }

        let slot = &mut self.seats[seat.index()];
        slot.info.persona_id = persona_id;
        slot.info.voice_pack_id = persona_id.map(|pid| pid.voice_pack_id().to_string());
        if slot.info.kind == SeatKind::Ai {
            if let Some(pid) = persona_id {
                slot.info.nickname = pid.display_name().to_string();
                slot.info.ai_style = Some(pid.default_style());
                slot.info.mood = Some(pid.initial_mood());
            } else {
                slot.info.mood = None;
            }
        } else {
            slot.info.mood = None;
        }
        self.broadcast_lobby_updated(None);
    }

    fn handle_set_pace(&mut self, conn_id: u64, pace: crate::protocol::TablePace) {
        if self
            .require_seated_human(conn_id, "changing table pace")
            .is_none()
        {
            return;
        }
        self.config.pace = pace;
        self.history.record_pace(pace);
        self.broadcast_lobby_updated(None);
    }

    fn handle_set_slow_think_nudge(&mut self, conn_id: u64, enabled: bool) {
        if self
            .require_seated_human(conn_id, "updating slow-think nudge setting")
            .is_none()
        {
            return;
        }
        self.banter.set_slow_think_nudge_enabled(enabled);
        self.history.record_slow_think_nudge(enabled);
        self.broadcast_lobby_updated(None);
    }

    fn handle_send_quick_emote(&mut self, conn_id: u64, emote: QuickEmote) {
        let Some((seat, tx)) = self.require_seated_human(conn_id, "sending a quick emote") else {
            return;
        };
        if self.phase != TablePhase::InMatch {
            Self::send_action_rejected(&tx, "仅可在对局进行中发送快捷互动", self.seq);
            return;
        }
        let Some(active) = self.active_match.as_ref() else {
            Self::send_action_rejected(&tx, "当前没有进行中的对局", self.seq);
            return;
        };
        let current_seq = active.public_seq();
        let now = tokio::time::Instant::now();
        if !self.banter.try_record_quick_emote(seat, now) {
            Self::send_action_rejected(&tx, "快捷互动冷却中，请稍后再试", current_seq);
            return;
        }

        self.broadcast_all(ServerMessage::QuickEmoteBroadcast {
            seat,
            emote,
            text: emote.label().to_string(),
        });

        if self.options.llm_client.is_some() {
            let seat_infos = self.seat_infos();
            let active = self
                .active_match
                .as_ref()
                .expect("active match checked above");
            let ctx = Self::build_banter_plan_context(
                active,
                &seat_infos,
                &self.seat_generation,
                self.options.banter_sync_cap,
                now,
                &self.persona_memory,
            );
            let mut reply_jobs = plan_quick_emote_replies(&ctx, seat, emote);
            self.banter.pending_async.append(&mut reply_jobs);
        }
    }

    pub(crate) fn handle_start_match(&mut self, conn_id: u64) {
        let Some((_, conn_tx)) = self.require_seated_human(conn_id, "starting a match") else {
            return;
        };
        if self.phase == TablePhase::InMatch {
            Self::send_error(&conn_tx, "match is already in progress");
            return;
        }

        let humans: Vec<SeatRecord> = self
            .seats
            .iter()
            .filter(|slot| slot.info.kind == SeatKind::Human)
            .cloned()
            .collect();
        if humans.is_empty() {
            Self::send_error(&conn_tx, "at least 1 seated human is required to start");
            return;
        }

        let wall_provider = (self.options.wall_factory)();
        let game = match ActiveMatch::new(
            self.config.mode,
            AnyRuleset::Tuidaohu(TuidaohuRuleset),
            wall_provider,
        ) {
            Ok(game) => game,
            Err(err) => {
                Self::send_error(&conn_tx, format!("failed to start match: {err}"));
                return;
            }
        };

        let previous_seat_personas: [Option<PersonaId>; 4] = Seat::ALL.map(|seat| {
            let info = &self.seats[seat.index()].info;
            if info.kind == SeatKind::Human {
                None
            } else {
                info.persona_id
            }
        });

        let mut new_seats = Seat::ALL.map(SeatRecord::empty);
        let seat_order: Option<[Seat; 4]> = match self.config.seating {
            SeatingRule::KeepLobbySeats => None,
            SeatingRule::Random => {
                let mut order = Seat::ALL;
                shuffle_slice(&mut order, &mut self.rng);
                Some(order)
            }
            SeatingRule::FixedOpposite => Some([Seat::East, Seat::West, Seat::South, Seat::North]),
            SeatingRule::FixedAdjacent => Some([Seat::East, Seat::South, Seat::West, Seat::North]),
        };

        let ai_target_seats: Vec<Seat> = match seat_order {
            None => {
                for human_record in humans {
                    let seat_idx = human_record.info.seat.index();
                    new_seats[seat_idx] = human_record;
                }
                Seat::ALL
                    .into_iter()
                    .filter(|seat| new_seats[seat.index()].info.kind == SeatKind::Empty)
                    .collect()
            }
            Some(order) => {
                let human_count = humans.len();
                for (idx, mut human_record) in humans.into_iter().enumerate() {
                    let target_seat = order[idx];
                    human_record.info.seat = target_seat;
                    new_seats[target_seat.index()] = human_record;
                }
                order[human_count..].to_vec()
            }
        };
        let assigned_personas = assign_ai_personas(
            &ai_target_seats,
            &previous_seat_personas,
            self.config.seating == SeatingRule::KeepLobbySeats,
            &mut self.rng,
        );
        for (target_seat, persona) in assigned_personas {
            new_seats[target_seat.index()] = SeatRecord {
                info: SeatInfo {
                    seat: target_seat,
                    kind: SeatKind::Ai,
                    nickname: persona.display_name().to_string(),
                    connected: true,
                    ai_assisted: false,
                    ai_difficulty: Some(self.config.ai_difficulty),
                    ai_style: Some(persona.default_style()),
                    persona_id: Some(persona),
                    mood: Some(persona.initial_mood()),
                    voice_pack_id: Some(persona.voice_pack_id().to_string()),
                },
                client_id: None,
                seat_token: None,
            };
        }

        for conn in self.connections.values_mut() {
            conn.seat = new_seats
                .iter()
                .find(|slot| {
                    slot.info.kind == SeatKind::Human
                        && slot.seat_token.as_deref() == Some(&conn.seat_token)
                        && slot.client_id.as_deref() == Some(&conn.client_id)
                })
                .map(|slot| slot.info.seat);
        }

        self.seats = new_seats;
        let match_id = self.next_match_id;
        self.next_match_id += 1;
        let _ = self.bump_release_generation();
        self.bump_all_seat_generations();
        self.seq = game.seq();
        self.set_phase(TablePhase::InMatch);
        self.pending_destructive_op = None;
        self.in_flight_ai.clear();
        self.banter.reset_for_match_transition();
        self.tts.bump_all_seat_banter_generations();
        let hand_attribution = HandAttribution::from_seats(&self.seats);
        let initial_seat_infos = self.seat_infos();
        let initial_hand_facts = initialize_seat_hand_facts(&game, &initial_seat_infos);
        self.active_match = Some(ActiveMatchState {
            match_id,
            match_uuid: Uuid::new_v4().to_string(),
            game,
            accepted_action_ids: HashSet::new(),
            pending_release: None,
            active_discard_window: None,
            ai_turn_trigger: AiTurnTrigger::AfterHandStarted,
            hand_attribution,
            hands_settled: 0,
            hand_generation: 1,
            any_ai_assisted: [false; 4],
            seat_hand_facts: initial_hand_facts,
            seat_last_banter_at: [None; 4],
        });

        self.broadcast_all_snapshots();

        if self.options.llm_client.is_some() {
            let now = tokio::time::Instant::now();
            let seat_infos = self.seat_infos();
            let seat_generation = self.seat_generation;
            let sync_cap = self.options.banter_sync_cap;
            if let Some(active_mut) = self.active_match.as_mut() {
                let job = {
                    let ctx = Self::build_banter_plan_context(
                        active_mut,
                        &seat_infos,
                        &seat_generation,
                        sync_cap,
                        now,
                        &self.persona_memory,
                    );
                    plan_initial_hand_banter(&ctx, &active_mut.seat_hand_facts)
                };
                if let Some(job) = job {
                    active_mut.seat_last_banter_at[job.guard.seat.index()] = Some(now);
                    self.banter.pending_async.push(job);
                }
            }
        }
    }

    fn send_action_rejected(
        tx: &mpsc::UnboundedSender<OutboundSignal>,
        reason: impl Into<String>,
        current_seq: u64,
    ) {
        Self::send_to(
            tx,
            ServerMessage::ActionRejected {
                reason: reason.into(),
                current_seq,
            },
        );
    }

    fn broadcast_discard_only_batch(&self, game: &ActiveMatch, discard_event: &RecordedEvent) {
        let seq = discard_event.seq();
        for conn in self.connections.values() {
            let Some(viewer_seat) = conn.seat else {
                continue;
            };
            let event = game.project_event(discard_event, viewer_seat);
            Self::send_to(
                &conn.tx,
                ServerMessage::EventBatch {
                    from_seq: seq,
                    to_seq: seq,
                    events: vec![event],
                    updated_legal_actions: Vec::new(),
                },
            );
        }
    }

    fn broadcast_event_batch(&self, game: &ActiveMatch, emitted: &[RecordedEvent]) {
        let Some(first) = emitted.first() else {
            return;
        };
        let Some(last) = emitted.last() else {
            return;
        };
        let from_seq = first.seq();
        let to_seq = last.seq();
        for conn in self.connections.values() {
            let Some(viewer_seat) = conn.seat else {
                continue;
            };
            let events: Vec<PlayerEvent> = emitted
                .iter()
                .map(|event| game.project_event(event, viewer_seat))
                .collect();
            let updated_legal_actions = game.project_view(viewer_seat).legal_actions;
            Self::send_to(
                &conn.tx,
                ServerMessage::EventBatch {
                    from_seq,
                    to_seq,
                    events,
                    updated_legal_actions,
                },
            );
        }
    }

    pub(crate) fn handle_submit_action(
        &mut self,
        conn_id: u64,
        expected_seq: u64,
        client_action_id: String,
        action: PlayerAction,
    ) {
        let Some((seat, tx)) = self.require_seated_human(conn_id, "submitting actions") else {
            return;
        };
        if self.phase != TablePhase::InMatch {
            Self::send_action_rejected(
                &tx,
                "can only submit actions during an active match",
                self.seq,
            );
            return;
        }
        if client_action_id.trim().is_empty() {
            let current_seq = self
                .active_match
                .as_ref()
                .map_or(self.seq, ActiveMatchState::public_seq);
            Self::send_action_rejected(&tx, "client_action_id must not be empty", current_seq);
            return;
        }

        let Some(active) = self.active_match.as_ref() else {
            Self::send_action_rejected(&tx, "no active match", self.seq);
            return;
        };
        if active
            .accepted_action_ids
            .contains(&(seat, client_action_id.clone()))
        {
            return;
        }

        let current_seq = active.public_seq();
        if self.seats[seat.index()].info.ai_assisted {
            Self::send_action_rejected(
                &tx,
                "AI assist is active for this seat; reclaim control first",
                current_seq,
            );
            return;
        }
        if active.pending_release.is_some() {
            Self::send_action_rejected(&tx, "discard release is still pending", current_seq);
            return;
        }
        if expected_seq != current_seq {
            Self::send_action_rejected(
                &tx,
                format!("expected seq {expected_seq}, but current seq is {current_seq}"),
                current_seq,
            );
            return;
        }

        if matches!(action, PlayerAction::EndMatch) && self.online_humans() >= 2 {
            Self::send_action_rejected(
                &tx,
                "cannot end FreeTable directly while multiple humans are online; use RequestEndFreeTable",
                current_seq,
            );
            return;
        }

        self.apply_match_step(seat, action, Some((tx, client_action_id)));
    }

    fn apply_match_step(
        &mut self,
        seat: Seat,
        action: PlayerAction,
        caller: Option<(mpsc::UnboundedSender<OutboundSignal>, String)>,
    ) {
        let humanized = self.options.pacing == PacingMode::Humanized;
        let pace = self.config.pace;
        let Some(active) = self.active_match.as_mut() else {
            return;
        };
        let prev_seq = active.game.seq();
        let capture_pre_views = humanized
            && (matches!(action, PlayerAction::Discard { .. })
                || (action == PlayerAction::Pass
                    && active
                        .active_discard_window
                        .as_ref()
                        .is_some_and(|w| !w.had_human_reactor)));
        let mut pre_views =
            capture_pre_views.then(|| Seat::ALL.map(|s| active.game.project_view(s)));

        let mut emitted = match active.game.apply_action(seat, action) {
            Ok(events) => events,
            Err(err) => {
                if let Some((tx, _)) = caller {
                    Self::send_action_rejected(&tx, err.to_string(), prev_seq);
                }
                return;
            }
        };

        if let Some((tx, action_id)) = caller {
            active.accepted_action_ids.insert((seat, action_id));
            if emitted.is_empty() {
                Self::send_to(
                    &tx,
                    ServerMessage::EventBatch {
                        from_seq: prev_seq,
                        to_seq: prev_seq,
                        events: Vec::new(),
                        updated_legal_actions: Vec::new(),
                    },
                );
                return;
            }
        } else if emitted.is_empty() {
            return;
        }

        if !humanized {
            active.active_discard_window = None;
            self.finalize_emitted_events(emitted);
            return;
        }

        let release_delay = discard_release_delay(self.options.pacing, pace);

        if let PlayerAction::Discard { tile, from_drawn } = action {
            if emitted.len() >= 2 && !release_delay.is_zero() {
                let discard_event = emitted.remove(0);
                let discard_seq = discard_event.seq();
                let mut public_views = pre_views.take().expect("captured for discard");
                for view in &mut public_views {
                    apply_discard_to_view(view, seat, tile, from_drawn, discard_seq);
                }
                let release_at = tokio::time::Instant::now() + release_delay;
                self.stage_pending_release(discard_seq, public_views, emitted, release_at);
                let active_ref = self.active_match.as_ref().expect("active match exists");
                self.broadcast_discard_only_batch(&active_ref.game, &discard_event);
                return;
            }

            if emitted.len() == 1 {
                let discard_seq = emitted[0].seq();
                let had_human_reactor = Seat::ALL.into_iter().any(|s| {
                    let info = &self.seats[s.index()].info;
                    info.kind == SeatKind::Human
                        && !info.ai_assisted
                        && !active.game.project_view(s).legal_actions.is_empty()
                });
                active.active_discard_window = Some(ActiveDiscardWindow {
                    discard_seq,
                    emitted_at: tokio::time::Instant::now(),
                    had_human_reactor,
                });
                self.finalize_emitted_events(emitted);
                return;
            }
        }

        let active_window = active.active_discard_window.take();
        if action == PlayerAction::Pass {
            if let Some(window) = active_window {
                let is_draw_or_draw_game = matches!(
                    active.game.project_event(&emitted[0], Seat::East).kind,
                    PlayerEventKind::TileDrawn { .. }
                        | PlayerEventKind::HandSettled {
                            settlement: HandSettlement {
                                outcome: HandOutcome::DrawGame,
                                ..
                            }
                        }
                );
                if !window.had_human_reactor && is_draw_or_draw_game {
                    let release_at = window.emitted_at + release_delay;
                    if tokio::time::Instant::now() < release_at {
                        let mut public_views = pre_views.take().expect("captured for ai pass");
                        for view in &mut public_views {
                            view.legal_actions.clear();
                        }
                        self.stage_pending_release(
                            window.discard_seq,
                            public_views,
                            emitted,
                            release_at,
                        );
                        return;
                    }
                }
            }
        }

        self.finalize_emitted_events(emitted);
    }

    fn stage_pending_release(
        &mut self,
        public_seq: u64,
        public_views: [PlayerView; 4],
        deferred_events: Vec<RecordedEvent>,
        release_at: tokio::time::Instant,
    ) {
        let release_generation = self.bump_release_generation();
        let active_mut = self.active_match.as_mut().expect("active match exists");
        active_mut.active_discard_window = None;
        active_mut.ai_turn_trigger = AiTurnTrigger::Normal;
        active_mut.pending_release = Some(PendingRelease {
            ticket: ReleaseTicket {
                match_id: active_mut.match_id,
                public_seq,
                release_generation,
                release_at,
            },
            public_views,
            deferred_events,
            scheduled: false,
        });
        self.seq = public_seq;
        self.in_flight_ai.clear();
    }

    pub(crate) fn finalize_emitted_events(&mut self, mut emitted: Vec<RecordedEvent>) {
        let Some(active) = self.active_match.as_ref() else {
            return;
        };
        let match_id = active.match_id;
        self.persist_settled_hands(&emitted);
        let mut public_phase = self
            .active_match
            .as_ref()
            .expect("active match exists")
            .game
            .project_view(Seat::East)
            .phase;
        if public_phase == PublicPhase::BetweenHands {
            self.auto_confirm_ai_between_hands(&mut emitted);
            if let Some(active_after) = self.active_match.as_ref() {
                public_phase = active_after.game.project_view(Seat::East).phase;
            }
        }
        self.persist_finished_matches(&emitted);

        let started_new_hand = self.active_match.as_ref().is_some_and(|active_ref| {
            emitted.iter().any(|event| {
                matches!(
                    active_ref.game.project_event(event, Seat::East).kind,
                    PlayerEventKind::HandStarted { .. }
                )
            })
        });
        let next_hand_attribution =
            started_new_hand.then(|| HandAttribution::from_seats(&self.seats));

        let Some(active_mut) = self.active_match.as_mut() else {
            return;
        };
        if let Some(attribution) = next_hand_attribution {
            active_mut.hand_attribution = attribution;
            active_mut.hand_generation += 1;
            self.tts.clear_pending_slot();
        }
        let next_trigger = compute_ai_turn_trigger(&active_mut.game, &emitted);
        active_mut.ai_turn_trigger = next_trigger;
        let new_seq = active_mut.game.seq();
        self.seq = new_seq;
        self.in_flight_ai
            .retain(|k| k.match_id == match_id && k.seq == new_seq);

        let active_ref = self.active_match.as_ref().expect("active match exists");
        self.broadcast_event_batch(&active_ref.game, &emitted);

        if self.options.llm_client.is_some() {
            let now = tokio::time::Instant::now();
            let seat_infos = self.seat_infos();
            let seat_generation = self.seat_generation;
            let sync_cap = self.options.banter_sync_cap;
            if let Some(active_mut) = self.active_match.as_mut() {
                let mut seat_hand_facts = std::mem::take(&mut active_mut.seat_hand_facts);
                let (mut sync_jobs, mut async_jobs) = {
                    let ctx = Self::build_banter_plan_context(
                        active_mut,
                        &seat_infos,
                        &seat_generation,
                        sync_cap,
                        now,
                        &self.persona_memory,
                    );
                    plan_banter_for_events(&ctx, &emitted, &mut seat_hand_facts)
                };
                active_mut.seat_hand_facts = seat_hand_facts;
                for job in sync_jobs.iter().chain(async_jobs.iter()) {
                    active_mut.seat_last_banter_at[job.guard.seat.index()] = Some(now);
                }
                self.banter.pending_sync.append(&mut sync_jobs);
                self.banter.pending_async.append(&mut async_jobs);
            }
        }

        match public_phase {
            PublicPhase::MatchFinished => {
                self.set_phase(TablePhase::Finished);
                self.pending_destructive_op = None;
                self.in_flight_ai.clear();
                self.broadcast_all_snapshots();
            }
            PublicPhase::Playing => {
                if self
                    .pending_destructive_op
                    .as_ref()
                    .is_some_and(|op| op.kind == DestructiveOpKind::EndFreeTable)
                {
                    self.pending_destructive_op = None;
                    self.broadcast_all_snapshots();
                }
            }
            PublicPhase::BetweenHands => {}
        }
    }

    fn persist_settled_hands(&mut self, emitted: &[RecordedEvent]) {
        let mode = self.config.mode;
        let timestamp_ms = current_unix_ms();
        let seat_infos = self.seat_infos();
        let has_llm = self.options.llm_client.is_some();
        let Some(active) = self.active_match.as_mut() else {
            return;
        };
        for event in emitted {
            let PlayerEventKind::HandSettled { settlement } =
                active.game.project_event(event, Seat::East).kind
            else {
                continue;
            };
            active.hands_settled += 1;
            for seat in Seat::ALL {
                if active.hand_attribution.assisted_by_ai[seat.index()] {
                    active.any_ai_assisted[seat.index()] = true;
                }
            }
            self.persona_memory
                .record_hand_settled(&seat_infos, &settlement, timestamp_ms);
            if has_llm {
                let mut note_jobs = plan_memorable_hand_note_jobs(&self.seats, &settlement);
                self.banter.pending_memory_notes.append(&mut note_jobs);
            }
            let seats = Seat::ALL.map(|seat| {
                let idx = seat.index();
                let slot = &self.seats[idx];
                HandSeatRecord {
                    seat,
                    nickname: normalize_nickname(&slot.info.nickname),
                    seat_kind: slot.info.kind,
                    assisted_by_ai: active.hand_attribution.assisted_by_ai[idx],
                    took_over_ai: active.hand_attribution.took_over_ai_in_hand[idx],
                }
            });
            self.history.append_hand(HandRecord {
                schema_version: HISTORY_SCHEMA_VERSION,
                match_id: active.match_uuid.clone(),
                hand_index: active.hands_settled,
                timestamp_ms,
                mode,
                settlement,
                seats,
                aborted: false,
            });
        }
    }

    fn persist_finished_matches(&mut self, emitted: &[RecordedEvent]) {
        let timestamp_ms = current_unix_ms();
        let has_llm = self.options.llm_client.is_some();
        let Some(active) = self.active_match.as_ref() else {
            return;
        };
        for event in emitted {
            let PlayerEventKind::MatchFinished { summary } =
                active.game.project_event(event, Seat::East).kind
            else {
                continue;
            };
            if summary.hands_played == 0 {
                continue;
            }
            if has_llm {
                let mut note_jobs = plan_match_finished_note_jobs(&self.seats, &summary);
                self.banter.pending_memory_notes.append(&mut note_jobs);
            }
            let seats = Seat::ALL.map(|seat| {
                let idx = seat.index();
                let slot = &self.seats[idx];
                MatchSeatRecord {
                    seat,
                    nickname: normalize_nickname(&slot.info.nickname),
                    seat_kind: slot.info.kind,
                    assisted_by_ai: active.any_ai_assisted[idx],
                    final_score: summary.final_scores[idx],
                    net_delta: summary.net_deltas[idx],
                }
            });
            self.history.append_match(MatchRecord {
                schema_version: HISTORY_SCHEMA_VERSION,
                match_id: active.match_uuid.clone(),
                timestamp_ms,
                mode: summary.mode,
                hands_played: summary.hands_played,
                initial_scores: summary.initial_scores,
                final_scores: summary.final_scores,
                net_deltas: summary.net_deltas,
                seats,
            });
        }
    }

    fn ai_decision_context(&self, seat: Seat) -> DecisionContext {
        let slot = &self.seats[seat.index()];
        DecisionContext {
            difficulty: slot.info.ai_difficulty.unwrap_or(self.config.ai_difficulty),
            style: slot.info.ai_style.unwrap_or(AiStyle::Balanced),
            budget: self.options.ai_budget.unwrap_or_default(),
        }
    }

    pub(crate) fn auto_confirm_ai_between_hands(&mut self, emitted: &mut Vec<RecordedEvent>) {
        let Some(match_id) = self.active_match.as_ref().map(|a| a.match_id) else {
            return;
        };
        for seat in Seat::ALL {
            if !self.is_ai_controlled(seat) {
                continue;
            }
            let Some(view) = self
                .active_match
                .as_ref()
                .map(|a| a.game.project_view(seat))
            else {
                return;
            };
            if !view.legal_actions.contains(&PlayerAction::ConfirmNextHand) {
                continue;
            }
            let ctx = self.ai_decision_context(seat);
            let step_seed = derive_ai_step_seed(self.ai_base_seed, match_id, view.seq, seat);
            let mut rng = ChaCha8Rng::seed_from_u64(step_seed);
            let decided = mahjong_ai::decide(&view, &ctx, &mut rng);
            let action = if decided == PlayerAction::EndMatch {
                PlayerAction::ConfirmNextHand
            } else {
                decided
            };
            if let Some(active) = self.active_match.as_mut() {
                if let Ok(mut more) = active.game.apply_action(seat, action) {
                    emitted.append(&mut more);
                }
            }
        }
    }

    fn take_unscheduled_pending_release(&mut self) -> Option<ReleaseTicket> {
        if self.phase != TablePhase::InMatch {
            return None;
        }
        let active = self.active_match.as_mut()?;
        let pending = active.pending_release.as_mut()?;
        if pending.scheduled {
            return None;
        }
        pending.scheduled = true;
        Some(pending.ticket)
    }

    fn collect_pending_ai_jobs(&mut self) -> Vec<PendingAiJob> {
        if self.phase != TablePhase::InMatch {
            return Vec::new();
        }
        let Some(active) = self.active_match.as_ref() else {
            return Vec::new();
        };
        if active.pending_release.is_some() {
            return Vec::new();
        }
        let match_id = active.match_id;
        let seq = active.game.seq();
        if active.game.project_view(Seat::East).phase != PublicPhase::Playing {
            return Vec::new();
        }

        let started_at = tokio::time::Instant::now();
        let trigger = active.ai_turn_trigger;
        let mut jobs = Vec::new();
        for seat in Seat::ALL {
            if !self.is_ai_controlled(seat) {
                continue;
            }
            let view = active.game.project_view(seat);
            if view.legal_actions.is_empty() {
                continue;
            }
            let key = AiJobKey {
                match_id,
                seq,
                seat,
                seat_generation: self.seat_generation[seat.index()],
            };
            if !self.in_flight_ai.insert(key) {
                continue;
            }
            jobs.push(PendingAiJob {
                key,
                view,
                ctx: self.ai_decision_context(seat),
                step_seed: derive_ai_step_seed(self.ai_base_seed, match_id, seq, seat),
                started_at,
                trigger,
            });
        }
        jobs
    }

    fn find_waiting_human_seat(&self, active: &ActiveMatchState) -> Option<Seat> {
        let sample_view = active.game.project_view(Seat::East);
        if sample_view.phase != PublicPhase::Playing {
            return None;
        }
        let mut waiting = Vec::new();
        for seat in Seat::ALL {
            let info = &self.seats[seat.index()].info;
            if info.kind == SeatKind::Human
                && !info.ai_assisted
                && info.connected
                && !active.game.project_view(seat).legal_actions.is_empty()
            {
                waiting.push(seat);
            }
        }
        if waiting.is_empty() {
            return None;
        }
        if let Some(active_seat) = sample_view.turn.active_seat {
            if waiting.contains(&active_seat) {
                return Some(active_seat);
            }
        }
        waiting.first().copied()
    }

    fn collect_slow_think_nudge(&mut self) -> Option<SlowThinkTicket> {
        if self.phase != TablePhase::InMatch
            || !self.banter.slow_think_nudge_enabled
            || self.options.llm_client.is_none()
        {
            self.banter.clear_active_nudge();
            return None;
        }
        let has_ai_persona = self
            .seats
            .iter()
            .any(|slot| slot.info.kind == SeatKind::Ai && slot.info.persona_id.is_some());
        if !has_ai_persona {
            self.banter.clear_active_nudge();
            return None;
        }
        let Some(active) = self.active_match.as_ref() else {
            self.banter.clear_active_nudge();
            return None;
        };
        if active.pending_release.is_some() {
            self.banter.clear_active_nudge();
            return None;
        }
        let Some(target_seat) = self.find_waiting_human_seat(active) else {
            self.banter.clear_active_nudge();
            return None;
        };

        let turn = NudgeTurnKey {
            match_id: active.match_id,
            hand_generation: active.hand_generation,
            event_seq: active.game.seq(),
            seat: target_seat,
        };
        let target_seat_generation = self.seat_generation[target_seat.index()];
        self.banter
            .schedule_slow_think_nudge(turn, target_seat_generation)
    }

    pub(crate) fn fire_slow_think_nudge(
        &mut self,
        ticket: SlowThinkTicket,
    ) -> Option<PendingBanterJob> {
        if !self.banter.consume_matching_nudge(ticket) {
            return None;
        }

        if self.phase != TablePhase::InMatch
            || !self.banter.slow_think_nudge_enabled
            || self.options.llm_client.is_none()
        {
            return None;
        }
        let target_slot = &self.seats[ticket.target_seat.index()];
        if target_slot.info.kind != SeatKind::Human
            || target_slot.info.ai_assisted
            || !target_slot.info.connected
            || self.seat_generation[ticket.target_seat.index()] != ticket.target_seat_generation
        {
            return None;
        }
        let turn = NudgeTurnKey {
            match_id: ticket.match_id,
            hand_generation: ticket.hand_generation,
            event_seq: ticket.seq,
            seat: ticket.target_seat,
        };
        if self.banter.is_turn_already_nudged(turn) {
            return None;
        }

        let random_pick = self.rng.next_u64();
        let seat_infos = self.seat_infos();
        let active = self.active_match.as_ref()?;
        if active.match_id != ticket.match_id
            || active.hand_generation != ticket.hand_generation
            || active.game.seq() != ticket.seq
            || active.pending_release.is_some()
        {
            return None;
        }
        let target_view = active.game.project_view(ticket.target_seat);
        if target_view.phase != PublicPhase::Playing || target_view.legal_actions.is_empty() {
            return None;
        }

        let now = tokio::time::Instant::now();
        let ctx = Self::build_banter_plan_context(
            active,
            &seat_infos,
            &self.seat_generation,
            self.options.banter_sync_cap,
            now,
            &self.persona_memory,
        );
        let job = plan_slow_think_nudge(&ctx, ticket.target_seat, random_pick)?;
        self.banter.mark_turn_nudged(turn);
        Some(job)
    }

    fn collect_scheduled_work(&mut self) -> ScheduledWork {
        let pending_release = self.take_unscheduled_pending_release();
        let ai_jobs = self.collect_pending_ai_jobs();
        let (sync_banter, async_banter, memory_notes) = self.banter.take_pending();
        let slow_think_nudge = self.collect_slow_think_nudge();
        ScheduledWork {
            ai_jobs,
            pending_release,
            sync_banter,
            async_banter,
            slow_think_nudge,
            memory_notes,
        }
    }

    pub(crate) fn commit_memory_note_result(
        &mut self,
        persona_id: PersonaId,
        player_nickname: &str,
        trigger: PersonaMemoryNoteTrigger,
        result: Result<String, LlmError>,
    ) {
        match result {
            Ok(note) => {
                if !note.trim().is_empty() {
                    self.persona_memory.record_grudge_note(
                        persona_id,
                        player_nickname,
                        &note,
                        trigger,
                        current_unix_ms(),
                    );
                }
            }
            Err(err) => {
                eprintln!(
                    "warning: failed to generate persona memory note for {persona_id:?}/{player_nickname}: {err}"
                );
            }
        }
    }

    pub(crate) fn broadcast_all(&self, msg: ServerMessage) {
        for conn in self.connections.values() {
            Self::send_to(&conn.tx, msg.clone());
        }
    }

    fn is_banter_guard_fresh(&self, guard: &BanterGuard) -> bool {
        let phase_ok = self.phase == TablePhase::InMatch
            || (guard.allow_finished_phase && self.phase == TablePhase::Finished);
        if !phase_ok || (guard.require_nudge_enabled && !self.banter.slow_think_nudge_enabled) {
            return false;
        }
        let seat_slot = &self.seats[guard.seat.index()];
        if seat_slot.info.kind != SeatKind::Ai
            || seat_slot.info.persona_id != Some(guard.persona_id)
            || self.seat_generation[guard.seat.index()] != guard.seat_generation
        {
            return false;
        }
        self.active_match.as_ref().is_some_and(|active| {
            active.match_id == guard.match_id
                && active.hand_generation == guard.hand_generation
                && guard.max_seq_drift.is_none_or(|max_drift| {
                    active.game.seq().saturating_sub(guard.event_seq) <= max_drift
                })
        })
    }

    pub(crate) fn can_dispatch_delayed_banter(&mut self, guard: &BanterGuard) -> bool {
        if !self.is_banter_guard_fresh(guard) {
            return false;
        }
        let now = tokio::time::Instant::now();
        let Some(active) = self.active_match.as_mut() else {
            return false;
        };
        if !is_seat_off_cooldown(
            &active.seat_last_banter_at,
            guard.seat,
            now,
            SEAT_BANTER_COOLDOWN,
        ) {
            return false;
        }
        active.seat_last_banter_at[guard.seat.index()] = Some(now);
        true
    }

    pub(crate) fn commit_banter_result(
        &mut self,
        guard: BanterGuard,
        chain: Option<BanterChainContext>,
        result: Result<BanterOutput, LlmError>,
    ) -> BanterCommitOutcome {
        let phase_ok = self.phase == TablePhase::InMatch
            || (guard.allow_finished_phase && self.phase == TablePhase::Finished);
        let match_ok = self
            .active_match
            .as_ref()
            .is_some_and(|active| active.match_id == guard.match_id);

        match result {
            Err(err) => {
                if phase_ok && match_ok {
                    if let Some(notice) = self.banter.record_llm_failure(&err.to_string()) {
                        self.broadcast_all(notice);
                    }
                }
                BanterCommitOutcome::default()
            }
            Ok(output) => {
                if let Some(notice) = self.banter.record_llm_recovery() {
                    self.broadcast_all(notice);
                }

                if !self.is_banter_guard_fresh(&guard) {
                    return BanterCommitOutcome::default();
                }

                tracing::debug!(
                    seat = ?guard.seat,
                    persona_id = ?guard.persona_id,
                    speak = output.speak,
                    mood = ?output.mood,
                    text_chars = output.text.chars().count(),
                    "committing banter decision"
                );

                if !output.speak {
                    let seat_info = &mut self.seats[guard.seat.index()].info;
                    if seat_info.mood != Some(output.mood) {
                        seat_info.mood = Some(output.mood);
                        self.broadcast_all(ServerMessage::PersonaMoodUpdated {
                            seat: guard.seat,
                            persona_id: guard.persona_id,
                            mood: output.mood,
                        });
                    }
                    return BanterCommitOutcome::default();
                }

                let now = tokio::time::Instant::now();
                if let Some(active) = self.active_match.as_mut() {
                    active.seat_last_banter_at[guard.seat.index()] = Some(now);
                }

                let (banter_id, seat_banter_generation) = self.tts.allocate_banter_id(guard.seat);
                self.seats[guard.seat.index()].info.mood = Some(output.mood);
                self.broadcast_all(ServerMessage::TableBanter {
                    banter_id: banter_id.clone(),
                    seat: guard.seat,
                    persona_id: guard.persona_id,
                    mood: output.mood,
                    text: output.text.clone(),
                    event_seq: guard.event_seq,
                    reply_to_seat: guard.reply_to_seat,
                });

                let tts_job = if self.tts.should_dispatch(now) {
                    let deadline_ms = if guard.allow_finished_phase {
                        TTS_SETTLEMENT_DEADLINE_MS
                    } else {
                        TTS_IN_HAND_DEADLINE_MS
                    };
                    let job = PendingTtsJob {
                        guard: TtsGuard {
                            banter_id: banter_id.clone(),
                            match_id: guard.match_id,
                            hand_generation: guard.hand_generation,
                            seat: guard.seat,
                            seat_generation: guard.seat_generation,
                            seat_banter_generation,
                            persona_id: guard.persona_id,
                            allow_finished_phase: guard.allow_finished_phase,
                        },
                        request: TtsSynthesisRequest::new(
                            banter_id,
                            guard.persona_id,
                            output.mood,
                            output.text.clone(),
                            deadline_ms,
                        ),
                        dispatched_at: now,
                        deadline_ms,
                    };
                    self.enqueue_tts_job(job, now)
                } else {
                    None
                };

                let followup = chain.and_then(|chain| {
                    let seat_infos = self.seat_infos();
                    let active_ref = self.active_match.as_ref()?;
                    let ctx = Self::build_banter_plan_context(
                        active_ref,
                        &seat_infos,
                        &self.seat_generation,
                        self.options.banter_sync_cap,
                        now,
                        &self.persona_memory,
                    );
                    let prev = ChainPrevLine {
                        guard,
                        mood: output.mood,
                        text: &output.text,
                    };
                    plan_chain_followup(
                        &ctx,
                        &prev,
                        chain,
                        &active_ref.seat_hand_facts,
                        BANTER_CHAIN_REPLY_DELAY,
                    )
                });

                BanterCommitOutcome { followup, tts_job }
            }
        }
    }

    pub(crate) fn cancel_ai_job(&mut self, key: AiJobKey) {
        self.in_flight_ai.remove(&key);
    }

    pub(crate) fn plan_ai_action_delay(
        &mut self,
        key: AiJobKey,
        action: PlayerAction,
        trigger: AiTurnTrigger,
        step_seed: u64,
    ) -> Option<Duration> {
        let is_valid = self.phase == TablePhase::InMatch
            && self.is_ai_controlled(key.seat)
            && self.seat_generation[key.seat.index()] == key.seat_generation
            && self.active_match.as_ref().is_some_and(|active| {
                active.match_id == key.match_id
                    && active.pending_release.is_none()
                    && active.game.seq() == key.seq
                    && self.in_flight_ai.contains(&key)
            });
        if !is_valid {
            self.in_flight_ai.remove(&key);
            return None;
        }
        let wall_remaining = self.active_match.as_ref().map_or(0, |active| {
            usize::from(active.game.project_view(key.seat).wall_remaining)
        });
        let persona_id = self.seats[key.seat.index()].info.persona_id;
        Some(target_ai_delay(
            self.options.pacing,
            self.config.pace,
            action,
            trigger,
            persona_id,
            wall_remaining,
            step_seed,
        ))
    }

    pub(crate) fn complete_pending_release(&mut self, ticket: ReleaseTicket) -> ScheduledWork {
        if self.phase != TablePhase::InMatch {
            return ScheduledWork::default();
        }
        let Some(active) = self.active_match.as_mut() else {
            return ScheduledWork::default();
        };
        if active.match_id != ticket.match_id {
            return ScheduledWork::default();
        }
        let matches_pending = active
            .pending_release
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket);
        if !matches_pending {
            return ScheduledWork::default();
        }
        let pending = active.pending_release.take().expect("verified above");
        self.finalize_emitted_events(pending.deferred_events);
        self.collect_scheduled_work()
    }

    pub(crate) fn complete_ai_job(&mut self, key: AiJobKey, action: PlayerAction) -> ScheduledWork {
        self.in_flight_ai.remove(&key);
        if self.phase != TablePhase::InMatch {
            return ScheduledWork::default();
        }
        if !self.is_ai_controlled(key.seat)
            || self.seat_generation[key.seat.index()] != key.seat_generation
        {
            return ScheduledWork::default();
        }
        let matches_state = self.active_match.as_ref().is_some_and(|active| {
            active.match_id == key.match_id
                && active.pending_release.is_none()
                && active.game.seq() == key.seq
        });
        if !matches_state {
            return ScheduledWork::default();
        }

        self.apply_match_step(key.seat, action, None);
        self.collect_scheduled_work()
    }

    fn online_human_seats(&self) -> Vec<Seat> {
        Seat::ALL
            .into_iter()
            .filter(|&seat| {
                let slot = &self.seats[seat.index()];
                slot.info.kind == SeatKind::Human && slot.info.connected
            })
            .collect()
    }

    fn reset_match_to_lobby(&mut self) {
        if self.phase == TablePhase::InMatch {
            if let Some(active) = self.active_match.as_ref() {
                if active.hands_settled > 0 {
                    self.history.mark_match_aborted(&active.match_uuid);
                }
            }
        }
        let _ = self.bump_release_generation();
        self.bump_all_seat_generations();
        self.set_phase(TablePhase::Lobby);
        self.active_match = None;
        self.in_flight_ai.clear();
        self.banter.reset_for_match_transition();
        self.tts.bump_all_seat_banter_generations();
        self.pending_destructive_op = None;
        self.seq = 0;
        for slot in &mut self.seats {
            if slot.info.kind == SeatKind::Ai {
                slot.clear_to_empty();
            }
        }
        self.broadcast_all_snapshots();
    }

    fn handle_return_to_lobby(&mut self, conn_id: u64) {
        let Some((_, tx)) = self.require_seated_human(conn_id, "returning to lobby") else {
            return;
        };
        if self.phase != TablePhase::Finished {
            Self::send_error(&tx, "can only return to lobby when match is Finished");
            return;
        }
        self.reset_match_to_lobby();
    }

    fn is_end_free_table_allowed(&self) -> bool {
        self.phase == TablePhase::InMatch
            && self.config.mode == MatchMode::FreeTable
            && self.active_match.as_ref().is_some_and(|a| {
                a.pending_release.is_none()
                    && a.game.project_view(Seat::East).phase == PublicPhase::BetweenHands
            })
    }

    fn execute_destructive_op(&mut self, kind: DestructiveOpKind, actor_seat: Seat) {
        match kind {
            DestructiveOpKind::DisbandMatch => {
                self.reset_match_to_lobby();
            }
            DestructiveOpKind::EndFreeTable => {
                if self.is_end_free_table_allowed() {
                    self.apply_match_step(actor_seat, PlayerAction::EndMatch, None);
                } else {
                    self.pending_destructive_op = None;
                    self.broadcast_all_snapshots();
                }
            }
        }
    }

    fn resolve_or_update_pending_op(&mut self, op: PendingDestructiveOp, actor_on_complete: Seat) {
        if op
            .required_seats
            .iter()
            .all(|req| op.confirmed_seats.contains(req))
        {
            self.execute_destructive_op(op.kind, actor_on_complete);
        } else {
            self.pending_destructive_op = Some(op);
            self.broadcast_all_snapshots();
        }
    }

    fn handle_request_destructive_op(&mut self, conn_id: u64, kind: DestructiveOpKind) {
        let Some((seat, tx)) = self.require_seated_human(conn_id, "requesting this operation")
        else {
            return;
        };
        if self.phase != TablePhase::InMatch {
            Self::send_error(&tx, "destructive table request is only valid InMatch");
            return;
        }
        if kind == DestructiveOpKind::EndFreeTable && !self.is_end_free_table_allowed() {
            Self::send_error(
                &tx,
                "RequestEndFreeTable is only valid during BetweenHands in FreeTable mode",
            );
            return;
        }
        if self.pending_destructive_op.is_some() {
            Self::send_error(&tx, "another confirmation request is already pending");
            return;
        }

        let required_seats = self.online_human_seats();
        if required_seats.len() <= 1 {
            self.execute_destructive_op(kind, seat);
            return;
        }

        self.pending_destructive_op = Some(PendingDestructiveOp {
            op_id: Uuid::new_v4().to_string(),
            kind,
            requested_by: seat,
            required_seats,
            confirmed_seats: vec![seat],
        });
        self.broadcast_all_snapshots();
    }

    fn handle_confirm_destructive_op(&mut self, conn_id: u64, op_id: String, accept: bool) {
        let Some((seat, tx)) = self.require_seated_human(conn_id, "confirming this operation")
        else {
            return;
        };
        let Some(mut op) = self.pending_destructive_op.take() else {
            Self::send_error(&tx, "no pending destructive operation");
            return;
        };
        if op.op_id != op_id {
            self.pending_destructive_op = Some(op);
            Self::send_error(&tx, "destructive operation id does not match");
            return;
        }
        if !op.required_seats.contains(&seat) {
            self.pending_destructive_op = Some(op);
            Self::send_error(&tx, "seat is not part of required confirmations");
            return;
        }

        if !accept {
            self.broadcast_all_snapshots();
            return;
        }

        if !op.confirmed_seats.contains(&seat) {
            op.confirmed_seats.push(seat);
        }
        let requested_by = op.requested_by;
        self.resolve_or_update_pending_op(op, requested_by);
    }

    fn reconcile_pending_destructive_op_on_disconnect(&mut self) {
        let Some(mut op) = self.pending_destructive_op.take() else {
            return;
        };
        op.required_seats.retain(|s| {
            let slot = &self.seats[s.index()];
            slot.info.kind == SeatKind::Human && slot.info.connected
        });
        op.confirmed_seats.retain(|s| op.required_seats.contains(s));

        if op.required_seats.is_empty() {
            return;
        }
        let actor = op
            .confirmed_seats
            .first()
            .copied()
            .unwrap_or(op.requested_by);
        self.resolve_or_update_pending_op(op, actor);
    }

    pub(crate) fn on_disconnect(&mut self, conn_id: u64) {
        let Some(removed) = self.connections.remove(&conn_id) else {
            return;
        };
        if self.token_to_conn.get(&removed.seat_token).copied() == Some(conn_id) {
            self.token_to_conn.remove(&removed.seat_token);
            if let Some(seat) = removed.seat {
                let slot = &mut self.seats[seat.index()];
                if slot.info.kind == SeatKind::Human
                    && slot.seat_token.as_deref() == Some(&removed.seat_token)
                    && slot.info.connected
                {
                    slot.info.connected = false;
                    self.banter.clear_nudge_for_seat(seat);
                    self.broadcast_lobby_updated(None);
                    self.reconcile_pending_destructive_op_on_disconnect();
                }
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct BanterCommitOutcome {
    pub followup: Option<PendingBanterJob>,
    pub tts_job: Option<PendingTtsJob>,
}

struct HelloParams {
    client_id: String,
    nickname: String,
    table_id: Option<String>,
    seat_token: Option<String>,
}

/// Thread-safe handle around the single active [`TableSession`].
#[derive(Clone)]
pub struct TableRuntime {
    inner: Arc<Mutex<TableSession>>,
    ai_pool: Arc<rayon::ThreadPool>,
    llm_client: Option<Arc<dyn LlmClient>>,
    tts_client: Option<Arc<dyn TtsClient>>,
    banter_audio_cache: Arc<std::sync::Mutex<BanterAudioCache>>,
}

impl TableRuntime {
    /// Creates a fresh table runtime in `Lobby` phase for the given bound port.
    #[must_use]
    pub fn new(port: u16) -> Self {
        let local_hostname = read_local_hostname();
        Self::with_hostname(port, local_hostname)
    }

    /// Creates a fresh table runtime with an explicit local hostname (useful for tests/startup).
    #[must_use]
    pub fn with_hostname(port: u16, local_hostname: Option<String>) -> Self {
        Self::with_options(port, local_hostname, TableOptions::default())
    }

    /// Creates a fresh table runtime with custom [`TableOptions`] and the default AI thread pool size.
    ///
    /// # Panics
    ///
    /// Panics if the dedicated Rayon AI thread pool fails to initialize.
    #[must_use]
    pub fn with_options(port: u16, local_hostname: Option<String>, options: TableOptions) -> Self {
        Self::try_with_options_and_threads(port, local_hostname, options, default_ai_threads())
            .expect("default AI thread pool must initialize")
    }

    /// Creates a fresh table runtime with custom [`TableOptions`] and explicit AI thread pool size.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidAiThreads`] if `ai_threads == 0`,
    /// [`ServerError::AiPool`] if the Rayon thread pool cannot be constructed, or
    /// [`ServerError::DataDir`] if `options.data_dir` cannot be opened.
    pub fn try_with_options_and_threads(
        port: u16,
        local_hostname: Option<String>,
        options: TableOptions,
        ai_threads: usize,
    ) -> Result<Self, ServerError> {
        let ai_pool = build_ai_pool(ai_threads)?;
        let llm_client = options.llm_client().cloned();
        let tts_client = options.tts_client().cloned();
        let session = TableSession::try_with_options(port, local_hostname, options)?;
        let banter_audio_cache = Arc::clone(&session.banter_audio_cache);
        let inner = Arc::new(Mutex::new(session));
        if let Some(tts) = tts_client.clone() {
            if tokio::runtime::Handle::try_current().is_ok() {
                spawn_tts_health_probe(Arc::downgrade(&inner), tts, None);
            }
        }
        Ok(Self {
            inner,
            ai_pool,
            llm_client,
            tts_client,
            banter_audio_cache,
        })
    }

    /// Looks up cached synthesized `BanterVoice` WAV bytes by `banter_id` (touching LRU recency).
    #[must_use]
    pub fn get_banter_audio(&self, banter_id: &str) -> Option<Vec<u8>> {
        self.banter_audio_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(banter_id, tokio::time::Instant::now())
    }

    /// Allocates a unique internal connection ID for a new WebSocket connection.
    pub async fn allocate_conn_id(&self) -> u64 {
        let mut session = self.inner.lock().await;
        session.allocate_conn_id()
    }

    /// Reads `(phase, online_humans)` for `/api/health`.
    pub async fn health_snapshot(&self) -> (TablePhase, u8) {
        let session = self.inner.lock().await;
        (session.phase(), session.online_humans())
    }

    /// Subscribes to table phase updates for lifecycle monitors.
    pub async fn subscribe_phase(&self) -> watch::Receiver<TablePhase> {
        let session = self.inner.lock().await;
        session.phase_tx.subscribe()
    }

    /// Computes aggregated player stats and recent match/hand records for `GET /api/stats`.
    pub async fn history_stats(&self) -> StatsResponse {
        let session = self.inner.lock().await;
        session.history.stats()
    }

    /// Returns persisted table defaults and recent nicknames for `GET /api/preferences`.
    pub async fn history_preferences(&self) -> PreferencesData {
        let session = self.inner.lock().await;
        session.history.preferences().clone()
    }

    /// Validates and atomically updates `public_url_override` in `preferences.json`.
    ///
    /// # Errors
    ///
    /// Returns [`PublicUrlSaveError`] if validation or disk persistence fails.
    pub async fn set_public_url_override(
        &self,
        raw: Option<&str>,
    ) -> Result<Option<String>, PublicUrlSaveError> {
        let mut session = self.inner.lock().await;
        session.history.set_public_url_override(raw)
    }

    /// Exports full preferences, matches, and hands JSON backup for `GET /api/history/export`.
    pub async fn history_export(&self) -> HistoryExportResponse {
        let session = self.inner.lock().await;
        session.history.export()
    }

    /// Clears `hands.jsonl` and `matches.jsonl` while preserving `preferences.json`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if clearing persisted JSONL files fails.
    pub async fn history_clear(&self) -> Result<ClearHistoryResponse, std::io::Error> {
        let mut session = self.inner.lock().await;
        session.history.clear_history()
    }

    /// Returns the persisted `PersonaMemory` snapshot for `GET /api/persona-memory`.
    pub async fn persona_memory_snapshot(&self) -> PersonaMemoryResponse {
        let session = self.inner.lock().await;
        session.persona_memory.snapshot()
    }

    /// Clears all in-memory and on-disk `PersonaMemory` records atomically.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if clearing `persona_memory.json` fails.
    pub async fn persona_memory_clear(&self) -> Result<ClearPersonaMemoryResponse, std::io::Error> {
        let mut session = self.inner.lock().await;
        session.persona_memory.clear()
    }

    /// Parses and handles one raw JSON text frame from `conn_id`.
    pub async fn handle_raw_text(
        &self,
        conn_id: u64,
        raw_text: &str,
        trust_tier: ClientTrustTier,
        tx: &mpsc::UnboundedSender<OutboundSignal>,
    ) -> HandleOutcome {
        let msg: ClientMessage = match serde_json::from_str(raw_text) {
            Ok(parsed) => parsed,
            Err(err) => {
                TableSession::send_error(tx, format!("invalid client message: {err}"));
                return HandleOutcome::default();
            }
        };

        if let ClientMessage::ClientPing { ts_ms } = msg {
            TableSession::send_to(tx, ServerMessage::ServerPong { ts_ms });
            return HandleOutcome {
                close_socket: false,
                valid_inbound_message: true,
            };
        }

        let (outcome, scheduled_work) = {
            let mut session = self.inner.lock().await;
            let mut outcome = HandleOutcome {
                close_socket: false,
                valid_inbound_message: true,
            };
            match msg {
                ClientMessage::Hello {
                    client_id,
                    nickname,
                    table_id,
                    seat_token,
                } => {
                    session.handle_hello(
                        conn_id,
                        trust_tier,
                        tx,
                        HelloParams {
                            client_id,
                            nickname,
                            table_id,
                            seat_token,
                        },
                    );
                }
                ClientMessage::ClientLeaving => {
                    session.on_disconnect(conn_id);
                    outcome = HandleOutcome {
                        close_socket: true,
                        valid_inbound_message: true,
                    };
                }
                ClientMessage::ClientPing { .. } => unreachable!("handled above"),
                other => {
                    let Some(conn) = session.connections.get(&conn_id).cloned() else {
                        TableSession::send_error(tx, "must send Hello before other messages");
                        return outcome;
                    };

                    match other {
                        ClientMessage::JoinSeat { seat } => {
                            session.handle_join_seat(conn_id, seat);
                        }
                        ClientMessage::LeaveSeat => {
                            session.handle_leave_seat(conn_id);
                        }
                        ClientMessage::UpdateLobbyConfig {
                            mode,
                            seating,
                            ai_difficulty,
                            pace,
                        } => {
                            session.handle_update_lobby_config(
                                conn_id,
                                LobbyConfig {
                                    mode,
                                    seating,
                                    ai_difficulty,
                                    pace,
                                },
                            );
                        }
                        ClientMessage::SetSeatPersona { seat, persona_id } => {
                            session.handle_set_seat_persona(conn_id, seat, persona_id);
                        }
                        ClientMessage::SetSeatVoicePack { voice_pack_id } => {
                            session.handle_set_seat_voice_pack(conn_id, voice_pack_id);
                        }
                        ClientMessage::SetPace { pace } => {
                            session.handle_set_pace(conn_id, pace);
                        }
                        ClientMessage::SendQuickEmote { emote } => {
                            session.handle_send_quick_emote(conn_id, emote);
                        }
                        ClientMessage::SetSlowThinkNudge { enabled } => {
                            session.handle_set_slow_think_nudge(conn_id, enabled);
                        }
                        ClientMessage::RequestSnapshot => {
                            let snapshot = session.build_snapshot(&conn);
                            TableSession::send_to(&conn.tx, ServerMessage::TableSnapshot(snapshot));
                        }
                        ClientMessage::ToggleAiAssist { seat, enabled } => {
                            session.handle_toggle_ai_assist(conn_id, seat, enabled);
                        }
                        ClientMessage::TakeOverSeat { seat, confirm } => {
                            session.handle_take_over_seat(conn_id, seat, confirm);
                        }
                        ClientMessage::StartMatch => {
                            session.handle_start_match(conn_id);
                        }
                        ClientMessage::SubmitAction {
                            expected_seq,
                            client_action_id,
                            action,
                        } => {
                            session.handle_submit_action(
                                conn_id,
                                expected_seq,
                                client_action_id,
                                action,
                            );
                        }
                        ClientMessage::ReturnToLobby => {
                            session.handle_return_to_lobby(conn_id);
                        }
                        ClientMessage::RequestEndFreeTable => {
                            session.handle_request_destructive_op(
                                conn_id,
                                DestructiveOpKind::EndFreeTable,
                            );
                        }
                        ClientMessage::RequestDisbandMatch => {
                            session.handle_request_destructive_op(
                                conn_id,
                                DestructiveOpKind::DisbandMatch,
                            );
                        }
                        ClientMessage::ConfirmDestructiveOp { op_id, accept } => {
                            session.handle_confirm_destructive_op(conn_id, op_id, accept);
                        }
                        ClientMessage::Hello { .. }
                        | ClientMessage::ClientLeaving
                        | ClientMessage::ClientPing { .. } => unreachable!("handled above"),
                    }
                }
            }
            let work = session.collect_scheduled_work();
            (outcome, work)
        };

        if !scheduled_work.is_empty() {
            spawn_scheduled_work(
                Arc::clone(&self.inner),
                Arc::clone(&self.ai_pool),
                self.llm_client.clone(),
                self.tts_client.clone(),
                scheduled_work,
            );
        }

        outcome
    }

    /// Cleans up connection state when a WebSocket closes.
    pub async fn on_disconnect(&self, conn_id: u64) {
        let mut session = self.inner.lock().await;
        session.on_disconnect(conn_id);
    }
}

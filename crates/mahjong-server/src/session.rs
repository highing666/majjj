//! Single active `TableSession` state and full match WebSocket coordination.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mahjong_ai::{AiStyle, DecisionBudget, DecisionContext};
use mahjong_core::{
    AnyRuleset, Match, MatchMode, PlayerAction, PlayerEvent, PlayerView, PublicPhase,
    RecordedEvent, Seat, SeededWallProvider, TuidaohuRuleset, WallProvider,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::lan::{discover_lan_urls, read_local_hostname, read_system_interfaces};
use crate::protocol::{
    ClientMessage, DestructiveOpKind, LanUrlInfo, LobbyConfig, PendingDestructiveOp, SeatInfo,
    SeatKind, SeatingRule, ServerMessage, TablePhase, TableSnapshot,
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

/// Injectable configuration hooks for deterministic testing of walls, seating, and AI RNG.
#[derive(Clone)]
pub struct TableOptions {
    wall_factory: WallProviderFactory,
    rng_seed: Option<u64>,
}

impl std::fmt::Debug for TableOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableOptions")
            .field("rng_seed", &self.rng_seed)
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
        }
    }

    /// Creates deterministic options with a custom [`WallProviderFactory`] and explicit RNG `seed`.
    #[must_use]
    pub fn with_wall_factory(seed: u64, wall_factory: WallProviderFactory) -> Self {
        Self {
            wall_factory,
            rng_seed: Some(seed),
        }
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
}

#[derive(Debug, Clone)]
struct SeatRecord {
    info: SeatInfo,
    client_id: Option<String>,
    seat_token: Option<String>,
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
struct ClientConnection {
    client_id: String,
    seat_token: String,
    nickname: String,
    seat: Option<Seat>,
    host_is_loopback: bool,
    tx: mpsc::UnboundedSender<OutboundSignal>,
}

struct ActiveMatchState {
    match_id: u64,
    game: ActiveMatch,
    accepted_action_ids: HashSet<(Seat, String)>,
}

/// Active single-table session state.
pub struct TableSession {
    table_id: String,
    phase: TablePhase,
    config: LobbyConfig,
    seats: [SeatRecord; 4],
    seq: u64,
    pending_destructive_op: Option<PendingDestructiveOp>,
    active_match: Option<ActiveMatchState>,
    next_match_id: u64,
    in_flight_ai: HashSet<(u64, u64, Seat)>,
    options: TableOptions,
    rng: ChaCha8Rng,
    ai_base_seed: u64,
    issued_tokens: HashMap<String, String>,
    connections: HashMap<u64, ClientConnection>,
    token_to_conn: HashMap<String, u64>,
    next_conn_id: u64,
    port: u16,
    local_hostname: Option<String>,
}

impl TableSession {
    /// Creates a new `TableSession` initialized in `TablePhase::Lobby`.
    #[must_use]
    pub fn new(port: u16, local_hostname: Option<String>) -> Self {
        Self::with_options(port, local_hostname, TableOptions::default())
    }

    /// Creates a new `TableSession` with custom [`TableOptions`].
    #[must_use]
    pub fn with_options(port: u16, local_hostname: Option<String>, options: TableOptions) -> Self {
        let base_seed = options.rng_seed.unwrap_or_else(random_u64);
        Self {
            table_id: Uuid::new_v4().to_string(),
            phase: TablePhase::Lobby,
            config: LobbyConfig::default(),
            seats: Seat::ALL.map(SeatRecord::empty),
            seq: 0,
            pending_destructive_op: None,
            active_match: None,
            next_match_id: 1,
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
        }
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

    fn allocate_conn_id(&mut self) -> u64 {
        let id = self.next_conn_id;
        self.next_conn_id += 1;
        id
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

    fn build_snapshot(&self, conn: &ClientConnection) -> TableSnapshot {
        let player_view = match (self.phase, conn.seat, &self.active_match) {
            (TablePhase::InMatch | TablePhase::Finished, Some(seat), Some(active)) => {
                Some(active.game.project_view(seat))
            }
            _ => None,
        };
        TableSnapshot {
            table_id: self.table_id.clone(),
            phase: self.phase,
            config: self.config,
            pace: self.config.pace,
            seats: self.seat_infos(),
            your_seat: conn.seat,
            seat_token: Some(conn.seat_token.clone()),
            player_view,
            pending_destructive_op: self.pending_destructive_op.clone(),
            seq: self.seq,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            lan_urls: self.lan_urls(),
            host_is_loopback: conn.host_is_loopback,
        }
    }

    fn broadcast_all_snapshots(&self) {
        self.broadcast_snapshots_excluding(None);
    }

    fn broadcast_snapshots_excluding(&self, exclude_conn_id: Option<u64>) {
        for (&id, conn) in &self.connections {
            if exclude_conn_id == Some(id) {
                continue;
            }
            let snapshot = self.build_snapshot(conn);
            Self::send_to(&conn.tx, ServerMessage::TableSnapshot(snapshot));
        }
    }

    fn broadcast_lobby_updated(&self, exclude_conn_id: Option<u64>) {
        let msg = ServerMessage::LobbyUpdated {
            phase: self.phase,
            config: self.config,
            seats: self.seat_infos(),
        };
        for (&id, conn) in &self.connections {
            if exclude_conn_id == Some(id) {
                continue;
            }
            let _ = conn.tx.send(OutboundSignal::Message(Box::new(msg.clone())));
        }
    }

    fn send_to(tx: &mpsc::UnboundedSender<OutboundSignal>, msg: ServerMessage) {
        let _ = tx.send(OutboundSignal::Message(Box::new(msg)));
    }

    fn send_error(tx: &mpsc::UnboundedSender<OutboundSignal>, message: impl Into<String>) {
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
        host_is_loopback: bool,
        tx: &mpsc::UnboundedSender<OutboundSignal>,
        params: HelloParams,
    ) {
        let client_id = params.client_id.trim().to_string();
        if client_id.is_empty() {
            Self::send_error(tx, "client_id must not be empty");
            return;
        }
        let nickname = params.nickname.trim().to_string();

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
            host_is_loopback,
            tx: tx.clone(),
        };
        let snapshot = self.build_snapshot(&conn);
        self.connections.insert(conn_id, conn);
        self.token_to_conn.insert(seat_token, conn_id);

        Self::send_to(tx, ServerMessage::TableSnapshot(snapshot));
        if lobby_changed {
            self.broadcast_lobby_updated(Some(conn_id));
        }
        if pending_op_updated {
            self.broadcast_snapshots_excluding(Some(conn_id));
        }
    }

    fn handle_join_seat(&mut self, conn_id: u64, requested_seat: Option<Seat>) {
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

        if let Some(old_seat) = existing_conn.seat {
            self.seats[old_seat.index()].clear_to_empty();
        }

        let effective_nickname = if existing_conn.nickname.is_empty() {
            format!("玩家{}", target_seat.index() + 1)
        } else {
            existing_conn.nickname.clone()
        };

        let slot = &mut self.seats[target_seat.index()];
        slot.info.kind = SeatKind::Human;
        slot.info.nickname = effective_nickname;
        slot.info.connected = true;
        slot.info.ai_assisted = false;
        slot.info.ai_difficulty = None;
        slot.info.ai_style = None;
        slot.info.voice_pack_id = None;
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

    fn handle_update_lobby_config(&mut self, conn_id: u64, next_config: LobbyConfig) {
        let Some(conn) = self.connections.get(&conn_id) else {
            return;
        };
        if conn.seat.is_none() {
            Self::send_error(&conn.tx, "must join a seat before updating lobby config");
            return;
        }
        if self.phase == TablePhase::InMatch {
            Self::send_error(
                &conn.tx,
                "cannot update lobby config while a match is in progress",
            );
            return;
        }
        self.config = next_config;
        self.broadcast_lobby_updated(None);
    }

    fn handle_set_pace(&mut self, conn_id: u64, pace: crate::protocol::TablePace) {
        let Some(conn) = self.connections.get(&conn_id) else {
            return;
        };
        if conn.seat.is_none() {
            Self::send_error(&conn.tx, "must join a seat before changing table pace");
            return;
        }
        self.config.pace = pace;
        self.broadcast_lobby_updated(None);
    }

    fn handle_start_match(&mut self, conn_id: u64) {
        let Some(conn_tx) = self.connections.get(&conn_id).map(|c| c.tx.clone()) else {
            return;
        };
        let Some(caller_seat) = self.connections.get(&conn_id).and_then(|c| c.seat) else {
            Self::send_error(&conn_tx, "must join a seat before starting a match");
            return;
        };
        if self.phase == TablePhase::InMatch {
            Self::send_error(&conn_tx, "match is already in progress");
            return;
        }
        if self.seats[caller_seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn_tx, "only a seated human can start a match");
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

        let seat_order = match self.config.seating {
            SeatingRule::Random => {
                let mut order = Seat::ALL;
                shuffle_slice(&mut order, &mut self.rng);
                order
            }
            SeatingRule::FixedOpposite => [Seat::East, Seat::West, Seat::South, Seat::North],
            SeatingRule::FixedAdjacent => [Seat::East, Seat::South, Seat::West, Seat::North],
        };

        let mut styles = [
            AiStyle::AggressiveFast,
            AiStyle::Balanced,
            AiStyle::DefensiveBigHand,
        ];
        shuffle_slice(&mut styles, &mut self.rng);

        let mut new_seats = Seat::ALL.map(SeatRecord::empty);
        let human_count = humans.len();
        for (idx, mut human_record) in humans.into_iter().enumerate() {
            let target_seat = seat_order[idx];
            human_record.info.seat = target_seat;
            new_seats[target_seat.index()] = human_record;
        }
        for (ai_idx, &target_seat) in seat_order[human_count..].iter().enumerate() {
            let style = styles[ai_idx % styles.len()];
            new_seats[target_seat.index()] = SeatRecord {
                info: SeatInfo {
                    seat: target_seat,
                    kind: SeatKind::Ai,
                    nickname: format!("AI {}", ai_idx + 1),
                    connected: true,
                    ai_assisted: false,
                    ai_difficulty: Some(self.config.ai_difficulty),
                    ai_style: Some(style),
                    voice_pack_id: None,
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
        self.seq = game.seq();
        self.phase = TablePhase::InMatch;
        self.pending_destructive_op = None;
        self.in_flight_ai.clear();
        self.active_match = Some(ActiveMatchState {
            match_id,
            game,
            accepted_action_ids: HashSet::new(),
        });

        self.broadcast_all_snapshots();
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

    fn handle_submit_action(
        &mut self,
        conn_id: u64,
        expected_seq: u64,
        client_action_id: String,
        action: PlayerAction,
    ) {
        let Some(conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        if self.phase != TablePhase::InMatch {
            Self::send_error(&conn.tx, "can only submit actions during an active match");
            return;
        }
        let Some(seat) = conn.seat else {
            Self::send_error(&conn.tx, "must join a seat before submitting actions");
            return;
        };
        if self.seats[seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn.tx, "seat is not controlled by a human");
            return;
        }
        if client_action_id.trim().is_empty() {
            let current_seq = self
                .active_match
                .as_ref()
                .map_or(self.seq, |m| m.game.seq());
            Self::send_action_rejected(&conn.tx, "client_action_id must not be empty", current_seq);
            return;
        }

        let Some(active) = self.active_match.as_ref() else {
            Self::send_error(&conn.tx, "no active match");
            return;
        };
        if active
            .accepted_action_ids
            .contains(&(seat, client_action_id.clone()))
        {
            return;
        }

        let current_seq = active.game.seq();
        if expected_seq != current_seq {
            Self::send_action_rejected(
                &conn.tx,
                format!("expected seq {expected_seq}, but current seq is {current_seq}"),
                current_seq,
            );
            return;
        }

        if matches!(action, PlayerAction::EndMatch) && self.online_humans() >= 2 {
            Self::send_action_rejected(
                &conn.tx,
                "cannot end FreeTable directly while multiple humans are online; use RequestEndFreeTable",
                current_seq,
            );
            return;
        }

        self.apply_match_step(seat, action, Some((conn.tx, client_action_id)));
    }

    fn apply_match_step(
        &mut self,
        seat: Seat,
        action: PlayerAction,
        caller: Option<(mpsc::UnboundedSender<OutboundSignal>, String)>,
    ) {
        let Some(active) = self.active_match.as_mut() else {
            return;
        };
        let prev_seq = active.game.seq();
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

        let match_id = active.match_id;
        let mut public_phase = active.game.project_view(Seat::East).phase;
        if public_phase == PublicPhase::BetweenHands {
            self.auto_confirm_ai_between_hands(&mut emitted);
            if let Some(active_after) = self.active_match.as_ref() {
                public_phase = active_after.game.project_view(Seat::East).phase;
            }
        }

        let Some(active_ref) = self.active_match.as_ref() else {
            return;
        };
        let new_seq = active_ref.game.seq();
        self.seq = new_seq;
        self.in_flight_ai
            .retain(|&(m_id, seq, _)| m_id == match_id && seq == new_seq);

        self.broadcast_event_batch(&active_ref.game, &emitted);

        match public_phase {
            PublicPhase::MatchFinished => {
                self.phase = TablePhase::Finished;
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

    fn ai_decision_context(&self, seat: Seat) -> DecisionContext {
        let slot = &self.seats[seat.index()];
        DecisionContext {
            difficulty: slot.info.ai_difficulty.unwrap_or(self.config.ai_difficulty),
            style: slot.info.ai_style.unwrap_or(AiStyle::Balanced),
            budget: DecisionBudget::default(),
        }
    }

    fn auto_confirm_ai_between_hands(&mut self, emitted: &mut Vec<RecordedEvent>) {
        let Some(active) = self.active_match.as_mut() else {
            return;
        };
        let match_id = active.match_id;
        for seat in Seat::ALL {
            if self.seats[seat.index()].info.kind != SeatKind::Ai {
                continue;
            }
            let view = active.game.project_view(seat);
            if !view.legal_actions.contains(&PlayerAction::ConfirmNextHand) {
                continue;
            }
            let ctx = DecisionContext {
                difficulty: self.seats[seat.index()]
                    .info
                    .ai_difficulty
                    .unwrap_or(self.config.ai_difficulty),
                style: self.seats[seat.index()]
                    .info
                    .ai_style
                    .unwrap_or(AiStyle::Balanced),
                budget: DecisionBudget::default(),
            };
            let step_seed = derive_ai_step_seed(self.ai_base_seed, match_id, view.seq, seat);
            let mut rng = ChaCha8Rng::seed_from_u64(step_seed);
            let decided = mahjong_ai::decide(&view, &ctx, &mut rng);
            let action = if decided == PlayerAction::EndMatch {
                PlayerAction::ConfirmNextHand
            } else {
                decided
            };
            if let Ok(mut more) = active.game.apply_action(seat, action) {
                emitted.append(&mut more);
            }
        }
    }

    fn collect_pending_ai_jobs(&mut self) -> Vec<PendingAiJob> {
        if self.phase != TablePhase::InMatch {
            return Vec::new();
        }
        let Some(active) = self.active_match.as_ref() else {
            return Vec::new();
        };
        let match_id = active.match_id;
        let seq = active.game.seq();
        if active.game.project_view(Seat::East).phase != PublicPhase::Playing {
            return Vec::new();
        }

        let mut jobs = Vec::new();
        for seat in Seat::ALL {
            if self.seats[seat.index()].info.kind != SeatKind::Ai {
                continue;
            }
            let view = active.game.project_view(seat);
            if view.legal_actions.is_empty() {
                continue;
            }
            if !self.in_flight_ai.insert((match_id, seq, seat)) {
                continue;
            }
            jobs.push(PendingAiJob {
                match_id,
                seq,
                seat,
                view,
                ctx: self.ai_decision_context(seat),
                step_seed: derive_ai_step_seed(self.ai_base_seed, match_id, seq, seat),
            });
        }
        jobs
    }

    fn complete_ai_job(
        &mut self,
        match_id: u64,
        expected_seq: u64,
        seat: Seat,
        action: PlayerAction,
    ) -> Vec<PendingAiJob> {
        self.in_flight_ai.remove(&(match_id, expected_seq, seat));
        if self.phase != TablePhase::InMatch {
            return Vec::new();
        }
        if self.seats[seat.index()].info.kind != SeatKind::Ai {
            return Vec::new();
        }
        let matches_state = self
            .active_match
            .as_ref()
            .is_some_and(|active| active.match_id == match_id && active.game.seq() == expected_seq);
        if !matches_state {
            return Vec::new();
        }

        self.apply_match_step(seat, action, None);
        self.collect_pending_ai_jobs()
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
        self.phase = TablePhase::Lobby;
        self.active_match = None;
        self.in_flight_ai.clear();
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
        let Some(conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        if self.phase != TablePhase::Finished {
            Self::send_error(&conn.tx, "can only return to lobby when match is Finished");
            return;
        }
        let Some(seat) = conn.seat else {
            Self::send_error(&conn.tx, "must be seated to return to lobby");
            return;
        };
        if self.seats[seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn.tx, "only a seated human can return to lobby");
            return;
        }
        self.reset_match_to_lobby();
    }

    fn is_end_free_table_allowed(&self) -> bool {
        self.phase == TablePhase::InMatch
            && self.config.mode == MatchMode::FreeTable
            && self
                .active_match
                .as_ref()
                .is_some_and(|a| a.game.project_view(Seat::East).phase == PublicPhase::BetweenHands)
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

    fn handle_request_destructive_op(&mut self, conn_id: u64, kind: DestructiveOpKind) {
        let Some(conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        let Some(seat) = conn.seat else {
            Self::send_error(&conn.tx, "must be seated to request this operation");
            return;
        };
        if self.seats[seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn.tx, "only a seated human can request this operation");
            return;
        }
        if self.phase != TablePhase::InMatch {
            Self::send_error(&conn.tx, "destructive table request is only valid InMatch");
            return;
        }
        if kind == DestructiveOpKind::EndFreeTable && !self.is_end_free_table_allowed() {
            Self::send_error(
                &conn.tx,
                "RequestEndFreeTable is only valid during BetweenHands in FreeTable mode",
            );
            return;
        }
        if self.pending_destructive_op.is_some() {
            Self::send_error(&conn.tx, "another confirmation request is already pending");
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
        let Some(conn) = self.connections.get(&conn_id).cloned() else {
            return;
        };
        let Some(seat) = conn.seat else {
            Self::send_error(&conn.tx, "must be seated to confirm this operation");
            return;
        };
        if self.seats[seat.index()].info.kind != SeatKind::Human {
            Self::send_error(&conn.tx, "only a seated human can confirm this operation");
            return;
        }
        let Some(mut op) = self.pending_destructive_op.take() else {
            Self::send_error(&conn.tx, "no pending destructive operation");
            return;
        };
        if op.op_id != op_id {
            self.pending_destructive_op = Some(op);
            Self::send_error(&conn.tx, "destructive operation id does not match");
            return;
        }
        if !op.required_seats.contains(&seat) {
            self.pending_destructive_op = Some(op);
            Self::send_error(&conn.tx, "seat is not part of required confirmations");
            return;
        }

        if !accept {
            self.broadcast_all_snapshots();
            return;
        }

        if !op.confirmed_seats.contains(&seat) {
            op.confirmed_seats.push(seat);
        }
        if op
            .required_seats
            .iter()
            .all(|req| op.confirmed_seats.contains(req))
        {
            self.execute_destructive_op(op.kind, op.requested_by);
        } else {
            self.pending_destructive_op = Some(op);
            self.broadcast_all_snapshots();
        }
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
        if op
            .required_seats
            .iter()
            .all(|req| op.confirmed_seats.contains(req))
        {
            let actor = op
                .confirmed_seats
                .first()
                .copied()
                .unwrap_or(op.requested_by);
            self.execute_destructive_op(op.kind, actor);
        } else {
            self.pending_destructive_op = Some(op);
            self.broadcast_all_snapshots();
        }
    }

    fn on_disconnect(&mut self, conn_id: u64) {
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
                    self.broadcast_lobby_updated(None);
                    self.reconcile_pending_destructive_op_on_disconnect();
                }
            }
        }
    }
}

struct HelloParams {
    client_id: String,
    nickname: String,
    table_id: Option<String>,
    seat_token: Option<String>,
}

struct PendingAiJob {
    match_id: u64,
    seq: u64,
    seat: Seat,
    view: PlayerView,
    ctx: DecisionContext,
    step_seed: u64,
}

fn spawn_ai_jobs(runtime: Arc<Mutex<TableSession>>, jobs: Vec<PendingAiJob>) {
    for job in jobs {
        let runtime = Arc::clone(&runtime);
        tokio::spawn(async move {
            let PendingAiJob {
                match_id,
                seq,
                seat,
                view,
                ctx,
                step_seed,
            } = job;
            let Ok(action) = tokio::task::spawn_blocking(move || {
                let mut rng = ChaCha8Rng::seed_from_u64(step_seed);
                mahjong_ai::decide(&view, &ctx, &mut rng)
            })
            .await
            else {
                let mut session = runtime.lock().await;
                session.in_flight_ai.remove(&(match_id, seq, seat));
                return;
            };

            let next_jobs = {
                let mut session = runtime.lock().await;
                session.complete_ai_job(match_id, seq, seat, action)
            };
            if !next_jobs.is_empty() {
                spawn_ai_jobs(runtime, next_jobs);
            }
        });
    }
}

/// Thread-safe handle around the single active [`TableSession`].
#[derive(Clone)]
pub struct TableRuntime {
    inner: Arc<Mutex<TableSession>>,
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

    /// Creates a fresh table runtime with custom [`TableOptions`].
    #[must_use]
    pub fn with_options(port: u16, local_hostname: Option<String>, options: TableOptions) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TableSession::with_options(
                port,
                local_hostname,
                options,
            ))),
        }
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

    /// Parses and handles one raw JSON text frame from `conn_id`.
    pub async fn handle_raw_text(
        &self,
        conn_id: u64,
        raw_text: &str,
        host_is_loopback: bool,
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
            return HandleOutcome::default();
        }

        let (outcome, ai_jobs) = {
            let mut session = self.inner.lock().await;
            let mut outcome = HandleOutcome::default();
            match msg {
                ClientMessage::Hello {
                    client_id,
                    nickname,
                    table_id,
                    seat_token,
                } => {
                    session.handle_hello(
                        conn_id,
                        host_is_loopback,
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
                    outcome = HandleOutcome { close_socket: true };
                }
                ClientMessage::ClientPing { .. } => unreachable!("handled above"),
                other => {
                    let Some(conn) = session.connections.get(&conn_id).cloned() else {
                        TableSession::send_error(tx, "must send Hello before other messages");
                        return HandleOutcome::default();
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
                        ClientMessage::SetPace { pace } => {
                            session.handle_set_pace(conn_id, pace);
                        }
                        ClientMessage::RequestSnapshot => {
                            let snapshot = session.build_snapshot(&conn);
                            TableSession::send_to(&conn.tx, ServerMessage::TableSnapshot(snapshot));
                        }
                        ClientMessage::ToggleAiAssist { .. } => {
                            TableSession::send_error(
                                &conn.tx,
                                "ToggleAiAssist is reserved for Slice 5 and not yet enabled",
                            );
                        }
                        ClientMessage::TakeOverSeat { .. } => {
                            TableSession::send_error(
                                &conn.tx,
                                "TakeOverSeat is reserved for Slice 5 and not yet enabled",
                            );
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
            let jobs = session.collect_pending_ai_jobs();
            (outcome, jobs)
        };

        if !ai_jobs.is_empty() {
            spawn_ai_jobs(Arc::clone(&self.inner), ai_jobs);
        }

        outcome
    }

    /// Cleans up connection state when a WebSocket closes.
    pub async fn on_disconnect(&self, conn_id: u64) {
        let mut session = self.inner.lock().await;
        session.on_disconnect(conn_id);
    }
}

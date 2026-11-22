//! Single active `TableSession` state and Lobby WebSocket coordination.

use std::collections::HashMap;
use std::sync::Arc;

use mahjong_core::{Match, MatchError, Ruleset, Seat, TuidaohuRuleset, WallProvider};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::lan::{discover_lan_urls, read_local_hostname, read_system_interfaces};
use crate::protocol::{
    ClientMessage, LanUrlInfo, LobbyConfig, PendingDestructiveOp, SeatInfo, SeatKind,
    ServerMessage, TablePhase, TableSnapshot,
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

/// Runtime wrapper keeping type compatibility with `Match` for Slice 2 match driving.
#[allow(dead_code)]
pub(crate) type ActiveMatch = Match<TuidaohuRuleset, BoxedWallProvider>;

/// Verifies `Ruleset` trait compatibility across crates.
#[allow(dead_code)]
pub(crate) fn _check_ruleset_bound<R: Ruleset>(_: &R) {}

#[allow(dead_code)]
fn _unused_match_error(_: MatchError) {}

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

/// Active single-table session state.
pub struct TableSession {
    table_id: String,
    phase: TablePhase,
    config: LobbyConfig,
    seats: [SeatRecord; 4],
    seq: u64,
    pending_destructive_op: Option<PendingDestructiveOp>,
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
        Self {
            table_id: Uuid::new_v4().to_string(),
            phase: TablePhase::Lobby,
            config: LobbyConfig::default(),
            seats: Seat::ALL.map(SeatRecord::empty),
            seq: 0,
            pending_destructive_op: None,
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
        TableSnapshot {
            table_id: self.table_id.clone(),
            phase: self.phase,
            config: self.config,
            pace: self.config.pace,
            seats: self.seat_infos(),
            your_seat: conn.seat,
            seat_token: Some(conn.seat_token.clone()),
            player_view: None,
            pending_destructive_op: self.pending_destructive_op.clone(),
            seq: self.seq,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            lan_urls: self.lan_urls(),
            host_is_loopback: conn.host_is_loopback,
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
        Self {
            inner: Arc::new(Mutex::new(TableSession::new(port, local_hostname))),
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

        let mut session = self.inner.lock().await;
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
                return HandleOutcome { close_socket: true };
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
                    ClientMessage::StartMatch
                    | ClientMessage::ReturnToLobby
                    | ClientMessage::SubmitAction { .. }
                    | ClientMessage::RequestEndFreeTable
                    | ClientMessage::RequestDisbandMatch
                    | ClientMessage::ConfirmDestructiveOp { .. } => {
                        TableSession::send_error(
                            &conn.tx,
                            "match driver is not enabled in lobby-only mode yet",
                        );
                    }
                    ClientMessage::Hello { .. }
                    | ClientMessage::ClientLeaving
                    | ClientMessage::ClientPing { .. } => unreachable!("handled above"),
                }
            }
        }

        HandleOutcome::default()
    }

    /// Cleans up connection state when a WebSocket closes.
    pub async fn on_disconnect(&self, conn_id: u64) {
        let mut session = self.inner.lock().await;
        session.on_disconnect(conn_id);
    }
}

//! Frozen v1 WebSocket and HTTP JSON wire protocol types exported via `ts-rs`.

use mahjong_ai::{AiDifficulty, AiStyle};
use mahjong_core::{MatchMode, PlayerAction, PlayerEvent, PlayerView, Seat};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::{PersonaId, PersonaMood};

/// Top-level lifecycle phase of a single active `TableSession`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum TablePhase {
    /// Waiting for players to seat and configure the match before starting.
    #[default]
    Lobby,
    /// Match is currently underway (including `BetweenHands`).
    InMatch,
    /// Match has completed and final summary is displayed.
    Finished,
}

/// Table-wide presentation and AI pacing tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum TablePace {
    /// Standard comfortable pacing.
    #[default]
    Standard,
    /// Accelerated pacing (~0.6x).
    Fast,
}

/// Seating assignment rule applied when starting a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum SeatingRule {
    /// Keep each human player in their chosen lobby seat and fill empty seats with AI.
    #[default]
    KeepLobbySeats,
    /// Randomize all four seats at match start.
    Random,
    /// Keep two human players seated across from each other (`East` / `West`).
    FixedOpposite,
    /// Keep two human players seated adjacent to each other (`East` / `South`).
    FixedAdjacent,
}

/// Shared lobby configuration synchronized across all clients at the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct LobbyConfig {
    /// Match progression mode (`EastOnly` or `FreeTable`).
    pub mode: MatchMode,
    /// Seating assignment rule on match start.
    pub seating: SeatingRule,
    /// Difficulty tier for bot-filled seats.
    pub ai_difficulty: AiDifficulty,
    /// Table-wide animation and AI pacing tier.
    pub pace: TablePace,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        }
    }
}

/// Occupant category for one of the four seats at the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum SeatKind {
    /// Controlled by a human player.
    Human,
    /// Controlled by an AI bot.
    Ai,
    /// Vacant lobby seat.
    #[default]
    Empty,
}

/// Public seat metadata shared in `TableSnapshot` and `LobbyUpdated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SeatInfo {
    /// Canonical seat identifier.
    pub seat: Seat,
    /// Seat occupant category.
    pub kind: SeatKind,
    /// Display nickname (empty string when `SeatKind::Empty`).
    pub nickname: String,
    /// Whether this seat currently has an active connection (always `true` for `Ai`, `false` for `Empty`).
    pub connected: bool,
    /// Whether this human seat is temporarily assisted by AI (Slice 5).
    pub ai_assisted: bool,
    /// AI difficulty if occupied by an AI or AI-assisted seat.
    pub ai_difficulty: Option<AiDifficulty>,
    /// AI play style if occupied by an AI or AI-assisted seat.
    pub ai_style: Option<AiStyle>,
    /// Assigned table persona (`牌友角色`) for an AI or lobby-configured empty seat.
    pub persona_id: Option<PersonaId>,
    /// Current facial/emotional mood (`牌友情绪`) for an active AI seat.
    pub mood: Option<PersonaMood>,
    /// Reserved v2 voice pack identifier placeholder (`None` in v1).
    pub voice_pack_id: Option<String>,
}

/// Discovered LAN URLs presented in the lobby for QR scanning and partner join.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct LanUrlInfo {
    /// Highest-priority LAN IPv4 HTTP URL (`192.168.x.x > 10.x.x.x > 172.16-31.x.x`).
    pub preferred_ipv4_url: Option<String>,
    /// All eligible private LAN IPv4 HTTP URLs ordered by priority.
    pub ipv4_urls: Vec<String>,
    /// Native Bonjour/mDNS `<LocalHostName>.local:<port>` URL if available.
    pub local_hostname_url: Option<String>,
}

/// Classification of a destructive table lifecycle request requiring multi-human confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DestructiveOpKind {
    /// End a `FreeTable` match during `BetweenHands`.
    EndFreeTable,
    /// Disband an in-progress match and return the table to `Lobby`.
    DisbandMatch,
}

/// Pending multi-human confirmation state for ending a free table or disbanding a match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PendingDestructiveOp {
    /// Unique operation identifier for matching `ConfirmDestructiveOp`.
    pub op_id: String,
    /// Destructive action kind.
    pub kind: DestructiveOpKind,
    /// Seat that initiated the request.
    pub requested_by: Seat,
    /// Online human seats whose confirmation is required.
    pub required_seats: Vec<Seat>,
    /// Seats that have already confirmed.
    pub confirmed_seats: Vec<Seat>,
}

/// Response payload for `GET /api/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HealthResponse {
    /// Application identifier (`"majjj"`).
    pub app: String,
    /// Crate/server semantic version.
    pub version: String,
    /// Current lifecycle phase of the active table session.
    pub phase: TablePhase,
    /// Number of currently connected human seats.
    pub online_humans: u8,
    /// Bound listening port.
    pub port: u16,
}

/// Six fixed table-wide human quick emotes (`快捷互动`) triggering immediate AI banter replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum QuickEmote {
    /// 「就这？」
    IsThatIt,
    /// 「搞快点」
    HurryUp,
    /// 「别点我 / 求放过」
    SpareMe,
    /// 「666 / 漂亮」
    NicePlay,
    /// 「发牌员演我」
    DealerRigged,
    /// 「这把我要胡大的」
    GoingBig,
}

impl QuickEmote {
    /// All six quick emotes in canonical UI display order.
    pub const ALL: [Self; 6] = [
        Self::IsThatIt,
        Self::HurryUp,
        Self::SpareMe,
        Self::NicePlay,
        Self::DealerRigged,
        Self::GoingBig,
    ];

    /// Authoritative Chinese bubble label broadcast to the table.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IsThatIt => "就这？",
            Self::HurryUp => "搞快点",
            Self::SpareMe => "别点我 / 求放过",
            Self::NicePlay => "666 / 漂亮",
            Self::DealerRigged => "发牌员演我",
            Self::GoingBig => "这把我要胡大的",
        }
    }

    /// Zero-based index for deterministic speaker rotation.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::IsThatIt => 0,
            Self::HurryUp => 1,
            Self::SpareMe => 2,
            Self::NicePlay => 3,
            Self::DealerRigged => 4,
            Self::GoingBig => 5,
        }
    }
}

/// Full authoritative seat-projected snapshot sent on connect, reconnect, and `RequestSnapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TableSnapshot {
    /// Identifier of the current active table session.
    pub table_id: String,
    /// Current lifecycle phase.
    pub phase: TablePhase,
    /// Shared lobby configuration.
    pub config: LobbyConfig,
    /// Active table pace.
    pub pace: TablePace,
    /// Whether AI personas gently nudge slow-thinking humans after 15–20 seconds.
    pub slow_think_nudge_enabled: bool,
    /// Public status for all four seats in `[East, South, West, North]` order.
    pub seats: [SeatInfo; 4],
    /// Seat owned by this connection, or `None` if unseated.
    pub your_seat: Option<Seat>,
    /// Authoritative session credential token for reconnect / duplicate-tab detection.
    pub seat_token: Option<String>,
    /// Seat-projected `mahjong-core` view when `phase` is `InMatch` or `Finished` and seated.
    pub player_view: Option<PlayerView>,
    /// Pending confirmation request when multiple online humans must agree.
    pub pending_destructive_op: Option<PendingDestructiveOp>,
    /// Current monotonic table event sequence number (`0` in `Lobby`).
    #[ts(type = "number")]
    pub seq: u64,
    /// Server semantic version string.
    pub server_version: String,
    /// Discovered LAN access URLs.
    pub lan_urls: LanUrlInfo,
    /// Whether this client connected via loopback (`127.0.0.1` / `::1`).
    pub host_is_loopback: bool,
}

/// Client-to-server WebSocket JSON messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ClientMessage {
    /// Initial handshake / credential issuance or reconnect restore.
    Hello {
        client_id: String,
        nickname: String,
        table_id: Option<String>,
        seat_token: Option<String>,
    },
    /// Claim an empty seat in the lobby (`None` chooses the first available empty seat).
    JoinSeat { seat: Option<Seat> },
    /// Vacate the client's current seat in the lobby.
    LeaveSeat,
    /// Peer lobby configuration update (any seated human in `Lobby` or `Finished`).
    UpdateLobbyConfig {
        mode: MatchMode,
        seating: SeatingRule,
        ai_difficulty: AiDifficulty,
        pace: TablePace,
    },
    /// Assign or clear (`None` = random backfill) a table persona for a non-human seat.
    SetSeatPersona {
        seat: Seat,
        persona_id: Option<PersonaId>,
    },
    /// Start a match from `Lobby` or rematch from `Finished`.
    StartMatch,
    /// Return from `Finished` back to `Lobby`, resetting AI seats to `Empty`.
    ReturnToLobby,
    /// Submit an authoritative gameplay action with optimistic sequence check and idempotency key.
    SubmitAction {
        #[ts(type = "number")]
        expected_seq: u64,
        client_action_id: String,
        action: PlayerAction,
    },
    /// Update table pacing tier.
    SetPace { pace: TablePace },
    /// Send a one-click table-wide quick emote (`快捷互动`) during an active match.
    SendQuickEmote { emote: QuickEmote },
    /// Toggle persistent gentle AI nudge banter after 15–20 seconds of human thinking.
    SetSlowThinkNudge { enabled: bool },
    /// Reserved Slice 5 message: toggle temporary AI assistance for a seat.
    ToggleAiAssist { seat: Seat, enabled: bool },
    /// Reserved Slice 5 message: take over an offline human or AI seat.
    TakeOverSeat { seat: Seat, confirm: bool },
    /// Request ending a `FreeTable` match during `BetweenHands`.
    RequestEndFreeTable,
    /// Request disbanding an in-progress match and returning to `Lobby`.
    RequestDisbandMatch,
    /// Accept or reject a pending destructive operation.
    ConfirmDestructiveOp { op_id: String, accept: bool },
    /// Request a fresh authoritative `TableSnapshot` (e.g., on `seq` gap).
    RequestSnapshot,
    /// Client-initiated liveness ping (`visibilitychange` / `online`).
    ClientPing {
        #[ts(type = "number")]
        ts_ms: u64,
    },
    /// Explicit notice emitted on `pagehide` before disconnecting.
    ClientLeaving,
}

/// Server-to-client WebSocket JSON messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
// Keeps `TableSnapshot(TableSnapshot)` unboxed to preserve the wire protocol shape.
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    /// Full authoritative table snapshot.
    TableSnapshot(TableSnapshot),
    /// Ordered incremental events plus the viewer's updated legal actions.
    EventBatch {
        #[ts(type = "number")]
        from_seq: u64,
        #[ts(type = "number")]
        to_seq: u64,
        events: Vec<PlayerEvent>,
        updated_legal_actions: Vec<PlayerAction>,
    },
    /// Broadcast lobby state update after seat or configuration changes.
    LobbyUpdated {
        phase: TablePhase,
        config: LobbyConfig,
        seats: [SeatInfo; 4],
        slow_think_nudge_enabled: bool,
    },
    /// Action rejection carrying the server's latest sequence number.
    ActionRejected {
        reason: String,
        #[ts(type = "number")]
        current_seq: u64,
    },
    /// Server-initiated heartbeat ping.
    ServerPing {
        #[ts(type = "number")]
        ts_ms: u64,
    },
    /// Reply to `ClientPing`.
    ServerPong {
        #[ts(type = "number")]
        ts_ms: u64,
    },
    /// Sent to an older connection when a newer tab replaces it with the same credential.
    SessionReplaced,
    /// Non-fatal protocol or validation notice sent to the requesting client.
    ErrorNotice { message: String },
    /// Comic speech bubble banter line emitted by an AI table persona (`牌桌台词`).
    TableBanter {
        seat: Seat,
        persona_id: PersonaId,
        mood: PersonaMood,
        text: String,
        #[ts(type = "number")]
        event_seq: u64,
        reply_to_seat: Option<Seat>,
    },
    /// Broadcast human quick-emote comic bubble (`快捷互动`) to all connected clients.
    QuickEmoteBroadcast {
        seat: Seat,
        emote: QuickEmote,
        text: String,
    },
    /// Status notice emitted when the mandatory LAN LLM service disconnects, times out, or recovers.
    LlmStatusNotice { available: bool, message: String },
}

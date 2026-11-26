//! HTTP and JSONL persistence DTOs for match/hand history and preferences (`ts-rs` exported).

use mahjong_ai::AiDifficulty;
use mahjong_core::{HandSettlement, MatchMode, Seat};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::protocol::{LobbyConfig, SeatKind, SeatingRule, TablePace};

/// Current schema version for JSONL history records, preferences, and HTTP exports.
pub const HISTORY_SCHEMA_VERSION: u32 = 1;

/// Persisted table defaults and recently used nicknames (`preferences.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PreferencesData {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Recently used normalized player nicknames, most recent first.
    pub recent_nicknames: Vec<String>,
    /// Default match mode (`EastOnly` or `FreeTable`).
    pub default_mode: MatchMode,
    /// Default seating rule on match start.
    pub default_seating: SeatingRule,
    /// Default AI difficulty for bot seats and temporary AI assist.
    pub default_ai_difficulty: AiDifficulty,
    /// Default table-wide animation and AI pacing tier.
    pub default_pace: TablePace,
    /// Optional persisted base URL for the LAN `llama-server` endpoint.
    #[serde(default)]
    pub llm_base_url: Option<String>,
}

impl Default for PreferencesData {
    fn default() -> Self {
        let lobby = LobbyConfig::default();
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            recent_nicknames: Vec::new(),
            default_mode: lobby.mode,
            default_seating: lobby.seating,
            default_ai_difficulty: lobby.ai_difficulty,
            default_pace: lobby.pace,
            llm_base_url: None,
        }
    }
}

impl From<&PreferencesData> for LobbyConfig {
    fn from(prefs: &PreferencesData) -> Self {
        Self {
            mode: prefs.default_mode,
            seating: prefs.default_seating,
            ai_difficulty: prefs.default_ai_difficulty,
            pace: prefs.default_pace,
        }
    }
}

/// Per-seat attribution snapshot recorded inside each settled hand row (`hands.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HandSeatRecord {
    /// Canonical seat identifier.
    pub seat: Seat,
    /// Trimmed player/AI nickname at hand settlement.
    pub nickname: String,
    /// Seat category (`Human` or `Ai`) at hand settlement.
    pub seat_kind: SeatKind,
    /// `true` if temporary AI assistance was enabled at any point during this hand.
    pub assisted_by_ai: bool,
    /// `true` if a human took over this seat from AI during this hand.
    pub took_over_ai: bool,
}

/// Persistent JSONL row written when a hand settles (`hands.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HandRecord {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Globally unique match identifier.
    pub match_id: String,
    /// 1-based settled hand index within the match.
    pub hand_index: u32,
    /// Unix epoch timestamp in milliseconds when settled.
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Match mode (`EastOnly` or `FreeTable`).
    pub mode: MatchMode,
    /// Authoritative hand settlement including fan items, kong transfers, and score deltas.
    pub settlement: HandSettlement,
    /// Per-seat attribution in `[East, South, West, North]` order.
    pub seats: [HandSeatRecord; 4],
    /// `true` if this match was later disbanded before normal completion.
    pub aborted: bool,
}

/// Per-seat summary recorded inside each completed match row (`matches.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MatchSeatRecord {
    /// Canonical seat identifier.
    pub seat: Seat,
    /// Trimmed player/AI nickname at match completion.
    pub nickname: String,
    /// Seat category (`Human` or `Ai`) at match completion.
    pub seat_kind: SeatKind,
    /// `true` if temporary AI assistance was used by this seat during the match.
    pub assisted_by_ai: bool,
    /// Final score at match completion.
    pub final_score: i32,
    /// Net score delta across the match.
    pub net_delta: i32,
}

/// Persistent JSONL row written when a match finishes cleanly (`matches.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MatchRecord {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Globally unique match identifier matching `hands.jsonl`.
    pub match_id: String,
    /// Unix epoch timestamp in milliseconds when the match finished.
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Match mode (`EastOnly` or `FreeTable`).
    pub mode: MatchMode,
    /// Total settled hands played in this match.
    pub hands_played: u32,
    /// Starting scores for `[East, South, West, North]`.
    pub initial_scores: [i32; 4],
    /// Final scores for `[East, South, West, North]`.
    pub final_scores: [i32; 4],
    /// Net score deltas for `[East, South, West, North]`.
    pub net_deltas: [i32; 4],
    /// Per-seat attribution and scores in `[East, South, West, North]` order.
    pub seats: [MatchSeatRecord; 4],
}

/// Aggregated lifetime statistics for one normalized human nickname.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PlayerStats {
    /// Trimmed player nickname.
    pub nickname: String,
    /// Count of completed matches (excludes disbanded matches and 0-hand free tables).
    pub completed_matches: u32,
    /// Count of settled hands played as a human seat.
    pub settled_hands: u32,
    /// Subset of `settled_hands` belonging to disbanded matches.
    pub aborted_hands: u32,
    /// Total winning hands (`Tsumo`, `Ron`, or `RobKongHu`).
    pub wins: u32,
    /// Self-drawn wins (`自摸`).
    pub tsumo_wins: u32,
    /// Deal-ins (`点炮`, including `RobKongHu`).
    pub deal_ins: u32,
    /// Settled hands where temporary AI assist was active at any point.
    pub ai_assisted_hands: u32,
    /// Cumulative net score delta across settled hands.
    #[ts(type = "number")]
    pub cumulative_net_score: i64,
    /// Average net score per completed match (`0.0` when `completed_matches == 0`).
    pub average_net_score: f64,
    /// Average net score per settled hand (`0.0` when `settled_hands == 0`).
    pub average_hand_net_score: f64,
}

/// Response payload for `GET /api/stats`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatsResponse {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Total number of completed matches on disk.
    pub total_matches: u32,
    /// Total number of settled hands on disk.
    pub total_hands: u32,
    /// Per-normalized-nickname statistics for human players.
    pub players: Vec<PlayerStats>,
    /// Most recent completed matches, newest first.
    pub recent_matches: Vec<MatchRecord>,
    /// Most recent settled hands, newest first.
    pub recent_hands: Vec<HandRecord>,
}

/// Response payload for `GET /api/history/export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HistoryExportResponse {
    /// Schema version (`1`).
    pub schema_version: u32,
    /// Unix epoch timestamp in milliseconds when the backup was generated.
    #[ts(type = "number")]
    pub exported_at_ms: u64,
    /// Current persisted preferences (`preferences.json`).
    pub preferences: PreferencesData,
    /// All completed matches in chronological order.
    pub matches: Vec<MatchRecord>,
    /// All settled hands in chronological order.
    pub hands: Vec<HandRecord>,
}

/// Request payload for `POST /api/history/clear`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClearHistoryRequest {
    /// Must be explicitly `true` to confirm clearing `hands.jsonl` and `matches.jsonl`.
    pub confirm: bool,
}

/// Response payload for `POST /api/history/clear`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClearHistoryResponse {
    /// `true` once `hands.jsonl` and `matches.jsonl` are cleared.
    pub cleared: bool,
    /// Number of match records removed.
    pub cleared_matches: u32,
    /// Number of hand records removed.
    pub cleared_hands: u32,
}

//! Public protocol, view, event, settlement, and error types.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::seat::{Seat, Wind};
use crate::tile::Tile;

/// Supported match progression modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum MatchMode {
    /// One East wind round (`东一局` through `东四局`, including `连庄`), starting at `100` points.
    EastOnly,
    /// Unlimited hands with cumulative net scores starting at `0`, ending via `EndMatch`.
    FreeTable,
}

/// Static capability descriptor exposed by a `Ruleset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuleProfile {
    /// Human-readable ruleset name.
    pub name: String,
    /// Whether `吃` (`Chi`) is enabled.
    pub allow_chi: bool,
    /// Whether four-of-a-kind counts as two pairs in `豪华七对`.
    pub allow_four_of_kind_seven_pairs: bool,
    /// Whether multiple seats may win on the same discard (`一炮多响`).
    pub allow_multi_ron: bool,
    /// Maximum capped fan per winning hand.
    pub fan_cap: u8,
    /// Total tiles in the wall deck.
    pub wall_size: u8,
    /// Number of dead-wall tiles reserved at the tail.
    pub dead_wall_size: u8,
    /// Starting scores for `EastOnly` matches.
    pub starting_scores_east_only: [i32; 4],
    /// Starting scores for `FreeTable` matches.
    pub starting_scores_free_table: [i32; 4],
}

/// Actions a player can submit to the table engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PlayerAction {
    /// Discard `tile` either directly from `drawn_tile` (`from_drawn = true`, `摸切`)
    /// or from `closed_tiles` (`from_drawn = false`, `手切`).
    Discard { tile: Tile, from_drawn: bool },
    /// Declare a self-drawn win (`自摸`).
    Tsumo,
    /// Claim a discard to win (`点炮`).
    Ron { tile: Tile },
    /// Claim an attempted `AddedKong` tile to win (`抢杠胡`).
    RobKongHu { tile: Tile },
    /// Claim a discard to form a triplet (`碰`).
    Pon { tile: Tile },
    /// Claim a discard to form an exposed kong (`明杠`).
    ExposedKong { tile: Tile },
    /// Upgrade an existing `Pon` meld with the 4th tile in hand (`补杠`).
    AddedKong { tile: Tile },
    /// Declare a concealed kong from 4 matching tiles in hand (`暗杠`).
    ConcealedKong { tile: Tile },
    /// Claim a discard from the previous seat to form a sequence (`吃`, reserved).
    Chi { tile: Tile, with: [Tile; 2] },
    /// Decline an optional reaction opportunity (`过`).
    Pass,
    /// Confirm readiness to deal the next hand during `BetweenHands`.
    ConfirmNextHand,
    /// End a `FreeTable` match during `BetweenHands`.
    EndMatch,
}

/// Origin of the current turn in `TurnAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TurnSource {
    /// Initial 14-tile hand dealt to the dealer at hand start.
    InitialDealer,
    /// Normal draw from the head of the wall.
    NormalDraw,
    /// Replacement draw from the tail of the wall after a kong.
    ReplacementDraw,
    /// Turn entered immediately after claiming a meld (`碰` / `吃`).
    AfterMeld,
}

/// Public discard record in a seat's river (`牌河`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct DiscardEntry {
    /// Discarded tile kind.
    pub tile: Tile,
    /// `true` for `摸切`, `false` for `手切`.
    pub is_tsumogiri: bool,
    /// Seat that claimed this discard for a meld, if any.
    pub claimed_by: Option<Seat>,
    /// Strictly monotonic table event sequence number of this discard.
    #[ts(type = "number")]
    pub seq: u64,
}

/// Publicly projected meld on a seat's table area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum MeldView {
    /// Exposed triplet (`碰`).
    Pon {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Exposed kong (`明杠`).
    ExposedKong {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Added kong upgraded from a `Pon` (`补杠`).
    AddedKong {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Concealed kong (`暗杠`): `Some(tile)` for the owner, `None` for opponents.
    ConcealedKong {
        tile: Option<Tile>,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Exposed sequence (`吃`, reserved for future rulesets).
    Chi {
        tiles: [Tile; 3],
        called_tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
}

/// Viewer's own hand split between sorted closed tiles and the latest drawn tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlayerHandView {
    /// Sorted concealed tiles held in hand.
    pub closed_tiles: Vec<Tile>,
    /// Newly drawn tile on the current turn, kept separate until discarded or melded.
    pub drawn_tile: Option<Tile>,
}

/// Publicly visible state for one seat at the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PublicSeatView {
    /// Seat identifier.
    pub seat: Seat,
    /// Current score.
    pub score: i32,
    /// Number of tiles currently held in closed hand (including `drawn_tile`).
    pub hand_count: u8,
    /// Ordered discards in this seat's river (`牌河`).
    pub river: Vec<DiscardEntry>,
    /// Publicly visible melds declared by this seat.
    pub melds: Vec<MeldView>,
    /// Sequence number of this seat's most recent draw or confirmed meld (`0` at hand start).
    #[ts(type = "number")]
    pub last_draw_or_meld_seq: u64,
}

/// Publicly visible match phase (never exposes hidden reaction windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum PublicPhase {
    /// Hand is currently active.
    Playing,
    /// Hand has settled and the table is waiting for `ConfirmNextHand` (or `EndMatch`).
    BetweenHands,
    /// Match has finished.
    MatchFinished,
}

/// Public turn metadata within the active or settled hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct TurnView {
    /// Current wind round.
    pub wind_round: Wind,
    /// 1-based hand number within the wind round.
    pub hand_number: u8,
    /// Consecutive dealer wins (`连庄`).
    pub dealer_streak: u32,
    /// Current dealer seat (`庄家`).
    pub dealer: Seat,
    /// Publicly active seat during `Playing`, or `None` between/after hands.
    pub active_seat: Option<Seat>,
}

/// Seat-projected snapshot of the table state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlayerView {
    /// Seat for which this view was projected.
    pub viewer: Seat,
    /// Match mode.
    pub mode: MatchMode,
    /// Static ruleset capabilities.
    pub rule_profile: RuleProfile,
    /// Public phase.
    pub phase: PublicPhase,
    /// Viewer's own private hand.
    pub hand: PlayerHandView,
    /// Public state for all four seats in `[East, South, West, North]` order.
    pub seats: [PublicSeatView; 4],
    /// Remaining tiles in the wall (`0..=83` during play).
    pub wall_remaining: u8,
    /// Current round/hand/turn metadata.
    pub turn: TurnView,
    /// Authoritative legal actions available to `viewer`.
    pub legal_actions: Vec<PlayerAction>,
    /// Settlement details during `BetweenHands` (and final hand on `MatchFinished`).
    pub last_settlement: Option<HandSettlement>,
    /// Readiness flags during `BetweenHands`.
    pub ready_seats: Option<[bool; 4]>,
    /// Final match summary once `MatchFinished` is reached.
    pub match_summary: Option<MatchSummary>,
    /// Latest monotonically increasing table event sequence number.
    #[ts(type = "number")]
    pub seq: u64,
}

/// Ruleset-agnostic fan breakdown entry on a winning settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScoreItem {
    /// Stable machine code for the fan item.
    pub code: String,
    /// Chinese display label matching `GLOSSARY.md`.
    pub label: String,
    /// Fan value contributed by this item.
    pub fan: u8,
}

/// Classification of a kong transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum KongKind {
    /// Exposed kong (`明杠`).
    ExposedKong,
    /// Added kong (`补杠`).
    AddedKong,
    /// Concealed kong (`暗杠`).
    ConcealedKong,
}

/// Score transfer generated by a confirmed kong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct KongTransfer {
    /// Kong category.
    pub kind: KongKind,
    /// Seat that declared the kong.
    pub actor: Seat,
    /// Discarder seat for `ExposedKong`, or `None` for `AddedKong` / `ConcealedKong`.
    pub from_seat: Option<Seat>,
    /// Tile kind of the kong.
    pub tile: Tile,
    /// Per-seat score deltas indexed by `Seat::index()`.
    pub score_deltas: [i32; 4],
}

/// Way in which a winning hand completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum WinKind {
    /// Self-drawn win (`自摸`).
    Tsumo,
    /// Claimed discard win (`点炮`).
    Ron,
    /// Robbing an `AddedKong` (`抢杠胡`).
    RobKongHu,
}

/// Scoring summary for one winning seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WinnerSettlement {
    /// Winning seat.
    pub seat: Seat,
    /// Win method.
    pub win_kind: WinKind,
    /// Breakdown of fan items.
    pub items: Vec<ScoreItem>,
    /// Effective total fan (capped by `RuleProfile::fan_cap`).
    pub total_fan: u8,
    /// Base points paid per payer before summing payers.
    pub base_points: i32,
}

/// Fully revealed meld shown at hand settlement (including concrete `ConcealedKong` tile).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum RevealedMeld {
    /// Exposed triplet (`碰`).
    Pon {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Exposed kong (`明杠`).
    ExposedKong {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Added kong (`补杠`).
    AddedKong {
        tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Concealed kong (`暗杠`) revealed at hand settlement.
    ConcealedKong {
        tile: Tile,
        #[ts(type = "number")]
        seq: u64,
    },
    /// Exposed sequence (`吃`, reserved).
    Chi {
        tiles: [Tile; 3],
        called_tile: Tile,
        from_seat: Seat,
        #[ts(type = "number")]
        seq: u64,
    },
}

/// One seat's complete revealed hand at hand settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RevealedHand {
    /// Seat identifier.
    pub seat: Seat,
    /// Sorted closed tiles held at hand end.
    pub closed_tiles: Vec<Tile>,
    /// Drawn tile held at hand end, if any.
    pub drawn_tile: Option<Tile>,
    /// All melds declared by this seat, with concealed kongs revealed.
    pub melds: Vec<RevealedMeld>,
}

/// Outcome of a completed hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HandOutcome {
    /// One or more winners (`一炮多响` supported).
    Win {
        winners: Vec<WinnerSettlement>,
        discarder: Option<Seat>,
        winning_tile: Tile,
    },
    /// Exhausted wall with no winner (`荒庄`).
    DrawGame,
}

/// Complete settlement record produced when a hand finishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HandSettlement {
    /// Wind round of the settled hand.
    pub wind_round: Wind,
    /// 1-based hand number within the wind round.
    pub hand_number: u8,
    /// Dealer streak active for this hand.
    pub dealer_streak: u32,
    /// Dealer seat for this hand.
    pub dealer: Seat,
    /// Win or `DrawGame` outcome.
    pub outcome: HandOutcome,
    /// Confirmed kong transfers included in this settlement.
    pub kong_transfers: Vec<KongTransfer>,
    /// Combined per-seat score deltas for this hand.
    pub score_deltas: [i32; 4],
    /// Updated cumulative scores after applying `score_deltas`.
    pub new_scores: [i32; 4],
    /// All four seats' revealed hands.
    pub revealed_hands: [RevealedHand; 4],
}

/// Final summary emitted when a match ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct MatchSummary {
    /// Mode of the completed match.
    pub mode: MatchMode,
    /// Number of completed hands.
    pub hands_played: u32,
    /// Starting scores at match start.
    pub initial_scores: [i32; 4],
    /// Final scores at match end.
    pub final_scores: [i32; 4],
    /// Net score change per seat (`final_scores[i] - initial_scores[i]`).
    pub net_deltas: [i32; 4],
}

/// Seat-projected incremental domain event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlayerEvent {
    /// Strictly monotonically increasing sequence number starting at `1`.
    #[ts(type = "number")]
    pub seq: u64,
    /// Projected event payload.
    pub kind: PlayerEventKind,
}

/// Seat-projected event variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
// Keeps `HandSettled { settlement: HandSettlement }` unboxed to match the shared protocol contract.
#[allow(clippy::large_enum_variant)]
pub enum PlayerEventKind {
    /// Emitted once when the match starts.
    MatchStarted {
        mode: MatchMode,
        initial_dealer: Seat,
        initial_scores: [i32; 4],
        rule_profile: RuleProfile,
    },
    /// Emitted when a new hand is dealt; `hand` contains only the viewer's own tiles.
    HandStarted {
        wind_round: Wind,
        hand_number: u8,
        dealer_streak: u32,
        dealer: Seat,
        hand: PlayerHandView,
        wall_remaining: u8,
        scores: [i32; 4],
    },
    /// Emitted on wall draw; `tile` is `Some` only for the drawing seat.
    TileDrawn {
        seat: Seat,
        tile: Option<Tile>,
        is_replacement: bool,
        wall_remaining: u8,
    },
    /// Emitted when a seat discards a tile.
    TileDiscarded {
        seat: Seat,
        tile: Tile,
        is_tsumogiri: bool,
    },
    /// Emitted when an `AddedKong` opens a `RobKongHu` window.
    AddedKongAttempted { seat: Seat, tile: Tile },
    /// Emitted when a meld is confirmed (`ConcealedKong` hides `tile` for opponents).
    MeldDeclared {
        seat: Seat,
        meld: MeldView,
        kong_score_deltas: Option<[i32; 4]>,
    },
    /// Emitted when a hand finishes and all hands are revealed.
    HandSettled { settlement: HandSettlement },
    /// Emitted when a seat confirms readiness for the next hand.
    NextHandConfirmed { seat: Seat },
    /// Emitted when the match finishes.
    MatchFinished { summary: MatchSummary },
}

/// Errors returned by match initialization and action application.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize, TS)]
pub enum MatchError {
    /// Seat argument could not be resolved.
    #[error("invalid seat")]
    InvalidSeat,
    /// Action is illegal in the current state for this seat.
    #[error("illegal action {action:?} for seat {seat:?}")]
    IllegalAction { seat: Seat, action: PlayerAction },
    /// Action does not belong to the current table phase.
    #[error("action {action:?} is not valid in current phase for seat {seat:?}")]
    WrongPhase { seat: Seat, action: PlayerAction },
    /// Seat attempted to act when another seat holds the turn or reaction window.
    #[error("not seat {seat:?}'s turn")]
    NotYourTurn { seat: Seat },
    /// Seat already submitted `ConfirmNextHand` for the current settlement.
    #[error("seat {seat:?} already confirmed next hand")]
    AlreadyConfirmed { seat: Seat },
    /// Wall supplied by a `WallProvider` does not match the ruleset deck invariants.
    #[error("invalid wall: {0}")]
    InvalidWall(String),
    /// Match has already transitioned to `MatchFinished`.
    #[error("match has already finished")]
    MatchAlreadyFinished,
}

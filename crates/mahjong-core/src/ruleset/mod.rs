//! Ruleset abstraction seam isolating rule variants from the core table state machine.

pub mod tuidaohu;

use crate::protocol::{
    HandOutcome, HandSettlement, KongKind, KongTransfer, MatchMode, PlayerAction, RevealedHand,
    RevealedMeld, RuleProfile, ScoreItem, TurnSource, WinKind,
};
use crate::seat::{Seat, Wind};
use crate::tile::Tile;

pub use tuidaohu::TuidaohuRuleset;

/// Arbitration tier for discard reactions (`Hu > Pon / ExposedKong > Chi > Pass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReactionPriority {
    /// Decline the opportunity (`过`).
    Pass = 0,
    /// Sequence meld (`吃`, reserved for future rulesets).
    Chi = 1,
    /// Triplet or exposed kong (`碰` / `明杠`).
    PonOrKong = 2,
    /// Winning claim (`点炮` / `抢杠胡`).
    Hu = 3,
}

/// Inputs required to enumerate legal actions on an active player's turn.
#[derive(Debug, Clone, Copy)]
pub struct TurnContext<'a> {
    /// Sorted closed tiles currently held by the active player.
    pub closed_tiles: &'a [Tile],
    /// Newly drawn tile on this turn, or `None` for `InitialDealer` / `AfterMeld`.
    pub drawn_tile: Option<Tile>,
    /// Melds already declared by the active player.
    pub melds: &'a [RevealedMeld],
    /// Origin of the current turn.
    pub source: TurnSource,
    /// Remaining drawable tiles in the wall.
    pub wall_remaining: u8,
}

/// Inputs required to enumerate legal reactions to another seat's discard.
#[derive(Debug, Clone, Copy)]
pub struct DiscardReactionContext<'a> {
    /// Reacting seat.
    pub actor: Seat,
    /// Seat that discarded the tile.
    pub discarder: Seat,
    /// Discarded tile.
    pub tile: Tile,
    /// Reacting seat's sorted closed tiles.
    pub closed_tiles: &'a [Tile],
    /// Reacting seat's existing melds.
    pub melds: &'a [RevealedMeld],
    /// Remaining drawable tiles in the wall.
    pub wall_remaining: u8,
    /// Whether `过水胡` currently blocks `Ron` for this seat.
    pub passed_hu_lock: bool,
    /// Whether `过水碰` currently blocks `Pon` for this specific tile.
    pub passed_pon_locked: bool,
    /// Whether `actor == discarder.next()` (for rulesets enabling `Chi`).
    pub is_next_seat: bool,
}

/// Inputs required to enumerate legal reactions to another seat's attempted `AddedKong`.
#[derive(Debug, Clone, Copy)]
pub struct RobKongContext<'a> {
    /// Reacting seat.
    pub actor: Seat,
    /// Seat attempting the `AddedKong`.
    pub kong_actor: Seat,
    /// Tile being upgraded to an `AddedKong`.
    pub tile: Tile,
    /// Reacting seat's sorted closed tiles.
    pub closed_tiles: &'a [Tile],
    /// Reacting seat's existing melds.
    pub melds: &'a [RevealedMeld],
    /// Remaining drawable tiles in the wall.
    pub wall_remaining: u8,
    /// Whether `过水胡` currently blocks `RobKongHu` for this seat.
    pub passed_hu_lock: bool,
}

/// Inputs required to evaluate a candidate winning hand.
#[derive(Debug, Clone, Copy)]
pub struct WinEvalInput<'a> {
    /// Sorted closed tiles held by the seat.
    pub closed_tiles: &'a [Tile],
    /// Winning extra tile (`drawn_tile` for `Tsumo`, claimed tile for `Ron`/`RobKongHu`),
    /// or `None` when the 14th tile is already inside `closed_tiles` (`InitialDealer`).
    pub extra_tile: Option<Tile>,
    /// Melds declared by the winning seat.
    pub melds: &'a [RevealedMeld],
    /// Method of winning (`Tsumo`, `Ron`, or `RobKongHu`).
    pub win_kind: WinKind,
    /// Turn source (used for `杠上开花` on `ReplacementDraw`).
    pub turn_source: Option<TurnSource>,
    /// Remaining wall tiles at the time of the win (used for `海底捞月`/`河底捞鱼`).
    pub wall_remaining: u8,
}

/// Scoring breakdown returned when a hand legally wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedWin {
    /// Ruleset-agnostic fan items.
    pub items: Vec<ScoreItem>,
    /// Effective total fan after applying the ruleset's cap.
    pub total_fan: u8,
    /// Base points paid per payer before summing multiple payers.
    pub base_points: i32,
}

/// Per-winner input bundle for hand settlement.
#[derive(Debug, Clone)]
pub struct WinCandidate {
    /// Winning seat.
    pub seat: Seat,
    /// Win kind.
    pub win_kind: WinKind,
    /// Evaluated scoring breakdown.
    pub evaluated: EvaluatedWin,
}

/// Inputs for producing a complete `HandSettlement`.
#[derive(Debug, Clone)]
pub struct SettleHandInput {
    /// Wind round of the settled hand.
    pub wind_round: Wind,
    /// 1-based hand number within the wind round.
    pub hand_number: u8,
    /// Current dealer streak.
    pub dealer_streak: u32,
    /// Dealer seat for this hand.
    pub dealer: Seat,
    /// Starting scores before this hand's settlements.
    pub current_scores: [i32; 4],
    /// Confirmed kong transfers accumulated during this hand.
    pub kong_transfers: Vec<KongTransfer>,
    /// Revealed hands for all four seats.
    pub revealed_hands: [RevealedHand; 4],
    /// Win details if one or more seats won, or `None` for `DrawGame`.
    pub win_details: Option<WinSettlementDetails>,
}

/// Winner-specific settlement parameters when a hand does not end in `DrawGame`.
#[derive(Debug, Clone)]
pub struct WinSettlementDetails {
    /// Winning seats with evaluated fan and base points.
    pub winners: Vec<WinCandidate>,
    /// Discarder seat for `Ron` / `RobKongHu`, or `None` for `Tsumo`.
    pub discarder: Option<Seat>,
    /// Winning tile.
    pub winning_tile: Tile,
}

/// Inputs for deciding next-hand rotation and match termination after `HandSettled`.
#[derive(Debug, Clone, Copy)]
pub struct HandProgressionInput<'a> {
    /// Active match mode.
    pub mode: MatchMode,
    /// Current wind round.
    pub wind_round: Wind,
    /// Current 1-based hand number.
    pub hand_number: u8,
    /// Current dealer streak.
    pub dealer_streak: u32,
    /// Current dealer seat.
    pub dealer: Seat,
    /// Outcome of the hand that just settled.
    pub outcome: &'a HandOutcome,
}

/// Decision produced by `Ruleset::advance_hand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandProgression {
    /// Continue to another hand with updated round/dealer metadata.
    NextHand {
        wind_round: Wind,
        hand_number: u8,
        dealer_streak: u32,
        dealer: Seat,
    },
    /// Terminate the match immediately after `HandSettled`.
    FinishMatch,
}

/// Pluggable ruleset trait covering the six rule responsibilities decoupled from `Match`.
pub trait Ruleset {
    /// 1 & 2. Static capability descriptor (`wall_size`, `dead_wall_size`, flags, starting scores).
    fn profile(&self) -> RuleProfile;

    /// 1. Canonical unshuffled deck for one hand.
    fn deck(&self) -> Vec<Tile>;

    /// 3. Enumerates legal actions during a seat's `TurnAction`.
    fn legal_turn_actions(&self, ctx: &TurnContext<'_>) -> Vec<PlayerAction>;

    /// 3. Enumerates legal actions when reacting to another seat's discard.
    fn legal_discard_reactions(&self, ctx: &DiscardReactionContext<'_>) -> Vec<PlayerAction>;

    /// 3. Enumerates legal actions when reacting to another seat's attempted `AddedKong`.
    fn legal_rob_kong_reactions(&self, ctx: &RobKongContext<'_>) -> Vec<PlayerAction>;

    /// 4. Priority tier of a reaction action for window arbitration.
    fn reaction_priority(&self, action: &PlayerAction) -> ReactionPriority;

    /// 5. Evaluates whether a hand wins and returns its fan breakdown and base points.
    fn evaluate_win(&self, input: &WinEvalInput<'_>) -> Option<EvaluatedWin>;

    /// 5. Computes per-seat kong score deltas for a confirmed kong.
    fn kong_score_deltas(&self, kind: KongKind, actor: Seat, from_seat: Option<Seat>) -> [i32; 4];

    /// 5. Produces the authoritative `HandSettlement` for either a win or `DrawGame`.
    fn settle_hand(&self, input: SettleHandInput) -> HandSettlement;

    /// 6. Determines dealer rotation (`连庄` vs pass dealer) and match completion.
    fn advance_hand(&self, input: HandProgressionInput<'_>) -> HandProgression;
}

/// Dynamic dispatch-free wrapper enabling runtime ruleset selection without `dyn Ruleset`.
#[derive(Debug, Clone, Copy)]
pub enum AnyRuleset {
    /// Standard 4-player Tuidaohu ruleset.
    Tuidaohu(TuidaohuRuleset),
}

impl Default for AnyRuleset {
    fn default() -> Self {
        Self::Tuidaohu(TuidaohuRuleset)
    }
}

impl Ruleset for AnyRuleset {
    fn profile(&self) -> RuleProfile {
        match self {
            Self::Tuidaohu(r) => r.profile(),
        }
    }

    fn deck(&self) -> Vec<Tile> {
        match self {
            Self::Tuidaohu(r) => r.deck(),
        }
    }

    fn legal_turn_actions(&self, ctx: &TurnContext<'_>) -> Vec<PlayerAction> {
        match self {
            Self::Tuidaohu(r) => r.legal_turn_actions(ctx),
        }
    }

    fn legal_discard_reactions(&self, ctx: &DiscardReactionContext<'_>) -> Vec<PlayerAction> {
        match self {
            Self::Tuidaohu(r) => r.legal_discard_reactions(ctx),
        }
    }

    fn legal_rob_kong_reactions(&self, ctx: &RobKongContext<'_>) -> Vec<PlayerAction> {
        match self {
            Self::Tuidaohu(r) => r.legal_rob_kong_reactions(ctx),
        }
    }

    fn reaction_priority(&self, action: &PlayerAction) -> ReactionPriority {
        match self {
            Self::Tuidaohu(r) => r.reaction_priority(action),
        }
    }

    fn evaluate_win(&self, input: &WinEvalInput<'_>) -> Option<EvaluatedWin> {
        match self {
            Self::Tuidaohu(r) => r.evaluate_win(input),
        }
    }

    fn kong_score_deltas(&self, kind: KongKind, actor: Seat, from_seat: Option<Seat>) -> [i32; 4] {
        match self {
            Self::Tuidaohu(r) => r.kong_score_deltas(kind, actor, from_seat),
        }
    }

    fn settle_hand(&self, input: SettleHandInput) -> HandSettlement {
        match self {
            Self::Tuidaohu(r) => r.settle_hand(input),
        }
    }

    fn advance_hand(&self, input: HandProgressionInput<'_>) -> HandProgression {
        match self {
            Self::Tuidaohu(r) => r.advance_hand(input),
        }
    }
}

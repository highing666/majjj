//! Pure deterministic mahjong table rules, state machine, and seat projections.

pub mod engine;
pub mod projection;
pub mod protocol;
pub mod ruleset;
pub mod seat;
pub mod tile;
pub mod wall;

pub use engine::{Match, RecordedEvent};
pub use projection::{project_event, project_view};
pub use protocol::{
    DiscardEntry, HandOutcome, HandSettlement, KongKind, KongTransfer, MatchError, MatchMode,
    MatchSummary, MeldView, PlayerAction, PlayerEvent, PlayerEventKind, PlayerHandView, PlayerView,
    PublicPhase, PublicSeatView, RevealedHand, RevealedMeld, RuleProfile, ScoreItem, TurnSource,
    TurnView, WinKind, WinnerSettlement,
};
pub use ruleset::{
    AnyRuleset, DiscardReactionContext, EvaluatedWin, HandProgression, HandProgressionInput,
    ReactionPriority, RobKongContext, Ruleset, SettleHandInput, TuidaohuRuleset, TurnContext,
    WinCandidate, WinEvalInput, WinSettlementDetails,
};
pub use seat::{Seat, Wind};
pub use tile::{Suit, Tile};
pub use wall::{build_preset_wall, PresetWallProvider, SeededWallProvider, WallProvider};

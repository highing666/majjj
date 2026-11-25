//! Server-driven humanized pacing table, overlapped think-time calculation, and two-beat
//! discard view projection.

use std::time::Duration;

use mahjong_core::{
    DiscardEntry, Match, PlayerAction, PlayerEventKind, PlayerView, PublicPhase, RecordedEvent,
    Ruleset, Seat, Tile, WallProvider,
};

use crate::protocol::TablePace;

const PACING_SEED_SALT: u64 = 0x517C_C1B7_2722_0A95;

/// Identity and target release instant for a deferred two-beat `EventBatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReleaseTicket {
    pub match_id: u64,
    pub public_seq: u64,
    pub release_generation: u64,
    pub release_at: tokio::time::Instant,
}

/// Controls whether `TableSession` applies humanized server pacing and two-beat discard releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacingMode {
    /// Production pacing matching `TablePace` (`Standard` / `Fast`) and two-beat discard release.
    #[default]
    Humanized,
    /// Zero-delay mode for fast logic regression tests.
    Immediate,
}

/// Context from the preceding public event affecting the next AI turn's target delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AiTurnTrigger {
    #[default]
    Normal,
    AfterHandStarted,
    AfterMeldDeclared,
}

/// Unified two-beat release delay between `TileDiscarded` (`t = 0`) and the next `TileDrawn`
/// (or 0-wall `HandSettled(DrawGame)`).
#[must_use]
pub fn discard_release_delay(mode: PacingMode, pace: TablePace) -> Duration {
    match (mode, pace) {
        (PacingMode::Immediate, _) => Duration::ZERO,
        (PacingMode::Humanized, TablePace::Standard) => Duration::from_millis(200),
        (PacingMode::Humanized, TablePace::Fast) => Duration::from_millis(120),
    }
}

/// Opening buffer after `HandStarted` before the dealer AI acts.
#[must_use]
pub fn hand_start_buffer(mode: PacingMode, pace: TablePace) -> Duration {
    match (mode, pace) {
        (PacingMode::Immediate, _) => Duration::ZERO,
        (PacingMode::Humanized, TablePace::Standard) => Duration::from_millis(650),
        (PacingMode::Humanized, TablePace::Fast) => Duration::from_millis(400),
    }
}

/// Minimum follow-up interval after `MeldDeclared` so an AI action aligns with the meld impact beat.
#[must_use]
pub fn meld_follow_up_min(mode: PacingMode, pace: TablePace) -> Duration {
    match (mode, pace) {
        (PacingMode::Immediate, _) => Duration::ZERO,
        (PacingMode::Humanized, TablePace::Standard) => Duration::from_millis(700),
        (PacingMode::Humanized, TablePace::Fast) => Duration::from_millis(420),
    }
}

/// Inclusive `(min_ms, max_ms)` humanized action window for an AI decision, or `None` for
/// zero-delay actions (`Pass`, `ConfirmNextHand`, `EndMatch`).
#[must_use]
pub fn action_delay_range_ms(action: PlayerAction, pace: TablePace) -> Option<(u64, u64)> {
    match (action, pace) {
        (
            PlayerAction::Discard {
                from_drawn: true, ..
            },
            TablePace::Standard,
        ) => Some((380, 580)),
        (
            PlayerAction::Discard {
                from_drawn: true, ..
            },
            TablePace::Fast,
        ) => Some((230, 350)),
        (
            PlayerAction::Discard {
                from_drawn: false, ..
            },
            TablePace::Standard,
        ) => Some((520, 850)),
        (
            PlayerAction::Discard {
                from_drawn: false, ..
            },
            TablePace::Fast,
        ) => Some((310, 510)),
        (
            PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::AddedKong { .. }
            | PlayerAction::ConcealedKong { .. }
            | PlayerAction::Chi { .. },
            TablePace::Standard,
        ) => Some((420, 680)),
        (
            PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::AddedKong { .. }
            | PlayerAction::ConcealedKong { .. }
            | PlayerAction::Chi { .. },
            TablePace::Fast,
        ) => Some((250, 410)),
        (
            PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. },
            TablePace::Standard,
        ) => Some((350, 550)),
        (
            PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. },
            TablePace::Fast,
        ) => Some((210, 330)),
        (PlayerAction::Pass | PlayerAction::ConfirmNextHand | PlayerAction::EndMatch, _) => None,
    }
}

/// Computes remaining sleep duration for an overlapped window starting at `started_at`.
#[must_use]
pub fn overlapped_wait(
    started_at: tokio::time::Instant,
    now: tokio::time::Instant,
    target: Duration,
) -> Duration {
    (started_at + target).saturating_duration_since(now)
}

pub(crate) fn target_ai_delay(
    mode: PacingMode,
    pace: TablePace,
    action: PlayerAction,
    trigger: AiTurnTrigger,
    step_seed: u64,
) -> Duration {
    if mode == PacingMode::Immediate {
        return Duration::ZERO;
    }
    let Some((min_ms, max_ms)) = action_delay_range_ms(action, pace) else {
        return Duration::ZERO;
    };
    let span = max_ms - min_ms + 1;
    let base_ms = min_ms + (mix_u64(step_seed ^ PACING_SEED_SALT) % span);
    let base = Duration::from_millis(base_ms);
    match trigger {
        AiTurnTrigger::Normal => base,
        AiTurnTrigger::AfterHandStarted => base + hand_start_buffer(mode, pace),
        AiTurnTrigger::AfterMeldDeclared => base.max(meld_follow_up_min(mode, pace)),
    }
}

/// Applies an immediate `TileDiscarded` transition onto a pre-discard `PlayerView` while
/// withholding the subsequent `TileDrawn` (or `HandSettled`) until the two-beat timer fires.
pub(crate) fn apply_discard_to_view(
    view: &mut PlayerView,
    discarder: Seat,
    tile: Tile,
    from_drawn: bool,
    discard_seq: u64,
) {
    view.phase = PublicPhase::Playing;
    view.turn.active_seat = Some(discarder);
    view.legal_actions.clear();
    view.seq = discard_seq;

    let seat_view = &mut view.seats[discarder.index()];
    seat_view.hand_count = seat_view.hand_count.saturating_sub(1);
    seat_view.river.push(DiscardEntry {
        tile,
        is_tsumogiri: from_drawn,
        claimed_by: None,
        seq: discard_seq,
    });

    if view.viewer == discarder {
        if from_drawn {
            view.hand.drawn_tile = None;
        } else {
            if let Some(pos) = view.hand.closed_tiles.iter().position(|&t| t == tile) {
                view.hand.closed_tiles.remove(pos);
            }
            if let Some(drawn) = view.hand.drawn_tile.take() {
                view.hand.closed_tiles.push(drawn);
                view.hand.closed_tiles.sort_unstable();
            }
        }
    }
}

pub(crate) fn compute_ai_turn_trigger<R: Ruleset, W: WallProvider>(
    game: &Match<R, W>,
    emitted: &[RecordedEvent],
) -> AiTurnTrigger {
    let mut trigger = AiTurnTrigger::Normal;
    for event in emitted {
        match game.project_event(event, Seat::East).kind {
            PlayerEventKind::HandStarted { .. } => {
                trigger = AiTurnTrigger::AfterHandStarted;
            }
            PlayerEventKind::MeldDeclared { .. } if trigger != AiTurnTrigger::AfterHandStarted => {
                trigger = AiTurnTrigger::AfterMeldDeclared;
            }
            _ => {}
        }
    }
    trigger
}

fn mix_u64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

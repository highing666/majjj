//! Server-driven humanized pacing table, overlapped think-time calculation, and two-beat
//! discard view projection.

use std::time::Duration;

use mahjong_core::{
    DiscardEntry, Match, PlayerAction, PlayerEventKind, PlayerView, PublicPhase, RecordedEvent,
    Ruleset, Seat, Tile, WallProvider,
};

use crate::persona::PersonaId;
use crate::protocol::TablePace;

const PACING_SEED_SALT: u64 = 0x517C_C1B7_2722_0A95;
const LONG_THINK_SEED_SALT: u64 = 0x9E37_79B9_517C_C1B7;

/// Inclusive `(min_ms, max_ms)` window for occasional `Standard` `手切` long-think pauses (ADR-0014).
pub const LONG_THINK_RANGE_MS: (u64, u64) = (5_000, 9_000);

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

/// Inclusive `(min_ms, max_ms)` baseline humanized action window for an AI decision (ADR-0014),
/// or `None` for zero-delay actions (`Pass`, `ConfirmNextHand`, `EndMatch`).
#[must_use]
pub fn action_delay_range_ms(action: PlayerAction, pace: TablePace) -> Option<(u64, u64)> {
    match (action, pace) {
        (
            PlayerAction::Discard {
                from_drawn: true, ..
            },
            TablePace::Standard,
        ) => Some((1_400, 2_600)),
        (
            PlayerAction::Discard {
                from_drawn: true, ..
            },
            TablePace::Fast,
        ) => Some((600, 1_200)),
        (
            PlayerAction::Discard {
                from_drawn: false, ..
            },
            TablePace::Standard,
        ) => Some((2_000, 4_000)),
        (
            PlayerAction::Discard {
                from_drawn: false, ..
            },
            TablePace::Fast,
        ) => Some((900, 1_800)),
        (
            PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::AddedKong { .. }
            | PlayerAction::ConcealedKong { .. }
            | PlayerAction::Chi { .. },
            TablePace::Standard,
        ) => Some((1_200, 2_200)),
        (
            PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::AddedKong { .. }
            | PlayerAction::ConcealedKong { .. }
            | PlayerAction::Chi { .. },
            TablePace::Fast,
        ) => Some((600, 1_100)),
        (
            PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. },
            TablePace::Standard,
        ) => Some((900, 1_600)),
        (
            PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. },
            TablePace::Fast,
        ) => Some((500, 900)),
        (PlayerAction::Pass | PlayerAction::ConfirmNextHand | PlayerAction::EndMatch, _) => None,
    }
}

/// Public-progress-driven percentage chance (`15%` early, `20%` mid, `25%` late) that a
/// `Standard` `手切` enters the `5,000–9,000 ms` long-think window (ADR-0014).
///
/// Depends strictly on publicly visible `wall_remaining` and never inspects private hand state.
#[must_use]
pub const fn long_think_chance_percent(wall_remaining: usize) -> u64 {
    if wall_remaining > 55 {
        15
    } else if wall_remaining > 28 {
        20
    } else {
        25
    }
}

/// Slight `Standard`-pace personality multiplier expressed in percent (`80`, `100`, `115`).
#[must_use]
pub const fn persona_pacing_multiplier_percent(persona_id: Option<PersonaId>) -> u64 {
    match persona_id {
        Some(PersonaId::QuickHothead) => 80,
        Some(PersonaId::StubbornBlamer | PersonaId::TiltedMaster) => 115,
        Some(PersonaId::TeaSarcasm | PersonaId::ChaosInstigator) | None => 100,
    }
}

fn sample_range_ms(min_ms: u64, max_ms: u64, step_seed: u64) -> u64 {
    let span = max_ms - min_ms + 1;
    min_ms + (mix_u64(step_seed ^ PACING_SEED_SALT) % span)
}

/// Samples the deterministic humanized thinking duration (in ms) for an AI action given
/// `pace`, optional `persona_id`, publicly visible `wall_remaining`, and `step_seed`.
#[must_use]
pub fn sample_humanized_delay_ms(
    action: PlayerAction,
    pace: TablePace,
    persona_id: Option<PersonaId>,
    wall_remaining: usize,
    step_seed: u64,
) -> Option<u64> {
    let (min_ms, max_ms) = action_delay_range_ms(action, pace)?;
    if pace == TablePace::Fast {
        return Some(sample_range_ms(min_ms, max_ms, step_seed));
    }

    let multiplier = persona_pacing_multiplier_percent(persona_id);
    let is_tedashi = matches!(
        action,
        PlayerAction::Discard {
            from_drawn: false,
            ..
        }
    );
    let triggers_long_think = is_tedashi
        && (mix_u64(step_seed ^ LONG_THINK_SEED_SALT) % 100)
            < long_think_chance_percent(wall_remaining);

    if triggers_long_think {
        let (long_min, long_max) = LONG_THINK_RANGE_MS;
        let raw = sample_range_ms(long_min, long_max, step_seed);
        return Some((raw * multiplier / 100).clamp(long_min, long_max));
    }

    let raw = sample_range_ms(min_ms, max_ms, step_seed);
    Some((raw * multiplier / 100).clamp(min_ms, max_ms))
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
    persona_id: Option<PersonaId>,
    wall_remaining: usize,
    step_seed: u64,
) -> Duration {
    if mode == PacingMode::Immediate {
        return Duration::ZERO;
    }
    let Some(base_ms) =
        sample_humanized_delay_ms(action, pace, persona_id, wall_remaining, step_seed)
    else {
        return Duration::ZERO;
    };
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

//! Pure synchronous, stateless mahjong AI decision contract and L1–L4 decision pipeline.

pub use mahjong_core;

mod calibrate;
pub mod eval;
mod l2;
mod l3;
mod l3_weights;
mod l4;
mod scoring;
mod shanten;
mod shared;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;

use mahjong_core::{PlayerAction, PlayerView, Tile};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// AI difficulty tier selectable at table setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum AiDifficulty {
    /// Full L1–L4 evaluation pipeline (expected value, non-Riichi defense, and Monte Carlo search).
    #[default]
    Strong,
    /// Intermediate L1–L3 evaluation (expected value and light defense without L4 Monte Carlo search).
    Intermediate,
}

/// AI play-style preference selectable per bot seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum AiStyle {
    /// Favors speed and quick tenpai over hand value.
    AggressiveFast,
    /// Balanced offense and defense.
    #[default]
    Balanced,
    /// Favors higher-scoring shapes and conservative defense.
    DefensiveBigHand,
}

/// Computational budget governing Monte Carlo search in Slice 3+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecisionBudget {
    /// Fixed sample count for bit-exact reproducible offline evaluation.
    Deterministic { max_samples: usize },
    /// Bounded sample count with soft/hard millisecond wall-clock caps for live play.
    Realtime {
        max_samples: usize,
        soft_ms: u64,
        hard_ms: u64,
    },
}

impl Default for DecisionBudget {
    fn default() -> Self {
        Self::Realtime {
            max_samples: 128,
            soft_ms: 120,
            hard_ms: 180,
        }
    }
}

/// Stateless configuration context passed to [`decide`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DecisionContext {
    /// Selected AI difficulty tier.
    pub difficulty: AiDifficulty,
    /// Selected AI style preference.
    pub style: AiStyle,
    /// Search budget mode.
    pub budget: DecisionBudget,
}

/// Purely random legal-action baseline bot for offline ablation benchmarking.
///
/// Strictly picks from `view.legal_actions`, confirms next hands in `BetweenHands`
/// (never auto-ending `FreeTable` matches), and otherwise chooses uniformly at random.
pub struct RandomBot;

impl RandomBot {
    /// Selects a strictly legal action solely from `view` and `rng`.
    ///
    /// # Panics
    ///
    /// Panics if `view.legal_actions` is empty.
    #[must_use]
    pub fn decide<R: RngCore + ?Sized>(view: &PlayerView, rng: &mut R) -> PlayerAction {
        assert!(
            !view.legal_actions.is_empty(),
            "RandomBot::decide requires non-empty legal_actions"
        );

        if view.legal_actions.contains(&PlayerAction::ConfirmNextHand) {
            return PlayerAction::ConfirmNextHand;
        }

        let idx = shared::gen_range_usize(rng, view.legal_actions.len());
        view.legal_actions[idx]
    }
}

/// Stateless Slice 2 placeholder AI and Slice 3 quantitative baseline.
///
/// Always picks an action strictly from `view.legal_actions`, obeys the "always win
/// when offered" rule, confirms next hands in `BetweenHands`, and selects discards
/// and meld/kong choices using public-view tile efficiency with `摸切` by default.
pub struct TileEfficiencyBot;

impl TileEfficiencyBot {
    /// Computes a strictly legal action solely from `view` and `rng`.
    ///
    /// # Panics
    ///
    /// Panics if `view.legal_actions` is empty.
    #[must_use]
    pub fn decide<R: RngCore + ?Sized>(view: &PlayerView, rng: &mut R) -> PlayerAction {
        assert!(
            !view.legal_actions.is_empty(),
            "TileEfficiencyBot::decide requires non-empty legal_actions"
        );

        if let Some(action) = l2::fast_path_action(view) {
            return action;
        }

        if let Some(action) = choose_turn_action(view, rng) {
            return action;
        }

        if let Some(action) = choose_discard_reaction(view) {
            return action;
        }

        view.legal_actions[0]
    }
}

/// Pure synchronous, stateless AI entry point consuming only a seat-projected [`PlayerView`].
///
/// Routes through the L1–L4 decision pipeline (`l3::decide` / `l4::decide_with_l4`) with
/// tactical iron rules, all 11 Tuidaohu fan items, kong scoring, non-Riichi defense, and
/// opportunistic Monte Carlo search (for [`AiDifficulty::Strong`]).
#[must_use]
pub fn decide<R: RngCore + ?Sized>(
    view: &PlayerView,
    ctx: &DecisionContext,
    rng: &mut R,
) -> PlayerAction {
    l3::decide(view, ctx, rng)
}

fn choose_turn_action<R: RngCore + ?Sized>(view: &PlayerView, rng: &mut R) -> Option<PlayerAction> {
    let best_discard = evaluate_best_discard(view, rng);
    if let Some(kong_action) = choose_turn_kong(view, best_discard.as_ref().map(|d| d.shanten)) {
        return Some(kong_action);
    }
    best_discard.map(|d| d.action)
}

struct DiscardChoice {
    shanten: i8,
    action: PlayerAction,
}

fn evaluate_best_discard<R: RngCore + ?Sized>(
    view: &PlayerView,
    rng: &mut R,
) -> Option<DiscardChoice> {
    let mut discardable = [false; Tile::COUNT];
    let mut any_discard = false;
    for action in &view.legal_actions {
        if let PlayerAction::Discard { tile, .. } = *action {
            discardable[tile.index()] = true;
            any_discard = true;
        }
    }
    if !any_discard {
        return None;
    }

    let pre_hand = shanten::hand_counts(view);
    let mut hand = pre_hand;
    let remaining = shanten::compute_remaining(view);
    let melds = view.seats[view.viewer.index()].melds.len();
    let allow_quad = view.rule_profile.allow_four_of_kind_seven_pairs;

    let mut best_key: Option<(i8, u16, u8)> = None;
    let mut best_shanten = i8::MAX;
    let mut tied_tiles: Vec<Tile> = Vec::new();

    for (idx, &can_discard) in discardable.iter().enumerate() {
        if !can_discard || hand[idx] == 0 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        hand[idx] -= 1;
        let sh = shanten::compute_shanten(&hand, melds, allow_quad);
        let ukeire = shanten::compute_ukeire(&hand, melds, allow_quad, &remaining);
        hand[idx] += 1;

        let tie_prio = shanten::discard_tie_priority(tile, &pre_hand);
        let key = (-sh, ukeire, tie_prio);

        match best_key {
            None => {
                best_key = Some(key);
                best_shanten = sh;
                tied_tiles.push(tile);
            }
            Some(cur) if key > cur => {
                best_key = Some(key);
                best_shanten = sh;
                tied_tiles.clear();
                tied_tiles.push(tile);
            }
            Some(cur) if key == cur => {
                tied_tiles.push(tile);
            }
            Some(_) => {}
        }
    }

    let best_tile = if tied_tiles.is_empty() {
        let fallback = view
            .legal_actions
            .iter()
            .copied()
            .find(|a| matches!(a, PlayerAction::Discard { .. }))?;
        return Some(DiscardChoice {
            shanten: i8::MAX,
            action: fallback,
        });
    } else if tied_tiles.len() == 1 {
        tied_tiles[0]
    } else {
        tied_tiles[shared::gen_range_usize(rng, tied_tiles.len())]
    };

    let action = l2::resolve_discard_action(view, best_tile)?;
    Some(DiscardChoice {
        shanten: best_shanten,
        action,
    })
}

fn choose_turn_kong(view: &PlayerView, best_discard_shanten: Option<i8>) -> Option<PlayerAction> {
    let mut hand = shanten::hand_counts(view);
    let melds = view.seats[view.viewer.index()].melds.len();
    let allow_quad = view.rule_profile.allow_four_of_kind_seven_pairs;

    let mut best_kong: Option<(i8, PlayerAction)> = None;
    for &action in &view.legal_actions {
        let kong_shanten = match action {
            PlayerAction::ConcealedKong { tile } if hand[tile.index()] >= 4 => {
                hand[tile.index()] -= 4;
                let s = shanten::compute_shanten(&hand, melds + 1, allow_quad);
                hand[tile.index()] += 4;
                s
            }
            PlayerAction::AddedKong { tile } if hand[tile.index()] >= 1 => {
                hand[tile.index()] -= 1;
                let s = shanten::compute_shanten(&hand, melds, allow_quad);
                hand[tile.index()] += 1;
                s
            }
            _ => continue,
        };

        let acceptable = match best_discard_shanten {
            Some(discard_sh) => kong_shanten <= discard_sh,
            None => true,
        };
        if acceptable && best_kong.is_none_or(|(best_sh, _)| kong_shanten < best_sh) {
            best_kong = Some((kong_shanten, action));
        }
    }

    best_kong.map(|(_, action)| action)
}

fn choose_discard_reaction(view: &PlayerView) -> Option<PlayerAction> {
    let mut hand = shanten::hand_counts(view);
    let melds = view.seats[view.viewer.index()].melds.len();
    let allow_quad = view.rule_profile.allow_four_of_kind_seven_pairs;
    let cur_shanten = shanten::compute_shanten(&hand, melds, allow_quad);

    for &action in &view.legal_actions {
        if let PlayerAction::ExposedKong { tile } = action {
            if hand[tile.index()] >= 3 {
                hand[tile.index()] -= 3;
                let after_sh = shanten::compute_shanten(&hand, melds + 1, allow_quad);
                hand[tile.index()] += 3;
                if after_sh < cur_shanten || (after_sh == cur_shanten && after_sh <= 1) {
                    return Some(action);
                }
            }
        }
    }

    for &action in &view.legal_actions {
        if let PlayerAction::Pon { tile } = action {
            if hand[tile.index()] >= 2 {
                hand[tile.index()] -= 2;
                let best_post_pon =
                    best_shanten_after_one_discard(&mut hand, melds + 1, allow_quad);
                hand[tile.index()] += 2;
                if best_post_pon < cur_shanten {
                    return Some(action);
                }
            }
        }
    }

    view.legal_actions
        .contains(&PlayerAction::Pass)
        .then_some(PlayerAction::Pass)
}

fn best_shanten_after_one_discard(
    hand: &mut [u8; Tile::COUNT],
    melds: usize,
    allow_quad: bool,
) -> i8 {
    let mut best = i8::MAX;
    for idx in 0..Tile::COUNT {
        if hand[idx] == 0 {
            continue;
        }
        hand[idx] -= 1;
        let s = shanten::compute_shanten(hand, melds, allow_quad);
        hand[idx] += 1;
        if s < best {
            best = s;
        }
    }
    best
}

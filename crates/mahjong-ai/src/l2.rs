//! Layer 2 (L2): Tactical iron rules, single-player multi-turn expected-value DP,
//! and unified EV evaluation across `Discard`, `Pon`, `ExposedKong`, `AddedKong`,
//! `ConcealedKong`, and `Pass`.

use mahjong_core::{KongKind, PlayerAction, PlayerView, Tile, TurnSource, WinKind};
use rand_core::RngCore;

use crate::scoring::{
    evaluate_win_with_extra, kong_net_delta, MeldSummary, ScoringInput, ScoringRules,
};
use crate::shanten::{
    compute_remaining, compute_shanten, core_counts_to_riichi, discard_tie_priority, hand_counts,
    necessary_tiles_seven_pairs_mask, necessary_tiles_standard_mask,
    necessary_tiles_thirteen_orphans_mask, riichi_mask_to_core, shanten_seven_pairs,
    shanten_standard, shanten_thirteen_orphans, ukeire_from_mask, ShantenBreakdown,
};
use crate::DecisionContext;
use xiangting::{calculate_necessary_tiles, PlayerCount};

/// Maximum `shanten_thirteen_orphans` at which `十三幺` is allowed to compete.
pub const THIRTEEN_ORPHANS_MAX_SHANTEN: i8 = 3;

const SURVIVAL_PER_ROUND: f64 = 0.85;
const WIN_DEFENSE_BONUS_PER_WIN: f64 = 0.92;
const PON_FLEXIBILITY_PENALTY_CLOSED: f64 = 0.02;
const PON_FLEXIBILITY_PENALTY_OPEN: f64 = 0.01;
const EXPOSED_KONG_FLEXIBILITY_PENALTY_CLOSED: f64 = 0.28;
const EXPOSED_KONG_FLEXIBILITY_PENALTY_OPEN: f64 = 0.08;
const EV_TIE_EPS: f64 = 1e-9;

const TERMINAL_OR_HONOR_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

/// Hook supplying expected loss from an opponent's `抢杠胡` when attempting `AddedKong`.
pub trait RobKongRiskModel {
    /// Expected point loss (`>= 0.0`) from `抢杠胡` if `tile` is declared as an `AddedKong`.
    fn rob_kong_risk_ev(&self, view: &PlayerView, tile: Tile) -> f64;
}

/// Default L2 hook assuming zero `抢杠胡` risk (overridden by L3 in later slices).
#[derive(Debug, Clone, Copy, Default)]
pub struct ZeroRobKongRisk;

impl RobKongRiskModel for ZeroRobKongRisk {
    fn rob_kong_risk_ev(&self, _view: &PlayerView, _tile: Tile) -> f64 {
        0.0
    }
}

/// L2 expected-value breakdown for a single deduplicated legal candidate action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateEv {
    /// Concrete legal action (discards pre-resolved per Iron Rule 2 `摸切` preference).
    pub action: PlayerAction,
    /// Total expected net score delta relative to current sunk state (win EV + kong delta - penalties).
    pub ev: f64,
    /// Estimated probability of reaching tenpai (`0.0..=1.0`).
    pub p_tenpai: f64,
    /// Estimated probability of winning the hand (`0.0..=1.0`).
    pub p_win: f64,
    /// Expected winning payout conditioned on winning (`>= 0.0`).
    pub expected_win_score: f64,
    /// Minimum shanten of the resulting closed hand (gated for `十三幺`).
    pub shanten_after: i8,
    /// Live ukeire copies of the resulting closed hand.
    pub ukeire: u16,
    /// Tie-break priority (`discard_tie_priority` for discards, `0` otherwise).
    pub tie_priority: u8,
}

/// Fixed-capacity stack buffer holding candidate evaluations with zero heap allocations.
#[derive(Debug, Clone, Copy)]
pub struct CandidateBuffer {
    items: [CandidateEv; 40],
    len: usize,
}

impl Default for CandidateBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CandidateBuffer {
    /// Creates an empty stack buffer.
    #[must_use]
    pub const fn new() -> Self {
        const EMPTY: CandidateEv = CandidateEv {
            action: PlayerAction::Pass,
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten_after: i8::MAX,
            ukeire: 0,
            tie_priority: 0,
        };
        Self {
            items: [EMPTY; 40],
            len: 0,
        }
    }

    /// Returns the slice of evaluated candidates.
    #[must_use]
    pub fn as_slice(&self) -> &[CandidateEv] {
        &self.items[..self.len]
    }

    /// Returns a mutable slice of evaluated candidates.
    pub fn as_mut_slice(&mut self) -> &mut [CandidateEv] {
        &mut self.items[..self.len]
    }

    fn push(&mut self, cand: CandidateEv) {
        if self.len < self.items.len() {
            self.items[self.len] = cand;
            self.len += 1;
        }
    }
}

/// Applies L2 tactical Iron Rule 1 (`Tsumo`/`RobKongHu`/`Ron`) and `BetweenHands` auto-confirm.
#[must_use]
pub fn fast_path_action(view: &PlayerView) -> Option<PlayerAction> {
    // Iron Rule 1: Always take any legal win immediately.
    if let Some(&win) = view.legal_actions.iter().find(|a| {
        matches!(
            a,
            PlayerAction::Tsumo | PlayerAction::RobKongHu { .. } | PlayerAction::Ron { .. }
        )
    }) {
        return Some(win);
    }

    // BetweenHands: Always confirm next hand; never auto-pick EndMatch.
    if view.legal_actions.contains(&PlayerAction::ConfirmNextHand) {
        return Some(PlayerAction::ConfirmNextHand);
    }

    None
}

/// Resolves a chosen discard `tile` using Iron Rule 2 (`摸切` `from_drawn: true` when legal).
#[must_use]
pub fn resolve_discard_action(view: &PlayerView, best_tile: Tile) -> Option<PlayerAction> {
    if view.hand.drawn_tile == Some(best_tile) {
        let tsumogiri = PlayerAction::Discard {
            tile: best_tile,
            from_drawn: true,
        };
        if view.legal_actions.contains(&tsumogiri) {
            return Some(tsumogiri);
        }
    }

    let shouqie = PlayerAction::Discard {
        tile: best_tile,
        from_drawn: false,
    };
    if view.legal_actions.contains(&shouqie) {
        return Some(shouqie);
    }

    view.legal_actions
        .iter()
        .copied()
        .find(|a| matches!(a, PlayerAction::Discard { tile, .. } if *tile == best_tile))
}

/// Evaluates all legal non-fast-path candidate actions for `view` on a unified EV scale.
#[must_use]
pub fn evaluate_candidates(
    view: &PlayerView,
    risk_model: &dyn RobKongRiskModel,
) -> CandidateBuffer {
    let mut out = CandidateBuffer::new();
    let pre_hand = hand_counts(view);
    let remaining = compute_remaining(view);
    let melds = MeldSummary::from_meld_views(&view.seats[view.viewer.index()].melds);
    let rules = ScoringRules::from_profile(&view.rule_profile);
    let mut caches = DpCaches::new();

    let ctx = EvalEnv {
        remaining: &remaining,
        total_unseen: remaining.iter().map(|&c| u16::from(c)).sum(),
        wall_remaining: view.wall_remaining,
        turns_left: estimated_turns_left(view.wall_remaining),
        rules,
    };

    // 1. Deduplicated Discard candidates (Iron Rule 2 baked into each action).
    let mut discardable = [false; Tile::COUNT];
    for &action in &view.legal_actions {
        if let PlayerAction::Discard { tile, .. } = action {
            discardable[tile.index()] = true;
        }
    }

    let mut work_hand = pre_hand;
    for (idx, &can_discard) in discardable.iter().enumerate() {
        if !can_discard || work_hand[idx] == 0 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        let Some(action) = resolve_discard_action(view, tile) else {
            continue;
        };
        work_hand[idx] -= 1;
        let eval = eval_closed_hand(&work_hand, &melds, &ctx, &mut caches);
        work_hand[idx] += 1;

        out.push(CandidateEv {
            action,
            ev: eval.ev,
            p_tenpai: eval.p_tenpai,
            p_win: eval.p_win,
            expected_win_score: eval.expected_win_score,
            shanten_after: eval.shanten,
            ukeire: eval.ukeire,
            tie_priority: discard_tie_priority(tile, &pre_hand),
        });
    }

    // 2. Turn Kongs (ConcealedKong, AddedKong) and Discard Reactions (ExposedKong, Pon, Pass).
    for &action in &view.legal_actions {
        match action {
            PlayerAction::ConcealedKong { tile } if work_hand[tile.index()] >= 4 => {
                work_hand[tile.index()] -= 4;
                let melds_after = melds.with_added_concealed_kong(tile);
                let cand = evaluate_kong_action(
                    action,
                    &work_hand,
                    &melds_after,
                    f64::from(kong_net_delta(KongKind::ConcealedKong)),
                    0.0,
                    &ctx,
                    &mut caches,
                );
                work_hand[tile.index()] += 4;
                out.push(cand);
            }
            PlayerAction::AddedKong { tile } if work_hand[tile.index()] >= 1 => {
                work_hand[tile.index()] -= 1;
                let melds_after = melds.with_upgraded_added_kong(tile);
                let rob_risk = risk_model.rob_kong_risk_ev(view, tile).max(0.0);
                let cand = evaluate_kong_action(
                    action,
                    &work_hand,
                    &melds_after,
                    f64::from(kong_net_delta(KongKind::AddedKong)),
                    rob_risk,
                    &ctx,
                    &mut caches,
                );
                work_hand[tile.index()] += 1;
                out.push(cand);
            }
            PlayerAction::ExposedKong { tile } if work_hand[tile.index()] >= 3 => {
                let cur_sh = effective_shanten_and_mask(
                    &work_hand,
                    usize::from(melds.count),
                    ctx.rules.allow_four_of_kind_seven_pairs,
                )
                .2;
                work_hand[tile.index()] -= 3;
                let melds_after = melds.with_added_exposed_kong(tile);
                let after_sh = effective_shanten_and_mask(
                    &work_hand,
                    usize::from(melds_after.count),
                    ctx.rules.allow_four_of_kind_seven_pairs,
                )
                .2;
                let shanten_drop_cost = f64::from((after_sh - cur_sh).max(0)) * 0.70;
                let flex_cost = (if melds.is_menqing {
                    EXPOSED_KONG_FLEXIBILITY_PENALTY_CLOSED
                } else {
                    EXPOSED_KONG_FLEXIBILITY_PENALTY_OPEN
                }) + shanten_drop_cost;
                let cand = evaluate_kong_action(
                    action,
                    &work_hand,
                    &melds_after,
                    f64::from(kong_net_delta(KongKind::ExposedKong)),
                    flex_cost,
                    &ctx,
                    &mut caches,
                );
                work_hand[tile.index()] += 3;
                out.push(cand);
            }
            PlayerAction::Pon { tile } if work_hand[tile.index()] >= 2 => {
                work_hand[tile.index()] -= 2;
                let melds_after = melds.with_added_pon(tile);
                let cand = evaluate_pon_action(
                    action,
                    &mut work_hand,
                    &melds,
                    &melds_after,
                    &ctx,
                    &mut caches,
                );
                work_hand[tile.index()] += 2;
                out.push(cand);
            }
            PlayerAction::Pass => {
                let eval = eval_closed_hand(&work_hand, &melds, &ctx, &mut caches);
                out.push(CandidateEv {
                    action,
                    ev: eval.ev,
                    p_tenpai: eval.p_tenpai,
                    p_win: eval.p_win,
                    expected_win_score: eval.expected_win_score,
                    shanten_after: eval.shanten,
                    ukeire: eval.ukeire,
                    tie_priority: 0,
                });
            }
            _ => {}
        }
    }

    out
}

/// Selects the highest-EV candidate action with deterministic secondary and RNG tie-breaking.
#[must_use]
pub fn select_best_candidate<R: RngCore + ?Sized>(
    candidates: &[CandidateEv],
    rng: &mut R,
) -> Option<PlayerAction> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_idx = 0usize;
    let mut tied_indices = [0usize; 40];
    let mut tied_len = 1usize;
    tied_indices[0] = 0;

    for (idx, cand) in candidates.iter().enumerate().skip(1) {
        let cur = &candidates[best_idx];
        let cmp = compare_candidates(cand, cur);
        if cmp > 0 {
            best_idx = idx;
            tied_indices[0] = idx;
            tied_len = 1;
        } else if cmp == 0 && tied_len < tied_indices.len() {
            tied_indices[tied_len] = idx;
            tied_len += 1;
        }
    }

    if tied_len == 1 {
        Some(candidates[tied_indices[0]].action)
    } else {
        let pick = gen_range_usize(rng, tied_len);
        Some(candidates[tied_indices[pick]].action)
    }
}

/// Executes the pure L2 decision pipeline (`Iron Rules -> L2 Multi-Turn EV DP -> Iron Rule 2`).
///
/// # Panics
///
/// Panics if `view.legal_actions` is empty.
#[must_use]
pub fn decide<R: RngCore + ?Sized>(
    view: &PlayerView,
    _ctx: &DecisionContext,
    rng: &mut R,
) -> PlayerAction {
    assert!(
        !view.legal_actions.is_empty(),
        "l2::decide requires non-empty legal_actions"
    );

    if let Some(action) = fast_path_action(view) {
        return action;
    }

    let candidates = evaluate_candidates(view, &ZeroRobKongRisk);
    select_best_candidate(candidates.as_slice(), rng).unwrap_or(view.legal_actions[0])
}

fn compare_candidates(a: &CandidateEv, b: &CandidateEv) -> i8 {
    let diff = a.ev - b.ev;
    if diff > EV_TIE_EPS {
        return 1;
    }
    if diff < -EV_TIE_EPS {
        return -1;
    }

    let key_a = (-a.shanten_after, a.ukeire, a.tie_priority);
    let key_b = (-b.shanten_after, b.ukeire, b.tie_priority);
    if key_a > key_b {
        1
    } else if key_a < key_b {
        -1
    } else {
        0
    }
}

struct EvalEnv<'a> {
    remaining: &'a [u8; Tile::COUNT],
    total_unseen: u16,
    wall_remaining: u8,
    turns_left: usize,
    rules: ScoringRules,
}

#[derive(Clone, Copy)]
struct HandEval {
    ev: f64,
    p_tenpai: f64,
    p_win: f64,
    expected_win_score: f64,
    shanten: i8,
    ukeire: u16,
}

#[derive(Clone, Copy, Default)]
struct TenpaiSummary {
    has_wait: bool,
    h_win_round: f64,
    e_score_given_win: f64,
}

#[derive(Clone, Copy)]
struct S0CacheEntry {
    key: u128,
    occupied: bool,
    summary: TenpaiSummary,
}

#[derive(Clone, Copy)]
struct S1CacheEntry {
    key: u128,
    occupied: bool,
    p_step_1: f64,
    p_win_round_est: f64,
    e_win_score: f64,
}

#[derive(Clone, Copy)]
struct ShCacheEntry {
    key: u128,
    occupied: bool,
    shanten: i8,
}

struct DpCaches {
    s0: [S0CacheEntry; 256],
    s1: [S1CacheEntry; 512],
    sh: [ShCacheEntry; 1024],
}

impl DpCaches {
    fn new() -> Self {
        Self {
            s0: [S0CacheEntry {
                key: 0,
                occupied: false,
                summary: TenpaiSummary {
                    has_wait: false,
                    h_win_round: 0.0,
                    e_score_given_win: 0.0,
                },
            }; 256],
            s1: [S1CacheEntry {
                key: 0,
                occupied: false,
                p_step_1: 0.0,
                p_win_round_est: 0.0,
                e_win_score: 0.0,
            }; 512],
            sh: [ShCacheEntry {
                key: 0,
                occupied: false,
                shanten: 0,
            }; 1024],
        }
    }
}

fn pack_counts(counts: &[u8; Tile::COUNT]) -> u128 {
    let mut key = 0u128;
    for (idx, &c) in counts.iter().enumerate() {
        key |= u128::from(c) << (idx * 3);
    }
    key
}

fn cache_slot(key: u128, len: usize) -> usize {
    let mixed = (key as u64) ^ ((key >> 64) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (mixed as usize) & (len - 1)
}

fn estimated_turns_left(wall_remaining: u8) -> usize {
    if wall_remaining == 0 {
        0
    } else {
        usize::from(wall_remaining / 4).max(1)
    }
}

fn evaluate_kong_action(
    action: PlayerAction,
    hand_after: &[u8; Tile::COUNT],
    melds_after: &MeldSummary,
    kong_delta: f64,
    penalty: f64,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> CandidateEv {
    let (bd, waits_mask, eff_sh) = effective_shanten_and_mask(
        hand_after,
        usize::from(melds_after.count),
        ctx.rules.allow_four_of_kind_seven_pairs,
    );
    let ukeire = ukeire_from_mask(waits_mask, ctx.remaining);

    // Replacement draw immediate 杠上开花 upside when already tenpai (eff_sh == 0).
    let (p_rinshan, ev_rinshan) = if eff_sh == 0 && ctx.wall_remaining > 0 {
        compute_rinshan_ev(hand_after, melds_after, waits_mask, ctx)
    } else {
        (0.0, 0.0)
    };

    let future_eval =
        eval_closed_hand_with_bd(hand_after, melds_after, bd, waits_mask, eff_sh, ctx, caches);
    let total_p_win = (p_rinshan + (1.0 - p_rinshan) * future_eval.p_win).min(1.0);
    let win_ev = ev_rinshan + (1.0 - p_rinshan) * future_eval.ev;
    let net_ev = kong_delta - penalty + win_ev;
    let cond_win_score = if total_p_win > 1e-6 {
        win_ev / total_p_win
    } else {
        future_eval.expected_win_score
    };

    CandidateEv {
        action,
        ev: net_ev,
        p_tenpai: future_eval.p_tenpai,
        p_win: total_p_win,
        expected_win_score: cond_win_score,
        shanten_after: eff_sh,
        ukeire,
        tie_priority: 0,
    }
}

pub(crate) fn evaluate_pon_with_scorer<F>(
    view: &PlayerView,
    pon_tile: Tile,
    utility_fn: F,
) -> Option<CandidateEv>
where
    F: FnMut(&CandidateEv, Tile) -> f64,
{
    let mut work_hand = hand_counts(view);
    if work_hand[pon_tile.index()] < 2 {
        return None;
    }
    let remaining = compute_remaining(view);
    let melds_before = MeldSummary::from_meld_views(&view.seats[view.viewer.index()].melds);
    let melds_after = melds_before.with_added_pon(pon_tile);
    let rules = ScoringRules::from_profile(&view.rule_profile);
    let mut caches = DpCaches::new();
    let ctx = EvalEnv {
        remaining: &remaining,
        total_unseen: remaining.iter().map(|&c| u16::from(c)).sum(),
        wall_remaining: view.wall_remaining,
        turns_left: estimated_turns_left(view.wall_remaining),
        rules,
    };

    work_hand[pon_tile.index()] -= 2;
    let cand = evaluate_pon_inner(
        PlayerAction::Pon { tile: pon_tile },
        &mut work_hand,
        &melds_before,
        &melds_after,
        &ctx,
        &mut caches,
        utility_fn,
    );
    Some(cand)
}

fn evaluate_pon_action(
    action: PlayerAction,
    work_hand: &mut [u8; Tile::COUNT],
    melds_before: &MeldSummary,
    melds_after: &MeldSummary,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> CandidateEv {
    evaluate_pon_inner(
        action,
        work_hand,
        melds_before,
        melds_after,
        ctx,
        caches,
        |cand, _| cand.ev,
    )
}

fn evaluate_pon_inner<F>(
    action: PlayerAction,
    work_hand: &mut [u8; Tile::COUNT],
    melds_before: &MeldSummary,
    melds_after: &MeldSummary,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
    mut utility_fn: F,
) -> CandidateEv
where
    F: FnMut(&CandidateEv, Tile) -> f64,
{
    let flex_penalty = if melds_before.is_menqing {
        PON_FLEXIBILITY_PENALTY_CLOSED
    } else {
        PON_FLEXIBILITY_PENALTY_OPEN
    };

    let mut best: Option<(f64, CandidateEv)> = None;
    for d_idx in 0..Tile::COUNT {
        if work_hand[d_idx] == 0 {
            continue;
        }
        let discard_tile = Tile::new(d_idx as u8).expect("idx bounded by Tile::COUNT");
        work_hand[d_idx] -= 1;
        let eval = eval_closed_hand(work_hand, melds_after, ctx, caches);
        work_hand[d_idx] += 1;

        let mut cand = CandidateEv {
            action,
            ev: eval.ev - flex_penalty,
            p_tenpai: eval.p_tenpai,
            p_win: eval.p_win,
            expected_win_score: eval.expected_win_score,
            shanten_after: eval.shanten,
            ukeire: eval.ukeire,
            tie_priority: 0,
        };
        let utility = utility_fn(&cand, discard_tile);
        cand.ev = utility;

        if best.is_none_or(|(prev_u, prev_c)| {
            (utility > prev_u + EV_TIE_EPS)
                || ((utility - prev_u).abs() <= EV_TIE_EPS
                    && (-cand.shanten_after, cand.ukeire) > (-prev_c.shanten_after, prev_c.ukeire))
        }) {
            best = Some((utility, cand));
        }
    }

    best.map(|(_, cand)| cand).unwrap_or(CandidateEv {
        action,
        ev: -flex_penalty,
        p_tenpai: 0.0,
        p_win: 0.0,
        expected_win_score: 0.0,
        shanten_after: i8::MAX,
        ukeire: 0,
        tie_priority: 0,
    })
}

fn compute_rinshan_ev(
    hand_after: &[u8; Tile::COUNT],
    melds_after: &MeldSummary,
    waits_mask: u64,
    ctx: &EvalEnv<'_>,
) -> (f64, f64) {
    let n_u = f64::from(ctx.total_unseen.max(1));
    let rinshan_wall = ctx.wall_remaining.saturating_sub(1);
    let input = ScoringInput {
        win_kind: WinKind::Tsumo,
        turn_source: Some(TurnSource::ReplacementDraw),
        wall_remaining: rinshan_wall,
    };

    let mut p_win = 0.0f64;
    let mut ev = 0.0f64;
    let mut bits = waits_mask & ((1u64 << Tile::COUNT) - 1);
    while bits != 0 {
        let idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        let rem = ctx.remaining[idx];
        if rem == 0 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        if let Some(win_eval) =
            evaluate_win_with_extra(hand_after, tile, melds_after, input, ctx.rules)
        {
            let p_tile = f64::from(rem) / n_u;
            p_win += p_tile;
            ev += p_tile * f64::from(win_eval.winner_payout(WinKind::Tsumo));
        }
    }
    (p_win.min(1.0), ev)
}

fn effective_shanten_and_mask(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_quad: bool,
) -> (ShantenBreakdown, u64, i8) {
    let melds = existing_melds.min(4);
    if melds == 0 {
        let seven_pairs = shanten_seven_pairs(counts, allow_quad);
        let thirteen_orphans = shanten_thirteen_orphans(counts);
        let include_13o = thirteen_orphans <= THIRTEEN_ORPHANS_MAX_SHANTEN;

        // Fast path via xiangting avoids `standard_exact_dp` when combined min is needed.
        let riichi = core_counts_to_riichi(counts);
        if let Ok((rn, r_mask)) = calculate_necessary_tiles(&riichi, PlayerCount::Four) {
            let riichi_min = (rn as i8) - 1;
            if include_13o || riichi_min < thirteen_orphans {
                let core_riichi_mask = riichi_mask_to_core(r_mask);
                let eff_min = if allow_quad {
                    riichi_min.min(seven_pairs)
                } else {
                    riichi_min
                };
                let mut mask = 0u64;
                if riichi_min == eff_min {
                    mask |= core_riichi_mask;
                }
                if allow_quad && seven_pairs == eff_min {
                    mask |= necessary_tiles_seven_pairs_mask(counts, true);
                }
                let standard = if riichi_min < shanten_seven_pairs(counts, false)
                    && riichi_min < thirteen_orphans
                {
                    riichi_min
                } else {
                    riichi_min.saturating_add(1)
                };
                let bd = ShantenBreakdown {
                    standard,
                    seven_pairs: Some(seven_pairs),
                    thirteen_orphans: Some(thirteen_orphans),
                    min: eff_min,
                };
                return (bd, mask, eff_min);
            }
        }

        let standard = shanten_standard(counts, 0);
        let eff_min = if include_13o {
            standard.min(seven_pairs).min(thirteen_orphans)
        } else {
            standard.min(seven_pairs)
        };
        let mut mask = 0u64;
        if standard == eff_min {
            mask |= necessary_tiles_standard_mask(counts, 0);
        }
        if seven_pairs == eff_min {
            mask |= necessary_tiles_seven_pairs_mask(counts, allow_quad);
        }
        if include_13o && thirteen_orphans == eff_min {
            mask |= necessary_tiles_thirteen_orphans_mask(counts);
        }
        let bd = ShantenBreakdown {
            standard,
            seven_pairs: Some(seven_pairs),
            thirteen_orphans: Some(thirteen_orphans),
            min: eff_min,
        };
        (bd, mask, eff_min)
    } else {
        let standard = shanten_standard(counts, melds);
        let mask = necessary_tiles_standard_mask(counts, melds);
        let bd = ShantenBreakdown {
            standard,
            seven_pairs: None,
            thirteen_orphans: None,
            min: standard,
        };
        (bd, mask, standard)
    }
}

fn fast_shanten_for_low_shanten(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_quad: bool,
    caches: &mut DpCaches,
) -> i8 {
    let key = pack_counts(counts) ^ (u128::from(existing_melds as u8) << 110);
    let slot = cache_slot(key, caches.sh.len());
    let entry = caches.sh[slot];
    if entry.occupied && entry.key == key {
        return entry.shanten;
    }
    // At s <= 2 (`s0 == 0` or `s1 == 1`), `13o` is already `<= 2 <= 3`, so `compute_shanten`
    // is exact and calls `xiangting` directly without `standard_exact_dp`.
    let sh = compute_shanten(counts, existing_melds, allow_quad);
    caches.sh[slot] = ShCacheEntry {
        key,
        occupied: true,
        shanten: sh,
    };
    sh
}

fn eval_closed_hand(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> HandEval {
    let (bd, waits_mask, eff_sh) = effective_shanten_and_mask(
        hand,
        usize::from(melds.count),
        ctx.rules.allow_four_of_kind_seven_pairs,
    );
    eval_closed_hand_with_bd(hand, melds, bd, waits_mask, eff_sh, ctx, caches)
}

fn eval_closed_hand_with_bd(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    bd: ShantenBreakdown,
    waits_mask: u64,
    eff_sh: i8,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> HandEval {
    let ukeire = ukeire_from_mask(waits_mask, ctx.remaining);
    match eff_sh {
        ..=0 => eval_s0(hand, melds, waits_mask, ukeire, ctx, caches),
        1 => eval_s1(hand, melds, bd, waits_mask, ukeire, ctx, caches),
        2 => eval_s2(hand, melds, bd, waits_mask, ukeire, ctx, caches),
        _ => eval_s_ge_3(hand, melds, bd, waits_mask, eff_sh, ukeire, ctx),
    }
}

fn compute_tenpai_summary(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    waits_mask: u64,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> TenpaiSummary {
    let key = pack_counts(hand);
    let slot = cache_slot(key, caches.s0.len());
    let entry = caches.s0[slot];
    if entry.occupied && entry.key == key {
        return entry.summary;
    }

    let n_u = f64::from(ctx.total_unseen.max(1));
    let safe_wall = ctx.wall_remaining.max(1);
    let tsumo_in = ScoringInput {
        win_kind: WinKind::Tsumo,
        turn_source: Some(TurnSource::NormalDraw),
        wall_remaining: safe_wall,
    };
    let ron_in = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: safe_wall,
    };
    let last_tsumo_in = ScoringInput {
        win_kind: WinKind::Tsumo,
        turn_source: Some(TurnSource::NormalDraw),
        wall_remaining: 0,
    };
    let last_ron_in = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: 0,
    };
    let last_tile_weight = if ctx.wall_remaining <= 4 && ctx.wall_remaining > 0 {
        0.5 / f64::from(ctx.wall_remaining)
    } else {
        0.0
    };

    let mut has_wait = false;
    let mut sum_weight = 0.0f64;
    let mut sum_weighted_payout = 0.0f64;

    let mut bits = waits_mask & ((1u64 << Tile::COUNT) - 1);
    while bits != 0 {
        let idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        has_wait = true;
        let rem = ctx.remaining[idx];
        if rem == 0 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        let Some(tsumo_eval) = evaluate_win_with_extra(hand, tile, melds, tsumo_in, ctx.rules)
        else {
            continue;
        };
        let Some(ron_eval) = evaluate_win_with_extra(hand, tile, melds, ron_in, ctx.rules) else {
            continue;
        };

        let mut tsumo_payout = f64::from(tsumo_eval.winner_payout(WinKind::Tsumo));
        let mut ron_payout = f64::from(ron_eval.winner_payout(WinKind::Ron));
        if last_tile_weight > 0.0 {
            if let Some(lt) = evaluate_win_with_extra(hand, tile, melds, last_tsumo_in, ctx.rules) {
                tsumo_payout = (1.0 - last_tile_weight) * tsumo_payout
                    + last_tile_weight * f64::from(lt.winner_payout(WinKind::Tsumo));
            }
            if let Some(lr) = evaluate_win_with_extra(hand, tile, melds, last_ron_in, ctx.rules) {
                ron_payout = (1.0 - last_tile_weight) * ron_payout
                    + last_tile_weight * f64::from(lr.winner_payout(WinKind::Ron));
            }
        }

        let r_f = f64::from(rem);
        let wt_tsumo = r_f;
        let wt_ron = r_f * ron_discard_multiplier(tile);
        sum_weight += wt_tsumo + wt_ron;
        sum_weighted_payout += wt_tsumo * tsumo_payout + wt_ron * ron_payout;
    }

    let summary = TenpaiSummary {
        has_wait,
        h_win_round: (sum_weight / n_u).min(0.95),
        e_score_given_win: if sum_weight > 0.0 {
            sum_weighted_payout / sum_weight
        } else {
            0.0
        },
    };
    caches.s0[slot] = S0CacheEntry {
        key,
        occupied: true,
        summary,
    };
    summary
}

fn ron_discard_multiplier(tile: Tile) -> f64 {
    if tile.is_honor() {
        2.60
    } else if tile.is_terminal() {
        2.30
    } else if matches!(tile.rank(), Some(2 | 8)) {
        2.00
    } else {
        1.75
    }
}

fn tenpai_win_prob(summary: TenpaiSummary, turns: usize, include_last_tail: bool) -> f64 {
    if summary.h_win_round <= 0.0 {
        return 0.0;
    }
    if turns == 0 {
        return if include_last_tail {
            (0.35 * summary.h_win_round).min(0.95)
        } else {
            0.0
        };
    }
    let h = summary.h_win_round;
    let stay = (1.0 - h) * SURVIVAL_PER_ROUND;
    let mut p_win = if include_last_tail {
        (0.35 * h).min(0.95)
    } else {
        0.0
    };
    for _ in 0..turns {
        p_win = h + stay * p_win;
    }
    p_win.min(1.0)
}

fn eval_s0(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    waits_mask: u64,
    ukeire: u16,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> HandEval {
    let summary = compute_tenpai_summary(hand, melds, waits_mask, ctx, caches);
    let p_tenpai = if summary.has_wait { 1.0 } else { 0.0 };
    let p_win = tenpai_win_prob(summary, ctx.turns_left, false);
    let ev = p_win * (summary.e_score_given_win + WIN_DEFENSE_BONUS_PER_WIN);
    HandEval {
        ev,
        p_tenpai,
        p_win,
        expected_win_score: summary.e_score_given_win,
        shanten: 0,
        ukeire,
    }
}

fn eval_s1(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    _bd: ShantenBreakdown,
    waits_mask: u64,
    ukeire: u16,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> HandEval {
    if ctx.turns_left == 0 || ukeire == 0 {
        return HandEval {
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten: 1,
            ukeire,
        };
    }

    let allow_quad = ctx.rules.allow_four_of_kind_seven_pairs;

    let mut transitions: [(f64, TenpaiSummary); 34] = [(0.0, TenpaiSummary::default()); 34];
    let mut trans_len = 0usize;
    let mut sum_p_improve = 0.0f64;
    let n_u = f64::from(ctx.total_unseen.max(1));

    let mut work = *hand;
    let mut bits = waits_mask & ((1u64 << Tile::COUNT) - 1);
    while bits != 0 {
        let x_idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        let rem_x = ctx.remaining[x_idx];
        if rem_x == 0 {
            continue;
        }
        work[x_idx] += 1;
        let mut best_summary: Option<TenpaiSummary> = None;
        let mut best_ev_ref = -1.0f64;

        for d_idx in 0..Tile::COUNT {
            if work[d_idx] == 0 || d_idx == x_idx {
                continue;
            }
            work[d_idx] -= 1;
            let sh0 =
                fast_shanten_for_low_shanten(&work, usize::from(melds.count), allow_quad, caches);
            if sh0 == 0 {
                let (_, mask0, _) =
                    effective_shanten_and_mask(&work, usize::from(melds.count), allow_quad);
                let sum0 = compute_tenpai_summary(&work, melds, mask0, ctx, caches);
                let ref_ev = tenpai_win_prob(sum0, ctx.turns_left.saturating_sub(1), true)
                    * sum0.e_score_given_win;
                if best_summary.is_none() || ref_ev > best_ev_ref {
                    best_ev_ref = ref_ev;
                    best_summary = Some(sum0);
                }
            }
            work[d_idx] += 1;
        }

        work[x_idx] -= 1;
        if let Some(sum0) = best_summary {
            let p_x = f64::from(rem_x) / n_u;
            transitions[trans_len] = (p_x, sum0);
            trans_len += 1;
            sum_p_improve += p_x;
        }
    }

    if trans_len == 0 {
        return HandEval {
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten: 1,
            ukeire,
        };
    }

    let scale = if sum_p_improve > 0.95 {
        0.95 / sum_p_improve
    } else {
        1.0
    };
    let p_stay = (1.0 - sum_p_improve * scale).max(0.0);

    let mut p_tenpai = 0.0f64;
    let mut p_win = 0.0f64;
    let mut ev = 0.0f64;
    let include_tail = ctx.wall_remaining >= 2;

    for t in 1..=ctx.turns_left {
        let mut step_tenpai = 0.0f64;
        let mut step_win = 0.0f64;
        let mut step_ev = 0.0f64;
        for &(p_x, sum0) in &transitions[..trans_len] {
            let w_x = p_x * scale;
            let pw = tenpai_win_prob(sum0, t - 1, include_tail);
            step_tenpai += w_x;
            step_win += w_x * pw;
            step_ev += w_x * pw * sum0.e_score_given_win;
        }
        p_tenpai = SURVIVAL_PER_ROUND * (step_tenpai + p_stay * p_tenpai);
        p_win = SURVIVAL_PER_ROUND * (step_win + p_stay * p_win);
        ev = SURVIVAL_PER_ROUND * (step_ev + p_stay * ev);
    }

    let cond_score = if p_win > 1e-9 { ev / p_win } else { 0.0 };
    let total_ev = ev + p_win * WIN_DEFENSE_BONUS_PER_WIN;
    HandEval {
        ev: total_ev,
        p_tenpai: p_tenpai.min(1.0),
        p_win: p_win.min(1.0),
        expected_win_score: cond_score,
        shanten: 1,
        ukeire,
    }
}

fn eval_s2(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    _bd: ShantenBreakdown,
    waits_mask: u64,
    ukeire: u16,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> HandEval {
    if ctx.turns_left == 0 || ukeire == 0 {
        return HandEval {
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten: 2,
            ukeire,
        };
    }

    let allow_quad = ctx.rules.allow_four_of_kind_seven_pairs;

    let mut transitions: [(f64, f64, f64, f64); 34] = [(0.0, 0.0, 0.0, 0.0); 34];
    let mut trans_len = 0usize;
    let mut sum_p_improve = 0.0f64;
    let n_u = f64::from(ctx.total_unseen.max(1));

    let mut work = *hand;
    let mut bits = waits_mask & ((1u64 << Tile::COUNT) - 1);
    while bits != 0 {
        let x_idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        let rem_x = ctx.remaining[x_idx];
        if rem_x == 0 {
            continue;
        }

        work[x_idx] += 1;
        let mut best_s1: Option<(f64, f64, f64)> = None;
        let mut best_ref_ev = -1.0f64;

        for d_idx in 0..Tile::COUNT {
            if work[d_idx] == 0 || d_idx == x_idx {
                continue;
            }
            work[d_idx] -= 1;
            let sh1 =
                fast_shanten_for_low_shanten(&work, usize::from(melds.count), allow_quad, caches);
            if sh1 == 1 {
                let (p_s1, p_w0, e_sc) = eval_s1_analytical(&work, melds, ctx, caches);
                let (_, _, ref_ev) =
                    analytical_s1_over_turns(p_s1, p_w0, e_sc, ctx.turns_left.saturating_sub(1));
                if best_s1.is_none() || ref_ev > best_ref_ev {
                    best_ref_ev = ref_ev;
                    best_s1 = Some((p_s1, p_w0, e_sc));
                }
            }
            work[d_idx] += 1;
        }

        work[x_idx] -= 1;
        if let Some((p_s1, p_w0, e_sc)) = best_s1 {
            let p_x = f64::from(rem_x) / n_u;
            transitions[trans_len] = (p_x, p_s1, p_w0, e_sc);
            trans_len += 1;
            sum_p_improve += p_x;
        }
    }

    if trans_len == 0 {
        return HandEval {
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten: 2,
            ukeire,
        };
    }

    let scale = if sum_p_improve > 0.95 {
        0.95 / sum_p_improve
    } else {
        1.0
    };
    let p_stay = (1.0 - sum_p_improve * scale).max(0.0);

    let mut p_tenpai = 0.0f64;
    let mut p_win = 0.0f64;
    let mut ev = 0.0f64;

    for t in 1..=ctx.turns_left {
        let mut step_tenpai = 0.0f64;
        let mut step_win = 0.0f64;
        let mut step_ev = 0.0f64;
        for &(p_x, p_s1, p_w0, e_sc) in &transitions[..trans_len] {
            let w_x = p_x * scale;
            let (pt, pw, ev_sub) = analytical_s1_over_turns(p_s1, p_w0, e_sc, t - 1);
            step_tenpai += w_x * pt;
            step_win += w_x * pw;
            step_ev += w_x * ev_sub;
        }
        p_tenpai = SURVIVAL_PER_ROUND * (step_tenpai + p_stay * p_tenpai);
        p_win = SURVIVAL_PER_ROUND * (step_win + p_stay * p_win);
        ev = SURVIVAL_PER_ROUND * (step_ev + p_stay * ev);
    }

    // Apply conservative 0.85 realization discount at 2-shanten so breaking a 1-shanten
    // hand back to 2-shanten only happens for a genuine expected-fan upgrade.
    p_win *= 0.85;
    ev *= 0.85;
    let cond_score = if p_win > 1e-9 { ev / p_win } else { 0.0 };
    let total_ev = ev + p_win * WIN_DEFENSE_BONUS_PER_WIN;
    HandEval {
        ev: total_ev,
        p_tenpai: p_tenpai.min(1.0),
        p_win: p_win.min(1.0),
        expected_win_score: cond_score,
        shanten: 2,
        ukeire,
    }
}

fn eval_s1_analytical(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    ctx: &EvalEnv<'_>,
    caches: &mut DpCaches,
) -> (f64, f64, f64) {
    let key = pack_counts(hand);
    let slot = cache_slot(key, caches.s1.len());
    let entry = caches.s1[slot];
    if entry.occupied && entry.key == key {
        return (entry.p_step_1, entry.p_win_round_est, entry.e_win_score);
    }

    let (bd, waits_mask, _) = effective_shanten_and_mask(
        hand,
        usize::from(melds.count),
        ctx.rules.allow_four_of_kind_seven_pairs,
    );
    let uk1 = ukeire_from_mask(waits_mask, ctx.remaining);
    let n_u = f64::from(ctx.total_unseen.max(1));
    let p_step_1 = (f64::from(uk1) / n_u).min(0.90);

    // Conservative average tenpai wait rate (~3.2 effective outs including Ron rate).
    let est_wait_copies =
        if melds.count == 0 && (bd.thirteen_orphans == Some(1) || bd.seven_pairs == Some(1)) {
            2.5
        } else {
            3.2
        };
    let p_win_round_est = ((est_wait_copies * 2.5) / n_u).min(0.35);
    let e_win_score = estimate_expected_win_score(hand, melds, ctx.remaining, bd, ctx.rules);

    caches.s1[slot] = S1CacheEntry {
        key,
        occupied: true,
        p_step_1,
        p_win_round_est,
        e_win_score,
    };
    (p_step_1, p_win_round_est, e_win_score)
}

fn analytical_s1_over_turns(
    p_step_1: f64,
    p_win_round: f64,
    e_win_score: f64,
    turns: usize,
) -> (f64, f64, f64) {
    if turns == 0 || p_step_1 <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let stay_1 = (1.0 - p_step_1) * SURVIVAL_PER_ROUND;
    let stay_0 = (1.0 - p_win_round) * SURVIVAL_PER_ROUND;
    let mut p_tenpai = 0.0f64;
    let mut p_win = 0.0f64;
    let mut p_win_from_0 = 0.0f64;
    for _ in 0..turns {
        p_tenpai = SURVIVAL_PER_ROUND * p_step_1 + stay_1 * p_tenpai;
        p_win = SURVIVAL_PER_ROUND * p_step_1 * p_win_from_0 + stay_1 * p_win;
        p_win_from_0 = p_win_round + stay_0 * p_win_from_0;
    }
    (p_tenpai.min(1.0), p_win.min(1.0), p_win * e_win_score)
}

fn eval_s_ge_3(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    bd: ShantenBreakdown,
    waits_mask: u64,
    eff_sh: i8,
    ukeire: u16,
    ctx: &EvalEnv<'_>,
) -> HandEval {
    if ctx.turns_left == 0 {
        return HandEval {
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten: eff_sh,
            ukeire,
        };
    }

    let n_u = f64::from(ctx.total_unseen.max(1));
    let two_step = compute_two_step_connectivity(hand, ctx.remaining);
    let eff_accept = f64::from(ukeire) + 0.16 * two_step;
    let step_rate = (eff_accept / n_u).clamp(0.0, 0.90);
    let e_win_score = estimate_expected_win_score(hand, melds, ctx.remaining, bd, ctx.rules);

    let horizon_factor = 1.0 - SURVIVAL_PER_ROUND.powi(ctx.turns_left as i32);
    // Monotonically penalize distance in shanten while rewarding live 2-step acceptance
    // and expected fan potential, ensuring `s=3` never loses to bloated `s=4`/`s=5` ukeire.
    let reach_prob =
        (0.28_f64).powi(i32::from(eff_sh)) * (0.25 + 0.75 * step_rate) * horizon_factor;
    let p_win = reach_prob.min(0.95);
    let dampened_score = if e_win_score > 2.0 {
        2.0 + 0.35 * (e_win_score - 2.0)
    } else {
        e_win_score
    };
    let ev = p_win * (dampened_score + WIN_DEFENSE_BONUS_PER_WIN)
        + 1e-5 * f64::from(ukeire)
        + 2e-6 * two_step;

    // Encourage keeping 1-shanten / 2-step value only softly relative to true shanten progress.
    let _ = waits_mask;
    HandEval {
        ev,
        p_tenpai: reach_prob.min(1.0),
        p_win,
        expected_win_score: e_win_score,
        shanten: eff_sh,
        ukeire,
    }
}

fn compute_two_step_connectivity(hand: &[u8; Tile::COUNT], remaining: &[u8; Tile::COUNT]) -> f64 {
    let mut score = 0.0f64;
    for (idx, &count) in hand.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let weight = f64::from(count.min(2));
        score += weight * f64::from(remaining[idx]) * 0.6;
        if idx < 27 {
            let suit_base = (idx / 9) * 9;
            let rank = idx % 9;
            if rank >= 1 {
                score += weight * f64::from(remaining[suit_base + rank - 1]) * 0.8;
            }
            if rank + 1 < 9 {
                score += weight * f64::from(remaining[suit_base + rank + 1]) * 0.8;
            }
            if rank >= 2 {
                score += weight * f64::from(remaining[suit_base + rank - 2]) * 0.35;
            }
            if rank + 2 < 9 {
                score += weight * f64::from(remaining[suit_base + rank + 2]) * 0.35;
            }
        }
    }
    score
}

fn estimate_expected_win_score(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    remaining: &[u8; Tile::COUNT],
    bd: ShantenBreakdown,
    rules: ScoringRules,
) -> f64 {
    let mut total = [0u8; Tile::COUNT];
    for idx in 0..Tile::COUNT {
        total[idx] = hand[idx].saturating_add(melds.counts[idx]);
    }

    let suit_counts = [
        total[0..9].iter().map(|&c| usize::from(c)).sum::<usize>(),
        total[9..18].iter().map(|&c| usize::from(c)).sum::<usize>(),
        total[18..27].iter().map(|&c| usize::from(c)).sum::<usize>(),
    ];
    let meld_suits = [
        melds.counts[0..9].iter().any(|&c| c > 0),
        melds.counts[9..18].iter().any(|&c| c > 0),
        melds.counts[18..27].iter().any(|&c| c > 0),
    ];
    let meld_has_honor = melds.counts[27..Tile::COUNT].iter().any(|&c| c > 0);
    let honor_cnt: usize = total[27..Tile::COUNT].iter().map(|&c| usize::from(c)).sum();
    let total_tiles: usize = suit_counts[0] + suit_counts[1] + suit_counts[2] + honor_cnt;

    // 1. Standard form expected fan & payout.
    let menqing_fan = if melds.is_menqing { 1.0 } else { 0.0 };

    // All Honors (字一色 = 4 fan) vs Full Flush (清一色 = +2) vs Half Flush (混一色 = +1).
    let mut best_flush_fan = 0.0f64;
    if !meld_suits[0] && !meld_suits[1] && !meld_suits[2] {
        let non_honors = total_tiles.saturating_sub(honor_cnt);
        let honor_prob = match non_honors {
            0 => 1.0,
            1 if bd.standard <= 2 => 0.40,
            2 if bd.standard <= 2 => 0.12,
            _ => 0.0,
        };
        best_flush_fan = best_flush_fan.max(4.0 * honor_prob);
    }
    for s in 0..3 {
        let other_meld_suit = meld_suits[(s + 1) % 3] || meld_suits[(s + 2) % 3];
        if other_meld_suit {
            continue;
        }
        let off_suit = total_tiles.saturating_sub(suit_counts[s]);
        let off_num = suit_counts[(s + 1) % 3] + suit_counts[(s + 2) % 3];
        let full_p = if meld_has_honor {
            0.0
        } else {
            match off_suit {
                0 => 1.0,
                1 => 0.65,
                2 if suit_counts[s] >= 10 => 0.25,
                3 if suit_counts[s] >= 9 => 0.10,
                4 if suit_counts[s] >= 8 => 0.04,
                _ => 0.0,
            }
        };
        let half_p = if honor_cnt == 0 {
            0.0
        } else {
            match off_num {
                0 => (1.0_f64 - full_p).max(0.0),
                1 if suit_counts[s] + honor_cnt >= 10 => 0.50,
                2 if suit_counts[s] + honor_cnt >= 10 => 0.16,
                3 if suit_counts[s] + honor_cnt >= 9 => 0.08,
                4 if suit_counts[s] + honor_cnt >= 8 => 0.03,
                _ => 0.0,
            }
        };
        best_flush_fan = best_flush_fan.max(2.0 * full_p + 1.0 * half_p);
    }

    // All Pungs (碰碰胡 = +1).
    let all_pungs_fan = if melds.all_pungs_or_kongs {
        let mut triplets = usize::from(melds.count);
        let mut live_pairs = 0usize;
        let mut total_pairs = 0usize;
        for idx in 0..Tile::COUNT {
            if hand[idx] >= 3 {
                triplets += 1;
            } else if hand[idx] == 2 {
                total_pairs += 1;
                if remaining[idx] > 0 {
                    live_pairs += 1;
                }
            }
        }
        if triplets == 4 && total_pairs == 1 {
            1.0
        } else if triplets == 3 && live_pairs >= 1 && total_pairs >= 2 {
            0.65
        } else if triplets == 2 && live_pairs >= 2 && total_pairs >= 3 {
            0.25
        } else if triplets == 1 && live_pairs >= 2 && total_pairs >= 4 {
            0.12
        } else if triplets == 0 && live_pairs >= 3 && total_pairs >= 4 {
            0.08
        } else {
            0.0
        }
    } else {
        0.0
    };

    let std_fan = (menqing_fan + best_flush_fan + all_pungs_fan).min(f64::from(rules.fan_cap));
    let std_base = (2.0_f64).powf(std_fan);

    // 2. Seven Pairs / Luxury Seven Pairs (when melds == 0).
    let mut seven_base = 0.0f64;
    let mut seven_weight = 0.0f64;
    if melds.count == 0 {
        if let Some(s7) = bd.seven_pairs {
            if s7 <= bd.standard + 1 && s7 <= 3 {
                let has_quad = rules.allow_four_of_kind_seven_pairs && hand.contains(&4);
                let quad_pot = rules.allow_four_of_kind_seven_pairs
                    && hand
                        .iter()
                        .enumerate()
                        .any(|(i, &c)| c == 3 && remaining[i] > 0);
                let pair_fan = if has_quad {
                    2.0
                } else if quad_pot && s7 <= 1 {
                    1.35
                } else {
                    1.0
                };
                // Seven pairs can stack with flush/all-honors (not menqing).
                let total_7p_fan = (pair_fan + best_flush_fan).min(f64::from(rules.fan_cap));
                seven_base = (2.0_f64).powf(total_7p_fan);
                seven_weight = if s7 < bd.standard {
                    0.85
                } else if s7 == bd.standard && s7 <= 1 {
                    0.35
                } else if s7 == 2 {
                    0.18
                } else {
                    0.05
                };
            }
        }
    }

    // 3. Thirteen Orphans (strictly gated by shanten_13o <= 3 and live missing tiles).
    let mut o13_base = 0.0f64;
    let mut o13_weight = 0.0f64;
    if melds.count == 0 {
        if let Some(s13) = bd.thirteen_orphans {
            if s13 <= THIRTEEN_ORPHANS_MAX_SHANTEN {
                let any_dead_missing = TERMINAL_OR_HONOR_INDICES
                    .iter()
                    .any(|&idx| hand[idx] == 0 && remaining[idx] == 0);
                if !any_dead_missing {
                    let capped_13o = f64::from(4u8.min(rules.fan_cap));
                    o13_base = (2.0_f64).powf(capped_13o);
                    o13_weight = if s13 <= 1 {
                        0.90
                    } else if s13 == 2 && bd.standard >= 3 {
                        0.55
                    } else if s13 == 2 {
                        0.15
                    } else if bd.standard >= 4 {
                        0.25
                    } else {
                        0.0
                    };
                }
            }
        }
    }

    let std_weight = (1.0 - seven_weight).max(0.15);
    let avg_base = (std_weight * std_base + seven_weight * seven_base + o13_weight * o13_base)
        / (std_weight + seven_weight + o13_weight);

    // Expected payout multiplier across Tsumo (3x, ~22% of wins) and Ron (1x, ~78% of wins).
    avg_base * 1.46
}

fn gen_range_usize<R: RngCore + ?Sized>(rng: &mut R, bound: usize) -> usize {
    debug_assert!(bound > 0 && bound <= u32::MAX as usize);
    let bound_u32 = bound as u32;
    let threshold = bound_u32.wrapping_neg() % bound_u32;
    loop {
        let candidate = rng.next_u32();
        if candidate >= threshold {
            return (candidate % bound_u32) as usize;
        }
    }
}

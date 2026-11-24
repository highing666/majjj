//! Layer 4 (L4): Opportunistic stack-`Copy` Flat Monte Carlo search with progressive
//! pruning, 136-tile conservation (including hidden opponent `ConcealedKong`),
//! dual-mode (`Deterministic` / `Realtime`) budgets, and Empirical Bayes blending.

use std::time::{Duration, Instant};

use mahjong_core::{
    KongKind, MeldView, PlayerAction, PlayerView, Seat, Suit, Tile, TurnSource, WinKind,
};
use rand_core::RngCore;
use rayon::prelude::*;
use xiangting::{calculate_necessary_tiles, calculate_replacement_number, PlayerCount};

use crate::l2::{select_best_candidate, CandidateBuffer, CandidateEv};
use crate::l3::{is_passed_hu_zero_risk, L3DefenseModel, L3Profile};
use crate::scoring::{
    evaluate_win, evaluate_win_with_extra, kong_net_delta, MeldSummary, ScoringInput, ScoringRules,
};
use crate::shanten::{
    core_counts_to_riichi, necessary_tiles_seven_pairs_mask, necessary_tiles_thirteen_orphans_mask,
    riichi_mask_to_core, shanten_seven_pairs, shanten_thirteen_orphans, ukeire_from_mask,
};
use crate::{AiDifficulty, DecisionBudget, DecisionContext};

const TRIGGER_MAX_WALL: u8 = 96;
const TRIGGER_SCORE_GAP: f64 = 0.40;
const TRIGGER_MELD_SCORE_GAP: f64 = 0.85;
const TRIGGER_TENPAI_PROB: f64 = 0.25;

const MAX_CANDIDATES: usize = 8;
const BASE_CANDIDATES: usize = 6;
const BATCH_SAMPLES: usize = 16;
const MAX_ROLLOUT_PLIES: usize = 56;
const PRIOR_WEIGHT: f64 = 380.0;
const SOFT_SEPARATION_MARGIN: f64 = 0.35;
const HARD_MARGIN_MS: u64 = 15;
const THIRTEEN_ORPHANS_MAX_SHANTEN: i8 = 3;
const NO_DRAWN_TILE: u8 = u8::MAX;

/// Pure stack-allocated `Copy` simulation state for L4 rollouts (zero heap allocations).
#[derive(Debug, Clone, Copy)]
pub struct SimState {
    /// Closed hand tile frequency tables (`0..34`) for each seat (`0..4`).
    pub hands: [[u8; Tile::COUNT]; 4],
    /// Meld summaries (including determinized hidden `ConcealedKong` counts) per seat.
    pub melds: [MeldSummary; 4],
    /// Bitmask of active `Pon` tiles per seat (`0..34`) for `AddedKong` legality.
    pub pon_masks: [u64; 4],
    /// Remaining wall sequence (`0..34` tile indices).
    pub wall: [u8; 84],
    /// Index of the next normal draw from the wall head.
    pub wall_head: u8,
    /// Exclusive upper index for kong replacement draws from the wall tail.
    pub wall_tail: u8,
    /// Incremental kong score deltas accumulated relative to the decision point.
    pub kong_deltas: [i16; 4],
    /// 4-bit `过水胡` lock bitmask per seat.
    pub passed_hu_mask: u8,
    /// Per-seat 34-bit `过水胡` tile lock bitmasks initialized from `view`.
    pub passed_hu_tile_masks: [u64; 4],
    /// Per-seat 34-bit `过水碰` lock bitmask.
    pub passed_pon_masks: [u64; 4],
    /// Cached tenpai wait bitmasks per seat (`0..34`) updated on discards/melds.
    pub wait_masks: [u64; 4],
    /// Publicly observable remaining tile counts (`4 - visible`) for rollout ukeire.
    pub public_remaining: [u8; Tile::COUNT],
    /// Whether 4-of-a-kind counts as two pairs in `豪华七对`.
    pub allow_four_of_kind_seven_pairs: bool,
    /// Maximum capped fan per winning hand.
    pub fan_cap: u8,
    /// Decision-making seat index (`0..4`).
    pub viewer: u8,
    /// Active seat index (`0..4`) during rollout turns.
    pub current_actor: u8,
    /// Turn origin for `current_actor`.
    pub current_turn_source: TurnSource,
    /// True when the root decision is in `AwaitingDiscardReactions` rather than `TurnAction`.
    pub in_discard_reaction: bool,
    /// Tile being reacted to when `in_discard_reaction` is true.
    pub pending_discard_tile: u8,
    /// Discarder seat index when `in_discard_reaction` is true.
    pub pending_discarder: u8,
    /// Most recently drawn tile (`0..34`) or `NO_DRAWN_TILE`.
    pub drawn_tile: u8,
}

impl SimState {
    #[inline]
    fn wall_len(&self) -> u8 {
        self.wall_tail.saturating_sub(self.wall_head)
    }

    #[inline]
    fn pop_wall_head(&mut self) -> Option<u8> {
        if self.wall_head < self.wall_tail {
            let t = self.wall[usize::from(self.wall_head)];
            self.wall_head += 1;
            Some(t)
        } else {
            None
        }
    }

    #[inline]
    fn pop_wall_tail(&mut self) -> Option<u8> {
        if self.wall_head < self.wall_tail {
            self.wall_tail -= 1;
            Some(self.wall[usize::from(self.wall_tail)])
        } else {
            None
        }
    }

    #[inline]
    fn scoring_rules(&self) -> ScoringRules {
        ScoringRules {
            allow_four_of_kind_seven_pairs: self.allow_four_of_kind_seven_pairs,
            fan_cap: self.fan_cap,
        }
    }
}

#[derive(Clone, Copy)]
struct CandidateStats {
    cand: CandidateEv,
    style_offset: f64,
    sum: f64,
    sum_sq: f64,
    count: usize,
    active: bool,
}

impl CandidateStats {
    fn blended_score(self) -> f64 {
        if self.count == 0 {
            self.cand.ev
        } else {
            let n = self.count as f64;
            let mc_utility = (self.sum / n) + self.style_offset;
            (PRIOR_WEIGHT * self.cand.ev + n * mc_utility) / (PRIOR_WEIGHT + n)
        }
    }

    fn std_error(self) -> f64 {
        if self.count < 2 {
            return 2.0;
        }
        let n = self.count as f64;
        let mean = self.sum / n;
        let var = (self.sum_sq / n - mean * mean).max(0.0);
        (var / n).sqrt()
    }
}

/// Checks whether L4 opportunistic Monte Carlo search should trigger for this decision.
#[must_use]
pub fn should_trigger(
    view: &PlayerView,
    ctx: &DecisionContext,
    l4_enabled: bool,
    candidates: &[CandidateEv],
    model: &L3DefenseModel,
) -> bool {
    if ctx.difficulty != AiDifficulty::Strong || !l4_enabled {
        return false;
    }
    if view.wall_remaining > TRIGGER_MAX_WALL
        || view.legal_actions.len() <= 1
        || candidates.len() <= 1
    {
        return false;
    }

    let gap = top_two_score_gap(candidates);
    let has_meld_or_kong = view.legal_actions.iter().any(|a| {
        matches!(
            a,
            PlayerAction::Pon { .. }
                | PlayerAction::ExposedKong { .. }
                | PlayerAction::AddedKong { .. }
                | PlayerAction::ConcealedKong { .. }
        )
    });
    if has_meld_or_kong
        && (gap < TRIGGER_MELD_SCORE_GAP || model.max_opponent_tenpai() >= TRIGGER_TENPAI_PROB)
    {
        return true;
    }

    if model.max_opponent_tenpai() >= TRIGGER_TENPAI_PROB && gap < 0.70 {
        return true;
    }

    let best_shanten = candidates
        .iter()
        .map(|c| c.shanten_after)
        .min()
        .unwrap_or(i8::MAX);
    (best_shanten <= 1 || view.wall_remaining <= 56) && gap < TRIGGER_SCORE_GAP
}

/// Executes L4 opportunistic Flat Monte Carlo search with progressive pruning and
/// Empirical Bayes blending, falling back to L1-L3 when trigger conditions are not met.
#[must_use]
pub fn decide_with_l4<R: RngCore + ?Sized>(
    view: &PlayerView,
    ctx: &DecisionContext,
    l4_enabled: bool,
    candidates: &CandidateBuffer,
    model: &L3DefenseModel,
    rng: &mut R,
) -> PlayerAction {
    let slice = candidates.as_slice();
    if !should_trigger(view, ctx, l4_enabled, slice, model) {
        return select_best_candidate(slice, rng).unwrap_or(view.legal_actions[0]);
    }

    let max_samples = match ctx.budget {
        DecisionBudget::Deterministic { max_samples }
        | DecisionBudget::Realtime { max_samples, .. } => max_samples,
    };
    if max_samples == 0 {
        return select_best_candidate(slice, rng).unwrap_or(view.legal_actions[0]);
    }

    let master_seed = rng.next_u64();

    // Never touch `std::time::Instant` in `Deterministic` mode.
    let realtime_timing = match ctx.budget {
        DecisionBudget::Deterministic { .. } => None,
        DecisionBudget::Realtime {
            soft_ms, hard_ms, ..
        } => {
            let effective_hard = hard_ms.saturating_sub(HARD_MARGIN_MS).max(hard_ms / 2);
            Some((
                Instant::now(),
                Duration::from_millis(soft_ms),
                Duration::from_millis(effective_hard),
            ))
        }
    };

    let profile = L3Profile::from_context(ctx, l4_enabled);
    let (mut pool_stats, pool_len) = select_candidate_pool(view, model, slice, profile);
    if pool_len <= 1 {
        return select_best_candidate(slice, rng).unwrap_or(view.legal_actions[0]);
    }

    let opp_weights = build_opponent_tile_weights(view, model);
    let opp_p_tenpai = Seat::ALL.map(|s| model.opponent_signal(s).p_tenpai);
    let mut completed_batches = 0usize;
    let mut sample_offset = 0usize;

    while sample_offset < max_samples {
        if let Some((start, _, hard_deadline)) = realtime_timing {
            if start.elapsed() >= hard_deadline {
                break;
            }
        }

        let batch_len = (max_samples - sample_offset).min(BATCH_SAMPLES);

        let mut active_indices = [0usize; MAX_CANDIDATES];
        let mut active_len = 0usize;
        for (i, st) in pool_stats[..pool_len].iter().enumerate() {
            if st.active {
                active_indices[active_len] = i;
                active_len += 1;
            }
        }
        if active_len <= 1 {
            break;
        }

        // Pre-sample one determinized `SimState` per sample_idx (common random numbers).
        let mut base_states = [empty_sim_state(); BATCH_SAMPLES];
        let mut aborted_during_sampling = false;
        for (offset, slot) in base_states[..batch_len].iter_mut().enumerate() {
            if let Some((start, _, hard_deadline)) = realtime_timing {
                if start.elapsed() >= hard_deadline {
                    aborted_during_sampling = true;
                    break;
                }
            }
            let sample_idx = sample_offset + offset;
            let sample_seed = derive_sample_seed(master_seed, sample_idx);
            *slot = sample_sim_state_with_weights(view, &opp_weights, &opp_p_tenpai, sample_seed);
        }
        if aborted_during_sampling {
            break;
        }

        let num_tasks = active_len * batch_len;
        let mut batch_results = [0.0f64; MAX_CANDIDATES * BATCH_SAMPLES];

        batch_results[..num_tasks]
            .par_iter_mut()
            .enumerate()
            .for_each(|(task_idx, out)| {
                if let Some((start, _, hard_deadline)) = realtime_timing {
                    if start.elapsed() >= hard_deadline {
                        *out = f64::NAN;
                        return;
                    }
                }
                let active_pos = task_idx / batch_len;
                let offset = task_idx % batch_len;
                let cand_idx = active_indices[active_pos];
                let sample_idx = sample_offset + offset;
                let rollout_seed = derive_rollout_seed(master_seed, cand_idx, sample_idx);
                let sim = base_states[offset];
                let action = pool_stats[cand_idx].cand.action;
                *out = evaluate_rollout(sim, action, profile, rollout_seed);
            });

        if batch_results[..num_tasks].iter().any(|v| v.is_nan()) {
            break;
        }

        // Sequential index-ordered reduction guarantees bit-exact cross-thread determinism.
        for (active_pos, &cand_idx) in active_indices[..active_len].iter().enumerate() {
            let st = &mut pool_stats[cand_idx];
            for offset in 0..batch_len {
                let val = batch_results[active_pos * batch_len + offset];
                st.sum += val;
                st.sum_sq += val * val;
                st.count += 1;
            }
        }

        completed_batches += 1;
        sample_offset += batch_len;

        progressive_prune(&mut pool_stats[..pool_len], completed_batches);

        if let Some((start, soft_deadline, hard_deadline)) = realtime_timing {
            let elapsed = start.elapsed();
            if elapsed >= hard_deadline {
                break;
            }
            if elapsed >= soft_deadline && soft_stop_ready(&pool_stats[..pool_len]) {
                break;
            }
        }
    }

    if completed_batches == 0 {
        return select_best_candidate(slice, rng).unwrap_or(view.legal_actions[0]);
    }

    select_best_from_stats(&pool_stats[..pool_len], rng).unwrap_or(view.legal_actions[0])
}

/// Deterministically samples a `SimState` from public `view` and `model` with strict
/// 136-tile conservation (including locking 4-copy tiles for opponent hidden `ConcealedKong`).
#[must_use]
pub fn sample_sim_state(view: &PlayerView, model: &L3DefenseModel, seed: u64) -> SimState {
    let opp_weights = build_opponent_tile_weights(view, model);
    let opp_p_tenpai = Seat::ALL.map(|s| model.opponent_signal(s).p_tenpai);
    sample_sim_state_with_weights(view, &opp_weights, &opp_p_tenpai, seed)
}

fn empty_sim_state() -> SimState {
    SimState {
        hands: [[0; Tile::COUNT]; 4],
        melds: [MeldSummary::empty(); 4],
        pon_masks: [0; 4],
        wall: [0; 84],
        wall_head: 0,
        wall_tail: 0,
        kong_deltas: [0; 4],
        passed_hu_mask: 0,
        passed_hu_tile_masks: [0; 4],
        passed_pon_masks: [0; 4],
        wait_masks: [0; 4],
        public_remaining: [0; Tile::COUNT],
        allow_four_of_kind_seven_pairs: true,
        fan_cap: 4,
        viewer: 0,
        current_actor: 0,
        current_turn_source: TurnSource::NormalDraw,
        in_discard_reaction: false,
        pending_discard_tile: 0,
        pending_discarder: 0,
        drawn_tile: NO_DRAWN_TILE,
    }
}

fn top_two_score_gap(candidates: &[CandidateEv]) -> f64 {
    let mut best1 = f64::NEG_INFINITY;
    let mut best2 = f64::NEG_INFINITY;
    for cand in candidates {
        if cand.ev > best1 {
            best2 = best1;
            best1 = cand.ev;
        } else if cand.ev > best2 {
            best2 = cand.ev;
        }
    }
    if best2.is_finite() {
        (best1 - best2).max(0.0)
    } else {
        f64::INFINITY
    }
}

fn is_meld_or_pass_action(action: PlayerAction) -> bool {
    matches!(
        action,
        PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::AddedKong { .. }
            | PlayerAction::ConcealedKong { .. }
            | PlayerAction::Pass
    )
}

fn candidate_style_offset(
    view: &PlayerView,
    model: &L3DefenseModel,
    cand: &CandidateEv,
    profile: L3Profile,
) -> f64 {
    let mut offset = 1.6 * profile.speed_bias * cand.p_win
        + 1.4 * profile.big_hand_bias * cand.p_win * (cand.expected_win_score - 2.0).max(0.0);
    match cand.action {
        PlayerAction::Discard { tile, .. } => {
            // Preserve L3's multi-turn Betaori / fold risk penalty when `eff_w > 1.0`,
            // since a greedy L1 rollout only observes single-turn deal-in loss at 1.0x.
            let eff_w = crate::l3::candidate_effective_risk_weight(cand, view, model, profile);
            let risk = model.discard_risk(tile);
            let late = view.wall_remaining <= 44;
            let tail_risk = if cand.shanten_after >= 2 || cand.ukeire <= 5 || late {
                (risk - 0.10).max(0.0)
            } else {
                0.0
            };
            if eff_w > 0.02 {
                offset -= (eff_w - 0.02) * risk;
            }
            offset -= profile.risk_weight * 2.5 * tail_risk;
        }
        PlayerAction::Pon { .. } | PlayerAction::ExposedKong { .. } => {
            offset += if profile.meld_bias > 0.0 {
                4.0 * profile.meld_bias
            } else {
                profile.meld_bias
            };
        }
        PlayerAction::AddedKong { tile } => {
            let rob_ev = crate::l2::RobKongRiskModel::rob_kong_risk_ev(model, view, tile);
            if rob_ev > 1.5 {
                offset -= 0.75 * (rob_ev - 1.0);
            }
        }
        _ => {}
    }
    offset
}

fn select_candidate_pool(
    view: &PlayerView,
    model: &L3DefenseModel,
    candidates: &[CandidateEv],
    profile: L3Profile,
) -> ([CandidateStats; MAX_CANDIDATES], usize) {
    const EMPTY_STAT: CandidateStats = CandidateStats {
        cand: CandidateEv {
            action: PlayerAction::Pass,
            ev: 0.0,
            p_tenpai: 0.0,
            p_win: 0.0,
            expected_win_score: 0.0,
            shanten_after: i8::MAX,
            ukeire: 0,
            tie_priority: 0,
        },
        style_offset: 0.0,
        sum: 0.0,
        sum_sq: 0.0,
        count: 0,
        active: false,
    };

    let mut ordered = [0usize; 40];
    let n = candidates.len().min(40);
    for (i, slot) in ordered[..n].iter_mut().enumerate() {
        *slot = i;
    }
    ordered[..n].sort_by(|&a, &b| {
        let ca = &candidates[a];
        let cb = &candidates[b];
        cb.ev
            .total_cmp(&ca.ev)
            .then_with(|| ca.shanten_after.cmp(&cb.shanten_after))
            .then_with(|| cb.ukeire.cmp(&ca.ukeire))
            .then_with(|| cb.tie_priority.cmp(&ca.tie_priority))
            .then_with(|| a.cmp(&b))
    });

    let mut chosen = [false; 40];
    let mut pool = [EMPTY_STAT; MAX_CANDIDATES];
    let mut pool_len = 0usize;

    for &idx in ordered[..n].iter().take(BASE_CANDIDATES) {
        if pool_len >= MAX_CANDIDATES {
            break;
        }
        chosen[idx] = true;
        pool[pool_len] = CandidateStats {
            cand: candidates[idx],
            style_offset: candidate_style_offset(view, model, &candidates[idx], profile),
            sum: 0.0,
            sum_sq: 0.0,
            count: 0,
            active: true,
        };
        pool_len += 1;
    }

    // Always retain any legal meld/kong/Pass alternatives up to `MAX_CANDIDATES`.
    for &idx in &ordered[..n] {
        if pool_len >= MAX_CANDIDATES {
            break;
        }
        if !chosen[idx] && is_meld_or_pass_action(candidates[idx].action) {
            chosen[idx] = true;
            pool[pool_len] = CandidateStats {
                cand: candidates[idx],
                style_offset: candidate_style_offset(view, model, &candidates[idx], profile),
                sum: 0.0,
                sum_sq: 0.0,
                count: 0,
                active: true,
            };
            pool_len += 1;
        }
    }

    (pool, pool_len)
}

fn progressive_prune(stats: &mut [CandidateStats], completed_batches: usize) {
    if completed_batches < 2 {
        return;
    }
    let active_count = stats.iter().filter(|s| s.active).count();
    if active_count <= 2 {
        return;
    }

    let mut order = [0usize; MAX_CANDIDATES];
    let len = stats.len().min(MAX_CANDIDATES);
    for (i, slot) in order[..len].iter_mut().enumerate() {
        *slot = i;
    }
    order[..len].sort_by(|&a, &b| {
        stats[b]
            .active
            .cmp(&stats[a].active)
            .then_with(|| {
                stats[b]
                    .blended_score()
                    .total_cmp(&stats[a].blended_score())
            })
            .then_with(|| a.cmp(&b))
    });

    let leader_idx = order[0];
    let leader_score = stats[leader_idx].blended_score();
    let leader_se = stats[leader_idx].std_error();
    let max_keep = if completed_batches == 2 { 4 } else { 3 };

    let mut kept = 0usize;
    for &idx in &order[..len] {
        if !stats[idx].active {
            continue;
        }
        // Always retain the top-2 blended candidates and never prune the #1 L1-L3 prior leader early.
        if kept < 2 || idx == 0 {
            kept += 1;
            continue;
        }
        if kept >= max_keep {
            stats[idx].active = false;
            continue;
        }
        let gap = leader_score - stats[idx].blended_score();
        let se = (leader_se * leader_se + stats[idx].std_error() * stats[idx].std_error()).sqrt();
        if gap > (2.0 * se).max(0.65) {
            stats[idx].active = false;
        } else {
            kept += 1;
        }
    }
}

fn soft_stop_ready(stats: &[CandidateStats]) -> bool {
    let mut best1 = f64::NEG_INFINITY;
    let mut best2 = f64::NEG_INFINITY;
    for st in stats {
        if !st.active {
            continue;
        }
        let s = st.blended_score();
        if s > best1 {
            best2 = best1;
            best1 = s;
        } else if s > best2 {
            best2 = s;
        }
    }
    !best2.is_finite() || (best1 - best2) >= SOFT_SEPARATION_MARGIN
}

fn select_best_from_stats<R: RngCore + ?Sized>(
    stats: &[CandidateStats],
    rng: &mut R,
) -> Option<PlayerAction> {
    let mut temp = [CandidateEv {
        action: PlayerAction::Pass,
        ev: 0.0,
        p_tenpai: 0.0,
        p_win: 0.0,
        expected_win_score: 0.0,
        shanten_after: i8::MAX,
        ukeire: 0,
        tie_priority: 0,
    }; MAX_CANDIDATES];
    let mut len = 0usize;
    for st in stats {
        if !st.active {
            continue;
        }
        let mut c = st.cand;
        c.ev = st.blended_score();
        temp[len] = c;
        len += 1;
    }
    select_best_candidate(&temp[..len], rng)
}

fn build_opponent_tile_weights(
    view: &PlayerView,
    model: &L3DefenseModel,
) -> [[u32; Tile::COUNT]; 4] {
    let mut weights = [[100u32; Tile::COUNT]; 4];
    for seat in Seat::ALL {
        if seat == view.viewer {
            continue;
        }
        let s_idx = seat.index();
        let sig = model.opponent_signal(seat);
        let max_flush = sig
            .flush_concentration
            .iter()
            .copied()
            .fold(0.0f64, f64::max)
            .max(sig.all_honors_prob);
        if max_flush < 0.15 && sig.p_tenpai < 0.25 {
            continue;
        }
        for (idx, w_slot) in weights[s_idx].iter_mut().enumerate() {
            let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
            let mut w = 100i32;
            if tile.is_honor() {
                if sig.all_honors_prob > 0.20 {
                    w += (260.0 * sig.all_honors_prob) as i32;
                } else if sig.honor_flush_affinity > 0.20 {
                    w += (130.0 * sig.honor_flush_affinity) as i32;
                }
            } else {
                let suit_idx = match tile.suit() {
                    Suit::Man => 0,
                    Suit::Bamboo => 1,
                    Suit::Dot => 2,
                    Suit::Wind | Suit::Dragon => unreachable!(),
                };
                let conc = sig.flush_concentration[suit_idx];
                let other_conc = sig.flush_concentration[(suit_idx + 1) % 3]
                    .max(sig.flush_concentration[(suit_idx + 2) % 3])
                    .max(sig.all_honors_prob);
                if conc > 0.15 {
                    w += (240.0 * conc) as i32;
                }
                if other_conc > 0.15 {
                    w -= (85.0 * other_conc) as i32;
                }
            }
            if sig.p_tenpai >= 0.25 && !is_passed_hu_zero_risk(view, seat, tile) {
                w += (30.0 * sig.p_tenpai) as i32;
            }
            *w_slot = w.max(8) as u32;
        }
    }
    weights
}

fn sample_sim_state_with_weights(
    view: &PlayerView,
    opp_weights: &[[u32; Tile::COUNT]; 4],
    opp_p_tenpai: &[f64; 4],
    seed: u64,
) -> SimState {
    let mut rng = SplitMix64(seed);
    let mut sim = empty_sim_state();
    sim.viewer = view.viewer.index() as u8;
    sim.allow_four_of_kind_seven_pairs = view.rule_profile.allow_four_of_kind_seven_pairs;
    sim.fan_cap = view.rule_profile.fan_cap;

    let mut visible = [0u8; Tile::COUNT];
    for &t in &view.hand.closed_tiles {
        let idx = t.index();
        sim.hands[view.viewer.index()][idx] += 1;
        visible[idx] += 1;
    }
    if let Some(drawn) = view.hand.drawn_tile {
        let idx = drawn.index();
        sim.hands[view.viewer.index()][idx] += 1;
        visible[idx] += 1;
        sim.drawn_tile = drawn.as_u8();
    }

    let mut hidden_kong_counts = [0u8; 4];
    for seat in Seat::ALL {
        let s_idx = seat.index();
        let sv = &view.seats[s_idx];
        sim.melds[s_idx] = MeldSummary::from_meld_views(&sv.melds);

        for entry in &sv.river {
            if entry.claimed_by.is_none() {
                visible[entry.tile.index()] += 1;
            }
        }
        for meld in &sv.melds {
            match *meld {
                MeldView::Pon { tile, .. } => {
                    visible[tile.index()] += 3;
                    sim.pon_masks[s_idx] |= 1u64 << tile.index();
                }
                MeldView::ExposedKong { tile, .. } | MeldView::AddedKong { tile, .. } => {
                    visible[tile.index()] += 4;
                }
                MeldView::ConcealedKong { tile: Some(t), .. } => {
                    visible[t.index()] += 4;
                }
                MeldView::ConcealedKong { tile: None, .. } => {
                    hidden_kong_counts[s_idx] += 1;
                }
                MeldView::Chi { tiles, .. } => {
                    for t in tiles {
                        visible[t.index()] += 1;
                    }
                }
            }
        }
    }

    let mut unseen = [0u8; Tile::COUNT];
    for idx in 0..Tile::COUNT {
        unseen[idx] = 4u8.saturating_sub(visible[idx]);
        sim.public_remaining[idx] = unseen[idx];
    }

    // Step 1: Lock 4 copies per opponent hidden `ConcealedKong { tile: None }` from `visible[k] == 0`.
    let mut full_quads = [0u8; Tile::COUNT];
    let mut fq_len = 0usize;
    for (idx, &vis) in visible.iter().enumerate() {
        if vis == 0 {
            full_quads[fq_len] = idx as u8;
            fq_len += 1;
        }
    }

    for (s_idx, &hk_count) in hidden_kong_counts.iter().enumerate() {
        for _ in 0..hk_count {
            debug_assert!(fq_len > 0, "hidden concealed kong requires an unseen quad");
            let pick = rng.gen_range_usize(fq_len);
            let tile_idx = usize::from(full_quads[pick]);
            fq_len -= 1;
            full_quads[pick] = full_quads[fq_len];
            unseen[tile_idx] = 0;
            sim.melds[s_idx].counts[tile_idx] = sim.melds[s_idx].counts[tile_idx].saturating_add(4);
        }
    }

    // Step 2: Weighted dealing to opponents' exact `hand_count`, constructing realistic
    // tenpai hands at rate `p_tenpai` from `unseen` while strictly conserving 136 tiles.
    for seat in Seat::ALL {
        let s_idx = seat.index();
        if seat == view.viewer {
            continue;
        }
        let need = usize::from(view.seats[s_idx].hand_count);
        if need == 0 {
            continue;
        }
        let p_tenpai = opp_p_tenpai[s_idx].clamp(0.0, 1.0);
        let sample_tenpai =
            p_tenpai >= 0.08 && (rng.next_u64() % 1000) < (p_tenpai * 1000.0) as u64;

        if sample_tenpai {
            deal_tenpai_conditioned_hand(
                &mut unseen,
                &mut sim.hands[s_idx],
                &opp_weights[s_idx],
                need,
                &mut rng,
            );
        } else {
            // Non-tenpai opponent: mild best-of-2 random deal (mostly 1-3 shanten).
            let mut best_hand = [0u8; Tile::COUNT];
            let mut best_unseen = unseen;
            let mut best_sh = i8::MAX;
            let trials = if need <= 7 { 2 } else { 1 };
            for _ in 0..trials {
                let mut cand_hand = [0u8; Tile::COUNT];
                let mut cand_unseen = unseen;
                for _ in 0..need {
                    let k = sample_weighted_tile(&cand_unseen, &opp_weights[s_idx], &mut rng);
                    cand_unseen[k] -= 1;
                    cand_hand[k] += 1;
                }
                if trials == 1 {
                    best_hand = cand_hand;
                    best_unseen = cand_unseen;
                    break;
                }
                let sh = fast_rollout_shanten(
                    &cand_hand,
                    usize::from(sim.melds[s_idx].count),
                    sim.allow_four_of_kind_seven_pairs,
                );
                if sh < best_sh {
                    best_sh = sh;
                    best_hand = cand_hand;
                    best_unseen = cand_unseen;
                }
            }
            sim.hands[s_idx] = best_hand;
            unseen = best_unseen;
        }
    }

    // Step 3: Fill and shuffle remaining unseen tiles into the wall.
    let mut wall_len = 0usize;
    for (idx, &cnt) in unseen.iter().enumerate() {
        for _ in 0..cnt {
            sim.wall[wall_len] = idx as u8;
            wall_len += 1;
        }
    }
    debug_assert_eq!(wall_len, usize::from(view.wall_remaining));
    for i in (1..wall_len).rev() {
        let j = rng.gen_range_usize(i + 1);
        sim.wall.swap(i, j);
    }
    sim.wall_head = 0;
    sim.wall_tail = wall_len as u8;

    // Detect whether viewer is in `AwaitingDiscardReactions` or `TurnAction`.
    let in_reaction = view
        .legal_actions
        .iter()
        .any(|a| matches!(a, PlayerAction::Pass));
    sim.in_discard_reaction = in_reaction;
    if in_reaction {
        let discarder = view.turn.active_seat.unwrap_or(view.viewer);
        sim.pending_discarder = discarder.index() as u8;
        let pending_tile = view.seats[discarder.index()]
            .river
            .last()
            .map_or(0, |d| d.tile.as_u8());
        sim.pending_discard_tile = pending_tile;
        sim.current_actor = discarder.index() as u8;
    } else {
        sim.current_actor = view.viewer.index() as u8;
        sim.current_turn_source = TurnSource::NormalDraw;
    }

    for s_idx in 0..4 {
        if s_idx != usize::from(sim.viewer) {
            sim.wait_masks[s_idx] = compute_wait_mask(
                &sim.hands[s_idx],
                &sim.melds[s_idx],
                sim.allow_four_of_kind_seven_pairs,
            );
            let opp_seat = Seat::ALL[s_idx];
            let opp_seq = view.seats[s_idx].last_draw_or_meld_seq;
            let mut lock_mask = 0u64;
            for sv in &view.seats {
                if sv.seat != opp_seat {
                    for d in &sv.river {
                        if d.seq > opp_seq {
                            lock_mask |= 1u64 << d.tile.index();
                        }
                    }
                }
            }
            sim.passed_hu_tile_masks[s_idx] = lock_mask;
        }
    }

    debug_assert_eq!(
        total_tiles_in_sim(&sim, view),
        Tile::DECK_SIZE,
        "L4 SimState 136-tile conservation invariant violated"
    );

    sim
}

#[allow(dead_code)]
fn total_tiles_in_sim(sim: &SimState, view: &PlayerView) -> usize {
    let mut total = 0usize;
    for s in 0..4 {
        total += sim.hands[s].iter().map(|&c| usize::from(c)).sum::<usize>();
        total += sim.melds[s]
            .counts
            .iter()
            .map(|&c| usize::from(c))
            .sum::<usize>();
        total += view.seats[s]
            .river
            .iter()
            .filter(|e| e.claimed_by.is_none())
            .count();
    }
    total += usize::from(sim.wall_len());
    total
}

fn deal_tenpai_conditioned_hand(
    unseen: &mut [u8; Tile::COUNT],
    hand: &mut [u8; Tile::COUNT],
    weights: &[u32; Tile::COUNT],
    need: usize,
    rng: &mut SplitMix64,
) {
    let mut left = need;

    // Decide whether to construct a pair + 2-tile proto-meld (ryanmen/kanchan/shanpon) vs all melds + tanki.
    let use_pair_and_taatsu = left >= 4 && (rng.next_u64() % 10) < 8;
    let target_melds = if use_pair_and_taatsu {
        (left - 4) / 3
    } else {
        left / 3
    };

    for _ in 0..target_melds {
        if left < 3 {
            break;
        }
        if !try_take_random_meld(unseen, hand, weights, rng) {
            break;
        }
        left -= 3;
    }

    if use_pair_and_taatsu && left >= 4 {
        if try_take_pair(unseen, hand, weights, rng) {
            left -= 2;
        }
        if left >= 2 && try_take_proto_meld(unseen, hand, weights, rng) {
            left -= 2;
        }
    }

    for _ in 0..left {
        let k = sample_weighted_tile(unseen, weights, rng);
        unseen[k] -= 1;
        hand[k] += 1;
    }
}

fn try_take_random_meld(
    unseen: &mut [u8; Tile::COUNT],
    hand: &mut [u8; Tile::COUNT],
    weights: &[u32; Tile::COUNT],
    rng: &mut SplitMix64,
) -> bool {
    let mut cands = [([0usize; 3], 0u64); 55];
    let mut len = 0usize;

    // Sequence melds (3 suits * 7 starts = 21).
    for suit in 0..3 {
        let base = suit * 9;
        for r in 0..7 {
            let a = base + r;
            let b = a + 1;
            let c = a + 2;
            if unseen[a] > 0 && unseen[b] > 0 && unseen[c] > 0 {
                let w = u64::from(unseen[a])
                    * u64::from(unseen[b])
                    * u64::from(unseen[c])
                    * u64::from((weights[a] + weights[b] + weights[c]) / 3);
                cands[len] = ([a, b, c], w.max(1));
                len += 1;
            }
        }
    }

    // Triplet melds (up to 34).
    for k in 0..Tile::COUNT {
        if unseen[k] >= 3 {
            let w = u64::from(unseen[k]) * u64::from(weights[k]) * 3;
            cands[len] = ([k, k, k], w.max(1));
            len += 1;
        }
    }

    if len == 0 {
        return false;
    }
    let total_w: u64 = cands[..len].iter().map(|&(_, w)| w).sum();
    let mut r = rng.gen_range_u64(total_w);
    for &(tiles, w) in &cands[..len] {
        if r < w {
            for t in tiles {
                unseen[t] -= 1;
                hand[t] += 1;
            }
            return true;
        }
        r -= w;
    }
    false
}

fn try_take_pair(
    unseen: &mut [u8; Tile::COUNT],
    hand: &mut [u8; Tile::COUNT],
    weights: &[u32; Tile::COUNT],
    rng: &mut SplitMix64,
) -> bool {
    let mut cands = [(0usize, 0u64); Tile::COUNT];
    let mut len = 0usize;
    for k in 0..Tile::COUNT {
        if unseen[k] >= 2 {
            let w = u64::from(unseen[k]) * u64::from(unseen[k] - 1) * u64::from(weights[k]);
            cands[len] = (k, w.max(1));
            len += 1;
        }
    }
    if len == 0 {
        return false;
    }
    let total_w: u64 = cands[..len].iter().map(|&(_, w)| w).sum();
    let mut r = rng.gen_range_u64(total_w);
    for &(k, w) in &cands[..len] {
        if r < w {
            unseen[k] -= 2;
            hand[k] += 2;
            return true;
        }
        r -= w;
    }
    false
}

fn try_take_proto_meld(
    unseen: &mut [u8; Tile::COUNT],
    hand: &mut [u8; Tile::COUNT],
    weights: &[u32; Tile::COUNT],
    rng: &mut SplitMix64,
) -> bool {
    let mut cands = [([0usize; 2], 0u64); 80];
    let mut len = 0usize;

    // Adjacent 2-tile sequences (ryanmen/penchan, 3 * 8 = 24) and 1-gap kanchan (3 * 7 = 21).
    for suit in 0..3 {
        let base = suit * 9;
        for r in 0..8 {
            let a = base + r;
            let b = a + 1;
            if unseen[a] > 0 && unseen[b] > 0 {
                let is_edge = r == 0 || r == 7;
                let shape_mult = if is_edge { 2 } else { 5 };
                let w = u64::from(unseen[a])
                    * u64::from(unseen[b])
                    * u64::from((weights[a] + weights[b]) / 2)
                    * shape_mult;
                cands[len] = ([a, b], w.max(1));
                len += 1;
            }
        }
        for r in 0..7 {
            let a = base + r;
            let c = a + 2;
            if unseen[a] > 0 && unseen[c] > 0 {
                let w = u64::from(unseen[a])
                    * u64::from(unseen[c])
                    * u64::from((weights[a] + weights[c]) / 2)
                    * 2;
                cands[len] = ([a, c], w.max(1));
                len += 1;
            }
        }
    }

    // Second pair (shanpon wait, up to 34).
    for k in 0..Tile::COUNT {
        if unseen[k] >= 2 {
            let w = u64::from(unseen[k]) * u64::from(unseen[k] - 1) * u64::from(weights[k]);
            cands[len] = ([k, k], w.max(1));
            len += 1;
        }
    }

    if len == 0 {
        return false;
    }
    let total_w: u64 = cands[..len].iter().map(|&(_, w)| w).sum();
    let mut r = rng.gen_range_u64(total_w);
    for &(pair, w) in &cands[..len] {
        if r < w {
            unseen[pair[0]] -= 1;
            hand[pair[0]] += 1;
            unseen[pair[1]] -= 1;
            hand[pair[1]] += 1;
            return true;
        }
        r -= w;
    }
    false
}

fn sample_weighted_tile(
    unseen: &[u8; Tile::COUNT],
    weights: &[u32; Tile::COUNT],
    rng: &mut SplitMix64,
) -> usize {
    let mut total_w = 0u64;
    for idx in 0..Tile::COUNT {
        if unseen[idx] > 0 {
            total_w += u64::from(unseen[idx]) * u64::from(weights[idx]);
        }
    }
    debug_assert!(total_w > 0, "unseen pool must not be empty");
    let mut r = rng.gen_range_u64(total_w);
    for idx in 0..Tile::COUNT {
        if unseen[idx] == 0 {
            continue;
        }
        let w = u64::from(unseen[idx]) * u64::from(weights[idx]);
        if r < w {
            return idx;
        }
        r -= w;
    }
    0
}

fn evaluate_rollout(
    mut sim: SimState,
    root_action: PlayerAction,
    profile: L3Profile,
    seed: u64,
) -> f64 {
    let mut rng = SplitMix64(seed);
    let viewer = usize::from(sim.viewer);

    match root_action {
        PlayerAction::Discard { tile, .. } => {
            let t_idx = tile.index();
            debug_assert!(sim.hands[viewer][t_idx] > 0);
            sim.hands[viewer][t_idx] -= 1;
            sim.drawn_tile = NO_DRAWN_TILE;
            sim.wait_masks[viewer] = compute_wait_mask(
                &sim.hands[viewer],
                &sim.melds[viewer],
                sim.allow_four_of_kind_seven_pairs,
            );
            if let Some(score) = resolve_discard_step(&mut sim, viewer, t_idx, profile, &mut rng) {
                return score;
            }
        }
        PlayerAction::ConcealedKong { tile } => {
            let t_idx = tile.index();
            debug_assert!(sim.hands[viewer][t_idx] >= 4);
            sim.hands[viewer][t_idx] -= 4;
            sim.melds[viewer] = sim.melds[viewer].with_added_concealed_kong(tile);
            add_kong_delta(&mut sim, viewer, KongKind::ConcealedKong, None);
            let Some(drawn) = sim.pop_wall_tail() else {
                return f64::from(sim.kong_deltas[viewer]);
            };
            sim.hands[viewer][usize::from(drawn)] += 1;
            sim.drawn_tile = drawn;
            sim.current_actor = viewer as u8;
            sim.current_turn_source = TurnSource::ReplacementDraw;
        }
        PlayerAction::AddedKong { tile } => {
            let t_idx = tile.index();
            if let Some(rob_score) = check_rob_kong_hu(&sim, viewer, t_idx, profile) {
                return rob_score;
            }
            debug_assert!(sim.hands[viewer][t_idx] >= 1);
            sim.hands[viewer][t_idx] -= 1;
            sim.melds[viewer] = sim.melds[viewer].with_upgraded_added_kong(tile);
            sim.pon_masks[viewer] &= !(1u64 << t_idx);
            add_kong_delta(&mut sim, viewer, KongKind::AddedKong, None);
            let Some(drawn) = sim.pop_wall_tail() else {
                return f64::from(sim.kong_deltas[viewer]);
            };
            sim.hands[viewer][usize::from(drawn)] += 1;
            sim.drawn_tile = drawn;
            sim.current_actor = viewer as u8;
            sim.current_turn_source = TurnSource::ReplacementDraw;
        }
        PlayerAction::Pon { tile } => {
            let discarder = usize::from(sim.pending_discarder);
            let t_idx = tile.index();
            if let Some(ron_score) =
                check_ron_reactions(&sim, discarder, t_idx, Some(viewer), profile)
            {
                return ron_score;
            }
            debug_assert!(sim.hands[viewer][t_idx] >= 2);
            sim.hands[viewer][t_idx] -= 2;
            sim.melds[viewer] = sim.melds[viewer].with_added_pon(tile);
            sim.pon_masks[viewer] |= 1u64 << t_idx;
            sim.passed_hu_mask &= !(1u8 << viewer);
            sim.passed_hu_tile_masks[viewer] = 0;
            sim.drawn_tile = NO_DRAWN_TILE;
            sim.current_actor = viewer as u8;
            sim.current_turn_source = TurnSource::AfterMeld;
        }
        PlayerAction::ExposedKong { tile } => {
            let discarder = usize::from(sim.pending_discarder);
            let t_idx = tile.index();
            if let Some(ron_score) =
                check_ron_reactions(&sim, discarder, t_idx, Some(viewer), profile)
            {
                return ron_score;
            }
            debug_assert!(sim.hands[viewer][t_idx] >= 3);
            sim.hands[viewer][t_idx] -= 3;
            sim.melds[viewer] = sim.melds[viewer].with_added_exposed_kong(tile);
            sim.passed_hu_mask &= !(1u8 << viewer);
            sim.passed_hu_tile_masks[viewer] = 0;
            add_kong_delta(&mut sim, viewer, KongKind::ExposedKong, Some(discarder));
            let Some(drawn) = sim.pop_wall_tail() else {
                return f64::from(sim.kong_deltas[viewer]);
            };
            sim.hands[viewer][usize::from(drawn)] += 1;
            sim.drawn_tile = drawn;
            sim.current_actor = viewer as u8;
            sim.current_turn_source = TurnSource::ReplacementDraw;
        }
        PlayerAction::Pass => {
            let discarder = usize::from(sim.pending_discarder);
            let t_idx = usize::from(sim.pending_discard_tile);
            sim.passed_pon_masks[viewer] |= 1u64 << t_idx;
            if let Some(ron_score) =
                check_ron_reactions(&sim, discarder, t_idx, Some(viewer), profile)
            {
                return ron_score;
            }
            if sim.wall_len() == 0 {
                return f64::from(sim.kong_deltas[viewer]);
            }
            let Some(drawn) = sim.pop_wall_head() else {
                return f64::from(sim.kong_deltas[viewer]);
            };
            let next_seat = (discarder + 1) % 4;
            sim.hands[next_seat][usize::from(drawn)] += 1;
            sim.passed_hu_mask &= !(1u8 << next_seat);
            sim.passed_hu_tile_masks[next_seat] = 0;
            sim.passed_pon_masks[next_seat] = 0;
            sim.drawn_tile = drawn;
            sim.current_actor = next_seat as u8;
            sim.current_turn_source = TurnSource::NormalDraw;
        }
        _ => return f64::from(sim.kong_deltas[viewer]),
    }

    for _ply in 0..MAX_ROLLOUT_PLIES {
        let actor = usize::from(sim.current_actor);

        // Iron Rule 1: Immediate Tsumo check when draw source allows it.
        if sim.current_turn_source != TurnSource::AfterMeld {
            let can_win = if sim.drawn_tile != NO_DRAWN_TILE {
                (sim.wait_masks[actor] & (1u64 << sim.drawn_tile)) != 0
            } else {
                fast_rollout_shanten(
                    &sim.hands[actor],
                    usize::from(sim.melds[actor].count),
                    sim.allow_four_of_kind_seven_pairs,
                ) == -1
            };
            if can_win {
                let input = ScoringInput {
                    win_kind: WinKind::Tsumo,
                    turn_source: Some(sim.current_turn_source),
                    wall_remaining: sim.wall_len(),
                };
                if let Some(eval) = evaluate_win(
                    &sim.hands[actor],
                    &sim.melds[actor],
                    input,
                    sim.scoring_rules(),
                ) {
                    let base = f64::from(eval.base_points);
                    let win_delta = if actor == viewer { 3.0 * base } else { -base };
                    return f64::from(sim.kong_deltas[viewer]) + win_delta;
                }
            }
        }

        // Fast rollout concealed kong when tenpai or 1-shanten and wall > 0.
        if sim.wall_len() > 0 && sim.current_turn_source != TurnSource::AfterMeld {
            if let Some(kong_t_idx) = maybe_rollout_concealed_kong(&sim, actor) {
                let kong_tile = Tile::new(kong_t_idx as u8).expect("idx bounded by Tile::COUNT");
                sim.hands[actor][kong_t_idx] -= 4;
                sim.melds[actor] = sim.melds[actor].with_added_concealed_kong(kong_tile);
                add_kong_delta(&mut sim, actor, KongKind::ConcealedKong, None);
                if actor != viewer {
                    sim.public_remaining[kong_t_idx] = 0;
                }
                let Some(drawn) = sim.pop_wall_tail() else {
                    return f64::from(sim.kong_deltas[viewer]);
                };
                sim.hands[actor][usize::from(drawn)] += 1;
                sim.drawn_tile = drawn;
                sim.current_turn_source = TurnSource::ReplacementDraw;
                sim.wait_masks[actor] = compute_wait_mask(
                    &sim.hands[actor],
                    &sim.melds[actor],
                    sim.allow_four_of_kind_seven_pairs,
                );
                continue;
            }
        }

        let (discard_idx, shanten_after) = choose_rollout_discard(&mut sim, actor, &mut rng);
        sim.hands[actor][discard_idx] -= 1;
        sim.drawn_tile = NO_DRAWN_TILE;
        if actor != viewer {
            sim.public_remaining[discard_idx] = sim.public_remaining[discard_idx].saturating_sub(1);
        }
        sim.wait_masks[actor] = if shanten_after == 0 {
            compute_wait_mask(
                &sim.hands[actor],
                &sim.melds[actor],
                sim.allow_four_of_kind_seven_pairs,
            )
        } else {
            0
        };

        if let Some(score) = resolve_discard_step(&mut sim, actor, discard_idx, profile, &mut rng) {
            return score;
        }
    }

    f64::from(sim.kong_deltas[viewer]) + terminal_heuristic(&sim, profile)
}

fn resolve_discard_step(
    sim: &mut SimState,
    discarder: usize,
    tile_idx: usize,
    profile: L3Profile,
    rng: &mut SplitMix64,
) -> Option<f64> {
    if let Some(ron_score) = check_ron_reactions(sim, discarder, tile_idx, None, profile) {
        return Some(ron_score);
    }

    let wall_rem = sim.wall_len();
    if wall_rem == 0 {
        return Some(f64::from(sim.kong_deltas[usize::from(sim.viewer)]));
    }

    // Fast L1 Pon check in counter-clockwise order.
    let viewer = usize::from(sim.viewer);
    let tile_bit = 1u64 << tile_idx;
    let tile = Tile::new(tile_idx as u8).expect("idx bounded by Tile::COUNT");

    for offset in 1..4 {
        let opp = (discarder + offset) % 4;
        if (sim.passed_pon_masks[opp] & tile_bit) != 0 || sim.hands[opp][tile_idx] < 2 {
            continue;
        }
        let melds_count = usize::from(sim.melds[opp].count);
        let cur_sh = fast_rollout_shanten(
            &sim.hands[opp],
            melds_count,
            sim.allow_four_of_kind_seven_pairs,
        );
        if cur_sh <= 0 {
            sim.passed_pon_masks[opp] |= tile_bit;
            continue;
        }
        sim.hands[opp][tile_idx] -= 2;
        let after_sh = best_shanten_after_one_discard(
            &mut sim.hands[opp],
            melds_count + 1,
            sim.allow_four_of_kind_seven_pairs,
        );
        if after_sh < cur_sh {
            sim.melds[opp] = sim.melds[opp].with_added_pon(tile);
            sim.pon_masks[opp] |= tile_bit;
            sim.passed_hu_mask &= !(1u8 << opp);
            sim.passed_hu_tile_masks[opp] = 0;
            if opp != viewer {
                sim.public_remaining[tile_idx] = sim.public_remaining[tile_idx].saturating_sub(2);
            }
            sim.drawn_tile = NO_DRAWN_TILE;
            sim.current_actor = opp as u8;
            sim.current_turn_source = TurnSource::AfterMeld;
            let _ = rng;
            return None;
        }
        sim.hands[opp][tile_idx] += 2;
        sim.passed_pon_masks[opp] |= tile_bit;
    }

    let Some(drawn) = sim.pop_wall_head() else {
        return Some(f64::from(sim.kong_deltas[viewer]));
    };
    let next_seat = (discarder + 1) % 4;
    sim.hands[next_seat][usize::from(drawn)] += 1;
    sim.passed_hu_mask &= !(1u8 << next_seat);
    sim.passed_hu_tile_masks[next_seat] = 0;
    sim.passed_pon_masks[next_seat] = 0;
    sim.drawn_tile = drawn;
    sim.current_actor = next_seat as u8;
    sim.current_turn_source = TurnSource::NormalDraw;
    None
}

fn check_ron_reactions(
    sim: &SimState,
    discarder: usize,
    tile_idx: usize,
    skip_seat: Option<usize>,
    profile: L3Profile,
) -> Option<f64> {
    let tile_bit = 1u64 << tile_idx;
    let tile = Tile::new(tile_idx as u8).expect("idx bounded by Tile::COUNT");
    let viewer = usize::from(sim.viewer);
    let input = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: sim.wall_len(),
    };
    let rules = sim.scoring_rules();

    let mut any_winner = false;
    let mut viewer_delta = 0.0f64;

    for offset in 1..4 {
        let reactor = (discarder + offset) % 4;
        if skip_seat == Some(reactor) {
            continue;
        }
        if (sim.passed_hu_mask & (1u8 << reactor)) != 0
            || (sim.passed_hu_tile_masks[reactor] & tile_bit) != 0
            || (sim.wait_masks[reactor] & tile_bit) == 0
        {
            continue;
        }
        if let Some(eval) =
            evaluate_win_with_extra(&sim.hands[reactor], tile, &sim.melds[reactor], input, rules)
        {
            any_winner = true;
            if reactor == viewer {
                viewer_delta += f64::from(eval.base_points);
            }
            if discarder == viewer {
                viewer_delta -= f64::from(eval.base_points) * profile.risk_weight;
            }
        }
    }

    any_winner.then(|| f64::from(sim.kong_deltas[viewer]) + viewer_delta)
}

fn check_rob_kong_hu(
    sim: &SimState,
    actor: usize,
    tile_idx: usize,
    profile: L3Profile,
) -> Option<f64> {
    let tile_bit = 1u64 << tile_idx;
    let tile = Tile::new(tile_idx as u8).expect("idx bounded by Tile::COUNT");
    let viewer = usize::from(sim.viewer);
    let input = ScoringInput {
        win_kind: WinKind::RobKongHu,
        turn_source: None,
        wall_remaining: sim.wall_len(),
    };
    let rules = sim.scoring_rules();

    let mut any_rob = false;
    let mut viewer_delta = 0.0f64;

    for offset in 1..4 {
        let reactor = (actor + offset) % 4;
        if (sim.passed_hu_mask & (1u8 << reactor)) != 0
            || (sim.passed_hu_tile_masks[reactor] & tile_bit) != 0
            || (sim.wait_masks[reactor] & tile_bit) == 0
        {
            continue;
        }
        if let Some(eval) =
            evaluate_win_with_extra(&sim.hands[reactor], tile, &sim.melds[reactor], input, rules)
        {
            any_rob = true;
            if reactor == viewer {
                viewer_delta += f64::from(eval.base_points);
            }
            if actor == viewer {
                viewer_delta -=
                    (f64::from(eval.base_points) + 4.0) * profile.rob_kong_risk_multiplier;
            }
        }
    }

    any_rob.then(|| f64::from(sim.kong_deltas[viewer]) + viewer_delta)
}

fn add_kong_delta(sim: &mut SimState, actor: usize, kind: KongKind, from_seat: Option<usize>) {
    let _ = kong_net_delta(kind);
    match kind {
        KongKind::ExposedKong => {
            if let Some(payer) = from_seat {
                sim.kong_deltas[actor] += 2;
                sim.kong_deltas[payer] -= 2;
            }
        }
        KongKind::AddedKong => {
            for s in 0..4 {
                if s != actor {
                    sim.kong_deltas[s] -= 1;
                    sim.kong_deltas[actor] += 1;
                }
            }
        }
        KongKind::ConcealedKong => {
            for s in 0..4 {
                if s != actor {
                    sim.kong_deltas[s] -= 2;
                    sim.kong_deltas[actor] += 2;
                }
            }
        }
    }
}

fn maybe_rollout_concealed_kong(sim: &SimState, actor: usize) -> Option<usize> {
    let melds_count = usize::from(sim.melds[actor].count);
    for idx in 0..Tile::COUNT {
        if sim.hands[actor][idx] == 4 {
            let mut work = sim.hands[actor];
            let cur_sh = best_shanten_after_one_discard(
                &mut work,
                melds_count,
                sim.allow_four_of_kind_seven_pairs,
            );
            if cur_sh > 0 {
                continue;
            }
            work[idx] -= 4;
            let kong_sh =
                fast_rollout_shanten(&work, melds_count + 1, sim.allow_four_of_kind_seven_pairs);
            if kong_sh <= 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn choose_rollout_discard(sim: &mut SimState, actor: usize, rng: &mut SplitMix64) -> (usize, i8) {
    let melds_count = usize::from(sim.melds[actor].count);
    let allow_quad = sim.allow_four_of_kind_seven_pairs;

    let mut best_sh = i8::MAX;
    let mut tied = [0usize; 14];
    let mut tied_len = 0usize;

    for idx in 0..Tile::COUNT {
        if sim.hands[actor][idx] == 0 {
            continue;
        }
        sim.hands[actor][idx] -= 1;
        let sh = fast_rollout_shanten(&sim.hands[actor], melds_count, allow_quad);
        sim.hands[actor][idx] += 1;

        if sh < best_sh {
            best_sh = sh;
            tied[0] = idx;
            tied_len = 1;
        } else if sh == best_sh && tied_len < tied.len() {
            tied[tied_len] = idx;
            tied_len += 1;
        }
    }

    if tied_len == 1 {
        return (tied[0], best_sh);
    }

    let mut best_idx = tied[0];
    if best_sh <= 1 {
        let mut best_ukeire = 0u16;
        let mut best_ties = [0usize; 14];
        let mut best_ties_len = 0usize;
        for &idx in &tied[..tied_len] {
            sim.hands[actor][idx] -= 1;
            let mask = compute_wait_mask(&sim.hands[actor], &sim.melds[actor], allow_quad);
            sim.hands[actor][idx] += 1;
            let uk = ukeire_from_mask(mask, &sim.public_remaining);
            if best_ties_len == 0 || uk > best_ukeire {
                best_ukeire = uk;
                best_ties[0] = idx;
                best_ties_len = 1;
            } else if uk == best_ukeire && best_ties_len < best_ties.len() {
                best_ties[best_ties_len] = idx;
                best_ties_len += 1;
            }
        }
        best_idx = if best_ties_len == 1 {
            best_ties[0]
        } else {
            best_ties[rng.gen_range_usize(best_ties_len)]
        };
    } else {
        let mut best_prio = i32::MIN;
        for &idx in &tied[..tied_len] {
            let prio = rollout_discard_priority(idx, &sim.hands[actor]);
            if prio > best_prio {
                best_prio = prio;
                best_idx = idx;
            }
        }
    }

    (best_idx, best_sh)
}

#[inline]
fn rollout_discard_priority(idx: usize, hand: &[u8; Tile::COUNT]) -> i32 {
    if idx >= 27 {
        let c = i32::from(hand[idx]);
        return if c == 1 { 100 } else { -20 * c };
    }
    let rank = idx % 9;
    let mut score = 0i32;
    if rank == 0 || rank == 8 {
        score += 30;
    } else if rank == 1 || rank == 7 {
        score += 12;
    }
    if rank > 0 && hand[idx - 1] > 0 {
        score -= 25;
    }
    if rank < 8 && hand[idx + 1] > 0 {
        score -= 25;
    }
    if rank > 1 && hand[idx - 2] > 0 {
        score -= 10;
    }
    if rank < 7 && hand[idx + 2] > 0 {
        score -= 10;
    }
    if hand[idx] >= 2 {
        score -= 30;
    }
    score
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
        let s = fast_rollout_shanten(hand, melds, allow_quad);
        hand[idx] += 1;
        if s < best {
            best = s;
        }
    }
    best
}

#[inline]
fn fast_rollout_shanten(counts: &[u8; Tile::COUNT], melds: usize, allow_quad: bool) -> i8 {
    let riichi = core_counts_to_riichi(counts);
    let std_or_riichi = match calculate_replacement_number(&riichi, PlayerCount::Four) {
        Ok(rn) => (rn as i8) - 1,
        Err(_) => crate::shanten::shanten_standard(counts, melds),
    };
    if melds > 0 {
        return std_or_riichi;
    }
    let s_7p = shanten_seven_pairs(counts, allow_quad);
    let s_13o = shanten_thirteen_orphans(counts);
    let mut best = std_or_riichi.min(s_7p);
    if s_13o <= THIRTEEN_ORPHANS_MAX_SHANTEN {
        best = best.min(s_13o);
    }
    best
}

fn compute_wait_mask(counts: &[u8; Tile::COUNT], melds: &MeldSummary, allow_quad: bool) -> u64 {
    let melds_count = usize::from(melds.count);
    let riichi = core_counts_to_riichi(counts);
    let mut mask = 0u64;
    if let Ok((rn, r_mask)) = calculate_necessary_tiles(&riichi, PlayerCount::Four) {
        let s_riichi = (rn as i8) - 1;
        if melds_count > 0 {
            return if s_riichi == 0 {
                riichi_mask_to_core(r_mask)
            } else {
                0
            };
        }
        let s_7p = shanten_seven_pairs(counts, allow_quad);
        let s_13o = shanten_thirteen_orphans(counts);
        let eff = s_riichi.min(s_7p).min(s_13o);
        if eff > 0 {
            return 0;
        }
        if s_riichi == 0 {
            mask |= riichi_mask_to_core(r_mask);
        }
        if s_7p == 0 {
            mask |= necessary_tiles_seven_pairs_mask(counts, allow_quad);
        }
        if s_13o == 0 {
            mask |= necessary_tiles_thirteen_orphans_mask(counts);
        }
        return mask;
    }
    0
}

fn terminal_heuristic(sim: &SimState, profile: L3Profile) -> f64 {
    let viewer = usize::from(sim.viewer);
    let allow_quad = sim.allow_four_of_kind_seven_pairs;
    let wall_frac = (f64::from(sim.wall_len()) / 40.0).clamp(0.15, 1.0);

    let v_sh = fast_rollout_shanten(
        &sim.hands[viewer],
        usize::from(sim.melds[viewer].count),
        allow_quad,
    );
    let v_scale = estimate_hand_value_scale(
        &sim.hands[viewer],
        &sim.melds[viewer],
        allow_quad,
        sim.fan_cap,
    );
    let mut score = match v_sh {
        ..=0 => {
            let uk = ukeire_from_mask(sim.wait_masks[viewer], &sim.public_remaining);
            let live_factor = if uk == 0 {
                0.0
            } else {
                let u = f64::from(uk);
                u / (u + 4.0)
            };
            0.55 * wall_frac * live_factor * v_scale
        }
        1 => 0.16 * wall_frac * v_scale,
        2 => 0.04 * wall_frac * v_scale,
        _ => -0.02,
    };

    for opp in 0..4 {
        if opp == viewer {
            continue;
        }
        let o_sh = fast_rollout_shanten(
            &sim.hands[opp],
            usize::from(sim.melds[opp].count),
            allow_quad,
        );
        let o_scale =
            estimate_hand_value_scale(&sim.hands[opp], &sim.melds[opp], allow_quad, sim.fan_cap);
        score -= profile.risk_weight
            * wall_frac
            * match o_sh {
                ..=0 => 0.30 * o_scale,
                1 => 0.08 * o_scale,
                _ => 0.0,
            };
    }

    score
}

fn estimate_hand_value_scale(
    hand: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    allow_quad: bool,
    fan_cap: u8,
) -> f64 {
    let mut total = [0u8; Tile::COUNT];
    for idx in 0..Tile::COUNT {
        total[idx] = hand[idx].saturating_add(melds.counts[idx]);
    }
    let has_man = total[0..9].iter().any(|&c| c > 0);
    let has_bamboo = total[9..18].iter().any(|&c| c > 0);
    let has_dot = total[18..27].iter().any(|&c| c > 0);
    let has_honor = total[27..Tile::COUNT].iter().any(|&c| c > 0);
    let num_suits = u8::from(has_man) + u8::from(has_bamboo) + u8::from(has_dot);

    let mut fan_est: f64 = if num_suits == 0 && has_honor {
        4.0
    } else if num_suits == 1 && !has_honor {
        2.0
    } else if num_suits == 1 {
        1.0
    } else {
        0.0
    };

    if melds.count == 0 && shanten_seven_pairs(hand, allow_quad) <= 0 {
        let has_quad = allow_quad && hand.contains(&4);
        fan_est += if has_quad { 2.0 } else { 1.0 };
    } else if melds.is_menqing {
        fan_est += 1.0;
    }

    let capped = fan_est.min(f64::from(fan_cap));
    // Expected average payout scale (~1.8 * base_points accounting for Tsumo 3x + Ron 1x).
    1.5 * 2.0_f64.powf(capped)
}

#[derive(Clone, Copy)]
struct SplitMix64(u64);

impl SplitMix64 {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[inline]
    fn gen_range_u64(&mut self, bound: u64) -> u64 {
        debug_assert!(bound > 0);
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let candidate = self.next_u64();
            if candidate >= threshold {
                return candidate % bound;
            }
        }
    }

    #[inline]
    fn gen_range_usize(&mut self, bound: usize) -> usize {
        self.gen_range_u64(bound as u64) as usize
    }
}

#[inline]
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
fn derive_sample_seed(master_seed: u64, sample_idx: usize) -> u64 {
    splitmix64(master_seed ^ (sample_idx as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93))
}

#[inline]
fn derive_rollout_seed(master_seed: u64, cand_idx: usize, sample_idx: usize) -> u64 {
    let c = (cand_idx as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let s = (sample_idx as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    splitmix64(master_seed ^ c ^ s)
}

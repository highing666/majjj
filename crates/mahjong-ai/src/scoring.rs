//! Zero-allocation exact Tuidaohu (`推倒胡`) win evaluator and kong delta scorer.

#[cfg(test)]
use mahjong_core::RevealedMeld;
use mahjong_core::{KongKind, MeldView, RuleProfile, Tile, TurnSource, WinKind};

/// `基本胡` (0 fan): emitted only when no other fan items apply.
pub const BASIC_HU: u16 = 1 << 0;
/// `门前清` (+1 fan): no open melds (`Pon`, `ExposedKong`, `AddedKong`, `Chi`); `ConcealedKong` preserves it.
pub const MENQING: u16 = 1 << 1;
/// `碰碰胡` (+1 fan): four triplets/kongs plus one pair.
pub const ALL_PUNGS: u16 = 1 << 2;
/// `混一色` (+1 fan): single number suit combined with honors.
pub const HALF_FLUSH: u16 = 1 << 3;
/// `清一色` (+2 fan): single number suit only (replaces `混一色`).
pub const FULL_FLUSH: u16 = 1 << 4;
/// `七对` (+1 fan): seven pairs with no melds (excludes `门前清`).
pub const SEVEN_PAIRS: u16 = 1 << 5;
/// `豪华七对` (+2 fan): seven pairs containing at least one 4-of-a-kind (replaces `七对`, excludes `门前清`).
pub const LUXURY_SEVEN_PAIRS: u16 = 1 << 6;
/// `字一色` (4 fan): honor tiles only (replaces `混一色`/`清一色`).
pub const ALL_HONORS: u16 = 1 << 7;
/// `十三幺` (4 fan): all 13 terminal/honor kinds with one duplicate (excludes `门前清`).
pub const THIRTEEN_ORPHANS: u16 = 1 << 8;
/// `杠上开花` (+1 fan): `Tsumo` directly from a kong replacement draw.
pub const SELF_DRAW_AFTER_KONG: u16 = 1 << 9;
/// `抢杠胡` (+1 fan): winning on an opponent's attempted `AddedKong`.
pub const ROB_KONG_HU: u16 = 1 << 10;
/// `海底捞月` (+1 fan): `Tsumo` on the final wall tile (`wall_remaining == 0`).
pub const LAST_TILE_TSUMO: u16 = 1 << 11;
/// `河底捞鱼` (+1 fan): `Ron` on the final discard after `wall_remaining == 0`.
pub const LAST_TILE_RON: u16 = 1 << 12;

/// Static ruleset flags needed for exact Tuidaohu scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoringRules {
    /// Whether 4-of-a-kind counts as two pairs in `豪华七对`.
    pub allow_four_of_kind_seven_pairs: bool,
    /// Maximum capped fan per winning hand.
    pub fan_cap: u8,
}

impl ScoringRules {
    /// Extracts scoring parameters from a [`RuleProfile`].
    #[must_use]
    pub const fn from_profile(profile: &RuleProfile) -> Self {
        Self {
            allow_four_of_kind_seven_pairs: profile.allow_four_of_kind_seven_pairs,
            fan_cap: profile.fan_cap,
        }
    }
}

impl Default for ScoringRules {
    fn default() -> Self {
        Self {
            allow_four_of_kind_seven_pairs: true,
            fan_cap: 4,
        }
    }
}

/// Situational flags describing how a winning hand was completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoringInput {
    /// Win method (`Tsumo`, `Ron`, or `RobKongHu`).
    pub win_kind: WinKind,
    /// Turn origin (`ReplacementDraw` enables `杠上开花`, `NormalDraw|ReplacementDraw` at `wall == 0` enables `海底捞月`).
    pub turn_source: Option<TurnSource>,
    /// Remaining wall tiles at the moment of the win.
    pub wall_remaining: u8,
}

/// Stack-only summary of a seat's declared melds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeldSummary {
    /// Number of declared melds (`0..=4`).
    pub count: u8,
    /// Whether `门前清` is still intact (no `Pon`, `ExposedKong`, `AddedKong`, or `Chi`).
    pub is_menqing: bool,
    /// Whether every declared meld is a triplet or kong.
    pub all_pungs_or_kongs: bool,
    /// Tile frequency table across all declared melds.
    pub counts: [u8; Tile::COUNT],
}

impl Default for MeldSummary {
    fn default() -> Self {
        Self::empty()
    }
}

enum MeldShape {
    Pon(Tile),
    ExposedKong(Tile),
    AddedKong(Tile),
    ConcealedKong(Option<Tile>),
    Chi([Tile; 3]),
}

impl From<&MeldView> for MeldShape {
    fn from(meld: &MeldView) -> Self {
        match *meld {
            MeldView::Pon { tile, .. } => Self::Pon(tile),
            MeldView::ExposedKong { tile, .. } => Self::ExposedKong(tile),
            MeldView::AddedKong { tile, .. } => Self::AddedKong(tile),
            MeldView::ConcealedKong { tile, .. } => Self::ConcealedKong(tile),
            MeldView::Chi { tiles, .. } => Self::Chi(tiles),
        }
    }
}

#[cfg(test)]
impl From<&RevealedMeld> for MeldShape {
    fn from(meld: &RevealedMeld) -> Self {
        match *meld {
            RevealedMeld::Pon { tile, .. } => Self::Pon(tile),
            RevealedMeld::ExposedKong { tile, .. } => Self::ExposedKong(tile),
            RevealedMeld::AddedKong { tile, .. } => Self::AddedKong(tile),
            RevealedMeld::ConcealedKong { tile, .. } => Self::ConcealedKong(Some(tile)),
            RevealedMeld::Chi { tiles, .. } => Self::Chi(tiles),
        }
    }
}

impl MeldSummary {
    /// Empty meld summary (closed hand).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            count: 0,
            is_menqing: true,
            all_pungs_or_kongs: true,
            counts: [0; Tile::COUNT],
        }
    }

    fn from_meld_shapes(shapes: impl IntoIterator<Item = MeldShape>) -> Self {
        let mut summary = Self::empty();
        for shape in shapes {
            summary.count = summary.count.saturating_add(1);
            match shape {
                MeldShape::Pon(tile) => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(3);
                }
                MeldShape::ExposedKong(tile) | MeldShape::AddedKong(tile) => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(4);
                }
                MeldShape::ConcealedKong(tile) => {
                    if let Some(t) = tile {
                        summary.counts[t.index()] = summary.counts[t.index()].saturating_add(4);
                    }
                }
                MeldShape::Chi(tiles) => {
                    summary.is_menqing = false;
                    summary.all_pungs_or_kongs = false;
                    for t in tiles {
                        summary.counts[t.index()] = summary.counts[t.index()].saturating_add(1);
                    }
                }
            }
        }
        summary
    }

    /// Builds a summary from public [`MeldView`] entries.
    #[must_use]
    pub fn from_meld_views(melds: &[MeldView]) -> Self {
        Self::from_meld_shapes(melds.iter().map(MeldShape::from))
    }

    /// Builds a summary from settled [`RevealedMeld`] entries.
    #[cfg(test)]
    #[must_use]
    pub fn from_revealed_melds(melds: &[RevealedMeld]) -> Self {
        Self::from_meld_shapes(melds.iter().map(MeldShape::from))
    }

    /// Returns a copy with an added `Pon` meld (`门前清` lost).
    #[must_use]
    pub const fn with_added_pon(mut self, tile: Tile) -> Self {
        self.count = self.count.saturating_add(1);
        self.is_menqing = false;
        self.counts[tile.index()] = self.counts[tile.index()].saturating_add(3);
        self
    }

    /// Returns a copy with an added `ExposedKong` meld (`门前清` lost).
    #[must_use]
    pub const fn with_added_exposed_kong(mut self, tile: Tile) -> Self {
        self.count = self.count.saturating_add(1);
        self.is_menqing = false;
        self.counts[tile.index()] = self.counts[tile.index()].saturating_add(4);
        self
    }

    /// Returns a copy with an added `ConcealedKong` meld (`门前清` preserved).
    #[must_use]
    pub const fn with_added_concealed_kong(mut self, tile: Tile) -> Self {
        self.count = self.count.saturating_add(1);
        self.counts[tile.index()] = self.counts[tile.index()].saturating_add(4);
        self
    }

    /// Returns a copy with an existing `Pon` upgraded to `AddedKong`.
    #[must_use]
    pub const fn with_upgraded_added_kong(mut self, tile: Tile) -> Self {
        self.counts[tile.index()] = self.counts[tile.index()].saturating_add(1);
        self
    }
}

/// Zero-alloc exact scoring result for a winning hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinEval {
    /// Bitmask of active fan flags (`BASIC_HU ..= LAST_TILE_RON`).
    pub fan_mask: u16,
    /// Uncapped total fan sum across active items.
    pub raw_fan: u16,
    /// Effective total fan capped at `RuleProfile::fan_cap`.
    pub total_fan: u8,
    /// Single-payer base points (`1 << total_fan`).
    pub base_points: i32,
}

impl WinEval {
    /// Net points gained by the winner (`3 * base_points` on `Tsumo`, `base_points` on `Ron`/`RobKongHu`).
    #[must_use]
    pub const fn winner_payout(self, win_kind: WinKind) -> i32 {
        match win_kind {
            WinKind::Tsumo => 3 * self.base_points,
            WinKind::Ron | WinKind::RobKongHu => self.base_points,
        }
    }
}

/// Evaluates a complete `(4 - melds.count) * 3 + 2` closed hand + `melds` with zero heap allocations.
#[must_use]
pub fn evaluate_win(
    closed_counts: &[u8; Tile::COUNT],
    melds: &MeldSummary,
    input: ScoringInput,
    rules: ScoringRules,
) -> Option<WinEval> {
    if melds.count > 4 {
        return None;
    }
    let expected_closed = usize::from(4 - melds.count) * 3 + 2;
    let actual_closed: usize = closed_counts.iter().map(|&c| usize::from(c)).sum();
    if actual_closed != expected_closed {
        return None;
    }

    let mut total_counts = [0u8; Tile::COUNT];
    for idx in 0..Tile::COUNT {
        let total = closed_counts[idx].saturating_add(melds.counts[idx]);
        if total > 4 {
            return None;
        }
        total_counts[idx] = total;
    }

    let (flush_mask, flush_fan) = compute_flush_fan(&total_counts);
    let (sit_mask, sit_fan) = compute_situational_fan(input);

    let mut best: Option<WinEval> = None;

    if melds.count == 0 {
        if is_thirteen_orphans(closed_counts) {
            let cand = finalize(THIRTEEN_ORPHANS | sit_mask, 4 + sit_fan, rules.fan_cap);
            best = pick_best(best, cand);
        }

        if let Some(has_quad) =
            check_seven_pairs(closed_counts, rules.allow_four_of_kind_seven_pairs)
        {
            let (pair_mask, pair_fan) = if has_quad {
                (LUXURY_SEVEN_PAIRS, 2)
            } else {
                (SEVEN_PAIRS, 1)
            };
            let cand = finalize(
                flush_mask | pair_mask | sit_mask,
                flush_fan + pair_fan + sit_fan,
                rules.fan_cap,
            );
            best = pick_best(best, cand);
        }
    }

    let mut work = *closed_counts;
    let (has_standard, has_all_pungs_closed) = analyze_standard_decompositions(&mut work);
    if has_standard {
        let mut mask = flush_mask | sit_mask;
        let mut raw_fan = flush_fan + sit_fan;
        if melds.all_pungs_or_kongs && has_all_pungs_closed {
            mask |= ALL_PUNGS;
            raw_fan += 1;
        }
        if melds.is_menqing {
            mask |= MENQING;
            raw_fan += 1;
        }
        if mask == 0 {
            mask = BASIC_HU;
        }
        let cand = finalize(mask, raw_fan, rules.fan_cap);
        best = pick_best(best, cand);
    }

    best
}

/// Convenience zero-alloc helper evaluating `closed_counts` plus one candidate `extra_tile`.
#[must_use]
pub fn evaluate_win_with_extra(
    closed_counts: &[u8; Tile::COUNT],
    extra_tile: Tile,
    melds: &MeldSummary,
    input: ScoringInput,
    rules: ScoringRules,
) -> Option<WinEval> {
    let idx = extra_tile.index();
    if closed_counts[idx].saturating_add(melds.counts[idx]) >= 4 {
        return None;
    }
    let mut with_extra = *closed_counts;
    with_extra[idx] += 1;
    evaluate_win(&with_extra, melds, input, rules)
}

/// Net kong score gained by the actor (`ExposedKong = +2`, `AddedKong = +3`, `ConcealedKong = +6`).
#[must_use]
pub const fn kong_net_delta(kind: KongKind) -> i32 {
    match kind {
        KongKind::ExposedKong => 2,
        KongKind::AddedKong => 3,
        KongKind::ConcealedKong => 6,
    }
}

#[cfg(test)]
fn kong_net_score_from_shapes(shapes: impl IntoIterator<Item = MeldShape>) -> i32 {
    let mut total = 0i32;
    for shape in shapes {
        match shape {
            MeldShape::ExposedKong(_) => total += kong_net_delta(KongKind::ExposedKong),
            MeldShape::AddedKong(_) => total += kong_net_delta(KongKind::AddedKong),
            MeldShape::ConcealedKong(_) => total += kong_net_delta(KongKind::ConcealedKong),
            MeldShape::Pon(_) | MeldShape::Chi(_) => {}
        }
    }
    total
}

/// Sums the actor's already-confirmed kong net points across `melds`.
#[cfg(test)]
#[must_use]
pub fn existing_kong_net_score(melds: &[MeldView]) -> i32 {
    kong_net_score_from_shapes(melds.iter().map(MeldShape::from))
}

/// Sums the actor's already-confirmed kong net points across settled [`RevealedMeld`] entries.
#[cfg(test)]
#[must_use]
pub fn existing_revealed_kong_net_score(melds: &[RevealedMeld]) -> i32 {
    kong_net_score_from_shapes(melds.iter().map(MeldShape::from))
}

fn finalize(fan_mask: u16, raw_fan: u16, fan_cap: u8) -> WinEval {
    let total_fan = raw_fan.min(u16::from(fan_cap)) as u8;
    let base_points = 1i32.checked_shl(u32::from(total_fan)).unwrap_or(i32::MAX);
    WinEval {
        fan_mask,
        raw_fan,
        total_fan,
        base_points,
    }
}

fn pick_best(best: Option<WinEval>, candidate: WinEval) -> Option<WinEval> {
    match best {
        None => Some(candidate),
        Some(prev) => {
            if (candidate.total_fan, candidate.raw_fan) > (prev.total_fan, prev.raw_fan) {
                Some(candidate)
            } else {
                Some(prev)
            }
        }
    }
}

fn compute_flush_fan(total_counts: &[u8; Tile::COUNT]) -> (u16, u16) {
    let has_man = total_counts[0..9].iter().any(|&c| c > 0);
    let has_bamboo = total_counts[9..18].iter().any(|&c| c > 0);
    let has_dot = total_counts[18..27].iter().any(|&c| c > 0);
    let has_honor = total_counts[27..Tile::COUNT].iter().any(|&c| c > 0);
    let number_suits = u8::from(has_man) + u8::from(has_bamboo) + u8::from(has_dot);

    if number_suits == 0 && has_honor {
        (ALL_HONORS, 4)
    } else if number_suits == 1 && !has_honor {
        (FULL_FLUSH, 2)
    } else if number_suits == 1 && has_honor {
        (HALF_FLUSH, 1)
    } else {
        (0, 0)
    }
}

fn compute_situational_fan(input: ScoringInput) -> (u16, u16) {
    let mut mask = 0u16;
    let mut fan = 0u16;
    if input.win_kind == WinKind::Tsumo && input.turn_source == Some(TurnSource::ReplacementDraw) {
        mask |= SELF_DRAW_AFTER_KONG;
        fan += 1;
    }
    if input.win_kind == WinKind::RobKongHu {
        mask |= ROB_KONG_HU;
        fan += 1;
    }
    if input.win_kind == WinKind::Tsumo
        && input.wall_remaining == 0
        && matches!(
            input.turn_source,
            Some(TurnSource::NormalDraw | TurnSource::ReplacementDraw)
        )
    {
        mask |= LAST_TILE_TSUMO;
        fan += 1;
    }
    if input.win_kind == WinKind::Ron && input.wall_remaining == 0 {
        mask |= LAST_TILE_RON;
        fan += 1;
    }
    (mask, fan)
}

fn is_thirteen_orphans(counts: &[u8; Tile::COUNT]) -> bool {
    counts.iter().enumerate().all(|(idx, &count)| {
        let tile = Tile::new(idx as u8).expect("index bounded by Tile::COUNT");
        if tile.is_terminal_or_honor() {
            count >= 1
        } else {
            count == 0
        }
    })
}

fn check_seven_pairs(counts: &[u8; Tile::COUNT], allow_quad: bool) -> Option<bool> {
    let mut has_quad = false;
    for &c in counts {
        match c {
            0 | 2 => {}
            4 if allow_quad => has_quad = true,
            _ => return None,
        }
    }
    Some(has_quad)
}

fn analyze_standard_decompositions(counts: &mut [u8; Tile::COUNT]) -> (bool, bool) {
    let mut found_any = false;
    let mut found_all_pungs = false;

    for pair_idx in 0..Tile::COUNT {
        if counts[pair_idx] >= 2 {
            counts[pair_idx] -= 2;
            partition_melds(counts, true, &mut found_any, &mut found_all_pungs);
            counts[pair_idx] += 2;
            if found_any && found_all_pungs {
                break;
            }
        }
    }

    (found_any, found_all_pungs)
}

fn partition_melds(
    counts: &mut [u8; Tile::COUNT],
    all_triplets_so_far: bool,
    found_any: &mut bool,
    found_all_pungs: &mut bool,
) {
    if *found_any && (*found_all_pungs || !all_triplets_so_far) {
        return;
    }
    let Some(first) = counts.iter().position(|&c| c > 0) else {
        *found_any = true;
        if all_triplets_so_far {
            *found_all_pungs = true;
        }
        return;
    };

    if counts[first] >= 3 {
        counts[first] -= 3;
        partition_melds(counts, all_triplets_so_far, found_any, found_all_pungs);
        counts[first] += 3;
    }

    if (!*found_any)
        && first < 27
        && first % 9 <= 6
        && counts[first + 1] > 0
        && counts[first + 2] > 0
    {
        counts[first] -= 1;
        counts[first + 1] -= 1;
        counts[first + 2] -= 1;
        partition_melds(counts, false, found_any, found_all_pungs);
        counts[first] += 1;
        counts[first + 1] += 1;
        counts[first + 2] += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::{counts_from_tiles, tile as tile_one, tiles};
    use mahjong_core::{Ruleset, Seat, TuidaohuRuleset, WinEvalInput};
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn expected_flag_for_code(code: &str) -> u16 {
        match code {
            "basic_hu" => BASIC_HU,
            "menqing" => MENQING,
            "all_pungs" => ALL_PUNGS,
            "half_flush" => HALF_FLUSH,
            "full_flush" => FULL_FLUSH,
            "seven_pairs" => SEVEN_PAIRS,
            "luxury_seven_pairs" => LUXURY_SEVEN_PAIRS,
            "all_honors" => ALL_HONORS,
            "thirteen_orphans" => THIRTEEN_ORPHANS,
            "self_draw_after_kong" => SELF_DRAW_AFTER_KONG,
            "rob_kong_hu" => ROB_KONG_HU,
            "last_tile_tsumo" => LAST_TILE_TSUMO,
            "last_tile_ron" => LAST_TILE_RON,
            other => panic!("unexpected Tuidaohu score item code: {other}"),
        }
    }

    fn assert_scorer_parity(
        closed_tiles: &[Tile],
        extra_tile: Option<Tile>,
        melds: &[RevealedMeld],
        win_kind: WinKind,
        turn_source: Option<TurnSource>,
        wall_remaining: u8,
        label: &str,
    ) {
        let ruleset = TuidaohuRuleset;
        let profile = ruleset.profile();
        let expected = ruleset.evaluate_win(&WinEvalInput {
            closed_tiles,
            extra_tile,
            melds,
            win_kind,
            turn_source,
            wall_remaining,
        });

        let mut counts = counts_from_tiles(closed_tiles);
        if let Some(t) = extra_tile {
            counts[t.index()] += 1;
        }
        let meld_summary = MeldSummary::from_revealed_melds(melds);
        let rules = ScoringRules::from_profile(&profile);
        let actual = evaluate_win(
            &counts,
            &meld_summary,
            ScoringInput {
                win_kind,
                turn_source,
                wall_remaining,
            },
            rules,
        );

        match (expected, actual) {
            (None, None) => {}
            (Some(exp), Some(act)) => {
                assert_eq!(
                    act.total_fan, exp.total_fan,
                    "total_fan mismatch ({label}): exp={exp:?}, act={act:?}"
                );
                assert_eq!(
                    act.base_points, exp.base_points,
                    "base_points mismatch ({label})"
                );
                let exp_mask: u16 = exp
                    .items
                    .iter()
                    .map(|item| expected_flag_for_code(&item.code))
                    .fold(0u16, |acc, bit| acc | bit);
                assert_eq!(
                    act.fan_mask, exp_mask,
                    "fan_mask mismatch ({label}): exp_items={:?}, act_mask={:#x}",
                    exp.items, act.fan_mask
                );
                let expected_payout = match win_kind {
                    WinKind::Tsumo => 3 * exp.base_points,
                    WinKind::Ron | WinKind::RobKongHu => exp.base_points,
                };
                assert_eq!(
                    act.winner_payout(win_kind),
                    expected_payout,
                    "winner payout mismatch ({label})"
                );
            }
            (exp, act) => {
                panic!("win/non-win mismatch ({label}): expected={exp:?}, actual={act:?}");
            }
        }
    }

    fn gen_idx(rng: &mut ChaCha8Rng, bound: usize) -> usize {
        (rng.next_u32() as usize) % bound
    }

    fn make_non_overlapping_melds(
        rng: &mut ChaCha8Rng,
        closed: &[Tile],
        meld_count: usize,
    ) -> Option<Vec<RevealedMeld>> {
        let mut used = counts_from_tiles(closed);
        let mut melds = Vec::with_capacity(meld_count);
        for seq_idx in 0..meld_count {
            let mut placed = false;
            for _ in 0..40 {
                let kind = gen_idx(rng, 5);
                let tile_idx = gen_idx(rng, Tile::COUNT);
                let t = Tile::new(tile_idx as u8).unwrap();
                let need = if kind <= 2 { 3 } else { 4 };
                if used[tile_idx] + need <= 4 {
                    used[tile_idx] += need;
                    let seq = (seq_idx + 1) as u64;
                    let meld = match kind {
                        0 | 1 => RevealedMeld::Pon {
                            tile: t,
                            from_seat: Seat::South,
                            seq,
                        },
                        2 => RevealedMeld::ExposedKong {
                            tile: t,
                            from_seat: Seat::West,
                            seq,
                        },
                        3 => RevealedMeld::AddedKong {
                            tile: t,
                            from_seat: Seat::North,
                            seq,
                        },
                        _ => RevealedMeld::ConcealedKong { tile: t, seq },
                    };
                    melds.push(meld);
                    placed = true;
                    break;
                }
            }
            if !placed {
                return None;
            }
        }
        Some(melds)
    }

    #[test]
    fn exact_scorer_matches_tuidaohu_ruleset_across_1200_hands_and_fan_items() {
        let mut seen_flags = 0u16;
        let mut checked = 0usize;

        struct Curated<'a> {
            label: &'a str,
            closed: &'a str,
            extra: Option<&'a str>,
            melds: Vec<RevealedMeld>,
            win_kind: WinKind,
            turn_source: Option<TurnSource>,
            wall_remaining: u8,
            expected_flags: u16,
            expected_fan: u8,
            expected_base: i32,
        }

        let curated = vec![
            Curated {
                label: "basic_hu_open",
                closed: "123m 456p 78s 22p",
                extra: Some("9s"),
                melds: vec![RevealedMeld::Pon {
                    tile: tile_one("E"),
                    from_seat: Seat::South,
                    seq: 1,
                }],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 20,
                expected_flags: BASIC_HU,
                expected_fan: 0,
                expected_base: 1,
            },
            Curated {
                label: "menqing_tsumo",
                closed: "123m 456m 789p 234s 5p",
                extra: Some("5p"),
                melds: vec![],
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::NormalDraw),
                wall_remaining: 30,
                expected_flags: MENQING,
                expected_fan: 1,
                expected_base: 2,
            },
            Curated {
                label: "concealed_kong_keeps_menqing",
                closed: "123m 456p 789s 2s",
                extra: Some("2s"),
                melds: vec![RevealedMeld::ConcealedKong {
                    tile: tile_one("E"),
                    seq: 1,
                }],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 15,
                expected_flags: MENQING,
                expected_fan: 1,
                expected_base: 2,
            },
            Curated {
                label: "all_pungs_open",
                closed: "111m 222p 33s 44s",
                extra: Some("3s"),
                melds: vec![RevealedMeld::Pon {
                    tile: tile_one("9m"),
                    from_seat: Seat::West,
                    seq: 1,
                }],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 10,
                expected_flags: ALL_PUNGS,
                expected_fan: 1,
                expected_base: 2,
            },
            Curated {
                label: "half_flush_with_all_pungs_and_menqing",
                closed: "111m 222m 555m 888m E",
                extra: Some("E"),
                melds: vec![],
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::NormalDraw),
                wall_remaining: 24,
                expected_flags: HALF_FLUSH | ALL_PUNGS | MENQING,
                expected_fan: 3,
                expected_base: 8,
            },
            Curated {
                label: "full_flush_replaces_half_flush_and_caps_at_4",
                closed: "111m 222m 333m 444m 5m",
                extra: Some("5m"),
                melds: vec![],
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::ReplacementDraw),
                wall_remaining: 0,
                expected_flags: FULL_FLUSH
                    | ALL_PUNGS
                    | MENQING
                    | SELF_DRAW_AFTER_KONG
                    | LAST_TILE_TSUMO,
                expected_fan: 4,
                expected_base: 16,
            },
            Curated {
                label: "seven_pairs_excludes_menqing",
                closed: "11m 44m 22p 55p 88s 33s E",
                extra: Some("E"),
                melds: vec![],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 12,
                expected_flags: SEVEN_PAIRS,
                expected_fan: 1,
                expected_base: 2,
            },
            Curated {
                label: "luxury_seven_pairs_non_stacking_quads_plus_half_flush",
                closed: "1111m 2222m 33m 44m E",
                extra: Some("E"),
                melds: vec![],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 12,
                expected_flags: LUXURY_SEVEN_PAIRS | HALF_FLUSH,
                expected_fan: 3,
                expected_base: 8,
            },
            Curated {
                label: "all_honors_caps_at_4_and_no_flush",
                closed: "EEE SSS WW CC",
                extra: Some("C"),
                melds: vec![RevealedMeld::Pon {
                    tile: tile_one("B"),
                    from_seat: Seat::East,
                    seq: 1,
                }],
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::NormalDraw),
                wall_remaining: 18,
                expected_flags: ALL_HONORS | ALL_PUNGS,
                expected_fan: 4,
                expected_base: 16,
            },
            Curated {
                label: "thirteen_orphans_excludes_menqing",
                closed: "19m 19s 19p E S W N C F B",
                extra: Some("1m"),
                melds: vec![],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 40,
                expected_flags: THIRTEEN_ORPHANS,
                expected_fan: 4,
                expected_base: 16,
            },
            Curated {
                label: "rob_kong_hu_and_last_tile_ron",
                closed: "123m 456p 789s 2s",
                extra: Some("2s"),
                melds: vec![RevealedMeld::Pon {
                    tile: tile_one("C"),
                    from_seat: Seat::North,
                    seq: 1,
                }],
                win_kind: WinKind::RobKongHu,
                turn_source: None,
                wall_remaining: 0,
                expected_flags: ROB_KONG_HU,
                expected_fan: 1,
                expected_base: 2,
            },
            Curated {
                label: "last_tile_ron_on_basic_hu",
                closed: "123m 456p 789s 2s",
                extra: Some("2s"),
                melds: vec![RevealedMeld::Pon {
                    tile: tile_one("C"),
                    from_seat: Seat::North,
                    seq: 1,
                }],
                win_kind: WinKind::Ron,
                turn_source: None,
                wall_remaining: 0,
                expected_flags: LAST_TILE_RON,
                expected_fan: 1,
                expected_base: 2,
            },
        ];

        for case in &curated {
            let closed = tiles(case.closed);
            let extra = case.extra.map(tile_one);
            assert_scorer_parity(
                &closed,
                extra,
                &case.melds,
                case.win_kind,
                case.turn_source,
                case.wall_remaining,
                case.label,
            );
            let mut counts = counts_from_tiles(&closed);
            if let Some(t) = extra {
                counts[t.index()] += 1;
            }
            let eval = evaluate_win(
                &counts,
                &MeldSummary::from_revealed_melds(&case.melds),
                ScoringInput {
                    win_kind: case.win_kind,
                    turn_source: case.turn_source,
                    wall_remaining: case.wall_remaining,
                },
                ScoringRules::from_profile(&TuidaohuRuleset.profile()),
            )
            .expect("curated hand wins");
            assert_eq!(eval.fan_mask, case.expected_flags, "case={}", case.label);
            assert_eq!(eval.total_fan, case.expected_fan, "case={}", case.label);
            assert_eq!(eval.base_points, case.expected_base, "case={}", case.label);
            seen_flags |= eval.fan_mask;
            checked += 1;
        }

        let all_13_flags = BASIC_HU
            | MENQING
            | ALL_PUNGS
            | HALF_FLUSH
            | FULL_FLUSH
            | SEVEN_PAIRS
            | LUXURY_SEVEN_PAIRS
            | ALL_HONORS
            | THIRTEEN_ORPHANS
            | SELF_DRAW_AFTER_KONG
            | ROB_KONG_HU
            | LAST_TILE_TSUMO
            | LAST_TILE_RON;
        assert_eq!(seen_flags, all_13_flags, "all 13 fan items covered");

        let quad_7p_counts = counts_from_tiles(&tiles("1111m 22p 33p 55s 77s EE"));
        let melds_empty = MeldSummary::empty();
        let input_ron = ScoringInput {
            win_kind: WinKind::Ron,
            turn_source: None,
            wall_remaining: 20,
        };
        let with_quad = evaluate_win(
            &quad_7p_counts,
            &melds_empty,
            input_ron,
            ScoringRules {
                allow_four_of_kind_seven_pairs: true,
                fan_cap: 4,
            },
        )
        .expect("wins when quad 7p allowed");
        assert_eq!(with_quad.fan_mask, LUXURY_SEVEN_PAIRS);
        assert_eq!(with_quad.total_fan, 2);

        let without_quad = evaluate_win(
            &quad_7p_counts,
            &melds_empty,
            input_ron,
            ScoringRules {
                allow_four_of_kind_seven_pairs: false,
                fan_cap: 4,
            },
        );
        assert!(without_quad.is_none(), "quad 7p rejected when rule off");

        let uncapped_full = evaluate_win(
            &counts_from_tiles(&tiles("111m 222m 333m 444m 55m")),
            &melds_empty,
            ScoringInput {
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::ReplacementDraw),
                wall_remaining: 0,
            },
            ScoringRules {
                allow_four_of_kind_seven_pairs: true,
                fan_cap: 3,
            },
        )
        .expect("capped at 3");
        assert_eq!(uncapped_full.total_fan, 3);
        assert_eq!(uncapped_full.base_points, 8);
        assert_eq!(uncapped_full.winner_payout(WinKind::Tsumo), 24);
        assert_eq!(uncapped_full.winner_payout(WinKind::Ron), 8);

        assert_eq!(kong_net_delta(KongKind::ExposedKong), 2);
        assert_eq!(kong_net_delta(KongKind::AddedKong), 3);
        assert_eq!(kong_net_delta(KongKind::ConcealedKong), 6);
        assert_eq!(
            existing_kong_net_score(&[
                MeldView::Pon {
                    tile: tile_one("1m"),
                    from_seat: Seat::South,
                    seq: 1
                },
                MeldView::ExposedKong {
                    tile: tile_one("2m"),
                    from_seat: Seat::West,
                    seq: 2
                },
                MeldView::AddedKong {
                    tile: tile_one("3m"),
                    from_seat: Seat::North,
                    seq: 3
                },
                MeldView::ConcealedKong {
                    tile: Some(tile_one("4m")),
                    seq: 4
                },
            ]),
            11
        );
        assert_eq!(
            existing_revealed_kong_net_score(&[
                RevealedMeld::ExposedKong {
                    tile: tile_one("2m"),
                    from_seat: Seat::West,
                    seq: 2
                },
                RevealedMeld::AddedKong {
                    tile: tile_one("3m"),
                    from_seat: Seat::North,
                    seq: 3
                },
                RevealedMeld::ConcealedKong {
                    tile: tile_one("4m"),
                    seq: 4
                },
            ]),
            11
        );

        let mut rng = ChaCha8Rng::seed_from_u64(0x2026_0034);
        let situational_cases = [
            (WinKind::Tsumo, Some(TurnSource::NormalDraw), 25u8),
            (WinKind::Tsumo, Some(TurnSource::ReplacementDraw), 12u8),
            (WinKind::Tsumo, Some(TurnSource::ReplacementDraw), 0u8),
            (WinKind::Tsumo, Some(TurnSource::InitialDealer), 83u8),
            (WinKind::Ron, None, 15u8),
            (WinKind::Ron, None, 0u8),
            (WinKind::RobKongHu, None, 18u8),
        ];

        for sample in 0..1_250usize {
            let meld_count = match sample % 10 {
                0..=4 => 0,
                5 | 6 => 1,
                7 | 8 => 2,
                _ => 3,
            };
            let closed_len = (4 - meld_count) * 3 + 2;
            let mut closed = Vec::with_capacity(closed_len);

            if meld_count == 0 && sample % 25 == 0 {
                let yaojiu = tiles("19m 19s 19p E S W N C F B");
                closed.extend_from_slice(&yaojiu);
                let extra = if sample % 50 == 0 {
                    yaojiu[gen_idx(&mut rng, yaojiu.len())]
                } else {
                    tile_one("5m")
                };
                closed.push(extra);
            } else if meld_count == 0 && sample % 7 == 0 {
                let mut counts = [0u8; Tile::COUNT];
                let mut placed = 0usize;
                while placed < 14 {
                    let idx = gen_idx(&mut rng, Tile::COUNT);
                    let add = if placed <= 10 && counts[idx] <= 2 {
                        2
                    } else {
                        1
                    };
                    if counts[idx] + add <= 4 && placed + usize::from(add) <= 14 {
                        counts[idx] += add;
                        placed += usize::from(add);
                        for _ in 0..add {
                            closed.push(Tile::new(idx as u8).unwrap());
                        }
                    }
                }
            } else {
                let suit_mode = sample % 4;
                let mut counts = [0u8; Tile::COUNT];
                let need_melds = 4 - meld_count;
                let mut ok = true;
                for _ in 0..need_melds {
                    let mut made = false;
                    for _ in 0..30 {
                        let is_seq = suit_mode != 3 && gen_idx(&mut rng, 3) != 0;
                        if is_seq {
                            let suit = match suit_mode {
                                1 => 0,
                                2 => gen_idx(&mut rng, 2) * 9,
                                _ => gen_idx(&mut rng, 3) * 9,
                            };
                            let start = suit + gen_idx(&mut rng, 7);
                            if counts[start] < 4 && counts[start + 1] < 4 && counts[start + 2] < 4 {
                                counts[start] += 1;
                                counts[start + 1] += 1;
                                counts[start + 2] += 1;
                                for k in [start, start + 1, start + 2] {
                                    closed.push(Tile::new(k as u8).unwrap());
                                }
                                made = true;
                                break;
                            }
                        } else {
                            let idx = match suit_mode {
                                1 => gen_idx(&mut rng, 9),
                                2 => {
                                    if gen_idx(&mut rng, 2) == 0 {
                                        gen_idx(&mut rng, 9)
                                    } else {
                                        27 + gen_idx(&mut rng, 7)
                                    }
                                }
                                3 => 27 + gen_idx(&mut rng, 7),
                                _ => gen_idx(&mut rng, Tile::COUNT),
                            };
                            if counts[idx] <= 1 {
                                counts[idx] += 3;
                                for _ in 0..3 {
                                    closed.push(Tile::new(idx as u8).unwrap());
                                }
                                made = true;
                                break;
                            }
                        }
                    }
                    if !made {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    for _ in 0..30 {
                        let idx = match suit_mode {
                            1 => gen_idx(&mut rng, 9),
                            2 => {
                                if gen_idx(&mut rng, 2) == 0 {
                                    gen_idx(&mut rng, 9)
                                } else {
                                    27 + gen_idx(&mut rng, 7)
                                }
                            }
                            3 => 27 + gen_idx(&mut rng, 7),
                            _ => gen_idx(&mut rng, Tile::COUNT),
                        };
                        if counts[idx] <= 2 {
                            closed.push(Tile::new(idx as u8).unwrap());
                            closed.push(Tile::new(idx as u8).unwrap());
                            break;
                        }
                    }
                }
                if closed.len() != closed_len {
                    closed.clear();
                    let mut counts2 = [0u8; Tile::COUNT];
                    while closed.len() < closed_len {
                        let idx = gen_idx(&mut rng, Tile::COUNT);
                        if counts2[idx] < 4 {
                            counts2[idx] += 1;
                            closed.push(Tile::new(idx as u8).unwrap());
                        }
                    }
                } else if sample % 5 == 4 {
                    let pop_pos = gen_idx(&mut rng, closed.len());
                    closed.remove(pop_pos);
                    let idx = gen_idx(&mut rng, Tile::COUNT);
                    closed.push(Tile::new(idx as u8).unwrap());
                }
            }

            let melds =
                make_non_overlapping_melds(&mut rng, &closed, meld_count).unwrap_or_default();
            let adj_closed = if melds.len() == meld_count {
                closed
            } else {
                let mut c = [0u8; Tile::COUNT];
                let mut hand = Vec::with_capacity(14);
                while hand.len() < 14 {
                    let idx = gen_idx(&mut rng, Tile::COUNT);
                    if c[idx] < 4 {
                        c[idx] += 1;
                        hand.push(Tile::new(idx as u8).unwrap());
                    }
                }
                hand
            };
            let actual_melds = if adj_closed.len() == (4 - melds.len()) * 3 + 2 {
                melds
            } else {
                Vec::new()
            };

            let (extra_tile, closed_slice) = if sample % 2 == 0 {
                (
                    Some(adj_closed[adj_closed.len() - 1]),
                    &adj_closed[..adj_closed.len() - 1],
                )
            } else {
                (None, adj_closed.as_slice())
            };
            let mut sorted_closed = closed_slice.to_vec();
            sorted_closed.sort();

            let (wk, ts, wr) = situational_cases[sample % situational_cases.len()];
            assert_scorer_parity(
                &sorted_closed,
                extra_tile,
                &actual_melds,
                wk,
                ts,
                wr,
                &format!("rand_{sample}"),
            );
            checked += 1;
        }

        assert!(checked >= 1_200, "checked only {checked} hands");
    }
}

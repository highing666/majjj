//! Zero-allocation exact Tuidaohu (`推倒胡`) win evaluator and kong delta scorer.

use mahjong_core::{KongKind, MeldView, RevealedMeld, RuleProfile, Tile, TurnSource, WinKind};

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

    /// Builds a summary from public [`MeldView`] entries.
    #[must_use]
    pub fn from_meld_views(melds: &[MeldView]) -> Self {
        let mut summary = Self::empty();
        for meld in melds {
            summary.count = summary.count.saturating_add(1);
            match *meld {
                MeldView::Pon { tile, .. } => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(3);
                }
                MeldView::ExposedKong { tile, .. } | MeldView::AddedKong { tile, .. } => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(4);
                }
                MeldView::ConcealedKong { tile, .. } => {
                    if let Some(t) = tile {
                        summary.counts[t.index()] = summary.counts[t.index()].saturating_add(4);
                    }
                }
                MeldView::Chi { tiles, .. } => {
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

    /// Builds a summary from settled [`RevealedMeld`] entries.
    #[must_use]
    pub fn from_revealed_melds(melds: &[RevealedMeld]) -> Self {
        let mut summary = Self::empty();
        for meld in melds {
            summary.count = summary.count.saturating_add(1);
            match *meld {
                RevealedMeld::Pon { tile, .. } => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(3);
                }
                RevealedMeld::ExposedKong { tile, .. } | RevealedMeld::AddedKong { tile, .. } => {
                    summary.is_menqing = false;
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(4);
                }
                RevealedMeld::ConcealedKong { tile, .. } => {
                    summary.counts[tile.index()] = summary.counts[tile.index()].saturating_add(4);
                }
                RevealedMeld::Chi { tiles, .. } => {
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
    /// Returns `true` if `flag` is present in `fan_mask`.
    #[must_use]
    pub const fn has_fan(self, flag: u16) -> bool {
        (self.fan_mask & flag) != 0
    }

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

/// Sums the actor's already-confirmed kong net points across `melds`.
#[must_use]
pub fn existing_kong_net_score(melds: &[MeldView]) -> i32 {
    let mut total = 0i32;
    for meld in melds {
        match meld {
            MeldView::ExposedKong { .. } => total += 2,
            MeldView::AddedKong { .. } => total += 3,
            MeldView::ConcealedKong { .. } => total += 6,
            MeldView::Pon { .. } | MeldView::Chi { .. } => {}
        }
    }
    total
}

/// Sums the actor's already-confirmed kong net points across settled [`RevealedMeld`] entries.
#[must_use]
pub fn existing_revealed_kong_net_score(melds: &[RevealedMeld]) -> i32 {
    let mut total = 0i32;
    for meld in melds {
        match meld {
            RevealedMeld::ExposedKong { .. } => total += 2,
            RevealedMeld::AddedKong { .. } => total += 3,
            RevealedMeld::ConcealedKong { .. } => total += 6,
            RevealedMeld::Pon { .. } | RevealedMeld::Chi { .. } => {}
        }
    }
    total
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

//! Fast zero-allocation shanten, visible-tile accounting, bitmask waits, and ukeire evaluation.

use mahjong_core::{MeldView, PlayerView, Tile};
use xiangting::{calculate_necessary_tiles, calculate_replacement_number, PlayerCount};

use crate::shared::{TERMINAL_OR_HONOR_INDICES, TILE_MASK_34};

/// Per-form shanten breakdown separated cleanly across standard (`4 melds + 1 pair`),
/// seven pairs (`七对` / `豪华七对`), and thirteen orphans (`十三幺`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShantenBreakdown {
    /// Standard `4 melds + 1 pair` shanten (`-1..=8`).
    pub standard: i8,
    /// Seven pairs shanten when `existing_melds == 0`, else `None`.
    pub seven_pairs: Option<i8>,
    /// Thirteen orphans shanten when `existing_melds == 0`, else `None`.
    pub thirteen_orphans: Option<i8>,
    /// Minimum shanten across all legal hand forms.
    pub min: i8,
}

const INF_SHANTEN: i8 = 100;

/// Converts `mahjong-core` tile frequencies (`m, s, p, z`) to Riichi crate order (`m, p, s, z`).
#[must_use]
pub fn core_counts_to_riichi(counts: &[u8; Tile::COUNT]) -> [u8; Tile::COUNT] {
    let mut out = [0u8; Tile::COUNT];
    out[0..9].copy_from_slice(&counts[0..9]);
    out[9..18].copy_from_slice(&counts[18..27]);
    out[18..27].copy_from_slice(&counts[9..18]);
    out[27..34].copy_from_slice(&counts[27..34]);
    out
}

/// Converts a 34-bit Riichi tile mask (`m, p, s, z`) back to `mahjong-core` bit order (`m, s, p, z`).
#[must_use]
pub const fn riichi_mask_to_core(mask: u64) -> u64 {
    const SUIT_MASK: u64 = 0x1ff;
    const HONOR_MASK: u64 = 0x7f << 27;
    let man = mask & SUIT_MASK;
    let pin_in_riichi = (mask >> 9) & SUIT_MASK;
    let sou_in_riichi = (mask >> 18) & SUIT_MASK;
    let honors = mask & HONOR_MASK;
    man | (sou_in_riichi << 9) | (pin_in_riichi << 18) | honors
}

/// Sums `remaining[idx]` over all tile indices `0..34` set in `mask`.
#[must_use]
pub fn ukeire_from_mask(mask: u64, remaining: &[u8; Tile::COUNT]) -> u16 {
    let mut bits = mask & TILE_MASK_34;
    let mut total = 0u16;
    while bits != 0 {
        let idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        total += u16::from(remaining[idx]);
    }
    total
}

/// Computes remaining unseen copies (`0..=4`) per tile kind from `view` alone.
///
/// Counts `view.hand.closed_tiles` + `drawn_tile`, unclaimed `river` entries
/// (`claimed_by.is_none()` to avoid double-counting), and public `melds`
/// (skipping opponent `ConcealedKong { tile: None }`).
#[must_use]
pub fn compute_remaining(view: &PlayerView) -> [u8; Tile::COUNT] {
    let mut visible = [0u8; Tile::COUNT];
    for &t in &view.hand.closed_tiles {
        visible[t.index()] = visible[t.index()].saturating_add(1);
    }
    if let Some(drawn) = view.hand.drawn_tile {
        visible[drawn.index()] = visible[drawn.index()].saturating_add(1);
    }
    for seat in &view.seats {
        for entry in &seat.river {
            if entry.claimed_by.is_none() {
                visible[entry.tile.index()] = visible[entry.tile.index()].saturating_add(1);
            }
        }
        for meld in &seat.melds {
            match *meld {
                MeldView::Pon { tile, .. } => {
                    visible[tile.index()] = visible[tile.index()].saturating_add(3);
                }
                MeldView::ExposedKong { tile, .. } | MeldView::AddedKong { tile, .. } => {
                    visible[tile.index()] = visible[tile.index()].saturating_add(4);
                }
                MeldView::ConcealedKong { tile: Some(t), .. } => {
                    visible[t.index()] = visible[t.index()].saturating_add(4);
                }
                MeldView::ConcealedKong { tile: None, .. } => {}
                MeldView::Chi { tiles, .. } => {
                    for t in tiles {
                        visible[t.index()] = visible[t.index()].saturating_add(1);
                    }
                }
            }
        }
    }

    let mut remaining = [0u8; Tile::COUNT];
    for idx in 0..Tile::COUNT {
        remaining[idx] = 4u8.saturating_sub(visible[idx]);
    }
    remaining
}

/// Counts the viewer's private hand (`closed_tiles` + `drawn_tile`) into `[u8; 34]`.
#[must_use]
pub fn hand_counts(view: &PlayerView) -> [u8; Tile::COUNT] {
    let mut counts = [0u8; Tile::COUNT];
    for &t in &view.hand.closed_tiles {
        counts[t.index()] += 1;
    }
    if let Some(drawn) = view.hand.drawn_tile {
        counts[drawn.index()] += 1;
    }
    counts
}

/// Zero-alloc `O(34)` seven-pairs shanten supporting `allow_four_of_kind_seven_pairs` (`豪华七对`).
#[must_use]
pub fn shanten_seven_pairs(counts: &[u8; Tile::COUNT], allow_four_of_kind_seven_pairs: bool) -> i8 {
    if allow_four_of_kind_seven_pairs {
        let pairs: i8 = counts.iter().map(|&c| (c / 2) as i8).sum();
        6 - pairs
    } else {
        let mut pairs = 0i8;
        let mut kinds = 0i8;
        for &c in counts {
            if c > 0 {
                kinds += 1;
                if c >= 2 {
                    pairs += 1;
                }
            }
        }
        6 - pairs + (7 - kinds).max(0)
    }
}

/// Zero-alloc `O(13)` thirteen-orphans (`十三幺`) shanten.
#[must_use]
pub fn shanten_thirteen_orphans(counts: &[u8; Tile::COUNT]) -> i8 {
    let mut unique = 0i8;
    let mut has_pair = 0i8;
    for &idx in &TERMINAL_OR_HONOR_INDICES {
        let c = counts[idx];
        if c > 0 {
            unique += 1;
            if c >= 2 {
                has_pair = 1;
            }
        }
    }
    13 - unique - has_pair
}

/// Returns the necessary-tile bitmask (`mahjong-core` `0..34`) for seven pairs.
#[must_use]
pub fn necessary_tiles_seven_pairs_mask(
    counts: &[u8; Tile::COUNT],
    allow_four_of_kind_seven_pairs: bool,
) -> u64 {
    if shanten_seven_pairs(counts, allow_four_of_kind_seven_pairs) == -1 {
        return 0;
    }
    let mut mask = 0u64;
    if allow_four_of_kind_seven_pairs {
        for (idx, &c) in counts.iter().enumerate() {
            if c == 1 || c == 3 {
                mask |= 1u64 << idx;
            }
        }
    } else {
        let kinds = counts.iter().filter(|&&c| c > 0).count();
        for (idx, &c) in counts.iter().enumerate() {
            if c == 1 || (kinds < 7 && c == 0) {
                mask |= 1u64 << idx;
            }
        }
    }
    mask
}

/// Returns the necessary-tile bitmask (`mahjong-core` `0..34`) for thirteen orphans.
#[must_use]
pub fn necessary_tiles_thirteen_orphans_mask(counts: &[u8; Tile::COUNT]) -> u64 {
    let mut unique = 0u8;
    let mut has_pair = false;
    for &idx in &TERMINAL_OR_HONOR_INDICES {
        let c = counts[idx];
        if c > 0 {
            unique += 1;
            if c >= 2 {
                has_pair = true;
            }
        }
    }
    if unique == 13 && has_pair {
        return 0;
    }
    let mut mask = 0u64;
    for &idx in &TERMINAL_OR_HONOR_INDICES {
        let c = counts[idx];
        if c == 0 || (!has_pair && c == 1) {
            mask |= 1u64 << idx;
        }
    }
    mask
}

/// Computes pure standard (`4 melds + 1 pair`) shanten via MIT `xiangting`, cleanly
/// separating standard form from Riichi 7-pairs / 13-orphans when `existing_melds == 0`.
#[must_use]
pub fn shanten_standard(counts: &[u8; Tile::COUNT], existing_melds: usize) -> i8 {
    let melds = existing_melds.min(4);
    let required_melds = 4 - melds;
    if xiangting_matches_meld_count(counts, required_melds) {
        let riichi = core_counts_to_riichi(counts);
        if let Ok(rn) = calculate_replacement_number(&riichi, PlayerCount::Four) {
            let s_x = (rn as i8) - 1;
            if melds > 0 {
                return s_x;
            }
            let s_r7p = shanten_seven_pairs(counts, false);
            let s_13o = shanten_thirteen_orphans(counts);
            if s_x < s_r7p && s_x < s_13o {
                return s_x;
            }
        }
    }
    standard_exact_dp(counts, required_melds).0
}

/// Computes pure standard (`4 melds + 1 pair`) necessary-tile bitmask (`mahjong-core` `0..34`).
#[must_use]
pub fn necessary_tiles_standard_mask(counts: &[u8; Tile::COUNT], existing_melds: usize) -> u64 {
    standard_shanten_and_waits(counts, existing_melds).1
}

fn standard_shanten_and_waits(counts: &[u8; Tile::COUNT], existing_melds: usize) -> (i8, u64) {
    let melds = existing_melds.min(4);
    let required_melds = 4 - melds;
    if xiangting_matches_meld_count(counts, required_melds) {
        let riichi = core_counts_to_riichi(counts);
        if let Ok((rn, r_mask)) = calculate_necessary_tiles(&riichi, PlayerCount::Four) {
            let s_x = (rn as i8) - 1;
            if melds > 0 {
                return (s_x, riichi_mask_to_core(r_mask));
            }
            let s_r7p = shanten_seven_pairs(counts, false);
            let s_13o = shanten_thirteen_orphans(counts);
            if s_x < s_r7p && s_x < s_13o {
                return (s_x, riichi_mask_to_core(r_mask));
            }
        }
    }
    standard_exact_dp(counts, required_melds)
}

fn xiangting_matches_meld_count(counts: &[u8; Tile::COUNT], required_melds: usize) -> bool {
    let mut sum = 0u8;
    for &c in counts {
        if c > 4 {
            return false;
        }
        sum = sum.saturating_add(c);
    }
    sum > 0 && sum <= 14 && !sum.is_multiple_of(3) && usize::from(sum / 3) == required_melds
}

/// Computes per-form shanten breakdown across standard, seven pairs, and thirteen orphans.
#[must_use]
pub fn compute_shanten_breakdown(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
) -> ShantenBreakdown {
    let standard = shanten_standard(counts, existing_melds);
    if existing_melds == 0 {
        let seven_pairs = shanten_seven_pairs(counts, allow_four_of_kind_seven_pairs);
        let thirteen_orphans = shanten_thirteen_orphans(counts);
        let min = standard.min(seven_pairs).min(thirteen_orphans);
        ShantenBreakdown {
            standard,
            seven_pairs: Some(seven_pairs),
            thirteen_orphans: Some(thirteen_orphans),
            min,
        }
    } else {
        ShantenBreakdown {
            standard,
            seven_pairs: None,
            thirteen_orphans: None,
            min: standard,
        }
    }
}

/// Computes the minimum shanten across all valid hand forms for the given rules.
#[must_use]
pub fn compute_shanten(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
) -> i8 {
    let melds = existing_melds.min(4);
    let required_melds = 4 - melds;
    if melds == 0 && xiangting_matches_meld_count(counts, required_melds) {
        let riichi = core_counts_to_riichi(counts);
        if let Ok(rn) = calculate_replacement_number(&riichi, PlayerCount::Four) {
            let riichi_min = (rn as i8) - 1;
            if allow_four_of_kind_seven_pairs {
                // min(std, 7p_true, 13o) == min(min(std, 7p_false, 13o), 7p_true) because 7p_true <= 7p_false.
                return riichi_min.min(shanten_seven_pairs(counts, true));
            }
            return riichi_min;
        }
    }
    compute_shanten_breakdown(counts, existing_melds, allow_four_of_kind_seven_pairs).min
}

/// Computes both the per-form [`ShantenBreakdown`] and the combined necessary-tile `u64` bitmask
/// (`mahjong-core` `0..34`) over all hand forms achieving `breakdown.min`.
#[must_use]
pub fn necessary_tiles_mask(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
) -> (ShantenBreakdown, u64) {
    let melds = existing_melds.min(4);
    let (standard, std_mask) = standard_shanten_and_waits(counts, melds);
    if melds == 0 {
        let seven_pairs = shanten_seven_pairs(counts, allow_four_of_kind_seven_pairs);
        let thirteen_orphans = shanten_thirteen_orphans(counts);
        let min = standard.min(seven_pairs).min(thirteen_orphans);
        let mut waits = 0u64;
        if standard == min {
            waits |= std_mask;
        }
        if seven_pairs == min {
            waits |= necessary_tiles_seven_pairs_mask(counts, allow_four_of_kind_seven_pairs);
        }
        if thirteen_orphans == min {
            waits |= necessary_tiles_thirteen_orphans_mask(counts);
        }
        let breakdown = ShantenBreakdown {
            standard,
            seven_pairs: Some(seven_pairs),
            thirteen_orphans: Some(thirteen_orphans),
            min,
        };
        (breakdown, waits)
    } else {
        let breakdown = ShantenBreakdown {
            standard,
            seven_pairs: None,
            thirteen_orphans: None,
            min: standard,
        };
        (breakdown, std_mask)
    }
}

/// Computes ukeire: total `remaining[t]` over tiles `t` that strictly reduce `compute_shanten`.
#[must_use]
pub fn compute_ukeire(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
    remaining: &[u8; Tile::COUNT],
) -> u16 {
    let (_, mask) = necessary_tiles_mask(counts, existing_melds, allow_four_of_kind_seven_pairs);
    ukeire_from_mask(mask, remaining)
}

/// Priority for breaking `(shanten, ukeire)` ties by shedding isolated honors/terminals
/// before middle tiles. Higher value means more eager to discard.
#[must_use]
pub fn discard_tie_priority(tile: Tile, pre_hand_counts: &[u8; Tile::COUNT]) -> u8 {
    let idx = tile.index();
    let single = pre_hand_counts[idx] == 1;
    let isolated = if tile.is_honor() {
        single
    } else {
        single && !has_suit_neighbor(pre_hand_counts, idx)
    };

    if isolated {
        if tile.is_honor() {
            50
        } else if tile.is_terminal() {
            40
        } else if matches!(tile.rank(), Some(2 | 8)) {
            30
        } else {
            20
        }
    } else if tile.is_terminal_or_honor() {
        10
    } else {
        0
    }
}

fn has_suit_neighbor(counts: &[u8; Tile::COUNT], idx: usize) -> bool {
    let suit_base = (idx / 9) * 9;
    let rank_idx = idx % 9;
    let low = rank_idx.saturating_sub(2);
    let high = (rank_idx + 2).min(8);
    (low..=high).any(|r| r != rank_idx && counts[suit_base + r] > 0)
}

#[derive(Clone, Copy)]
struct DpCell {
    shanten: i8,
    waits: u64,
}

impl DpCell {
    const INF: Self = Self {
        shanten: INF_SHANTEN,
        waits: 0,
    };

    #[inline]
    fn chmin(&mut self, shanten: i8, waits: u64) {
        if shanten < self.shanten {
            self.shanten = shanten;
            self.waits = waits;
        } else if shanten == self.shanten {
            self.waits |= waits;
        }
    }
}

#[derive(Clone, Copy)]
struct StepDelta {
    da: usize,
    db: usize,
    dc: usize,
    dp: usize,
    dm: usize,
}

const NO_SEQ_DELTAS: [StepDelta; 3] = [
    StepDelta {
        da: 0,
        db: 0,
        dc: 0,
        dp: 0,
        dm: 0,
    },
    StepDelta {
        da: 3,
        db: 0,
        dc: 0,
        dp: 0,
        dm: 1,
    },
    StepDelta {
        da: 2,
        db: 0,
        dc: 0,
        dp: 1,
        dm: 0,
    },
];

const WITH_SEQ_DELTAS: [StepDelta; 8] = [
    StepDelta {
        da: 0,
        db: 0,
        dc: 0,
        dp: 0,
        dm: 0,
    },
    StepDelta {
        da: 1,
        db: 1,
        dc: 1,
        dp: 0,
        dm: 1,
    },
    StepDelta {
        da: 2,
        db: 2,
        dc: 2,
        dp: 0,
        dm: 2,
    },
    StepDelta {
        da: 3,
        db: 0,
        dc: 0,
        dp: 0,
        dm: 1,
    },
    StepDelta {
        da: 4,
        db: 1,
        dc: 1,
        dp: 0,
        dm: 2,
    },
    StepDelta {
        da: 2,
        db: 0,
        dc: 0,
        dp: 1,
        dm: 0,
    },
    StepDelta {
        da: 3,
        db: 1,
        dc: 1,
        dp: 1,
        dm: 1,
    },
    StepDelta {
        da: 4,
        db: 2,
        dc: 2,
        dp: 1,
        dm: 2,
    },
];

/// Exact 4-copy-capped `required_melds + 1 pair` DP fallback used only when `existing_melds == 0`
/// and Riichi 7-pairs / 13-orphans match or beat standard shanten inside `xiangting` (or on
/// irregular closed-count test inputs).
fn standard_exact_dp(counts: &[u8; Tile::COUNT], required_melds: usize) -> (i8, u64) {
    let req_m = required_melds.min(4);
    let mut cur = [[[[DpCell::INF; 5]; 2]; 5]; 5];
    cur[0][0][0][0] = DpCell {
        shanten: -1,
        waits: 0,
    };

    for (n, &count) in counts.iter().enumerate() {
        let mut next = [[[[DpCell::INF; 5]; 2]; 5]; 5];
        let deltas: &[StepDelta] = if n < 27 && n % 9 <= 6 {
            &WITH_SEQ_DELTAS
        } else {
            &NO_SEQ_DELTAS
        };

        for delta in deltas {
            let max_a = 4usize.saturating_sub(delta.da);
            let max_b = 4usize.saturating_sub(delta.db);
            let max_p = 1usize.saturating_sub(delta.dp);
            let max_m = req_m.saturating_sub(delta.dm);

            for (a, row_a) in cur.iter().enumerate().take(max_a + 1) {
                let needed = a + delta.da;
                let dist = needed as i8 - count as i8;
                let add_dist = dist.max(0);
                let wait_bit = if dist > 0 { 1u64 << n } else { 0 };

                for (b, row_b) in row_a.iter().enumerate().take(max_b + 1) {
                    for (p, row_p) in row_b.iter().enumerate().take(max_p + 1) {
                        for (m, cell) in row_p.iter().enumerate().take(max_m + 1) {
                            if cell.shanten == INF_SHANTEN {
                                continue;
                            }
                            next[b + delta.db][delta.dc][p + delta.dp][m + delta.dm]
                                .chmin(cell.shanten + add_dist, cell.waits | wait_bit);
                        }
                    }
                }
            }
        }
        cur = next;
    }

    let final_cell = cur[0][0][1][req_m];
    (final_cell.shanten, final_cell.waits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::{counts_from_tiles, tile, tiles, tiles_from_counts};
    use mahjong_core::{
        RevealedMeld, Ruleset, Seat, Suit, Tile, TuidaohuRuleset, TurnSource, WinEvalInput, WinKind,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};
    use shanten_dp::{calc_shanten2, make_tile_limits, Mode};

    /// Independent Riichi index conversion using `Suit` + `rank` (not calling `mahjong_ai`'s helper).
    fn independent_riichi_index(t: Tile) -> usize {
        match t.suit() {
            Suit::Man => usize::from(t.rank().unwrap() - 1),
            Suit::Dot => 9 + usize::from(t.rank().unwrap() - 1),
            Suit::Bamboo => 18 + usize::from(t.rank().unwrap() - 1),
            Suit::Wind | Suit::Dragon => t.index(),
        }
    }

    fn independent_to_riichi_counts(counts: &[u8; Tile::COUNT]) -> [u8; Tile::COUNT] {
        let mut out = [0u8; Tile::COUNT];
        for (idx, &c) in counts.iter().enumerate() {
            let t = Tile::new(idx as u8).unwrap();
            out[independent_riichi_index(t)] = c;
        }
        out
    }

    fn independent_riichi_mask_to_core(riichi_mask: u64) -> u64 {
        let mut core_mask = 0u64;
        for t in Tile::all() {
            let r_idx = independent_riichi_index(t);
            if (riichi_mask & (1u64 << r_idx)) != 0 {
                core_mask |= 1u64 << t.index();
            }
        }
        core_mask
    }

    fn verify_against_shanten_dp(
        counts: &[u8; Tile::COUNT],
        existing_melds: usize,
        remaining: &[u8; Tile::COUNT],
        label: &str,
    ) {
        let riichi_counts = independent_to_riichi_counts(counts);
        let limits = make_tile_limits(false);
        let m = 4 - existing_melds;

        let dp_std = calc_shanten2(&riichi_counts, &limits, m, Mode::STANDARD, false, true)
            .expect("valid shanten-dp input")
            .expect("standard shanten exists");
        let expected_std_waits = independent_riichi_mask_to_core(dp_std.waits);

        for allow_quad in [false, true] {
            let breakdown: ShantenBreakdown =
                compute_shanten_breakdown(counts, existing_melds, allow_quad);
            assert_eq!(
                breakdown.standard, dp_std.shanten,
                "standard shanten mismatch ({label}, melds={existing_melds}, allow_quad={allow_quad}): counts={counts:?}"
            );
            assert_eq!(
                necessary_tiles_standard_mask(counts, existing_melds),
                expected_std_waits,
                "standard waits mismatch ({label}, melds={existing_melds}): counts={counts:?}"
            );

            let dp_all = calc_shanten2(&riichi_counts, &limits, m, Mode::all(), allow_quad, true)
                .expect("valid shanten-dp all")
                .expect("min shanten exists");

            if existing_melds == 0 {
                let dp_7p = calc_shanten2(
                    &riichi_counts,
                    &limits,
                    4,
                    Mode::SEVEN_PAIRS,
                    allow_quad,
                    true,
                )
                .unwrap()
                .unwrap();
                let dp_13o = calc_shanten2(
                    &riichi_counts,
                    &limits,
                    4,
                    Mode::THIRTEEN_ORPHANS,
                    false,
                    true,
                )
                .unwrap()
                .unwrap();

                assert_eq!(
                    breakdown.seven_pairs,
                    Some(dp_7p.shanten),
                    "seven_pairs mismatch ({label}, allow_quad={allow_quad}): counts={counts:?}"
                );
                assert_eq!(
                    shanten_seven_pairs(counts, allow_quad),
                    dp_7p.shanten,
                    "shanten_seven_pairs direct mismatch ({label})"
                );
                assert_eq!(
                    breakdown.thirteen_orphans,
                    Some(dp_13o.shanten),
                    "thirteen_orphans mismatch ({label}): counts={counts:?}"
                );
                assert_eq!(
                    shanten_thirteen_orphans(counts),
                    dp_13o.shanten,
                    "shanten_thirteen_orphans direct mismatch ({label})"
                );
            } else {
                assert_eq!(breakdown.seven_pairs, None);
                assert_eq!(breakdown.thirteen_orphans, None);
            }

            assert_eq!(breakdown.min, dp_all.shanten);
            assert_eq!(
                compute_shanten(counts, existing_melds, allow_quad),
                dp_all.shanten,
                "compute_shanten mismatch ({label}, melds={existing_melds}, allow_quad={allow_quad}): counts={counts:?}"
            );

            let expected_all_waits = independent_riichi_mask_to_core(dp_all.waits);
            let (bd2, actual_mask) = necessary_tiles_mask(counts, existing_melds, allow_quad);
            assert_eq!(bd2, breakdown);
            assert_eq!(
                actual_mask, expected_all_waits,
                "necessary_tiles_mask mismatch ({label}, melds={existing_melds}, allow_quad={allow_quad}): counts={counts:?}"
            );

            let expected_ukeire: u16 = (0..Tile::COUNT)
                .filter(|&idx| (expected_all_waits & (1u64 << idx)) != 0)
                .map(|idx| u16::from(remaining[idx]))
                .sum();
            assert_eq!(
                compute_ukeire(counts, existing_melds, allow_quad, remaining),
                expected_ukeire,
                "compute_ukeire mismatch ({label}, melds={existing_melds}, allow_quad={allow_quad})"
            );
        }
    }

    fn draw_hand_from_deck(rng: &mut ChaCha8Rng, tile_count: usize) -> [u8; Tile::COUNT] {
        let mut deck = Tile::standard_deck_136();
        for i in (1..deck.len()).rev() {
            let j = (rng.next_u32() as usize) % (i + 1);
            deck.swap(i, j);
        }
        counts_from_tiles(&deck[..tile_count])
    }

    fn draw_from_pool(
        rng: &mut ChaCha8Rng,
        pool: &[Tile],
        tile_count: usize,
        blocked: &[u8; Tile::COUNT],
    ) -> [u8; Tile::COUNT] {
        let mut counts = [0u8; Tile::COUNT];
        let mut total = 0usize;
        while total < tile_count {
            let t = pool[(rng.next_u32() as usize) % pool.len()];
            if counts[t.index()] + blocked[t.index()] < 4 {
                counts[t.index()] += 1;
                total += 1;
            }
        }
        counts
    }

    fn verify_win_equivalence(counts: &[u8; Tile::COUNT], melds: &[RevealedMeld], label: &str) {
        let ruleset = TuidaohuRuleset;
        let closed = tiles_from_counts(counts);
        let eval_win = ruleset
            .evaluate_win(&WinEvalInput {
                closed_tiles: &closed,
                extra_tile: None,
                melds,
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::NormalDraw),
                wall_remaining: 40,
            })
            .is_some();
        let sh = compute_shanten(counts, melds.len(), true);
        assert_eq!(
            sh == -1,
            eval_win,
            "shanten == -1 vs evaluate_win mismatch ({label}): shanten={sh}, eval_win={eval_win}, melds={melds:?}, counts={counts:?}"
        );
    }

    #[test]
    fn shanten_crosschecks_against_shanten_dp_and_tuidaohu_win_across_6000_hands() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x2026_0032);
        let mut total_checked = 0usize;

        // Verify bitmask conversion helper roundtrip & suit swap.
        let riichi_sample = (1u64 << 9) | (1u64 << 18) | (1u64 << 0) | (1u64 << 33);
        let core_expected = (1u64 << 18) | (1u64 << 9) | (1u64 << 0) | (1u64 << 33);
        assert_eq!(riichi_mask_to_core(riichi_sample), core_expected);

        let full_remaining = [4u8; Tile::COUNT];

        // 1. Random hands across 0..=4 existing_melds (both 13-3m and 14-3m closed tiles):
        //    5 * 2 * 350 = 3,500 hands.
        for existing_melds in 0..=4 {
            let len_after_discard = 13 - 3 * existing_melds;
            let len_before_discard = 14 - 3 * existing_melds;
            for sample in 0..350 {
                for len in [len_after_discard, len_before_discard] {
                    let counts = draw_hand_from_deck(&mut rng, len);
                    let mut remaining = [0u8; Tile::COUNT];
                    for idx in 0..Tile::COUNT {
                        remaining[idx] = 4 - counts[idx];
                    }
                    verify_against_shanten_dp(
                        &counts,
                        existing_melds,
                        &remaining,
                        &format!("random_m{existing_melds}_len{len}_{sample}"),
                    );
                    total_checked += 1;

                    if len == len_before_discard {
                        let mut melds = Vec::with_capacity(existing_melds);
                        let mut used = counts;
                        let mut cand = 0usize;
                        while melds.len() < existing_melds {
                            while used[cand] > 1 {
                                cand += 1;
                            }
                            used[cand] += 3;
                            melds.push(RevealedMeld::Pon {
                                tile: Tile::new(cand as u8).unwrap(),
                                from_seat: Seat::South,
                                seq: (melds.len() + 1) as u64,
                            });
                            cand += 1;
                        }
                        verify_win_equivalence(&counts, &melds, "random_open_or_closed");
                    }
                }
            }
        }

        // 2. Full-flush (清一色) and Half-flush (混一色) hands across 0..=4 melds:
        //    5 * 2 * 120 = 1,200 hands.
        let man_pool: Vec<Tile> = (0..9).map(|i| Tile::new(i).unwrap()).collect();
        let bamboo_honor_pool: Vec<Tile> = (9..18)
            .chain(27..34)
            .map(|i| Tile::new(i).unwrap())
            .collect();
        let dot_honor_pool: Vec<Tile> = (18..34).map(|i| Tile::new(i).unwrap()).collect();
        let zero_blocked = [0u8; Tile::COUNT];

        for existing_melds in 0..=4 {
            for sample in 0..120 {
                let pool = match sample % 3 {
                    0 => &man_pool,
                    1 => &bamboo_honor_pool,
                    _ => &dot_honor_pool,
                };
                for len in [13 - 3 * existing_melds, 14 - 3 * existing_melds] {
                    let counts = draw_from_pool(&mut rng, pool, len, &zero_blocked);
                    verify_against_shanten_dp(
                        &counts,
                        existing_melds,
                        &full_remaining,
                        &format!("flush_m{existing_melds}_len{len}_{sample}"),
                    );
                    total_checked += 1;

                    if len == 14 - 3 * existing_melds {
                        let mut melds = Vec::with_capacity(existing_melds);
                        let mut used = counts;
                        let mut cand = 0usize;
                        while melds.len() < existing_melds {
                            while used[cand] > 1 {
                                cand += 1;
                            }
                            used[cand] += 3;
                            melds.push(RevealedMeld::Pon {
                                tile: Tile::new(cand as u8).unwrap(),
                                from_seat: Seat::West,
                                seq: (melds.len() + 1) as u64,
                            });
                            cand += 1;
                        }
                        verify_win_equivalence(&counts, &melds, "flush");
                    }
                }
            }
        }

        // 3. Seven pairs & luxury seven pairs (1..=3 quads) with 13 & 14 tiles: 500 hands.
        for sample in 0..500 {
            let mut counts = [0u8; Tile::COUNT];
            let num_quads = (sample % 3) + 1;
            let mut tiles_placed = 0usize;
            for _ in 0..num_quads {
                let idx = (rng.next_u32() as usize) % Tile::COUNT;
                if counts[idx] == 0 {
                    counts[idx] = 4;
                    tiles_placed += 4;
                }
            }
            let target_len = if sample % 2 == 0 { 13 } else { 14 };
            while tiles_placed < target_len {
                let idx = (rng.next_u32() as usize) % Tile::COUNT;
                let add = if target_len - tiles_placed >= 2
                    && counts[idx] <= 2
                    && (rng.next_u32() % 4 != 0)
                {
                    2
                } else {
                    1
                };
                if counts[idx] + add <= 4 {
                    counts[idx] += add;
                    tiles_placed += usize::from(add);
                }
            }
            verify_against_shanten_dp(&counts, 0, &full_remaining, &format!("luxury_7p_{sample}"));
            total_checked += 1;
            if target_len == 14 {
                verify_win_equivalence(&counts, &[], "luxury_7p");
            }
        }

        // 4. Thirteen orphans bias hands (terminal/honor heavy): 300 hands.
        let yaojiu_pool: Vec<Tile> = Tile::all().filter(|t| t.is_terminal_or_honor()).collect();
        for sample in 0..300 {
            let len = if sample % 2 == 0 { 13 } else { 14 };
            let counts = draw_from_pool(&mut rng, &yaojiu_pool, len, &zero_blocked);
            verify_against_shanten_dp(&counts, 0, &full_remaining, &format!("13o_{sample}"));
            total_checked += 1;
            if len == 14 {
                verify_win_equivalence(&counts, &[], "13o");
            }
        }

        // 5. Explicit boundary hands.
        let boundary_cases: &[(&str, usize, i8, Option<bool>)] = &[
            ("123m 456m 789m 123p E", 0, 0, None),
            ("123m 456m 789m 123p E S", 0, 0, Some(false)),
            ("1111m 123p 112233s", 0, 1, None),
            ("1111m 222m 333p 444s", 0, 0, None),
            ("1111m 222p 333s 444z", 0, 1, None),
            ("1111234444m 1111p", 0, 1, Some(false)),
            ("11112222333444z", 0, 1, Some(false)),
            ("1111m 2222p 3333s 44z", 0, 1, Some(true)),
            ("1111m 22p 55p 88p 33s 6s C", 0, 2, Some(false)),
            ("11m 99m 11p 99p 11s 99s E", 0, 3, None),
            ("19m 19p 19s E S W N C F B", 0, 8, None),
            ("119m 19p 19s E S W N C F B", 0, 7, Some(true)),
            ("123m 456s 789p 11z", 1, -1, Some(true)),
            ("123m 456s 78p", 2, 0, None),
            ("123m 456s 77p", 2, -1, Some(true)),
            ("123m 4p", 3, 0, None),
            ("123m 44p", 3, -1, Some(true)),
            ("E", 4, 0, None),
            ("E S", 4, 0, Some(false)),
            ("E E", 4, -1, Some(true)),
        ];

        for (idx, &(spec, melds_n, expected_std, expected_win)) in boundary_cases.iter().enumerate()
        {
            let hand_tiles = tiles(spec);
            let counts = counts_from_tiles(&hand_tiles);
            verify_against_shanten_dp(
                &counts,
                melds_n,
                &full_remaining,
                &format!("boundary_{idx}_{spec}"),
            );
            let bd = compute_shanten_breakdown(&counts, melds_n, true);
            assert_eq!(
                bd.standard, expected_std,
                "boundary standard shanten mismatch for {spec:?}"
            );
            if let Some(should_win) = expected_win {
                let mut melds = Vec::new();
                let mut m_idx = 18u8;
                for k in 0..melds_n {
                    while counts[usize::from(m_idx)] > 1 {
                        m_idx = (m_idx + 1) % 34;
                    }
                    melds.push(RevealedMeld::Pon {
                        tile: Tile::new(m_idx).unwrap(),
                        from_seat: Seat::South,
                        seq: (k + 1) as u64,
                    });
                    m_idx = (m_idx + 1) % 34;
                }
                verify_win_equivalence(&counts, &melds, spec);
                assert_eq!(compute_shanten(&counts, melds_n, true) == -1, should_win);
            }
            total_checked += 1;
        }

        let no_pair_waits =
            necessary_tiles_standard_mask(&counts_from_tiles(&tiles("123m 456m 789m 123p E")), 0);
        assert_eq!(no_pair_waits, 1u64 << tile("E").index());

        assert!(
            total_checked >= 5_000,
            "expected >= 5,000 checked hands, got {total_checked}"
        );
    }
}

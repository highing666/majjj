//! Fast zero-allocation shanten, visible-tile accounting, bitmask waits, and ukeire evaluation.

use mahjong_core::{MeldView, PlayerView, Tile};
use xiangting::{calculate_necessary_tiles, calculate_replacement_number, PlayerCount};

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
const TERMINAL_OR_HONOR_INDICES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

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

/// Converts a 34-bit `mahjong-core` tile mask (`m, s, p, z`) to Riichi bit order (`m, p, s, z`).
#[must_use]
pub const fn core_mask_to_riichi(mask: u64) -> u64 {
    riichi_mask_to_core(mask)
}

/// Returns the single-bit `u64` mask corresponding to `tile`.
#[must_use]
pub const fn tile_bit(tile: Tile) -> u64 {
    1u64 << tile.index()
}

/// Returns `true` if `mask` has the bit set for `tile`.
#[must_use]
pub const fn mask_contains(mask: u64, tile: Tile) -> bool {
    (mask & tile_bit(tile)) != 0
}

/// Sums `remaining[idx]` over all tile indices `0..34` set in `mask`.
#[must_use]
pub fn ukeire_from_mask(mask: u64, remaining: &[u8; Tile::COUNT]) -> u16 {
    let mut bits = mask & ((1u64 << Tile::COUNT) - 1);
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

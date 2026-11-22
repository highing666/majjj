//! Fast zero-allocation shanten, visible-tile accounting, and ukeire evaluation.

use mahjong_core::{MeldView, PlayerView, Tile};

/// Computes remaining unseen copies (`0..=4`) per tile kind from `view` alone.
///
/// Counts `view.hand.closed_tiles` + `drawn_tile`, unclaimed `river` entries
/// (`claimed_by.is_none()` to avoid double-counting), and public `melds`
/// (skipping opponent `ConcealedKong { tile: None }`).
pub(crate) fn compute_remaining(view: &PlayerView) -> [u8; Tile::COUNT] {
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
pub(crate) fn hand_counts(view: &PlayerView) -> [u8; Tile::COUNT] {
    let mut counts = [0u8; Tile::COUNT];
    for &t in &view.hand.closed_tiles {
        counts[t.index()] += 1;
    }
    if let Some(drawn) = view.hand.drawn_tile {
        counts[drawn.index()] += 1;
    }
    counts
}

/// Computes the minimum shanten across all valid hand forms for the given rules.
pub(crate) fn compute_shanten(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
) -> i8 {
    let mut best = shanten_standard(counts, existing_melds);
    if existing_melds == 0 {
        best = best.min(shanten_seven_pairs(counts, allow_four_of_kind_seven_pairs));
        best = best.min(shanten_thirteen_orphans(counts));
    }
    best
}

/// Computes ukeire: total `remaining[t]` over tiles `t` (`counts[t] < 4`) that strictly
/// reduce `compute_shanten`.
pub(crate) fn compute_ukeire(
    counts: &mut [u8; Tile::COUNT],
    existing_melds: usize,
    allow_four_of_kind_seven_pairs: bool,
    remaining: &[u8; Tile::COUNT],
) -> u16 {
    let base_shanten = compute_shanten(counts, existing_melds, allow_four_of_kind_seven_pairs);
    let seven_pairs_active = existing_melds == 0
        && shanten_seven_pairs(counts, allow_four_of_kind_seven_pairs) == base_shanten;
    let orphans_active = existing_melds == 0 && shanten_thirteen_orphans(counts) == base_shanten;
    let distinct_kinds = if seven_pairs_active && !allow_four_of_kind_seven_pairs {
        counts.iter().filter(|&&c| c > 0).count()
    } else {
        0
    };

    let mut total = 0u16;
    for idx in 0..Tile::COUNT {
        let rem = remaining[idx];
        if rem == 0 || counts[idx] >= 4 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        let can_improve_seven_pairs = seven_pairs_active
            && (counts[idx] == 1
                || (allow_four_of_kind_seven_pairs && counts[idx] == 3)
                || (!allow_four_of_kind_seven_pairs && counts[idx] == 0 && distinct_kinds < 7));
        let can_improve_orphans = orphans_active && tile.is_terminal_or_honor();
        if !can_improve_seven_pairs && !can_improve_orphans && !has_local_connection(counts, idx) {
            continue;
        }

        counts[idx] += 1;
        let next_shanten = compute_shanten(counts, existing_melds, allow_four_of_kind_seven_pairs);
        counts[idx] -= 1;
        if next_shanten < base_shanten {
            total += u16::from(rem);
        }
    }
    total
}

/// Priority for breaking `(shanten, ukeire)` ties by shedding isolated honors/terminals
/// before middle tiles. Higher value means more eager to discard.
pub(crate) fn discard_tie_priority(tile: Tile, pre_hand_counts: &[u8; Tile::COUNT]) -> u8 {
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

fn has_local_connection(counts: &[u8; Tile::COUNT], idx: usize) -> bool {
    counts[idx] > 0 || (idx < 27 && has_suit_neighbor(counts, idx))
}

fn has_suit_neighbor(counts: &[u8; Tile::COUNT], idx: usize) -> bool {
    let suit_base = (idx / 9) * 9;
    let rank_idx = idx % 9;
    let low = rank_idx.saturating_sub(2);
    let high = (rank_idx + 2).min(8);
    (low..=high).any(|r| r != rank_idx && counts[suit_base + r] > 0)
}

fn shanten_seven_pairs(counts: &[u8; Tile::COUNT], allow_four_of_kind_seven_pairs: bool) -> i8 {
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

fn shanten_thirteen_orphans(counts: &[u8; Tile::COUNT]) -> i8 {
    let mut unique = 0i8;
    let mut has_pair = 0i8;
    for (idx, &c) in counts.iter().enumerate() {
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        if tile.is_terminal_or_honor() && c > 0 {
            unique += 1;
            if c >= 2 {
                has_pair = 1;
            }
        }
    }
    13 - unique - has_pair
}

fn shanten_standard(counts: &[u8; Tile::COUNT], existing_melds: usize) -> i8 {
    let mut work = *counts;
    let existing = existing_melds.min(4) as i8;
    let mut best = eval_standard_body(&mut work, existing, 0);

    for pair_idx in 0..Tile::COUNT {
        if work[pair_idx] >= 2 {
            work[pair_idx] -= 2;
            let s = eval_standard_body(&mut work, existing, 1);
            work[pair_idx] += 2;
            if s < best {
                best = s;
            }
        }
    }
    best
}

fn eval_standard_body(counts: &mut [u8; Tile::COUNT], existing: i8, has_pair: i8) -> i8 {
    let mut man = [0u8; 9];
    man.copy_from_slice(&counts[0..9]);
    let mut bamboo = [0u8; 9];
    bamboo.copy_from_slice(&counts[9..18]);
    let mut dot = [0u8; 9];
    dot.copy_from_slice(&counts[18..27]);

    let man_table = suit_meld_proto(&mut man);
    let bamboo_table = suit_meld_proto(&mut bamboo);
    let dot_table = suit_meld_proto(&mut dot);

    let mut honor_m = 0i8;
    let mut honor_p = 0i8;
    for &c in &counts[27..Tile::COUNT] {
        if c >= 3 {
            honor_m += 1;
        } else if c == 2 {
            honor_p += 1;
        }
    }

    let max_blocks = 4 - existing;
    let mut best_score = 2 * existing + has_pair;
    for (m1, &p1) in man_table.iter().enumerate() {
        if p1 < 0 {
            continue;
        }
        let m1 = m1 as i8;
        if m1 > max_blocks {
            break;
        }
        for (m2, &p2) in bamboo_table.iter().enumerate() {
            if p2 < 0 {
                continue;
            }
            let m2 = m2 as i8;
            if m1 + m2 > max_blocks {
                break;
            }
            for (m3, &p3) in dot_table.iter().enumerate() {
                if p3 < 0 {
                    continue;
                }
                let m3 = m3 as i8;
                let closed_m = m1 + m2 + m3 + honor_m;
                if closed_m > max_blocks {
                    break;
                }
                let total_proto = p1 + p2 + p3 + honor_p;
                let used_proto = total_proto.min(max_blocks - closed_m);
                let score = 2 * (existing + closed_m) + used_proto + has_pair;
                if score > best_score {
                    best_score = score;
                }
            }
        }
    }

    8 - best_score
}

fn suit_meld_proto(counts: &mut [u8; 9]) -> [i8; 5] {
    let mut out = [-1i8; 5];
    dfs_melds(counts, 0, 0, true, &mut out);
    out
}

fn dfs_melds(
    counts: &mut [u8; 9],
    mut idx: usize,
    melds: usize,
    mut allow_triplet: bool,
    out: &mut [i8; 5],
) {
    let start_idx = idx;
    while idx < 9 && counts[idx] == 0 {
        idx += 1;
    }
    if idx != start_idx {
        allow_triplet = true;
    }
    if idx == 9 {
        if melds < out.len() {
            let p = max_proto_melds(counts, 0);
            if p > out[melds] {
                out[melds] = p;
            }
        }
        return;
    }

    if melds < 4 {
        if allow_triplet && counts[idx] >= 3 {
            counts[idx] -= 3;
            dfs_melds(counts, idx, melds + 1, false, out);
            counts[idx] += 3;
        }
        if idx <= 6 && counts[idx + 1] > 0 && counts[idx + 2] > 0 {
            counts[idx] -= 1;
            counts[idx + 1] -= 1;
            counts[idx + 2] -= 1;
            dfs_melds(counts, idx, melds + 1, false, out);
            counts[idx] += 1;
            counts[idx + 1] += 1;
            counts[idx + 2] += 1;
        }
    }

    dfs_melds(counts, idx + 1, melds, true, out);
}

fn max_proto_melds(counts: &mut [u8; 9], mut idx: usize) -> i8 {
    while idx < 9 && counts[idx] == 0 {
        idx += 1;
    }
    if idx == 9 {
        return 0;
    }

    let c = counts[idx];
    let mut best = 0i8;
    if c >= 2 {
        counts[idx] -= 2;
        best = best.max(1 + max_proto_melds(counts, idx));
        counts[idx] += 2;
    }
    if idx + 1 < 9 && counts[idx + 1] > 0 {
        counts[idx] -= 1;
        counts[idx + 1] -= 1;
        best = best.max(1 + max_proto_melds(counts, idx));
        counts[idx] += 1;
        counts[idx + 1] += 1;
    }
    if idx + 2 < 9 && counts[idx + 2] > 0 {
        counts[idx] -= 1;
        counts[idx + 2] -= 1;
        best = best.max(1 + max_proto_melds(counts, idx));
        counts[idx] += 1;
        counts[idx + 2] += 1;
    }
    if c == 1 {
        counts[idx] -= 1;
        best = best.max(max_proto_melds(counts, idx + 1));
        counts[idx] += 1;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use mahjong_core::{
        RevealedMeld, Ruleset, Seat, TuidaohuRuleset, TurnSource, WinEvalInput, WinKind,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::{RngCore, SeedableRng};

    fn brute_force_standard_shanten(counts: &[u8; Tile::COUNT], existing_melds: usize) -> i8 {
        fn search_blocks(
            counts: &mut [u8; Tile::COUNT],
            idx: usize,
            melds: i8,
            proto_melds: i8,
            existing: i8,
            has_pair: i8,
            best_score: &mut i8,
        ) {
            let max_blocks = 4 - existing;
            if melds + proto_melds > max_blocks {
                return;
            }
            let score = 2 * (existing + melds) + proto_melds + has_pair;
            if score > *best_score {
                *best_score = score;
            }
            let mut next = idx;
            while next < Tile::COUNT && counts[next] == 0 {
                next += 1;
            }
            if next == Tile::COUNT {
                return;
            }

            // Try meld: triplet
            if melds + proto_melds < max_blocks && counts[next] >= 3 {
                counts[next] -= 3;
                search_blocks(
                    counts,
                    next,
                    melds + 1,
                    proto_melds,
                    existing,
                    has_pair,
                    best_score,
                );
                counts[next] += 3;
            }
            // Try meld: sequence
            if melds + proto_melds < max_blocks
                && next < 27
                && next % 9 <= 6
                && counts[next + 1] > 0
                && counts[next + 2] > 0
            {
                counts[next] -= 1;
                counts[next + 1] -= 1;
                counts[next + 2] -= 1;
                search_blocks(
                    counts,
                    next,
                    melds + 1,
                    proto_melds,
                    existing,
                    has_pair,
                    best_score,
                );
                counts[next] += 1;
                counts[next + 1] += 1;
                counts[next + 2] += 1;
            }
            // Try proto-meld: pair
            if melds + proto_melds < max_blocks && counts[next] >= 2 {
                counts[next] -= 2;
                search_blocks(
                    counts,
                    next,
                    melds,
                    proto_melds + 1,
                    existing,
                    has_pair,
                    best_score,
                );
                counts[next] += 2;
            }
            // Try proto-meld: adjacent
            if melds + proto_melds < max_blocks
                && next < 27
                && next % 9 <= 7
                && counts[next + 1] > 0
            {
                counts[next] -= 1;
                counts[next + 1] -= 1;
                search_blocks(
                    counts,
                    next,
                    melds,
                    proto_melds + 1,
                    existing,
                    has_pair,
                    best_score,
                );
                counts[next] += 1;
                counts[next + 1] += 1;
            }
            // Try proto-meld: gap
            if melds + proto_melds < max_blocks
                && next < 27
                && next % 9 <= 6
                && counts[next + 2] > 0
            {
                counts[next] -= 1;
                counts[next + 2] -= 1;
                search_blocks(
                    counts,
                    next,
                    melds,
                    proto_melds + 1,
                    existing,
                    has_pair,
                    best_score,
                );
                counts[next] += 1;
                counts[next + 2] += 1;
            }
            // Skip one copy of `next`
            counts[next] -= 1;
            search_blocks(
                counts,
                next,
                melds,
                proto_melds,
                existing,
                has_pair,
                best_score,
            );
            counts[next] += 1;
        }

        let existing = existing_melds.min(4) as i8;
        let mut work = *counts;
        let mut best_score = 2 * existing;
        search_blocks(&mut work, 0, 0, 0, existing, 0, &mut best_score);
        for p in 0..Tile::COUNT {
            if work[p] >= 2 {
                work[p] -= 2;
                search_blocks(&mut work, 0, 0, 0, existing, 1, &mut best_score);
                work[p] += 2;
            }
        }
        8 - best_score
    }

    fn random_hand(rng: &mut ChaCha8Rng, tile_count: usize) -> [u8; Tile::COUNT] {
        let mut deck = Tile::standard_deck_136();
        for i in (1..deck.len()).rev() {
            let j = (rng.next_u32() as usize) % (i + 1);
            deck.swap(i, j);
        }
        let mut counts = [0u8; Tile::COUNT];
        for &t in deck.iter().take(tile_count) {
            counts[t.index()] += 1;
        }
        counts
    }

    #[test]
    fn fast_shanten_matches_brute_force_and_win_evaluation() {
        let mut rng = ChaCha8Rng::seed_from_u64(0x5EED_1234);

        // Cross-check fast per-suit standard shanten vs unsplit brute-force across 0..=4 melds.
        for existing_melds in 0..=4 {
            let after_discard_len = 13 - 3 * existing_melds;
            let before_discard_len = 14 - 3 * existing_melds;
            for _ in 0..80 {
                for len in [after_discard_len, before_discard_len] {
                    let counts = random_hand(&mut rng, len);
                    assert_eq!(
                        shanten_standard(&counts, existing_melds),
                        brute_force_standard_shanten(&counts, existing_melds),
                        "mismatch for len={len}, existing_melds={existing_melds}, counts={counts:?}"
                    );
                }
            }
        }

        // Cross-check shanten == -1 <=> TuidaohuRuleset::evaluate_win().is_some()
        // across random and structured near-winning hands.
        let ruleset = TuidaohuRuleset;
        for sample in 0..1000 {
            let counts = if sample % 4 == 0 {
                // Bias toward narrow suit pools so complete hands occur frequently.
                let mut counts = [0u8; Tile::COUNT];
                let mut total = 0usize;
                while total < 14 {
                    let idx = (rng.next_u32() as usize) % 9;
                    if counts[idx] < 4 {
                        counts[idx] += 1;
                        total += 1;
                    }
                }
                counts
            } else {
                random_hand(&mut rng, 14)
            };

            let mut closed = Vec::with_capacity(14);
            for (idx, &c) in counts.iter().enumerate() {
                let tile = Tile::new(idx as u8).expect("valid index");
                for _ in 0..c {
                    closed.push(tile);
                }
            }
            let melds: Vec<RevealedMeld> = Vec::new();
            let ruleset_win = ruleset
                .evaluate_win(&WinEvalInput {
                    closed_tiles: &closed,
                    extra_tile: None,
                    melds: &melds,
                    win_kind: WinKind::Tsumo,
                    turn_source: Some(TurnSource::NormalDraw),
                    wall_remaining: 40,
                })
                .is_some();
            let sh = compute_shanten(&counts, 0, true);
            assert_eq!(
                sh == -1,
                ruleset_win,
                "shanten={sh} vs evaluate_win={ruleset_win} for counts={counts:?}"
            );
        }

        // Verify with existing melds (e.g. 2 Pons + complete closed tiles).
        let mut counts_with_melds = [0u8; Tile::COUNT];
        counts_with_melds[0] = 1;
        counts_with_melds[1] = 1;
        counts_with_melds[2] = 1;
        counts_with_melds[9] = 3;
        counts_with_melds[30] = 2; // 8 closed tiles + 2 melds
        let melds = vec![
            RevealedMeld::Pon {
                tile: Tile::new(20).unwrap(),
                from_seat: Seat::South,
                seq: 1,
            },
            RevealedMeld::Pon {
                tile: Tile::new(21).unwrap(),
                from_seat: Seat::West,
                seq: 2,
            },
        ];
        let mut closed = Vec::new();
        for (idx, &c) in counts_with_melds.iter().enumerate() {
            let t = Tile::new(idx as u8).unwrap();
            for _ in 0..c {
                closed.push(t);
            }
        }
        assert!(ruleset
            .evaluate_win(&WinEvalInput {
                closed_tiles: &closed,
                extra_tile: None,
                melds: &melds,
                win_kind: WinKind::Tsumo,
                turn_source: Some(TurnSource::NormalDraw),
                wall_remaining: 40,
            })
            .is_some());
        assert_eq!(compute_shanten(&counts_with_melds, melds.len(), true), -1);
    }
}

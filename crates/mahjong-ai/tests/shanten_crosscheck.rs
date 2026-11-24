mod common;

use common::{counts_from_tiles, tile, tiles, tiles_from_counts};
use mahjong_ai::shanten::{
    compute_shanten, compute_shanten_breakdown, compute_ukeire, necessary_tiles_mask,
    necessary_tiles_standard_mask, riichi_mask_to_core, shanten_seven_pairs,
    shanten_thirteen_orphans, ShantenBreakdown,
};
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
                    // Build `existing_melds` disjoint Pons/Sequences without overlapping closed counts.
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
    //    5 * 2 * 120 = 1,200 hands (high frequency of complete / near-complete hands).
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

    // 5. Explicit boundary hands:
    //    - 4-melds-no-pair (13 & 14 tiles, plus 5th-tile wait edge cases!)
    //    - Luxury seven pairs win & tenpai (with/without allow_four_of_kind_seven_pairs)
    //    - Thirteen orphans 13-wait, 1-wait, win
    //    - Melded 1-tile and 2-tile hands (4 melds)
    let boundary_cases: &[(&str, usize, i8, Option<bool>)] = &[
        // (hand, existing_melds, expected_standard_shanten, is_win_under_tuidaohu)
        ("123m 456m 789m 123p E", 0, 0, None),
        ("123m 456m 789m 123p E S", 0, 0, Some(false)), // 4 melds, NO pair -> NOT a win!
        ("1111m 123p 112233s", 0, 1, None),             // 5th-tile wait trap!
        ("1111m 222m 333p 444s", 0, 0, None), // 111m + 123m (needs 3m) + 22m + 333p + 444s!
        ("1111m 222p 333s 444z", 0, 1, None), // Isolated quad + 3 melds: cannot wait on 5th 1m!
        ("1111234444m 1111p", 0, 1, Some(false)),
        ("11112222333444z", 0, 1, Some(false)),
        ("1111m 2222p 3333s 44z", 0, 1, Some(true)), // Luxury 7-pairs win (standard=1 via 123m)!
        ("1111m 22p 55p 88p 33s 6s C", 0, 2, Some(false)),
        ("11m 99m 11p 99p 11s 99s E", 0, 3, None), // 7-pairs tenpai, standard=3!
        ("19m 19p 19s E S W N C F B", 0, 8, None), // 13-orphans 13-wait tenpai!
        ("119m 19p 19s E S W N C F B", 0, 7, Some(true)), // 13-orphans win!
        ("123m 456s 789p 11z", 1, -1, Some(true)),
        ("123m 456s 78p", 2, 0, None),
        ("123m 456s 77p", 2, -1, Some(true)),
        ("123m 4p", 3, 0, None),
        ("123m 44p", 3, -1, Some(true)),
        ("E", 4, 0, None),
        ("E S", 4, 0, Some(false)),
        ("E E", 4, -1, Some(true)),
    ];

    for (idx, &(spec, melds_n, expected_std, expected_win)) in boundary_cases.iter().enumerate() {
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
                // Pick honors/tiles not conflicting with closed hand for win eval.
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

    // Also verify 4-melds-no-pair necessary tile is specifically the singleton, not already-4x tiles.
    let no_pair_waits =
        necessary_tiles_standard_mask(&counts_from_tiles(&tiles("123m 456m 789m 123p E")), 0);
    assert_eq!(no_pair_waits, 1u64 << tile("E").index());

    assert!(
        total_checked >= 5_000,
        "expected >= 5,000 checked hands, got {total_checked}"
    );
}

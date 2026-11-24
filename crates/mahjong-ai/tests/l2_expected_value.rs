mod common;

use common::{counts_from_tiles, tiles};
use mahjong_ai::scoring::{
    existing_kong_net_score, existing_revealed_kong_net_score, kong_net_delta, MeldSummary,
    ScoringInput, ScoringRules, ALL_HONORS, ALL_PUNGS, BASIC_HU, FULL_FLUSH, HALF_FLUSH,
    LAST_TILE_RON, LAST_TILE_TSUMO, LUXURY_SEVEN_PAIRS, MENQING, ROB_KONG_HU, SELF_DRAW_AFTER_KONG,
    SEVEN_PAIRS, THIRTEEN_ORPHANS,
};
use mahjong_ai::{
    decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext, TileEfficiencyBot,
};
use mahjong_core::{
    build_preset_wall, project_view, KongKind, Match, MatchMode, MeldView, PlayerAction,
    PresetWallProvider, PublicPhase, RevealedMeld, Ruleset, Seat, Tile, TuidaohuRuleset,
    TurnSource, WinEvalInput, WinKind,
};
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
    let actual = mahjong_ai::scoring::evaluate_win(
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

    // 1. Explicit curated hands covering all 13 ScoreItem codes, exclusions, caps, and edge cases.
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
        let eval = mahjong_ai::scoring::evaluate_win(
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

    // 2. Toggle checks: `allow_four_of_kind_seven_pairs = false` and custom `fan_cap`.
    let quad_7p_counts = counts_from_tiles(&tiles("1111m 22p 33p 55s 77s EE"));
    let melds_empty = MeldSummary::empty();
    let input_ron = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: 20,
    };
    let with_quad = mahjong_ai::scoring::evaluate_win(
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

    let without_quad = mahjong_ai::scoring::evaluate_win(
        &quad_7p_counts,
        &melds_empty,
        input_ron,
        ScoringRules {
            allow_four_of_kind_seven_pairs: false,
            fan_cap: 4,
        },
    );
    assert!(without_quad.is_none(), "quad 7p rejected when rule off");

    let uncapped_full = mahjong_ai::scoring::evaluate_win(
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

    // 3. Kong net delta checks matching TuidaohuRuleset::kong_score_deltas.
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

    // 4. Randomized >= 1,200 winning, non-winning, melded, flush, 7p, and 13o parity hands.
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
            // Thirteen orphans & near-13o hands
            let yaojiu = tiles("19m 19s 19p E S W N C F B");
            closed.extend_from_slice(&yaojiu);
            let extra = if sample % 50 == 0 {
                yaojiu[gen_idx(&mut rng, yaojiu.len())]
            } else {
                tile_one("5m")
            };
            closed.push(extra);
        } else if meld_count == 0 && sample % 7 == 0 {
            // Seven pairs / luxury seven pairs (or 1-off)
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
            // Structured standard winning or near-winning hand (possibly single suit / flush)
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
                // Fallback random hand of right length
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
                // Perturb one tile to cover non-winning parity cases too
                let pop_pos = gen_idx(&mut rng, closed.len());
                closed.remove(pop_pos);
                let idx = gen_idx(&mut rng, Tile::COUNT);
                closed.push(Tile::new(idx as u8).unwrap());
            }
        }

        let melds = make_non_overlapping_melds(&mut rng, &closed, meld_count).unwrap_or_default();
        let adj_closed = if melds.len() == meld_count {
            closed
        } else {
            // Retry with 0 melds and 14 tiles
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

fn tile_one(tok: &str) -> Tile {
    common::tile(tok)
}

fn l2_ctx() -> DecisionContext {
    DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 64 },
    }
}

#[test]
fn l2_obeys_iron_rules_and_between_hands_via_decide() {
    let mut rng = ChaCha8Rng::seed_from_u64(101);
    let ctx = l2_ctx();

    // Iron Rule 1: Tsumo immediately taken, BetweenHands returns ConfirmNextHand (never EndMatch).
    let wall = build_preset_wall(
        &tiles("123m 456m 789m 111p 22p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s EEE SSS WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");
    let east_view = project_view(&game, Seat::East);
    assert_eq!(decide(&east_view, &ctx, &mut rng), PlayerAction::Tsumo);

    game.apply_action(Seat::East, PlayerAction::Tsumo)
        .expect("tsumo settles hand");
    let between_view = project_view(&game, Seat::East);
    assert_eq!(between_view.phase, PublicPhase::BetweenHands);
    assert_eq!(
        decide(&between_view, &ctx, &mut rng),
        PlayerAction::ConfirmNextHand
    );

    // Iron Rule 2: 摸切 (from_drawn = true) when discarding tile equal to drawn_tile.
    let wall2 = build_preset_wall(
        &tiles("123m 456m 789m 23p 55p W"),
        &tiles("111s 222s 333s 444s F"),
        &tiles("555s 666s 777s 888s C"),
        &tiles("999s 111p 777p 888p B"),
        &tiles("E S N 5p"),
        &[],
    )
    .expect("valid wall");
    let mut game2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall2),
    )
    .expect("match starts");
    game2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("W"),
                from_drawn: false,
            },
        )
        .unwrap();
    game2
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile_one("E"),
                from_drawn: true,
            },
        )
        .unwrap();
    game2
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile_one("S"),
                from_drawn: true,
            },
        )
        .unwrap();
    game2
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile_one("N"),
                from_drawn: true,
            },
        )
        .unwrap();
    let drawn_view = project_view(&game2, Seat::East);
    assert_eq!(drawn_view.hand.drawn_tile, Some(tile_one("5p")));
    assert_eq!(
        decide(&drawn_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("5p"),
            from_drawn: true,
        }
    );
}

#[test]
fn l2_prefers_full_flush_and_half_flush_ev_and_respects_fan_cap() {
    let mut rng = ChaCha8Rng::seed_from_u64(202);
    let ctx = l2_ctx();

    // 1. 清一色 (3 fan with 门前清, 12 ukeire) vs basic 门前清 (1 fan, 16 ukeire):
    //    Hand: 111m 222m 56m 88m 99m 23p (14 tiles, shanten = 1)
    //    - TileEfficiencyBot discards 8m/9m (16 ukeire: 47m 14p) for 1 fan.
    //    - L2 discards 2p or 3p (12 ukeire: 4789m) for 3-fan 清一色 + 门前清!
    //    - Also when fan_cap = 1, 清一色 gives zero bonus over 门前清, so L2 switches to 8m/9m!
    let full_flush_wall = build_preset_wall(
        &tiles("111m 222m 56m 88m 99m 23p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s 777p 888p 999p W"),
        &[],
        &[],
    )
    .expect("valid wall");
    let full_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, full_flush_wall),
    )
    .expect("match starts");
    let mut full_view = project_view(&full_game, Seat::East);

    assert!(
        matches!(
            TileEfficiencyBot::decide(&full_view, &mut rng),
            PlayerAction::Discard { tile: t, .. } if t == tile_one("8m") || t == tile_one("9m")
        ),
        "L1 baseline should chase 16 ukeire by discarding 8m/9m"
    );
    let l2_choice = decide(&full_view, &ctx, &mut rng);
    assert!(
        matches!(
            l2_choice,
            PlayerAction::Discard { tile: t, .. } if t == tile_one("2p") || t == tile_one("3p")
        ),
        "L2 must shed pinzu (2p/3p) to chase 3-fan 清一色+门前清, got {l2_choice:?}"
    );

    full_view.rule_profile.fan_cap = 1;
    let capped_choice = decide(&full_view, &ctx, &mut rng);
    assert!(
        matches!(
            capped_choice,
            PlayerAction::Discard { tile: t, .. } if t == tile_one("8m") || t == tile_one("9m")
        ),
        "when fan_cap=1 caps 清一色 at 1 fan, L2 should maximize ukeire via 8m/9m, got {capped_choice:?}"
    );

    // 2. 混一色 (2 fan with 门前清, 12 ukeire) vs 1 fan 门前清 (16 ukeire):
    //    Hand: EEE 111m 56m 88m 99m 23p (14 tiles, shanten = 1)
    let half_flush_wall = build_preset_wall(
        &tiles("EEE 111m 56m 88m 99m 23p"),
        &tiles("111s 222s 333s 444s S"),
        &tiles("555s 666s 777s 888s W"),
        &tiles("999s 777p 888p 999p N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let half_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, half_flush_wall),
    )
    .expect("match starts");
    let half_view = project_view(&half_game, Seat::East);
    assert!(
        matches!(
            TileEfficiencyBot::decide(&half_view, &mut rng),
            PlayerAction::Discard { tile: t, .. } if t == tile_one("8m") || t == tile_one("9m")
        ),
        "L1 baseline sheds 8m/9m for 16 ukeire"
    );
    let l2_half = decide(&half_view, &ctx, &mut rng);
    assert!(
        matches!(
            l2_half,
            PlayerAction::Discard { tile: t, .. } if t == tile_one("2p") || t == tile_one("3p")
        ),
        "L2 must shed pinzu (2p/3p) to keep 2-fan 混一色+门前清, got {l2_half:?}"
    );
}

#[test]
fn l2_values_all_pungs_luxury_seven_pairs_and_gates_thirteen_orphans() {
    let mut rng = ChaCha8Rng::seed_from_u64(303);
    let ctx = l2_ctx();

    // 1. 碰碰胡 + 门前清 (2 fan, 4 outs) vs 门前清 (1 fan, 8 outs):
    //    Hand: 111m 222p 333s 44s 55p 6p (14 tiles)
    let toitoi_wall = build_preset_wall(
        &tiles("111m 222p 333s 44s 55p 6p"),
        &tiles("999m 999p 999s EEE F"),
        &tiles("888m 888p 888s SSS B"),
        &tiles("777m 777p 777s WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let toitoi_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, toitoi_wall),
    )
    .expect("match starts");
    let toitoi_view = project_view(&toitoi_game, Seat::East);
    assert_eq!(
        TileEfficiencyBot::decide(&toitoi_view, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("5p"),
            from_drawn: false,
        }
    );
    assert_eq!(
        decide(&toitoi_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("6p"),
            from_drawn: false,
        },
        "L2 discards 6p to lock in 2-fan 碰碰胡 + 门前清"
    );

    // 2. 豪华七对 + 清一色 (4 fan, 1 out) vs 1-fan 七对 (3 outs):
    //    Hand: 111m 22m 44m 66m 88m 99m 3p (14 tiles)
    let lux_wall = build_preset_wall(
        &tiles("111m 22m 44m 66m 88m 99m 3p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s 777p 888p 999p W"),
        &[],
        &[],
    )
    .expect("valid wall");
    let lux_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, lux_wall),
    )
    .expect("match starts");
    let lux_view = project_view(&lux_game, Seat::East);
    assert_eq!(
        TileEfficiencyBot::decide(&lux_view, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("1m"),
            from_drawn: false,
        }
    );
    assert_eq!(
        decide(&lux_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("3p"),
            from_drawn: false,
        },
        "L2 discards 3p for 4-fan 清一色+豪华七对"
    );

    // Toggle allow_four_of_kind_seven_pairs on 1111m 22p 44p 66s 88s 3s C (comparing discards):
    // - true: discard C (or 3s) for 2-fan 豪华七对 tenpai
    // - false: discard 1m (shanten 1 -> 6 ukeire vs shanten 2 if keeping 1111m)
    let lux_toggle_wall = build_preset_wall(
        &tiles("1111m 22p 44p 66s 88s 3s C"),
        &tiles("999m 999p 999s EEE F"),
        &tiles("555m 555p 555s SSS B"),
        &tiles("777m 777p 777s WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let lux_toggle_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, lux_toggle_wall),
    )
    .expect("match starts");
    let mut lux_toggle_view = project_view(&lux_toggle_game, Seat::East);
    lux_toggle_view
        .legal_actions
        .retain(|a| matches!(a, PlayerAction::Discard { .. }));
    assert!(
        matches!(
            decide(&lux_toggle_view, &ctx, &mut rng),
            PlayerAction::Discard { tile: t, .. } if t == tile_one("C") || t == tile_one("3s")
        ),
        "when 4-of-a-kind seven pairs is on, L2 keeps 1111m for 豪华七对"
    );
    lux_toggle_view.rule_profile.allow_four_of_kind_seven_pairs = false;
    assert_eq!(
        mahjong_ai::l2::decide(&lux_toggle_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("1m"),
            from_drawn: false,
        },
        "when 4-of-a-kind seven pairs is off, L2 must break 1111m via 1m"
    );

    // 3. 十三幺 gating:
    //    (a) shanten_13o == 4 (9 unique terminals/honors + 34m 56p 7s) -> NEVER chase 十三幺, shed terminal/honor.
    let messy_13o_wall = build_preset_wall(
        &tiles("19m 19s 1p E S W N 34m 56p 7s"),
        &tiles("222m 777m 888m 222p C"),
        &tiles("333p 444p 777p 888p F"),
        &tiles("222s 333s 444s 555s B"),
        &[],
        &[],
    )
    .expect("valid wall");
    let messy_13o_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, messy_13o_wall),
    )
    .expect("match starts");
    let messy_view = project_view(&messy_13o_game, Seat::East);
    let messy_action = decide(&messy_view, &ctx, &mut rng);
    assert!(
        matches!(
            messy_action,
            PlayerAction::Discard { tile: t, .. } if t.is_terminal_or_honor()
        ),
        "with shanten_13o == 4, L2 must not hoard orphans at the cost of 34m/56p, got {messy_action:?}"
    );

    //    (b) shanten_13o == 2 (11 unique terminals/honors + 11m + 4m 5p) -> pursues 4-fan 十三幺 by shedding 4m or 5p.
    let close_13o_wall = build_preset_wall(
        &tiles("119m 19s 19p E S W N C 4m 5p"),
        &tiles("222m 333m 666m 777m F"),
        &tiles("222p 333p 444p 666p B"),
        &tiles("222s 333s 444s 555s 7s"),
        &[],
        &[],
    )
    .expect("valid wall");
    let close_13o_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, close_13o_wall),
    )
    .expect("match starts");
    let close_view = project_view(&close_13o_game, Seat::East);
    let close_action = decide(&close_view, &ctx, &mut rng);
    assert!(
        matches!(
            close_action,
            PlayerAction::Discard { tile: t, .. } if t == tile_one("4m") || t == tile_one("5p")
        ),
        "with shanten_13o == 2, L2 must discard middle tile 4m/5p to pursue 十三幺, got {close_action:?}"
    );
}

#[test]
fn l2_pon_vs_pass_disciplined_on_menqing_and_big_hands() {
    let mut rng = ChaCha8Rng::seed_from_u64(404);
    let ctx = l2_ctx();

    // 1. Closed 1-shanten hand (11m 22m 33m 44m 67m 99m C) where Pon(1m) loses 门前清 and 七对
    //    (leaving only 1 remaining 8m wait at 2-fan 清一色 after four 5m and three 8m are discarded),
    //    while Pass preserves 16+ improving tiles for 3-4 fan 清一色+门前清/七对/豪华七对:
    //    `TileEfficiencyBot` calls Pon(1m) (1->0 shanten), whereas L2 chooses Pass!
    let mut pass_closed_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999p 999s 888s E 5m 5m 8m 1m"),
                &tiles("11m 22m 33m 44m 67m 99m C"),
                &tiles("888p 666p 777s WW 5m 8m"),
                &tiles("555p 444p 666s 13s 5m 8m"),
                &tiles("B 2s 3s 4s N F 1p 2p 3p 1p 2p W"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    for (seat, tok, from_drawn) in [
        (Seat::East, "5m", false),
        (Seat::South, "B", true),
        (Seat::West, "5m", false),
        (Seat::North, "5m", false),
        (Seat::East, "5m", false),
        (Seat::South, "N", true),
        (Seat::West, "8m", false),
        (Seat::North, "8m", false),
        (Seat::East, "8m", false),
        (Seat::South, "3p", true),
        (Seat::West, "1p", true),
        (Seat::North, "2p", true),
        (Seat::East, "1m", false),
    ] {
        pass_closed_game
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile: tile_one(tok),
                    from_drawn,
                },
            )
            .unwrap();
    }
    let south_closed_view = project_view(&pass_closed_game, Seat::South);
    assert!(south_closed_view
        .legal_actions
        .contains(&PlayerAction::Pon {
            tile: tile_one("1m")
        }));
    assert_eq!(
        TileEfficiencyBot::decide(&south_closed_view, &mut rng),
        PlayerAction::Pon {
            tile: tile_one("1m")
        },
        "L1 blindly Pons 1->0 regardless of losing 门前清"
    );
    assert_eq!(
        decide(&south_closed_view, &ctx, &mut rng),
        PlayerAction::Pass,
        "L2 passes Pon to preserve 3-fan 清一色+门前清 EV"
    );

    // 2. High-value 混一色 + 碰碰胡 Pon: South holds 111m 44m 88m EE SS 2p (13 tiles)
    //    and East discards E -> Pon(E) + discard 2p reaches 2-fan open tenpai immediately!
    let mut big_pon_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999p 999s 888s WWW E C"),
                &tiles("111m 44m 888m EE SS 2p"),
                &tiles("777p 666s 555s BBB F"),
                &tiles("444p 333p 222s NNN 1s"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    big_pon_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("E"),
                from_drawn: false,
            },
        )
        .unwrap();
    let big_pon_view = project_view(&big_pon_game, Seat::South);
    assert_eq!(
        decide(&big_pon_view, &ctx, &mut rng),
        PlayerAction::Pon {
            tile: tile_one("E")
        },
        "L2 must Pon E to enter 混一色+碰碰胡 tenpai"
    );

    // 3. Already open hand (!is_menqing): South already called Pon(9p) and holds 11m 234p 56s 8s 55z,
    //    East discards 1m -> 门前清 already lost, so L2 calls Pon(1m) to reach tenpai!
    let mut open_pon_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 888p 999s EEE 9p 1m"),
                &tiles("99p 11m 234p 56s 55z N B"),
                &tiles("888m 777p 888s SSS F"),
                &tiles("777m 666p 777s WWW C"),
                &tiles("2m 3m 4m 5m"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    open_pon_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("9p"),
                from_drawn: false,
            },
        )
        .unwrap();
    open_pon_game
        .apply_action(
            Seat::South,
            PlayerAction::Pon {
                tile: tile_one("9p"),
            },
        )
        .unwrap();
    open_pon_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile_one("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    open_pon_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile_one("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    open_pon_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile_one("3m"),
                from_drawn: true,
            },
        )
        .unwrap();
    open_pon_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let open_pon_view = project_view(&open_pon_game, Seat::South);
    assert_eq!(
        decide(&open_pon_view, &ctx, &mut rng),
        PlayerAction::Pon {
            tile: tile_one("1m")
        },
        "with 门前清 already lost, L2 calls Pon(1m) for speed"
    );
}

#[test]
fn l2_evaluates_kongs_rinshan_and_rob_kong_risk_hook() {
    use mahjong_ai::l2::{evaluate_candidates, RobKongRiskModel, ZeroRobKongRisk};

    let mut rng = ChaCha8Rng::seed_from_u64(505);
    let ctx = l2_ctx();

    // 1a. ConcealedKong declared when it keeps shanten 0 (+6 net kong score + 杠上开花 upside).
    let ck_yes_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 234m 567p 89s 55z"),
                &tiles("999m 999p 999s EEE F"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW N"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    assert_eq!(
        decide(&project_view(&ck_yes_game, Seat::East), &ctx, &mut rng),
        PlayerAction::ConcealedKong {
            tile: tile_one("1m")
        }
    );

    // 1b. ConcealedKong skipped when it breaks 4-fan 清一色+豪华七对 tenpai.
    let ck_skip_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 22m 44m 66m 88m 3m C"),
                &tiles("999p 999s 888s EEE F"),
                &tiles("888p 777s 666s SSS B"),
                &tiles("777p 555s 444s WWW N"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    assert_eq!(
        decide(&project_view(&ck_skip_game, Seat::East), &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("C"),
            from_drawn: false,
        },
        "L2 skips +6 ConcealedKong to preserve 4-fan 清一色+豪华七对 tenpai"
    );

    // 2. ExposedKong: closed 3-fan 混一色+碰碰胡+门前清 passes ExposedKong (+2 can't offset losing 门前清's 4->8 base loss),
    //    whereas open hand (!is_menqing) takes ExposedKong for +2 + 杠上开花 upside.
    let mut ek_closed_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999p 999s 888s WWW 1m B"),
                &tiles("111m 333m 888m 44m EE"),
                &tiles("888p 777s 666s SSS F"),
                &tiles("777p 666p 555s 123p C"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    ek_closed_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let ek_closed_view = project_view(&ek_closed_game, Seat::South);
    assert_eq!(
        TileEfficiencyBot::decide(&ek_closed_view, &mut rng),
        PlayerAction::ExposedKong {
            tile: tile_one("1m")
        }
    );
    assert_eq!(
        decide(&ek_closed_view, &ctx, &mut rng),
        PlayerAction::Pass,
        "L2 refuses +2 ExposedKong that breaks 门前清 on 3-fan 混一色+碰碰胡"
    );

    let mut ek_open_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 888p 999s EEE 9p 1m"),
                &tiles("99p 111m 234p 56s 55z B"),
                &tiles("888m 777p 888s SSS F"),
                &tiles("777m 666p 777s WWW C"),
                &tiles("2m 3m 4m 5m"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    ek_open_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("9p"),
                from_drawn: false,
            },
        )
        .unwrap();
    ek_open_game
        .apply_action(
            Seat::South,
            PlayerAction::Pon {
                tile: tile_one("9p"),
            },
        )
        .unwrap();
    ek_open_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile_one("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    ek_open_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile_one("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    ek_open_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile_one("3m"),
                from_drawn: true,
            },
        )
        .unwrap();
    ek_open_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let ek_open_view = project_view(&ek_open_game, Seat::South);
    assert_eq!(
        decide(&ek_open_view, &ctx, &mut rng),
        PlayerAction::ExposedKong {
            tile: tile_one("1m")
        },
        "with 门前清 already lost, L2 takes ExposedKong for +2 + 杠上开花"
    );

    // 3. AddedKong + RobKongRiskModel hook:
    //    With ZeroRobKongRisk, AddedKong(5m) wins (+3 net + 杠上开花).
    //    With high rob-kong risk supplied via RobKongRiskModel, AddedKong EV drops below Discard.
    let mut ak_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 5m C"),
                &tiles("55m 123p 456p 78s 44m B"),
                &tiles("888m 888p 888s SSS N"),
                &tiles("777m 777p 777s WWW F"),
                &tiles("1m 2m 1s 5m"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Pon {
                tile: tile_one("5m"),
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile_one("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile_one("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile_one("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile_one("1s"),
                from_drawn: true,
            },
        )
        .unwrap();
    let ak_view = project_view(&ak_game, Seat::South);
    assert_eq!(
        decide(&ak_view, &ctx, &mut rng),
        PlayerAction::AddedKong {
            tile: tile_one("5m")
        }
    );

    struct HighRobRisk;
    impl RobKongRiskModel for HighRobRisk {
        fn rob_kong_risk_ev(&self, _view: &mahjong_core::PlayerView, _tile: Tile) -> f64 {
            5.0
        }
    }

    let cands_zero = evaluate_candidates(&ak_view, &ZeroRobKongRisk);
    let cands_risk = evaluate_candidates(&ak_view, &HighRobRisk);
    let ak_zero = cands_zero
        .as_slice()
        .iter()
        .find(|c| {
            c.action
                == PlayerAction::AddedKong {
                    tile: tile_one("5m"),
                }
        })
        .expect("AddedKong candidate present");
    let ak_risk = cands_risk
        .as_slice()
        .iter()
        .find(|c| {
            c.action
                == PlayerAction::AddedKong {
                    tile: tile_one("5m"),
                }
        })
        .expect("AddedKong candidate present");
    assert!(
        (ak_zero.ev - ak_risk.ev - 5.0).abs() < 1e-6,
        "rob_kong_risk_ev must subtract cleanly from AddedKong EV"
    );
    let best_under_risk = cands_risk
        .as_slice()
        .iter()
        .max_by(|a, b| a.ev.total_cmp(&b.ev))
        .expect("non-empty candidates");
    assert!(
        matches!(best_under_risk.action, PlayerAction::Discard { .. }),
        "high rob-kong risk must deter AddedKong in favor of Discard, got {:?}",
        best_under_risk.action
    );
}

#[test]
fn l2_handles_s2_dp_and_escapes_dead_tenpai_waits() {
    let mut rng = ChaCha8Rng::seed_from_u64(606);
    let ctx = l2_ctx();

    // 1. At s == 0 with 0 live waits (all 4 copies of 2p visible in rivers), L1 still locks
    //    into dead tenpai (`Discard 5s`), whereas L2 pivots out (`Discard 1p` or `3p`) to live EV!
    let mut dead_wait_game2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999p 999m 888s E 3m 7m 2p 2p"),
                &tiles("123m 456m 789s 22s 1p S"),
                &tiles("888m 777p 666s WW 4m 2p"),
                &tiles("555p 444p 333s CC 7s 2p"),
                &tiles("B F N 1s 3p 4s 8p 5m 5s"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    for (seat, tok, from_drawn) in [
        (Seat::East, "2p", false),
        (Seat::South, "B", true),
        (Seat::West, "2p", false),
        (Seat::North, "2p", false),
        (Seat::East, "2p", false),
        (Seat::South, "S", false),
        (Seat::West, "4s", true),
        (Seat::North, "8p", true),
        (Seat::East, "E", false),
    ] {
        dead_wait_game2
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile: tile_one(tok),
                    from_drawn,
                },
            )
            .unwrap();
    }
    let south_dead_view = project_view(&dead_wait_game2, Seat::South);
    assert_eq!(
        TileEfficiencyBot::decide(&south_dead_view, &mut rng),
        PlayerAction::Discard {
            tile: tile_one("5s"),
            from_drawn: true,
        },
        "L1 locks into dead 0-wait tenpai"
    );
    let l2_escape = decide(&south_dead_view, &ctx, &mut rng);
    assert!(
        matches!(
            l2_escape,
            PlayerAction::Discard { tile: t, .. } if t == tile_one("1p") || t == tile_one("3p")
        ),
        "L2 escapes dead 2p tenpai by shedding 1p/3p for live 5s/pairs EV, got {l2_escape:?}"
    );

    // 2. s == 2 DP: 123m 99p 34p 57s 5m 8m 1s 9s C -> sheds isolated C over middle/terminal connected tiles.
    let s2_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("123m 99p 34p 57s 5m 8m 1s 9s C"),
                &tiles("666m 777m 111p 222p E"),
                &tiles("555p 666p 777p 888p S"),
                &tiles("222s 333s 444s 666s W"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    let s2_view = project_view(&s2_game, Seat::East);
    let s2_action = decide(&s2_view, &ctx, &mut rng);
    assert_eq!(
        s2_action,
        PlayerAction::Discard {
            tile: tile_one("C"),
            from_drawn: false,
        }
    );
}

mod common;

use common::{tile, tiles};
use mahjong_ai::l3::{decide_with_options, L3DefenseModel, L3Profile};
use mahjong_ai::l4::{sample_sim_state, should_trigger, SimState};
use mahjong_ai::{decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext};
use mahjong_core::{
    build_preset_wall, project_view, Match, MatchMode, MeldView, PlayerAction, PresetWallProvider,
    PublicPhase, Seat, SeededWallProvider, Tile, TuidaohuRuleset,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use std::time::Instant;

fn assert_copy<T: Copy>() {}

fn strong_det_ctx(max_samples: usize) -> DecisionContext {
    DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples },
    }
}

fn verify_sim_conservation(sim: &SimState, view: &mahjong_core::PlayerView) {
    let mut tile_totals = [0u8; Tile::COUNT];

    for seat in Seat::ALL {
        let idx = seat.index();
        for (k, &c) in sim.hands[idx].iter().enumerate() {
            tile_totals[k] = tile_totals[k].saturating_add(c);
        }
        for (k, &c) in sim.melds[idx].counts.iter().enumerate() {
            tile_totals[k] = tile_totals[k].saturating_add(c);
        }
        for entry in &view.seats[idx].river {
            if entry.claimed_by.is_none() {
                tile_totals[entry.tile.index()] = tile_totals[entry.tile.index()].saturating_add(1);
            }
        }
    }

    for &t in &sim.wall[usize::from(sim.wall_head)..usize::from(sim.wall_tail)] {
        tile_totals[usize::from(t)] = tile_totals[usize::from(t)].saturating_add(1);
    }

    let grand_total: usize = tile_totals.iter().map(|&c| usize::from(c)).sum();
    assert_eq!(
        grand_total,
        Tile::DECK_SIZE,
        "136-tile conservation violated"
    );
    for (k, &cnt) in tile_totals.iter().enumerate() {
        assert_eq!(cnt, 4, "tile index {k} must have exact count 4, got {cnt}");
    }
}

#[test]
fn hidden_concealed_kongs_conserve_136_tiles_across_one_and_two_kongs() {
    assert_copy::<SimState>();

    // Case 1: 1 opponent hidden ConcealedKong { tile: None }.
    let mut game1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 234m 456p 89s 55z"),
                &tiles("234m 123p 234s 67s EE"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW N"),
                &tiles("2p 5s 6p"),
                &tiles("3p"),
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game1
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: tile("1m") })
        .expect("East concealed kong");
    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .expect("East discards 3p");

    let south_view1 = project_view(&game1, Seat::South);
    assert_eq!(
        south_view1.seats[Seat::East.index()]
            .melds
            .iter()
            .filter(|m| matches!(m, MeldView::ConcealedKong { tile: None, .. }))
            .count(),
        1
    );

    let ctx = strong_det_ctx(32);
    let profile = L3Profile::from_context(&ctx, true);
    let model1 = L3DefenseModel::new(&south_view1, profile);
    for seed in 1..=32u64 {
        let sim = sample_sim_state(&south_view1, &model1, seed);
        verify_sim_conservation(&sim, &south_view1);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let action = decide(&south_view1, &ctx, &mut rng);
        assert!(south_view1.legal_actions.contains(&action));
    }

    // Case 2: 2 opponent hidden ConcealedKong { tile: None } across two opponents.
    let mut game2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 234m 123p 89s 55z"),
                &tiles("9999p 234m 567s 6s EE"),
                &tiles("567m 123s 456p 45s FF"),
                &tiles("777m 777p 777s WWW N"),
                &tiles("2p 5p 6p"),
                &tiles("3p 4p"),
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game2
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: tile("1m") })
        .expect("East concealed kong");
    game2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .expect("East discards 3p");
    game2
        .apply_action(
            Seat::South,
            PlayerAction::ConcealedKong { tile: tile("9p") },
        )
        .expect("South concealed kong");
    game2
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("4p"),
                from_drawn: true,
            },
        )
        .expect("South discards 4p");

    let west_view2 = project_view(&game2, Seat::West);
    let hidden_count = west_view2
        .seats
        .iter()
        .flat_map(|s| s.melds.iter())
        .filter(|m| matches!(m, MeldView::ConcealedKong { tile: None, .. }))
        .count();
    assert_eq!(hidden_count, 2);

    let model2 = L3DefenseModel::new(&west_view2, profile);
    for seed in 1..=32u64 {
        let sim = sample_sim_state(&west_view2, &model2, seed * 17);
        verify_sim_conservation(&sim, &west_view2);
        let mut rng = ChaCha8Rng::seed_from_u64(seed * 17);
        let action = decide(&west_view2, &ctx, &mut rng);
        assert!(west_view2.legal_actions.contains(&action));
    }
}

fn high_tension_meld_view() -> mahjong_core::PlayerView {
    let wall = build_preset_wall(
        &tiles("11m 23m 56m 78p 22s 67s 9m C"),
        &tiles("WW 444m 999p 666m 1s 9s"),
        &tiles("11s 99s EE 23s 44p 2p 7p 8p"),
        &tiles("666s 333m 888m 555s 1m"),
        &tiles("N B S F E"),
        &[],
    )
    .expect("valid wall");
    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");
    for (seat, action) in [
        (
            Seat::East,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        ),
        (
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: false,
            },
        ),
        (Seat::West, PlayerAction::Pon { tile: tile("1s") }),
        (
            Seat::West,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        ),
        (
            Seat::North,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: true,
            },
        ),
        (
            Seat::East,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        ),
        (
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9s"),
                from_drawn: false,
            },
        ),
        (Seat::West, PlayerAction::Pon { tile: tile("9s") }),
        (
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: false,
            },
        ),
        (
            Seat::North,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        ),
    ] {
        game.apply_action(seat, action).expect("step applies");
    }
    project_view(&game, Seat::East)
}

fn run_match_trace_in_pool(threads: usize, max_samples: usize) -> Vec<(Seat, PlayerAction)> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("pool builds");
    let ctx = strong_det_ctx(max_samples);

    pool.install(|| {
        let mut game = Match::new(
            MatchMode::EastOnly,
            TuidaohuRuleset,
            SeededWallProvider::new(707),
        )
        .expect("match starts");
        let mut rngs: [ChaCha8Rng; 4] =
            std::array::from_fn(|i| ChaCha8Rng::seed_from_u64(9001 + i as u64));
        let mut trace = Vec::new();

        loop {
            let views = Seat::ALL.map(|s| project_view(&game, s));
            if views[0].phase == PublicPhase::MatchFinished {
                break;
            }
            let mut stepped = false;
            for (idx, view) in views.iter().enumerate() {
                if view.legal_actions.is_empty() {
                    continue;
                }
                if view.phase == PublicPhase::BetweenHands
                    && !view.legal_actions.contains(&PlayerAction::ConfirmNextHand)
                {
                    continue;
                }
                let action = decide(view, &ctx, &mut rngs[idx]);
                trace.push((view.viewer, action));
                game.apply_action(view.viewer, action)
                    .expect("legal action applies");
                stepped = true;
                break;
            }
            assert!(stepped, "match stalled");
        }

        trace
    })
}

#[test]
fn deterministic_budget_is_bit_exact_across_single_and_multi_thread_pools() {
    let view = high_tension_meld_view();
    let ctx = strong_det_ctx(128);
    let (cands, model) = mahjong_ai::l3::evaluate_l3_candidates(&view, &ctx, true);
    assert!(
        should_trigger(&view, &ctx, true, cands.as_slice(), &model),
        "high tension meld view must trigger L4"
    );

    let pool1 = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("1-thread pool");
    let pool4 = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .expect("4-thread pool");

    for seed in 1..=8u64 {
        let a1 = pool1.install(|| {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            decide(&view, &ctx, &mut rng)
        });
        let a4 = pool4.install(|| {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            decide(&view, &ctx, &mut rng)
        });
        assert_eq!(
            a1, a4,
            "seed {seed} must be bit-exact across 1 vs 4 Rayon threads"
        );
    }

    let trace_1 = run_match_trace_in_pool(1, 32);
    let trace_4 = run_match_trace_in_pool(4, 32);
    assert!(!trace_1.is_empty());
    assert_eq!(
        trace_1, trace_4,
        "full EastOnly Match trace must be 100% bit-exact across 1 vs 4 Rayon threads"
    );
}

#[test]
fn intermediate_difficulty_never_triggers_l4_even_under_high_tension_or_melds() {
    let view = high_tension_meld_view();
    let inter_1 = DecisionContext {
        difficulty: AiDifficulty::Intermediate,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 1 },
    };
    let inter_256 = DecisionContext {
        difficulty: AiDifficulty::Intermediate,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 256 },
    };

    let (cands, model) = mahjong_ai::l3::evaluate_l3_candidates(&view, &inter_256, false);
    assert!(!should_trigger(
        &view,
        &inter_256,
        false,
        cands.as_slice(),
        &model
    ));
    assert!(!should_trigger(
        &view,
        &inter_256,
        true,
        cands.as_slice(),
        &model
    ));

    for seed in 0..16u64 {
        let mut r1 = ChaCha8Rng::seed_from_u64(seed);
        let mut r2 = ChaCha8Rng::seed_from_u64(seed);
        let mut r3 = ChaCha8Rng::seed_from_u64(seed);
        let a_1 = decide(&view, &inter_1, &mut r1);
        let a_256 = decide(&view, &inter_256, &mut r2);
        let a_no_l4 = decide_with_options(&view, &inter_256, false, &mut r3);
        assert_eq!(a_1, a_256, "Intermediate must ignore max_samples");
        assert_eq!(a_1, a_no_l4, "Intermediate must match L1-L3 directly");
    }
}

#[test]
fn realtime_budget_enforces_wall_clock_caps_and_returns_legal_actions() {
    let view = high_tension_meld_view();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .expect("4-thread pool");

    let std_rt = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Realtime {
            max_samples: 128,
            soft_ms: 120,
            hard_ms: 180,
        },
    };
    let tight_rt = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Realtime {
            max_samples: 128,
            soft_ms: 10,
            hard_ms: 25,
        },
    };

    pool.install(|| {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let t0 = Instant::now();
        let action_std = decide(&view, &std_rt, &mut rng);
        let elapsed_std = t0.elapsed();
        assert!(view.legal_actions.contains(&action_std));
        assert!(
            elapsed_std.as_millis() <= 240,
            "standard Realtime exceeded wall-clock bound: {elapsed_std:?}"
        );

        let t1 = Instant::now();
        let action_tight = decide(&view, &tight_rt, &mut rng);
        let elapsed_tight = t1.elapsed();
        assert!(view.legal_actions.contains(&action_tight));
        assert!(
            elapsed_tight.as_millis() <= 80,
            "tight Realtime (10/25ms) did not abort promptly: {elapsed_tight:?}"
        );
    });
}

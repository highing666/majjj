mod common;

use common::tiles;
use mahjong_ai::{
    decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext, TileEfficiencyBot,
};
use mahjong_core::{
    build_preset_wall, project_view, Match, MatchMode, PlayerAction, PresetWallProvider,
    PublicPhase, Seat, Tile, TuidaohuRuleset,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

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
        decide(&lux_toggle_view, &ctx, &mut rng),
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
fn l2_evaluates_kongs_rinshan_and_added_kong_via_decide() {
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

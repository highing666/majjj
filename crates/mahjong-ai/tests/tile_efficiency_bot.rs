mod common;

use common::{tile, tiles};
use mahjong_ai::{
    decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext, TileEfficiencyBot,
};
use mahjong_core::{
    build_preset_wall, project_event, project_view, HandOutcome, Match, MatchMode, MatchSummary,
    PlayerAction, PlayerEventKind, PresetWallProvider, PublicPhase, Seat, SeededWallProvider,
    TuidaohuRuleset,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use ts_rs::TS;

#[test]
fn contract_types_defaults_serde_and_ts_exports() {
    assert_eq!(AiDifficulty::default(), AiDifficulty::Strong);
    assert_eq!(AiStyle::default(), AiStyle::Balanced);

    let ctx = DecisionContext::default();
    assert_eq!(ctx.difficulty, AiDifficulty::Strong);
    assert_eq!(ctx.style, AiStyle::Balanced);
    assert!(matches!(
        ctx.budget,
        DecisionBudget::Realtime { .. } | DecisionBudget::Deterministic { .. }
    ));

    for diff in [AiDifficulty::Strong, AiDifficulty::Intermediate] {
        let json = serde_json::to_string(&diff).expect("serialize AiDifficulty");
        let parsed: AiDifficulty = serde_json::from_str(&json).expect("deserialize AiDifficulty");
        assert_eq!(parsed, diff);
    }
    for style in [
        AiStyle::AggressiveFast,
        AiStyle::Balanced,
        AiStyle::DefensiveBigHand,
    ] {
        let json = serde_json::to_string(&style).expect("serialize AiStyle");
        let parsed: AiStyle = serde_json::from_str(&json).expect("deserialize AiStyle");
        assert_eq!(parsed, style);
    }

    assert!(AiDifficulty::decl().contains("Strong"));
    assert!(AiDifficulty::decl().contains("Intermediate"));
    assert!(AiStyle::decl().contains("AggressiveFast"));
    assert!(AiStyle::decl().contains("Balanced"));
    assert!(AiStyle::decl().contains("DefensiveBigHand"));
}

#[test]
fn iron_rule_1_wins_immediately_and_between_hands_confirms_next_hand() {
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let ctx = DecisionContext::default();

    // 1. Tsumo on dealer's initial 14-tile hand in FreeTable, then BetweenHands confirms next hand.
    let wall = build_preset_wall(
        &tiles("123m 456m 789m 111p 22p"),
        &tiles("111s 222s 333s 444s E"),
        &tiles("555s 666s 777s 888s S"),
        &tiles("999s EEE SSS WWW N"),
        &[],
        &[],
    )
    .expect("valid preset wall");
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");
    let east_view = project_view(&game, Seat::East);
    assert!(east_view.legal_actions.contains(&PlayerAction::Tsumo));
    assert_eq!(decide(&east_view, &ctx, &mut rng), PlayerAction::Tsumo);
    assert_eq!(
        TileEfficiencyBot::decide(&east_view, &mut rng),
        PlayerAction::Tsumo
    );

    game.apply_action(Seat::East, PlayerAction::Tsumo)
        .expect("tsumo settles hand");
    let between_view = project_view(&game, Seat::East);
    assert_eq!(between_view.phase, PublicPhase::BetweenHands);
    assert!(between_view
        .legal_actions
        .contains(&PlayerAction::ConfirmNextHand));
    assert!(between_view.legal_actions.contains(&PlayerAction::EndMatch));
    assert_eq!(
        decide(&between_view, &ctx, &mut rng),
        PlayerAction::ConfirmNextHand
    );

    // 2. Ron on discard reaction even when Pon and Pass are also legal.
    let mut ron_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("123m 456m 789m 11p 22p 5p"),
                &tiles("123s 456s 789s 44p 55p"),
                &tiles("11m 22m 33m 44m 55m 66m 7m"),
                &tiles("11s 22s 33s 44s 55s 66s 8s"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    ron_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("5p"),
                from_drawn: false,
            },
        )
        .expect("East discards 5p");
    let south_view = project_view(&ron_game, Seat::South);
    assert!(south_view
        .legal_actions
        .contains(&PlayerAction::Ron { tile: tile("5p") }));
    assert!(south_view
        .legal_actions
        .contains(&PlayerAction::Pon { tile: tile("5p") }));
    assert_eq!(
        decide(&south_view, &ctx, &mut rng),
        PlayerAction::Ron { tile: tile("5p") }
    );

    // 3. RobKongHu on opponent's AddedKong attempt.
    let mut rob_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("123m 456m 789m 13p 44p 2p"),
                &tiles("123s 456s 789s 22p 88p"),
                &tiles("11m 22m 33m 44m 55m 66m 7m"),
                &tiles("11s 22s 33s 44s 55s 66s 8s"),
                &tiles("W N C 2p"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    rob_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        )
        .expect("East discards 2p");
    rob_game
        .apply_action(Seat::South, PlayerAction::Pon { tile: tile("2p") })
        .expect("South pons 2p");
    rob_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("8p"),
                from_drawn: false,
            },
        )
        .expect("South discards 8p");
    rob_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("W"),
                from_drawn: true,
            },
        )
        .expect("West discards W");
    rob_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("N"),
                from_drawn: true,
            },
        )
        .expect("North discards N");
    rob_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: true,
            },
        )
        .expect("East discards C");
    rob_game
        .apply_action(Seat::South, PlayerAction::AddedKong { tile: tile("2p") })
        .expect("South attempts AddedKong 2p");
    let east_rob_view = project_view(&rob_game, Seat::East);
    assert!(east_rob_view
        .legal_actions
        .contains(&PlayerAction::RobKongHu { tile: tile("2p") }));
    assert_eq!(
        decide(&east_rob_view, &ctx, &mut rng),
        PlayerAction::RobKongHu { tile: tile("2p") }
    );
}

#[test]
fn tile_efficiency_reduces_shanten_accounts_visible_and_defaults_to_tsumogiri() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // 1. Sheds isolated honor C to reach tenpai (shanten = 0).
    let wall1 = build_preset_wall(
        &tiles("123m 456m 789m 23p 88s C"),
        &tiles("111p 444p 555p 666p E"),
        &tiles("777p 888p 999p 111s S"),
        &tiles("222s 333s 444s NNN W"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall1),
    )
    .expect("match starts");
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game1, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("C"),
            from_drawn: false
        }
    );

    // 2. Iron Rule 2 (摸切): drawn_tile 5p is also in closed_tiles (55p), discarding 5p is best,
    //    must choose from_drawn = true.
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
                tile: tile("W"),
                from_drawn: false,
            },
        )
        .expect("East discards W");
    game2
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: true,
            },
        )
        .expect("South discards E");
    game2
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        )
        .expect("West discards S");
    game2
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("N"),
                from_drawn: true,
            },
        )
        .expect("North discards N");
    let east_drawn_view = project_view(&game2, Seat::East);
    assert_eq!(east_drawn_view.hand.drawn_tile, Some(tile("5p")));
    assert_eq!(
        TileEfficiencyBot::decide(&east_drawn_view, &mut rng),
        PlayerAction::Discard {
            tile: tile("5p"),
            from_drawn: true
        }
    );

    // 3. Visible tile accounting without double-counting claimed discards:
    //    North holds 123m 456m EE SS 13s 46p; South has Pon(2s) claimed from East's river
    //    (3 visible 2s -> 1 remaining), while all 4 copies of 5p sit in rivers (0 remaining).
    //    Bot must keep 13s (wait 2s) and discard 4p or 6p rather than breaking 13s.
    let wall3 = build_preset_wall(
        &tiles("999m 999p CCC 135m 2s 5p"),
        &tiles("888m 888p BBB W 22s 5p"),
        &tiles("777m 777p NNN 468s 5p"),
        &tiles("123m 456m EE SS 13s 4p"),
        &tiles("5p 1p 2p 3p 1p 6p"),
        &[],
    )
    .expect("valid wall");
    let mut game3 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall3),
    )
    .expect("match starts");
    game3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("2s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game3
        .apply_action(Seat::South, PlayerAction::Pon { tile: tile("2s") })
        .unwrap();
    game3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("5p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game3
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("5p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game3
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("5p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game3
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("5p"),
                from_drawn: false,
            },
        )
        .unwrap();
    let north_view = project_view(&game3, Seat::North);
    let choice3 = TileEfficiencyBot::decide(&north_view, &mut rng);
    assert!(
        matches!(
            choice3,
            PlayerAction::Discard { tile: t, .. } if t == tile("4p") || t == tile("6p")
        ),
        "expected bot to discard 4p or 6p and keep live 13s wait, got {choice3:?}"
    );

    // 4. Tie-breaker: sheds isolated honor before isolated terminal before middle tile.
    let wall4 = build_preset_wall(
        &tiles("123m 456m EE SS 55p 1m W"),
        &tiles("999m 999p 999s CCC F"),
        &tiles("888m 888p 888s BBB N"),
        &tiles("777m 777p 777s 234p 6p"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game4 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall4),
    )
    .expect("match starts");
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game4, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("W"),
            from_drawn: false
        }
    );

    let wall5 = build_preset_wall(
        &tiles("123m 456m EE SS 55p 1p 5s"),
        &tiles("999m 999p 999s CCC F"),
        &tiles("888m 888p 888s BBB N"),
        &tiles("777m 777p 777s 234m 6m"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game5 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall5),
    )
    .expect("match starts");
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game5, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("1p"),
            from_drawn: false
        }
    );

    // 5. Seven Pairs (allow_four_of_kind_seven_pairs = true vs false) and Thirteen Orphans.
    let wall6 = build_preset_wall(
        &tiles("1111m 22p 55p 88p 33s 6s C"),
        &tiles("999m 999p 999s EEE F"),
        &tiles("444m 444p 444s SSS B"),
        &tiles("777m 777p 777s WWW N"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game6 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall6),
    )
    .expect("match starts");
    let mut seven_pair_view = project_view(&game6, Seat::East);
    assert_eq!(
        TileEfficiencyBot::decide(&seven_pair_view, &mut rng),
        PlayerAction::Discard {
            tile: tile("C"),
            from_drawn: false
        }
    );
    // When 4-of-a-kind seven pairs is disallowed, breaking 1111m gives higher ukeire (6s + C vs only 6s).
    seven_pair_view.rule_profile.allow_four_of_kind_seven_pairs = false;
    assert_eq!(
        TileEfficiencyBot::decide(&seven_pair_view, &mut rng),
        PlayerAction::Discard {
            tile: tile("1m"),
            from_drawn: false
        }
    );

    // Thirteen Orphans: discards 5m to enter tenpai waiting on B.
    let wall7 = build_preset_wall(
        &tiles("19m 19s 19p E S W N C F 1m 5m"),
        &tiles("222m 333m 444m 666m 7m"),
        &tiles("222p 333p 444p 666p 7p"),
        &tiles("222s 333s 444s 666s 7s"),
        &[],
        &[],
    )
    .expect("valid wall");
    let game7 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall7),
    )
    .expect("match starts");
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game7, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: false
        }
    );
}

#[test]
fn turn_kongs_and_discard_reactions_follow_shanten_rules() {
    let mut rng = ChaCha8Rng::seed_from_u64(77);

    // 1a. ConcealedKong declared when it does not worsen shanten (0 -> 0).
    let game_ck_yes = Match::new(
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
        TileEfficiencyBot::decide(&project_view(&game_ck_yes, Seat::East), &mut rng),
        PlayerAction::ConcealedKong { tile: tile("1m") }
    );

    // 1b. ConcealedKong skipped when it worsens standard shanten (0 -> 1).
    let game_ck_no_std = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("11112m 345p 678s 99s 5z"),
                &tiles("999m 999p 888p EEE F"),
                &tiles("888m 777p 444s SSS B"),
                &tiles("777m 666p 333s WWW N"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game_ck_no_std, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("5z"),
            from_drawn: false
        }
    );

    // 1c. ConcealedKong skipped when it breaks Seven Pairs tenpai.
    let game_ck_no_7p = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 22p 33p 44s 55s 6s C"),
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
        TileEfficiencyBot::decide(&project_view(&game_ck_no_7p, Seat::East), &mut rng),
        PlayerAction::Discard {
            tile: tile("C"),
            from_drawn: false
        }
    );

    // 2a. AddedKong declared when it keeps shanten (0 -> 0).
    let mut game_ak_yes = Match::new(
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
    game_ak_yes
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_ak_yes
        .apply_action(Seat::South, PlayerAction::Pon { tile: tile("5m") })
        .unwrap();
    game_ak_yes
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_ak_yes
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_ak_yes
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_ak_yes
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: true,
            },
        )
        .unwrap();
    let ak_yes_view = project_view(&game_ak_yes, Seat::South);
    assert!(ak_yes_view
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: tile("5m") }));
    assert_eq!(
        TileEfficiencyBot::decide(&ak_yes_view, &mut rng),
        PlayerAction::AddedKong { tile: tile("5m") }
    );

    // 2b. AddedKong skipped when 4th tile forms 456m sequence (discarding N gives shanten 0, AddedKong gives 1).
    let mut game_ak_no = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 5m C"),
                &tiles("55m 46m 123p 45p 88s N B"),
                &tiles("333m 666p 333s SSS F"),
                &tiles("777m 777p 777s WWW 1p"),
                &tiles("1m 2m 1s 5m"),
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_ak_no
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_ak_no
        .apply_action(Seat::South, PlayerAction::Pon { tile: tile("5m") })
        .unwrap();
    game_ak_no
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_ak_no
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_ak_no
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_ak_no
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: true,
            },
        )
        .unwrap();
    let ak_no_view = project_view(&game_ak_no, Seat::South);
    assert!(ak_no_view
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: tile("5m") }));
    assert_eq!(
        TileEfficiencyBot::decide(&ak_no_view, &mut rng),
        PlayerAction::Discard {
            tile: tile("N"),
            from_drawn: false
        }
    );

    // 3a. ExposedKong taken when it keeps shanten <= 1 (0 -> 0).
    let mut game_ek_yes = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m B"),
                &tiles("111m 234p 567p 56s 55z"),
                &tiles("888m 888p 888s SSS F"),
                &tiles("777m 777p 777s WWW 4m"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_ek_yes
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let ek_yes_view = project_view(&game_ek_yes, Seat::South);
    assert!(ek_yes_view
        .legal_actions
        .contains(&PlayerAction::ExposedKong { tile: tile("1m") }));
    assert_eq!(
        TileEfficiencyBot::decide(&ek_yes_view, &mut rng),
        PlayerAction::ExposedKong { tile: tile("1m") }
    );

    // 3b. ExposedKong skipped (Pass) when it worsens shanten (0 -> 1) or stalls at shanten 2.
    let mut game_ek_no_break = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m C"),
                &tiles("11123m 456p 789p 12s"),
                &tiles("888m 888p 888s SSS F"),
                &tiles("777m 777p 777s WWW B"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_ek_no_break
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game_ek_no_break, Seat::South), &mut rng),
        PlayerAction::Pass
    );

    let mut game_ek_no_sh2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m C"),
                &tiles("111m 24p 68p 13s 57s N F"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW 4m"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_ek_no_sh2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game_ek_no_sh2, Seat::South), &mut rng),
        PlayerAction::Pass
    );

    // 4a. Pon taken when post-Pon discard strictly reduces shanten (1 -> 0).
    let mut game_pon_yes = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m C"),
                &tiles("11m 234p 567p 89s 55z N"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW F"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_pon_yes
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let pon_yes_view = project_view(&game_pon_yes, Seat::South);
    assert!(pon_yes_view
        .legal_actions
        .contains(&PlayerAction::Pon { tile: tile("1m") }));
    assert_eq!(
        TileEfficiencyBot::decide(&pon_yes_view, &mut rng),
        PlayerAction::Pon { tile: tile("1m") }
    );

    // 4b. Pon skipped (Pass) when already tenpai (0 -> 0) or Seven Pairs tenpai.
    let mut game_pon_no = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m B"),
                &tiles("1134m 456p 789s 222p"),
                &tiles("888m 888p 888s SSS F"),
                &tiles("777m 777p 777s WWW 4m"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_pon_no
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game_pon_no, Seat::South), &mut rng),
        PlayerAction::Pass
    );

    let mut game_pon_no_7p = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("999m 999p 999s EEE 1m C"),
                &tiles("11m 22p 33p 44p 55s 66s 7s"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW F"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    game_pon_no_7p
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        TileEfficiencyBot::decide(&project_view(&game_pon_no_7p, Seat::South), &mut rng),
        PlayerAction::Pass
    );
}

#[derive(Debug, Default)]
struct MatchRunTrace {
    actions_taken: Vec<(Seat, PlayerAction)>,
    hands_settled: usize,
    wins: usize,
    draw_games: usize,
    pons: usize,
    kongs: usize,
    summary: Option<MatchSummary>,
}

fn run_bot_match(
    mode: MatchMode,
    wall_seed: u64,
    bot_seed: u64,
    contexts: [DecisionContext; 4],
    max_hands_for_free_table: Option<u32>,
) -> MatchRunTrace {
    let mut game = Match::new(mode, TuidaohuRuleset, SeededWallProvider::new(wall_seed))
        .expect("seeded match initializes");
    let mut seat_rngs: [ChaCha8Rng; 4] = std::array::from_fn(|idx| {
        ChaCha8Rng::seed_from_u64(bot_seed ^ ((idx as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)))
    });
    let mut trace = MatchRunTrace::default();

    loop {
        let views = Seat::ALL.map(|seat| project_view(&game, seat));
        if views[0].phase == PublicPhase::MatchFinished {
            assert!(views.iter().all(|v| v.legal_actions.is_empty()));
            trace.summary = views[0].match_summary;
            break;
        }

        if mode == MatchMode::FreeTable {
            if let Some(limit) = max_hands_for_free_table {
                if views[0].phase == PublicPhase::BetweenHands
                    && trace.hands_settled as u32 >= limit
                    && views[0].legal_actions.contains(&PlayerAction::EndMatch)
                {
                    let recorded = game
                        .apply_action(Seat::East, PlayerAction::EndMatch)
                        .expect("EndMatch succeeds");
                    for ev in &recorded {
                        if let PlayerEventKind::MatchFinished { summary } =
                            project_event(ev, Seat::East).kind
                        {
                            trace.summary = Some(summary);
                        }
                    }
                    break;
                }
            }
        }

        let mut progressed = false;
        for (idx, view) in views.iter().enumerate() {
            if view.legal_actions.is_empty() {
                continue;
            }
            if view.phase == PublicPhase::BetweenHands
                && !view.legal_actions.contains(&PlayerAction::ConfirmNextHand)
            {
                continue;
            }
            let seat = view.viewer;
            let action = decide(view, &contexts[idx], &mut seat_rngs[idx]);
            assert!(
                view.legal_actions.contains(&action),
                "seat {seat:?} returned illegal action {action:?} not in {:?}",
                view.legal_actions
            );

            // Iron Rule 1 check across all real play.
            let has_win = view.legal_actions.iter().any(|a| {
                matches!(
                    a,
                    PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. }
                )
            });
            if has_win {
                assert!(
                    matches!(
                        action,
                        PlayerAction::Tsumo
                            | PlayerAction::Ron { .. }
                            | PlayerAction::RobKongHu { .. }
                    ),
                    "bot missed legal win: chose {action:?} out of {:?}",
                    view.legal_actions
                );
            }

            // Iron Rule 2 check across all real play.
            if let PlayerAction::Discard { tile, from_drawn } = action {
                if view.hand.drawn_tile == Some(tile)
                    && view.legal_actions.contains(&PlayerAction::Discard {
                        tile,
                        from_drawn: true,
                    })
                {
                    assert!(
                        from_drawn,
                        "bot must default to 摸切 when discarding drawn_tile {tile}"
                    );
                }
            }

            // BetweenHands check: never auto-pick EndMatch.
            if view.phase == PublicPhase::BetweenHands {
                assert_eq!(action, PlayerAction::ConfirmNextHand);
            }

            let recorded = game
                .apply_action(seat, action)
                .expect("legal AI action applies cleanly");
            trace.actions_taken.push((seat, action));
            for ev in &recorded {
                match project_event(ev, Seat::East).kind {
                    PlayerEventKind::MeldDeclared { meld, .. } => match meld {
                        mahjong_core::MeldView::Pon { .. } => trace.pons += 1,
                        mahjong_core::MeldView::ExposedKong { .. }
                        | mahjong_core::MeldView::AddedKong { .. }
                        | mahjong_core::MeldView::ConcealedKong { .. } => trace.kongs += 1,
                        mahjong_core::MeldView::Chi { .. } => {}
                    },
                    PlayerEventKind::HandSettled { settlement } => {
                        trace.hands_settled += 1;
                        match settlement.outcome {
                            HandOutcome::Win { .. } => trace.wins += 1,
                            HandOutcome::DrawGame => trace.draw_games += 1,
                        }
                    }
                    _ => {}
                }
            }
            progressed = true;
            break;
        }
        assert!(
            progressed,
            "deadlock: no seat had legal actions before MatchFinished"
        );
    }

    trace
}

#[test]
fn multi_hand_east_only_and_free_table_deterministic_zero_panic() {
    let det_budget = DecisionBudget::Deterministic { max_samples: 16 };
    let varied_contexts = [
        DecisionContext {
            difficulty: AiDifficulty::Strong,
            style: AiStyle::Balanced,
            budget: det_budget,
        },
        DecisionContext {
            difficulty: AiDifficulty::Intermediate,
            style: AiStyle::AggressiveFast,
            budget: DecisionBudget::Realtime {
                max_samples: 16,
                soft_ms: 120,
                hard_ms: 180,
            },
        },
        DecisionContext {
            difficulty: AiDifficulty::Strong,
            style: AiStyle::DefensiveBigHand,
            budget: det_budget,
        },
        DecisionContext {
            difficulty: AiDifficulty::Strong,
            style: AiStyle::Balanced,
            budget: det_budget,
        },
    ];

    let mut total_hands = 0usize;
    let mut total_wins = 0usize;
    let mut total_draws = 0usize;
    let mut total_pons = 0usize;

    // 25 full EastOnly matches (~100+ hands).
    for seed in 100..125u64 {
        let trace = run_bot_match(MatchMode::EastOnly, seed, seed * 31, varied_contexts, None);
        let summary = trace.summary.expect("EastOnly reaches MatchFinished");
        assert!(summary.hands_played >= 4);
        assert_eq!(summary.final_scores.iter().sum::<i32>(), 400);
        assert_eq!(trace.hands_settled as u32, summary.hands_played);

        total_hands += trace.hands_settled;
        total_wins += trace.wins;
        total_draws += trace.draw_games;
        total_pons += trace.pons;
    }

    // Same-seed determinism check (exact action-by-action & summary parity under Deterministic budget).
    let det_budget = DecisionBudget::Deterministic { max_samples: 16 };
    let det_varied_contexts = varied_contexts.map(|mut c| {
        c.budget = det_budget;
        c
    });
    let replay_a = run_bot_match(
        MatchMode::EastOnly,
        107,
        107 * 31,
        det_varied_contexts,
        None,
    );
    let replay_b = run_bot_match(
        MatchMode::EastOnly,
        107,
        107 * 31,
        det_varied_contexts,
        None,
    );
    assert_eq!(replay_a.actions_taken, replay_b.actions_taken);
    assert_eq!(replay_a.summary, replay_b.summary);

    // FreeTable multi-hand progression (> 4 hands across wind rotation) ending cleanly.
    let free_det_ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: det_budget,
    };
    let free_a = run_bot_match(MatchMode::FreeTable, 777, 999, [free_det_ctx; 4], Some(6));
    let free_b = run_bot_match(MatchMode::FreeTable, 777, 999, [free_det_ctx; 4], Some(6));
    assert_eq!(free_a.actions_taken, free_b.actions_taken);
    let free_summary = free_a.summary.expect("FreeTable finishes on EndMatch");
    assert_eq!(free_summary.hands_played, 6);
    assert_eq!(free_summary.final_scores.iter().sum::<i32>(), 0);

    assert!(total_hands >= 100, "played {total_hands} hands");
    assert!(total_wins > 0, "expected real AI wins across 100+ hands");
    assert!(
        total_draws > 0 || total_wins > 20,
        "expected varied hand outcomes"
    );
    assert!(total_pons > 0, "expected Pon reactions across 100+ hands");
}

#[test]
fn symmetric_rng_tiebreak_and_hidden_opponent_concealed_kong_handling() {
    let ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 16 },
    };

    // 1. Symmetric tie-break between isolated W and N uses RNG deterministically.
    let tie_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("123m 456m EE SS 55p W N"),
                &tiles("999m 999p 999s CCC F"),
                &tiles("888m 888p 888s BBB 1p"),
                &tiles("777m 777p 777s 234p 6p"),
                &[],
                &[],
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    let tie_view = project_view(&tie_game, Seat::East);
    let mut saw_w = false;
    let mut saw_n = false;
    for seed in 0..32u64 {
        let mut r1 = ChaCha8Rng::seed_from_u64(seed);
        let mut r2 = ChaCha8Rng::seed_from_u64(seed);
        let a1 = decide(&tie_view, &ctx, &mut r1);
        let a2 = decide(&tie_view, &ctx, &mut r2);
        assert_eq!(a1, a2, "same seed must be deterministic");
        if a1
            == (PlayerAction::Discard {
                tile: tile("W"),
                from_drawn: false,
            })
        {
            saw_w = true;
        } else if a1
            == (PlayerAction::Discard {
                tile: tile("N"),
                from_drawn: false,
            })
        {
            saw_n = true;
        } else {
            panic!("unexpected discard in W/N tie: {a1:?}");
        }
    }
    assert!(
        saw_w && saw_n,
        "expected RNG tie-break to visit both W and N"
    );

    // 2. Opponent ConcealedKong { tile: None } in PlayerView is handled cleanly.
    let mut kong_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(
            Seat::East,
            build_preset_wall(
                &tiles("1111m 234m 567p 89s 55z"),
                &tiles("999m 999p 999s EEE F"),
                &tiles("888m 888p 888s SSS B"),
                &tiles("777m 777p 777s WWW N"),
                &tiles("2p"),
                &tiles("3p"),
            )
            .expect("valid wall"),
        ),
    )
    .expect("match starts");
    kong_game
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: tile("1m") })
        .expect("East declares concealed kong");
    kong_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .expect("East discards replacement 3p");
    let south_view = project_view(&kong_game, Seat::South);
    assert!(matches!(
        south_view.seats[Seat::East.index()].melds.as_slice(),
        [mahjong_core::MeldView::ConcealedKong { tile: None, .. }]
    ));
    let mut rng = ChaCha8Rng::seed_from_u64(55);
    let south_action = decide(&south_view, &ctx, &mut rng);
    assert!(south_view.legal_actions.contains(&south_action));
}

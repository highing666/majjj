mod common;

use common::{tile, tiles};
use mahjong_ai::l2::{self, RobKongRiskModel};
use mahjong_ai::l3::{is_passed_hu_zero_risk, kabe_wait_combos, L3DefenseModel, L3Profile};
use mahjong_ai::{
    decide, AiDifficulty, AiStyle, DecisionBudget, DecisionContext, TileEfficiencyBot,
};
use mahjong_core::{
    build_preset_wall, project_view, Match, MatchMode, PlayerAction, PresetWallProvider, Seat,
    Tile, TuidaohuRuleset,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

fn strong_no_l4_ctx() -> DecisionContext {
    DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 64 },
    }
}

#[test]
fn passed_hu_zero_risk_window_protects_discard_and_expires_after_draw_or_meld() {
    let mut rng = ChaCha8Rng::seed_from_u64(3501);
    let ctx = strong_no_l4_ctx();

    // Draw-expiry path:
    // head_draws after C/F/4p: let's trace exact turn order cleanly!
    // Rebuild explicitly with exact turn trace:
    // Turn order after West discards 7s:
    //   North draws head[4]=4p, discards 4p
    //   East draws head[5]=3p, discards 3p
    //   South draws head[6]=8m? Wait we put 5m at head[6] above — let's adjust head_draws inside test!
    let wall = build_preset_wall(
        &tiles("123s 456s 78s EE 22p 5m 1m"),
        &tiles("11p 444m 777m 999s 9m B"),
        &tiles("11m 99m CC 67m 88p 9p 4s 7s"),
        &tiles("222m 333m 444p 111s C"),
        // 0:North(8s), 1:East(6s), 2:South(W), 3:North(F),
        // 4:North(4p), 5:East(3p), 6:South(3s), 7:West(7p),
        // 8:North(5m -> West passes Ron!), 9:East(5p),
        // 10:South(N), 11:West(9p -> West draw expires 过水胡!), 12:North(2s), 13:East(5m)
        &tiles("8s 6s W F 4p 3p 3s 7p 5m 5p N 9p 2s 5m"),
        &[],
    )
    .expect("valid preset wall");

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");

    // West completes 3 melds (1m, 9m, C) -> tenpai on 58m with Man flush signal.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: tile("1m"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Pon { tile: tile("1m") })
        .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: tile("9p"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: tile("8s"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: tile("6s"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: tile("9m"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Pon { tile: tile("9m") })
        .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: tile("4s"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: tile("C"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Pon { tile: tile("C") })
        .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: tile("7s"),
            from_drawn: false,
        },
    )
    .unwrap();

    // Advance one quiet round so West shows a tsumogiri streak too.
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: tile("4p"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: tile("3p"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: tile("3s"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: tile("7p"),
            from_drawn: true,
        },
    )
    .unwrap();

    // Before North discards 5m: 5m is NOT in West's 过水胡 0-risk window.
    let pre_view = project_view(&game, Seat::East);
    assert!(!is_passed_hu_zero_risk(&pre_view, Seat::West, tile("5m")));

    // North draws and discards 5m; West has Ron(5m) and Passes -> 过水胡 0-risk window opens for West!
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Pass).unwrap();

    let safe_view = project_view(&game, Seat::East);
    assert!(
        is_passed_hu_zero_risk(&safe_view, Seat::West, tile("5m")),
        "North's 5m discard after West's last_draw_or_meld_seq must lock 0-risk for West"
    );
    assert!(
        !is_passed_hu_zero_risk(&safe_view, Seat::West, tile("5p")),
        "5p has not been discarded since West's last draw/meld"
    );
    // East holds 123s 456s 78s EE 22p 5m + drawn 5p: 5m is 0-risk against West & South, whereas 5p is live!
    assert_eq!(
        decide(&safe_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: false,
        },
        "AI must exploit 过水胡 0-risk window to discard 5m safely instead of live 5p"
    );

    // Advance so East discards 5m, South discards N, West draws 9p & discards 9p, North discards 2s,
    // and East draws another 5m. West's draw updates West.last_draw_or_meld_seq, expiring 0-risk!
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: tile("N"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: tile("9p"),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: tile("2s"),
            from_drawn: true,
        },
    )
    .unwrap();

    let expired_view = project_view(&game, Seat::East);
    assert!(
        !is_passed_hu_zero_risk(&expired_view, Seat::West, tile("5m")),
        "过水胡 0-risk window must automatically expire after West draws"
    );
    // Now 5m is live again into West's 3-meld Man flush while 5p is off-flush -> AI switches to 5p!
    assert_eq!(
        decide(&expired_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile("5p"),
            from_drawn: false,
        },
        "once West draws and 过水胡 expires, AI must switch from dangerous 5m to safer 5p"
    );

    // Also verify expiry immediately when West MELDS (Pon) instead of drawing!
    let meld_wall = build_preset_wall(
        &tiles("123s 456s 78s EE 22p 5m 1m"),
        &tiles("11p 444m 777m 999s 9m 6p"),
        &tiles("11m 99m CC 67m 66p 9p 4s 7s"),
        &tiles("222m 333m 444p 111s C"),
        // 0:North(8s), 1:East(6s), 2:South(W), 3:North(F),
        // 4:North(4p), 5:East(3p), 6:South(3s), 7:West(7p),
        // 8:North(5m -> West Pass), 9:East(5p), 10:South(N) -> South discards 6p -> West Pons 6p -> North(2s) -> East(5m)
        &tiles("8s 6s W F 4p 3p 3s 7p 5m 5p N 2s 5m"),
        &[],
    )
    .expect("valid preset wall");

    let mut meld_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, meld_wall),
    )
    .expect("match starts");
    meld_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("1m") })
        .unwrap();
    meld_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("9p"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("6s"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9m"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("9m") })
        .unwrap();
    meld_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("4s"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("C") })
        .unwrap();
    meld_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7s"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("4p"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("3s"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: true,
            },
        )
        .unwrap();
    meld_game
        .apply_action(Seat::West, PlayerAction::Pass)
        .unwrap();
    assert!(is_passed_hu_zero_risk(
        &project_view(&meld_game, Seat::East),
        Seat::West,
        tile("5m")
    ));

    meld_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    // South discards 6p -> West Pons 6p (updates West.last_draw_or_meld_seq without drawing!).
    meld_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("6p"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("6p") })
        .unwrap();
    meld_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("6m"),
                from_drawn: false,
            },
        )
        .unwrap();
    meld_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("2s"),
                from_drawn: true,
            },
        )
        .unwrap();

    assert!(
        !is_passed_hu_zero_risk(
            &project_view(&meld_game, Seat::East),
            Seat::West,
            tile("5m")
        ),
        "过水胡 0-risk window must also expire when West claims a Pon meld"
    );
}

#[test]
fn non_riichi_defense_prefers_kabe_over_live_middle_and_deters_dangerous_added_kong() {
    let mut rng = ChaCha8Rng::seed_from_u64(3502);
    let ctx = strong_no_l4_ctx();

    // 1. Kabe + flush + multi-meld + tedashi defense:
    //    East holds 1123p 456s 13s 78p 99s 5m (1-shanten keeping 1123p and discarding 5m).
    //    All four 2p (1 in East + 3 in rivers) and all four 1p (2 in East + 2 in rivers)
    //    make 1p completely Kabe-safe (`remaining[1p] == 0 && remaining[2p] == 0`),
    //    while West has 3 melds (1m, 9m, C) + Man flush + mid/late middle-tile 手切 (4s).
    //    TileEfficiencyBot & pure L2 discard live 5m for offense, while L3 folds Kabe-safe 1p!
    let wall1 = build_preset_wall(
        &tiles("1123p 456s 13s 78p 99s 1m"),
        &tiles("1p 2p 2p 9m 444m 777m 33s B"),
        &tiles("11m 99m CC 67m 88p 9p 4s 7s"),
        &tiles("1p 2p 222m 333m 444p 1s C"),
        // 0:North(8s), 1:East(6s), 2:South(W), 3:North(F),
        // 4:North(3p), 5:East(3p), 6:South(5p), 7:West(6p),
        // 8:North(N), 9:East(3p), 10:South(E), 11:West(9s),
        // 12:North(2s), 13:East(S), 14:South(5s), 15:West(7p),
        // 16:North(8p), 17:East(5m)
        &tiles("8s 6s W F 3p 3p 5p 6p N 3p E 8m 2s S 5s 7p 8p 5m"),
        &[],
    )
    .expect("valid kabe wall");

    let mut game1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall1),
    )
    .expect("match starts");

    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("1m") })
        .unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("9p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("6s"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("9m") })
        .unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("4s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("C") })
        .unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7s"),
                from_drawn: false,
            },
        )
        .unwrap();

    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("6p"),
                from_drawn: true,
            },
        )
        .unwrap();

    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("N"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("8m"),
                from_drawn: true,
            },
        )
        .unwrap();

    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        )
        .unwrap();
    game1.apply_action(Seat::East, PlayerAction::Pass).unwrap();
    game1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: true,
            },
        )
        .unwrap();
    game1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        )
        .unwrap();

    let kabe_view = project_view(&game1, Seat::East);
    assert_eq!(
        TileEfficiencyBot::decide(&kabe_view, &mut rng),
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true,
        },
        "L1 baseline blindly discards live 5m into Man flush"
    );
    assert_eq!(
        l2::decide(&kabe_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true,
        },
        "pure L2 offense discards live 5m into Man flush"
    );
    assert_eq!(
        decide(&kabe_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile("1p"),
            from_drawn: false,
        },
        "L3 defense must discard Kabe-safe 1p instead of deadly live 5m"
    );

    // 2. Number vs Honor when `remaining[tile] == 0`:
    //    For a number tile (5m with 3467m live), `remaining[5m] == 0` zeroes shanpon/tanki ONLY,
    //    keeping ryanmen/kanchan wait risk > 0; for an honor (E), `remaining[E] == 0` blocks all standard waits!
    let mut rem_check = [3u8; Tile::COUNT];
    rem_check[tile("5m").index()] = 0;
    rem_check[tile("E").index()] = 0;
    let combos_5m = kabe_wait_combos(tile("5m"), &rem_check);
    let combos_e = kabe_wait_combos(tile("E"), &rem_check);
    assert_eq!(combos_5m.shanpon, 0);
    assert_eq!(combos_5m.tanki, 0);
    assert!(combos_5m.ryanmen > 0 && combos_5m.kanchan > 0);
    assert_eq!(combos_e.total_std_combos(), 0);

    // 3. AddedKong `抢杠胡` deterrence:
    //    East has Pon(5m) + 123s 456s 7p 99s E + drawn 5m (3 E visible in rivers -> E is dead).
    //    West has 3 Man melds (1m, 9m, 2m) + 清一色 Man flush signals + 6 non-Man discards in mid-game.
    //    L1 & pure L2 take AddedKong(5m) (+3 kong delta), whereas Strong L3 deters AddedKong(5m)
    //    and sheds dead E, while Intermediate still takes AddedKong(5m).
    let ak_wall = build_preset_wall(
        &tiles("55m 123s 456s 7p 99s E B 1m"),
        &tiles("11p 444p 777p 66p 5m 9m 2m"),
        &tiles("11m 99m 22m 67m 88p E 4s 7s"),
        &tiles("333m 444m 555p 111s C"),
        &tiles("E 8s S W N F 3p C 2p 9p 3p B 8s E 2p 9p 8s 3p W S 5m"),
        &[],
    )
    .expect("valid ak wall");

    let mut ak_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, ak_wall),
    )
    .expect("match starts");

    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("1m") })
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(Seat::East, PlayerAction::Pon { tile: tile("5m") })
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9m"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("9m") })
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("4s"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("N"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("F"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("2m"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("2m") })
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7s"),
                from_drawn: false,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("9p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("W"),
                from_drawn: true,
            },
        )
        .unwrap();
    ak_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        )
        .unwrap();

    let ak_view = project_view(&ak_game, Seat::East);
    assert!(ak_view
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: tile("5m") }));

    // Confirm L3 model assigns 0 wait to West (melded) and near-0 total risk to dead honor E,
    // while assigning high risk to 5m AddedKong.
    let model = L3DefenseModel::new(&ak_view, L3Profile::from_context(&ctx, false));
    assert!(model.discard_risk(tile("E")) < 1e-3);
    assert!(model.rob_kong_risk_ev(&ak_view, tile("5m")) > 2.5);

    assert_eq!(
        TileEfficiencyBot::decide(&ak_view, &mut rng),
        PlayerAction::AddedKong { tile: tile("5m") }
    );
    assert_eq!(
        l2::decide(&ak_view, &ctx, &mut rng),
        PlayerAction::AddedKong { tile: tile("5m") }
    );
    assert_eq!(
        decide(&ak_view, &ctx, &mut rng),
        PlayerAction::Discard {
            tile: tile("E"),
            from_drawn: false,
        },
        "L3 must pass on dangerous AddedKong(5m) into West's Man flush and discard dead E"
    );

    // Confirm Intermediate (lower L3 defense weight) behaves differently from Strong here:
    // Intermediate still accepts AddedKong(5m) whereas Strong deters it.
    let inter_ctx = DecisionContext {
        difficulty: AiDifficulty::Intermediate,
        style: AiStyle::Balanced,
        budget: DecisionBudget::Deterministic { max_samples: 64 },
    };
    assert_eq!(
        decide(&ak_view, &inter_ctx, &mut rng),
        PlayerAction::AddedKong { tile: tile("5m") },
        "Intermediate uses moderate L3 defense weight and takes AddedKong(5m)"
    );
}

#[test]
fn difficulty_and_style_profiles_produce_distinct_decisions() {
    let mut rng = ChaCha8Rng::seed_from_u64(3503);
    let budget = DecisionBudget::Deterministic { max_samples: 64 };

    // 1. Speed vs Meld/Defense discipline across AiStyle (`AggressiveFast` vs `Balanced` / `DefensiveBigHand`):
    //    East holds 11m 23m 56m 78p 22s 67s (13 tiles, 2-shanten closed with multi-ryanmen 门前清 value).
    //    West has 2 melds (1s, 9s) + mid-game river, North discards 1m.
    //    AggressiveFast claims Pon(1m) for immediate 1-shanten speed, whereas Balanced and DefensiveBigHand
    //    pass Pon(1m) to preserve 门前清 value and hand safety.
    let pon_wall = build_preset_wall(
        &tiles("11m 23m 56m 78p 22s 67s 9m C"),
        &tiles("WW 444m 999p 666m 1s 9s"),
        &tiles("11s 99s EE 23s 44p 2p 7p 8p"),
        &tiles("666s 333m 888m 555s 1m"),
        &tiles("N B S F E"),
        &[],
    )
    .expect("valid style pon wall");
    let mut pon_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, pon_wall),
    )
    .expect("match starts");
    pon_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        )
        .unwrap();
    pon_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: false,
            },
        )
        .unwrap();
    pon_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("1s") })
        .unwrap();
    pon_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("2p"),
                from_drawn: false,
            },
        )
        .unwrap();
    pon_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    pon_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        )
        .unwrap();
    pon_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("9s"),
                from_drawn: false,
            },
        )
        .unwrap();
    pon_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("9s") })
        .unwrap();
    pon_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: false,
            },
        )
        .unwrap();
    pon_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    let pon_view = project_view(&pon_game, Seat::East);
    assert!(pon_view
        .legal_actions
        .contains(&PlayerAction::Pon { tile: tile("1m") }));
    let agg_ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::AggressiveFast,
        budget,
    };
    let bal_ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::Balanced,
        budget,
    };
    let def_ctx = DecisionContext {
        difficulty: AiDifficulty::Strong,
        style: AiStyle::DefensiveBigHand,
        budget,
    };
    let agg_pon = decide(&pon_view, &agg_ctx, &mut rng);
    let bal_pon = decide(&pon_view, &bal_ctx, &mut rng);
    let def_pon = decide(&pon_view, &def_ctx, &mut rng);
    assert_eq!(
        agg_pon,
        PlayerAction::Pon { tile: tile("1m") },
        "AggressiveFast should claim Pon(1m) for fast 1-shanten speed"
    );
    assert_eq!(
        bal_pon,
        PlayerAction::Pass,
        "Balanced should pass Pon(1m) at 2-shanten to preserve 门前清 value"
    );
    assert_eq!(
        def_pon,
        PlayerAction::Pass,
        "DefensiveBigHand should pass Pon(1m) to preserve 门前清 and hand safety"
    );

    // 2. Offense vs Defense risk threshold separating Balanced vs DefensiveBigHand (and AggressiveFast):
    //    East holds 999s 456s 122p 68p 11s + drawn 5m (discarding 5m keeps full-ukeire 1-shanten;
    //    discarding dead 0-risk Kabe tile 1p sacrifices 3p acceptance for safety).
    //    West has 1 Man meld (1m) + Man flush signals.
    let border_wall = build_preset_wall(
        &tiles("999s 456s 122p 68p 11s 1m"),
        &tiles("11p 8p 9m 1s 555p 66p 33s B"),
        &tiles("11m 99m CC 67m 77p 4s 7s 5s"),
        &tiles("1p 333m 444p 1s 2s 8s F 7m 3m"),
        &tiles("B 6s W N 4s 3p 5s 3p 9p 7p 4p 9p 9p W S S 4m 5m"),
        &[],
    )
    .expect("valid border wall");
    let mut border_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, border_wall),
    )
    .expect("match starts");
    border_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(Seat::West, PlayerAction::Pon { tile: tile("1m") })
        .unwrap();
    border_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("6s"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("8p"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("4s"),
                from_drawn: false,
            },
        )
        .unwrap();

    border_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("F"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(Seat::East, PlayerAction::Pass)
        .unwrap();
    border_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("3p"),
                from_drawn: true,
            },
        )
        .unwrap();

    border_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("8s"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("7p"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("7s"),
                from_drawn: false,
            },
        )
        .unwrap();

    border_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: tile("W"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: tile("S"),
                from_drawn: true,
            },
        )
        .unwrap();
    border_game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: tile("4m"),
                from_drawn: true,
            },
        )
        .unwrap();

    let border_view = project_view(&border_game, Seat::East);
    let agg_b = mahjong_ai::l3::decide_with_options(&border_view, &agg_ctx, false, &mut rng);
    let bal_b = mahjong_ai::l3::decide_with_options(&border_view, &bal_ctx, false, &mut rng);
    let def_b = mahjong_ai::l3::decide_with_options(&border_view, &def_ctx, false, &mut rng);
    assert_eq!(
        agg_b,
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true
        }
    );
    assert_eq!(
        bal_b,
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true
        },
        "Balanced pushes 5m to preserve 1-shanten acceptance against 3-meld threat"
    );
    assert_eq!(
        def_b,
        PlayerAction::Discard {
            tile: tile("1p"),
            from_drawn: false
        },
        "DefensiveBigHand folds dead 0-risk Kabe tile 1p rather than risking Man flush 5m"
    );

    // Also verify EvalStrategy::Intermediate vs StrongNoL4 wiring via eval::EvalStrategy::decide.
    use mahjong_ai::eval::EvalStrategy;
    let strong_no_l4_b = EvalStrategy::StrongNoL4.decide(&border_view, budget, &mut rng);
    assert_eq!(strong_no_l4_b, bal_b);
    assert_eq!(
        EvalStrategy::Intermediate.decide(&border_view, budget, &mut rng),
        PlayerAction::Discard {
            tile: tile("5m"),
            from_drawn: true
        }
    );
}

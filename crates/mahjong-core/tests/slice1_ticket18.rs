mod common;

use common::{assert_136_tile_conservation, tile as t, tiles};
use mahjong_core::{
    build_preset_wall, project_event, project_view, DiscardReactionContext, EvaluatedWin,
    HandOutcome, HandProgression, HandProgressionInput, HandSettlement, KongKind, Match,
    MatchError, MatchMode, MeldView, PlayerAction, PlayerEventKind, PlayerView, PresetWallProvider,
    PublicPhase, ReactionPriority, RevealedMeld, RobKongContext, RuleProfile, Ruleset, ScoreItem,
    Seat, SettleHandInput, Tile, TuidaohuRuleset, TurnContext, WinEvalInput, WinKind,
};

#[test]
fn slice1_precise_windowing_zero_leakage_and_all_pass_continuation() {
    let east_14 = tiles("19m 258s 147p ESWNCF");
    let south_plain = tiles("2468m 147s 258p ESB");
    let west_pon_1m = tiles("11357m 369s 369p WN");
    let north_plain = tiles("258m 159s 148p CFBN");

    // 1. Discard with no eligible reactions advances directly to next draw without opening a window.
    let direct_wall = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let mut direct_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, direct_wall),
    )
    .unwrap();
    let direct_events = direct_game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("9m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(direct_events.len(), 2);
    assert!(matches!(
        project_event(&direct_events[0], Seat::North).kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::East,
            tile,
            is_tsumogiri: false,
        } if tile == t("9m")
    ));
    assert!(matches!(
        project_event(&direct_events[1], Seat::South).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            tile: Some(t_drawn),
            is_replacement: false,
            wall_remaining: 82,
        } if t_drawn == t("B")
    ));

    // 2. Discard opening reaction window: compare two matches where West vs South holds Pon { 1m }
    // to verify zero leakage to unrelated seat North.
    let wall_west_pon = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let wall_south_pon = build_preset_wall(
        &east_14,
        &west_pon_1m,
        &south_plain,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();

    let mut game_west = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_west_pon),
    )
    .unwrap();
    let mut game_south = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_south_pon),
    )
    .unwrap();

    let discard_west = game_west
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let discard_south = game_south
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    // No reaction-waiting event is emitted.
    assert_eq!(discard_west.len(), 1);
    assert_eq!(discard_south.len(), 1);
    assert_eq!(game_west.seq(), 3);
    assert_eq!(
        project_view(&game_west, Seat::North),
        project_view(&game_south, Seat::North)
    );

    // PublicPhase stays Playing and active_seat stays Some(East) for all seats.
    for seat in Seat::ALL {
        let view = project_view(&game_west, seat);
        assert_eq!(view.phase, PublicPhase::Playing);
        assert_eq!(view.turn.active_seat, Some(Seat::East));
        assert_eq!(view.wall_remaining, 83);
        if seat == Seat::West {
            assert_eq!(
                view.legal_actions,
                vec![PlayerAction::Pon { tile: t("1m") }, PlayerAction::Pass]
            );
        } else {
            assert!(view.legal_actions.is_empty());
        }
    }

    // Non-eligible seats get NotYourTurn without mutating state; eligible seat gets WrongPhase/IllegalAction.
    let before_views: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&game_west, s));
    for non_eligible in [Seat::East, Seat::South, Seat::North] {
        assert!(matches!(
            game_west
                .apply_action(non_eligible, PlayerAction::Pass)
                .unwrap_err(),
            MatchError::NotYourTurn { seat } if seat == non_eligible
        ));
    }
    assert!(matches!(
        game_west
            .apply_action(Seat::West, PlayerAction::ConfirmNextHand)
            .unwrap_err(),
        MatchError::WrongPhase {
            seat: Seat::West,
            ..
        }
    ));
    assert!(matches!(
        game_west
            .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
            .unwrap_err(),
        MatchError::IllegalAction {
            seat: Seat::West,
            ..
        }
    ));
    assert_eq!(Seat::ALL.map(|s| project_view(&game_west, s)), before_views);
    assert_eq!(game_west.seq(), 3);

    // 3. Two pending seats (West Pon, North Ron): first Pass emits nothing and hides legal_actions;
    // second Pass resolves to next seat (South) drawing from wall head.
    let north_ron_1m = tiles("23m 123s 456789p CC");
    let two_pending_wall = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_ron_1m,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let mut two_pending = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, two_pending_wall),
    )
    .unwrap();
    two_pending
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    assert_eq!(
        project_view(&two_pending, Seat::West).legal_actions,
        vec![PlayerAction::Pon { tile: t("1m") }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&two_pending, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );

    let south_before = project_view(&two_pending, Seat::South);
    let first_pass_events = two_pending
        .apply_action(Seat::West, PlayerAction::Pass)
        .unwrap();
    assert!(first_pass_events.is_empty());
    assert_eq!(two_pending.seq(), 3);
    assert_eq!(project_view(&two_pending, Seat::South), south_before);
    assert!(project_view(&two_pending, Seat::West)
        .legal_actions
        .is_empty());
    assert!(matches!(
        two_pending
            .apply_action(Seat::West, PlayerAction::Pass)
            .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::West }
    ));

    let second_pass_events = two_pending
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert_eq!(second_pass_events.len(), 1);
    assert!(matches!(
        project_event(&second_pass_events[0], Seat::South).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            tile: Some(t_drawn),
            is_replacement: false,
            wall_remaining: 82,
        } if t_drawn == t("B")
    ));
    let south_after = project_view(&two_pending, Seat::South);
    assert_eq!(south_after.turn.active_seat, Some(Seat::South));
    assert_eq!(south_after.hand.drawn_tile, Some(t("B")));
}

#[test]
fn slice2_pon_execution_state_event_after_meld_and_loses_menqing() {
    let east_14 = tiles("19m 458s 145p ESWNCF");
    let south_13 = tiles("5678m 679s 236p ESB");
    let west_13 = tiles("11234m 123s 789p WC");
    let north_13 = tiles("568m 459s 146p FBNS");
    // Head draws after West's Pon discard: North draws 2p, East draws 3p, South draws 8s, West draws C.
    let head_draws = tiles("23p 8s C");
    let wall =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &head_draws, &[]).unwrap();

    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    // East discards 1m (seq = 3).
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("1m"),
            from_drawn: false,
        },
    )
    .unwrap();

    // West claims Pon { 1m }.
    let pon_events = game
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("1m") })
        .unwrap();
    assert_eq!(pon_events.len(), 1);
    let meld_seq = pon_events[0].seq();
    assert_eq!(meld_seq, 4);

    for viewer in Seat::ALL {
        match project_event(&pon_events[0], viewer).kind {
            PlayerEventKind::MeldDeclared {
                seat,
                meld,
                kong_score_deltas,
            } => {
                assert_eq!(seat, Seat::West);
                assert_eq!(
                    meld,
                    MeldView::Pon {
                        tile: t("1m"),
                        from_seat: Seat::East,
                        seq: meld_seq,
                    }
                );
                assert_eq!(kong_score_deltas, None);
            }
            other => panic!("expected MeldDeclared, got {other:?}"),
        }
    }

    let west_view = project_view(&game, Seat::West);
    assert_eq!(west_view.phase, PublicPhase::Playing);
    assert_eq!(west_view.turn.active_seat, Some(Seat::West));
    assert_eq!(west_view.wall_remaining, 83);
    assert_eq!(west_view.hand.drawn_tile, None);
    assert_eq!(west_view.hand.closed_tiles, tiles("234m 123s 789p WC"));
    assert_eq!(west_view.seats[Seat::West.index()].hand_count, 11);
    assert_eq!(
        west_view.seats[Seat::West.index()].last_draw_or_meld_seq,
        meld_seq
    );
    assert_eq!(
        west_view.seats[Seat::West.index()].melds,
        vec![MeldView::Pon {
            tile: t("1m"),
            from_seat: Seat::East,
            seq: meld_seq,
        }]
    );
    assert_eq!(west_view.seats[Seat::East.index()].river.len(), 1);
    assert_eq!(
        west_view.seats[Seat::East.index()].river[0].claimed_by,
        Some(Seat::West)
    );
    assert_eq!(west_view.seats[Seat::East.index()].river[0].seq, 3);
    assert!(west_view.legal_actions.iter().all(|a| matches!(
        a,
        PlayerAction::Discard {
            from_drawn: false,
            ..
        }
    )));
    assert_eq!(west_view.legal_actions.len(), 11);
    assert_136_tile_conservation(&game);

    // West discards W in AfterMeld turn -> North draws 2p and discards it -> East -> South -> West draws C.
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: t("W"),
            from_drawn: false,
        },
    )
    .unwrap();
    for (seat, tile) in [
        (Seat::North, t("2p")),
        (Seat::East, t("3p")),
        (Seat::South, t("8s")),
    ] {
        game.apply_action(
            seat,
            PlayerAction::Discard {
                tile,
                from_drawn: true,
            },
        )
        .unwrap();
    }

    // West wins via Tsumo on C; Pon broke 门前清 so score is 基本胡 (0 fan, 1 base point).
    let win_events = game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    let settlement = match project_event(&win_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement.score_deltas, [-1, -1, 3, -1]);
    assert_eq!(
        settlement.revealed_hands[Seat::West.index()].melds,
        vec![RevealedMeld::Pon {
            tile: t("1m"),
            from_seat: Seat::East,
            seq: meld_seq,
        }]
    );
    match settlement.outcome {
        HandOutcome::Win { winners, .. } => {
            assert_eq!(winners.len(), 1);
            assert_eq!(winners[0].seat, Seat::West);
            assert_eq!(winners[0].win_kind, WinKind::Tsumo);
            assert_eq!(winners[0].total_fan, 0);
            assert_eq!(winners[0].base_points, 1);
            assert_eq!(
                winners[0].items,
                vec![ScoreItem {
                    code: "basic_hu".to_string(),
                    label: "基本胡".to_string(),
                    fan: 0,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }
}

#[test]
fn slice3_single_ron_and_hu_over_pon_early_resolution() {
    let east_14 = tiles("19m 458s 145p ESWNCF");
    let south_pon_1m = tiles("11567m 679s 236p EB");
    let west_ron_1m = tiles("23m 123s 778899p CC");
    let north_plain = tiles("468m 459s 146p FBNS");

    // Sub-case A: West submits Ron immediately -> preempts South (Pon-only) with early resolution.
    let wall_a = build_preset_wall(
        &east_14,
        &south_pon_1m,
        &west_ron_1m,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let mut game_a = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_a),
    )
    .unwrap();

    game_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    assert_eq!(
        project_view(&game_a, Seat::South).legal_actions,
        vec![PlayerAction::Pon { tile: t("1m") }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&game_a, Seat::West).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );

    // West wins immediately without waiting for South.
    let ron_events = game_a
        .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert_eq!(ron_events.len(), 1);
    let settlement = match project_event(&ron_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement.score_deltas, [-2, 0, 2, 0]);
    assert_eq!(settlement.new_scores, [98, 100, 102, 100]);
    match &settlement.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, Some(Seat::East));
            assert_eq!(*winning_tile, t("1m"));
            assert_eq!(winners.len(), 1);
            assert_eq!(winners[0].seat, Seat::West);
            assert_eq!(winners[0].win_kind, WinKind::Ron);
            assert_eq!(winners[0].total_fan, 1);
            assert_eq!(winners[0].base_points, 2);
            assert_eq!(
                winners[0].items,
                vec![ScoreItem {
                    code: "menqing".to_string(),
                    label: "门前清".to_string(),
                    fan: 1,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }
    let after_a = project_view(&game_a, Seat::East);
    assert_eq!(after_a.phase, PublicPhase::BetweenHands);
    // Ron does NOT mark DiscardEntry.claimed_by.
    assert_eq!(after_a.seats[Seat::East.index()].river[0].claimed_by, None);

    // Sub-case B: South submits Pon first -> window stays open for West's Ron -> West submits Ron -> Hu beats Pon.
    let wall_b = build_preset_wall(
        &east_14,
        &south_pon_1m,
        &west_ron_1m,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let mut game_b = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b),
    )
    .unwrap();
    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    let after_south_pon = game_b
        .apply_action(Seat::South, PlayerAction::Pon { tile: t("1m") })
        .unwrap();
    assert!(after_south_pon.is_empty());
    assert!(project_view(&game_b, Seat::South).legal_actions.is_empty());
    assert_eq!(
        project_view(&game_b, Seat::West).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );

    let after_west_ron = game_b
        .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert_eq!(after_west_ron.len(), 1);
    match project_event(&after_west_ron[0], Seat::South).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(settlement.score_deltas, [-2, 0, 2, 0]);
            assert!(settlement.revealed_hands[Seat::South.index()]
                .melds
                .is_empty());
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // Sub-case C: South submits Pon first -> West submits Pass -> South's Pon resolves.
    let wall_c = build_preset_wall(
        &east_14,
        &south_pon_1m,
        &west_ron_1m,
        &north_plain,
        &tiles("B"),
        &[],
    )
    .unwrap();
    let mut game_c = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_c),
    )
    .unwrap();
    game_c
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_c
        .apply_action(Seat::South, PlayerAction::Pon { tile: t("1m") })
        .unwrap()
        .is_empty());
    let pass_resolves_pon = game_c.apply_action(Seat::West, PlayerAction::Pass).unwrap();
    assert_eq!(pass_resolves_pon.len(), 1);
    assert!(matches!(
        project_event(&pass_resolves_pon[0], Seat::East).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::South,
            meld: MeldView::Pon {
                tile,
                from_seat: Seat::East,
                ..
            },
            ..
        } if tile == t("1m")
    ));
    assert_eq!(
        project_view(&game_c, Seat::South).turn.active_seat,
        Some(Seat::South)
    );
}

#[test]
fn slice4_multi_ron_two_and_three_winners_with_dealer_streak() {
    // Part A: 2 Ron winners (West 1-fan, North 2-fan) + 1 Pon-only seat (South).
    let east_14 = tiles("19m 258s 15p ESWNCFB");
    let south_pon = tiles("11567m 679s 236p EB");
    let west_ron_1fan = tiles("23m 123s 778899p CC");
    let north_ron_2fan = tiles("23444999m NNN FF");
    let wall_a = build_preset_wall(
        &east_14,
        &south_pon,
        &west_ron_1fan,
        &north_ron_2fan,
        &tiles("4p"),
        &[],
    )
    .unwrap();

    let mut game_a = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_a.clone()),
    )
    .unwrap();
    game_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    // West submits Ron first -> MUST wait for North (also offered Ron), while South is Pon-only.
    let after_west = game_a
        .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert!(after_west.is_empty());
    assert_eq!(
        project_view(&game_a, Seat::West).phase,
        PublicPhase::Playing
    );
    assert!(project_view(&game_a, Seat::West).legal_actions.is_empty());
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&game_a, Seat::South).legal_actions,
        vec![PlayerAction::Pon { tile: t("1m") }, PlayerAction::Pass]
    );

    // North submits Ron -> resolves IMMEDIATELY without waiting for Pon-only South.
    let after_north = game_a
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert_eq!(after_north.len(), 1);
    let settlement_a = match project_event(&after_north[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    // West: 1 fan (2 pts), North: 混一色 + 门前清 = 2 fan (4 pts). East pays both independently.
    assert_eq!(settlement_a.score_deltas, [-6, 0, 2, 4]);
    assert_eq!(settlement_a.new_scores, [94, 100, 102, 104]);
    match &settlement_a.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, Some(Seat::East));
            assert_eq!(*winning_tile, t("1m"));
            assert_eq!(winners.len(), 2);
            assert_eq!(winners[0].seat, Seat::West);
            assert_eq!(winners[0].total_fan, 1);
            assert_eq!(winners[0].base_points, 2);
            assert_eq!(winners[1].seat, Seat::North);
            assert_eq!(winners[1].total_fan, 2);
            assert_eq!(winners[1].base_points, 4);
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // Also test West Ron + North Pass -> resolves immediately for West without waiting for South.
    let mut game_a2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_a),
    )
    .unwrap();
    game_a2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_a2
        .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
        .unwrap()
        .is_empty());
    let settled_a2 = game_a2
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert_eq!(settled_a2.len(), 1);
    match project_event(&settled_a2[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(settlement.score_deltas, [-2, 0, 2, 0]);
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // Part B: 3 winners (一炮多响) submitted out of order + dealer 连庄.
    let b_east_14 = tiles("23m 123s 123p EEE CCB");
    let b_south_13 = tiles("1456m 456s 456p SSS");
    let b_west_13 = tiles("1789m 789s 789p WWW");
    let b_north_13 = tiles("1259m 269s 258p NFC");
    let wall_b1 = build_preset_wall(
        &b_east_14,
        &b_south_13,
        &b_west_13,
        &b_north_13,
        &tiles("BBB"),
        &[],
    )
    .unwrap();
    let wall_b2 = Tile::standard_deck_136();

    let mut game_b = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, vec![wall_b1, wall_b2]),
    )
    .unwrap();

    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();

    // All three non-discarders (East, South, West) are offered Ron { 1m }.
    for seat in [Seat::East, Seat::South, Seat::West] {
        assert_eq!(
            project_view(&game_b, seat).legal_actions,
            vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
        );
    }

    // Submit in non-turn order: West -> East -> South.
    assert!(game_b
        .apply_action(Seat::West, PlayerAction::Ron { tile: t("1m") })
        .unwrap()
        .is_empty());
    assert!(game_b
        .apply_action(Seat::East, PlayerAction::Ron { tile: t("1m") })
        .unwrap()
        .is_empty());
    let settled_3 = game_b
        .apply_action(Seat::South, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert_eq!(settled_3.len(), 1);

    let settlement_b = match project_event(&settled_3[0], Seat::North).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement_b.score_deltas, [2, 2, 2, -6]);
    assert_eq!(settlement_b.new_scores, [102, 102, 102, 94]);
    match &settlement_b.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, Some(Seat::North));
            assert_eq!(*winning_tile, t("1m"));
            let winner_seats: Vec<Seat> = winners.iter().map(|w| w.seat).collect();
            assert_eq!(winner_seats, vec![Seat::East, Seat::South, Seat::West]);
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // Confirm next hand: dealer (East) was among winners -> 连庄!
    for seat in Seat::ALL {
        game_b
            .apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
    let hand2_view = project_view(&game_b, Seat::East);
    assert_eq!(hand2_view.turn.hand_number, 1);
    assert_eq!(hand2_view.turn.dealer, Seat::East);
    assert_eq!(hand2_view.turn.dealer_streak, 1);
}

#[test]
fn slice5_last_discard_at_zero_wall_allows_last_tile_ron_forbids_pon_and_settles_draw_on_pass() {
    // East holds 4p*2 + B*2; 0 copies of 4p or B exist in the 83-tile wall.
    let east_14 = tiles("123m 123s 12344p BBN");
    let south_13 = tiles("468m 468s 468p SCBE");
    let west_13 = tiles("579m 579s 4579p WFC");
    let north_13 = tiles("579m 579s 579p NFSB");

    let wall_ron = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[]).unwrap();
    let mut game_ron = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_ron.clone()),
    )
    .unwrap();

    let play_to_last_discard = |game: &mut Match<TuidaohuRuleset, PresetWallProvider>| {
        // Discard 1 (East discards N), then 82 tsumogiri discards so North draws tile #83.
        game.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("N"),
                from_drawn: false,
            },
        )
        .unwrap();
        for _ in 1..83 {
            let probe = project_view(game, Seat::East);
            let active = probe.turn.active_seat.unwrap();
            let drawn = project_view(game, active).hand.drawn_tile.unwrap();
            game.apply_action(
                active,
                PlayerAction::Discard {
                    tile: drawn,
                    from_drawn: true,
                },
            )
            .unwrap();
        }
        let north_before_last = project_view(game, Seat::North);
        assert_eq!(north_before_last.turn.active_seat, Some(Seat::North));
        assert_eq!(north_before_last.wall_remaining, 0);

        game.apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: false,
            },
        )
        .unwrap()
    };

    let last_discard_events = play_to_last_discard(&mut game_ron);
    assert_eq!(last_discard_events.len(), 1);

    // East has 2 copies of B AND wins on B, but Pon is forbidden at wall_remaining == 0.
    let east_window = project_view(&game_ron, Seat::East);
    assert_eq!(east_window.phase, PublicPhase::Playing);
    assert_eq!(east_window.wall_remaining, 0);
    assert_eq!(
        east_window.legal_actions,
        vec![PlayerAction::Ron { tile: t("B") }, PlayerAction::Pass]
    );

    let win_events = game_ron
        .apply_action(Seat::East, PlayerAction::Ron { tile: t("B") })
        .unwrap();
    assert_eq!(win_events.len(), 1);
    let settlement = match project_event(&win_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement.score_deltas, [4, 0, 0, -4]);
    match &settlement.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, Some(Seat::North));
            assert_eq!(*winning_tile, t("B"));
            assert_eq!(winners[0].total_fan, 2);
            assert_eq!(winners[0].base_points, 4);
            assert!(winners[0].items.contains(&ScoreItem {
                code: "last_tile_ron".to_string(),
                label: "河底捞鱼".to_string(),
                fan: 1,
            }));
            assert!(winners[0].items.contains(&ScoreItem {
                code: "menqing".to_string(),
                label: "门前清".to_string(),
                fan: 1,
            }));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // Passing on the last discard at wall_remaining == 0 settles DrawGame.
    let mut game_pass = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_ron),
    )
    .unwrap();
    play_to_last_discard(&mut game_pass);
    let draw_events = game_pass
        .apply_action(Seat::East, PlayerAction::Pass)
        .unwrap();
    assert_eq!(draw_events.len(), 1);
    match project_event(&draw_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(settlement.outcome, HandOutcome::DrawGame);
            assert_eq!(settlement.score_deltas, [0; 4]);
        }
        other => panic!("expected HandSettled(DrawGame), got {other:?}"),
    }

    // If a seat only has 2 copies of the last discard (cannot Ron) at wall_remaining == 0,
    // no reaction window opens and DrawGame settles directly on the discard.
    let mut east_no_win = east_14.clone();
    east_no_win[0] = t("9m"); // break 123m so East has 2 B (Pon-only) and cannot Ron.
    let wall_pon_only_last =
        build_preset_wall(&east_no_win, &south_13, &west_13, &north_13, &[], &[]).unwrap();
    let mut game_pon_only = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_pon_only_last),
    )
    .unwrap();
    let direct_draw_events = play_to_last_discard(&mut game_pon_only);
    assert_eq!(direct_draw_events.len(), 2);
    assert!(matches!(
        project_event(&direct_draw_events[1], Seat::East).kind,
        PlayerEventKind::HandSettled {
            settlement: mahjong_core::HandSettlement {
                outcome: HandOutcome::DrawGame,
                ..
            }
        }
    ));
}

#[test]
fn slice6_passed_hu_lock_lifecycle() {
    // Sub-test A: North passes Ron on 1m -> blocked on South's 4m -> Pon on 7p allowed & clears 过水胡 -> Rons 2m.
    let east_14 = tiles("19m 458s 139p ESWNFB");
    let south_13 = tiles("5678m 679s 238p ESB");
    let west_13 = tiles("4578m 468s 129p WNF");
    let north_13 = tiles("23m 123s 45677p CCC");
    // South draws 4m, West draws 7p, East draws 2m.
    let wall_a = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("4m 7p 2m"),
        &[],
    )
    .unwrap();
    let mut game_a = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_a),
    )
    .unwrap();

    // 1. East discards 1m -> North passes Ron -> acquires 过水胡.
    game_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );
    let after_north_pass = game_a
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert_eq!(after_north_pass.len(), 1); // South draws 4m.

    // 2. South discards 4m (which would win for North) -> blocked by 过水胡, directly advances to West.
    let south_discard_4m = game_a
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("4m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(south_discard_4m.len(), 2);
    assert_eq!(
        project_view(&game_a, Seat::West).turn.active_seat,
        Some(Seat::West)
    );
    assert!(project_view(&game_a, Seat::North).legal_actions.is_empty());

    // 3. West discards 7p -> 过水胡 does NOT block Pon -> North claims Pon { 7p }.
    let west_discard_7p = game_a
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("7p"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_discard_7p.len(), 1);
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: t("7p") }, PlayerAction::Pass]
    );
    game_a
        .apply_action(Seat::North, PlayerAction::Pon { tile: t("7p") })
        .unwrap();

    // North discards 3m in AfterMeld, waiting on 2m.
    game_a
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("3m"),
                from_drawn: false,
            },
        )
        .unwrap();

    // 4. East draws and discards 2m -> Pon cleared 过水胡 so North can Ron 2m!
    let east_discard_2m = game_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_discard_2m.len(), 1);
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("2m") }, PlayerAction::Pass]
    );
    let win_events = game_a
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("2m") })
        .unwrap();
    assert!(matches!(
        project_event(&win_events[0], Seat::North).kind,
        PlayerEventKind::HandSettled { .. }
    ));

    // Sub-test B: 过水胡 does NOT block Tsumo, and own TileDrawn clears 过水胡 for future Ron.
    let south_waiting = north_13.clone();
    let north_filler = south_13.clone();
    // Case B1: South passes 1m, immediately draws 1m -> Tsumo allowed.
    let wall_b1 = build_preset_wall(
        &east_14,
        &south_waiting,
        &west_13,
        &north_filler,
        &tiles("1m"),
        &[],
    )
    .unwrap();
    let mut game_b1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b1),
    )
    .unwrap();
    game_b1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b1
        .apply_action(Seat::South, PlayerAction::Pass)
        .unwrap();
    let south_b1 = project_view(&game_b1, Seat::South);
    assert!(south_b1.legal_actions.contains(&PlayerAction::Tsumo));
    game_b1
        .apply_action(Seat::South, PlayerAction::Tsumo)
        .unwrap();

    // Case B2: South passes 1m, draws B (clears 过水胡), discards B, West discards 1m -> South can Ron!
    let wall_b2 = build_preset_wall(
        &east_14,
        &south_waiting,
        &west_13,
        &north_filler,
        &tiles("B 1m"),
        &[],
    )
    .unwrap();
    let mut game_b2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b2),
    )
    .unwrap();
    game_b2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b2
        .apply_action(Seat::South, PlayerAction::Pass)
        .unwrap();
    game_b2
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    let west_1m = game_b2
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_1m.len(), 1);
    assert_eq!(
        project_view(&game_b2, Seat::South).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );
    game_b2
        .apply_action(Seat::South, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
}

#[test]
fn slice6_passed_pon_lock_lifecycle() {
    let east_14 = tiles("9m 2456s 249p ESWNCF");
    let south_13 = tiles("1378m 135s 357p ESB");
    let west_13 = tiles("278m 146s 356p WNCF");
    let north_13 = tiles("456m 22789s 1188p W");

    // Wall 1:
    // - East discards 2s -> North passes Pon { 2s } (acquires 过水碰 for 2s).
    // - South draws B & discards B; West draws 8p -> North Pon 8p -> East draws 2s (North now tenpai on 2s & 1p!)
    // -> North is STILL NOT offered Pon { 2s } (Pon 8p did NOT clear 过水碰), AND IS offered Ron { 2s }!
    let wall_1 = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("B 8p 2s"),
        &[],
    )
    .unwrap();
    let mut game_1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_1),
    )
    .unwrap();

    game_1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: t("2s") }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();

    game_1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("B"),
                from_drawn: true,
            },
        )
        .unwrap();
    let west_8p = game_1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("8p"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_8p.len(), 1);
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: t("8p") }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Pon { tile: t("8p") })
        .unwrap();
    game_1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("W"),
                from_drawn: false,
            },
        )
        .unwrap();

    // East draws and discards 2s. North holds two 2s AND wins on 2s:
    // Pon { 8p } did NOT clear 过水碰 for 2s (so Pon { 2s } is forbidden), while Ron { 2s } IS allowed!
    let east_2s = game_1
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_2s.len(), 1);
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("2s") }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("2s") })
        .unwrap();

    // Wall 2: Verify same-round Pon { 2s } is blocked BEFORE North draws.
    let wall_2 =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &tiles("2s"), &[]).unwrap();
    let mut game_2 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_2),
    )
    .unwrap();

    game_2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_2
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();

    // South discards 2s before North draws -> blocked by 过水碰, directly advances to West.
    let south_2s = game_2
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(south_2s.len(), 2);
    assert_eq!(
        project_view(&game_2, Seat::West).turn.active_seat,
        Some(Seat::West)
    );

    // Wall 3: Verify TileDrawn for North clears 过水碰 so Pon { 2s } is offered again afterwards.
    let wall_3 = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("BBC 2s"),
        &[],
    )
    .unwrap();
    let mut game_3 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_3),
    )
    .unwrap();

    game_3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_3
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    for (seat, tile) in [
        (Seat::South, t("B")),
        (Seat::West, t("B")),
        (Seat::North, t("C")),
    ] {
        game_3
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile,
                    from_drawn: true,
                },
            )
            .unwrap();
    }

    // East draws and discards 2s -> North IS offered Pon { 2s } again!
    let east_2s_again = game_3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("2s"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_2s_again.len(), 1);
    assert_eq!(
        project_view(&game_3, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: t("2s") }, PlayerAction::Pass]
    );
    game_3
        .apply_action(Seat::North, PlayerAction::Pon { tile: t("2s") })
        .unwrap();
}

#[test]
fn slice6_dual_ron_and_pon_offer_pon_does_not_lock_while_pass_locks_both() {
    let east_14 = tiles("59m 458s 139p ESWNFB");
    let south_13 = tiles("1278m 679s 238p ESB");
    let west_13 = tiles("1278m 468s 129p WNF");
    let north_13 = tiles("4556m 123s 456p CCC");
    let wall =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &tiles("56m"), &[]).unwrap();

    // Case 1: Offered [Ron { 5m }, Pon { 5m }]; choosing Pon does NOT set 过水胡.
    let mut game_pon = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
    )
    .unwrap();
    game_pon
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_pon, Seat::North).legal_actions,
        vec![
            PlayerAction::Ron { tile: t("5m") },
            PlayerAction::Pon { tile: t("5m") },
            PlayerAction::Pass,
        ]
    );
    game_pon
        .apply_action(Seat::North, PlayerAction::Pon { tile: t("5m") })
        .unwrap();
    game_pon
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("4m"),
                from_drawn: false,
            },
        )
        .unwrap();
    // East draws 5m (head_draws[0]), discards 5m; South draws 6m (head_draws[1]), discards 6m.
    game_pon
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: true,
            },
        )
        .unwrap();
    game_pon
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("6m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_pon, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("6m") }, PlayerAction::Pass]
    );
    game_pon
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("6m") })
        .unwrap();

    // Case 2: Offered [Ron { 5m }, Pon { 5m }]; choosing Pass locks BOTH 过水胡 and 过水碰 for 5m.
    let mut game_pass = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();
    game_pass
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_pass
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    // South draws 5m (head_draws[0]) and discards 5m -> both Ron and Pon are locked for North!
    let south_5m = game_pass
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(south_5m.len(), 2);
    assert!(project_view(&game_pass, Seat::North)
        .legal_actions
        .is_empty());
}

/// Test-only `Ruleset` wrapper over `TuidaohuRuleset` enabling `Chi` for `discarder.next()`
/// across the public `Match` seam.
#[derive(Debug, Clone, Copy, Default)]
struct ChiEnabledTuidaohuRuleset;

impl Ruleset for ChiEnabledTuidaohuRuleset {
    fn profile(&self) -> RuleProfile {
        let mut profile = TuidaohuRuleset.profile();
        profile.name = "TuidaohuWithChi".to_string();
        profile.allow_chi = true;
        profile
    }

    fn deck(&self) -> Vec<Tile> {
        TuidaohuRuleset.deck()
    }

    fn legal_turn_actions(&self, ctx: &TurnContext<'_>) -> Vec<PlayerAction> {
        TuidaohuRuleset.legal_turn_actions(ctx)
    }

    fn legal_discard_reactions(&self, ctx: &DiscardReactionContext<'_>) -> Vec<PlayerAction> {
        let mut actions = TuidaohuRuleset.legal_discard_reactions(ctx);
        if ctx.is_next_seat && ctx.wall_remaining > 0 && !ctx.tile.is_honor() {
            let raw = ctx.tile.as_u8();
            let suit_offset = raw % 9;
            let candidates = [
                (suit_offset >= 2)
                    .then(|| [Tile::new(raw - 2).unwrap(), Tile::new(raw - 1).unwrap()]),
                ((1..=7).contains(&suit_offset))
                    .then(|| [Tile::new(raw - 1).unwrap(), Tile::new(raw + 1).unwrap()]),
                (suit_offset <= 6)
                    .then(|| [Tile::new(raw + 1).unwrap(), Tile::new(raw + 2).unwrap()]),
            ];
            for with in candidates.into_iter().flatten() {
                if ctx.closed_tiles.contains(&with[0]) && ctx.closed_tiles.contains(&with[1]) {
                    actions.push(PlayerAction::Chi {
                        tile: ctx.tile,
                        with,
                    });
                }
            }
        }
        actions
    }

    fn legal_rob_kong_reactions(&self, ctx: &RobKongContext<'_>) -> Vec<PlayerAction> {
        TuidaohuRuleset.legal_rob_kong_reactions(ctx)
    }

    fn reaction_priority(&self, action: &PlayerAction) -> ReactionPriority {
        TuidaohuRuleset.reaction_priority(action)
    }

    fn evaluate_win(&self, input: &WinEvalInput<'_>) -> Option<EvaluatedWin> {
        TuidaohuRuleset.evaluate_win(input)
    }

    fn kong_score_deltas(&self, kind: KongKind, actor: Seat, from_seat: Option<Seat>) -> [i32; 4] {
        TuidaohuRuleset.kong_score_deltas(kind, actor, from_seat)
    }

    fn settle_hand(&self, input: SettleHandInput) -> HandSettlement {
        TuidaohuRuleset.settle_hand(input)
    }

    fn advance_hand(&self, input: HandProgressionInput<'_>) -> HandProgression {
        TuidaohuRuleset.advance_hand(input)
    }
}

#[test]
fn custom_chi_ruleset_executes_chi_clears_passed_hu_lock_and_breaks_menqing() {
    let east_14 = tiles("456s 1456p EENNFFB");
    let south_13 = tiles("12399m 123s 123p CC");
    let west_13 = tiles("456789s 789p ESWN");
    let north_13 = tiles("45678m 456s 456p CB");
    let wall = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("B S W 39m"),
        &[],
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::EastOnly,
        ChiEnabledTuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();
    assert!(project_view(&game, Seat::East).rule_profile.allow_chi);

    // East -> South -> West pass turns with safe discards.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("F"),
            from_drawn: false,
        },
    )
    .unwrap();
    for (seat, tile) in [(Seat::South, t("B")), (Seat::West, t("S"))] {
        game.apply_action(
            seat,
            PlayerAction::Discard {
                tile,
                from_drawn: true,
            },
        )
        .unwrap();
    }

    // North discards C; South is offered Ron + Pon and Passes -> sets passed_hu_lock.
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: t("C"),
            from_drawn: false,
        },
    )
    .unwrap();
    assert_eq!(
        project_view(&game, Seat::South).legal_actions,
        vec![
            PlayerAction::Ron { tile: t("C") },
            PlayerAction::Pon { tile: t("C") },
            PlayerAction::Pass,
        ]
    );
    game.apply_action(Seat::South, PlayerAction::Pass).unwrap();

    // East draws 3m and discards 3m -> only South (next seat) is offered Chi.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("3m"),
            from_drawn: true,
        },
    )
    .unwrap();
    assert_eq!(
        project_view(&game, Seat::South).legal_actions,
        vec![
            PlayerAction::Chi {
                tile: t("3m"),
                with: [t("1m"), t("2m")],
            },
            PlayerAction::Pass,
        ]
    );
    for other in [Seat::East, Seat::West, Seat::North] {
        assert!(project_view(&game, other).legal_actions.is_empty());
    }

    // South executes Chi!
    let chi_events = game
        .apply_action(
            Seat::South,
            PlayerAction::Chi {
                tile: t("3m"),
                with: [t("1m"), t("2m")],
            },
        )
        .unwrap();
    assert_eq!(chi_events.len(), 1);
    let meld_seq = chi_events[0].seq();
    let expected_meld = MeldView::Chi {
        tiles: [t("1m"), t("2m"), t("3m")],
        called_tile: t("3m"),
        from_seat: Seat::East,
        seq: meld_seq,
    };
    assert_eq!(
        project_event(&chi_events[0], Seat::North).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::South,
            meld: expected_meld,
            kong_score_deltas: None,
        }
    );

    let south_view = project_view(&game, Seat::South);
    assert_eq!(south_view.phase, PublicPhase::Playing);
    assert_eq!(south_view.turn.active_seat, Some(Seat::South));
    assert_eq!(south_view.hand.drawn_tile, None);
    assert_eq!(south_view.hand.closed_tiles, tiles("399m 123s 123p CC"));
    assert_eq!(south_view.seats[Seat::South.index()].hand_count, 11);
    assert_eq!(
        south_view.seats[Seat::South.index()].melds,
        vec![expected_meld]
    );
    assert_eq!(
        south_view.seats[Seat::South.index()].last_draw_or_meld_seq,
        meld_seq
    );
    assert_eq!(
        south_view.seats[Seat::East.index()]
            .river
            .last()
            .unwrap()
            .claimed_by,
        Some(Seat::South)
    );
    assert!(south_view.legal_actions.iter().all(|a| matches!(
        a,
        PlayerAction::Discard {
            from_drawn: false,
            ..
        }
    )));

    // South discards extra 3m; West draws and discards 9m.
    // Chi cleared passed_hu_lock and broke 门前清 -> South wins BasicHu (0 fan, 1 point).
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: t("3m"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: t("9m"),
            from_drawn: true,
        },
    )
    .unwrap();
    assert_eq!(
        project_view(&game, Seat::South).legal_actions,
        vec![
            PlayerAction::Ron { tile: t("9m") },
            PlayerAction::Pon { tile: t("9m") },
            PlayerAction::Pass,
        ]
    );
    game.apply_action(Seat::South, PlayerAction::Ron { tile: t("9m") })
        .unwrap();

    let settled = project_view(&game, Seat::South).last_settlement.unwrap();
    assert_eq!(settled.score_deltas, [0, 1, -1, 0]);
    assert_eq!(
        settled.revealed_hands[Seat::South.index()].melds,
        vec![RevealedMeld::Chi {
            tiles: [t("1m"), t("2m"), t("3m")],
            called_tile: t("3m"),
            from_seat: Seat::East,
            seq: meld_seq,
        }]
    );
    match &settled.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, Some(Seat::West));
            assert_eq!(*winning_tile, t("9m"));
            assert_eq!(winners.len(), 1);
            assert_eq!(winners[0].seat, Seat::South);
            assert_eq!(winners[0].win_kind, WinKind::Ron);
            assert_eq!(winners[0].total_fan, 0);
            assert_eq!(winners[0].base_points, 1);
            assert_eq!(
                winners[0].items,
                vec![ScoreItem {
                    code: "basic_hu".to_string(),
                    label: "基本胡".to_string(),
                    fan: 0,
                }]
            );
        }
        other => panic!("expected Win settlement, got {other:?}"),
    }
}

#[test]
fn custom_chi_ruleset_arbitrates_hu_over_pon_over_chi_and_sorts_chi_tiles() {
    let east_14 = tiles("5m 123s 123p EESSWWN");
    let south_13 = tiles("46m 456s 456p CCFFN");
    let west_13 = tiles("5589m 789s 789p BBE");
    let north_13 = tiles("34m 123s 123999p SS");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[]).unwrap();

    // 1. Hu > Pon > Chi even when Chi (South) and Pon (West) submit before Ron (North).
    let mut game_hu = Match::new(
        MatchMode::EastOnly,
        ChiEnabledTuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
    )
    .unwrap();
    game_hu
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_hu, Seat::South).legal_actions,
        vec![
            PlayerAction::Chi {
                tile: t("5m"),
                with: [t("4m"), t("6m")],
            },
            PlayerAction::Pass,
        ]
    );
    assert_eq!(
        project_view(&game_hu, Seat::West).legal_actions,
        vec![PlayerAction::Pon { tile: t("5m") }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&game_hu, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("5m") }, PlayerAction::Pass]
    );
    assert!(game_hu
        .apply_action(
            Seat::South,
            PlayerAction::Chi {
                tile: t("5m"),
                with: [t("4m"), t("6m")],
            },
        )
        .unwrap()
        .is_empty());
    assert!(game_hu
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("5m") })
        .unwrap()
        .is_empty());
    game_hu
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("5m") })
        .unwrap();
    let hu_settled = project_view(&game_hu, Seat::North).last_settlement.unwrap();
    match &hu_settled.outcome {
        HandOutcome::Win {
            winners, discarder, ..
        } => {
            assert_eq!(*discarder, Some(Seat::East));
            assert_eq!(winners.len(), 1);
            assert_eq!(winners[0].seat, Seat::North);
            assert_eq!(winners[0].win_kind, WinKind::Ron);
            assert_eq!(winners[0].total_fan, 1);
            assert_eq!(winners[0].base_points, 2);
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 2. Pon > Chi early resolution when Hu seat passes and West submits Pon first.
    let mut game_pon_early = Match::new(
        MatchMode::EastOnly,
        ChiEnabledTuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
    )
    .unwrap();
    game_pon_early
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_pon_early
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap()
        .is_empty());
    let pon_ev = game_pon_early
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("5m") })
        .unwrap();
    assert_eq!(pon_ev.len(), 1);
    assert_eq!(
        project_view(&game_pon_early, Seat::West).turn.active_seat,
        Some(Seat::West)
    );
    assert_eq!(
        project_view(&game_pon_early, Seat::South).seats[Seat::West.index()].melds,
        vec![MeldView::Pon {
            tile: t("5m"),
            from_seat: Seat::East,
            seq: pon_ev[0].seq(),
        }]
    );
    assert!(project_view(&game_pon_early, Seat::South)
        .hand
        .closed_tiles
        .contains(&t("4m")));
    assert!(project_view(&game_pon_early, Seat::South)
        .hand
        .closed_tiles
        .contains(&t("6m")));

    // 3. Pon > Chi when South submits Chi before West submits Pon.
    let mut game_pon_after_chi = Match::new(
        MatchMode::EastOnly,
        ChiEnabledTuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
    )
    .unwrap();
    game_pon_after_chi
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_pon_after_chi
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap()
        .is_empty());
    assert!(game_pon_after_chi
        .apply_action(
            Seat::South,
            PlayerAction::Chi {
                tile: t("5m"),
                with: [t("4m"), t("6m")],
            },
        )
        .unwrap()
        .is_empty());
    let pon_after_ev = game_pon_after_chi
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("5m") })
        .unwrap();
    assert_eq!(pon_after_ev.len(), 1);
    assert_eq!(
        project_view(&game_pon_after_chi, Seat::West)
            .turn
            .active_seat,
        Some(Seat::West)
    );

    // 4. Chi wins when Hu and Pon seats both Pass, and sorts [4m, 5m, 6m] with called_tile = 5m.
    let mut game_chi = Match::new(
        MatchMode::EastOnly,
        ChiEnabledTuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();
    game_chi
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("5m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_chi
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap()
        .is_empty());
    assert!(game_chi
        .apply_action(Seat::West, PlayerAction::Pass)
        .unwrap()
        .is_empty());
    let chi_ev = game_chi
        .apply_action(
            Seat::South,
            PlayerAction::Chi {
                tile: t("5m"),
                with: [t("4m"), t("6m")],
            },
        )
        .unwrap();
    assert_eq!(chi_ev.len(), 1);
    let south_after_chi = project_view(&game_chi, Seat::South);
    assert_eq!(south_after_chi.turn.active_seat, Some(Seat::South));
    assert_eq!(
        south_after_chi.seats[Seat::South.index()].melds,
        vec![MeldView::Chi {
            tiles: [t("4m"), t("5m"), t("6m")],
            called_tile: t("5m"),
            from_seat: Seat::East,
            seq: chi_ev[0].seq(),
        }]
    );
    assert!(!south_after_chi.hand.closed_tiles.contains(&t("4m")));
    assert!(!south_after_chi.hand.closed_tiles.contains(&t("5m")));
    assert!(!south_after_chi.hand.closed_tiles.contains(&t("6m")));
}

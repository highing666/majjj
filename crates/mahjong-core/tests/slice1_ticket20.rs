mod common;

use common::{assert_136_tile_conservation, tile as t, tiles};
use mahjong_core::{
    build_preset_wall, project_event, project_view, AnyRuleset, HandOutcome, HandSettlement,
    KongKind, KongTransfer, Match, MatchError, MatchMode, MeldView, PlayerAction, PlayerEventKind,
    PlayerView, PresetWallProvider, PublicPhase, RevealedMeld, Ruleset, ScoreItem, Seat, Tile,
    TuidaohuRuleset, WallProvider, WinKind,
};

fn next_legal_seat<R: Ruleset, W: WallProvider>(game: &Match<R, W>) -> (Seat, PlayerView) {
    Seat::ALL
        .into_iter()
        .map(|s| (s, project_view(game, s)))
        .find(|(_, v)| !v.legal_actions.is_empty())
        .expect("active hand should have at least one legal action")
}

fn default_pass_or_tsumogiri(view: &PlayerView) -> PlayerAction {
    if view.legal_actions.contains(&PlayerAction::Pass) {
        PlayerAction::Pass
    } else {
        view.legal_actions
            .iter()
            .copied()
            .find(|a| {
                matches!(
                    a,
                    PlayerAction::Discard {
                        from_drawn: true,
                        ..
                    }
                )
            })
            .unwrap_or(view.legal_actions[0])
    }
}

fn drain_until<R: Ruleset, W: WallProvider>(
    game: &mut Match<R, W>,
    mut stop: impl FnMut(&mut Match<R, W>, Seat, &PlayerView) -> bool,
) {
    loop {
        let (seat, view) = next_legal_seat(game);
        if stop(game, seat, &view) {
            break;
        }
        let action = default_pass_or_tsumogiri(&view);
        game.apply_action(seat, action).unwrap();
    }
}

#[test]
fn slice1_concealed_kong_sources_projection_menqing_never_robbable_and_zero_wall_guard() {
    // South holds a 13-tile hand waiting on 1m while East holds all 4 copies of 1m + 4 copies of C.
    let south_waiting_1m = tiles("23m 123567p 234s BB");
    let west_13 = tiles("456789m 89p 189s ES");
    let north_13 = tiles("456789m 89p 189s WN");
    let east_14 = tiles("1111m 234p 56s F CCCC");
    let wall = build_preset_wall(
        &east_14,
        &south_waiting_1m,
        &west_13,
        &north_13,
        &[],
        &tiles("7s F"),
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    let init_east = project_view(&game, Seat::East);
    assert_136_tile_conservation(&game);
    let initial_kongs: Vec<PlayerAction> = init_east
        .legal_actions
        .iter()
        .copied()
        .filter(|a| matches!(a, PlayerAction::ConcealedKong { .. }))
        .collect();
    // Deterministic tile order: 1m (index 0) before C (index 31).
    assert_eq!(
        initial_kongs,
        vec![
            PlayerAction::ConcealedKong { tile: t("1m") },
            PlayerAction::ConcealedKong { tile: t("C") },
        ]
    );

    // East declares ConcealedKong { 1m } on InitialDealer; NOT robbable by South.
    let kong1_events = game
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: t("1m") })
        .unwrap();
    assert_eq!(kong1_events.len(), 2);

    // Event 0: MeldDeclared (ConcealedKong hides tile from opponents, exposes to East).
    let meld1_seq = kong1_events[0].seq();
    assert_eq!(
        project_event(&kong1_events[0], Seat::East).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::ConcealedKong {
                tile: Some(t("1m")),
                seq: meld1_seq,
            },
            kong_score_deltas: Some([6, -2, -2, -2]),
        }
    );
    for opp in [Seat::South, Seat::West, Seat::North] {
        assert_eq!(
            project_event(&kong1_events[0], opp).kind,
            PlayerEventKind::MeldDeclared {
                seat: Seat::East,
                meld: MeldView::ConcealedKong {
                    tile: None,
                    seq: meld1_seq,
                },
                kong_score_deltas: Some([6, -2, -2, -2]),
            }
        );
    }

    // Event 1: TileDrawn from wall tail (`is_replacement: true`).
    let draw1_seq = kong1_events[1].seq();
    assert_eq!(draw1_seq, meld1_seq + 1);
    assert_eq!(
        project_event(&kong1_events[1], Seat::East).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(t("7s")),
            is_replacement: true,
            wall_remaining: 82,
        }
    );
    assert_eq!(
        project_event(&kong1_events[1], Seat::South).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: None,
            is_replacement: true,
            wall_remaining: 82,
        }
    );

    // Check view after 1st ConcealedKong: scores untouched until HandSettled; melds projected per-seat.
    let view_east_1 = project_view(&game, Seat::East);
    assert_136_tile_conservation(&game);
    assert_eq!(view_east_1.phase, PublicPhase::Playing);
    assert_eq!(view_east_1.turn.active_seat, Some(Seat::East));
    assert_eq!(view_east_1.wall_remaining, 82);
    assert_eq!(view_east_1.hand.drawn_tile, Some(t("7s")));
    assert_eq!(view_east_1.hand.closed_tiles.len(), 10);
    assert_eq!(
        view_east_1.seats[Seat::East.index()].last_draw_or_meld_seq,
        draw1_seq
    );
    assert_eq!(
        view_east_1.seats[Seat::East.index()].score,
        100,
        "kong transfers must settle at HandSettled, not mid-hand"
    );
    assert_eq!(
        view_east_1.seats[Seat::East.index()].melds,
        vec![MeldView::ConcealedKong {
            tile: Some(t("1m")),
            seq: meld1_seq,
        }]
    );
    let view_south_1 = project_view(&game, Seat::South);
    assert_eq!(
        view_south_1.seats[Seat::East.index()].melds,
        vec![MeldView::ConcealedKong {
            tile: None,
            seq: meld1_seq,
        }]
    );
    assert!(view_south_1.legal_actions.is_empty());

    // Chained ReplacementDraw ConcealedKong { C } where all 4 C are in closed_tiles and 7s is in drawn_tile.
    assert!(view_east_1
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: t("C") }));
    let kong2_events = game
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: t("C") })
        .unwrap();
    assert_eq!(kong2_events.len(), 2);

    let view_east_2 = project_view(&game, Seat::East);
    assert_136_tile_conservation(&game);
    assert_eq!(view_east_2.wall_remaining, 81);
    assert_eq!(view_east_2.hand.drawn_tile, Some(t("F")));
    assert_eq!(view_east_2.hand.closed_tiles, tiles("567s 234p F"));
    assert!(view_east_2.legal_actions.contains(&PlayerAction::Tsumo));

    // East declares Tsumo on 2nd replacement draw -> 门前清 (+1) + 杠上开花 (+1) = 2 fan (4 base points).
    let tsumo_events = game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
    let settled = project_event(&tsumo_events[0], Seat::North);
    match settled.kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(settlement.kong_transfers.len(), 2);
            assert_eq!(
                settlement.kong_transfers[0],
                KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor: Seat::East,
                    from_seat: None,
                    tile: t("1m"),
                    score_deltas: [6, -2, -2, -2],
                }
            );
            assert_eq!(
                settlement.kong_transfers[1],
                KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor: Seat::East,
                    from_seat: None,
                    tile: t("C"),
                    score_deltas: [6, -2, -2, -2],
                }
            );
            assert_eq!(settlement.score_deltas, [24, -8, -8, -8]);
            assert_eq!(settlement.new_scores, [124, 92, 92, 92]);
            match &settlement.outcome {
                HandOutcome::Win {
                    winners,
                    discarder,
                    winning_tile,
                } => {
                    assert_eq!(*discarder, None);
                    assert_eq!(*winning_tile, t("F"));
                    assert_eq!(winners.len(), 1);
                    assert_eq!(winners[0].win_kind, WinKind::Tsumo);
                    assert_eq!(winners[0].total_fan, 2);
                    assert_eq!(winners[0].base_points, 4);
                    assert_eq!(
                        winners[0].items,
                        vec![
                            ScoreItem {
                                code: "menqing".to_string(),
                                label: "门前清".to_string(),
                                fan: 1,
                            },
                            ScoreItem {
                                code: "self_draw_after_kong".to_string(),
                                label: "杠上开花".to_string(),
                                fan: 1,
                            },
                        ]
                    );
                }
                other => panic!("expected Win, got {other:?}"),
            }
            // HandSettled reveals East's ConcealedKong concrete tiles to all seats.
            assert_eq!(
                settlement.revealed_hands[Seat::East.index()].melds,
                vec![
                    RevealedMeld::ConcealedKong {
                        tile: t("1m"),
                        seq: meld1_seq,
                    },
                    RevealedMeld::ConcealedKong {
                        tile: t("C"),
                        seq: kong2_events[0].seq(),
                    },
                ]
            );
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }
}

#[test]
fn slice1_concealed_kong_on_normal_draw_with_drawn_tile_and_after_meld_and_zero_wall_forbidden() {
    let east_14 = tiles("189m 19s 19p ESWNCFB");
    let south_13 = tiles("112222m 234s 234p E");
    let west_13 = tiles("3334444m 567s 567p");
    let north_13 = tiles("567m 189s 189p SWNC");
    let wall = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("3m"),
        &tiles("BF"),
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    // East discards 1m -> South Pons 1m -> enters AfterMeld with 4x 2m in closed_tiles.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("1m"),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::South, PlayerAction::Pon { tile: t("1m") })
        .unwrap();

    let south_after_meld = project_view(&game, Seat::South);
    assert_136_tile_conservation(&game);
    assert!(south_after_meld
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: t("2m") }));

    // South declares ConcealedKong { 2m } directly in AfterMeld.
    game.apply_action(Seat::South, PlayerAction::ConcealedKong { tile: t("2m") })
        .unwrap();
    let south_after_kong = project_view(&game, Seat::South);
    assert_136_tile_conservation(&game);
    assert_eq!(south_after_kong.hand.drawn_tile, Some(t("B")));
    assert_eq!(south_after_kong.wall_remaining, 82);
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: t("B"),
            from_drawn: true,
        },
    )
    .unwrap();

    // West now holds 3x 3m in closed_tiles + 3m in drawn_tile AND 4x 4m in closed_tiles.
    let west_view = project_view(&game, Seat::West);
    assert_eq!(west_view.hand.drawn_tile, Some(t("3m")));
    assert!(west_view
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: t("3m") }));
    assert!(west_view
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: t("4m") }));

    // West declares ConcealedKong { 3m } (consuming drawn_tile + 3 from closed_tiles).
    game.apply_action(Seat::West, PlayerAction::ConcealedKong { tile: t("3m") })
        .unwrap();
    let west_after_kong = project_view(&game, Seat::West);
    assert_136_tile_conservation(&game);
    assert_eq!(west_after_kong.hand.drawn_tile, Some(t("F")));
    assert!(!west_after_kong.hand.closed_tiles.contains(&t("3m")));
    assert_eq!(west_after_kong.wall_remaining, 80);

    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: t("F"),
            from_drawn: true,
        },
    )
    .unwrap();

    // Drain to wall_remaining == 0 on West's turn; 4x 4m must NOT offer ConcealedKong { 4m }.
    let mut checked_zero_wall = false;
    drain_until(&mut game, |g, seat, view| {
        if view.wall_remaining == 0 && seat == Seat::West {
            assert!(
                !view
                    .legal_actions
                    .iter()
                    .any(|a| matches!(a, PlayerAction::ConcealedKong { .. })),
                "ConcealedKong must be forbidden at wall_remaining == 0"
            );
            let before_seq = g.seq();
            assert!(matches!(
                g.apply_action(seat, PlayerAction::ConcealedKong { tile: t("4m") })
                    .unwrap_err(),
                MatchError::IllegalAction {
                    seat: Seat::West,
                    action: PlayerAction::ConcealedKong { .. }
                }
            ));
            assert_eq!(g.seq(), before_seq);
            checked_zero_wall = true;
            return true;
        }
        false
    });
    assert!(checked_zero_wall);
}

#[test]
fn slice2_exposed_kong_execution_no_pao_hu_priority_passed_pon_independence_and_no_gangshangpao() {
    let east_14 = tiles("17m 19s 19p ESWNCFBB");
    let south_13 = tiles("2468m 246s 246p ESB");
    let west_13 = tiles("111234m 567s 567p C");
    let north_13 = tiles("23m 234888s 345p FF");
    let wall =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &tiles("C")).unwrap();

    // Test A1: West submits ExposedKong first, North submits Pass -> ExposedKong -> West replacement Tsumo.
    let mut game_a = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
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
        project_view(&game_a, Seat::West).legal_actions,
        vec![
            PlayerAction::Pon { tile: t("1m") },
            PlayerAction::ExposedKong { tile: t("1m") },
            PlayerAction::Pass,
        ]
    );
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: t("1m") }, PlayerAction::Pass]
    );

    assert!(game_a
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: t("1m") })
        .unwrap()
        .is_empty());
    assert!(project_view(&game_a, Seat::West).legal_actions.is_empty());

    let resolve_events = game_a
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert_eq!(resolve_events.len(), 2);
    let meld_seq = resolve_events[0].seq();
    let draw_seq = resolve_events[1].seq();
    assert_eq!(
        project_event(&resolve_events[0], Seat::South).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::West,
            meld: MeldView::ExposedKong {
                tile: t("1m"),
                from_seat: Seat::East,
                seq: meld_seq,
            },
            kong_score_deltas: Some([-2, 0, 2, 0]),
        }
    );
    assert_eq!(
        project_event(&resolve_events[1], Seat::West).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::West,
            tile: Some(t("C")),
            is_replacement: true,
            wall_remaining: 82,
        }
    );

    let west_after = project_view(&game_a, Seat::West);
    assert_136_tile_conservation(&game_a);
    assert_eq!(
        west_after.seats[Seat::East.index()].river[0].claimed_by,
        Some(Seat::West)
    );
    assert_eq!(
        west_after.seats[Seat::West.index()].last_draw_or_meld_seq,
        draw_seq
    );
    assert!(west_after.legal_actions.contains(&PlayerAction::Tsumo));

    // West Tsumos -> 杠上开花 (+1 fan, 2 base points), 3-way split (NO 明杠 pao).
    let tsumo_ev = game_a
        .apply_action(Seat::West, PlayerAction::Tsumo)
        .unwrap();
    match project_event(&tsumo_ev[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(
                settlement.kong_transfers,
                vec![KongTransfer {
                    kind: KongKind::ExposedKong,
                    actor: Seat::West,
                    from_seat: Some(Seat::East),
                    tile: t("1m"),
                    score_deltas: [-2, 0, 2, 0],
                }]
            );
            assert_eq!(settlement.score_deltas, [-4, -2, 8, -2]);
            assert_eq!(settlement.new_scores, [96, 98, 108, 98]);
            match settlement.outcome {
                HandOutcome::Win { winners, .. } => {
                    assert_eq!(winners[0].total_fan, 1);
                    assert_eq!(winners[0].base_points, 2);
                    assert_eq!(
                        winners[0].items,
                        vec![ScoreItem {
                            code: "self_draw_after_kong".to_string(),
                            label: "杠上开花".to_string(),
                            fan: 1,
                        }]
                    );
                }
                other => panic!("expected Win, got {other:?}"),
            }
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // Test A2: West submits ExposedKong first, North submits Ron -> Hu beats ExposedKong.
    let mut game_b = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
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
    game_b
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: t("1m") })
        .unwrap();
    let ron_ev = game_b
        .apply_action(Seat::North, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    assert_eq!(ron_ev.len(), 1);
    match project_event(&ron_ev[0], Seat::West).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert!(settlement.kong_transfers.is_empty());
            assert_eq!(settlement.score_deltas, [-2, 0, 0, 2]);
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // Test A3: Passing Pon { 1m } locks Pon { 1m }, NOT ExposedKong { 7m } before West draws.
    let east_c = tiles("19m 19s 19p ESWNCFBB");
    let west_c = tiles("1177723m 567s 56p C");
    let wall_c = build_preset_wall(
        &east_c,
        &south_13,
        &west_c,
        &north_13,
        &tiles("7m"),
        &tiles("C"),
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
    game_c.apply_action(Seat::West, PlayerAction::Pass).unwrap();
    game_c
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();

    game_c
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("7m"),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_c, Seat::West).legal_actions,
        vec![
            PlayerAction::Pon { tile: t("7m") },
            PlayerAction::ExposedKong { tile: t("7m") },
            PlayerAction::Pass,
        ]
    );
    game_c
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: t("7m") })
        .unwrap();
    assert_eq!(
        project_view(&game_c, Seat::South).seats[Seat::South.index()].river[0].claimed_by,
        Some(Seat::West)
    );
}

#[test]
fn slice2_last_tile_exposed_kong_stacks_gangshang_haidi_and_discard_after_kong_has_no_gangshangpao()
{
    // 1. Discard after replacement draw is ordinary Ron (no 杠上炮) and keeps KongTransfer.
    let east_14 = tiles("123456789m 19s 19p E");
    let south_13 = tiles("23m 234789s 234p BB");
    let west_13 = tiles("1m 111567s 678p CCN");
    let north_13 = tiles("456m 55p 234s SWFFE");
    let wall_no_gsp =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &tiles("B")).unwrap();
    let mut game_gsp = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_no_gsp),
    )
    .unwrap();
    game_gsp
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("1s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_gsp
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: t("1s") })
        .unwrap();
    game_gsp
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: false,
            },
        )
        .unwrap();
    let ron_events = game_gsp
        .apply_action(Seat::South, PlayerAction::Ron { tile: t("1m") })
        .unwrap();
    match project_event(&ron_events[0], Seat::South).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(settlement.score_deltas, [-2, 2, 0, 0]);
            match settlement.outcome {
                HandOutcome::Win { winners, .. } => {
                    assert_eq!(winners[0].total_fan, 1);
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
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // 2. Last-tile ExposedKong at wall_remaining == 1 -> replacement Tsumo stacks 杠上开花 + 海底捞月 (capped at 4).
    let east_last = tiles("123456789m 19p ESW");
    let south_last = tiles("147m 258p 246s WNCF");
    let west_last = tiles("258m 369p 188s ESBB");
    let north_last_cap = tiles("1113335557779s");
    let wall_last_cap = build_preset_wall(
        &east_last,
        &south_last,
        &west_last,
        &north_last_cap,
        &[],
        &tiles("9s"),
    )
    .unwrap();
    let mut game_cap = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_last_cap),
    )
    .unwrap();
    game_cap
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("E"),
                from_drawn: false,
            },
        )
        .unwrap();

    drain_until(&mut game_cap, |g, seat, view| {
        if view.wall_remaining == 1 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            g.apply_action(
                Seat::West,
                PlayerAction::Discard {
                    tile: t("1s"),
                    from_drawn: false,
                },
            )
            .unwrap();
            let north_view = project_view(g, Seat::North);
            assert_eq!(north_view.wall_remaining, 1);
            assert!(north_view
                .legal_actions
                .contains(&PlayerAction::ExposedKong { tile: t("1s") }));
            g.apply_action(Seat::North, PlayerAction::ExposedKong { tile: t("1s") })
                .unwrap();
            let north_repl = project_view(g, Seat::North);
            assert_eq!(north_repl.wall_remaining, 0);
            assert_eq!(north_repl.hand.drawn_tile, Some(t("9s")));
            assert!(north_repl.legal_actions.contains(&PlayerAction::Tsumo));
            let settled_ev = g.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
            match project_event(&settled_ev[0], Seat::North).kind {
                PlayerEventKind::HandSettled { settlement } => match settlement.outcome {
                    HandOutcome::Win { winners, .. } => {
                        assert_eq!(winners[0].total_fan, 4);
                        assert_eq!(winners[0].base_points, 16);
                        assert!(winners[0]
                            .items
                            .iter()
                            .any(|i| i.code == "self_draw_after_kong"));
                        assert!(winners[0].items.iter().any(|i| i.code == "last_tile_tsumo"));
                        assert!(winners[0].items.iter().any(|i| i.code == "full_flush"));
                        assert!(winners[0].items.iter().any(|i| i.code == "all_pungs"));
                    }
                    other => panic!("expected Win, got {other:?}"),
                },
                other => panic!("expected HandSettled, got {other:?}"),
            }
            return true;
        }
        false
    });

    // 2b. Uncapped last-tile replacement Tsumo: 杠上开花 (1) + 海底捞月 (1) = 2 fan (4 base points).
    let south_last_uncapped = tiles("147m 258p 296s WNCF");
    let west_last_uncapped = tiles("258m 369p 198s ESBB");
    let north_last_uncapped = tiles("1113459s 234m 456p");
    let wall_last_uncapped = build_preset_wall(
        &east_last,
        &south_last_uncapped,
        &west_last_uncapped,
        &north_last_uncapped,
        &[],
        &tiles("9s"),
    )
    .unwrap();
    let mut game_uncapped = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_last_uncapped),
    )
    .unwrap();
    game_uncapped
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("E"),
                from_drawn: false,
            },
        )
        .unwrap();
    drain_until(&mut game_uncapped, |g, seat, view| {
        if view.wall_remaining == 1 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            g.apply_action(
                Seat::West,
                PlayerAction::Discard {
                    tile: t("1s"),
                    from_drawn: false,
                },
            )
            .unwrap();
            g.apply_action(Seat::North, PlayerAction::ExposedKong { tile: t("1s") })
                .unwrap();
            let settled_ev = g.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
            match project_event(&settled_ev[0], Seat::North).kind {
                PlayerEventKind::HandSettled { settlement } => {
                    assert_eq!(settlement.score_deltas, [-4, -4, -6, 14]);
                    match settlement.outcome {
                        HandOutcome::Win { winners, .. } => {
                            assert_eq!(winners[0].total_fan, 2);
                            assert_eq!(winners[0].base_points, 4);
                            assert_eq!(
                                winners[0].items,
                                vec![
                                    ScoreItem {
                                        code: "self_draw_after_kong".to_string(),
                                        label: "杠上开花".to_string(),
                                        fan: 1,
                                    },
                                    ScoreItem {
                                        code: "last_tile_tsumo".to_string(),
                                        label: "海底捞月".to_string(),
                                        fan: 1,
                                    },
                                ]
                            );
                        }
                        other => panic!("expected Win, got {other:?}"),
                    }
                }
                other => panic!("expected HandSettled, got {other:?}"),
            }
            return true;
        }
        false
    });

    // 3. ExposedKong is forbidden when wall_remaining == 0.
    let east_zero = tiles("123456m 19p 111s ESW");
    let north_zero = tiles("789m 246p 1357s CFB");
    let west_zero = tiles("258m 369p 489s ESBB");
    let wall_zero =
        build_preset_wall(&east_zero, &south_last, &west_zero, &north_zero, &[], &[]).unwrap();
    let mut game_zero = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_zero),
    )
    .unwrap();
    game_zero
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("E"),
                from_drawn: false,
            },
        )
        .unwrap();
    drain_until(&mut game_zero, |g, seat, view| {
        if view.wall_remaining == 0 && seat == Seat::North {
            let ev = g
                .apply_action(
                    Seat::North,
                    PlayerAction::Discard {
                        tile: t("1s"),
                        from_drawn: false,
                    },
                )
                .unwrap();
            assert!(matches!(
                project_event(ev.last().unwrap(), Seat::East).kind,
                PlayerEventKind::HandSettled {
                    settlement: HandSettlement {
                        outcome: HandOutcome::DrawGame,
                        ..
                    }
                }
            ));
            return true;
        }
        false
    });
}

#[test]
fn slice3_added_kong_one_step_sources_ordering_after_meld_and_draw_game_keeps_kong_transfers() {
    let east_14 = tiles("17m 19s 19p ESWNCFBB");
    let south_13 = tiles("2468m 246s 246p ESB");
    let west_13 = tiles("11234m 567s 567p CF");
    let north_13 = tiles("357m 389s 389p SWNN");
    let wall_a = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &tiles("8p 351m"),
        &tiles("C"),
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
    game_a
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("1m") })
        .unwrap();
    game_a
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("F"),
                from_drawn: false,
            },
        )
        .unwrap();
    for seat in [Seat::North, Seat::East, Seat::South] {
        let drawn = project_view(&game_a, seat).hand.drawn_tile.unwrap();
        game_a
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile: drawn,
                    from_drawn: true,
                },
            )
            .unwrap();
    }

    let west_turn = project_view(&game_a, Seat::West);
    assert_eq!(west_turn.hand.drawn_tile, Some(t("1m")));
    assert!(west_turn
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: t("1m") }));

    // Nobody can RobKongHu on 1m -> completes in one step with [MeldDeclared, TileDrawn] (no AddedKongAttempted).
    let added_ev = game_a
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    assert_eq!(added_ev.len(), 2);
    let meld_seq = added_ev[0].seq();
    let draw_seq = added_ev[1].seq();
    assert_eq!(
        project_event(&added_ev[0], Seat::North).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::West,
            meld: MeldView::AddedKong {
                tile: t("1m"),
                from_seat: Seat::East,
                seq: meld_seq,
            },
            kong_score_deltas: Some([-1, -1, 3, -1]),
        }
    );
    assert_eq!(
        project_event(&added_ev[1], Seat::West).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::West,
            tile: Some(t("C")),
            is_replacement: true,
            wall_remaining: 78,
        }
    );

    let west_repl = project_view(&game_a, Seat::West);
    assert_136_tile_conservation(&game_a);
    assert_eq!(
        west_repl.seats[Seat::West.index()].last_draw_or_meld_seq,
        draw_seq
    );
    assert_eq!(
        west_repl.seats[Seat::West.index()].melds,
        vec![MeldView::AddedKong {
            tile: t("1m"),
            from_seat: Seat::East,
            seq: meld_seq,
        }]
    );
    assert!(west_repl.legal_actions.contains(&PlayerAction::Tsumo));

    // Tsumo on replacement draw -> win [+6, -2, -2, -2] + AddedKong [-1, -1, +3, -1] = [-3, -3, +9, -3].
    let win_ev = game_a
        .apply_action(Seat::West, PlayerAction::Tsumo)
        .unwrap();
    match project_event(&win_ev[0], Seat::West).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(
                settlement.kong_transfers,
                vec![KongTransfer {
                    kind: KongKind::AddedKong,
                    actor: Seat::West,
                    from_seat: None,
                    tile: t("1m"),
                    score_deltas: [-1, -1, 3, -1],
                }]
            );
            assert_eq!(settlement.score_deltas, [-3, -3, 9, -3]);
            assert_eq!(settlement.new_scores, [97, 97, 109, 97]);
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // 2. Multiple Pons + AfterMeld AddedKong + ReplacementDraw AddedKong + DrawGame keeps kong_transfers.
    let east_b = tiles("9s 23468m 19p ESWNCF");
    let south_b = tiles("3579m 246s 246p ESB");
    let west_b = tiles("999s 111m 135s 357p N");
    let north_b = tiles("246m 78s 89p ESWCFB");
    let wall_b = build_preset_wall(
        &east_b,
        &south_b,
        &west_b,
        &north_b,
        &tiles("91m"),
        &tiles("57p"),
    )
    .unwrap();
    let mut game_b = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b),
    )
    .unwrap();
    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("9s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("9s") })
        .unwrap();
    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("N"),
                from_drawn: false,
            },
        )
        .unwrap();
    for (seat, tile) in [(Seat::North, t("9m")), (Seat::East, t("1m"))] {
        game_b
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile,
                    from_drawn: true,
                },
            )
            .unwrap();
    }
    game_b
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("1m") })
        .unwrap();

    // Deterministic tile order must emit AddedKong { 1m } BEFORE AddedKong { 9s }.
    let west_after_2pon = project_view(&game_b, Seat::West);
    let added_kongs: Vec<PlayerAction> = west_after_2pon
        .legal_actions
        .iter()
        .copied()
        .filter(|a| matches!(a, PlayerAction::AddedKong { .. }))
        .collect();
    assert_eq!(
        added_kongs,
        vec![
            PlayerAction::AddedKong { tile: t("1m") },
            PlayerAction::AddedKong { tile: t("9s") },
        ]
    );

    game_b
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: t("9s") })
        .unwrap();
    let west_after_k1 = project_view(&game_b, Seat::West);
    assert_136_tile_conservation(&game_b);
    assert_eq!(west_after_k1.hand.drawn_tile, Some(t("5p")));
    assert!(west_after_k1
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: t("1m") }));

    game_b
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    let west_after_k2 = project_view(&game_b, Seat::West);
    assert_136_tile_conservation(&game_b);
    assert_eq!(west_after_k2.hand.drawn_tile, Some(t("7p")));
    assert!(!west_after_k2.hand.closed_tiles.contains(&t("1m")));
    assert!(west_after_k2.hand.closed_tiles.contains(&t("5p")));
    assert_eq!(
        west_after_k2.seats[Seat::West.index()]
            .melds
            .iter()
            .filter(|m| matches!(m, MeldView::AddedKong { .. }))
            .count(),
        2
    );

    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("7p"),
                from_drawn: true,
            },
        )
        .unwrap();
    loop {
        let probe = project_view(&game_b, Seat::East);
        if probe.phase == PublicPhase::BetweenHands {
            let settlement = probe.last_settlement.unwrap();
            assert_eq!(settlement.outcome, HandOutcome::DrawGame);
            assert_eq!(settlement.kong_transfers.len(), 2);
            assert_eq!(settlement.score_deltas, [-2, -2, 6, -2]);
            assert_eq!(settlement.new_scores, [-2, -2, 6, -2]);
            break;
        }
        let (seat, view) = next_legal_seat(&game_b);
        let action = default_pass_or_tsumogiri(&view);
        game_b.apply_action(seat, action).unwrap();
    }

    // 3. AddedKong is forbidden when wall_remaining == 0.
    let mut east_c = east_b.clone();
    east_c[0] = t("7m");
    let mut north_c = north_b.clone();
    north_c[0] = t("9s");
    let wall_c =
        build_preset_wall(&east_c, &south_b, &west_b, &north_c, &tiles("889p"), &[]).unwrap();
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
                tile: t("7m"),
                from_drawn: false,
            },
        )
        .unwrap();
    for seat in [Seat::South, Seat::West] {
        let drawn = project_view(&game_c, seat).hand.drawn_tile.unwrap();
        game_c
            .apply_action(
                seat,
                PlayerAction::Discard {
                    tile: drawn,
                    from_drawn: true,
                },
            )
            .unwrap();
    }
    game_c
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("9s"),
                from_drawn: false,
            },
        )
        .unwrap();
    game_c
        .apply_action(Seat::West, PlayerAction::Pon { tile: t("9s") })
        .unwrap();
    game_c
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("N"),
                from_drawn: false,
            },
        )
        .unwrap();
    let mut checked_added_zero = false;
    drain_until(&mut game_c, |g, seat, view| {
        if view.wall_remaining == 0 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            assert!(
                !view
                    .legal_actions
                    .iter()
                    .any(|a| matches!(a, PlayerAction::AddedKong { .. })),
                "AddedKong must be forbidden at wall_remaining == 0"
            );
            let before_seq = g.seq();
            assert!(matches!(
                g.apply_action(seat, PlayerAction::AddedKong { tile: t("9s") })
                    .unwrap_err(),
                MatchError::IllegalAction {
                    seat: Seat::West,
                    action: PlayerAction::AddedKong { .. }
                }
            ));
            assert_eq!(g.seq(), before_seq);
            checked_added_zero = true;
            return true;
        }
        false
    });
    assert!(checked_added_zero);
}

#[test]
fn slice4_added_kong_awaiting_rob_kong_zero_leakage_error_matrix_and_pass_locks() {
    let east_base = tiles("11456m 456s 789p CCF");
    let south_wait_1m_b = tiles("23m 123s 123456p BN");
    let west_plain_no_1m = tiles("789m 789s 246p SWNE");
    let north_plain_b = tiles("579m 248s 135p ESWB");
    let head_common = tiles("9p 1m BB 8p 1m");
    let wall_a2 = build_preset_wall(
        &east_base,
        &south_wait_1m_b,
        &west_plain_no_1m,
        &north_plain_b,
        &head_common,
        &tiles("9p"),
    )
    .unwrap();
    let wall_b2 = build_preset_wall(
        &east_base,
        &west_plain_no_1m,
        &south_wait_1m_b,
        &north_plain_b,
        &head_common,
        &tiles("9p"),
    )
    .unwrap();
    let drive_to_rob_window = |wall: Vec<Tile>| {
        let mut g = Match::new(
            MatchMode::EastOnly,
            TuidaohuRuleset,
            PresetWallProvider::single(Seat::East, wall),
        )
        .unwrap();
        g.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("F"),
                from_drawn: false,
            },
        )
        .unwrap();
        let s_d = project_view(&g, Seat::South).hand.drawn_tile.unwrap();
        g.apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d,
                from_drawn: true,
            },
        )
        .unwrap();
        g.apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
        g.apply_action(Seat::East, PlayerAction::Pon { tile: t("1m") })
            .unwrap();
        g.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("C"),
                from_drawn: false,
            },
        )
        .unwrap();
        for seat in [Seat::South, Seat::West] {
            g.apply_action(
                seat,
                PlayerAction::Discard {
                    tile: t("N"),
                    from_drawn: false,
                },
            )
            .unwrap();
        }
        let n_d = project_view(&g, Seat::North).hand.drawn_tile.unwrap();
        g.apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: n_d,
                from_drawn: true,
            },
        )
        .unwrap();
        g
    };

    let mut g_south_rob = drive_to_rob_window(wall_a2);
    let mut g_west_rob = drive_to_rob_window(wall_b2);

    let east_before_rob = project_view(&g_south_rob, Seat::East);
    assert_eq!(east_before_rob.hand.drawn_tile, Some(t("1m")));
    assert!(east_before_rob
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: t("1m") }));
    let pre_seq = east_before_rob.seats[Seat::East.index()].last_draw_or_meld_seq;

    let ev_a = g_south_rob
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    let ev_b = g_west_rob
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    assert_eq!(ev_a.len(), 1);
    assert_eq!(ev_b.len(), 1);
    assert_eq!(
        project_event(&ev_a[0], Seat::North).kind,
        PlayerEventKind::AddedKongAttempted {
            seat: Seat::East,
            tile: t("1m"),
        }
    );

    // Zero leakage to unrelated seat North!
    assert_eq!(
        project_view(&g_south_rob, Seat::North),
        project_view(&g_west_rob, Seat::North)
    );

    for seat in Seat::ALL {
        let view = project_view(&g_south_rob, seat);
        assert_136_tile_conservation(&g_south_rob);
        assert_eq!(view.phase, PublicPhase::Playing);
        assert_eq!(view.turn.active_seat, Some(Seat::East));
        assert_eq!(
            view.seats[Seat::East.index()].last_draw_or_meld_seq,
            pre_seq,
            "AddedKongAttempted must NOT update last_draw_or_meld_seq"
        );
        assert!(matches!(
            view.seats[Seat::East.index()].melds.as_slice(),
            [MeldView::Pon {
                tile,
                from_seat: Seat::West,
                ..
            }] if *tile == t("1m")
        ));
        if seat == Seat::South {
            assert_eq!(
                view.legal_actions,
                vec![
                    PlayerAction::RobKongHu { tile: t("1m") },
                    PlayerAction::Pass
                ]
            );
        } else {
            assert!(view.legal_actions.is_empty());
        }
    }

    // Error matrix during AwaitingRobKong without mutating state.
    let snapshot_views: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&g_south_rob, s));
    let snapshot_seq = g_south_rob.seq();
    for non_eligible in [Seat::East, Seat::West, Seat::North] {
        assert!(matches!(
            g_south_rob
                .apply_action(non_eligible, PlayerAction::Pass)
                .unwrap_err(),
            MatchError::NotYourTurn { seat } if seat == non_eligible
        ));
    }
    assert!(matches!(
        g_south_rob
            .apply_action(Seat::South, PlayerAction::ConfirmNextHand)
            .unwrap_err(),
        MatchError::WrongPhase {
            seat: Seat::South,
            ..
        }
    ));
    assert!(matches!(
        g_south_rob
            .apply_action(Seat::South, PlayerAction::Ron { tile: t("1m") })
            .unwrap_err(),
        MatchError::WrongPhase {
            seat: Seat::South,
            ..
        }
    ));
    assert!(matches!(
        g_south_rob
            .apply_action(Seat::South, PlayerAction::RobKongHu { tile: t("2m") })
            .unwrap_err(),
        MatchError::IllegalAction {
            seat: Seat::South,
            ..
        }
    ));
    assert_eq!(
        Seat::ALL.map(|s| project_view(&g_south_rob, s)),
        snapshot_views
    );
    assert_eq!(g_south_rob.seq(), snapshot_seq);

    // South Passes -> records passed_hu_lock for South, completes AddedKong + draws tail 9p.
    let pass_ev = g_south_rob
        .apply_action(Seat::South, PlayerAction::Pass)
        .unwrap();
    assert_eq!(pass_ev.len(), 2);
    assert!(matches!(
        project_event(&pass_ev[0], Seat::South).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::AddedKong {
                tile,
                from_seat: Seat::West,
                ..
            },
            kong_score_deltas: Some([3, -1, -1, -1]),
        } if tile == t("1m")
    ));
    assert!(matches!(
        project_event(&pass_ev[1], Seat::East).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(repl),
            is_replacement: true,
            ..
        } if repl == t("9p")
    ));

    // South's passed_hu_lock blocks Ron { 4m }.
    let block_ev = g_south_rob
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("4m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(block_ev.len(), 2);
    assert!(matches!(
        project_event(&block_ev[1], Seat::South).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            is_replacement: false,
            ..
        }
    ));

    // Prior passed_hu_lock suppresses RobKongHu, completing AddedKong in one step.
    let east_rev2 = tiles("11156m 456s 789p CCF");
    let north_rev2 = tiles("479m 248s 135p ESWB");
    let wall_rev3 = build_preset_wall(
        &east_rev2,
        &south_wait_1m_b,
        &west_plain_no_1m,
        &north_rev2,
        &tiles("9p 1m B 8p W E"),
        &tiles("7m"),
    )
    .unwrap();
    let mut g_rev3 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_rev3),
    )
    .unwrap();
    g_rev3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("F"),
                from_drawn: false,
            },
        )
        .unwrap();
    let s_d = project_view(&g_rev3, Seat::South).hand.drawn_tile.unwrap();
    g_rev3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d,
                from_drawn: true,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("1m"),
                from_drawn: true,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(Seat::East, PlayerAction::Pon { tile: t("1m") })
        .unwrap();
    g_rev3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("C"),
                from_drawn: false,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("N"),
                from_drawn: false,
            },
        )
        .unwrap();
    let w_d = project_view(&g_rev3, Seat::West).hand.drawn_tile.unwrap();
    g_rev3
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: w_d,
                from_drawn: true,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("4m"),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&g_rev3, Seat::South).legal_actions,
        vec![PlayerAction::Ron { tile: t("4m") }, PlayerAction::Pass]
    );
    g_rev3
        .apply_action(Seat::South, PlayerAction::Pass)
        .unwrap();
    let one_step_ev = g_rev3
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    assert_eq!(one_step_ev.len(), 2);
    assert!(matches!(
        project_event(&one_step_ev[0], Seat::South).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::AddedKong { tile, .. },
            ..
        } if tile == t("1m")
    ));
    assert!(matches!(
        project_event(&one_step_ev[1], Seat::East).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(repl),
            is_replacement: true,
            ..
        } if repl == t("7m")
    ));
}

#[test]
fn slice4_rob_kong_hu_single_multi_ron_voids_added_kong_keeps_prior_kongs_and_dealer_streak() {
    // 1. Single RobKongHu after an earlier confirmed ConcealedKong.
    let east_1 = tiles("11456m 9999p 456s CF");
    let south_1 = tiles("178m 789s 123p ESWN");
    let west_1 = tiles("23m 123s 456p CCCBE");
    let north_1 = tiles("789m 789s 123p ESWN");
    let wall_1 = build_preset_wall(
        &east_1,
        &south_1,
        &west_1,
        &north_1,
        &tiles("78p B 4s 1m"),
        &tiles("3m"),
    )
    .unwrap();
    let mut g1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_1),
    )
    .unwrap();

    g1.apply_action(Seat::East, PlayerAction::ConcealedKong { tile: t("9p") })
        .unwrap();
    g1.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("F"),
            from_drawn: false,
        },
    )
    .unwrap();
    g1.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: t("1m"),
            from_drawn: false,
        },
    )
    .unwrap();
    g1.apply_action(Seat::East, PlayerAction::Pon { tile: t("1m") })
        .unwrap();
    g1.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t("3m"),
            from_drawn: false,
        },
    )
    .unwrap();
    let s_d1 = project_view(&g1, Seat::South).hand.drawn_tile.unwrap();
    g1.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: s_d1,
            from_drawn: true,
        },
    )
    .unwrap();
    g1.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: t("E"),
            from_drawn: false,
        },
    )
    .unwrap();
    let n_d = project_view(&g1, Seat::North).hand.drawn_tile.unwrap();
    g1.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: n_d,
            from_drawn: true,
        },
    )
    .unwrap();

    let attempt_ev = g1
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: t("1m") })
        .unwrap();
    assert_eq!(attempt_ev.len(), 1);
    assert!(matches!(
        project_event(&attempt_ev[0], Seat::West).kind,
        PlayerEventKind::AddedKongAttempted {
            seat: Seat::East,
            tile,
        } if tile == t("1m")
    ));
    assert_eq!(
        project_view(&g1, Seat::West).legal_actions,
        vec![
            PlayerAction::RobKongHu { tile: t("1m") },
            PlayerAction::Pass
        ]
    );

    let rob_ev = g1
        .apply_action(Seat::West, PlayerAction::RobKongHu { tile: t("1m") })
        .unwrap();
    assert_eq!(rob_ev.len(), 1);
    match project_event(&rob_ev[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert_eq!(
                settlement.kong_transfers,
                vec![KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor: Seat::East,
                    from_seat: None,
                    tile: t("9p"),
                    score_deltas: [6, -2, -2, -2],
                }]
            );
            assert_eq!(settlement.score_deltas, [2, -2, 2, -2]);
            assert_eq!(settlement.new_scores, [102, 98, 102, 98]);
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
                    assert_eq!(winners[0].win_kind, WinKind::RobKongHu);
                    assert_eq!(winners[0].total_fan, 2);
                    assert_eq!(winners[0].base_points, 4);
                    assert_eq!(
                        winners[0].items,
                        vec![
                            ScoreItem {
                                code: "menqing".to_string(),
                                label: "门前清".to_string(),
                                fan: 1,
                            },
                            ScoreItem {
                                code: "rob_kong_hu".to_string(),
                                label: "抢杠胡".to_string(),
                                fan: 1,
                            },
                        ]
                    );
                }
                other => panic!("expected Win, got {other:?}"),
            }
            assert_eq!(
                settlement.revealed_hands[Seat::East.index()].drawn_tile,
                Some(t("1m"))
            );
            assert!(settlement.revealed_hands[Seat::East.index()]
                .melds
                .iter()
                .any(|meld| matches!(meld, RevealedMeld::Pon { tile, .. } if *tile == t("1m"))));
            assert!(!settlement.revealed_hands[Seat::East.index()]
                .melds
                .iter()
                .any(|meld| matches!(meld, RevealedMeld::AddedKong { .. })));
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // 2. Multi-ron RobKongHu (一炮多响) + Dealer (East) wins -> 连庄.
    let east_multi = tiles("23s 2347m 234678p EN");
    let south_multi = tiles("11456s 567m 55p SSS");
    let west_multi = tiles("178s 4568m 347p FFN");
    let north_multi_13 = tiles("19m 9s 19p ESWNCFB 4m");
    let wall_multi = build_preset_wall(
        &east_multi,
        &south_multi,
        &west_multi,
        &north_multi_13,
        &tiles("34p 8m B E 1s"),
        &[],
    )
    .unwrap();
    let dummy_next_wall = wall_multi.clone();
    fn drive_multi_rob_window<R: Ruleset, W: WallProvider>(mut g: Match<R, W>) -> Match<R, W> {
        g.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("7m"),
                from_drawn: false,
            },
        )
        .unwrap();
        let s_d0 = project_view(&g, Seat::South).hand.drawn_tile.unwrap();
        g.apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d0,
                from_drawn: true,
            },
        )
        .unwrap();
        g.apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t("1s"),
                from_drawn: false,
            },
        )
        .unwrap();
        g.apply_action(Seat::South, PlayerAction::Pon { tile: t("1s") })
            .unwrap();
        g.apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t("S"),
                from_drawn: false,
            },
        )
        .unwrap();
        let w_d2 = project_view(&g, Seat::West).hand.drawn_tile.unwrap();
        g.apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: w_d2,
                from_drawn: true,
            },
        )
        .unwrap();
        g.apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t("4m"),
                from_drawn: false,
            },
        )
        .unwrap();
        g.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t("N"),
                from_drawn: false,
            },
        )
        .unwrap();
        g.apply_action(Seat::South, PlayerAction::AddedKong { tile: t("1s") })
            .unwrap();
        g
    }

    let mut g_multi = drive_multi_rob_window(
        Match::new(
            MatchMode::EastOnly,
            TuidaohuRuleset,
            PresetWallProvider::new(Seat::East, vec![wall_multi, dummy_next_wall.clone()]),
        )
        .unwrap(),
    );

    for seat in [Seat::North, Seat::East] {
        assert_eq!(
            project_view(&g_multi, seat).legal_actions,
            vec![
                PlayerAction::RobKongHu { tile: t("1s") },
                PlayerAction::Pass
            ]
        );
    }
    assert!(project_view(&g_multi, Seat::West).legal_actions.is_empty());
    assert!(project_view(&g_multi, Seat::South).legal_actions.is_empty());

    assert!(g_multi
        .apply_action(Seat::East, PlayerAction::RobKongHu { tile: t("1s") })
        .unwrap()
        .is_empty());
    assert!(project_view(&g_multi, Seat::East).legal_actions.is_empty());
    assert!(matches!(
        g_multi
            .apply_action(Seat::East, PlayerAction::Pass)
            .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::East }
    ));

    let multi_ev = g_multi
        .apply_action(Seat::North, PlayerAction::RobKongHu { tile: t("1s") })
        .unwrap();
    assert_eq!(multi_ev.len(), 1);
    match project_event(&multi_ev[0], Seat::South).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert!(settlement.kong_transfers.is_empty());
            assert_eq!(settlement.score_deltas, [4, -20, 0, 16]);
            assert_eq!(settlement.new_scores, [104, 80, 100, 116]);
            match settlement.outcome {
                HandOutcome::Win {
                    winners,
                    discarder,
                    winning_tile,
                } => {
                    assert_eq!(discarder, Some(Seat::South));
                    assert_eq!(winning_tile, t("1s"));
                    assert_eq!(winners.len(), 2);
                    assert_eq!(winners[0].seat, Seat::North);
                    assert_eq!(winners[0].win_kind, WinKind::RobKongHu);
                    assert_eq!(winners[0].total_fan, 4);
                    assert_eq!(winners[0].base_points, 16);
                    assert_eq!(winners[1].seat, Seat::East);
                    assert_eq!(winners[1].win_kind, WinKind::RobKongHu);
                    assert_eq!(winners[1].total_fan, 2);
                    assert_eq!(winners[1].base_points, 4);
                }
                other => panic!("expected Win, got {other:?}"),
            }
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    for seat in Seat::ALL {
        g_multi
            .apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
    let next_hand_view = project_view(&g_multi, Seat::East);
    assert_eq!(next_hand_view.turn.hand_number, 1);
    assert_eq!(next_hand_view.turn.dealer_streak, 1);
    assert_eq!(next_hand_view.turn.dealer, Seat::East);

    // 3. Two-eligible RobKongWindow via AnyRuleset: North Passes first, then East RobKongHus.
    let mut g_partial = drive_multi_rob_window(
        Match::new(
            MatchMode::EastOnly,
            AnyRuleset::default(),
            PresetWallProvider::single(Seat::East, dummy_next_wall),
        )
        .unwrap(),
    );

    let north_pass = g_partial
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert!(north_pass.is_empty());
    assert!(project_view(&g_partial, Seat::North)
        .legal_actions
        .is_empty());
    assert!(matches!(
        g_partial
            .apply_action(Seat::North, PlayerAction::RobKongHu { tile: t("1s") })
            .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::North }
    ));

    let east_rob = g_partial
        .apply_action(Seat::East, PlayerAction::RobKongHu { tile: t("1s") })
        .unwrap();
    assert_eq!(east_rob.len(), 1);
    match project_event(&east_rob[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert!(settlement.kong_transfers.is_empty());
            assert_eq!(settlement.score_deltas, [4, -4, 0, 0]);
            match settlement.outcome {
                HandOutcome::Win { winners, .. } => {
                    assert_eq!(winners.len(), 1);
                    assert_eq!(winners[0].seat, Seat::East);
                    assert_eq!(winners[0].win_kind, WinKind::RobKongHu);
                }
                other => panic!("expected Win, got {other:?}"),
            }
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }
}

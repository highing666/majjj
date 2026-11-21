use mahjong_core::{
    build_preset_wall, project_event, project_view, HandOutcome, KongKind, KongTransfer, Match,
    MatchError, MatchMode, MeldView, PlayerAction, PlayerEventKind, PlayerView, PresetWallProvider,
    PublicPhase, RevealedMeld, ScoreItem, Seat, Tile, TuidaohuRuleset, WinKind,
};

fn t(raw: u8) -> Tile {
    Tile::new(raw).expect("valid tile index")
}

fn m(rank: u8) -> Tile {
    assert!((1..=9).contains(&rank));
    t(rank - 1)
}

fn s(rank: u8) -> Tile {
    assert!((1..=9).contains(&rank));
    t(9 + rank - 1)
}

fn p(rank: u8) -> Tile {
    assert!((1..=9).contains(&rank));
    t(18 + rank - 1)
}

const EAST_W: Tile = match Tile::new(27) {
    Some(tile) => tile,
    None => unreachable!(),
};
const SOUTH_W: Tile = match Tile::new(28) {
    Some(tile) => tile,
    None => unreachable!(),
};
const WEST_W: Tile = match Tile::new(29) {
    Some(tile) => tile,
    None => unreachable!(),
};
const NORTH_W: Tile = match Tile::new(30) {
    Some(tile) => tile,
    None => unreachable!(),
};
const RED_D: Tile = match Tile::new(31) {
    Some(tile) => tile,
    None => unreachable!(),
};
const GREEN_D: Tile = match Tile::new(32) {
    Some(tile) => tile,
    None => unreachable!(),
};
const WHITE_D: Tile = match Tile::new(33) {
    Some(tile) => tile,
    None => unreachable!(),
};

fn assert_136_tile_conservation(view: &PlayerView) {
    let total_hands: usize = view.seats.iter().map(|s| usize::from(s.hand_count)).sum();
    let total_unclaimed_river: usize = view
        .seats
        .iter()
        .flat_map(|s| s.river.iter())
        .filter(|d| d.claimed_by.is_none())
        .count();
    let total_melds: usize = view
        .seats
        .iter()
        .flat_map(|s| s.melds.iter())
        .map(|meld| match meld {
            MeldView::Pon { .. } | MeldView::Chi { .. } => 3,
            MeldView::ExposedKong { .. }
            | MeldView::AddedKong { .. }
            | MeldView::ConcealedKong { .. } => 4,
        })
        .sum();
    assert_eq!(
        total_hands + total_unclaimed_river + total_melds + usize::from(view.wall_remaining),
        136,
        "136-tile conservation violated: hands={total_hands}, river={total_unclaimed_river}, melds={total_melds}, wall={}",
        view.wall_remaining
    );
}

#[test]
fn slice1_concealed_kong_sources_projection_menqing_never_robbable_and_zero_wall_guard() {
    // 1. InitialDealer with two 4-of-a-kinds emits ConcealedKong in deterministic tile order,
    // is never robbable (even when South waits on 1m!), projects Some(tile) to owner and None to opponents,
    // preserves 门前清, draws replacement from wall tail, and reveals concrete tile on HandSettled.
    // South holds a 13-tile hand waiting on 1m (even though East has all 4 copies of 1m!).
    let south_waiting_1m = vec![
        m(2),
        m(3),
        p(1),
        p(2),
        p(3),
        p(5),
        p(6),
        p(7),
        s(2),
        s(3),
        s(4),
        WHITE_D,
        WHITE_D,
    ];
    let west_13 = vec![
        m(4),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
        p(8),
        p(9),
        s(1),
        s(8),
        s(9),
        EAST_W,
        SOUTH_W,
    ];
    let north_13 = vec![
        m(4),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
        p(8),
        p(9),
        s(1),
        s(8),
        s(9),
        WEST_W,
        NORTH_W,
    ];
    // Tail draws: first RED_D? Wait: East has 4 RED_D, so tail draw is GREEN_D then WHITE_D (Wait: South has 2 WHITE_D -> 2 left).
    // Let's make East wait on GREEN_D pair after konging 1m and discarding 1 RED_D? Wait:
    // East after kong 1m has [p2,p3,p4,s5,s6,s7, RED_D x4] (10 closed tiles) + tail_draw[0].
    // If tail_draw[0] = GREEN_D, and East discards GREEN_D? No, let's test chained ReplacementDraw kong!
    // Actually: if East has 4x 1m and 4x RED_D, after kong 1m -> tail_draw[0] = NORTH_W -> ReplacementDraw ALSO sees ConcealedKong { RED_D }!
    // Then East declares ConcealedKong { RED_D } -> tail_draw[1] = WEST_W? No, if tail_draw[1] makes a win:
    // Wait: after 2 kongs (1m, RED_D), East needs 2 sets + 1 pair in closed tiles (6 + 2 = 8 tiles).
    // Currently p2,p3,p4 + s5,s6,s7 = 6 tiles, so 1 pair left -> if East had GREEN_D in place of 4th RED_D? No, then only one 4-of-a-kind.
    // To test BOTH deterministic order of two ConcealedKongs AND winning after ReplacementDraw:
    // Give East: 1m x4, RED_D x4, p2, p3, p4, s5, s6, GREEN_D (14 tiles).
    // 1) InitialDealer sees [ConcealedKong { 1m }, ConcealedKong { RED_D }].
    // 2) East declares ConcealedKong { 1m } -> tail_draw[0] = s(7) -> East has [p2,p3,p4,s5,s6,GREEN_D,RED_Dx4] + drawn s(7).
    //    Now in ReplacementDraw, East STILL has 4x RED_D in closed_tiles while drawn_tile = s(7)!
    // 3) East declares ConcealedKong { RED_D } (4 from closed_tiles, drawn_tile s(7) moves into sorted closed_tiles!)
    //    -> tail_draw[1] = GREEN_D -> East completes [p2,p3,p4] [s5,s6,s7] [GREEN_D, GREEN_D] + 2 ConcealedKongs -> Tsumo!
    let east_14 = vec![
        m(1),
        m(1),
        m(1),
        m(1),
        p(2),
        p(3),
        p(4),
        s(5),
        s(6),
        GREEN_D,
        RED_D,
        RED_D,
        RED_D,
        RED_D,
    ];
    let wall = build_preset_wall(
        &east_14,
        &south_waiting_1m,
        &west_13,
        &north_13,
        &[],
        &[s(7), GREEN_D],
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    let init_east = project_view(&game, Seat::East);
    assert_136_tile_conservation(&init_east);
    let initial_kongs: Vec<PlayerAction> = init_east
        .legal_actions
        .iter()
        .copied()
        .filter(|a| matches!(a, PlayerAction::ConcealedKong { .. }))
        .collect();
    // Deterministic tile order: 1m (index 0) before RED_D (index 31).
    assert_eq!(
        initial_kongs,
        vec![
            PlayerAction::ConcealedKong { tile: m(1) },
            PlayerAction::ConcealedKong { tile: RED_D },
        ]
    );

    // East declares ConcealedKong { 1m } on InitialDealer; NOT robbable by South!
    let kong1_events = game
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: m(1) })
        .unwrap();
    assert_eq!(kong1_events.len(), 2);

    // Event 0: MeldDeclared (ConcealedKong hides tile from opponents, exposes to East).
    let meld1_seq = kong1_events[0].seq();
    assert_eq!(
        project_event(&kong1_events[0], Seat::East).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::ConcealedKong {
                tile: Some(m(1)),
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
            tile: Some(s(7)),
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
    assert_136_tile_conservation(&view_east_1);
    assert_eq!(view_east_1.phase, PublicPhase::Playing);
    assert_eq!(view_east_1.turn.active_seat, Some(Seat::East));
    assert_eq!(view_east_1.wall_remaining, 82);
    assert_eq!(view_east_1.hand.drawn_tile, Some(s(7)));
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
            tile: Some(m(1)),
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

    // Chained ReplacementDraw ConcealedKong { RED_D } where all 4 RED_D are in closed_tiles and s(7) is in drawn_tile.
    assert!(view_east_1
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: RED_D }));
    let kong2_events = game
        .apply_action(Seat::East, PlayerAction::ConcealedKong { tile: RED_D })
        .unwrap();
    assert_eq!(kong2_events.len(), 2);

    let view_east_2 = project_view(&game, Seat::East);
    assert_136_tile_conservation(&view_east_2);
    assert_eq!(view_east_2.wall_remaining, 81);
    assert_eq!(view_east_2.hand.drawn_tile, Some(GREEN_D));
    // s(7) was moved from drawn_tile into sorted closed_tiles alongside p2,p3,p4,s5,s6,GREEN_D!
    assert_eq!(
        view_east_2.hand.closed_tiles,
        vec![s(5), s(6), s(7), p(2), p(3), p(4), GREEN_D]
    );
    assert!(view_east_2.legal_actions.contains(&PlayerAction::Tsumo));

    // East declares Tsumo on the 2nd replacement draw -> 门前清 (+1) + 杠上开花 (+1) = 2 fan (4 base points).
    // Win deltas: East +12, others -4 each. Kong deltas: 2 * (+6, -2, -2, -2) = (+12, -4, -4, -4).
    // Combined deltas: East +24, South/West/North -8 each -> new scores [124, 92, 92, 92].
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
                    tile: m(1),
                    score_deltas: [6, -2, -2, -2],
                }
            );
            assert_eq!(
                settlement.kong_transfers[1],
                KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor: Seat::East,
                    from_seat: None,
                    tile: RED_D,
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
                    assert_eq!(*winning_tile, GREEN_D);
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
                        tile: m(1),
                        seq: meld1_seq,
                    },
                    RevealedMeld::ConcealedKong {
                        tile: RED_D,
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
    // Tests:
    // 1. AfterMeld ConcealedKong (South Pons 1m, then immediately declares ConcealedKong { 2m } before discarding).
    // 2. NormalDraw ConcealedKong where the 4th tile is drawn_tile (West holds 3x 3m, draws 3m, ConcealedKongs 3m).
    // 3. ConcealedKong forbidden when wall_remaining == 0 (even when holding 4x 4m).
    let east_14 = vec![
        m(1),
        m(9),
        s(1),
        s(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
        m(8),
    ];
    let south_13 = vec![
        m(1),
        m(1),
        m(2),
        m(2),
        m(2),
        m(2),
        s(2),
        s(3),
        s(4),
        p(2),
        p(3),
        p(4),
        EAST_W,
    ];
    let west_13 = vec![
        m(3),
        m(3),
        m(3),
        m(4),
        m(4),
        m(4),
        m(4),
        s(5),
        s(6),
        s(7),
        p(5),
        p(6),
        p(7),
    ];
    let north_13 = vec![
        m(5),
        m(6),
        m(7),
        s(1),
        s(8),
        s(9),
        p(1),
        p(8),
        p(9),
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
    ];
    let wall = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &[m(3)],
        &[WHITE_D, GREEN_D],
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    // East discards 1m -> South Pons 1m -> enters AfterMeld with 4x 2m in closed_tiles!
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::South, PlayerAction::Pon { tile: m(1) })
        .unwrap();

    let south_after_meld = project_view(&game, Seat::South);
    assert_136_tile_conservation(&south_after_meld);
    assert!(south_after_meld
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: m(2) }));

    // South declares ConcealedKong { 2m } directly in AfterMeld (before discarding!).
    game.apply_action(Seat::South, PlayerAction::ConcealedKong { tile: m(2) })
        .unwrap();
    let south_after_kong = project_view(&game, Seat::South);
    assert_136_tile_conservation(&south_after_kong);
    assert_eq!(south_after_kong.hand.drawn_tile, Some(WHITE_D));
    assert_eq!(south_after_kong.wall_remaining, 82);
    // South discards WHITE_D (tsumogiri) -> West draws m(3) (head_draws[0]).
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: true,
        },
    )
    .unwrap();

    // West now holds 3x m(3) in closed_tiles + m(3) in drawn_tile AND 4x m(4) in closed_tiles!
    let west_view = project_view(&game, Seat::West);
    assert_eq!(west_view.hand.drawn_tile, Some(m(3)));
    assert!(west_view
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: m(3) }));
    assert!(west_view
        .legal_actions
        .contains(&PlayerAction::ConcealedKong { tile: m(4) }));

    // West declares ConcealedKong { m(3) } (consuming drawn_tile + 3 from closed_tiles).
    game.apply_action(Seat::West, PlayerAction::ConcealedKong { tile: m(3) })
        .unwrap();
    let west_after_kong = project_view(&game, Seat::West);
    assert_136_tile_conservation(&west_after_kong);
    assert_eq!(west_after_kong.hand.drawn_tile, Some(GREEN_D));
    assert!(!west_after_kong.hand.closed_tiles.contains(&m(3)));
    assert_eq!(west_after_kong.wall_remaining, 80);

    // West discards GREEN_D (tsumogiri) and drain the remaining 80 wall tiles without winning until wall_remaining == 0,
    // verifying West holding 4x m(4) does NOT get ConcealedKong { m(4) } at wall_remaining == 0.
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: GREEN_D,
            from_drawn: true,
        },
    )
    .unwrap();

    // Advance turns using safe discards (tsumogiri when available, or discarding a non-4m tile) until wall_remaining == 0.
    loop {
        let current = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .expect("playing seat should have legal action");
        let (seat, view) = current;
        if view.wall_remaining == 0 && seat == Seat::West {
            assert!(
                !view
                    .legal_actions
                    .iter()
                    .any(|a| matches!(a, PlayerAction::ConcealedKong { .. })),
                "ConcealedKong must be forbidden at wall_remaining == 0"
            );
            let before_seq = game.seq();
            assert!(matches!(
                game.apply_action(seat, PlayerAction::ConcealedKong { tile: m(4) })
                    .unwrap_err(),
                MatchError::IllegalAction {
                    seat: Seat::West,
                    action: PlayerAction::ConcealedKong { .. }
                }
            ));
            assert_eq!(game.seq(), before_seq);
            break;
        }
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game.apply_action(seat, action).unwrap();
    }
}

#[test]
fn slice2_exposed_kong_execution_no_pao_hu_priority_passed_pon_independence_and_no_gangshangpao() {
    // 1. Sub-case A: ExposedKong execution, claimed_by, broken 门前清, no 明杠 pao on 杠上开花,
    // and Pass on Pon/ExposedKong locks Pon but NOT ExposedKong.
    let east_14 = vec![
        m(1),
        m(7),
        s(1),
        s(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
        WHITE_D,
    ];
    let south_13 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(2),
        s(4),
        s(6),
        p(2),
        p(4),
        p(6),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    // West holds 3x 1m + 2m,3m,4m + 5s,6s,7s + 5p,6p,7p + RED_D; tail draw RED_D completes Tsumo!
    // No 门前清 (broken by ExposedKong), no 碰碰胡/混一色/清一色 -> ONLY 杠上开花 (+1 fan, 2 base points).
    let west_13 = vec![
        m(1),
        m(1),
        m(1),
        m(2),
        m(3),
        m(4),
        s(5),
        s(6),
        s(7),
        p(5),
        p(6),
        p(7),
        RED_D,
    ];
    // North wins on 1m Ron (waiting on m(2),m(3) -> 1m), so East discarding 1m offers both West ExposedKong and North Ron!
    let north_13 = vec![
        m(2),
        m(3),
        s(2),
        s(3),
        s(4),
        s(8),
        s(8),
        s(8),
        p(3),
        p(4),
        p(5),
        GREEN_D,
        GREEN_D,
    ];
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[RED_D]).unwrap();

    // Test A1: West submits ExposedKong first, North submits Pass -> resolves to ExposedKong -> West replacement Tsumo (no 明杠 pao!).
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    let west_offered = project_view(&game_a, Seat::West).legal_actions;
    assert_eq!(
        west_offered,
        vec![
            PlayerAction::Pon { tile: m(1) },
            PlayerAction::ExposedKong { tile: m(1) },
            PlayerAction::Pass,
        ]
    );
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );

    // West submits ExposedKong first; must wait for North (Hu > PonOrKong).
    let pending_events = game_a
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: m(1) })
        .unwrap();
    assert!(pending_events.is_empty());
    assert!(project_view(&game_a, Seat::West).legal_actions.is_empty());

    // North Passes -> North locks 过水胡, West completes ExposedKong + draws RED_D from tail!
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
                tile: m(1),
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
            tile: Some(RED_D),
            is_replacement: true,
            wall_remaining: 82,
        }
    );

    let west_after = project_view(&game_a, Seat::West);
    assert_136_tile_conservation(&west_after);
    assert_eq!(
        west_after.seats[Seat::East.index()].river[0].claimed_by,
        Some(Seat::West)
    );
    assert_eq!(
        west_after.seats[Seat::West.index()].last_draw_or_meld_seq,
        draw_seq
    );
    assert!(west_after.legal_actions.contains(&PlayerAction::Tsumo));

    // West Tsumos -> 杠上开花 (+1 fan, 2 base points), 3-way split (NO 明杠 pao!):
    // Win: East -2, South -2, West +6, North -2.
    // Kong: East -2, West +2.
    // Net: [-4, -2, +8, -2].
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
                    tile: m(1),
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

    // Test A2: West submits ExposedKong first, North submits Ron -> Hu beats ExposedKong!
    let mut game_b = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall.clone()),
    )
    .unwrap();
    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: m(1) })
        .unwrap();
    let ron_ev = game_b
        .apply_action(Seat::North, PlayerAction::Ron { tile: m(1) })
        .unwrap();
    assert_eq!(ron_ev.len(), 1);
    match project_event(&ron_ev[0], Seat::West).kind {
        PlayerEventKind::HandSettled { settlement } => {
            assert!(settlement.kong_transfers.is_empty());
            assert_eq!(settlement.score_deltas, [-2, 0, 0, 2]);
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // Test A3: Passing Pon { 1m } locks Pon { 1m } via 过水碰, but does NOT block ExposedKong { 7m }
    // when South immediately discards 7m before West draws!
    let east_c = vec![
        m(1),
        m(9),
        s(1),
        s(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
        WHITE_D,
    ];
    let west_c = vec![
        m(1),
        m(1),
        m(7),
        m(7),
        m(7),
        m(2),
        m(3),
        s(5),
        s(6),
        s(7),
        p(5),
        p(6),
        RED_D,
    ];
    let wall_c =
        build_preset_wall(&east_c, &south_13, &west_c, &north_13, &[m(7)], &[RED_D]).unwrap();
    let mut game_c = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_c),
    )
    .unwrap();
    // East discards 1m -> West passes Pon { 1m }, North passes Ron { 1m } -> South draws head_draws[0] = m(7)!
    game_c
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    game_c.apply_action(Seat::West, PlayerAction::Pass).unwrap();
    game_c
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();

    // South discards m(7) (tsumogiri). West still hasn't drawn, holds 3x m(7), and gets Pon + ExposedKong!
    game_c
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: m(7),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_c, Seat::West).legal_actions,
        vec![
            PlayerAction::Pon { tile: m(7) },
            PlayerAction::ExposedKong { tile: m(7) },
            PlayerAction::Pass,
        ]
    );
    game_c
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: m(7) })
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
    // 2. Last-tile replacement Tsumo at wall_remaining == 0 stacks 杠上开花 + 海底捞月 (capped at 4).
    // 3. ExposedKong is forbidden at wall_remaining == 0.
    let east_14 = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
        s(1),
        s(9),
        p(1),
        p(9),
        EAST_W,
    ];
    let south_13 = vec![
        m(2),
        m(3),
        s(2),
        s(3),
        s(4),
        s(7),
        s(8),
        s(9),
        p(2),
        p(3),
        p(4),
        WHITE_D,
        WHITE_D,
    ];
    // West has 3x s(1) + 4x p(5)? Wait: West has 3x s(1), s(5), s(6), s(7), p(6), p(7), p(8), RED_D, RED_D, GREEN_D, NORTH_W.
    // After ExposedKong s(1), West draws tail WHITE_D -> discards m(1)? No, West has no m(1).
    // Give West m(1) so West discards m(1) after replacement draw -> South Rons m(1) (门前清 1 fan, NO 杠上炮!).
    let west_13 = vec![
        m(1),
        s(1),
        s(1),
        s(1),
        s(5),
        s(6),
        s(7),
        p(6),
        p(7),
        p(8),
        RED_D,
        RED_D,
        NORTH_W,
    ];
    let north_13 = vec![
        m(4),
        m(5),
        m(6),
        p(5),
        p(5),
        s(2),
        s(3),
        s(4),
        SOUTH_W,
        WEST_W,
        GREEN_D,
        GREEN_D,
        EAST_W,
    ];
    let wall_no_gsp =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[WHITE_D]).unwrap();
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
                tile: s(1),
                from_drawn: false,
            },
        )
        .unwrap();
    game_gsp
        .apply_action(Seat::West, PlayerAction::ExposedKong { tile: s(1) })
        .unwrap();
    // West discards m(1) after replacement draw -> South Rons m(1)!
    game_gsp
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    let ron_events = game_gsp
        .apply_action(Seat::South, PlayerAction::Ron { tile: m(1) })
        .unwrap();
    match project_event(&ron_events[0], Seat::South).kind {
        PlayerEventKind::HandSettled { settlement } => {
            // West got +2 from East (ExposedKong), South got +2 from West (门前清 Ron only, NO 杠上炮!).
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

    // 2. Last-tile ExposedKong at wall_remaining == 1 -> replacement Tsumo stacks 杠上开花 + 海底捞月 (capped at 4)!
    // North holds full-flush bamboo all-pungs: 3x s(1), 3x s(3), 3x s(5), 3x s(7), s(9).
    // West holds the 4th s(1). When West draws at wall_remaining == 1 and discards s(1),
    // North calls ExposedKong { s(1) }, draws s(9) from wall tail (wall_remaining == 0), and Tsumos:
    // 清一色 (2) + 碰碰胡 (1) + 杠上开花 (1) + 海底捞月 (1) = raw 5 -> capped at 4 (16 base points)!
    let east_last = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
    ];
    let south_last = vec![
        m(1),
        m(4),
        m(7),
        p(2),
        p(5),
        p(8),
        s(2),
        s(4),
        s(6),
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let west_last = vec![
        m(2),
        m(5),
        m(8),
        p(3),
        p(6),
        p(9),
        s(1),
        s(8),
        s(8),
        EAST_W,
        SOUTH_W,
        WHITE_D,
        WHITE_D,
    ];
    let north_last_cap = vec![
        s(1),
        s(1),
        s(1),
        s(3),
        s(3),
        s(3),
        s(5),
        s(5),
        s(5),
        s(7),
        s(7),
        s(7),
        s(9),
    ];
    let wall_last_cap = build_preset_wall(
        &east_last,
        &south_last,
        &west_last,
        &north_last_cap,
        &[],
        &[s(9)],
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
                tile: EAST_W,
                from_drawn: false,
            },
        )
        .unwrap();

    loop {
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game_cap, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .unwrap();
        if view.wall_remaining == 1 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            // West discards s(1) from closed_tiles at wall_remaining == 1!
            game_cap
                .apply_action(
                    Seat::West,
                    PlayerAction::Discard {
                        tile: s(1),
                        from_drawn: false,
                    },
                )
                .unwrap();
            let north_view = project_view(&game_cap, Seat::North);
            assert_eq!(north_view.wall_remaining, 1);
            assert!(north_view
                .legal_actions
                .contains(&PlayerAction::ExposedKong { tile: s(1) }));
            game_cap
                .apply_action(Seat::North, PlayerAction::ExposedKong { tile: s(1) })
                .unwrap();
            let north_repl = project_view(&game_cap, Seat::North);
            assert_eq!(north_repl.wall_remaining, 0);
            assert_eq!(north_repl.hand.drawn_tile, Some(s(9)));
            assert!(north_repl.legal_actions.contains(&PlayerAction::Tsumo));
            let settled_ev = game_cap
                .apply_action(Seat::North, PlayerAction::Tsumo)
                .unwrap();
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
            break;
        }
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game_cap.apply_action(seat, action).unwrap();
    }

    // 2b. Uncapped last-tile replacement Tsumo: 杠上开花 (1) + 海底捞月 (1) = 2 fan (4 base points)!
    let south_last_uncapped = vec![
        m(1),
        m(4),
        m(7),
        p(2),
        p(5),
        p(8),
        s(2),
        s(9),
        s(6),
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let west_last_uncapped = vec![
        m(2),
        m(5),
        m(8),
        p(3),
        p(6),
        p(9),
        s(1),
        s(9),
        s(8),
        EAST_W,
        SOUTH_W,
        WHITE_D,
        WHITE_D,
    ];
    let north_last_uncapped = vec![
        s(1),
        s(1),
        s(1),
        m(2),
        m(3),
        m(4),
        p(4),
        p(5),
        p(6),
        s(3),
        s(4),
        s(5),
        s(9),
    ];
    let wall_last_uncapped = build_preset_wall(
        &east_last,
        &south_last_uncapped,
        &west_last_uncapped,
        &north_last_uncapped,
        &[],
        &[s(9)],
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
                tile: EAST_W,
                from_drawn: false,
            },
        )
        .unwrap();
    loop {
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game_uncapped, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .unwrap();
        if view.wall_remaining == 1 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            game_uncapped
                .apply_action(
                    Seat::West,
                    PlayerAction::Discard {
                        tile: s(1),
                        from_drawn: false,
                    },
                )
                .unwrap();
            game_uncapped
                .apply_action(Seat::North, PlayerAction::ExposedKong { tile: s(1) })
                .unwrap();
            let settled_ev = game_uncapped
                .apply_action(Seat::North, PlayerAction::Tsumo)
                .unwrap();
            match project_event(&settled_ev[0], Seat::North).kind {
                PlayerEventKind::HandSettled { settlement } => {
                    // Kong: West -2, North +2. Win (2 fan = 4 pts, 3-way): East -4, South -4, West -4, North +12.
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
            break;
        }
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game_uncapped.apply_action(seat, action).unwrap();
    }

    // 3. ExposedKong is forbidden when wall_remaining == 0!
    // Give East 3x s(1) and North the 4th s(1); drain to wall_remaining == 0 when North discards s(1).
    let east_zero = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        p(1),
        p(9),
        s(1),
        s(1),
        s(1),
        EAST_W,
        SOUTH_W,
        WEST_W,
    ];
    let north_zero = vec![
        m(7),
        m(8),
        m(9),
        p(2),
        p(4),
        p(6),
        s(1),
        s(3),
        s(5),
        s(7),
        RED_D,
        GREEN_D,
        WHITE_D,
    ];
    let west_zero = vec![
        m(2),
        m(5),
        m(8),
        p(3),
        p(6),
        p(9),
        s(4),
        s(8),
        s(9),
        EAST_W,
        SOUTH_W,
        WHITE_D,
        WHITE_D,
    ];
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
                tile: EAST_W,
                from_drawn: false,
            },
        )
        .unwrap();
    loop {
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game_zero, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .unwrap();
        if view.wall_remaining == 0 && seat == Seat::North {
            let ev = game_zero
                .apply_action(
                    Seat::North,
                    PlayerAction::Discard {
                        tile: s(1),
                        from_drawn: false,
                    },
                )
                .unwrap();
            // Even though East holds 3x s(1), at wall_remaining == 0 ExposedKong & Pon are forbidden,
            // and East isn't waiting on s(1) for Ron -> settles directly as DrawGame!
            assert!(matches!(
                project_event(ev.last().unwrap(), Seat::East).kind,
                PlayerEventKind::HandSettled {
                    settlement: mahjong_core::HandSettlement {
                        outcome: HandOutcome::DrawGame,
                        ..
                    }
                }
            ));
            break;
        }
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game_zero.apply_action(seat, action).unwrap();
    }
}

#[test]
fn slice3_added_kong_one_step_sources_ordering_after_meld_and_draw_game_keeps_kong_transfers() {
    // 1. NormalDraw AddedKong (4th tile == drawn_tile), one-step (no AddedKongAttempted),
    // preserves original Pon.from_seat, pays 3 others 1 each, and replacement Tsumo gets 杠上开花.
    let east_14 = vec![
        m(1),
        m(7),
        s(1),
        s(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
        WHITE_D,
    ];
    let south_13 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(2),
        s(4),
        s(6),
        p(2),
        p(4),
        p(6),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    // West has 2x 1m + 2m,3m,4m + 5s,6s,7s + 5p,6p,7p + RED_D, GREEN_D.
    // East discards 1m -> West Pons 1m (from East) -> discards GREEN_D.
    // South draws p(8) & discards -> West draws 1m (head_draws[1]) -> AddedKong { 1m } -> tail draw RED_D -> Tsumo!
    let west_13 = vec![
        m(1),
        m(1),
        m(2),
        m(3),
        m(4),
        s(5),
        s(6),
        s(7),
        p(5),
        p(6),
        p(7),
        RED_D,
        GREEN_D,
    ];
    let north_13 = vec![
        m(3),
        m(5),
        m(7),
        s(3),
        s(8),
        s(9),
        p(3),
        p(8),
        p(9),
        SOUTH_W,
        WEST_W,
        NORTH_W,
        NORTH_W,
    ];
    let wall_a = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &[p(8), m(3), m(5), m(1)],
        &[RED_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    game_a
        .apply_action(Seat::West, PlayerAction::Pon { tile: m(1) })
        .unwrap();
    game_a
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: GREEN_D,
                from_drawn: false,
            },
        )
        .unwrap();
    for seat in [Seat::North, Seat::East, Seat::South] {
        let view = project_view(&game_a, seat);
        let drawn = view.hand.drawn_tile.unwrap();
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
    assert_eq!(west_turn.hand.drawn_tile, Some(m(1)));
    assert!(west_turn
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: m(1) }));

    // Nobody can RobKongHu on m(1) -> completes in one step with [MeldDeclared, TileDrawn] (NO AddedKongAttempted!).
    let added_ev = game_a
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    assert_eq!(added_ev.len(), 2);
    let meld_seq = added_ev[0].seq();
    let draw_seq = added_ev[1].seq();
    assert_eq!(
        project_event(&added_ev[0], Seat::North).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::West,
            meld: MeldView::AddedKong {
                tile: m(1),
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
            tile: Some(RED_D),
            is_replacement: true,
            wall_remaining: 78,
        }
    );

    let west_repl = project_view(&game_a, Seat::West);
    assert_136_tile_conservation(&west_repl);
    assert_eq!(
        west_repl.seats[Seat::West.index()].last_draw_or_meld_seq,
        draw_seq
    );
    assert_eq!(
        west_repl.seats[Seat::West.index()].melds,
        vec![MeldView::AddedKong {
            tile: m(1),
            from_seat: Seat::East,
            seq: meld_seq,
        }]
    );
    assert!(west_repl.legal_actions.contains(&PlayerAction::Tsumo));

    // Tsumo on replacement draw: 杠上开花 (1 fan, 2 base points) -> win [+6, -2, -2, -2] for West ->
    // plus AddedKong [-1, -1, +3, -1] = [-3, -3, +9, -3]!
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
                    tile: m(1),
                    score_deltas: [-1, -1, 3, -1],
                }]
            );
            assert_eq!(settlement.score_deltas, [-3, -3, 9, -3]);
            assert_eq!(settlement.new_scores, [97, 97, 109, 97]);
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // 2. Multiple Pons + AfterMeld AddedKong + ReplacementDraw AddedKong (4th tile in closed_tiles) +
    // deterministic tile order + wall_remaining == 0 forbids AddedKong + DrawGame retains kong_transfers (荒庄不荒杠).
    let east_b = vec![
        s(9),
        m(3),
        m(2),
        m(4),
        m(6),
        m(8),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let south_b = vec![
        m(9),
        m(3),
        m(5),
        m(7),
        s(2),
        s(4),
        s(6),
        p(2),
        p(4),
        p(6),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    // West starts with 3x s(9) and 3x m(1):
    // East discards s(9) -> West Pons s(9) -> West discards NORTH_W.
    // North draws m(9), East draws m(1) and discards m(1) -> West Pons m(1)!
    // Now in AfterMeld, West has melds=[Pon(s9), Pon(m1)] and closed_tiles containing BOTH s(9) and m(1)!
    let west_b = vec![
        s(9),
        s(9),
        s(9),
        m(1),
        m(1),
        m(1),
        s(1),
        s(3),
        s(5),
        p(3),
        p(5),
        p(7),
        NORTH_W,
    ];
    let north_b = vec![
        m(2),
        m(4),
        m(6),
        s(7),
        s(8),
        p(8),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        RED_D,
        GREEN_D,
        WHITE_D,
    ];
    let wall_b = build_preset_wall(
        &east_b,
        &south_b,
        &west_b,
        &north_b,
        // After West Pons s(9) and discards NORTH_W: North draws [0]=m(9), East draws [1]=m(1) and discards m(1)!
        &[m(9), m(1)],
        &[p(5), p(7)],
    )
    .unwrap();
    let mut game_b = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b),
    )
    .unwrap();
    // 1. East discards s(9) -> West Pons s(9) -> West discards NORTH_W.
    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: s(9),
                from_drawn: false,
            },
        )
        .unwrap();
    game_b
        .apply_action(Seat::West, PlayerAction::Pon { tile: s(9) })
        .unwrap();
    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: NORTH_W,
                from_drawn: false,
            },
        )
        .unwrap();
    // 2. North draws m(9) and tsumogiris; East draws m(1) and discards m(1) -> West Pons m(1)!
    game_b
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(9),
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(Seat::West, PlayerAction::Pon { tile: m(1) })
        .unwrap();

    // West is now in AfterMeld with Pon(s9) and Pon(m1), and closed_tiles contains both m(1) and s(9).
    // Deterministic tile order must emit AddedKong { m(1) } BEFORE AddedKong { s(9) }!
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
            PlayerAction::AddedKong { tile: m(1) },
            PlayerAction::AddedKong { tile: s(9) },
        ]
    );

    // West declares AddedKong { s(9) } in AfterMeld -> draws tail[0] = p(5).
    game_b
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: s(9) })
        .unwrap();
    let west_after_k1 = project_view(&game_b, Seat::West);
    assert_136_tile_conservation(&west_after_k1);
    assert_eq!(west_after_k1.hand.drawn_tile, Some(p(5)));
    assert!(west_after_k1
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: m(1) }));

    // West declares AddedKong { m(1) } in ReplacementDraw (with drawn_tile == Some(p(5)) != m(1)) ->
    // p(5) is sorted into closed_tiles, m(1) is removed, draws tail[1] = p(7)!
    game_b
        .apply_action(Seat::West, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    let west_after_k2 = project_view(&game_b, Seat::West);
    assert_136_tile_conservation(&west_after_k2);
    assert_eq!(west_after_k2.hand.drawn_tile, Some(p(7)));
    assert!(!west_after_k2.hand.closed_tiles.contains(&m(1)));
    assert!(west_after_k2.hand.closed_tiles.contains(&p(5)));
    assert_eq!(
        west_after_k2.seats[Seat::West.index()]
            .melds
            .iter()
            .filter(|m| matches!(m, MeldView::AddedKong { .. }))
            .count(),
        2
    );

    // Finish hand to DrawGame (pass any reaction, tsumogiri non-winning draws) and verify 荒庄不荒杠!
    // First West discards p(7) (tsumogiri).
    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: p(7),
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
            // 2 AddedKongs by West: +6 for West, -2 each for East, South, North!
            assert_eq!(settlement.score_deltas, [-2, -2, 6, -2]);
            assert_eq!(settlement.new_scores, [-2, -2, 6, -2]);
            break;
        }
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game_b, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .unwrap();
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game_b.apply_action(seat, action).unwrap();
    }

    // 3. AddedKong is forbidden when wall_remaining == 0!
    // Give North s(9) (instead of East) so North discards s(9) on turn 1 -> West Pons s(9) ->
    // West draws every 4k tiles down to wall_remaining == 0 (TurnAction) and cannot AddedKong { s(9) }!
    let mut east_c = east_b.clone();
    east_c[0] = m(7);
    let mut north_c = north_b.clone();
    north_c[0] = s(9);
    let wall_c = build_preset_wall(
        &east_c,
        &south_b,
        &west_b,
        &north_c,
        &[p(8), p(8), p(9)],
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
                tile: m(7),
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
                tile: s(9),
                from_drawn: false,
            },
        )
        .unwrap();
    game_c
        .apply_action(Seat::West, PlayerAction::Pon { tile: s(9) })
        .unwrap();
    game_c
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: NORTH_W,
                from_drawn: false,
            },
        )
        .unwrap();
    loop {
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(&game_c, s)))
            .find(|(_, v)| !v.legal_actions.is_empty())
            .unwrap();
        if view.wall_remaining == 0 && seat == Seat::West && view.hand.drawn_tile.is_some() {
            assert!(
                !view
                    .legal_actions
                    .iter()
                    .any(|a| matches!(a, PlayerAction::AddedKong { .. })),
                "AddedKong must be forbidden at wall_remaining == 0"
            );
            let before_seq = game_c.seq();
            assert!(matches!(
                game_c
                    .apply_action(seat, PlayerAction::AddedKong { tile: s(9) })
                    .unwrap_err(),
                MatchError::IllegalAction {
                    seat: Seat::West,
                    action: PlayerAction::AddedKong { .. }
                }
            ));
            assert_eq!(game_c.seq(), before_seq);
            break;
        }
        let action = if view.legal_actions.contains(&PlayerAction::Pass) {
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
        };
        game_c.apply_action(seat, action).unwrap();
    }
}

#[test]
fn slice4_added_kong_awaiting_rob_kong_zero_leakage_error_matrix_and_pass_locks() {
    // 1. Zero leakage during AwaitingRobKong + error matrix + 136-tile conservation +
    // Pass records passed_hu_lock (blocking subsequent Ron) and all-Pass completes AddedKong.
    let east_base = vec![
        m(1),
        m(1),
        m(4),
        m(5),
        m(6),
        s(4),
        s(5),
        s(6),
        p(7),
        p(8),
        p(9),
        RED_D,
        RED_D,
        GREEN_D,
    ];
    // Not tenpai initially (holds NORTH_W instead of 2nd WHITE_D); becomes tenpai for m(1)/m(4)
    // when drawing WHITE_D and discarding NORTH_W AFTER East's Pon(1m)!
    let south_wait_1m_b = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(1),
        p(2),
        p(3),
        p(4),
        p(5),
        p(6),
        WHITE_D,
        NORTH_W,
    ];
    let west_plain_no_1m = vec![
        m(7),
        m(8),
        m(9),
        s(7),
        s(8),
        s(9),
        p(2),
        p(4),
        p(6),
        SOUTH_W,
        WEST_W,
        NORTH_W,
        EAST_W,
    ];
    let north_plain_b = vec![
        m(5),
        m(7),
        m(9),
        s(2),
        s(4),
        s(8),
        p(1),
        p(3),
        p(5),
        EAST_W,
        SOUTH_W,
        WEST_W,
        WHITE_D,
    ];
    // Flow:
    // - East discards GREEN_D.
    // - South draws head[0]=p(9) & tsumogiris.
    // - West draws head[1]=m(1) & tsumogiris -> East Pons m(1), discards RED_D.
    // - South draws head[2]=WHITE_D, discards NORTH_W; West draws head[3]=WHITE_D, discards NORTH_W.
    //   (Depending on who holds south_wait_1m_b, only South in g_south_rob or West in g_west_rob becomes tenpai!)
    // - North draws head[4]=p(8) & tsumogiris.
    // - East draws head[5]=m(1) -> AddedKong { m(1) }!
    let head_common = &[p(9), m(1), WHITE_D, WHITE_D, p(8), m(1)];
    let wall_a2 = build_preset_wall(
        &east_base,
        &south_wait_1m_b,
        &west_plain_no_1m,
        &north_plain_b,
        head_common,
        &[p(9)],
    )
    .unwrap();
    let wall_b2 = build_preset_wall(
        &east_base,
        &west_plain_no_1m,
        &south_wait_1m_b,
        &north_plain_b,
        head_common,
        &[p(9)],
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
                tile: GREEN_D,
                from_drawn: false,
            },
        )
        .unwrap();
        // South draws head[0]=p(9) -> discards
        let s_d = project_view(&g, Seat::South).hand.drawn_tile.unwrap();
        g.apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d,
                from_drawn: true,
            },
        )
        .unwrap();
        // West draws head[1]=m(1) -> discards -> East Pons m(1) -> East discards RED_D
        g.apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: true,
            },
        )
        .unwrap();
        g.apply_action(Seat::East, PlayerAction::Pon { tile: m(1) })
            .unwrap();
        g.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: RED_D,
                from_drawn: false,
            },
        )
        .unwrap();
        // South & West both draw WHITE_D and discard NORTH_W; North draws p(8) and tsumogiris.
        for seat in [Seat::South, Seat::West] {
            g.apply_action(
                seat,
                PlayerAction::Discard {
                    tile: NORTH_W,
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
    assert_eq!(east_before_rob.hand.drawn_tile, Some(m(1)));
    assert!(east_before_rob
        .legal_actions
        .contains(&PlayerAction::AddedKong { tile: m(1) }));
    let pre_seq = east_before_rob.seats[Seat::East.index()].last_draw_or_meld_seq;

    let ev_a = g_south_rob
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    let ev_b = g_west_rob
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    assert_eq!(ev_a.len(), 1);
    assert_eq!(ev_b.len(), 1);
    assert_eq!(
        project_event(&ev_a[0], Seat::North).kind,
        PlayerEventKind::AddedKongAttempted {
            seat: Seat::East,
            tile: m(1),
        }
    );

    // Zero leakage to unrelated seat North!
    assert_eq!(
        project_view(&g_south_rob, Seat::North),
        project_view(&g_west_rob, Seat::North)
    );

    // Check views while AwaitingRobKong is open:
    for seat in Seat::ALL {
        let view = project_view(&g_south_rob, seat);
        assert_136_tile_conservation(&view);
        assert_eq!(view.phase, PublicPhase::Playing);
        assert_eq!(view.turn.active_seat, Some(Seat::East));
        assert_eq!(
            view.seats[Seat::East.index()].last_draw_or_meld_seq,
            pre_seq,
            "AddedKongAttempted must NOT update last_draw_or_meld_seq"
        );
        // Pon remains un-upgraded and 4th tile remains in East's hand during AwaitingRobKong!
        assert!(matches!(
            view.seats[Seat::East.index()].melds.as_slice(),
            [MeldView::Pon {
                tile,
                from_seat: Seat::West,
                ..
            }] if *tile == m(1)
        ));
        if seat == Seat::South {
            assert_eq!(
                view.legal_actions,
                vec![PlayerAction::RobKongHu { tile: m(1) }, PlayerAction::Pass]
            );
        } else {
            assert!(view.legal_actions.is_empty());
        }
    }

    // Error matrix during AwaitingRobKong without mutating state!
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
            .apply_action(Seat::South, PlayerAction::Ron { tile: m(1) })
            .unwrap_err(),
        MatchError::WrongPhase {
            seat: Seat::South,
            ..
        }
    ));
    assert!(matches!(
        g_south_rob
            .apply_action(Seat::South, PlayerAction::RobKongHu { tile: m(2) })
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

    // South Passes -> records passed_hu_lock for South, completes AddedKong + draws tail p(9)!
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
        } if tile == m(1)
    ));
    assert!(matches!(
        project_event(&pass_ev[1], Seat::East).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(repl),
            is_replacement: true,
            ..
        } if repl == p(9)
    ));

    // Verify South's passed_hu_lock is active after passing RobKongHu:
    // East now discards m(4) (which would otherwise complete South's m2-m3 wait!),
    // and South's passed_hu_lock blocks Ron { m(4) }, advancing directly to South's draw!
    let block_ev = g_south_rob
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(4),
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

    // Also verify the reverse direction: prior passed_hu_lock (from passing a discard Ron)
    // suppresses RobKongHu, so AddedKong completes in one step with NO AddedKongAttempted!
    let east_rev2 = vec![
        m(1),
        m(1),
        m(1),
        m(5),
        m(6),
        s(4),
        s(5),
        s(6),
        p(7),
        p(8),
        p(9),
        RED_D,
        RED_D,
        GREEN_D,
    ];
    let north_rev2 = vec![
        m(4),
        m(7),
        m(9),
        s(2),
        s(4),
        s(8),
        p(1),
        p(3),
        p(5),
        EAST_W,
        SOUTH_W,
        WEST_W,
        WHITE_D,
    ];
    let wall_rev3 = build_preset_wall(
        &east_rev2,
        &south_wait_1m_b,
        &west_plain_no_1m,
        &north_rev2,
        &[p(9), m(1), WHITE_D, p(8), WEST_W, EAST_W],
        &[m(7)],
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
                tile: GREEN_D,
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
                tile: m(1),
                from_drawn: true,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(Seat::East, PlayerAction::Pon { tile: m(1) })
        .unwrap();
    g_rev3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: RED_D,
                from_drawn: false,
            },
        )
        .unwrap();
    g_rev3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: NORTH_W,
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
    // North discards m(4) -> South passes Ron { m(4) } -> East draws head[5]=EAST_W!
    g_rev3
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(4),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&g_rev3, Seat::South).legal_actions,
        vec![PlayerAction::Ron { tile: m(4) }, PlayerAction::Pass]
    );
    g_rev3
        .apply_action(Seat::South, PlayerAction::Pass)
        .unwrap();
    // South is now locked by passed_hu_lock, and East is in TurnAction with Pon(m(1)) + m(1) in closed_tiles!
    // East declares AddedKong { m(1) }: South CANNOT RobKongHu because of passed_hu_lock, so AddedKong completes in ONE step!
    let one_step_ev = g_rev3
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    assert_eq!(one_step_ev.len(), 2);
    assert!(matches!(
        project_event(&one_step_ev[0], Seat::South).kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::AddedKong { tile, .. },
            ..
        } if tile == m(1)
    ));
    assert!(matches!(
        project_event(&one_step_ev[1], Seat::East).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(repl),
            is_replacement: true,
            ..
        } if repl == m(7)
    ));
}

#[test]
fn slice4_rob_kong_hu_single_multi_ron_voids_added_kong_keeps_prior_kongs_and_dealer_streak() {
    // 1. Single RobKongHu after an earlier confirmed ConcealedKong:
    // - East declares ConcealedKong { p(9) } -> +6/-2 transfer recorded.
    // - East claims Pon(m(1)) from South, later draws 4th m(1) -> AddedKong { m(1) }.
    // - West submits RobKongHu { m(1) } -> voided AddedKong emits NO MeldDeclared, adds NO AddedKong transfer,
    //   keeps Pon(m(1)) in East's revealed melds and 4th m(1) in East's hand, preserves prior ConcealedKong transfer,
    //   charges East for RobKongHu (门前清 + 抢杠胡 = 2 fan, 4 pts), and rotates dealer to South!
    let east_1 = vec![
        m(1),
        m(1),
        p(9),
        p(9),
        p(9),
        p(9),
        m(4),
        m(5),
        m(6),
        s(4),
        s(5),
        s(6),
        RED_D,
        GREEN_D,
    ];
    let south_1 = vec![
        m(1),
        m(7),
        m(8),
        s(7),
        s(8),
        s(9),
        p(1),
        p(2),
        p(3),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
    ];
    let west_1 = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(4),
        p(5),
        p(6),
        RED_D,
        RED_D,
        RED_D,
        WHITE_D,
        EAST_W,
    ];
    let north_1 = vec![
        m(7),
        m(8),
        m(9),
        s(7),
        s(8),
        s(9),
        p(1),
        p(2),
        p(3),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
    ];
    // East flow:
    // - InitialDealer: ConcealedKong { p(9) } -> tail[0] = m(3) -> discards GREEN_D.
    // - South draws head[0]=p(7) & discards m(1) -> East Pons m(1), discards m(3).
    // - South draws head[1]=p(8) & tsumogiris.
    // - West draws head[2]=WHITE_D, discards EAST_W (now tenpai waiting on m(1)/m(4)!).
    // - North draws head[3]=s(4) & tsumogiris.
    // - East draws head[4]=m(1) -> AddedKong { m(1) }!
    let wall_1 = build_preset_wall(
        &east_1,
        &south_1,
        &west_1,
        &north_1,
        &[p(7), p(8), WHITE_D, s(4), m(1)],
        &[m(3)],
    )
    .unwrap();
    let mut g1 = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_1),
    )
    .unwrap();

    g1.apply_action(Seat::East, PlayerAction::ConcealedKong { tile: p(9) })
        .unwrap();
    g1.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: GREEN_D,
            from_drawn: false,
        },
    )
    .unwrap();
    g1.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    g1.apply_action(Seat::East, PlayerAction::Pon { tile: m(1) })
        .unwrap();
    g1.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: m(3),
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
            tile: EAST_W,
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

    // East attempts AddedKong { m(1) } -> opens AwaitingRobKong for West!
    let attempt_ev = g1
        .apply_action(Seat::East, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    assert_eq!(attempt_ev.len(), 1);
    assert!(matches!(
        project_event(&attempt_ev[0], Seat::West).kind,
        PlayerEventKind::AddedKongAttempted {
            seat: Seat::East,
            tile,
        } if tile == m(1)
    ));
    assert_eq!(
        project_view(&g1, Seat::West).legal_actions,
        vec![PlayerAction::RobKongHu { tile: m(1) }, PlayerAction::Pass]
    );

    // West submits RobKongHu { m(1) } -> settles immediately!
    let rob_ev = g1
        .apply_action(Seat::West, PlayerAction::RobKongHu { tile: m(1) })
        .unwrap();
    assert_eq!(rob_ev.len(), 1);
    match project_event(&rob_ev[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => {
            // Prior ConcealedKong (+6, -2, -2, -2) preserved; voided AddedKong NOT in kong_transfers!
            assert_eq!(
                settlement.kong_transfers,
                vec![KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor: Seat::East,
                    from_seat: None,
                    tile: p(9),
                    score_deltas: [6, -2, -2, -2],
                }]
            );
            // West RobKongHu: 门前清 (1) + 抢杠胡 (1) = 2 fan -> 4 pts from East.
            // Combined deltas: [6 - 4, -2, -2 + 4, -2] = [2, -2, 2, -2]!
            assert_eq!(settlement.score_deltas, [2, -2, 2, -2]);
            assert_eq!(settlement.new_scores, [102, 98, 102, 98]);
            match &settlement.outcome {
                HandOutcome::Win {
                    winners,
                    discarder,
                    winning_tile,
                } => {
                    assert_eq!(*discarder, Some(Seat::East));
                    assert_eq!(*winning_tile, m(1));
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
            // East's revealed hand keeps Pon(m(1)) (NOT AddedKong!) AND keeps the 4th m(1) in drawn_tile!
            assert_eq!(
                settlement.revealed_hands[Seat::East.index()].drawn_tile,
                Some(m(1))
            );
            assert!(settlement.revealed_hands[Seat::East.index()]
                .melds
                .iter()
                .any(|meld| matches!(meld, RevealedMeld::Pon { tile, .. } if *tile == m(1))));
            assert!(!settlement.revealed_hands[Seat::East.index()]
                .melds
                .iter()
                .any(|meld| matches!(meld, RevealedMeld::AddedKong { .. })));
        }
        other => panic!("expected HandSettled, got {other:?}"),
    }

    // 2. Multi-ron RobKongHu (一炮多响) + Dealer (East) wins -> 连庄!
    // South attempts AddedKong { s(1) }.
    // East (dealer) holds 门前清 waiting on s(1) -> 门前清 + 抢杠胡 = 2 fan (4 pts).
    // North holds 十三幺 missing s(1) -> 十三幺 (4) + 抢杠胡 (1) capped at 4 fan (16 pts).
    // South pays 4 + 16 = 20 pts!
    let east_multi = vec![
        s(2),
        s(3),
        m(2),
        m(3),
        m(4),
        p(2),
        p(3),
        p(4),
        p(6),
        p(7),
        p(8),
        EAST_W,
        NORTH_W,
        m(7),
    ];
    let south_multi = vec![
        s(1),
        s(1),
        s(4),
        s(5),
        s(6),
        m(5),
        m(6),
        m(7),
        p(5),
        p(5),
        SOUTH_W,
        SOUTH_W,
        SOUTH_W,
    ];
    let west_multi = vec![
        s(1),
        m(4),
        m(5),
        m(6),
        m(8),
        s(7),
        s(8),
        p(3),
        p(4),
        p(7),
        GREEN_D,
        GREEN_D,
        NORTH_W,
    ];
    let north_multi_13 = vec![
        m(1),
        m(9),
        s(9),
        p(1),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
        m(4),
    ];
    let wall_multi = build_preset_wall(
        &east_multi,
        &south_multi,
        &west_multi,
        &north_multi_13,
        &[p(3), p(4), m(8), WHITE_D, EAST_W, s(1)],
        &[],
    )
    .unwrap();
    let dummy_next_wall = wall_multi.clone();
    let mut g_multi = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, vec![wall_multi, dummy_next_wall.clone()]),
    )
    .unwrap();

    g_multi
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(7),
                from_drawn: false,
            },
        )
        .unwrap();
    // South draws head[0]=p(3) & tsumogiris; West draws head[1]=p(4) & discards s(1) -> South Pons s(1) & discards SOUTH_W.
    let s_d0 = project_view(&g_multi, Seat::South).hand.drawn_tile.unwrap();
    g_multi
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d0,
                from_drawn: true,
            },
        )
        .unwrap();
    g_multi
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: s(1),
                from_drawn: false,
            },
        )
        .unwrap();
    g_multi
        .apply_action(Seat::South, PlayerAction::Pon { tile: s(1) })
        .unwrap();
    g_multi
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: SOUTH_W,
                from_drawn: false,
            },
        )
        .unwrap();
    // West draws head[2]=m(8) & tsumogiris;
    let w_d2 = project_view(&g_multi, Seat::West).hand.drawn_tile.unwrap();
    g_multi
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: w_d2,
                from_drawn: true,
            },
        )
        .unwrap();
    // North draws head[3]=WHITE_D & discards m(4) (now tenpai for 十三幺 missing s(1)!);
    g_multi
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(4),
                from_drawn: false,
            },
        )
        .unwrap();
    // East draws head[4]=EAST_W & discards NORTH_W (now tenpai waiting on s(1)!).
    g_multi
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: NORTH_W,
                from_drawn: false,
            },
        )
        .unwrap();

    // South attempts AddedKong { s(1) } -> opens AwaitingRobKong for BOTH North and East!
    g_multi
        .apply_action(Seat::South, PlayerAction::AddedKong { tile: s(1) })
        .unwrap();
    assert_eq!(
        project_view(&g_multi, Seat::North).legal_actions,
        vec![PlayerAction::RobKongHu { tile: s(1) }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&g_multi, Seat::East).legal_actions,
        vec![PlayerAction::RobKongHu { tile: s(1) }, PlayerAction::Pass]
    );
    assert!(project_view(&g_multi, Seat::West).legal_actions.is_empty());
    assert!(project_view(&g_multi, Seat::South).legal_actions.is_empty());

    // East submits RobKongHu FIRST -> window stays open waiting for North (multi-ron)!
    let first_rob = g_multi
        .apply_action(Seat::East, PlayerAction::RobKongHu { tile: s(1) })
        .unwrap();
    assert!(first_rob.is_empty());
    assert!(project_view(&g_multi, Seat::East).legal_actions.is_empty());
    assert!(matches!(
        g_multi
            .apply_action(Seat::East, PlayerAction::Pass)
            .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::East }
    ));

    // North submits RobKongHu SECOND -> settles multi-ron in counter-clockwise order from South ([North, East])!
    let multi_ev = g_multi
        .apply_action(Seat::North, PlayerAction::RobKongHu { tile: s(1) })
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
                    assert_eq!(winning_tile, s(1));
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

    // Dealer (East) won -> confirm all 4 -> 连庄 (hand_number == 1, dealer_streak == 1, dealer == East)!
    for seat in Seat::ALL {
        g_multi
            .apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
    let next_hand_view = project_view(&g_multi, Seat::East);
    assert_eq!(next_hand_view.turn.hand_number, 1);
    assert_eq!(next_hand_view.turn.dealer_streak, 1);
    assert_eq!(next_hand_view.turn.dealer, Seat::East);

    // 3. Two-eligible RobKongWindow via AnyRuleset: North Passes first (window stays open!),
    // North cannot act again (NotYourTurn), then East submits RobKongHu -> settles for East only!
    let mut g_partial = Match::new(
        MatchMode::EastOnly,
        mahjong_core::AnyRuleset::default(),
        PresetWallProvider::single(Seat::East, dummy_next_wall),
    )
    .unwrap();
    g_partial
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(7),
                from_drawn: false,
            },
        )
        .unwrap();
    let s_d0_p = project_view(&g_partial, Seat::South)
        .hand
        .drawn_tile
        .unwrap();
    g_partial
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: s_d0_p,
                from_drawn: true,
            },
        )
        .unwrap();
    g_partial
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: s(1),
                from_drawn: false,
            },
        )
        .unwrap();
    g_partial
        .apply_action(Seat::South, PlayerAction::Pon { tile: s(1) })
        .unwrap();
    g_partial
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: SOUTH_W,
                from_drawn: false,
            },
        )
        .unwrap();
    let w_d2_p = project_view(&g_partial, Seat::West)
        .hand
        .drawn_tile
        .unwrap();
    g_partial
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: w_d2_p,
                from_drawn: true,
            },
        )
        .unwrap();
    g_partial
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(4),
                from_drawn: false,
            },
        )
        .unwrap();
    g_partial
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: NORTH_W,
                from_drawn: false,
            },
        )
        .unwrap();
    g_partial
        .apply_action(Seat::South, PlayerAction::AddedKong { tile: s(1) })
        .unwrap();

    let north_pass = g_partial
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert!(north_pass.is_empty());
    assert!(project_view(&g_partial, Seat::North)
        .legal_actions
        .is_empty());
    assert!(matches!(
        g_partial
            .apply_action(Seat::North, PlayerAction::RobKongHu { tile: s(1) })
            .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::North }
    ));

    let east_rob = g_partial
        .apply_action(Seat::East, PlayerAction::RobKongHu { tile: s(1) })
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

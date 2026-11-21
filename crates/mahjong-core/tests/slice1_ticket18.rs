use mahjong_core::{
    build_preset_wall, project_event, project_view, HandOutcome, Match, MatchError, MatchMode,
    MeldView, PlayerAction, PlayerEventKind, PlayerView, PresetWallProvider, PublicPhase,
    RevealedMeld, ScoreItem, Seat, Tile, TuidaohuRuleset, WinKind,
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

#[test]
fn slice1_precise_windowing_zero_leakage_and_all_pass_continuation() {
    let east_14 = vec![
        m(1),
        m(9),
        s(2),
        s(5),
        s(8),
        p(1),
        p(4),
        p(7),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let south_plain = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(1),
        s(4),
        s(7),
        p(2),
        p(5),
        p(8),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    let west_pon_1m = vec![
        m(1),
        m(1),
        m(3),
        m(5),
        m(7),
        s(3),
        s(6),
        s(9),
        p(3),
        p(6),
        p(9),
        WEST_W,
        NORTH_W,
    ];
    let north_plain = vec![
        m(2),
        m(5),
        m(8),
        s(1),
        s(5),
        s(9),
        p(1),
        p(4),
        p(8),
        RED_D,
        GREEN_D,
        WHITE_D,
        NORTH_W,
    ];

    // 1. Discard with no eligible reactions advances directly to next draw without opening a window.
    let direct_wall = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_plain,
        &[WHITE_D],
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
                tile: m(9),
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
        } if tile == m(9)
    ));
    assert!(matches!(
        project_event(&direct_events[1], Seat::South).kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            tile: Some(t_drawn),
            is_replacement: false,
            wall_remaining: 82,
        } if t_drawn == WHITE_D
    ));

    // 2. Discard opening reaction window: compare two matches where West vs South holds Pon { 1m }
    // to verify zero leakage to unrelated seat North.
    let wall_west_pon = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_plain,
        &[WHITE_D],
        &[],
    )
    .unwrap();
    let wall_south_pon = build_preset_wall(
        &east_14,
        &west_pon_1m,
        &south_plain,
        &north_plain,
        &[WHITE_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    let discard_south = game_south
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(1),
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
                vec![PlayerAction::Pon { tile: m(1) }, PlayerAction::Pass]
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
            .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
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
    let north_ron_1m = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(4),
        p(5),
        p(6),
        p(7),
        p(8),
        p(9),
        RED_D,
        RED_D,
    ];
    let two_pending_wall = build_preset_wall(
        &east_14,
        &south_plain,
        &west_pon_1m,
        &north_ron_1m,
        &[WHITE_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    assert_eq!(
        project_view(&two_pending, Seat::West).legal_actions,
        vec![PlayerAction::Pon { tile: m(1) }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&two_pending, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
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
        } if t_drawn == WHITE_D
    ));
    let south_after = project_view(&two_pending, Seat::South);
    assert_eq!(south_after.turn.active_seat, Some(Seat::South));
    assert_eq!(south_after.hand.drawn_tile, Some(WHITE_D));
}

#[test]
fn slice2_pon_execution_state_event_after_meld_and_loses_menqing() {
    let east_14 = vec![
        m(1),
        m(9),
        s(4),
        s(5),
        s(8),
        p(1),
        p(4),
        p(5),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let south_13 = vec![
        m(5),
        m(6),
        m(7),
        m(8),
        s(6),
        s(7),
        s(9),
        p(2),
        p(3),
        p(6),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    let west_13 = vec![
        m(1),
        m(1),
        m(2),
        m(3),
        m(4),
        s(1),
        s(2),
        s(3),
        p(7),
        p(8),
        p(9),
        WEST_W,
        RED_D,
    ];
    let north_13 = vec![
        m(5),
        m(6),
        m(8),
        s(4),
        s(5),
        s(9),
        p(1),
        p(4),
        p(6),
        GREEN_D,
        WHITE_D,
        NORTH_W,
        SOUTH_W,
    ];
    // Head draws after West's Pon discard: North draws p(2), East draws p(3), South draws s(8), West draws RED_D.
    let head_draws = vec![p(2), p(3), s(8), RED_D];
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
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();

    // West claims Pon { 1m }.
    let pon_events = game
        .apply_action(Seat::West, PlayerAction::Pon { tile: m(1) })
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
                        tile: m(1),
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
    assert_eq!(
        west_view.hand.closed_tiles,
        vec![
            m(2),
            m(3),
            m(4),
            s(1),
            s(2),
            s(3),
            p(7),
            p(8),
            p(9),
            WEST_W,
            RED_D
        ]
    );
    assert_eq!(west_view.seats[Seat::West.index()].hand_count, 11);
    assert_eq!(
        west_view.seats[Seat::West.index()].last_draw_or_meld_seq,
        meld_seq
    );
    assert_eq!(
        west_view.seats[Seat::West.index()].melds,
        vec![MeldView::Pon {
            tile: m(1),
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

    // 136-tile conservation check after Pon.
    let total_hands: usize = west_view
        .seats
        .iter()
        .map(|s| usize::from(s.hand_count))
        .sum();
    let total_unclaimed_river: usize = west_view
        .seats
        .iter()
        .flat_map(|s| s.river.iter())
        .filter(|d| d.claimed_by.is_none())
        .count();
    let total_melds: usize = west_view
        .seats
        .iter()
        .flat_map(|s| s.melds.iter())
        .map(|m| match m {
            MeldView::Pon { .. } | MeldView::Chi { .. } => 3,
            _ => 4,
        })
        .sum();
    assert_eq!(
        total_hands + total_unclaimed_river + total_melds + usize::from(west_view.wall_remaining),
        136
    );

    // West discards WEST_W in AfterMeld turn -> North draws p(2) and discards it -> East -> South -> West draws RED_D.
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: WEST_W,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: p(2),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: p(3),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: s(8),
            from_drawn: true,
        },
    )
    .unwrap();

    // West wins via Tsumo on RED_D; Pon broke 门前清 so score is 基本胡 (0 fan, 1 base point).
    let win_events = game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    let settlement = match project_event(&win_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement.score_deltas, [-1, -1, 3, -1]);
    assert_eq!(
        settlement.revealed_hands[Seat::West.index()].melds,
        vec![RevealedMeld::Pon {
            tile: m(1),
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
    let east_14 = vec![
        m(1),
        m(9),
        s(4),
        s(5),
        s(8),
        p(1),
        p(4),
        p(5),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let south_pon_1m = vec![
        m(1),
        m(1),
        m(5),
        m(6),
        m(7),
        s(6),
        s(7),
        s(9),
        p(2),
        p(3),
        p(6),
        EAST_W,
        WHITE_D,
    ];
    let west_ron_1m = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(7),
        p(7),
        p(8),
        p(8),
        p(9),
        p(9),
        RED_D,
        RED_D,
    ];
    let north_plain = vec![
        m(4),
        m(6),
        m(8),
        s(4),
        s(5),
        s(9),
        p(1),
        p(4),
        p(6),
        GREEN_D,
        WHITE_D,
        NORTH_W,
        SOUTH_W,
    ];

    // Sub-case A: West submits Ron immediately -> preempts South (Pon-only) with early resolution.
    let wall_a = build_preset_wall(
        &east_14,
        &south_pon_1m,
        &west_ron_1m,
        &north_plain,
        &[WHITE_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    assert_eq!(
        project_view(&game_a, Seat::South).legal_actions,
        vec![PlayerAction::Pon { tile: m(1) }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&game_a, Seat::West).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );

    // West wins immediately without waiting for South.
    let ron_events = game_a
        .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
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
            assert_eq!(*winning_tile, m(1));
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
        &[WHITE_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    let after_south_pon = game_b
        .apply_action(Seat::South, PlayerAction::Pon { tile: m(1) })
        .unwrap();
    assert!(after_south_pon.is_empty());
    assert!(project_view(&game_b, Seat::South).legal_actions.is_empty());
    assert_eq!(
        project_view(&game_b, Seat::West).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );

    let after_west_ron = game_b
        .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
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
        &[WHITE_D],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_c
        .apply_action(Seat::South, PlayerAction::Pon { tile: m(1) })
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
        } if tile == m(1)
    ));
    assert_eq!(
        project_view(&game_c, Seat::South).turn.active_seat,
        Some(Seat::South)
    );
}

#[test]
fn slice4_multi_ron_two_and_three_winners_with_dealer_streak() {
    // Part A: 2 Ron winners (West 1-fan, North 2-fan) + 1 Pon-only seat (South).
    let east_14 = vec![
        m(1),
        m(9),
        s(2),
        s(5),
        s(8),
        p(1),
        p(5),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
    ];
    let south_pon = vec![
        m(1),
        m(1),
        m(5),
        m(6),
        m(7),
        s(6),
        s(7),
        s(9),
        p(2),
        p(3),
        p(6),
        EAST_W,
        WHITE_D,
    ];
    let west_ron_1fan = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(7),
        p(7),
        p(8),
        p(8),
        p(9),
        p(9),
        RED_D,
        RED_D,
    ];
    let north_ron_2fan = vec![
        m(2),
        m(3),
        m(4),
        m(4),
        m(4),
        m(9),
        m(9),
        m(9),
        NORTH_W,
        NORTH_W,
        NORTH_W,
        GREEN_D,
        GREEN_D,
    ];
    let wall_a = build_preset_wall(
        &east_14,
        &south_pon,
        &west_ron_1fan,
        &north_ron_2fan,
        &[p(4)],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    // West submits Ron first -> MUST wait for North (also offered Ron), while South is Pon-only.
    let after_west = game_a
        .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
        .unwrap();
    assert!(after_west.is_empty());
    assert_eq!(
        project_view(&game_a, Seat::West).phase,
        PublicPhase::Playing
    );
    assert!(project_view(&game_a, Seat::West).legal_actions.is_empty());
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );
    assert_eq!(
        project_view(&game_a, Seat::South).legal_actions,
        vec![PlayerAction::Pon { tile: m(1) }, PlayerAction::Pass]
    );

    // North submits Ron -> resolves IMMEDIATELY without waiting for Pon-only South.
    let after_north = game_a
        .apply_action(Seat::North, PlayerAction::Ron { tile: m(1) })
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
            assert_eq!(*winning_tile, m(1));
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    assert!(game_a2
        .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
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
    let b_east_14 = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(1),
        p(2),
        p(3),
        EAST_W,
        EAST_W,
        EAST_W,
        RED_D,
        RED_D,
        WHITE_D,
    ];
    let b_south_13 = vec![
        m(1),
        m(4),
        m(5),
        m(6),
        s(4),
        s(5),
        s(6),
        p(4),
        p(5),
        p(6),
        SOUTH_W,
        SOUTH_W,
        SOUTH_W,
    ];
    let b_west_13 = vec![
        m(1),
        m(7),
        m(8),
        m(9),
        s(7),
        s(8),
        s(9),
        p(7),
        p(8),
        p(9),
        WEST_W,
        WEST_W,
        WEST_W,
    ];
    let b_north_13 = vec![
        m(1),
        m(2),
        m(5),
        m(9),
        s(2),
        s(6),
        s(9),
        p(2),
        p(5),
        p(8),
        NORTH_W,
        GREEN_D,
        RED_D,
    ];
    let wall_b1 = build_preset_wall(
        &b_east_14,
        &b_south_13,
        &b_west_13,
        &b_north_13,
        &[WHITE_D, WHITE_D, WHITE_D],
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
                tile: WHITE_D,
                from_drawn: false,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    game_b
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();

    // All three non-discarders (East, South, West) are offered Ron { 1m }.
    for seat in [Seat::East, Seat::South, Seat::West] {
        assert_eq!(
            project_view(&game_b, seat).legal_actions,
            vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
        );
    }

    // Submit in non-turn order: West -> East -> South.
    assert!(game_b
        .apply_action(Seat::West, PlayerAction::Ron { tile: m(1) })
        .unwrap()
        .is_empty());
    assert!(game_b
        .apply_action(Seat::East, PlayerAction::Ron { tile: m(1) })
        .unwrap()
        .is_empty());
    let settled_3 = game_b
        .apply_action(Seat::South, PlayerAction::Ron { tile: m(1) })
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
            assert_eq!(*winning_tile, m(1));
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
    // East holds p(4)*2 + WHITE_D*2; 0 copies of p(4) or WHITE_D exist in the 83-tile wall.
    let east_14 = vec![
        m(1),
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(1),
        p(2),
        p(3),
        p(4),
        p(4),
        WHITE_D,
        WHITE_D,
        NORTH_W,
    ];
    let south_13 = vec![
        m(4),
        m(6),
        m(8),
        s(4),
        s(6),
        s(8),
        p(4),
        p(6),
        p(8),
        SOUTH_W,
        RED_D,
        WHITE_D,
        EAST_W,
    ];
    let west_13 = vec![
        m(5),
        m(7),
        m(9),
        s(5),
        s(7),
        s(9),
        p(4),
        p(5),
        p(7),
        p(9),
        WEST_W,
        GREEN_D,
        RED_D,
    ];
    let north_13 = vec![
        m(5),
        m(7),
        m(9),
        s(5),
        s(7),
        s(9),
        p(5),
        p(7),
        p(9),
        NORTH_W,
        GREEN_D,
        SOUTH_W,
        WHITE_D,
    ];

    let wall_ron = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[]).unwrap();
    let mut game_ron = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_ron.clone()),
    )
    .unwrap();

    let play_to_last_discard = |game: &mut Match<TuidaohuRuleset, PresetWallProvider>| {
        // Discard 1 (East discards NORTH_W), then 82 tsumogiri discards so North draws tile #83.
        game.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: NORTH_W,
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
                tile: WHITE_D,
                from_drawn: false,
            },
        )
        .unwrap()
    };

    let last_discard_events = play_to_last_discard(&mut game_ron);
    assert_eq!(last_discard_events.len(), 1);

    // East has 2 copies of WHITE_D AND wins on WHITE_D, but Pon is forbidden at wall_remaining == 0.
    let east_window = project_view(&game_ron, Seat::East);
    assert_eq!(east_window.phase, PublicPhase::Playing);
    assert_eq!(east_window.wall_remaining, 0);
    assert_eq!(
        east_window.legal_actions,
        vec![PlayerAction::Ron { tile: WHITE_D }, PlayerAction::Pass]
    );

    let win_events = game_ron
        .apply_action(Seat::East, PlayerAction::Ron { tile: WHITE_D })
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
            assert_eq!(*winning_tile, WHITE_D);
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
    east_no_win[0] = m(9); // break m(1,2,3) so East has 2 WHITE_D (Pon-only) and cannot Ron.
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
    let east_14 = vec![
        m(1),
        m(9),
        s(4),
        s(5),
        s(8),
        p(1),
        p(3),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        GREEN_D,
        WHITE_D,
    ];
    let south_13 = vec![
        m(5),
        m(6),
        m(7),
        m(8),
        s(6),
        s(7),
        s(9),
        p(2),
        p(3),
        p(8),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    let west_13 = vec![
        m(4),
        m(5),
        m(7),
        m(8),
        s(4),
        s(6),
        s(8),
        p(1),
        p(2),
        p(9),
        WEST_W,
        NORTH_W,
        GREEN_D,
    ];
    let north_13 = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(4),
        p(5),
        p(6),
        p(7),
        p(7),
        RED_D,
        RED_D,
        RED_D,
    ];
    // South draws m(4), West draws p(7), East draws m(2).
    let wall_a = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &[m(4), p(7), m(2)],
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
                tile: m(1),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );
    let after_north_pass = game_a
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    assert_eq!(after_north_pass.len(), 1); // South draws m(4).

    // 2. South discards 4m (which would win for North) -> blocked by 过水胡, directly advances to West.
    let south_discard_4m = game_a
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: m(4),
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
                tile: p(7),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_discard_7p.len(), 1);
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: p(7) }, PlayerAction::Pass]
    );
    game_a
        .apply_action(Seat::North, PlayerAction::Pon { tile: p(7) })
        .unwrap();

    // North discards 3m in AfterMeld, waiting on 2m.
    game_a
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(3),
                from_drawn: false,
            },
        )
        .unwrap();

    // 4. East draws and discards 2m -> Pon cleared 过水胡 so North can Ron 2m!
    let east_discard_2m = game_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(2),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_discard_2m.len(), 1);
    assert_eq!(
        project_view(&game_a, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(2) }, PlayerAction::Pass]
    );
    let win_events = game_a
        .apply_action(Seat::North, PlayerAction::Ron { tile: m(2) })
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
        &[m(1)],
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
                tile: m(1),
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

    // Case B2: South passes 1m, draws WHITE_D (clears 过水胡), discards WHITE_D, West discards 1m -> South can Ron!
    let wall_b2 = build_preset_wall(
        &east_14,
        &south_waiting,
        &west_13,
        &north_filler,
        &[WHITE_D, m(1)],
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
                tile: m(1),
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
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    let west_1m = game_b2
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: m(1),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_1m.len(), 1);
    assert_eq!(
        project_view(&game_b2, Seat::South).legal_actions,
        vec![PlayerAction::Ron { tile: m(1) }, PlayerAction::Pass]
    );
    game_b2
        .apply_action(Seat::South, PlayerAction::Ron { tile: m(1) })
        .unwrap();
}

#[test]
fn slice6_passed_pon_lock_lifecycle() {
    let east_14 = vec![
        s(2),
        m(9),
        s(4),
        s(5),
        s(6),
        p(2),
        p(4),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let south_13 = vec![
        m(1),
        m(3),
        m(7),
        m(8),
        s(1),
        s(3),
        s(5),
        p(3),
        p(5),
        p(7),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    let west_13 = vec![
        m(2),
        m(7),
        m(8),
        s(1),
        s(4),
        s(6),
        p(3),
        p(5),
        p(6),
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
    ];
    let north_13 = vec![
        s(2),
        s(2),
        p(8),
        p(8),
        p(1),
        p(1),
        m(4),
        m(5),
        m(6),
        s(7),
        s(8),
        s(9),
        WEST_W,
    ];

    // Wall 1:
    // - East discards 2s -> North passes Pon { 2s } (acquires 过水碰 for 2s).
    // - South draws 2s and discards 2s -> North is NOT offered Pon { 2s }.
    // - West draws 8p and discards 8p -> North IS offered Pon { 8p } (per-tile), claims Pon, discards WEST_W.
    // - East draws 2s? Wait: 4th 2s isn't available if South already drew 2s!
    // So in Wall 1: South draws WHITE_D, West draws 8p, North Pon 8p -> East draws 2s (North now tenpai on 2s & 1p!)
    // -> North is STILL NOT offered Pon { 2s } (Pon 8p did NOT clear 过水碰), AND IS offered Ron { 2s }!
    let wall_1 = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &[WHITE_D, p(8), s(2)],
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
                tile: s(2),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: s(2) }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();

    game_1
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    let west_8p = game_1
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: p(8),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(west_8p.len(), 1);
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: p(8) }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Pon { tile: p(8) })
        .unwrap();
    game_1
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: WEST_W,
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
                tile: s(2),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_2s.len(), 1);
    assert_eq!(
        project_view(&game_1, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: s(2) }, PlayerAction::Pass]
    );
    game_1
        .apply_action(Seat::North, PlayerAction::Ron { tile: s(2) })
        .unwrap();

    // Wall 2: Verify same-round Pon { 2s } is blocked BEFORE North draws.
    let wall_2 = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[s(2)], &[]).unwrap();
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
                tile: s(2),
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
                tile: s(2),
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
        &[WHITE_D, WHITE_D, RED_D, s(2)],
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
                tile: s(2),
                from_drawn: false,
            },
        )
        .unwrap();
    game_3
        .apply_action(Seat::North, PlayerAction::Pass)
        .unwrap();
    game_3
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    game_3
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: WHITE_D,
                from_drawn: true,
            },
        )
        .unwrap();
    game_3
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: RED_D,
                from_drawn: true,
            },
        )
        .unwrap();

    // East draws and discards 2s -> North IS offered Pon { 2s } again!
    let east_2s_again = game_3
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: s(2),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(east_2s_again.len(), 1);
    assert_eq!(
        project_view(&game_3, Seat::North).legal_actions,
        vec![PlayerAction::Pon { tile: s(2) }, PlayerAction::Pass]
    );
    game_3
        .apply_action(Seat::North, PlayerAction::Pon { tile: s(2) })
        .unwrap();
}

#[test]
fn slice6_dual_ron_and_pon_offer_pon_does_not_lock_while_pass_locks_both() {
    let east_14 = vec![
        m(5),
        m(9),
        s(4),
        s(5),
        s(8),
        p(1),
        p(3),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        GREEN_D,
        WHITE_D,
    ];
    let south_13 = vec![
        m(1),
        m(2),
        m(7),
        m(8),
        s(6),
        s(7),
        s(9),
        p(2),
        p(3),
        p(8),
        EAST_W,
        SOUTH_W,
        WHITE_D,
    ];
    let west_13 = vec![
        m(1),
        m(2),
        m(7),
        m(8),
        s(4),
        s(6),
        s(8),
        p(1),
        p(2),
        p(9),
        WEST_W,
        NORTH_W,
        GREEN_D,
    ];
    let north_13 = vec![
        m(4),
        m(5),
        m(5),
        m(6),
        s(1),
        s(2),
        s(3),
        p(4),
        p(5),
        p(6),
        RED_D,
        RED_D,
        RED_D,
    ];
    let wall =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[m(5), m(6)], &[]).unwrap();

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
                tile: m(5),
                from_drawn: false,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_pon, Seat::North).legal_actions,
        vec![
            PlayerAction::Ron { tile: m(5) },
            PlayerAction::Pon { tile: m(5) },
            PlayerAction::Pass,
        ]
    );
    game_pon
        .apply_action(Seat::North, PlayerAction::Pon { tile: m(5) })
        .unwrap();
    game_pon
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: m(4),
                from_drawn: false,
            },
        )
        .unwrap();
    // East draws 5m (head_draws[0]), discards 5m; South draws 6m (head_draws[1]), discards 6m.
    game_pon
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: m(5),
                from_drawn: true,
            },
        )
        .unwrap();
    game_pon
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: m(6),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(
        project_view(&game_pon, Seat::North).legal_actions,
        vec![PlayerAction::Ron { tile: m(6) }, PlayerAction::Pass]
    );
    game_pon
        .apply_action(Seat::North, PlayerAction::Ron { tile: m(6) })
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
                tile: m(5),
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
                tile: m(5),
                from_drawn: true,
            },
        )
        .unwrap();
    assert_eq!(south_5m.len(), 2);
    assert!(project_view(&game_pass, Seat::North)
        .legal_actions
        .is_empty());
}

use mahjong_core::{
    build_preset_wall, project_event, project_view, AnyRuleset, HandOutcome, Match, MatchError,
    MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, PublicPhase, RevealedMeld,
    Ruleset, ScoreItem, Seat, Tile, TuidaohuRuleset, TurnContext, TurnSource, WinEvalInput,
    WinKind, Wind,
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

fn filler_hands() -> (Vec<Tile>, Vec<Tile>, Vec<Tile>) {
    let south_13 = vec![
        s(1),
        s(1),
        s(1),
        s(2),
        s(2),
        s(2),
        s(3),
        s(3),
        s(3),
        s(4),
        s(4),
        s(4),
        s(5),
    ];
    let west_13 = vec![
        p(5),
        p(5),
        p(5),
        p(6),
        p(6),
        p(6),
        p(7),
        p(7),
        p(7),
        p(8),
        p(8),
        p(8),
        p(9),
    ];
    let north_13 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
        NORTH_W, NORTH_W, GREEN_D,
    ];
    (south_13, west_13, north_13)
}

#[test]
fn slice1_initial_dealer_tsumo_settles_with_menqing_no_tenhou_and_dealer_streak() {
    let dealer_winning_14 = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        s(7),
        s(8),
        s(9),
        p(2),
        p(3),
        p(4),
        RED_D,
        RED_D,
    ];
    let dealer_non_winning_14 = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        s(7),
        s(8),
        s(9),
        p(2),
        p(3),
        p(4),
        RED_D,
        WHITE_D,
    ];
    let (south_13, west_13, north_13) = filler_hands();

    let non_win_wall = build_preset_wall(
        &dealer_non_winning_14,
        &south_13,
        &west_13,
        &north_13,
        &[],
        &[],
    )
    .unwrap();
    let mut non_win_match = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, non_win_wall),
    )
    .unwrap();
    let non_win_view = project_view(&non_win_match, Seat::East);
    assert!(!non_win_view.legal_actions.contains(&PlayerAction::Tsumo));
    let seq_before = non_win_match.seq();
    assert_eq!(
        non_win_match
            .apply_action(Seat::East, PlayerAction::Tsumo)
            .unwrap_err(),
        MatchError::IllegalAction {
            seat: Seat::East,
            action: PlayerAction::Tsumo,
        }
    );
    assert_eq!(non_win_match.seq(), seq_before);

    let win_wall_1 =
        build_preset_wall(&dealer_winning_14, &south_13, &west_13, &north_13, &[], &[]).unwrap();
    let win_wall_2 = Tile::standard_deck_136();
    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, vec![win_wall_1, win_wall_2]),
    )
    .unwrap();

    let east_view = project_view(&game, Seat::East);
    assert!(east_view.legal_actions.contains(&PlayerAction::Tsumo));
    assert!(project_view(&game, Seat::South).legal_actions.is_empty());

    let emitted = game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
    assert_eq!(emitted.len(), 1);
    let settled_ev = project_event(&emitted[0], Seat::West);
    assert_eq!(settled_ev.seq, 3);

    let settlement = match settled_ev.kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };

    assert_eq!(settlement.wind_round, Wind::East);
    assert_eq!(settlement.hand_number, 1);
    assert_eq!(settlement.dealer_streak, 0);
    assert_eq!(settlement.dealer, Seat::East);
    assert_eq!(settlement.score_deltas, [6, -2, -2, -2]);
    assert_eq!(settlement.new_scores, [106, 98, 98, 98]);
    assert!(settlement.kong_transfers.is_empty());
    assert_eq!(settlement.revealed_hands[0].closed_tiles, dealer_winning_14);
    assert_eq!(settlement.revealed_hands[0].drawn_tile, None);
    assert_eq!(settlement.revealed_hands[1].closed_tiles, south_13);
    assert_eq!(settlement.revealed_hands[2].closed_tiles, west_13);
    assert_eq!(settlement.revealed_hands[3].closed_tiles, north_13);

    match &settlement.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, None);
            assert_eq!(*winning_tile, RED_D);
            assert_eq!(winners.len(), 1);
            let w = &winners[0];
            assert_eq!(w.seat, Seat::East);
            assert_eq!(w.win_kind, WinKind::Tsumo);
            assert_eq!(w.total_fan, 1);
            assert_eq!(w.base_points, 2);
            assert_eq!(
                w.items,
                vec![ScoreItem {
                    code: "menqing".to_string(),
                    label: "门前清".to_string(),
                    fan: 1,
                }]
            );
        }
        other => panic!("expected Win outcome, got {other:?}"),
    }

    let between_view = project_view(&game, Seat::East);
    assert_eq!(between_view.phase, PublicPhase::BetweenHands);
    assert_eq!(between_view.last_settlement, Some(settlement));
    assert_eq!(
        game.apply_action(Seat::East, PlayerAction::Tsumo)
            .unwrap_err(),
        MatchError::WrongPhase {
            seat: Seat::East,
            action: PlayerAction::Tsumo,
        }
    );

    for seat in Seat::ALL {
        game.apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
    let next_hand_view = project_view(&game, Seat::East);
    assert_eq!(next_hand_view.phase, PublicPhase::Playing);
    assert_eq!(next_hand_view.turn.hand_number, 1);
    assert_eq!(next_hand_view.turn.dealer_streak, 1);
    assert_eq!(next_hand_view.turn.dealer, Seat::East);
    assert_eq!(next_hand_view.seats.map(|s| s.score), [106, 98, 98, 98]);
}

#[test]
fn slice2_normal_draw_all_pungs_tsumo_equal_payment_and_dealer_rotation() {
    let east_14 = vec![
        s(1),
        s(2),
        s(3),
        s(4),
        s(5),
        s(6),
        p(1),
        p(2),
        p(3),
        p(5),
        p(6),
        p(7),
        EAST_W,
        SOUTH_W,
    ];
    let south_13 = vec![
        m(1),
        m(1),
        m(1),
        m(2),
        m(2),
        m(2),
        m(3),
        m(3),
        m(3),
        p(4),
        p(4),
        p(4),
        RED_D,
    ];
    let west_13 = vec![
        s(7),
        s(7),
        s(7),
        s(8),
        s(8),
        s(8),
        s(9),
        s(9),
        s(9),
        p(8),
        p(8),
        p(8),
        p(9),
    ];
    let north_13 = vec![
        WEST_W,
        WEST_W,
        WEST_W,
        NORTH_W,
        NORTH_W,
        NORTH_W,
        GREEN_D,
        GREEN_D,
        GREEN_D,
        WHITE_D,
        WHITE_D,
        WHITE_D,
        m(9),
    ];

    let wall_1 =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[RED_D], &[]).unwrap();
    let wall_2 = Tile::standard_deck_136();
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, vec![wall_1, wall_2]),
    )
    .unwrap();

    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: SOUTH_W,
            from_drawn: false,
        },
    )
    .unwrap();

    let south_view = project_view(&game, Seat::South);
    assert_eq!(south_view.hand.drawn_tile, Some(RED_D));
    assert!(south_view.legal_actions.contains(&PlayerAction::Tsumo));

    let events = game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();
    assert_eq!(events.len(), 1);
    let settlement = match project_event(&events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };

    // Equal 4-point payment from dealer East and non-dealers West/North.
    assert_eq!(settlement.score_deltas, [-4, 12, -4, -4]);
    assert_eq!(settlement.new_scores, [-4, 12, -4, -4]);
    assert_eq!(settlement.revealed_hands[1].drawn_tile, Some(RED_D));
    match &settlement.outcome {
        HandOutcome::Win {
            winners,
            discarder,
            winning_tile,
        } => {
            assert_eq!(*discarder, None);
            assert_eq!(*winning_tile, RED_D);
            let w = &winners[0];
            assert_eq!(w.seat, Seat::South);
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert!(w.items.contains(&ScoreItem {
                code: "all_pungs".to_string(),
                label: "碰碰胡".to_string(),
                fan: 1,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "menqing".to_string(),
                label: "门前清".to_string(),
                fan: 1,
            }));
            assert_eq!(w.items.len(), 2);
        }
        other => panic!("expected Win, got {other:?}"),
    }

    for seat in Seat::ALL {
        game.apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
    let next_view = project_view(&game, Seat::South);
    assert_eq!(next_view.turn.dealer, Seat::South);
    assert_eq!(next_view.turn.hand_number, 2);
    assert_eq!(next_view.turn.dealer_streak, 0);
}

#[test]
fn slice2_last_tile_tsumo_and_east_only_hand4_non_dealer_win_finishes_match() {
    // Helper wall for a quick non-dealer Tsumo on the 1st normal draw (so dealer rotates).
    let quick_non_dealer_win_wall = || {
        let (south_wait, west_f, north_f) = (
            vec![
                m(1),
                m(2),
                m(3),
                m(4),
                m(5),
                m(6),
                s(7),
                s(8),
                s(9),
                p(2),
                p(3),
                p(4),
                RED_D,
            ],
            vec![
                p(5),
                p(5),
                p(5),
                p(6),
                p(6),
                p(6),
                p(7),
                p(7),
                p(7),
                p(8),
                p(8),
                p(8),
                p(9),
            ],
            vec![
                EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
                NORTH_W, NORTH_W, GREEN_D,
            ],
        );
        let dealer_14 = vec![
            s(1),
            s(1),
            s(1),
            s(2),
            s(2),
            s(2),
            s(3),
            s(3),
            s(3),
            s(4),
            s(4),
            s(4),
            s(5),
            WHITE_D,
        ];
        build_preset_wall(&dealer_14, &south_wait, &west_f, &north_f, &[RED_D], &[]).unwrap()
    };

    // Hand 4: North is dealer; West (3rd after North: North->East->South->West) draws tile #83.
    let north_dealer_14 = vec![
        s(1),
        s(2),
        s(4),
        s(5),
        s(7),
        s(8),
        p(1),
        p(3),
        p(5),
        p(7),
        p(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
    ];
    let east_13 = vec![
        m(1),
        m(3),
        m(5),
        m(7),
        m(9),
        p(1),
        p(3),
        p(6),
        p(8),
        NORTH_W,
        GREEN_D,
        RED_D,
        RED_D,
    ];
    let south_13 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(1),
        s(3),
        s(6),
        s(9),
        p(2),
        p(5),
        p(8),
        EAST_W,
        WHITE_D,
    ];
    let west_13 = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        s(7),
        s(8),
        s(9),
        p(2),
        p(3),
        p(4),
        RED_D,
    ];
    let hand4_wall = build_preset_wall(
        &north_dealer_14,
        &east_13,
        &south_13,
        &west_13,
        &[],
        &[RED_D],
    )
    .unwrap();

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(
            Seat::East,
            vec![
                quick_non_dealer_win_wall(),
                quick_non_dealer_win_wall(),
                quick_non_dealer_win_wall(),
                hand4_wall,
            ],
        ),
    )
    .unwrap();

    for hand_idx in 1..=3 {
        let v = project_view(&game, Seat::East);
        let dealer = v.turn.dealer;
        assert_eq!(v.turn.hand_number, hand_idx);
        let discard_tile = project_view(&game, dealer).hand.closed_tiles[13];
        game.apply_action(
            dealer,
            PlayerAction::Discard {
                tile: discard_tile,
                from_drawn: false,
            },
        )
        .unwrap();
        let next_seat = dealer.next();
        game.apply_action(next_seat, PlayerAction::Tsumo).unwrap();
        for seat in Seat::ALL {
            game.apply_action(seat, PlayerAction::ConfirmNextHand)
                .unwrap();
        }
    }

    let hand4_start = project_view(&game, Seat::North);
    assert_eq!(hand4_start.turn.hand_number, 4);
    assert_eq!(hand4_start.turn.dealer, Seat::North);
    assert_eq!(hand4_start.wall_remaining, 83);

    // Discard 83 times so West receives the 83rd draw at wall_remaining == 0.
    for step in 0..83 {
        let probe = project_view(&game, Seat::East);
        let active = probe.turn.active_seat.unwrap();
        let active_view = project_view(&game, active);
        let action = if step == 0 {
            PlayerAction::Discard {
                tile: active_view.hand.closed_tiles[0],
                from_drawn: false,
            }
        } else {
            PlayerAction::Discard {
                tile: active_view.hand.drawn_tile.unwrap(),
                from_drawn: true,
            }
        };
        game.apply_action(active, action).unwrap();
    }

    let west_final_draw = project_view(&game, Seat::West);
    assert_eq!(west_final_draw.wall_remaining, 0);
    assert_eq!(west_final_draw.turn.active_seat, Some(Seat::West));
    assert_eq!(west_final_draw.hand.drawn_tile, Some(RED_D));
    assert!(west_final_draw.legal_actions.contains(&PlayerAction::Tsumo));

    let finish_events = game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    assert_eq!(finish_events.len(), 2);

    let settlement = match project_event(&finish_events[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled first, got {other:?}"),
    };
    match &settlement.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.seat, Seat::West);
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert!(w.items.contains(&ScoreItem {
                code: "last_tile_tsumo".to_string(),
                label: "海底捞月".to_string(),
                fan: 1,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "menqing".to_string(),
                label: "门前清".to_string(),
                fan: 1,
            }));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    let summary = match project_event(&finish_events[1], Seat::East).kind {
        PlayerEventKind::MatchFinished { summary } => summary,
        other => panic!("expected MatchFinished second, got {other:?}"),
    };
    assert_eq!(summary.mode, MatchMode::EastOnly);
    assert_eq!(summary.hands_played, 4);
    // Hands 1..3: South, West, North each won 1 fan (2 pts * 3 = +6; losers -2).
    // Hand 4: West won 2 fan (4 pts * 3 = +12; losers -4).
    // Totals from 100: East = 100 - 2 - 2 - 2 - 4 = 90;
    // South = 100 + 6 - 2 - 2 - 4 = 98;
    // West = 100 - 2 + 6 - 2 + 12 = 114;
    // North = 100 - 2 - 2 + 6 - 4 = 98.
    assert_eq!(summary.final_scores, [90, 98, 114, 98]);
    assert_eq!(summary.net_deltas, [-10, -2, 14, -2]);
    assert_eq!(
        project_view(&game, Seat::East).phase,
        PublicPhase::MatchFinished
    );
}

#[test]
fn slice3_half_flush_full_flush_exclusion_and_all_honors_cap() {
    let number_filler = || {
        (
            vec![
                s(1),
                s(1),
                s(1),
                s(2),
                s(2),
                s(2),
                s(3),
                s(3),
                s(3),
                s(4),
                s(4),
                s(4),
                s(5),
            ],
            vec![
                p(1),
                p(1),
                p(1),
                p(2),
                p(2),
                p(2),
                p(3),
                p(3),
                p(3),
                p(4),
                p(4),
                p(4),
                p(5),
            ],
            vec![
                s(6),
                s(6),
                s(6),
                s(7),
                s(7),
                s(7),
                p(6),
                p(6),
                p(6),
                p(7),
                p(7),
                p(7),
                p(8),
            ],
        )
    };
    let (f1, f2, f3) = number_filler();

    // 1. Half flush: 123m 456m 789m EEE CC -> half_flush (+1) + menqing (+1) = 2 fan.
    let half_flush_14 = vec![
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
        EAST_W,
        EAST_W,
        EAST_W,
        RED_D,
        RED_D,
    ];
    let wall_half = build_preset_wall(&half_flush_14, &f1, &f2, &f3, &[], &[]).unwrap();
    let mut game_half = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_half),
    )
    .unwrap();
    let ev_half = game_half
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    let s_half = match project_event(&ev_half[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    match &s_half.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert!(w.items.contains(&ScoreItem {
                code: "half_flush".to_string(),
                label: "混一色".to_string(),
                fan: 1,
            }));
            assert!(!w.items.iter().any(|i| i.code == "full_flush"));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 2. Full flush: 123m 456m 789m 234m 55m -> full_flush (+2) + menqing (+1) = 3 fan (replaces half_flush).
    let full_flush_14 = vec![
        m(1),
        m(2),
        m(2),
        m(3),
        m(3),
        m(4),
        m(4),
        m(5),
        m(5),
        m(5),
        m(6),
        m(7),
        m(8),
        m(9),
    ];
    let wall_full = build_preset_wall(&full_flush_14, &f1, &f2, &f3, &[], &[]).unwrap();
    let mut game_full = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_full),
    )
    .unwrap();
    let ev_full = game_full
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    let s_full = match project_event(&ev_full[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    match &s_full.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 3);
            assert_eq!(w.base_points, 8);
            assert!(w.items.contains(&ScoreItem {
                code: "full_flush".to_string(),
                label: "清一色".to_string(),
                fan: 2,
            }));
            assert!(!w.items.iter().any(|i| i.code == "half_flush"));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 3. All honors: EEE SSS WWW CCC BB -> 4 fan cap (16x).
    let all_honors_14 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, RED_D, RED_D,
        RED_D, WHITE_D, WHITE_D,
    ];
    let wall_honors = build_preset_wall(&all_honors_14, &f1, &f2, &f3, &[], &[]).unwrap();
    let mut game_honors = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_honors),
    )
    .unwrap();
    let ev_honors = game_honors
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    let s_honors = match project_event(&ev_honors[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(s_honors.score_deltas, [48, -16, -16, -16]);
    match &s_honors.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 4);
            assert_eq!(w.base_points, 16);
            assert!(w.items.contains(&ScoreItem {
                code: "all_honors".to_string(),
                label: "字一色".to_string(),
                fan: 4,
            }));
            assert!(!w.items.iter().any(|i| i.code == "half_flush"));
            assert!(!w.items.iter().any(|i| i.code == "full_flush"));
        }
        other => panic!("expected Win, got {other:?}"),
    }
}

#[test]
fn slice4_seven_pairs_luxury_seven_pairs_and_non_stacking() {
    let f1 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(1),
        s(4),
        s(6),
        s(9),
        p(1),
        p(5),
        p(6),
        p(8),
        SOUTH_W,
    ];
    let f2 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(1),
        s(4),
        s(6),
        s(9),
        p(1),
        p(5),
        p(6),
        p(8),
        WEST_W,
    ];
    let f3 = vec![
        m(2),
        m(4),
        m(6),
        m(8),
        s(1),
        s(4),
        s(6),
        s(9),
        p(1),
        p(5),
        p(6),
        p(8),
        NORTH_W,
    ];

    let settle_dealer =
        |dealer_14: Vec<Tile>, filler_1: &[Tile], filler_2: &[Tile], filler_3: &[Tile]| {
            let wall =
                build_preset_wall(&dealer_14, filler_1, filler_2, filler_3, &[], &[]).unwrap();
            let mut game = Match::new(
                MatchMode::FreeTable,
                TuidaohuRuleset,
                PresetWallProvider::single(Seat::East, wall),
            )
            .unwrap();
            let ev = game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
            match project_event(&ev[0], Seat::East).kind {
                PlayerEventKind::HandSettled { settlement } => settlement,
                other => panic!("expected HandSettled, got {other:?}"),
            }
        };

    // 1. Seven pairs: 11m 33m 55s 77s 22p 44p EE -> seven_pairs (1), no menqing.
    let s1 = settle_dealer(
        vec![
            m(1),
            m(1),
            m(3),
            m(3),
            s(5),
            s(5),
            s(7),
            s(7),
            p(2),
            p(2),
            p(4),
            p(4),
            EAST_W,
            EAST_W,
        ],
        &f1,
        &f2,
        &f3,
    );
    match &s1.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 1);
            assert_eq!(w.base_points, 2);
            assert_eq!(
                w.items,
                vec![ScoreItem {
                    code: "seven_pairs".to_string(),
                    label: "七对".to_string(),
                    fan: 1,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 2. Luxury seven pairs (1 quad): 1111m 33s 55s 77p 99p EE -> luxury_seven_pairs (2), no seven_pairs, no menqing.
    let s2 = settle_dealer(
        vec![
            m(1),
            m(1),
            m(1),
            m(1),
            s(3),
            s(3),
            s(5),
            s(5),
            p(7),
            p(7),
            p(9),
            p(9),
            EAST_W,
            EAST_W,
        ],
        &f1,
        &f2,
        &f3,
    );
    match &s2.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert_eq!(
                w.items,
                vec![ScoreItem {
                    code: "luxury_seven_pairs".to_string(),
                    label: "豪华七对".to_string(),
                    fan: 2,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 3. Multi-quad luxury seven pairs (3 quads): 1111m 2222s 3333p EE -> still +2 (no stacking!).
    let s3 = settle_dealer(
        vec![
            m(1),
            m(1),
            m(1),
            m(1),
            s(2),
            s(2),
            s(2),
            s(2),
            p(3),
            p(3),
            p(3),
            p(3),
            EAST_W,
            EAST_W,
        ],
        &f1,
        &f2,
        &f3,
    );
    match &s3.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert_eq!(
                w.items,
                vec![ScoreItem {
                    code: "luxury_seven_pairs".to_string(),
                    label: "豪华七对".to_string(),
                    fan: 2,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 4. All-honors luxury seven pairs: EEEE SSSS WW CC BB -> capped at 4 fan (16x), no menqing.
    let num_f1 = vec![
        m(1),
        m(2),
        m(4),
        m(5),
        m(7),
        m(8),
        s(1),
        s(2),
        s(4),
        s(5),
        s(7),
        s(8),
        p(1),
    ];
    let num_f2 = vec![
        m(1),
        m(2),
        m(4),
        m(5),
        m(7),
        m(8),
        s(1),
        s(2),
        s(4),
        s(5),
        s(7),
        s(8),
        p(2),
    ];
    let num_f3 = vec![
        m(1),
        m(2),
        m(4),
        m(5),
        m(7),
        m(8),
        s(1),
        s(2),
        s(4),
        s(5),
        s(7),
        s(8),
        p(3),
    ];
    let s4 = settle_dealer(
        vec![
            EAST_W, EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W,
            RED_D, RED_D, WHITE_D, WHITE_D,
        ],
        &num_f1,
        &num_f2,
        &num_f3,
    );
    match &s4.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 4);
            assert_eq!(w.base_points, 16);
            assert!(w.items.contains(&ScoreItem {
                code: "all_honors".to_string(),
                label: "字一色".to_string(),
                fan: 4,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "luxury_seven_pairs".to_string(),
                label: "豪华七对".to_string(),
                fan: 2,
            }));
            assert!(!w.items.iter().any(|i| i.code == "menqing"));
        }
        other => panic!("expected Win, got {other:?}"),
    }
}

#[test]
fn slice5_thirteen_orphans_four_fan_cap_and_no_menqing() {
    let f1 = vec![
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        s(2),
        s(3),
        s(4),
        s(5),
        s(6),
        s(7),
        p(2),
    ];
    let f2 = vec![
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        s(2),
        s(3),
        s(4),
        s(5),
        s(6),
        s(7),
        p(3),
    ];
    let f3 = vec![
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        s(2),
        s(3),
        s(4),
        s(5),
        s(6),
        s(7),
        p(4),
    ];

    let orphans_14 = vec![
        m(1),
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
    ];
    let wall = build_preset_wall(&orphans_14, &f1, &f2, &f3, &[], &[]).unwrap();
    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .unwrap();

    let ev = game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
    let settlement = match project_event(&ev[0], Seat::East).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(settlement.score_deltas, [48, -16, -16, -16]);
    match &settlement.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 4);
            assert_eq!(w.base_points, 16);
            assert_eq!(
                w.items,
                vec![ScoreItem {
                    code: "thirteen_orphans".to_string(),
                    label: "十三幺".to_string(),
                    fan: 4,
                }]
            );
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // NormalDraw Thirteen Orphans with duplicate White Dragon.
    let south_wait_13 = vec![
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
    ];
    let east_14 = vec![
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        m(7),
        s(2),
        s(3),
        s(4),
        s(5),
        s(6),
        s(7),
        p(2),
        p(5),
    ];
    let wall2 = build_preset_wall(&east_14, &south_wait_13, &f2, &f3, &[WHITE_D], &[]).unwrap();
    let mut game2 = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall2),
    )
    .unwrap();
    game2
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: p(5),
                from_drawn: false,
            },
        )
        .unwrap();
    let ev2 = game2
        .apply_action(Seat::South, PlayerAction::Tsumo)
        .unwrap();
    let s2 = match project_event(&ev2[0], Seat::South).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    };
    assert_eq!(s2.score_deltas, [-16, 48, -16, -16]);
}

#[test]
fn slice6_multi_decomposition_selects_highest_fan_breakdown() {
    let f1 = vec![
        s(1),
        s(1),
        s(1),
        s(2),
        s(2),
        s(2),
        s(3),
        s(3),
        s(3),
        s(4),
        s(4),
        s(4),
        s(5),
    ];
    let f2 = vec![
        s(6),
        s(6),
        s(6),
        s(7),
        s(7),
        s(7),
        s(8),
        s(8),
        s(8),
        s(9),
        s(9),
        s(9),
        p(9),
    ];
    let f3 = vec![
        p(5),
        p(5),
        p(5),
        p(6),
        p(6),
        p(6),
        p(7),
        p(7),
        p(7),
        p(8),
        p(8),
        p(8),
        NORTH_W,
    ];

    let settle = |dealer_14: Vec<Tile>| {
        let wall = build_preset_wall(&dealer_14, &f1, &f2, &f3, &[], &[]).unwrap();
        let mut game = Match::new(
            MatchMode::FreeTable,
            TuidaohuRuleset,
            PresetWallProvider::single(Seat::East, wall),
        )
        .unwrap();
        let ev = game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
        match project_event(&ev[0], Seat::East).kind {
            PlayerEventKind::HandSettled { settlement } => settlement,
            other => panic!("expected HandSettled, got {other:?}"),
        }
    };

    // 1. 111222333m 444p 55z: all-pungs + menqing (2 fan) beats 3 sequences + menqing (1 fan).
    let s1 = settle(vec![
        m(1),
        m(1),
        m(1),
        m(2),
        m(2),
        m(2),
        m(3),
        m(3),
        m(3),
        p(4),
        p(4),
        p(4),
        RED_D,
        RED_D,
    ]);
    match &s1.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 2);
            assert_eq!(w.base_points, 4);
            assert!(w.items.contains(&ScoreItem {
                code: "all_pungs".to_string(),
                label: "碰碰胡".to_string(),
                fan: 1,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "menqing".to_string(),
                label: "门前清".to_string(),
                fan: 1,
            }));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 2. 22233344455666m: pair 55m + 4 triplets (4 fan) beats pair 22m + sequences (3 fan).
    let s2 = settle(vec![
        m(2),
        m(2),
        m(2),
        m(3),
        m(3),
        m(3),
        m(4),
        m(4),
        m(4),
        m(5),
        m(5),
        m(6),
        m(6),
        m(6),
    ]);
    match &s2.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 4);
            assert_eq!(w.base_points, 16);
            assert!(w.items.contains(&ScoreItem {
                code: "full_flush".to_string(),
                label: "清一色".to_string(),
                fan: 2,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "all_pungs".to_string(),
                label: "碰碰胡".to_string(),
                fan: 1,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "menqing".to_string(),
                label: "门前清".to_string(),
                fan: 1,
            }));
        }
        other => panic!("expected Win, got {other:?}"),
    }

    // 3. 111122223333m 44m: luxury_seven_pairs + full_flush (4 fan) beats 123m*4 + 44m (3 fan).
    let s3 = settle(vec![
        m(1),
        m(1),
        m(1),
        m(1),
        m(2),
        m(2),
        m(2),
        m(2),
        m(3),
        m(3),
        m(3),
        m(3),
        m(4),
        m(4),
    ]);
    match &s3.outcome {
        HandOutcome::Win { winners, .. } => {
            let w = &winners[0];
            assert_eq!(w.total_fan, 4);
            assert_eq!(w.base_points, 16);
            assert!(w.items.contains(&ScoreItem {
                code: "luxury_seven_pairs".to_string(),
                label: "豪华七对".to_string(),
                fan: 2,
            }));
            assert!(w.items.contains(&ScoreItem {
                code: "full_flush".to_string(),
                label: "清一色".to_string(),
                fan: 2,
            }));
            assert!(!w.items.iter().any(|i| i.code == "menqing"));
        }
        other => panic!("expected Win, got {other:?}"),
    }
}

#[test]
fn slice7_non_winning_hands_reject_tsumo() {
    let (f1, f2, f3) = (
        vec![
            m(3),
            m(3),
            m(3),
            m(4),
            m(4),
            m(4),
            s(4),
            s(4),
            s(4),
            p(4),
            p(4),
            p(4),
            p(5),
        ],
        vec![
            p(6),
            p(6),
            p(6),
            p(7),
            p(7),
            p(7),
            p(8),
            p(8),
            p(8),
            s(5),
            s(5),
            s(5),
            s(6),
        ],
        vec![
            s(7),
            s(7),
            s(7),
            s(8),
            s(8),
            s(8),
            m(5),
            m(5),
            m(5),
            m(6),
            m(6),
            m(6),
            m(7),
        ],
    );

    let non_winning_hands = [
        // 14 arbitrary honors that do not form 4 melds+pair or 7 pairs.
        vec![
            EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, RED_D,
            RED_D, RED_D, GREEN_D, WHITE_D,
        ],
        // Incomplete thirteen orphans (missing 9p).
        vec![
            m(1),
            m(1),
            m(9),
            m(9),
            s(1),
            s(9),
            p(1),
            EAST_W,
            SOUTH_W,
            WEST_W,
            NORTH_W,
            RED_D,
            GREEN_D,
            WHITE_D,
        ],
        // Cross-suit wrap 8m 9m 1s must not count as a sequence.
        vec![
            m(8),
            m(9),
            s(1),
            m(1),
            m(2),
            m(3),
            s(2),
            s(3),
            s(4),
            p(1),
            p(2),
            p(3),
            RED_D,
            RED_D,
        ],
        // Six pairs + two unmatched tiles.
        vec![
            m(1),
            m(1),
            m(2),
            m(2),
            s(2),
            s(2),
            s(3),
            s(3),
            p(2),
            p(2),
            p(3),
            p(3),
            EAST_W,
            SOUTH_W,
        ],
        // Four triplets + two unmatched tiles.
        vec![
            m(1),
            m(1),
            m(1),
            m(2),
            m(2),
            m(2),
            s(2),
            s(2),
            s(2),
            p(2),
            p(2),
            p(2),
            GREEN_D,
            WHITE_D,
        ],
    ];

    for bad_hand in non_winning_hands {
        let wall = build_preset_wall(&bad_hand, &f1, &f2, &f3, &[], &[]).unwrap();
        let mut game = Match::new(
            MatchMode::FreeTable,
            TuidaohuRuleset,
            PresetWallProvider::single(Seat::East, wall),
        )
        .unwrap();
        let view = project_view(&game, Seat::East);
        assert!(
            !view.legal_actions.contains(&PlayerAction::Tsumo),
            "hand {bad_hand:?} should not be legal for Tsumo"
        );
        let seq_before = game.seq();
        assert_eq!(
            game.apply_action(Seat::East, PlayerAction::Tsumo)
                .unwrap_err(),
            MatchError::IllegalAction {
                seat: Seat::East,
                action: PlayerAction::Tsumo,
            }
        );
        assert_eq!(game.seq(), seq_before);
    }
}

#[test]
fn slice8_ruleset_seam_open_melds_basic_hu_situational_fans_and_after_meld_guard() {
    let tuidaohu = TuidaohuRuleset;
    let any_ruleset = AnyRuleset::default();
    let pon_1m = RevealedMeld::Pon {
        tile: m(1),
        from_seat: Seat::South,
        seq: 5,
    };
    let concealed_east = RevealedMeld::ConcealedKong {
        tile: EAST_W,
        seq: 6,
    };

    // 1. 基本胡 (0 fan, 1 base point) requires an open meld with no other fan.
    let basic_closed = [m(2), m(3), m(4), s(1), s(2), s(3), p(1), p(2), p(3), RED_D];
    let basic_eval = tuidaohu
        .evaluate_win(&WinEvalInput {
            closed_tiles: &basic_closed,
            extra_tile: Some(RED_D),
            melds: &[pon_1m],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::NormalDraw),
            wall_remaining: 20,
        })
        .expect("open meld standard hand wins");
    assert_eq!(basic_eval.total_fan, 0);
    assert_eq!(basic_eval.base_points, 1);
    assert_eq!(
        basic_eval.items,
        vec![ScoreItem {
            code: "basic_hu".to_string(),
            label: "基本胡".to_string(),
            fan: 0,
        }]
    );
    assert_eq!(
        any_ruleset.evaluate_win(&WinEvalInput {
            closed_tiles: &basic_closed,
            extra_tile: Some(RED_D),
            melds: &[pon_1m],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::NormalDraw),
            wall_remaining: 20,
        }),
        Some(basic_eval)
    );

    // Same hand on last tile emits 海底捞月 (+1) and MUST NOT emit 基本胡.
    let last_tile_open = tuidaohu
        .evaluate_win(&WinEvalInput {
            closed_tiles: &basic_closed,
            extra_tile: Some(RED_D),
            melds: &[pon_1m],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::NormalDraw),
            wall_remaining: 0,
        })
        .unwrap();
    assert_eq!(last_tile_open.total_fan, 1);
    assert_eq!(last_tile_open.base_points, 2);
    assert_eq!(
        last_tile_open.items,
        vec![ScoreItem {
            code: "last_tile_tsumo".to_string(),
            label: "海底捞月".to_string(),
            fan: 1,
        }]
    );

    // 2. TurnSource::AfterMeld must NEVER offer Tsumo even if 11 closed + 1 Pon is complete.
    let after_meld_11 = [
        m(2),
        m(3),
        m(4),
        s(1),
        s(2),
        s(3),
        p(1),
        p(2),
        p(3),
        RED_D,
        RED_D,
    ];
    let after_meld_actions = tuidaohu.legal_turn_actions(&TurnContext {
        closed_tiles: &after_meld_11,
        drawn_tile: None,
        melds: &[pon_1m],
        source: TurnSource::AfterMeld,
        wall_remaining: 20,
    });
    assert!(!after_meld_actions.contains(&PlayerAction::Tsumo));

    // 3. ConcealedKong preserves 门前清 and enables 杠上开花 (+ 海底捞月 stacking).
    let kong_closed = [m(2), m(3), m(4), s(1), s(2), s(3), p(1), p(2), p(3), RED_D];
    let kong_bloom_and_last = tuidaohu
        .evaluate_win(&WinEvalInput {
            closed_tiles: &kong_closed,
            extra_tile: Some(RED_D),
            melds: &[concealed_east],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::ReplacementDraw),
            wall_remaining: 0,
        })
        .unwrap();
    assert_eq!(kong_bloom_and_last.total_fan, 3);
    assert_eq!(kong_bloom_and_last.base_points, 8);
    assert!(kong_bloom_and_last.items.contains(&ScoreItem {
        code: "menqing".to_string(),
        label: "门前清".to_string(),
        fan: 1,
    }));
    assert!(kong_bloom_and_last.items.contains(&ScoreItem {
        code: "self_draw_after_kong".to_string(),
        label: "杠上开花".to_string(),
        fan: 1,
    }));
    assert!(kong_bloom_and_last.items.contains(&ScoreItem {
        code: "last_tile_tsumo".to_string(),
        label: "海底捞月".to_string(),
        fan: 1,
    }));

    // ReplacementDraw is legal for Tsumo in legal_turn_actions.
    let replacement_actions = any_ruleset.legal_turn_actions(&TurnContext {
        closed_tiles: &kong_closed,
        drawn_tile: Some(RED_D),
        melds: &[concealed_east],
        source: TurnSource::ReplacementDraw,
        wall_remaining: 10,
    });
    assert!(replacement_actions.contains(&PlayerAction::Tsumo));

    // 4. 抢杠胡 (RobKongHu) and 河底捞鱼 (LastTileRon).
    let closed_13 = [
        m(1),
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        s(7),
        s(8),
        s(9),
        p(2),
        p(3),
        p(4),
        RED_D,
    ];
    let rob_kong_eval = tuidaohu
        .evaluate_win(&WinEvalInput {
            closed_tiles: &closed_13,
            extra_tile: Some(RED_D),
            melds: &[],
            win_kind: WinKind::RobKongHu,
            turn_source: None,
            wall_remaining: 12,
        })
        .unwrap();
    assert_eq!(rob_kong_eval.total_fan, 2);
    assert_eq!(rob_kong_eval.base_points, 4);
    assert!(rob_kong_eval.items.contains(&ScoreItem {
        code: "rob_kong_hu".to_string(),
        label: "抢杠胡".to_string(),
        fan: 1,
    }));
    assert!(rob_kong_eval.items.contains(&ScoreItem {
        code: "menqing".to_string(),
        label: "门前清".to_string(),
        fan: 1,
    }));

    let last_tile_ron_eval = tuidaohu
        .evaluate_win(&WinEvalInput {
            closed_tiles: &closed_13,
            extra_tile: Some(RED_D),
            melds: &[],
            win_kind: WinKind::Ron,
            turn_source: None,
            wall_remaining: 0,
        })
        .unwrap();
    assert_eq!(last_tile_ron_eval.total_fan, 2);
    assert_eq!(last_tile_ron_eval.base_points, 4);
    assert!(last_tile_ron_eval.items.contains(&ScoreItem {
        code: "last_tile_ron".to_string(),
        label: "河底捞鱼".to_string(),
        fan: 1,
    }));
    assert!(last_tile_ron_eval.items.contains(&ScoreItem {
        code: "menqing".to_string(),
        label: "门前清".to_string(),
        fan: 1,
    }));

    // 5. Invalid tile counts (>4 of a tile or wrong closed length) are rejected.
    let five_of_a_kind = [m(1), m(1), m(1), m(1), m(2), m(3), s(1), s(2), s(3), RED_D];
    assert_eq!(
        tuidaohu.evaluate_win(&WinEvalInput {
            closed_tiles: &five_of_a_kind,
            extra_tile: Some(RED_D),
            melds: &[pon_1m],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::NormalDraw),
            wall_remaining: 10,
        }),
        None
    );
    assert_eq!(
        tuidaohu.evaluate_win(&WinEvalInput {
            closed_tiles: &closed_13,
            extra_tile: None,
            melds: &[],
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::InitialDealer),
            wall_remaining: 83,
        }),
        None
    );
}

use mahjong_core::{
    build_preset_wall, project_event, project_view, AnyRuleset, DiscardReactionContext,
    EvaluatedWin, HandOutcome, HandProgression, HandProgressionInput, KongKind, KongTransfer,
    Match, MatchError, MatchMode, PlayerAction, PlayerEvent, PlayerEventKind, PlayerView,
    PresetWallProvider, PublicPhase, RevealedHand, RobKongContext, Ruleset, ScoreItem, Seat,
    SeededWallProvider, SettleHandInput, Tile, TuidaohuRuleset, TurnContext, TurnSource,
    WallProvider, WinCandidate, WinEvalInput, WinKind, WinSettlementDetails, Wind,
    WinnerSettlement,
};

fn t(raw: u8) -> Tile {
    Tile::new(raw).expect("valid tile index")
}

fn m(rank: u8) -> Tile {
    t(rank - 1)
}

fn s(rank: u8) -> Tile {
    t(9 + rank - 1)
}

fn p(rank: u8) -> Tile {
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

/// Builds a preset wall where `dealer` holds a 14-tile winning hand (`123m 456m 789s 234p CC`, 1 fan `门前清`).
fn dealer_win_wall() -> Vec<Tile> {
    let dealer_14 = vec![
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
    let s1 = vec![
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
    let s2 = vec![
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
    let s3 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
        NORTH_W, NORTH_W, GREEN_D,
    ];
    build_preset_wall(&dealer_14, &s1, &s2, &s3, &[], &[]).unwrap()
}

/// Builds a preset wall where `dealer.next()` draws `RED_D` on the first draw and wins 1-fan `门前清` Tsumo.
fn next_seat_quick_win_wall() -> Vec<Tile> {
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
    let seat1_13 = vec![
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
    let seat2_13 = vec![
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
    let seat3_13 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
        NORTH_W, NORTH_W, GREEN_D,
    ];
    build_preset_wall(&dealer_14, &seat1_13, &seat2_13, &seat3_13, &[RED_D], &[]).unwrap()
}

#[test]
fn free_table_supports_more_than_four_hands_with_wind_rotation_cumulative_scores_and_end_match() {
    let walls = vec![
        next_seat_quick_win_wall(), // Hand 1 (East 1, dealer East -> South wins +6)
        dealer_win_wall(),          // Hand 2 (East 2, dealer South -> South wins +6, 连庄)
        next_seat_quick_win_wall(), // Hand 3 (East 2 streak 1, dealer South -> West wins +6)
        next_seat_quick_win_wall(), // Hand 4 (East 3, dealer West -> North wins +6)
        next_seat_quick_win_wall(), // Hand 5 (East 4, dealer North -> East wins +6)
        next_seat_quick_win_wall(), // Hand 6 (South 1, dealer East -> South wins +6)
    ];

    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, walls),
    )
    .expect("FreeTable match should initialize");

    let initial_view = project_view(&game, Seat::East);
    assert_eq!(initial_view.seats.map(|s| s.score), [0, 0, 0, 0]);
    assert_eq!(initial_view.turn.wind_round, Wind::East);
    assert_eq!(initial_view.turn.hand_number, 1);
    assert_eq!(initial_view.turn.dealer, Seat::East);

    let confirm_all = |game: &mut Match<TuidaohuRuleset, PresetWallProvider>| {
        for seat in Seat::ALL {
            game.apply_action(seat, PlayerAction::ConfirmNextHand)
                .unwrap();
        }
    };

    // Hand 1 (East 1): East discards WHITE_D, South draws RED_D and Tsumo (+6, others -2).
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    let h2 = project_view(&game, Seat::South);
    assert_eq!(h2.turn.wind_round, Wind::East);
    assert_eq!(h2.turn.hand_number, 2);
    assert_eq!(h2.turn.dealer_streak, 0);
    assert_eq!(h2.turn.dealer, Seat::South);
    assert_eq!(h2.seats.map(|s| s.score), [-2, 6, -2, -2]);

    // Hand 2 (East 2): dealer South Tsumo immediately (+6, others -2) -> 连庄.
    game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    let h3 = project_view(&game, Seat::South);
    assert_eq!(h3.turn.wind_round, Wind::East);
    assert_eq!(h3.turn.hand_number, 2);
    assert_eq!(h3.turn.dealer_streak, 1);
    assert_eq!(h3.turn.dealer, Seat::South);
    assert_eq!(h3.seats.map(|s| s.score), [-4, 12, -4, -4]);

    // Hand 3 (East 2 streak 1): South discards WHITE_D, West Tsumo -> East 3, dealer West.
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    let h4 = project_view(&game, Seat::West);
    assert_eq!(h4.turn.wind_round, Wind::East);
    assert_eq!(h4.turn.hand_number, 3);
    assert_eq!(h4.turn.dealer_streak, 0);
    assert_eq!(h4.turn.dealer, Seat::West);
    assert_eq!(h4.seats.map(|s| s.score), [-6, 10, 2, -6]);

    // Hand 4 (East 3): West discards WHITE_D, North Tsumo -> East 4, dealer North.
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    let h5 = project_view(&game, Seat::North);
    assert_eq!(h5.turn.wind_round, Wind::East);
    assert_eq!(h5.turn.hand_number, 4);
    assert_eq!(h5.turn.dealer_streak, 0);
    assert_eq!(h5.turn.dealer, Seat::North);
    assert_eq!(h5.seats.map(|s| s.score), [-8, 8, 0, 0]);

    // Hand 5 (East 4): North discards WHITE_D, East Tsumo -> South 1, dealer East!
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    let h6 = project_view(&game, Seat::East);
    assert_eq!(h6.turn.wind_round, Wind::South);
    assert_eq!(h6.turn.hand_number, 1);
    assert_eq!(h6.turn.dealer_streak, 0);
    assert_eq!(h6.turn.dealer, Seat::East);
    assert_eq!(h6.seats.map(|s| s.score), [-2, 6, -2, -2]);

    // Hand 6 (South 1): East discards WHITE_D, South Tsumo -> cumulative [-4, 12, -4, -4].
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();

    // Even after East confirms ConfirmNextHand, EndMatch remains legal for East in FreeTable BetweenHands.
    game.apply_action(Seat::East, PlayerAction::ConfirmNextHand)
        .unwrap();
    let east_between = project_view(&game, Seat::East);
    assert_eq!(east_between.phase, PublicPhase::BetweenHands);
    assert_eq!(east_between.legal_actions, vec![PlayerAction::EndMatch]);

    let end_events = game
        .apply_action(Seat::East, PlayerAction::EndMatch)
        .expect("seat that confirmed next hand can still end FreeTable match");
    assert_eq!(end_events.len(), 1);
    match project_event(&end_events[0], Seat::North).kind {
        PlayerEventKind::MatchFinished { summary } => {
            assert_eq!(summary.mode, MatchMode::FreeTable);
            assert_eq!(summary.hands_played, 6);
            assert_eq!(summary.initial_scores, [0, 0, 0, 0]);
            assert_eq!(summary.final_scores, [-4, 12, -4, -4]);
            assert_eq!(summary.net_deltas, [-4, 12, -4, -4]);
        }
        other => panic!("expected MatchFinished, got {other:?}"),
    }

    let finished = project_view(&game, Seat::East);
    assert_eq!(finished.phase, PublicPhase::MatchFinished);
    assert_eq!(finished.ready_seats, None);
    assert!(finished.legal_actions.is_empty());
    assert!(finished.last_settlement.is_some());
}

/// Custom `WallProvider` that returns an invalid wall on demand before switching to valid walls.
struct FlakySecondWallProvider {
    initial_dealer: Seat,
    first_wall: Vec<Tile>,
    valid_second_wall: Vec<Tile>,
    calls: usize,
    fail_on_call: usize,
}

impl WallProvider for FlakySecondWallProvider {
    fn initial_dealer(&mut self) -> Seat {
        self.initial_dealer
    }

    fn next_wall(&mut self, _deck: &[Tile]) -> Vec<Tile> {
        self.calls += 1;
        if self.calls == 1 {
            self.first_wall.clone()
        } else if self.calls == self.fail_on_call {
            vec![m(1); 10] // invalid length
        } else {
            self.valid_second_wall.clone()
        }
    }
}

#[test]
fn between_hands_confirmations_wall_validation_and_end_match_rejection_matrix() {
    let wall1 = dealer_win_wall();
    let wall2 = dealer_win_wall();
    let provider = FlakySecondWallProvider {
        initial_dealer: Seat::East,
        first_wall: wall1,
        valid_second_wall: wall2,
        calls: 0,
        fail_on_call: 2,
    };

    let mut free_game = Match::new(MatchMode::FreeTable, TuidaohuRuleset, provider).unwrap();

    // 1. EndMatch is rejected in TurnAction for both active seat (East) and inactive seat (South)
    //    without mutating state or sequence number.
    let seq_turn = free_game.seq();
    let views_turn = Seat::ALL.map(|s| project_view(&free_game, s));
    for seat in [Seat::East, Seat::South] {
        let err = free_game
            .apply_action(seat, PlayerAction::EndMatch)
            .unwrap_err();
        assert_eq!(
            err,
            MatchError::WrongPhase {
                seat,
                action: PlayerAction::EndMatch,
            }
        );
    }
    assert_eq!(free_game.seq(), seq_turn);
    assert_eq!(Seat::ALL.map(|s| project_view(&free_game, s)), views_turn);

    // Settle hand 1 into BetweenHands.
    free_game
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    assert_eq!(
        project_view(&free_game, Seat::East).phase,
        PublicPhase::BetweenHands
    );

    // 2. Confirm 1, 2, 3 out of order (North, West, East) stay in BetweenHands; duplicates fail.
    for (idx, seat) in [Seat::North, Seat::West, Seat::East]
        .into_iter()
        .enumerate()
    {
        let evs = free_game
            .apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(
            project_event(&evs[0], Seat::South).kind,
            PlayerEventKind::NextHandConfirmed { seat }
        );
        let view = project_view(&free_game, seat);
        assert_eq!(view.phase, PublicPhase::BetweenHands);
        assert!(!view.legal_actions.contains(&PlayerAction::ConfirmNextHand));
        assert!(view.legal_actions.contains(&PlayerAction::EndMatch));

        let seq_before_dup = free_game.seq();
        let err_dup = free_game
            .apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap_err();
        assert_eq!(err_dup, MatchError::AlreadyConfirmed { seat });
        assert_eq!(free_game.seq(), seq_before_dup);
        assert_eq!(
            project_view(&free_game, Seat::South)
                .ready_seats
                .unwrap()
                .iter()
                .filter(|&&r| r)
                .count(),
            idx + 1
        );
    }

    assert_eq!(
        project_view(&free_game, Seat::South).ready_seats,
        Some([true, false, true, true])
    );

    // 3. 4th confirmation triggers next_wall validation; invalid wall leaves BetweenHands untouched.
    let seq_before_bad_wall = free_game.seq();
    let views_before_bad_wall = Seat::ALL.map(|s| project_view(&free_game, s));
    let bad_wall_err = free_game
        .apply_action(Seat::South, PlayerAction::ConfirmNextHand)
        .unwrap_err();
    assert!(matches!(bad_wall_err, MatchError::InvalidWall(_)));
    assert_eq!(free_game.seq(), seq_before_bad_wall);
    assert_eq!(
        Seat::ALL.map(|s| project_view(&free_game, s)),
        views_before_bad_wall
    );

    // Retry 4th confirmation now succeeds cleanly and resets per-hand state.
    let start_evs = free_game
        .apply_action(Seat::South, PlayerAction::ConfirmNextHand)
        .unwrap();
    assert_eq!(start_evs.len(), 2);
    assert!(matches!(
        project_event(&start_evs[0], Seat::East).kind,
        PlayerEventKind::NextHandConfirmed { seat: Seat::South }
    ));
    assert!(matches!(
        project_event(&start_evs[1], Seat::East).kind,
        PlayerEventKind::HandStarted {
            hand_number: 1,
            dealer_streak: 1,
            dealer: Seat::East,
            wall_remaining: 83,
            ..
        }
    ));

    // Finish hand 2 into BetweenHands and test EndMatch -> MatchFinished rejection.
    free_game
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    free_game
        .apply_action(Seat::West, PlayerAction::EndMatch)
        .unwrap();
    let seq_finished = free_game.seq();
    assert_eq!(
        free_game
            .apply_action(Seat::East, PlayerAction::EndMatch)
            .unwrap_err(),
        MatchError::MatchAlreadyFinished
    );
    assert_eq!(free_game.seq(), seq_finished);

    // 4. EastOnly rejects EndMatch in BetweenHands without mutating state.
    let mut east_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, vec![dealer_win_wall(), dealer_win_wall()]),
    )
    .unwrap();
    east_game
        .apply_action(Seat::East, PlayerAction::Tsumo)
        .unwrap();
    let seq_east_between = east_game.seq();
    let views_east_between = Seat::ALL.map(|s| project_view(&east_game, s));
    for seat in Seat::ALL {
        assert!(!views_east_between[seat.index()]
            .legal_actions
            .contains(&PlayerAction::EndMatch));
        assert_eq!(
            east_game
                .apply_action(seat, PlayerAction::EndMatch)
                .unwrap_err(),
            MatchError::IllegalAction {
                seat,
                action: PlayerAction::EndMatch,
            }
        );
    }
    assert_eq!(east_game.seq(), seq_east_between);
    assert_eq!(
        Seat::ALL.map(|s| project_view(&east_game, s)),
        views_east_between
    );
}

fn dealer_four_fan_tsumo_wall() -> Vec<Tile> {
    let dealer_14 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
        NORTH_W, NORTH_W, RED_D, RED_D,
    ];
    let s1 = vec![
        m(1),
        m(1),
        m(1),
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
    ];
    let s2 = vec![
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
    let s3 = vec![
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
    ];
    build_preset_wall(&dealer_14, &s1, &s2, &s3, &[], &[]).unwrap()
}

fn next_seat_four_fan_tsumo_wall() -> Vec<Tile> {
    let dealer_14 = vec![
        m(1),
        m(1),
        m(1),
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
        m(6),
    ];
    let s1 = vec![
        EAST_W, EAST_W, EAST_W, SOUTH_W, SOUTH_W, SOUTH_W, WEST_W, WEST_W, WEST_W, NORTH_W,
        NORTH_W, NORTH_W, RED_D,
    ];
    let s2 = vec![
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
    let s3 = vec![
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
    ];
    build_preset_wall(&dealer_14, &s1, &s2, &s3, &[RED_D], &[]).unwrap()
}

#[test]
fn east_only_handles_dealer_streaks_negative_scores_east4_streak_and_finish_summary() {
    let walls = vec![
        next_seat_four_fan_tsumo_wall(), // Hand 1 (East 1, East): South wins +48 -> [84, 148, 84, 84]
        dealer_four_fan_tsumo_wall(),    // Hand 2 (East 2, South): South wins -> [68, 196, 68, 68]
        dealer_four_fan_tsumo_wall(), // Hand 3 (East 2 #1, South): South wins -> [52, 244, 52, 52]
        dealer_four_fan_tsumo_wall(), // Hand 4 (East 2 #2, South): South wins -> [36, 292, 36, 36]
        dealer_four_fan_tsumo_wall(), // Hand 5 (East 2 #3, South): South wins -> [20, 340, 20, 20]
        dealer_four_fan_tsumo_wall(), // Hand 6 (East 2 #4, South): South wins -> [4, 388, 4, 4]
        next_seat_four_fan_tsumo_wall(), // Hand 7 (East 2 #5, South): West wins -> [-12, 372, 52, -12] (NEGATIVE!)
        next_seat_four_fan_tsumo_wall(), // Hand 8 (East 3, West): North wins -> [-28, 356, 36, 36]
        dealer_four_fan_tsumo_wall(), // Hand 9 (East 4, North): North wins -> [-44, 340, 20, 84] (连庄 at East 4!)
        Tile::standard_deck_136(),    // Hand 10 (East 4 #1, North): DrawGame -> MatchFinished!
    ];

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, walls),
    )
    .expect("EastOnly match starts");

    assert_eq!(
        project_view(&game, Seat::East).seats.map(|s| s.score),
        [100, 100, 100, 100]
    );

    let confirm_all = |game: &mut Match<TuidaohuRuleset, PresetWallProvider>| {
        for seat in Seat::ALL {
            game.apply_action(seat, PlayerAction::ConfirmNextHand)
                .unwrap();
        }
    };

    // Hand 1: East discards m(6), South draws RED_D and Tsumo.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: m(6),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    // Hands 2..=6: South wins 5 consecutive dealer streaks at East 2.
    for expected_streak in 0..5u32 {
        let view = project_view(&game, Seat::South);
        assert_eq!(view.turn.wind_round, Wind::East);
        assert_eq!(view.turn.hand_number, 2);
        assert_eq!(view.turn.dealer_streak, expected_streak);
        assert_eq!(view.turn.dealer, Seat::South);

        game.apply_action(Seat::South, PlayerAction::Tsumo).unwrap();
        confirm_all(&mut game);
    }

    // Hand 7 (East 2, streak 5): West wins -> East and North go negative (-12) without bust!
    let pre_h7 = project_view(&game, Seat::South);
    assert_eq!(pre_h7.turn.hand_number, 2);
    assert_eq!(pre_h7.turn.dealer_streak, 5);
    assert_eq!(pre_h7.seats.map(|s| s.score), [4, 388, 4, 4]);

    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: m(6),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    let neg_view = project_view(&game, Seat::East);
    assert_eq!(neg_view.phase, PublicPhase::BetweenHands);
    assert_eq!(neg_view.seats.map(|s| s.score), [-12, 372, 52, -12]);
    confirm_all(&mut game);

    // Hand 8 (East 3, West): North wins -> rotates to East 4, North.
    let h8 = project_view(&game, Seat::West);
    assert_eq!(h8.turn.hand_number, 3);
    assert_eq!(h8.turn.dealer_streak, 0);
    assert_eq!(h8.turn.dealer, Seat::West);
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: m(6),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
    confirm_all(&mut game);

    // Hand 9 (East 4, North): dealer North wins -> MUST stay in BetweenHands and 连庄 at East 4!
    let h9 = project_view(&game, Seat::North);
    assert_eq!(h9.turn.wind_round, Wind::East);
    assert_eq!(h9.turn.hand_number, 4);
    assert_eq!(h9.turn.dealer_streak, 0);
    assert_eq!(h9.turn.dealer, Seat::North);
    assert_eq!(h9.seats.map(|s| s.score), [-28, 356, 36, 36]);

    let east4_streak_events = game.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
    assert_eq!(east4_streak_events.len(), 1);
    assert!(matches!(
        project_event(&east4_streak_events[0], Seat::East).kind,
        PlayerEventKind::HandSettled { .. }
    ));
    let east4_between = project_view(&game, Seat::East);
    assert_eq!(east4_between.phase, PublicPhase::BetweenHands);
    assert_eq!(east4_between.match_summary, None);
    assert_eq!(east4_between.seats.map(|s| s.score), [-44, 340, 20, 84]);
    confirm_all(&mut game);

    // Hand 10 (East 4, streak 1, North): play all 84 discards to DrawGame -> immediate MatchFinished!
    let h10 = project_view(&game, Seat::North);
    assert_eq!(h10.turn.wind_round, Wind::East);
    assert_eq!(h10.turn.hand_number, 4);
    assert_eq!(h10.turn.dealer_streak, 1);
    assert_eq!(h10.turn.dealer, Seat::North);

    let mut final_step_events = Vec::new();
    loop {
        let probe = project_view(&game, Seat::East);
        if probe.phase != PublicPhase::Playing {
            break;
        }
        let active = probe.turn.active_seat.unwrap();
        let active_view = project_view(&game, active);
        let discard = active_view
            .legal_actions
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
            .or_else(|| {
                active_view
                    .legal_actions
                    .iter()
                    .copied()
                    .find(|a| matches!(a, PlayerAction::Discard { .. }))
            })
            .unwrap();
        final_step_events = game.apply_action(active, discard).unwrap();
    }

    assert_eq!(final_step_events.len(), 3);
    assert!(matches!(
        project_event(&final_step_events[0], Seat::East).kind,
        PlayerEventKind::TileDiscarded { .. }
    ));
    assert!(matches!(
        project_event(&final_step_events[1], Seat::East).kind,
        PlayerEventKind::HandSettled { .. }
    ));
    let summary = match project_event(&final_step_events[2], Seat::East).kind {
        PlayerEventKind::MatchFinished { summary } => summary,
        other => panic!("expected MatchFinished after East 4 DrawGame, got {other:?}"),
    };

    assert_eq!(summary.mode, MatchMode::EastOnly);
    assert_eq!(summary.hands_played, 10);
    assert_eq!(summary.initial_scores, [100, 100, 100, 100]);
    assert_eq!(summary.final_scores, [-44, 340, 20, 84]);
    assert_eq!(summary.net_deltas, [-144, 240, -80, -16]);
    assert_eq!(summary.net_deltas.iter().sum::<i32>(), 0);

    let finished_view = project_view(&game, Seat::West);
    assert_eq!(finished_view.phase, PublicPhase::MatchFinished);
    assert_eq!(finished_view.turn.hand_number, 4);
    assert_eq!(finished_view.turn.dealer_streak, 1);
    assert_eq!(finished_view.turn.dealer, Seat::North);
    assert_eq!(finished_view.ready_seats, None);
    assert_eq!(finished_view.match_summary, Some(summary));
    assert!(finished_view.last_settlement.is_some());
    assert!(finished_view.legal_actions.is_empty());
}

fn winner_entry(seat: Seat, win_kind: WinKind, fan: u8) -> WinnerSettlement {
    WinnerSettlement {
        seat,
        win_kind,
        items: vec![ScoreItem {
            code: "menqing".to_string(),
            label: "门前清".to_string(),
            fan,
        }],
        total_fan: fan,
        base_points: 1i32 << fan,
    }
}

#[test]
fn multi_winner_hand_progression_and_ruleset_vs_any_ruleset_parity() {
    let tuidaohu = TuidaohuRuleset;
    let any = AnyRuleset::Tuidaohu(TuidaohuRuleset);
    assert_eq!(any.profile(), tuidaohu.profile());
    assert_eq!(any.deck(), tuidaohu.deck());
    assert_eq!(AnyRuleset::default().profile(), tuidaohu.profile());

    // Parity across reaction priority, kong deltas, and turn/reaction queries.
    for action in [
        PlayerAction::Tsumo,
        PlayerAction::Ron { tile: m(1) },
        PlayerAction::RobKongHu { tile: m(1) },
        PlayerAction::Pon { tile: m(1) },
        PlayerAction::ExposedKong { tile: m(1) },
        PlayerAction::Chi {
            tile: m(1),
            with: [m(2), m(3)],
        },
        PlayerAction::Pass,
    ] {
        assert_eq!(
            any.reaction_priority(&action),
            tuidaohu.reaction_priority(&action)
        );
    }
    assert_eq!(
        any.kong_score_deltas(KongKind::ExposedKong, Seat::East, Some(Seat::South)),
        tuidaohu.kong_score_deltas(KongKind::ExposedKong, Seat::East, Some(Seat::South))
    );
    assert_eq!(
        any.kong_score_deltas(KongKind::AddedKong, Seat::West, None),
        tuidaohu.kong_score_deltas(KongKind::AddedKong, Seat::West, None)
    );
    assert_eq!(
        any.kong_score_deltas(KongKind::ConcealedKong, Seat::North, None),
        tuidaohu.kong_score_deltas(KongKind::ConcealedKong, Seat::North, None)
    );

    let sample_13 = [
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
    let turn_ctx = TurnContext {
        closed_tiles: &sample_13,
        drawn_tile: Some(RED_D),
        melds: &[],
        source: TurnSource::NormalDraw,
        wall_remaining: 15,
    };
    assert_eq!(
        any.legal_turn_actions(&turn_ctx),
        tuidaohu.legal_turn_actions(&turn_ctx)
    );
    let discard_ctx = DiscardReactionContext {
        actor: Seat::South,
        discarder: Seat::East,
        tile: RED_D,
        closed_tiles: &sample_13,
        melds: &[],
        wall_remaining: 15,
        passed_hu_lock: false,
        passed_pon_locked: false,
        is_next_seat: true,
    };
    assert_eq!(
        any.legal_discard_reactions(&discard_ctx),
        tuidaohu.legal_discard_reactions(&discard_ctx)
    );
    let rob_ctx = RobKongContext {
        actor: Seat::South,
        kong_actor: Seat::East,
        tile: RED_D,
        closed_tiles: &sample_13,
        melds: &[],
        wall_remaining: 15,
        passed_hu_lock: false,
    };
    assert_eq!(
        any.legal_rob_kong_reactions(&rob_ctx),
        tuidaohu.legal_rob_kong_reactions(&rob_ctx)
    );

    let win_eval = WinEvalInput {
        closed_tiles: &sample_13,
        extra_tile: Some(RED_D),
        melds: &[],
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: 15,
    };
    assert_eq!(
        any.evaluate_win(&win_eval),
        tuidaohu.evaluate_win(&win_eval)
    );

    // Multi-winner (一炮多响) HandProgression cases:
    let multi_with_dealer = HandOutcome::Win {
        winners: vec![
            winner_entry(Seat::West, WinKind::Ron, 1),
            winner_entry(Seat::East, WinKind::Ron, 2),
        ],
        discarder: Some(Seat::South),
        winning_tile: RED_D,
    };
    let multi_without_dealer = HandOutcome::Win {
        winners: vec![
            winner_entry(Seat::South, WinKind::Ron, 1),
            winner_entry(Seat::West, WinKind::Ron, 2),
            winner_entry(Seat::North, WinKind::Ron, 1),
        ],
        discarder: Some(Seat::East),
        winning_tile: RED_D,
    };

    // 1. Multi-winner including dealer triggers 连庄 even at East 4 in EastOnly and FreeTable.
    for mode in [MatchMode::EastOnly, MatchMode::FreeTable] {
        let input = HandProgressionInput {
            mode,
            wind_round: Wind::East,
            hand_number: 4,
            dealer_streak: 2,
            dealer: Seat::East,
            outcome: &multi_with_dealer,
        };
        let expected = HandProgression::NextHand {
            wind_round: Wind::East,
            hand_number: 4,
            dealer_streak: 3,
            dealer: Seat::East,
        };
        assert_eq!(tuidaohu.advance_hand(input), expected);
        assert_eq!(any.advance_hand(input), expected);
    }

    // 2. Multi-winner without dealer rotates counter-clockwise and finishes EastOnly at East 4.
    let east1_non_dealer_multi = HandProgressionInput {
        mode: MatchMode::EastOnly,
        wind_round: Wind::East,
        hand_number: 1,
        dealer_streak: 3,
        dealer: Seat::East,
        outcome: &multi_without_dealer,
    };
    let expected_east2 = HandProgression::NextHand {
        wind_round: Wind::East,
        hand_number: 2,
        dealer_streak: 0,
        dealer: Seat::South,
    };
    assert_eq!(
        tuidaohu.advance_hand(east1_non_dealer_multi),
        expected_east2
    );
    assert_eq!(any.advance_hand(east1_non_dealer_multi), expected_east2);

    let east4_non_dealer_multi_east_only = HandProgressionInput {
        mode: MatchMode::EastOnly,
        wind_round: Wind::East,
        hand_number: 4,
        dealer_streak: 1,
        dealer: Seat::East,
        outcome: &multi_without_dealer,
    };
    assert_eq!(
        tuidaohu.advance_hand(east4_non_dealer_multi_east_only),
        HandProgression::FinishMatch
    );
    assert_eq!(
        any.advance_hand(east4_non_dealer_multi_east_only),
        HandProgression::FinishMatch
    );

    // 3. FreeTable East 4 -> South 1 and North 4 -> East 1 on multi-winner without dealer.
    let multi_without_north_dealer = HandOutcome::Win {
        winners: vec![
            winner_entry(Seat::East, WinKind::Ron, 1),
            winner_entry(Seat::South, WinKind::Ron, 2),
        ],
        discarder: Some(Seat::West),
        winning_tile: RED_D,
    };
    let free_east4 = HandProgressionInput {
        mode: MatchMode::FreeTable,
        wind_round: Wind::East,
        hand_number: 4,
        dealer_streak: 1,
        dealer: Seat::North,
        outcome: &multi_without_north_dealer,
    };
    assert_eq!(
        any.advance_hand(free_east4),
        HandProgression::NextHand {
            wind_round: Wind::South,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
        }
    );
    let free_north4 = HandProgressionInput {
        mode: MatchMode::FreeTable,
        wind_round: Wind::North,
        hand_number: 4,
        dealer_streak: 0,
        dealer: Seat::North,
        outcome: &multi_without_north_dealer,
    };
    assert_eq!(
        any.advance_hand(free_north4),
        HandProgression::NextHand {
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: Seat::East,
        }
    );

    // 4. Long-horizon FreeTable progression (> 255 hands) never overflows hand_number.
    let mut wind = Wind::East;
    let mut hand = 1u8;
    let mut streak = 0u32;
    let mut dealer = Seat::East;
    for _ in 0..300 {
        let next = any.advance_hand(HandProgressionInput {
            mode: MatchMode::FreeTable,
            wind_round: wind,
            hand_number: hand,
            dealer_streak: streak,
            dealer,
            outcome: &HandOutcome::DrawGame,
        });
        match next {
            HandProgression::NextHand {
                wind_round,
                hand_number,
                dealer_streak,
                dealer: next_dealer,
            } => {
                assert!((1..=4).contains(&hand_number));
                assert_eq!(dealer_streak, 0);
                assert_eq!(next_dealer, dealer.next());
                wind = wind_round;
                hand = hand_number;
                streak = dealer_streak;
                dealer = next_dealer;
            }
            HandProgression::FinishMatch => panic!("FreeTable must never auto-finish"),
        }
    }

    // 5. Multi-winner settle_hand parity between TuidaohuRuleset and AnyRuleset.
    let revealed = std::array::from_fn(|idx| RevealedHand {
        seat: Seat::from_index(idx).unwrap(),
        closed_tiles: sample_13.to_vec(),
        drawn_tile: None,
        melds: Vec::new(),
    });
    let settle_input = SettleHandInput {
        wind_round: Wind::East,
        hand_number: 2,
        dealer_streak: 0,
        dealer: Seat::South,
        current_scores: [100; 4],
        kong_transfers: vec![KongTransfer {
            kind: KongKind::ExposedKong,
            actor: Seat::East,
            from_seat: Some(Seat::North),
            tile: m(9),
            score_deltas: [2, 0, 0, -2],
        }],
        revealed_hands: revealed,
        win_details: Some(WinSettlementDetails {
            winners: vec![
                WinCandidate {
                    seat: Seat::South,
                    win_kind: WinKind::Ron,
                    evaluated: EvaluatedWin {
                        items: vec![ScoreItem {
                            code: "menqing".to_string(),
                            label: "门前清".to_string(),
                            fan: 1,
                        }],
                        total_fan: 1,
                        base_points: 2,
                    },
                },
                WinCandidate {
                    seat: Seat::West,
                    win_kind: WinKind::Ron,
                    evaluated: EvaluatedWin {
                        items: vec![ScoreItem {
                            code: "full_flush".to_string(),
                            label: "清一色".to_string(),
                            fan: 2,
                        }],
                        total_fan: 2,
                        base_points: 4,
                    },
                },
            ],
            discarder: Some(Seat::East),
            winning_tile: RED_D,
        }),
    };
    let s_tuidaohu = tuidaohu.settle_hand(settle_input.clone());
    let s_any = any.settle_hand(settle_input);
    assert_eq!(s_tuidaohu, s_any);
    // East: +2 (kong) - 2 (to South) - 4 (to West) = -4; South: +2; West: +4; North: -2 (kong).
    assert_eq!(s_tuidaohu.score_deltas, [-4, 2, 4, -2]);
    assert_eq!(s_tuidaohu.new_scores, [96, 102, 104, 98]);
}

#[test]
fn any_ruleset_seeded_match_parity_and_free_table_full_wind_cycle() {
    // 1. 17-hand FreeTable full cycle across East/South/West/North winds back to East 1.
    let walls: Vec<Vec<Tile>> = (0..17).map(|_| next_seat_quick_win_wall()).collect();
    let mut cycle_game = Match::new(
        MatchMode::FreeTable,
        AnyRuleset::default(),
        PresetWallProvider::new(Seat::East, walls),
    )
    .unwrap();

    let expected_rounds = [
        (Wind::East, 1, Seat::East),
        (Wind::East, 2, Seat::South),
        (Wind::East, 3, Seat::West),
        (Wind::East, 4, Seat::North),
        (Wind::South, 1, Seat::East),
        (Wind::South, 2, Seat::South),
        (Wind::South, 3, Seat::West),
        (Wind::South, 4, Seat::North),
        (Wind::West, 1, Seat::East),
        (Wind::West, 2, Seat::South),
        (Wind::West, 3, Seat::West),
        (Wind::West, 4, Seat::North),
        (Wind::North, 1, Seat::East),
        (Wind::North, 2, Seat::South),
        (Wind::North, 3, Seat::West),
        (Wind::North, 4, Seat::North),
        (Wind::East, 1, Seat::East),
    ];

    for (hand_idx, &(exp_wind, exp_hand, exp_dealer)) in expected_rounds.iter().enumerate() {
        let view = project_view(&cycle_game, exp_dealer);
        assert_eq!(view.turn.wind_round, exp_wind);
        assert_eq!(view.turn.hand_number, exp_hand);
        assert_eq!(view.turn.dealer, exp_dealer);
        assert_eq!(view.turn.dealer_streak, 0);

        cycle_game
            .apply_action(
                exp_dealer,
                PlayerAction::Discard {
                    tile: WHITE_D,
                    from_drawn: false,
                },
            )
            .unwrap();
        cycle_game
            .apply_action(exp_dealer.next(), PlayerAction::Tsumo)
            .unwrap();

        if hand_idx + 1 < expected_rounds.len() {
            for seat in Seat::ALL {
                cycle_game
                    .apply_action(seat, PlayerAction::ConfirmNextHand)
                    .unwrap();
            }
        } else {
            cycle_game
                .apply_action(Seat::East, PlayerAction::EndMatch)
                .unwrap();
        }
    }

    let summary = project_view(&cycle_game, Seat::East)
        .match_summary
        .expect("MatchSummary present");
    assert_eq!(summary.hands_played, 17);
    // East won hands 4, 8, 12, 16 -> 4*(+6) + 13*(-2) = -2;
    // South won hands 1, 5, 9, 13, 17 -> 5*(+6) + 12*(-2) = +6;
    // West won hands 2, 6, 10, 14 -> -2; North won hands 3, 7, 11, 15 -> -2.
    assert_eq!(summary.final_scores, [-2, 6, -2, -2]);
    assert_eq!(summary.net_deltas, [-2, 6, -2, -2]);

    // 2. Seeded multi-hand Match parity between TuidaohuRuleset and AnyRuleset in both modes.
    fn drive_seeded<R: Ruleset>(
        mode: MatchMode,
        ruleset: R,
        seed: u64,
        max_hands: usize,
    ) -> (Vec<[PlayerView; 4]>, Vec<[PlayerEvent; 4]>) {
        let mut game = Match::new(mode, ruleset, SeededWallProvider::new(seed)).unwrap();
        let mut views = vec![Seat::ALL.map(|s| project_view(&game, s))];
        let mut events: Vec<[PlayerEvent; 4]> = game
            .events()
            .iter()
            .map(|ev| Seat::ALL.map(|s| project_event(ev, s)))
            .collect();
        let mut hands_completed = 0usize;

        loop {
            let probe = project_view(&game, Seat::East);
            match probe.phase {
                PublicPhase::Playing => {
                    let active = probe.turn.active_seat.unwrap();
                    let active_view = project_view(&game, active);
                    let action = active_view
                        .legal_actions
                        .iter()
                        .copied()
                        .find(|a| matches!(a, PlayerAction::Tsumo))
                        .or_else(|| {
                            active_view.legal_actions.iter().copied().find(|a| {
                                matches!(
                                    a,
                                    PlayerAction::Discard {
                                        from_drawn: true,
                                        ..
                                    }
                                )
                            })
                        })
                        .unwrap_or(active_view.legal_actions[0]);
                    let emitted = game.apply_action(active, action).unwrap();
                    for ev in &emitted {
                        events.push(Seat::ALL.map(|s| project_event(ev, s)));
                    }
                    views.push(Seat::ALL.map(|s| project_view(&game, s)));
                }
                PublicPhase::BetweenHands => {
                    hands_completed += 1;
                    if mode == MatchMode::FreeTable && hands_completed >= max_hands {
                        let emitted = game
                            .apply_action(Seat::East, PlayerAction::EndMatch)
                            .unwrap();
                        for ev in &emitted {
                            events.push(Seat::ALL.map(|s| project_event(ev, s)));
                        }
                        views.push(Seat::ALL.map(|s| project_view(&game, s)));
                    } else {
                        for seat in Seat::ALL {
                            let emitted = game
                                .apply_action(seat, PlayerAction::ConfirmNextHand)
                                .unwrap();
                            for ev in &emitted {
                                events.push(Seat::ALL.map(|s| project_event(ev, s)));
                            }
                            views.push(Seat::ALL.map(|s| project_view(&game, s)));
                        }
                    }
                }
                PublicPhase::MatchFinished => break,
            }
        }
        (views, events)
    }

    assert_eq!(
        drive_seeded(MatchMode::EastOnly, TuidaohuRuleset, 2026_0019, 10),
        drive_seeded(MatchMode::EastOnly, AnyRuleset::default(), 2026_0019, 10)
    );
    assert_eq!(
        drive_seeded(MatchMode::FreeTable, TuidaohuRuleset, 2026_0020, 6),
        drive_seeded(MatchMode::FreeTable, AnyRuleset::default(), 2026_0020, 6)
    );
}

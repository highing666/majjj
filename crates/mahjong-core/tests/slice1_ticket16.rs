mod common;

use mahjong_core::{
    build_preset_wall, project_event, project_view, AnyRuleset, HandOutcome, Match, MatchError,
    MatchMode, PlayerAction, PlayerEvent, PlayerEventKind, PlayerView, PresetWallProvider,
    PublicPhase, RuleProfile, Seat, SeededWallProvider, Suit, Tile, TuidaohuRuleset, Wind,
};
use ts_rs::TS;

#[test]
fn common_tile_notation_parses_all_34_kinds() {
    let mut all = common::tiles("123456789m 123456789s 123456789p ESWNCFB");
    let expected: Vec<Tile> = Tile::all().collect();
    assert_eq!(all, expected);

    assert_eq!(common::tiles("1234567z"), common::tiles("ESWNCFB"));
    assert_eq!(common::tile("5m"), t(4));
    assert_eq!(common::tile("C"), t(31));
    assert_eq!(common::tile("7z"), t(33));

    all.clear();
}

#[test]
fn slice1_tile_encoding_and_standard_deck_invariants() {
    assert_eq!(Tile::new(34), None);
    assert_eq!(Tile::new(255), None);

    let deck = Tile::standard_deck_136();
    assert_eq!(deck.len(), 136);

    let mut counts = [0u8; 34];
    for &t in &deck {
        counts[t.index()] += 1;
    }
    assert_eq!(counts, [4u8; 34]);

    let one_man = Tile::new(0).unwrap();
    assert_eq!(one_man.suit(), Suit::Man);
    assert_eq!(one_man.rank(), Some(1));
    assert!(one_man.is_terminal());
    assert!(!one_man.is_honor());
    assert!(one_man.is_terminal_or_honor());

    let five_bamboo = Tile::new(13).unwrap();
    assert_eq!(five_bamboo.suit(), Suit::Bamboo);
    assert_eq!(five_bamboo.rank(), Some(5));
    assert!(!five_bamboo.is_terminal_or_honor());

    let nine_dot = Tile::new(26).unwrap();
    assert_eq!(nine_dot.suit(), Suit::Dot);
    assert_eq!(nine_dot.rank(), Some(9));
    assert!(nine_dot.is_terminal());

    let east = Tile::new(27).unwrap();
    assert_eq!(east.suit(), Suit::Wind);
    assert_eq!(east.rank(), None);
    assert!(east.is_honor());
    assert!(!east.is_terminal());
    assert!(east.is_terminal_or_honor());

    let white = Tile::new(33).unwrap();
    assert_eq!(white.suit(), Suit::Dragon);
    assert_eq!(white.rank(), None);
    assert!(white.is_honor());

    assert_eq!(Seat::East.next(), Seat::South);
    assert_eq!(Seat::South.next(), Seat::West);
    assert_eq!(Seat::West.next(), Seat::North);
    assert_eq!(Seat::North.next(), Seat::East);
}

#[test]
fn slice1_match_start_deals_14_and_13_and_projects_isolated_views() {
    let game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        SeededWallProvider::new(42),
    )
    .expect("seeded match should initialize");

    assert_eq!(game.seq(), 2);
    assert_eq!(game.events().len(), 2);

    let ev_match_started = project_event(&game.events()[0], Seat::East);
    assert_eq!(ev_match_started.seq, 1);
    let initial_dealer = match ev_match_started.kind {
        PlayerEventKind::MatchStarted {
            mode,
            initial_dealer,
            initial_scores,
            rule_profile,
        } => {
            assert_eq!(mode, MatchMode::EastOnly);
            assert_eq!(initial_scores, [100; 4]);
            assert!(!rule_profile.allow_chi);
            assert_eq!(rule_profile.wall_size, 136);
            assert_eq!(rule_profile.dead_wall_size, 0);
            assert_eq!(rule_profile.fan_cap, 4);
            initial_dealer
        }
        other => panic!("expected MatchStarted, got {other:?}"),
    };

    for seat in Seat::ALL {
        let view = project_view(&game, seat);
        assert_eq!(view.viewer, seat);
        assert_eq!(view.mode, MatchMode::EastOnly);
        assert_eq!(view.phase, PublicPhase::Playing);
        assert_eq!(view.wall_remaining, 83);
        assert_eq!(view.seq, 2);
        assert_eq!(view.turn.wind_round, Wind::East);
        assert_eq!(view.turn.hand_number, 1);
        assert_eq!(view.turn.dealer_streak, 0);
        assert_eq!(view.turn.dealer, initial_dealer);
        assert_eq!(view.turn.active_seat, Some(initial_dealer));
        assert_eq!(view.last_settlement, None);
        assert_eq!(view.ready_seats, None);
        assert_eq!(view.match_summary, None);
        assert_eq!(view.hand.drawn_tile, None);

        let expected_len = if seat == initial_dealer { 14 } else { 13 };
        assert_eq!(view.hand.closed_tiles.len(), expected_len);
        assert!(view.hand.closed_tiles.windows(2).all(|w| w[0] <= w[1]));

        let started = game.project_event(&game.events()[1], seat);
        assert_eq!(started.seq, 2);
        match started.kind {
            PlayerEventKind::HandStarted {
                hand,
                dealer,
                wall_remaining,
                scores,
                ..
            } => {
                assert_eq!(dealer, initial_dealer);
                assert_eq!(wall_remaining, 83);
                assert_eq!(scores, [100; 4]);
                assert_eq!(hand, view.hand);
            }
            other => panic!("expected HandStarted, got {other:?}"),
        }

        for seat_view in &view.seats {
            assert_eq!(seat_view.last_draw_or_meld_seq, 0);
            assert!(seat_view.river.is_empty());
            assert!(seat_view.melds.is_empty());
            let expected_count = if seat_view.seat == initial_dealer {
                14
            } else {
                13
            };
            assert_eq!(seat_view.hand_count, expected_count);
        }

        if seat == initial_dealer {
            assert!(!view.legal_actions.is_empty());
            assert!(view.legal_actions.iter().all(|a| {
                matches!(
                    a,
                    PlayerAction::Discard {
                        tile,
                        from_drawn: false
                    } if view.hand.closed_tiles.contains(tile)
                )
            }));
        } else {
            assert!(view.legal_actions.is_empty());
        }
    }
}

#[test]
fn slice1_rejects_invalid_walls() {
    let err_len = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, vec![Tile::new(0).unwrap(); 10]),
    )
    .unwrap_err();
    assert!(matches!(err_len, MatchError::InvalidWall(_)));

    let bad_multiset = vec![Tile::new(0).unwrap(); 136];
    let err_multiset = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, bad_multiset),
    )
    .unwrap_err();
    assert!(matches!(err_multiset, MatchError::InvalidWall(_)));

    let five_ones = vec![Tile::new(0).unwrap(); 5];
    let err_builder = build_preset_wall(&five_ones, &[], &[], &[], &[], &[]).unwrap_err();
    assert!(matches!(err_builder, MatchError::InvalidWall(_)));
}

fn t(raw: u8) -> Tile {
    Tile::new(raw).unwrap()
}

#[test]
fn slice2_dealer_first_discard_is_tsumogiri_false_and_advances_counter_clockwise() {
    let dealer_14 = vec![
        t(0),
        t(1),
        t(2),
        t(3),
        t(4),
        t(5),
        t(6),
        t(7),
        t(8),
        t(9),
        t(10),
        t(11),
        t(12),
        t(13),
    ];
    let seat1_13 = vec![
        t(0),
        t(1),
        t(2),
        t(3),
        t(4),
        t(5),
        t(6),
        t(7),
        t(8),
        t(9),
        t(10),
        t(11),
        t(12),
    ];
    let seat2_13 = vec![
        t(14),
        t(15),
        t(16),
        t(17),
        t(18),
        t(19),
        t(20),
        t(21),
        t(22),
        t(23),
        t(24),
        t(25),
        t(26),
    ];
    let seat3_13 = vec![
        t(14),
        t(15),
        t(16),
        t(17),
        t(18),
        t(19),
        t(20),
        t(21),
        t(22),
        t(23),
        t(24),
        t(25),
        t(26),
    ];
    let head_draws = vec![t(31), t(32)];
    let wall = build_preset_wall(
        &dealer_14,
        &seat1_13,
        &seat2_13,
        &seat3_13,
        &head_draws,
        &[],
    )
    .expect("valid preset wall");

    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::South, wall),
    )
    .expect("match starts");

    // Dealer cannot discard with from_drawn: true on InitialDealer.
    let err = game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: true,
            },
        )
        .unwrap_err();
    assert!(matches!(err, MatchError::IllegalAction { .. }));
    assert_eq!(game.seq(), 2);

    let events = game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: false,
            },
        )
        .expect("dealer first discard succeeds");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq(), 3);
    assert_eq!(events[1].seq(), 4);

    match project_event(&events[0], Seat::East).kind {
        PlayerEventKind::TileDiscarded {
            seat,
            tile,
            is_tsumogiri,
        } => {
            assert_eq!(seat, Seat::South);
            assert_eq!(tile, t(0));
            assert!(!is_tsumogiri);
        }
        other => panic!("expected TileDiscarded, got {other:?}"),
    }

    match project_event(&events[1], Seat::West).kind {
        PlayerEventKind::TileDrawn {
            seat,
            tile,
            is_replacement,
            wall_remaining,
        } => {
            assert_eq!(seat, Seat::West);
            assert_eq!(tile, Some(t(31)));
            assert!(!is_replacement);
            assert_eq!(wall_remaining, 82);
        }
        other => panic!("expected TileDrawn for West, got {other:?}"),
    }

    for hidden_seat in [Seat::East, Seat::South, Seat::North] {
        match project_event(&events[1], hidden_seat).kind {
            PlayerEventKind::TileDrawn {
                seat,
                tile,
                is_replacement,
                wall_remaining,
            } => {
                assert_eq!(seat, Seat::West);
                assert_eq!(tile, None);
                assert!(!is_replacement);
                assert_eq!(wall_remaining, 82);
            }
            other => panic!("expected TileDrawn with hidden tile, got {other:?}"),
        }
    }

    let west_view = project_view(&game, Seat::West);
    assert_eq!(west_view.seq, 4);
    assert_eq!(west_view.wall_remaining, 82);
    assert_eq!(west_view.turn.active_seat, Some(Seat::West));
    assert_eq!(west_view.hand.closed_tiles.len(), 13);
    assert_eq!(west_view.hand.drawn_tile, Some(t(31)));
    assert_eq!(west_view.seats[Seat::South.index()].hand_count, 13);
    assert_eq!(west_view.seats[Seat::West.index()].hand_count, 14);
    assert_eq!(west_view.seats[Seat::West.index()].last_draw_or_meld_seq, 4);
    assert_eq!(
        west_view.seats[Seat::South.index()].last_draw_or_meld_seq,
        0
    );
    assert_eq!(west_view.seats[Seat::South.index()].river.len(), 1);
    assert_eq!(west_view.seats[Seat::South.index()].river[0].tile, t(0));
    assert!(!west_view.seats[Seat::South.index()].river[0].is_tsumogiri);
    assert_eq!(west_view.seats[Seat::South.index()].river[0].seq, 3);
}

#[test]
fn slice2_tsumogiri_and_shouqie_including_empty_shouqie_recording() {
    let dealer_14 = vec![
        t(0),
        t(0),
        t(0),
        t(1),
        t(1),
        t(1),
        t(2),
        t(2),
        t(2),
        t(3),
        t(3),
        t(3),
        t(4),
        t(4),
    ];
    let south_13 = vec![
        t(5),
        t(5),
        t(5),
        t(6),
        t(6),
        t(6),
        t(7),
        t(7),
        t(7),
        t(8),
        t(8),
        t(8),
        t(9),
    ];
    let west_13 = vec![
        t(10),
        t(10),
        t(10),
        t(11),
        t(11),
        t(11),
        t(12),
        t(12),
        t(12),
        t(13),
        t(13),
        t(13),
        t(14),
    ];
    let north_13 = vec![
        t(15),
        t(15),
        t(15),
        t(16),
        t(16),
        t(16),
        t(17),
        t(17),
        t(17),
        t(18),
        t(18),
        t(18),
        t(19),
    ];
    // South draws 20 (摸切), West draws 14 (already in closed hand -> empty 手切), North draws 25 (手切).
    let head_draws = vec![t(20), t(14), t(25), t(26)];
    let wall = build_preset_wall(&dealer_14, &south_13, &west_13, &north_13, &head_draws, &[])
        .expect("valid wall");

    let mut game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall),
    )
    .expect("match starts");

    // 1. East (dealer) discards 0m -> South draws 20.
    game.apply_action(
        Seat::East,
        PlayerAction::Discard {
            tile: t(0),
            from_drawn: false,
        },
    )
    .unwrap();

    // 2. South performs tsumogiri on drawn 20.
    let south_view = project_view(&game, Seat::South);
    assert!(south_view.legal_actions.contains(&PlayerAction::Discard {
        tile: t(20),
        from_drawn: true
    }));
    assert!(!south_view.legal_actions.contains(&PlayerAction::Discard {
        tile: t(20),
        from_drawn: false
    }));

    let south_events = game
        .apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t(20),
                from_drawn: true,
            },
        )
        .unwrap();
    match project_event(&south_events[0], Seat::East).kind {
        PlayerEventKind::TileDiscarded {
            seat,
            tile,
            is_tsumogiri,
        } => {
            assert_eq!(seat, Seat::South);
            assert_eq!(tile, t(20));
            assert!(is_tsumogiri);
        }
        other => panic!("expected TileDiscarded, got {other:?}"),
    }
    let south_after = project_view(&game, Seat::South);
    assert_eq!(south_after.hand.closed_tiles, south_13);
    assert_eq!(south_after.hand.drawn_tile, None);
    assert!(south_after.seats[Seat::South.index()].river[0].is_tsumogiri);

    // North holds 1p*3 + 2p and sees a Ron window on South's 3p (t(20)); pass so West draws 14.
    game.apply_action(Seat::North, PlayerAction::Pass).unwrap();

    // 3. West has 14 in closed_tiles AND drawn_tile == Some(14) -> both from_drawn flags are legal!
    let west_view = project_view(&game, Seat::West);
    assert!(west_view.legal_actions.contains(&PlayerAction::Discard {
        tile: t(14),
        from_drawn: true
    }));
    assert!(west_view.legal_actions.contains(&PlayerAction::Discard {
        tile: t(14),
        from_drawn: false
    }));
    let west_events = game
        .apply_action(
            Seat::West,
            PlayerAction::Discard {
                tile: t(14),
                from_drawn: false,
            },
        )
        .unwrap(); // empty 手切
    match project_event(&west_events[0], Seat::East).kind {
        PlayerEventKind::TileDiscarded {
            seat,
            tile,
            is_tsumogiri,
        } => {
            assert_eq!(seat, Seat::West);
            assert_eq!(tile, t(14));
            assert!(!is_tsumogiri);
        }
        other => panic!("expected TileDiscarded, got {other:?}"),
    }
    let west_after = project_view(&game, Seat::West);
    assert_eq!(west_after.hand.closed_tiles, west_13);
    assert_eq!(west_after.hand.drawn_tile, None);
    assert!(!west_after.seats[Seat::West.index()].river[0].is_tsumogiri);

    // 4. North has drawn 25, discards 15 (closed tile != drawn) -> drawn 25 sorted into closed_tiles.
    let north_events = game
        .apply_action(
            Seat::North,
            PlayerAction::Discard {
                tile: t(15),
                from_drawn: false,
            },
        )
        .unwrap();
    match project_event(&north_events[0], Seat::East).kind {
        PlayerEventKind::TileDiscarded {
            seat,
            tile,
            is_tsumogiri,
        } => {
            assert_eq!(seat, Seat::North);
            assert_eq!(tile, t(15));
            assert!(!is_tsumogiri);
        }
        other => panic!("expected TileDiscarded, got {other:?}"),
    }
    let north_after = project_view(&game, Seat::North);
    assert_eq!(north_after.hand.drawn_tile, None);
    assert_eq!(north_after.hand.closed_tiles.len(), 13);
    assert!(north_after.hand.closed_tiles.contains(&t(25)));
    assert!(north_after
        .hand
        .closed_tiles
        .windows(2)
        .all(|w| w[0] <= w[1]));
}

fn play_current_hand_to_settlement<R: mahjong_core::Ruleset, W: mahjong_core::WallProvider>(
    game: &mut Match<R, W>,
) -> Vec<mahjong_core::RecordedEvent> {
    let mut all_events = Vec::new();
    let mut last_seq = game.seq();
    let mut discards = 0usize;

    loop {
        let probe = project_view(game, Seat::East);
        if probe.phase != PublicPhase::Playing {
            break;
        }
        if let Some(reactor) = Seat::ALL.into_iter().find(|&s| {
            project_view(game, s)
                .legal_actions
                .contains(&PlayerAction::Pass)
        }) {
            let emitted = game
                .apply_action(reactor, PlayerAction::Pass)
                .expect("legal Pass in reaction window");
            for ev in &emitted {
                assert_eq!(ev.seq(), last_seq + 1, "seq must increase strictly by 1");
                last_seq = ev.seq();
            }
            all_events.extend(emitted);
            continue;
        }
        let active = probe.turn.active_seat.expect("active seat during Playing");
        let active_view = project_view(game, active);

        let total_hand: usize = active_view
            .seats
            .iter()
            .map(|s| usize::from(s.hand_count))
            .sum();
        let total_river: usize = active_view
            .seats
            .iter()
            .flat_map(|s| s.river.iter())
            .filter(|entry| entry.claimed_by.is_none())
            .count();
        assert_eq!(
            total_hand + total_river + usize::from(active_view.wall_remaining),
            136,
            "136-tile conservation must hold before every action"
        );

        let action = active_view
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
            .expect("discard must be legal on TurnAction");

        let emitted = game.apply_action(active, action).expect("legal discard");
        discards += 1;
        for ev in &emitted {
            assert_eq!(ev.seq(), last_seq + 1, "seq must increase strictly by 1");
            last_seq = ev.seq();
        }
        all_events.extend(emitted);
    }

    assert_eq!(discards, 84, "136-tile draw game has exactly 84 discards");
    all_events
}

#[test]
fn slice3_full_hand_to_draw_game_and_four_confirmations_starts_next_hand() {
    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        SeededWallProvider::new(100),
    )
    .expect("match starts");

    let initial_dealer = project_view(&game, Seat::East).turn.dealer;
    let hand1_events = play_current_hand_to_settlement(&mut game);

    let last_ev = project_event(hand1_events.last().unwrap(), Seat::East);
    let settlement = match last_ev.kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled at end of 136 tiles, got {other:?}"),
    };
    assert_eq!(settlement.outcome, HandOutcome::DrawGame);
    assert_eq!(settlement.score_deltas, [0; 4]);
    assert_eq!(settlement.new_scores, [100; 4]);
    assert!(settlement.kong_transfers.is_empty());
    for revealed in &settlement.revealed_hands {
        assert_eq!(revealed.closed_tiles.len(), 13);
        assert_eq!(revealed.drawn_tile, None);
    }

    let between_view = project_view(&game, Seat::East);
    assert_eq!(between_view.phase, PublicPhase::BetweenHands);
    assert_eq!(between_view.turn.active_seat, None);
    assert_eq!(between_view.wall_remaining, 0);
    assert_eq!(between_view.ready_seats, Some([false; 4]));
    assert_eq!(between_view.last_settlement, Some(settlement));
    assert_eq!(
        between_view.legal_actions,
        vec![PlayerAction::ConfirmNextHand]
    );

    // Wrong phase and illegal EndMatch in EastOnly do not mutate state.
    let seq_before_errors = game.seq();
    let err_discard = game
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: false,
            },
        )
        .unwrap_err();
    assert!(matches!(err_discard, MatchError::WrongPhase { .. }));
    let err_end = game
        .apply_action(Seat::East, PlayerAction::EndMatch)
        .unwrap_err();
    assert!(matches!(err_end, MatchError::IllegalAction { .. }));
    assert_eq!(game.seq(), seq_before_errors);

    // First confirmation from East, then duplicate rejected.
    let c1 = game
        .apply_action(Seat::East, PlayerAction::ConfirmNextHand)
        .unwrap();
    assert_eq!(c1.len(), 1);
    assert!(matches!(
        project_event(&c1[0], Seat::South).kind,
        PlayerEventKind::NextHandConfirmed { seat: Seat::East }
    ));
    let east_between = project_view(&game, Seat::East);
    assert_eq!(east_between.ready_seats, Some([true, false, false, false]));
    assert!(east_between.legal_actions.is_empty());

    let seq_before_dup = game.seq();
    let err_dup = game
        .apply_action(Seat::East, PlayerAction::ConfirmNextHand)
        .unwrap_err();
    assert!(matches!(err_dup, MatchError::AlreadyConfirmed { .. }));
    assert_eq!(game.seq(), seq_before_dup);

    game.apply_action(Seat::South, PlayerAction::ConfirmNextHand)
        .unwrap();
    game.apply_action(Seat::West, PlayerAction::ConfirmNextHand)
        .unwrap();

    // 4th confirmation deals and starts Hand 2.
    let c4 = game
        .apply_action(Seat::North, PlayerAction::ConfirmNextHand)
        .unwrap();
    assert_eq!(c4.len(), 2);
    assert!(matches!(
        project_event(&c4[0], Seat::East).kind,
        PlayerEventKind::NextHandConfirmed { seat: Seat::North }
    ));
    let next_dealer = initial_dealer.next();
    match project_event(&c4[1], next_dealer).kind {
        PlayerEventKind::HandStarted {
            wind_round,
            hand_number,
            dealer_streak,
            dealer,
            hand,
            wall_remaining,
            scores,
        } => {
            assert_eq!(wind_round, Wind::East);
            assert_eq!(hand_number, 2);
            assert_eq!(dealer_streak, 0);
            assert_eq!(dealer, next_dealer);
            assert_eq!(hand.closed_tiles.len(), 14);
            assert_eq!(hand.drawn_tile, None);
            assert_eq!(wall_remaining, 83);
            assert_eq!(scores, [100; 4]);
        }
        other => panic!("expected HandStarted for hand 2, got {other:?}"),
    }

    let hand2_view = project_view(&game, next_dealer);
    assert_eq!(hand2_view.phase, PublicPhase::Playing);
    assert_eq!(hand2_view.turn.hand_number, 2);
    assert_eq!(hand2_view.turn.dealer, next_dealer);
    assert_eq!(hand2_view.turn.active_seat, Some(next_dealer));
    assert_eq!(hand2_view.ready_seats, None);
    assert_eq!(hand2_view.last_settlement, None);
    for seat_view in &hand2_view.seats {
        assert_eq!(seat_view.last_draw_or_meld_seq, 0);
        assert!(seat_view.river.is_empty());
    }
}

#[test]
fn slice3_free_table_end_match_and_east_only_fourth_hand_completion() {
    // 1. FreeTable EndMatch during BetweenHands.
    let mut free_game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        SeededWallProvider::new(7),
    )
    .expect("free table match starts");

    play_current_hand_to_settlement(&mut free_game);
    let free_between = project_view(&free_game, Seat::South);
    assert!(free_between.legal_actions.contains(&PlayerAction::EndMatch));

    let end_events = free_game
        .apply_action(Seat::South, PlayerAction::EndMatch)
        .expect("EndMatch succeeds in FreeTable BetweenHands");
    assert_eq!(end_events.len(), 1);
    match project_event(&end_events[0], Seat::East).kind {
        PlayerEventKind::MatchFinished { summary } => {
            assert_eq!(summary.mode, MatchMode::FreeTable);
            assert_eq!(summary.hands_played, 1);
            assert_eq!(summary.initial_scores, [0; 4]);
            assert_eq!(summary.final_scores, [0; 4]);
            assert_eq!(summary.net_deltas, [0; 4]);
        }
        other => panic!("expected MatchFinished, got {other:?}"),
    }

    let finished_view = project_view(&free_game, Seat::East);
    assert_eq!(finished_view.phase, PublicPhase::MatchFinished);
    assert!(finished_view.legal_actions.is_empty());
    assert!(finished_view.match_summary.is_some());
    assert_eq!(
        free_game
            .apply_action(Seat::East, PlayerAction::ConfirmNextHand)
            .unwrap_err(),
        MatchError::MatchAlreadyFinished
    );

    // 2. EastOnly finishes immediately after East 4 DrawGame without entering BetweenHands.
    let mut east_game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        SeededWallProvider::new(99),
    )
    .expect("east only match starts");

    for hand_idx in 1..=4 {
        let events = play_current_hand_to_settlement(&mut east_game);
        if hand_idx < 4 {
            assert_eq!(
                project_view(&east_game, Seat::East).phase,
                PublicPhase::BetweenHands
            );
            for seat in Seat::ALL {
                east_game
                    .apply_action(seat, PlayerAction::ConfirmNextHand)
                    .unwrap();
            }
        } else {
            assert!(events.iter().any(|ev| {
                matches!(
                    project_event(ev, Seat::East).kind,
                    PlayerEventKind::MatchFinished { summary }
                        if summary.mode == MatchMode::EastOnly && summary.hands_played == 4
                )
            }));
            assert_eq!(
                project_view(&east_game, Seat::East).phase,
                PublicPhase::MatchFinished
            );
        }
    }
}

#[test]
fn slice4_projection_non_interference_hides_opponent_secrets() {
    let east_14 = vec![
        t(0),
        t(0),
        t(1),
        t(2),
        t(3),
        t(4),
        t(5),
        t(6),
        t(7),
        t(8),
        t(9),
        t(10),
        t(11),
        t(12),
    ];
    let south_a = vec![t(13); 4]
        .into_iter()
        .chain(vec![t(14); 4])
        .chain(vec![t(15); 4])
        .chain([t(16)])
        .collect::<Vec<_>>();
    let west_a = vec![t(17); 4]
        .into_iter()
        .chain(vec![t(18); 4])
        .chain(vec![t(19); 4])
        .chain([t(20)])
        .collect::<Vec<_>>();
    let north_a = vec![t(21); 4]
        .into_iter()
        .chain(vec![t(22); 4])
        .chain(vec![t(23); 4])
        .chain([t(24)])
        .collect::<Vec<_>>();
    let wall_a = build_preset_wall(&east_14, &south_a, &west_a, &north_a, &[t(31)], &[]).unwrap();

    // Swap South/West/North hands and South's draw tile completely while keeping East identical.
    let south_b = vec![t(25); 4]
        .into_iter()
        .chain(vec![t(26); 4])
        .chain(vec![t(27); 4])
        .chain([t(28)])
        .collect::<Vec<_>>();
    let west_b = vec![t(29); 4]
        .into_iter()
        .chain(vec![t(30); 4])
        .chain(vec![t(31); 4])
        .chain([t(32)])
        .collect::<Vec<_>>();
    let north_b = vec![t(33); 4]
        .into_iter()
        .chain(vec![t(21); 4])
        .chain(vec![t(22); 4])
        .chain([t(23)])
        .collect::<Vec<_>>();
    let wall_b = build_preset_wall(&east_14, &south_b, &west_b, &north_b, &[t(15)], &[]).unwrap();

    let mut match_a = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_a),
    )
    .unwrap();
    let mut match_b = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, wall_b),
    )
    .unwrap();

    assert_eq!(
        project_view(&match_a, Seat::East),
        project_view(&match_b, Seat::East)
    );
    assert_eq!(
        project_event(&match_a.events()[1], Seat::East),
        project_event(&match_b.events()[1], Seat::East)
    );

    let ev_a = match_a
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: false,
            },
        )
        .unwrap();
    let ev_b = match_b
        .apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: false,
            },
        )
        .unwrap();

    // East's view and East's projected events cannot distinguish opponent hands or drawn tile (31 vs 15).
    assert_eq!(
        project_view(&match_a, Seat::East),
        project_view(&match_b, Seat::East)
    );
    assert_eq!(
        project_event(&ev_a[1], Seat::East),
        project_event(&ev_b[1], Seat::East)
    );
    assert_ne!(
        project_event(&ev_a[1], Seat::South),
        project_event(&ev_b[1], Seat::South)
    );
    // Debug formatting on RecordedEvent must not leak drawn tile indices.
    let debug_str = format!("{:?}", ev_a[1]);
    assert!(!debug_str.contains("31"));
}

#[test]
fn slice4_illegal_and_out_of_turn_actions_do_not_mutate_state() {
    let mut game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        PresetWallProvider::single(Seat::East, Tile::standard_deck_136()),
    )
    .unwrap();

    let views_before: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&game, s));
    let seq_before = game.seq();
    let event_count_before = game.events().len();

    // Out-of-turn action.
    assert!(matches!(
        game.apply_action(
            Seat::South,
            PlayerAction::Discard {
                tile: t(0),
                from_drawn: false
            }
        )
        .unwrap_err(),
        MatchError::NotYourTurn { seat: Seat::South }
    ));

    // Illegal tile not in East's closed hand (standard_deck_136 gives East tiles 0..=3).
    assert!(matches!(
        game.apply_action(
            Seat::East,
            PlayerAction::Discard {
                tile: t(33),
                from_drawn: false
            }
        )
        .unwrap_err(),
        MatchError::IllegalAction {
            seat: Seat::East,
            ..
        }
    ));

    // Wrong phase actions during TurnAction.
    for wrong in [
        PlayerAction::ConfirmNextHand,
        PlayerAction::EndMatch,
        PlayerAction::Pass,
        PlayerAction::Chi {
            tile: t(0),
            with: [t(1), t(2)],
        },
    ] {
        assert!(matches!(
            game.apply_action(Seat::East, wrong).unwrap_err(),
            MatchError::WrongPhase {
                seat: Seat::East,
                ..
            }
        ));
    }

    let views_after: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&game, s));
    assert_eq!(views_before, views_after);
    assert_eq!(game.seq(), seq_before);
    assert_eq!(game.events().len(), event_count_before);

    // Also verify InvalidWall on 4th ConfirmNextHand leaves BetweenHands unmutated!
    play_current_hand_to_settlement(&mut game);
    game.apply_action(Seat::East, PlayerAction::ConfirmNextHand)
        .unwrap();
    game.apply_action(Seat::South, PlayerAction::ConfirmNextHand)
        .unwrap();
    game.apply_action(Seat::West, PlayerAction::ConfirmNextHand)
        .unwrap();

    let between_before: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&game, s));
    let seq_between = game.seq();
    let err_next_wall = game
        .apply_action(Seat::North, PlayerAction::ConfirmNextHand)
        .unwrap_err();
    assert!(matches!(err_next_wall, MatchError::InvalidWall(_)));
    let between_after: [PlayerView; 4] = Seat::ALL.map(|s| project_view(&game, s));
    assert_eq!(between_before, between_after);
    assert_eq!(game.seq(), seq_between);
}

#[test]
fn slice4_deterministic_u64_seed_replay_is_identical_across_runs() {
    fn run_match(seed: u64) -> (Vec<[PlayerView; 4]>, Vec<[PlayerEvent; 4]>) {
        let mut game = Match::new(
            MatchMode::EastOnly,
            AnyRuleset::default(),
            SeededWallProvider::new(seed),
        )
        .unwrap();
        let mut views = Vec::new();
        let mut events: Vec<[PlayerEvent; 4]> = game
            .events()
            .iter()
            .map(|ev| Seat::ALL.map(|s| project_event(ev, s)))
            .collect();
        views.push(Seat::ALL.map(|s| project_view(&game, s)));

        loop {
            let probe = project_view(&game, Seat::East);
            match probe.phase {
                PublicPhase::Playing => {
                    let (actor, action) = Seat::ALL
                        .into_iter()
                        .find_map(|s| {
                            project_view(&game, s)
                                .legal_actions
                                .first()
                                .copied()
                                .map(|action| (s, action))
                        })
                        .expect("at least one seat has legal actions during Playing");
                    let emitted = game.apply_action(actor, action).unwrap();
                    for ev in &emitted {
                        events.push(Seat::ALL.map(|s| project_event(ev, s)));
                    }
                    views.push(Seat::ALL.map(|s| project_view(&game, s)));
                }
                PublicPhase::BetweenHands => {
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
                PublicPhase::MatchFinished => break,
            }
        }
        (views, events)
    }

    let (views_1, events_1) = run_match(20_261_121);
    let (views_2, events_2) = run_match(20_261_121);
    let (views_other, _) = run_match(20_261_122);

    assert_eq!(views_1, views_2);
    assert_eq!(events_1, events_2);
    assert_ne!(views_1[0], views_other[0]);
}

#[test]
fn slice4_ts_rs_declarations_and_serde_json_roundtrip() {
    assert_eq!(Tile::decl(), "type Tile = number;");
    assert!(PlayerAction::decl().contains("Tile"));
    assert!(!PlayerView::decl().is_empty());
    assert!(!PlayerEvent::decl().is_empty());
    assert!(!RuleProfile::decl().is_empty());
    assert!(!MatchError::decl().is_empty());

    assert!(serde_json::from_str::<Tile>("34").is_err());
    assert_eq!(serde_json::from_str::<Tile>("33").unwrap(), t(33));
    assert_eq!(serde_json::to_string(&t(5)).unwrap(), "5");

    let game = Match::new(
        MatchMode::EastOnly,
        TuidaohuRuleset,
        SeededWallProvider::new(12345),
    )
    .unwrap();
    let view = project_view(&game, Seat::East);
    let view_json = serde_json::to_string(&view).unwrap();
    let view_back: PlayerView = serde_json::from_str(&view_json).unwrap();
    assert_eq!(view, view_back);

    let ev = project_event(&game.events()[1], Seat::East);
    let ev_json = serde_json::to_string(&ev).unwrap();
    let ev_back: PlayerEvent = serde_json::from_str(&ev_json).unwrap();
    assert_eq!(ev, ev_back);
}

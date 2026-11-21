use mahjong_core::{
    build_preset_wall, project_event, project_view, AnyRuleset, HandOutcome, KongKind,
    KongTransfer, Match, MatchMode, MatchSummary, MeldView, PlayerAction, PlayerEventKind,
    PlayerView, PresetWallProvider, PublicPhase, Ruleset, ScoreItem, Seat, Tile, TuidaohuRuleset,
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

fn assert_conservation_and_zero_sum(view: &PlayerView, expected_total_score: i32) {
    let total_hands: usize = view.seats.iter().map(|s| usize::from(s.hand_count)).sum();
    let total_river: usize = view
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
        total_hands + total_river + total_melds + usize::from(view.wall_remaining),
        136
    );
    let score_sum: i32 = view.seats.iter().map(|s| s.score).sum();
    assert_eq!(score_sum, expected_total_score);
}

fn confirm_all_seats<R: Ruleset, W: mahjong_core::WallProvider>(game: &mut Match<R, W>) {
    for seat in Seat::ALL {
        game.apply_action(seat, PlayerAction::ConfirmNextHand)
            .unwrap();
    }
}

/// Wall where `dealer.next()` discards `m(1)` and BOTH `dealer` (1 fan, 2 pts) and
/// `dealer.prev()` (2 fan, 4 pts) win via `Ron` (`一炮多响` including `dealer`).
fn next_discard_dealer_double_ron_wall() -> Vec<Tile> {
    let dealer_14 = vec![
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
    let seat1_13 = vec![
        m(1),
        m(4),
        m(7),
        m(8),
        s(4),
        s(7),
        p(4),
        p(7),
        WEST_W,
        NORTH_W,
        GREEN_D,
        EAST_W,
        WHITE_D,
    ];
    let seat2_13 = vec![
        m(5),
        m(6),
        m(7),
        m(8),
        s(5),
        s(6),
        s(8),
        p(5),
        p(6),
        p(8),
        WEST_W,
        NORTH_W,
        GREEN_D,
    ];
    let seat3_13 = vec![
        m(2),
        m(3),
        m(4),
        m(4),
        m(4),
        m(9),
        m(9),
        m(9),
        SOUTH_W,
        SOUTH_W,
        SOUTH_W,
        GREEN_D,
        GREEN_D,
    ];
    build_preset_wall(&dealer_14, &seat1_13, &seat2_13, &seat3_13, &[p(9)], &[]).unwrap()
}

/// Wall where `dealer` discards `m(1)` and BOTH `dealer.next()` (1 fan, 2 pts) and
/// `dealer + 2` (2 fan, 4 pts) win via `Ron` (`一炮多响` excluding `dealer`).
fn dealer_discard_non_dealer_double_ron_wall() -> Vec<Tile> {
    let dealer_14 = vec![
        m(1),
        m(4),
        m(7),
        s(4),
        s(7),
        p(4),
        p(7),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        RED_D,
        GREEN_D,
        WHITE_D,
    ];
    let seat1_13 = vec![
        m(2),
        m(3),
        s(1),
        s(2),
        s(3),
        p(1),
        p(2),
        p(3),
        p(8),
        p(8),
        p(8),
        RED_D,
        RED_D,
    ];
    let seat2_13 = vec![
        m(2),
        m(3),
        m(5),
        m(5),
        m(5),
        m(9),
        m(9),
        m(9),
        WEST_W,
        WEST_W,
        WEST_W,
        WHITE_D,
        WHITE_D,
    ];
    let seat3_13 = vec![
        m(6),
        m(8),
        s(6),
        s(8),
        s(9),
        p(5),
        p(6),
        p(9),
        NORTH_W,
        NORTH_W,
        GREEN_D,
        GREEN_D,
        EAST_W,
    ];
    build_preset_wall(&dealer_14, &seat1_13, &seat2_13, &seat3_13, &[], &[]).unwrap()
}

fn trigger_next_discard_dealer_double_ron<R: Ruleset, W: mahjong_core::WallProvider>(
    game: &mut Match<R, W>,
    dealer: Seat,
) -> mahjong_core::HandSettlement {
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        dealer.next(),
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    assert!(game
        .apply_action(dealer.prev(), PlayerAction::Ron { tile: m(1) })
        .unwrap()
        .is_empty());
    let settled_ev = game
        .apply_action(dealer, PlayerAction::Ron { tile: m(1) })
        .unwrap();
    assert_eq!(settled_ev.len(), 1);
    match project_event(&settled_ev[0], dealer).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    }
}

fn trigger_dealer_discard_non_dealer_double_ron<R: Ruleset, W: mahjong_core::WallProvider>(
    game: &mut Match<R, W>,
    dealer: Seat,
) -> Vec<mahjong_core::RecordedEvent> {
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    assert!(game
        .apply_action(dealer.next().next(), PlayerAction::Ron { tile: m(1) })
        .unwrap()
        .is_empty());
    game.apply_action(dealer.next(), PlayerAction::Ron { tile: m(1) })
        .unwrap()
}

#[test]
fn multi_ron_with_and_without_dealer_controls_streak_rotation_and_match_end() {
    // EastOnly match exercising:
    // H1 (East 1, East): multi-ron INCLUDING East -> 连庄 (East 1, streak 1, East)
    // H2 (East 1 #1, East): multi-ron EXCLUDING East -> rotates (East 2, streak 0, South)
    // H3 (East 2, South): multi-ron EXCLUDING South -> rotates (East 3, streak 0, West)
    // H4 (East 3, West): multi-ron EXCLUDING West -> rotates (East 4, streak 0, North)
    // H5 (East 4, North): multi-ron INCLUDING North -> 连庄 at East 4 (East 4, streak 1, North)
    // H6 (East 4 #1, North): multi-ron EXCLUDING North -> immediate MatchFinished!
    let walls = vec![
        next_discard_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        next_discard_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
    ];
    let mut east_game = Match::new(
        MatchMode::EastOnly,
        AnyRuleset::default(),
        PresetWallProvider::new(Seat::East, walls),
    )
    .unwrap();

    // Hand 1: East (dealer) + North Ron on South's m(1).
    let s1 = trigger_next_discard_dealer_double_ron(&mut east_game, Seat::East);
    assert_eq!(s1.score_deltas, [2, -6, 0, 4]);
    assert_eq!(s1.new_scores, [102, 94, 100, 104]);
    assert_conservation_and_zero_sum(&project_view(&east_game, Seat::East), 400);
    confirm_all_seats(&mut east_game);

    let v2 = project_view(&east_game, Seat::East);
    assert_eq!(v2.turn.wind_round, Wind::East);
    assert_eq!(v2.turn.hand_number, 1);
    assert_eq!(v2.turn.dealer_streak, 1);
    assert_eq!(v2.turn.dealer, Seat::East);

    // Hand 2: South + West Ron on East's m(1) (excludes dealer East) -> rotates to South, East 2.
    let ev2 = trigger_dealer_discard_non_dealer_double_ron(&mut east_game, Seat::East);
    assert_eq!(ev2.len(), 1);
    confirm_all_seats(&mut east_game);
    let v3 = project_view(&east_game, Seat::South);
    assert_eq!(v3.turn.wind_round, Wind::East);
    assert_eq!(v3.turn.hand_number, 2);
    assert_eq!(v3.turn.dealer_streak, 0);
    assert_eq!(v3.turn.dealer, Seat::South);
    assert_eq!(v3.seats.map(|s| s.score), [96, 96, 104, 104]);

    // Hand 3 (East 2, South): West + North Ron on South's m(1) -> rotates to West, East 3.
    trigger_dealer_discard_non_dealer_double_ron(&mut east_game, Seat::South);
    confirm_all_seats(&mut east_game);
    let v4 = project_view(&east_game, Seat::West);
    assert_eq!(v4.turn.hand_number, 3);
    assert_eq!(v4.turn.dealer_streak, 0);
    assert_eq!(v4.turn.dealer, Seat::West);
    assert_eq!(v4.seats.map(|s| s.score), [96, 90, 106, 108]);

    // Hand 4 (East 3, West): North + East Ron on West's m(1) -> rotates to North, East 4.
    trigger_dealer_discard_non_dealer_double_ron(&mut east_game, Seat::West);
    confirm_all_seats(&mut east_game);
    let v5 = project_view(&east_game, Seat::North);
    assert_eq!(v5.turn.hand_number, 4);
    assert_eq!(v5.turn.dealer_streak, 0);
    assert_eq!(v5.turn.dealer, Seat::North);
    assert_eq!(v5.seats.map(|s| s.score), [100, 90, 100, 110]);

    // Hand 5 (East 4, North): North (dealer) + West Ron on East's m(1) -> MUST stay in BetweenHands & 连庄!
    let s5 = trigger_next_discard_dealer_double_ron(&mut east_game, Seat::North);
    assert_eq!(s5.score_deltas, [-6, 0, 4, 2]);
    assert_eq!(s5.new_scores, [94, 90, 104, 112]);
    assert_eq!(
        project_view(&east_game, Seat::North).phase,
        PublicPhase::BetweenHands
    );
    confirm_all_seats(&mut east_game);

    let v6 = project_view(&east_game, Seat::North);
    assert_eq!(v6.turn.wind_round, Wind::East);
    assert_eq!(v6.turn.hand_number, 4);
    assert_eq!(v6.turn.dealer_streak, 1);
    assert_eq!(v6.turn.dealer, Seat::North);

    // Hand 6 (East 4 #1, North): East + South Ron on North's m(1) (excludes dealer North) -> MatchFinished!
    let ev6 = trigger_dealer_discard_non_dealer_double_ron(&mut east_game, Seat::North);
    assert_eq!(ev6.len(), 2);
    assert!(matches!(
        project_event(&ev6[0], Seat::East).kind,
        PlayerEventKind::HandSettled { .. }
    ));
    let summary = match project_event(&ev6[1], Seat::East).kind {
        PlayerEventKind::MatchFinished { summary } => summary,
        other => panic!("expected MatchFinished, got {other:?}"),
    };
    assert_eq!(summary.hands_played, 6);
    assert_eq!(summary.final_scores, [96, 94, 104, 106]);
    assert_eq!(summary.final_scores.iter().sum::<i32>(), 400);
    assert_eq!(summary.net_deltas, [-4, -6, 4, 6]);
    assert_eq!(summary.net_deltas.iter().sum::<i32>(), 0);

    // Also verify in FreeTable that East 4 multi-ron excluding dealer rotates to South 1 / East!
    let free_walls = vec![
        dealer_discard_non_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        dealer_discard_non_dealer_double_ron_wall(),
        next_discard_dealer_double_ron_wall(),
    ];
    let mut free_game = Match::new(
        MatchMode::FreeTable,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, free_walls),
    )
    .unwrap();
    let expected_scores_after = [[-6, 2, 4, 0], [-6, -4, 6, 4], [-2, -4, 0, 6], [0, 0, 0, 0]];
    for (idx, dealer) in Seat::ALL.into_iter().enumerate() {
        trigger_dealer_discard_non_dealer_double_ron(&mut free_game, dealer);
        let settled_view = project_view(&free_game, dealer);
        assert_conservation_and_zero_sum(&settled_view, 0);
        assert_eq!(
            settled_view.seats.map(|s| s.score),
            expected_scores_after[idx]
        );
        confirm_all_seats(&mut free_game);
    }
    let south1 = project_view(&free_game, Seat::East);
    assert_eq!(south1.turn.wind_round, Wind::South);
    assert_eq!(south1.turn.hand_number, 1);
    assert_eq!(south1.turn.dealer, Seat::East);
    assert_eq!(south1.turn.dealer_streak, 0);
    assert_conservation_and_zero_sum(&south1, 0);

    // Hand 5 (South 1, dealer East): East + North Ron on South's m(1) -> [2, -6, 0, 4], then EndMatch.
    let s_south1 = trigger_next_discard_dealer_double_ron(&mut free_game, Seat::East);
    assert_eq!(s_south1.new_scores, [2, -6, 0, 4]);
    free_game
        .apply_action(Seat::North, PlayerAction::EndMatch)
        .unwrap();
    let free_summary = project_view(&free_game, Seat::East).match_summary.unwrap();
    assert_eq!(free_summary.hands_played, 5);
    assert_eq!(free_summary.final_scores, [2, -6, 0, 4]);
    assert_eq!(free_summary.net_deltas.iter().sum::<i32>(), 0);
}

/// Hand wall combining ConcealedKong (East: m9), ExposedKong (West from South: s9),
/// AddedKong (East after Pon from North: p9), ending in DrawGame (`荒庄不荒杠`).
fn multi_kong_draw_game_wall() -> Vec<Tile> {
    let dealer_14 = vec![
        m(9),
        m(9),
        m(9),
        m(9),
        p(9),
        p(9),
        m(2),
        m(5),
        s(2),
        s(5),
        p(2),
        p(5),
        EAST_W,
        WHITE_D,
    ];
    let seat1_13 = vec![
        m(1),
        m(4),
        m(7),
        s(1),
        s(4),
        s(7),
        p(1),
        p(4),
        p(7),
        SOUTH_W,
        WEST_W,
        RED_D,
        GREEN_D,
    ];
    let seat2_13 = vec![
        s(9),
        s(9),
        s(9),
        m(3),
        m(6),
        s(3),
        s(6),
        p(3),
        p(6),
        EAST_W,
        SOUTH_W,
        WEST_W,
        RED_D,
    ];
    let seat3_13 = vec![
        m(1),
        m(4),
        m(7),
        s(1),
        s(4),
        s(7),
        p(1),
        p(4),
        p(7),
        WEST_W,
        NORTH_W,
        RED_D,
        WHITE_D,
    ];
    build_preset_wall(
        &dealer_14,
        &seat1_13,
        &seat2_13,
        &seat3_13,
        &[s(9), p(9), m(2), m(5), s(2), p(9)],
        &[NORTH_W, GREEN_D, SOUTH_W],
    )
    .unwrap()
}

/// Plays `multi_kong_draw_game_wall()` relative to `dealer`:
/// - `dealer` ConcealedKong { m(9) } (+6, -2, -2, -2)
/// - `dealer.next()` discards `s(9)` -> `dealer+2` ExposedKong { s(9) } (+2 to `dealer+2`, -2 from `dealer.next()`)
/// - `dealer.prev()` discards `p(9)` -> `dealer` Pon { p(9) } -> later draws `p(9)` -> AddedKong { p(9) } (+3, -1, -1, -1)
/// - Drains wall to `DrawGame` (`荒庄不荒杠`).
fn play_multi_kong_draw_hand<R: Ruleset, W: mahjong_core::WallProvider>(
    game: &mut Match<R, W>,
    dealer: Seat,
    expected_total: i32,
) {
    let s1 = dealer.next();
    let s2 = s1.next();
    let s3 = s2.next();

    game.apply_action(dealer, PlayerAction::ConcealedKong { tile: m(9) })
        .unwrap();
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();

    // s1 draws head[0]=s(9) and discards it -> s2 claims ExposedKong { s(9) }!
    game.apply_action(
        s1,
        PlayerAction::Discard {
            tile: s(9),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(s2, PlayerAction::ExposedKong { tile: s(9) })
        .unwrap();
    game.apply_action(
        s2,
        PlayerAction::Discard {
            tile: GREEN_D,
            from_drawn: true,
        },
    )
    .unwrap();

    // s3 draws head[1]=p(9) and discards it -> dealer claims Pon { p(9) }, discards NORTH_W.
    game.apply_action(
        s3,
        PlayerAction::Discard {
            tile: p(9),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(dealer, PlayerAction::Pon { tile: p(9) })
        .unwrap();
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: NORTH_W,
            from_drawn: false,
        },
    )
    .unwrap();

    // s1, s2, s3 tsumogiri head[2..=4]; dealer draws head[5]=p(9) and completes AddedKong { p(9) }!
    for seat in [s1, s2, s3] {
        let drawn = project_view(game, seat).hand.drawn_tile.unwrap();
        game.apply_action(
            seat,
            PlayerAction::Discard {
                tile: drawn,
                from_drawn: true,
            },
        )
        .unwrap();
    }
    game.apply_action(dealer, PlayerAction::AddedKong { tile: p(9) })
        .unwrap();
    let rep = project_view(game, dealer).hand.drawn_tile.unwrap();
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: rep,
            from_drawn: true,
        },
    )
    .unwrap();

    // Drain remaining wall via Pass + tsumogiri until DrawGame settles.
    loop {
        let probe = project_view(game, dealer);
        assert_conservation_and_zero_sum(&probe, expected_total);
        if probe.phase != PublicPhase::Playing {
            break;
        }
        let (seat, view) = Seat::ALL
            .into_iter()
            .map(|s| (s, project_view(game, s)))
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
                .unwrap()
        };
        game.apply_action(seat, action).unwrap();
    }
}

/// Hand 2 wall (`dealer = South`):
/// - `East` (`dealer + 3`) declares `ConcealedKong { m(9) }` (+6, -2, -2, -2)
/// - `West` (`dealer + 1`) Pons `m(1)` from `North`, later attempts `AddedKong { m(1) }`
/// - `South` (`dealer`) robs it via `RobKongHu { m(1) }` (2 fan = 4 pts from `West`, AddedKong voided!) -> 连庄!
fn concealed_kong_plus_voided_added_kong_rob_wall() -> Vec<Tile> {
    let dealer_14 = vec![
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
        RED_D,
        NORTH_W,
        GREEN_D,
    ];
    let seat1_13 = vec![
        m(1),
        m(1),
        m(5),
        m(7),
        s(4),
        s(6),
        s(8),
        p(7),
        p(8),
        p(9),
        SOUTH_W,
        WEST_W,
        NORTH_W,
    ];
    let seat2_13 = vec![
        m(1),
        m(4),
        m(6),
        m(8),
        s(5),
        s(7),
        s(9),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
        WHITE_D,
        GREEN_D,
    ];
    let seat3_13 = vec![
        m(9),
        m(9),
        m(9),
        m(9),
        m(4),
        m(6),
        s(4),
        s(6),
        s(8),
        EAST_W,
        RED_D,
        WHITE_D,
        NORTH_W,
    ];
    build_preset_wall(
        &dealer_14,
        &seat1_13,
        &seat2_13,
        &seat3_13,
        &[p(1), s(5), m(6), m(7), RED_D, m(1)],
        &[m(8)],
    )
    .unwrap()
}

fn play_concealed_kong_and_voided_rob_kong_hand<R: Ruleset, W: mahjong_core::WallProvider>(
    game: &mut Match<R, W>,
    dealer: Seat,
) -> mahjong_core::HandSettlement {
    let s1 = dealer.next();
    let s2 = s1.next();
    let s3 = s2.next();

    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: GREEN_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        s1,
        PlayerAction::Discard {
            tile: p(1),
            from_drawn: true,
        },
    )
    .unwrap();
    game.apply_action(
        s2,
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(s1, PlayerAction::Pon { tile: m(1) })
        .unwrap();
    game.apply_action(
        s1,
        PlayerAction::Discard {
            tile: SOUTH_W,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(
        s2,
        PlayerAction::Discard {
            tile: m(6),
            from_drawn: true,
        },
    )
    .unwrap();

    // s3 declares ConcealedKong { m(9) } (+6 to s3, -2 each others), draws tail[0]=m(8) & discards it.
    game.apply_action(s3, PlayerAction::ConcealedKong { tile: m(9) })
        .unwrap();
    game.apply_action(
        s3,
        PlayerAction::Discard {
            tile: m(8),
            from_drawn: true,
        },
    )
    .unwrap();

    // dealer discards NORTH_W (keeping drawn RED_D -> now tenpai for m(1)!).
    game.apply_action(
        dealer,
        PlayerAction::Discard {
            tile: NORTH_W,
            from_drawn: false,
        },
    )
    .unwrap();

    // s1 draws m(1) and attempts AddedKong { m(1) }, which dealer robs!
    game.apply_action(s1, PlayerAction::AddedKong { tile: m(1) })
        .unwrap();
    let rob_ev = game
        .apply_action(dealer, PlayerAction::RobKongHu { tile: m(1) })
        .unwrap();
    assert_eq!(rob_ev.len(), 1);
    match project_event(&rob_ev[0], dealer).kind {
        PlayerEventKind::HandSettled { settlement } => settlement,
        other => panic!("expected HandSettled, got {other:?}"),
    }
}

/// Hand wall where `dealer.next()` draws `RED_D` after `ExposedKong` or direct `Tsumo`:
/// - `dealer` discards `m(1)`.
/// - `dealer.next()` claims `ExposedKong { m(1) }`, replacement-draws `RED_D`, and `Tsumo`s (`杠上开花` = 1 fan = 2 pts).
fn exposed_kong_replacement_tsumo_wall() -> Vec<Tile> {
    let dealer_14 = vec![
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
    let seat1_13 = vec![
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
    let seat2_13 = vec![
        m(4),
        m(6),
        m(8),
        s(2),
        s(4),
        s(8),
        p(2),
        p(4),
        p(8),
        EAST_W,
        SOUTH_W,
        WEST_W,
        NORTH_W,
    ];
    let seat3_13 = vec![
        m(5),
        m(7),
        m(9),
        s(3),
        s(4),
        s(8),
        p(3),
        p(4),
        p(9),
        GREEN_D,
        GREEN_D,
        WHITE_D,
        NORTH_W,
    ];
    build_preset_wall(&dealer_14, &seat1_13, &seat2_13, &seat3_13, &[], &[RED_D]).unwrap()
}

/// Quick 1-fan `门前清` Tsumo for `dealer.next()` after `dealer` discards `WHITE_D`.
fn quick_next_seat_tsumo_wall() -> Vec<Tile> {
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

fn run_five_hand_kong_accounting_sequence(mode: MatchMode, expected_total: i32) -> MatchSummary {
    let mut walls = vec![
        multi_kong_draw_game_wall(), // H1 (East 1, East): DrawGame w/ 3 kongs -> [9, -5, -1, -3]
        concealed_kong_plus_voided_added_kong_rob_wall(), // H2 (East 2, South): ConcealedKong + voided AddedKong RobKongHu -> [6, 2, -6, -2], 连庄
        exposed_kong_replacement_tsumo_wall(), // H3 (East 2 #1, South): ExposedKong + Replacement Tsumo -> [-2, -4, 8, -2]
        quick_next_seat_tsumo_wall(), // H4 (East 3, West): North 1-fan Tsumo -> [-2, -2, -2, 6]
        quick_next_seat_tsumo_wall(), // H5 (East 4, North): East 1-fan Tsumo -> [6, -2, -2, -2]
    ];
    if mode == MatchMode::FreeTable {
        walls.push(multi_kong_draw_game_wall()); // H6 (South 1, East): DrawGame w/ 3 kongs -> [9, -5, -1, -3]
    }

    let mut game = Match::new(
        mode,
        TuidaohuRuleset,
        PresetWallProvider::new(Seat::East, walls),
    )
    .unwrap();
    let base = if mode == MatchMode::EastOnly { 100 } else { 0 };

    // Hand 1: DrawGame with ConcealedKong (+6,-2,-2,-2), ExposedKong (0,-2,+2,0), AddedKong (+3,-1,-1,-1) = [+9,-5,-1,-3].
    play_multi_kong_draw_hand(&mut game, Seat::East, expected_total);
    let v_after_1 = project_view(&game, Seat::East);
    assert_conservation_and_zero_sum(&v_after_1, expected_total);
    let s1 = v_after_1.last_settlement.unwrap();
    assert_eq!(s1.outcome, HandOutcome::DrawGame);
    assert_eq!(s1.kong_transfers.len(), 3);
    assert_eq!(s1.score_deltas, [9, -5, -1, -3]);
    assert_eq!(s1.new_scores, [base + 9, base - 5, base - 1, base - 3]);
    confirm_all_seats(&mut game);

    // Hand 2 (East 2, dealer South): ConcealedKong by East (+6,-2,-2,-2) + voided AddedKong by West robbed by South (+4,-4) = [+6, +2, -6, -2].
    let v2_start = project_view(&game, Seat::South);
    assert_eq!(v2_start.turn.hand_number, 2);
    assert_eq!(v2_start.turn.dealer, Seat::South);
    let s2 = play_concealed_kong_and_voided_rob_kong_hand(&mut game, Seat::South);
    assert_eq!(
        s2.kong_transfers,
        vec![KongTransfer {
            kind: KongKind::ConcealedKong,
            actor: Seat::East,
            from_seat: None,
            tile: m(9),
            score_deltas: [6, -2, -2, -2],
        }]
    );
    match &s2.outcome {
        HandOutcome::Win {
            winners, discarder, ..
        } => {
            assert_eq!(*discarder, Some(Seat::West));
            assert_eq!(winners.len(), 1);
            assert_eq!(winners[0].seat, Seat::South);
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
    assert_eq!(s2.score_deltas, [6, 2, -6, -2]);
    assert_eq!(s2.new_scores, [base + 15, base - 3, base - 7, base - 5]);
    assert_conservation_and_zero_sum(&project_view(&game, Seat::South), expected_total);
    confirm_all_seats(&mut game);

    // Hand 3 (East 2 streak 1, dealer South): West claims ExposedKong on South (+2,-2) + replacement Tsumo (-2,-2,+6,-2) = [-2, -4, +8, -2].
    let v3_start = project_view(&game, Seat::South);
    assert_eq!(v3_start.turn.hand_number, 2);
    assert_eq!(v3_start.turn.dealer_streak, 1);
    assert_eq!(v3_start.turn.dealer, Seat::South);
    game.apply_action(
        Seat::South,
        PlayerAction::Discard {
            tile: m(1),
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::West, PlayerAction::ExposedKong { tile: m(1) })
        .unwrap();
    game.apply_action(Seat::West, PlayerAction::Tsumo).unwrap();
    let v_after_3 = project_view(&game, Seat::West);
    assert_conservation_and_zero_sum(&v_after_3, expected_total);
    let s3 = v_after_3.last_settlement.unwrap();
    assert_eq!(s3.score_deltas, [-2, -4, 8, -2]);
    assert_eq!(s3.new_scores, [base + 13, base - 7, base + 1, base - 7]);
    confirm_all_seats(&mut game);

    // Hand 4 (East 3, dealer West): North 1-fan Tsumo ([-2, -2, -2, +6]).
    let v4_start = project_view(&game, Seat::West);
    assert_eq!(v4_start.turn.hand_number, 3);
    assert_eq!(v4_start.turn.dealer, Seat::West);
    game.apply_action(
        Seat::West,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::North, PlayerAction::Tsumo).unwrap();
    assert_conservation_and_zero_sum(&project_view(&game, Seat::North), expected_total);
    confirm_all_seats(&mut game);

    // Hand 5 (East 4, dealer North): East 1-fan Tsumo ([+6, -2, -2, -2]).
    let v5_start = project_view(&game, Seat::North);
    assert_eq!(v5_start.turn.hand_number, 4);
    assert_eq!(v5_start.turn.dealer, Seat::North);
    game.apply_action(
        Seat::North,
        PlayerAction::Discard {
            tile: WHITE_D,
            from_drawn: false,
        },
    )
    .unwrap();
    game.apply_action(Seat::East, PlayerAction::Tsumo).unwrap();

    if mode == MatchMode::EastOnly {
        let finished = project_view(&game, Seat::East);
        assert_eq!(finished.phase, PublicPhase::MatchFinished);
        assert_conservation_and_zero_sum(&finished, expected_total);
        return finished.match_summary.unwrap();
    }

    // FreeTable: confirm all 4 -> South 1, play Hand 6 DrawGame w/ 3 kongs -> EndMatch!
    confirm_all_seats(&mut game);
    let s1_start = project_view(&game, Seat::East);
    assert_eq!(s1_start.turn.wind_round, Wind::South);
    assert_eq!(s1_start.turn.hand_number, 1);
    assert_eq!(s1_start.turn.dealer, Seat::East);
    play_multi_kong_draw_hand(&mut game, Seat::East, expected_total);
    let end_ev = game
        .apply_action(Seat::West, PlayerAction::EndMatch)
        .unwrap();
    assert_eq!(end_ev.len(), 1);
    let finished = project_view(&game, Seat::East);
    assert_eq!(finished.phase, PublicPhase::MatchFinished);
    assert_conservation_and_zero_sum(&finished, expected_total);
    finished.match_summary.unwrap()
}

#[test]
fn multi_hand_cumulative_kongs_voided_rob_kong_and_draw_game_zero_sum_across_modes() {
    let east_summary = run_five_hand_kong_accounting_sequence(MatchMode::EastOnly, 400);
    assert_eq!(east_summary.mode, MatchMode::EastOnly);
    assert_eq!(east_summary.hands_played, 5);
    assert_eq!(east_summary.initial_scores, [100, 100, 100, 100]);
    assert_eq!(east_summary.final_scores, [117, 89, 97, 97]);
    assert_eq!(east_summary.net_deltas, [17, -11, -3, -3]);
    assert_eq!(east_summary.final_scores.iter().sum::<i32>(), 400);
    assert_eq!(east_summary.net_deltas.iter().sum::<i32>(), 0);

    let free_summary = run_five_hand_kong_accounting_sequence(MatchMode::FreeTable, 0);
    assert_eq!(free_summary.mode, MatchMode::FreeTable);
    assert_eq!(free_summary.hands_played, 6);
    assert_eq!(free_summary.initial_scores, [0, 0, 0, 0]);
    assert_eq!(free_summary.final_scores, [26, -16, -4, -6]);
    assert_eq!(free_summary.net_deltas, [26, -16, -4, -6]);
    assert_eq!(free_summary.final_scores.iter().sum::<i32>(), 0);
    assert_eq!(free_summary.net_deltas.iter().sum::<i32>(), 0);
}

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    connect_and_hello, connect_ws, fetch_health, recv_msg, recv_snapshot, send_msg,
    spawn_seeded_server, spawn_with_wall_factory, tile, tiles, WsClient,
};
use mahjong_ai::{AiDifficulty, TileEfficiencyBot};
use mahjong_core::{
    build_preset_wall, HandOutcome, MatchMode, MeldView, PlayerAction, PlayerEvent,
    PlayerEventKind, PresetWallProvider, PublicPhase, RevealedMeld, Seat, WinKind,
};
use mahjong_server::protocol::{
    ClientMessage, DestructiveOpKind, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
    TableSnapshot,
};
use mahjong_server::session::{BoxedWallProvider, WallProviderFactory};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

async fn assert_no_pending_messages_via_ping(ws: &mut WsClient, ping_ts: u64) {
    send_msg(ws, &ClientMessage::ClientPing { ts_ms: ping_ts }).await;
    assert_eq!(
        recv_msg(ws).await,
        ServerMessage::ServerPong { ts_ms: ping_ts }
    );
}

async fn recv_event_batch(ws: &mut WsClient) -> (u64, u64, Vec<PlayerEvent>, Vec<PlayerAction>) {
    match recv_msg(ws).await {
        ServerMessage::EventBatch {
            from_seq,
            to_seq,
            events,
            updated_legal_actions,
        } => (from_seq, to_seq, events, updated_legal_actions),
        other => panic!("expected EventBatch, got {other:?}"),
    }
}

#[test]
fn tile_parser_supports_all_34_tiles_including_z_honors() {
    assert_eq!(tiles("1234567z"), tiles("E S W N C F B"));
    assert_eq!(tile("7z"), tile("B"));
}

#[tokio::test]
async fn start_match_validates_human_seat_applies_seating_rules_fills_distinct_ai_styles_and_projects_views(
) {
    let server = spawn_seeded_server(101, None).await;
    let addr = server.addr();

    let mut observer = connect_and_hello(addr, "obs", "Observer").await;

    // Unseated observer cannot StartMatch.
    send_msg(&mut observer, &ClientMessage::StartMatch).await;
    assert!(matches!(
        recv_msg(&mut observer).await,
        ServerMessage::ErrorNotice { .. }
    ));

    let mut p1 = connect_and_hello(addr, "p1", "Alice").await;
    let mut p2 = connect_and_hello(addr, "p2", "Bob").await;

    // P1 joins North, P2 joins South, then test FixedOpposite -> East & West.
    send_msg(
        &mut p1,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::North),
        },
    )
    .await;
    let _ = recv_snapshot(&mut p1).await;

    send_msg(
        &mut p2,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let _ = recv_snapshot(&mut p2).await;

    send_msg(
        &mut p1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Fast,
        },
    )
    .await;

    send_msg(&mut p1, &ClientMessage::StartMatch).await;

    let snap_p1 = recv_snapshot(&mut p1).await;
    let snap_p2 = recv_snapshot(&mut p2).await;
    let snap_obs = recv_snapshot(&mut observer).await;

    assert_eq!(snap_p1.phase, TablePhase::InMatch);
    assert_eq!(snap_p2.your_seat, Some(Seat::East));
    assert_eq!(snap_p1.your_seat, Some(Seat::West));
    assert_eq!(snap_obs.your_seat, None);
    assert!(snap_obs.player_view.is_none());

    let health = fetch_health(addr).await;
    assert_eq!(health.phase, TablePhase::InMatch);
    assert_eq!(health.online_humans, 2);

    // Remaining seats (South, North) must be Ai with distinct AiStyles and Intermediate difficulty.
    let mut ai_styles = HashSet::new();
    for seat in [Seat::South, Seat::North] {
        let info = &snap_p1.seats[seat.index()];
        assert_eq!(info.kind, SeatKind::Ai);
        assert!(info.connected);
        assert_eq!(info.ai_difficulty, Some(AiDifficulty::Intermediate));
        let style = info.ai_style.expect("AI seat must have AiStyle");
        assert!(ai_styles.insert(style), "AI styles should be distinct");
    }

    let view1 = snap_p1.player_view.expect("seated player gets PlayerView");
    let view2 = snap_p2.player_view.expect("seated player gets PlayerView");
    assert_eq!(view1.viewer, Seat::West);
    assert_eq!(view2.viewer, Seat::East);
    assert_eq!(view1.phase, PublicPhase::Playing);
    assert_eq!(view1.seq, snap_p1.seq);
    assert!(snap_p1.seq >= 2);
    assert_ne!(view1.hand.closed_tiles, view2.hand.closed_tiles);

    // Unseated observer gets TableSnapshot on RequestSnapshot with player_view == None.
    send_msg(&mut observer, &ClientMessage::RequestSnapshot).await;
    let snap_obs_req = recv_snapshot(&mut observer).await;
    assert_eq!(snap_obs_req.phase, TablePhase::InMatch);
    assert!(snap_obs_req.player_view.is_none());

    let _ = observer.close(None).await;
    let _ = p1.close(None).await;
    let _ = p2.close(None).await;
    server.shutdown().await;

    // Verify FixedAdjacent places 2 humans at East and South, and Random 1P+3AI fills all 3 distinct AiStyles.
    let server_adj = spawn_seeded_server(202, None).await;
    let addr_adj = server_adj.addr();
    let mut a1 = connect_and_hello(addr_adj, "a1", "A1").await;
    let mut a2 = connect_and_hello(addr_adj, "a2", "A2").await;
    send_msg(
        &mut a1,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::West),
        },
    )
    .await;
    let _ = recv_snapshot(&mut a1).await;
    send_msg(
        &mut a2,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::North),
        },
    )
    .await;
    let _ = recv_snapshot(&mut a2).await;
    send_msg(
        &mut a1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut a1, &ClientMessage::StartMatch).await;
    let snap_a1 = recv_snapshot(&mut a1).await;
    let snap_a2 = recv_snapshot(&mut a2).await;
    assert_eq!(snap_a1.your_seat, Some(Seat::East));
    assert_eq!(snap_a2.your_seat, Some(Seat::South));
    let _ = a1.close(None).await;
    let _ = a2.close(None).await;
    server_adj.shutdown().await;

    let server_rand = spawn_seeded_server(303, None).await;
    let addr_rand = server_rand.addr();
    let mut r1 = connect_and_hello(addr_rand, "r1", "Solo").await;
    send_msg(&mut r1, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut r1).await;
    send_msg(&mut r1, &ClientMessage::StartMatch).await;
    let snap_r1 = recv_snapshot(&mut r1).await;
    let solo_seat = snap_r1.your_seat.expect("solo human must be seated");
    let mut solo_ai_styles = HashSet::new();
    for seat in Seat::ALL {
        if seat == solo_seat {
            assert_eq!(snap_r1.seats[seat.index()].kind, SeatKind::Human);
        } else {
            assert_eq!(snap_r1.seats[seat.index()].kind, SeatKind::Ai);
            solo_ai_styles.insert(snap_r1.seats[seat.index()].ai_style.unwrap());
        }
    }
    assert_eq!(solo_ai_styles.len(), 3);
    let _ = r1.close(None).await;
    server_rand.shutdown().await;
}

#[tokio::test]
async fn submit_action_enforces_seq_idempotency_zero_leakage_and_empty_partial_reaction_batch() {
    let wall = build_preset_wall(
        &tiles("11115m 234678s 234p"),
        &tiles("55m 111p 234678m EE"),
        &tiles("46m 345678s 789p FF"),
        &tiles("999m 999s 555p CCBB"),
        &tiles("1s"),
        &tiles("9p"),
    )
    .expect("valid preset wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server = spawn_with_wall_factory(42, factory).await;
    let addr = server.addr();

    let mut obs = connect_and_hello(addr, "obs", "Observer").await;
    let mut p_east = connect_and_hello(addr, "p1", "East").await;
    let mut p_south = connect_and_hello(addr, "p2", "South").await;
    let mut p_west = connect_and_hello(addr, "p3", "West").await;
    let mut p_north = connect_and_hello(addr, "p4", "North").await;

    for (ws, seat) in [
        (&mut p_east, Seat::East),
        (&mut p_south, Seat::South),
        (&mut p_west, Seat::West),
        (&mut p_north, Seat::North),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }

    send_msg(
        &mut p_east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut p_east, &ClientMessage::StartMatch).await;

    let snap_e = recv_snapshot(&mut p_east).await;
    let snap_s = recv_snapshot(&mut p_south).await;
    let snap_w = recv_snapshot(&mut p_west).await;
    let snap_n = recv_snapshot(&mut p_north).await;
    let _snap_obs = recv_snapshot(&mut obs).await;
    assert_eq!(snap_e.seq, 2);
    assert_eq!(snap_s.your_seat, Some(Seat::South));
    assert_eq!(snap_w.your_seat, Some(Seat::West));
    assert_eq!(snap_n.your_seat, Some(Seat::North));

    // 1. East declares ConcealedKong(1m) -> zero leakage in EventBatch.
    send_msg(
        &mut p_east,
        &ClientMessage::SubmitAction {
            expected_seq: 2,
            client_action_id: "kong-1".to_string(),
            action: PlayerAction::ConcealedKong { tile: tile("1m") },
        },
    )
    .await;

    let (from_e, to_e, ev_e, legal_e) = recv_event_batch(&mut p_east).await;
    assert_eq!((from_e, to_e), (3, 4));
    assert!(matches!(
        ev_e[0].kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::East,
            meld: MeldView::ConcealedKong { tile: Some(t), .. },
            ..
        } if t == tile("1m")
    ));
    assert!(matches!(
        ev_e[1].kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::East,
            tile: Some(t),
            is_replacement: true,
            ..
        } if t == tile("9p")
    ));
    assert!(!legal_e.is_empty());

    for opponent in [&mut p_south, &mut p_west, &mut p_north] {
        let (from_o, to_o, ev_o, legal_o) = recv_event_batch(opponent).await;
        assert_eq!((from_o, to_o), (3, 4));
        assert!(matches!(
            ev_o[0].kind,
            PlayerEventKind::MeldDeclared {
                seat: Seat::East,
                meld: MeldView::ConcealedKong { tile: None, .. },
                ..
            }
        ));
        assert!(matches!(
            ev_o[1].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::East,
                tile: None,
                is_replacement: true,
                ..
            }
        ));
        assert!(legal_o.is_empty());
    }
    // Unseated observer never receives gameplay EventBatch.
    assert_no_pending_messages_via_ping(&mut obs, 111).await;

    // 2. Idempotent duplicate retry of ("kong-1") -> ignored silently without ActionRejected or state mutation.
    send_msg(
        &mut p_east,
        &ClientMessage::SubmitAction {
            expected_seq: 2,
            client_action_id: "kong-1".to_string(),
            action: PlayerAction::ConcealedKong { tile: tile("1m") },
        },
    )
    .await;
    assert_no_pending_messages_via_ping(&mut p_east, 222).await;

    // 3. Stale expected_seq -> ActionRejected { current_seq: 4 }.
    send_msg(
        &mut p_east,
        &ClientMessage::SubmitAction {
            expected_seq: 2,
            client_action_id: "stale-seq".to_string(),
            action: PlayerAction::Discard {
                tile: tile("9p"),
                from_drawn: true,
            },
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut p_east).await,
        ServerMessage::ActionRejected { current_seq: 4, .. }
    ));

    // 4. Illegal action / wrong turn -> ActionRejected { current_seq: 4 }.
    send_msg(
        &mut p_south,
        &ClientMessage::SubmitAction {
            expected_seq: 4,
            client_action_id: "wrong-turn".to_string(),
            action: PlayerAction::Pass,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut p_south).await,
        ServerMessage::ActionRejected { current_seq: 4, .. }
    ));

    // 5. East discards 5m -> South (Pon) and West (Ron) both have reactions; East & North have empty legal_actions.
    send_msg(
        &mut p_east,
        &ClientMessage::SubmitAction {
            expected_seq: 4,
            client_action_id: "discard-5m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        },
    )
    .await;
    let (_, _, _, legal_e) = recv_event_batch(&mut p_east).await;
    let (_, _, _, legal_s) = recv_event_batch(&mut p_south).await;
    let (_, _, _, legal_w) = recv_event_batch(&mut p_west).await;
    let (_, _, _, legal_n) = recv_event_batch(&mut p_north).await;
    assert!(legal_e.is_empty());
    assert!(legal_n.is_empty());
    assert!(legal_s.contains(&PlayerAction::Pon { tile: tile("5m") }));
    assert!(legal_w.contains(&PlayerAction::Ron { tile: tile("5m") }));

    // 6. South submits Pass (partial reaction window -> Ok(Vec::new())).
    // ONLY South receives empty EventBatch(5..=5, events: [], updated_legal_actions: []).
    // East, West, North receive NOTHING.
    send_msg(
        &mut p_south,
        &ClientMessage::SubmitAction {
            expected_seq: 5,
            client_action_id: "south-pass".to_string(),
            action: PlayerAction::Pass,
        },
    )
    .await;
    let (from_s, to_s, ev_s, legal_s_after) = recv_event_batch(&mut p_south).await;
    assert_eq!((from_s, to_s), (5, 5));
    assert!(ev_s.is_empty());
    assert!(legal_s_after.is_empty());

    assert_no_pending_messages_via_ping(&mut p_east, 333).await;
    assert_no_pending_messages_via_ping(&mut p_west, 444).await;
    assert_no_pending_messages_via_ping(&mut p_north, 555).await;

    // Duplicate retry of partial reaction id is also idempotently ignored.
    send_msg(
        &mut p_south,
        &ClientMessage::SubmitAction {
            expected_seq: 5,
            client_action_id: "south-pass".to_string(),
            action: PlayerAction::Pass,
        },
    )
    .await;
    assert_no_pending_messages_via_ping(&mut p_south, 666).await;

    // 7. RequestSnapshot mid-match preserves ConcealedKong zero leakage.
    send_msg(&mut p_south, &ClientMessage::RequestSnapshot).await;
    let snap_s_mid = recv_snapshot(&mut p_south).await;
    assert_eq!(snap_s_mid.seq, 5);
    let view_s_mid = snap_s_mid.player_view.unwrap();
    assert_eq!(
        view_s_mid.seats[Seat::East.index()].melds[0],
        MeldView::ConcealedKong { tile: None, seq: 3 }
    );

    // 8. West submits Ron(5m) -> HandSettled reveals ConcealedKong(1m).
    send_msg(
        &mut p_west,
        &ClientMessage::SubmitAction {
            expected_seq: 5,
            client_action_id: "west-ron".to_string(),
            action: PlayerAction::Ron { tile: tile("5m") },
        },
    )
    .await;
    for client in [&mut p_east, &mut p_south, &mut p_west, &mut p_north] {
        let (from_seq, to_seq, evs, _) = recv_event_batch(client).await;
        assert_eq!((from_seq, to_seq), (6, 6));
        match &evs[0].kind {
            PlayerEventKind::HandSettled { settlement } => {
                assert!(matches!(
                    settlement.outcome,
                    HandOutcome::Win { ref winners, .. }
                        if winners.len() == 1
                            && winners[0].seat == Seat::West
                            && winners[0].win_kind == WinKind::Ron
                ));
                assert_eq!(
                    settlement.revealed_hands[Seat::East.index()].melds[0],
                    RevealedMeld::ConcealedKong {
                        tile: tile("1m"),
                        seq: 3
                    }
                );
            }
            other => panic!("expected HandSettled, got {other:?}"),
        }
    }

    for mut c in [obs, p_east, p_south, p_west, p_north] {
        let _ = c.close(None).await;
    }
    server.shutdown().await;
}

#[tokio::test]
async fn one_player_three_ai_completes_full_east_only_match_and_emits_settled_plus_finished_batch()
{
    let server = spawn_seeded_server(777, None).await;
    let addr = server.addr();

    let mut human = connect_and_hello(addr, "solo-human", "Alice").await;
    send_msg(&mut human, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut human).await;
    send_msg(&mut human, &ClientMessage::StartMatch).await;

    let initial_snap = recv_snapshot(&mut human).await;
    assert_eq!(initial_snap.phase, TablePhase::InMatch);
    let human_seat = initial_snap.your_seat.expect("human seated");

    let mut current_seq = initial_snap.seq;
    let mut legal_actions = initial_snap.player_view.unwrap().legal_actions;
    let mut rng = ChaCha8Rng::seed_from_u64(9001);
    let mut action_counter: u64 = 0;
    let mut hands_settled = 0u32;
    let mut saw_combined_finish_batch = false;
    let finished_snapshot: TableSnapshot;

    let observe_batch = |from_seq: u64,
                         to_seq: u64,
                         events: &[PlayerEvent],
                         current_seq: &mut u64,
                         hands_settled: &mut u32,
                         saw_combined_finish_batch: &mut bool| {
        if !events.is_empty() {
            assert_eq!(from_seq, *current_seq + 1);
            *current_seq = to_seq;
        }

        let mut batch_had_settled = false;
        let mut batch_had_finished = false;
        let mut ai_confirms_in_batch = HashSet::new();

        for ev in events {
            match &ev.kind {
                PlayerEventKind::TileDrawn { seat, tile, .. } => {
                    if *seat == human_seat {
                        assert!(tile.is_some());
                    } else {
                        assert!(tile.is_none(), "must not leak opponent draw tile");
                    }
                }
                PlayerEventKind::MeldDeclared {
                    seat,
                    meld: MeldView::ConcealedKong { tile, .. },
                    ..
                } if *seat != human_seat => {
                    assert!(tile.is_none(), "must not leak opponent ConcealedKong");
                }
                PlayerEventKind::HandSettled { .. } => {
                    *hands_settled += 1;
                    batch_had_settled = true;
                }
                PlayerEventKind::NextHandConfirmed { seat } if *seat != human_seat => {
                    ai_confirms_in_batch.insert(*seat);
                }
                PlayerEventKind::MatchFinished { summary } => {
                    batch_had_finished = true;
                    assert_eq!(summary.mode, MatchMode::EastOnly);
                    assert!(summary.hands_played >= 4);
                }
                _ => {}
            }
        }

        if batch_had_settled && !batch_had_finished {
            assert_eq!(ai_confirms_in_batch.len(), 3);
        }
        if batch_had_settled && batch_had_finished {
            *saw_combined_finish_batch = true;
        }
    };

    loop {
        if !legal_actions.is_empty() {
            action_counter += 1;
            let action = if legal_actions.contains(&PlayerAction::ConfirmNextHand) {
                PlayerAction::ConfirmNextHand
            } else {
                send_msg(&mut human, &ClientMessage::RequestSnapshot).await;
                let snap = loop {
                    match recv_msg(&mut human).await {
                        ServerMessage::TableSnapshot(s) => break s,
                        ServerMessage::EventBatch {
                            from_seq,
                            to_seq,
                            events,
                            ..
                        } => {
                            observe_batch(
                                from_seq,
                                to_seq,
                                &events,
                                &mut current_seq,
                                &mut hands_settled,
                                &mut saw_combined_finish_batch,
                            );
                            continue;
                        }
                        other => panic!("unexpected message while awaiting snapshot: {other:?}"),
                    }
                };
                if snap.phase == TablePhase::Finished {
                    finished_snapshot = snap;
                    break;
                }
                current_seq = snap.seq;
                let view = snap.player_view.unwrap();
                legal_actions = view.legal_actions.clone();
                if legal_actions.is_empty() {
                    continue;
                }
                TileEfficiencyBot::decide(&view, &mut rng)
            };

            send_msg(
                &mut human,
                &ClientMessage::SubmitAction {
                    expected_seq: current_seq,
                    client_action_id: format!("h-{action_counter}"),
                    action,
                },
            )
            .await;
        }

        match recv_msg(&mut human).await {
            ServerMessage::EventBatch {
                from_seq,
                to_seq,
                events,
                updated_legal_actions,
            } => {
                observe_batch(
                    from_seq,
                    to_seq,
                    &events,
                    &mut current_seq,
                    &mut hands_settled,
                    &mut saw_combined_finish_batch,
                );
                legal_actions = updated_legal_actions;
            }
            ServerMessage::TableSnapshot(snap) => {
                if snap.phase == TablePhase::Finished {
                    finished_snapshot = snap;
                    break;
                }
            }
            other => panic!("unexpected server message during 1P+3AI match: {other:?}"),
        }
    }

    assert!(hands_settled >= 4);
    assert!(
        saw_combined_finish_batch,
        "EastOnly final hand must emit HandSettled + MatchFinished in the same EventBatch"
    );
    let final_snap = finished_snapshot;
    assert_eq!(final_snap.phase, TablePhase::Finished);
    assert!(final_snap.pending_destructive_op.is_none());
    let final_view = final_snap.player_view.unwrap();
    assert_eq!(final_view.phase, PublicPhase::MatchFinished);
    assert!(final_view.match_summary.is_some());
    assert_eq!(fetch_health(addr).await.phase, TablePhase::Finished);

    let _ = human.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn multi_ai_concurrent_reaction_window_on_same_seq_resolves_cleanly() {
    // East (Human) discards 5m; South (AI, Pon 5m), West (AI, Ron 5m), North (AI, Ron 5m)
    // are all eligible on the exact same seq and run concurrently via spawn_blocking.
    let wall = build_preset_wall(
        &tiles("5m 123s 456s 789p 111m E"),
        &tiles("55m 14m 678m 111p CC 2s"),
        &tiles("46m 345s 678s 789p FF"),
        &tiles("5m 999m 888s 222p BBB"),
        &tiles("1s"),
        &tiles("9p"),
    )
    .expect("valid preset wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server = spawn_with_wall_factory(88, factory).await;
    let addr = server.addr();

    let mut human = connect_and_hello(addr, "h1", "EastHuman").await;
    send_msg(
        &mut human,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut human).await;
    send_msg(
        &mut human,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut human, &ClientMessage::StartMatch).await;
    let start_snap = recv_snapshot(&mut human).await;
    assert_eq!(start_snap.your_seat, Some(Seat::East));
    assert_eq!(start_snap.seq, 2);

    // East discards 5m -> triggers concurrent reaction window for South, West, and North AIs!
    send_msg(
        &mut human,
        &ClientMessage::SubmitAction {
            expected_seq: 2,
            client_action_id: "discard-5m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        },
    )
    .await;

    let (f1, t1, ev1, _) = recv_event_batch(&mut human).await;
    assert_eq!((f1, t1), (3, 3));
    assert!(matches!(
        ev1[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::East,
            tile: t,
            ..
        } if t == tile("5m")
    ));

    // Next batch must be the resolved multi-ron HandSettled (+ AI auto-confirm NextHandConfirmed).
    let (f2, _, ev2, legal2) = recv_event_batch(&mut human).await;
    assert_eq!(f2, 4);
    match &ev2[0].kind {
        PlayerEventKind::HandSettled { settlement } => match &settlement.outcome {
            HandOutcome::Win { winners, .. } => {
                let win_seats: HashSet<Seat> = winners.iter().map(|w| w.seat).collect();
                assert_eq!(win_seats, HashSet::from([Seat::West, Seat::North]));
            }
            other => panic!("expected multi-ron Win, got {other:?}"),
        },
        other => panic!("expected HandSettled, got {other:?}"),
    }
    assert!(legal2.contains(&PlayerAction::ConfirmNextHand));

    let _ = human.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn added_kong_attempted_two_phase_pass_and_rob_kong_hu_over_websocket() {
    let wall = build_preset_wall(
        &tiles("55m 234s 567888p EEE"),
        &tiles("46m 111234p 789s BC"),
        &tiles("222333m 444s 99p FF"),
        &tiles("5m 777888999m FBB"),
        &tiles("1m 9p 9s B 8s 7s 5m"),
        &tiles("1s"),
    )
    .expect("valid preset wall");

    let wall_for_pass = wall.clone();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall_for_pass.clone(),
        )))
    });
    let server = spawn_with_wall_factory(55, factory).await;
    let addr = server.addr();

    let mut p_east = connect_and_hello(addr, "e1", "East").await;
    let mut p_south = connect_and_hello(addr, "s1", "South").await;
    let mut p_west = connect_and_hello(addr, "w1", "West").await;
    let mut p_north = connect_and_hello(addr, "n1", "North").await;

    for (ws, seat) in [
        (&mut p_east, Seat::East),
        (&mut p_south, Seat::South),
        (&mut p_west, Seat::West),
        (&mut p_north, Seat::North),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }

    send_msg(
        &mut p_east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut p_east, &ClientMessage::StartMatch).await;
    for ws in [&mut p_east, &mut p_south, &mut p_west, &mut p_north] {
        let _ = recv_snapshot(ws).await;
    }

    async fn drive_to_added_kong_attempt(
        seats: &mut [&mut WsClient; 4],
        mut seq: u64,
        tag: &str,
    ) -> u64 {
        let script = [
            (
                Seat::East,
                PlayerAction::Discard {
                    tile: tile("E"),
                    from_drawn: false,
                },
            ),
            (
                Seat::South,
                PlayerAction::Discard {
                    tile: tile("1m"),
                    from_drawn: true,
                },
            ),
            (
                Seat::West,
                PlayerAction::Discard {
                    tile: tile("9p"),
                    from_drawn: true,
                },
            ),
            (
                Seat::North,
                PlayerAction::Discard {
                    tile: tile("5m"),
                    from_drawn: false,
                },
            ),
            (Seat::East, PlayerAction::Pon { tile: tile("5m") }),
            (
                Seat::East,
                PlayerAction::Discard {
                    tile: tile("E"),
                    from_drawn: false,
                },
            ),
            (
                Seat::South,
                PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            ),
            (
                Seat::West,
                PlayerAction::Discard {
                    tile: tile("8s"),
                    from_drawn: true,
                },
            ),
            (
                Seat::North,
                PlayerAction::Discard {
                    tile: tile("7s"),
                    from_drawn: true,
                },
            ),
        ];

        for (step, (actor, action)) in script.into_iter().enumerate() {
            send_msg(
                seats[actor.index()],
                &ClientMessage::SubmitAction {
                    expected_seq: seq,
                    client_action_id: format!("{tag}-{step}"),
                    action,
                },
            )
            .await;
            for ws in seats.iter_mut() {
                let (_, to_seq, _, _) = recv_event_batch(ws).await;
                seq = to_seq;
            }
        }

        // East submits AddedKong(5m) -> emits AddedKongAttempted, opens RobKongHu window for South!
        send_msg(
            seats[Seat::East.index()],
            &ClientMessage::SubmitAction {
                expected_seq: seq,
                client_action_id: format!("{tag}-added-kong"),
                action: PlayerAction::AddedKong { tile: tile("5m") },
            },
        )
        .await;
        for (idx, ws) in seats.iter_mut().enumerate() {
            let (_, to_seq, evs, legal) = recv_event_batch(ws).await;
            seq = to_seq;
            assert!(matches!(
                evs[0].kind,
                PlayerEventKind::AddedKongAttempted {
                    seat: Seat::East,
                    tile: t
                } if t == tile("5m")
            ));
            if idx == Seat::South.index() {
                assert!(legal.contains(&PlayerAction::RobKongHu { tile: tile("5m") }));
                assert!(legal.contains(&PlayerAction::Pass));
            } else {
                assert!(legal.is_empty());
            }
        }
        seq
    }

    // Pass branch: South passes on AddedKongAttempted -> MeldDeclared(AddedKong) + replacement draw.
    let seq = drive_to_added_kong_attempt(
        &mut [&mut p_east, &mut p_south, &mut p_west, &mut p_north],
        2,
        "h1",
    )
    .await;

    send_msg(
        &mut p_south,
        &ClientMessage::SubmitAction {
            expected_seq: seq,
            client_action_id: "h1-south-pass".to_string(),
            action: PlayerAction::Pass,
        },
    )
    .await;
    for (idx, ws) in [&mut p_east, &mut p_south, &mut p_west, &mut p_north]
        .into_iter()
        .enumerate()
    {
        let (_, _, evs, _) = recv_event_batch(ws).await;
        assert!(matches!(
            evs[0].kind,
            PlayerEventKind::MeldDeclared {
                seat: Seat::East,
                meld: MeldView::AddedKong { tile: t, .. },
                kong_score_deltas: Some(_),
            } if t == tile("5m")
        ));
        assert!(matches!(
            evs[1].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::East,
                tile: ref dt,
                is_replacement: true,
                ..
            } if (idx == Seat::East.index()) == dt.is_some()
        ));
    }

    for mut c in [p_east, p_south, p_west, p_north] {
        let _ = c.close(None).await;
    }
    server.shutdown().await;

    // RobKongHu branch: South robs East's AddedKongAttempted.
    let factory2: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server2 = spawn_with_wall_factory(56, factory2).await;
    let addr2 = server2.addr();
    let mut e2 = connect_and_hello(addr2, "e2", "East").await;
    let mut s2 = connect_and_hello(addr2, "s2", "South").await;
    let mut w2 = connect_and_hello(addr2, "w2", "West").await;
    let mut n2 = connect_and_hello(addr2, "n2", "North").await;
    for (ws, seat) in [
        (&mut e2, Seat::East),
        (&mut s2, Seat::South),
        (&mut w2, Seat::West),
        (&mut n2, Seat::North),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }
    send_msg(
        &mut e2,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut e2, &ClientMessage::StartMatch).await;
    for ws in [&mut e2, &mut s2, &mut w2, &mut n2] {
        let _ = recv_snapshot(ws).await;
    }

    let seq2 =
        drive_to_added_kong_attempt(&mut [&mut e2, &mut s2, &mut w2, &mut n2], 2, "h2").await;
    send_msg(
        &mut s2,
        &ClientMessage::SubmitAction {
            expected_seq: seq2,
            client_action_id: "h2-rob".to_string(),
            action: PlayerAction::RobKongHu { tile: tile("5m") },
        },
    )
    .await;
    for ws in [&mut e2, &mut s2, &mut w2, &mut n2] {
        let (_, _, evs, _) = recv_event_batch(ws).await;
        match &evs[0].kind {
            PlayerEventKind::HandSettled { settlement } => {
                assert!(matches!(
                    settlement.outcome,
                    HandOutcome::Win { ref winners, .. }
                        if winners.len() == 1
                            && winners[0].seat == Seat::South
                            && winners[0].win_kind == WinKind::RobKongHu
                ));
                assert!(settlement.kong_transfers.is_empty());
                assert!(matches!(
                    settlement.revealed_hands[Seat::East.index()].melds[0],
                    RevealedMeld::Pon { .. }
                ));
            }
            other => panic!("expected HandSettled(RobKongHu), got {other:?}"),
        }
    }

    for mut c in [e2, s2, w2, n2] {
        let _ = c.close(None).await;
    }
    server2.shutdown().await;
}

#[tokio::test]
async fn destructive_ops_end_free_table_and_disband_match_mutual_confirm_and_offline_unlock() {
    // Wall where East (dealer) holds a 14-tile Tsumo immediately at seq = 2.
    let tsumo_wall = build_preset_wall(
        &tiles("111m 234s 567p 888p EE"),
        &tiles("123m 456m 789m 111s C"),
        &tiles("234p 567s 899s 11p FF"),
        &tiles("345m 678s 999p BBB N"),
        &tiles("1s"),
        &tiles("9p"),
    )
    .expect("valid tsumo preset wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![
                tsumo_wall.clone(),
                tsumo_wall.clone(),
                tsumo_wall.clone(),
                tsumo_wall.clone(),
            ],
        )))
    });
    let server = spawn_with_wall_factory(606, factory).await;
    let addr = server.addr();

    let mut p1 = connect_and_hello(addr, "d1", "Alice").await;
    send_msg(&mut p1, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut p1).await;

    // 1. Solo EastOnly: RequestEndFreeTable rejected; RequestDisbandMatch executes immediately.
    send_msg(&mut p1, &ClientMessage::StartMatch).await;
    let s1 = recv_snapshot(&mut p1).await;
    assert_eq!(s1.phase, TablePhase::InMatch);

    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    assert!(matches!(
        recv_msg(&mut p1).await,
        ServerMessage::ErrorNotice { .. }
    ));

    send_msg(&mut p1, &ClientMessage::RequestDisbandMatch).await;
    let disbanded = recv_snapshot(&mut p1).await;
    assert_eq!(disbanded.phase, TablePhase::Lobby);
    assert_eq!(disbanded.seq, 0);
    assert!(disbanded.player_view.is_none());
    assert!(disbanded.pending_destructive_op.is_none());
    let p1_seat = disbanded.your_seat.unwrap();
    for seat in Seat::ALL {
        if seat == p1_seat {
            assert_eq!(disbanded.seats[seat.index()].kind, SeatKind::Human);
        } else {
            assert_eq!(disbanded.seats[seat.index()].kind, SeatKind::Empty);
        }
    }

    // 2. Solo FreeTable: RequestEndFreeTable invalid during Playing, executes immediately in BetweenHands.
    send_msg(
        &mut p1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut p1, &ClientMessage::StartMatch).await;
    let s2 = recv_snapshot(&mut p1).await;
    assert_eq!(s2.your_seat, Some(Seat::East));

    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    assert!(matches!(
        recv_msg(&mut p1).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // Tsumo -> BetweenHands -> RequestEndFreeTable immediately finishes match.
    send_msg(
        &mut p1,
        &ClientMessage::SubmitAction {
            expected_seq: s2.seq,
            client_action_id: "solo-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    let _ = recv_event_batch(&mut p1).await;

    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    let (_, _, end_evs, _) = recv_event_batch(&mut p1).await;
    assert!(matches!(
        end_evs[0].kind,
        PlayerEventKind::MatchFinished { .. }
    ));
    let finished_solo = recv_snapshot(&mut p1).await;
    assert_eq!(finished_solo.phase, TablePhase::Finished);
    assert!(finished_solo.pending_destructive_op.is_none());

    // ReturnToLobby from Finished resets AI seats back to Empty.
    send_msg(&mut p1, &ClientMessage::ReturnToLobby).await;
    let back_lobby = recv_snapshot(&mut p1).await;
    assert_eq!(back_lobby.phase, TablePhase::Lobby);
    assert_eq!(back_lobby.seq, 0);
    assert!(back_lobby.player_view.is_none());
    assert_eq!(
        back_lobby
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Empty)
            .count(),
        3
    );

    // 3. 2P + 2AI FreeTable: direct EndMatch rejected, RequestEndFreeTable mutual reject/accept,
    //    rematch from Finished, and RequestDisbandMatch unlocked on disconnect.
    let mut p2 = connect_and_hello(addr, "d2", "Bob").await;
    send_msg(&mut p2, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut p2).await;

    send_msg(&mut p1, &ClientMessage::StartMatch).await;
    let snap1 = recv_snapshot(&mut p1).await;
    let snap2 = recv_snapshot(&mut p2).await;
    assert_eq!(snap1.your_seat, Some(Seat::East));
    assert_eq!(snap2.your_seat, Some(Seat::West));

    send_msg(
        &mut p1,
        &ClientMessage::SubmitAction {
            expected_seq: snap1.seq,
            client_action_id: "2p-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    let (_, between_seq, _, _) = recv_event_batch(&mut p1).await;
    let _ = recv_event_batch(&mut p2).await;

    // Direct SubmitAction(EndMatch) must be rejected while >= 2 humans are online.
    send_msg(
        &mut p1,
        &ClientMessage::SubmitAction {
            expected_seq: between_seq,
            client_action_id: "direct-end".to_string(),
            action: PlayerAction::EndMatch,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut p1).await,
        ServerMessage::ActionRejected { current_seq, .. } if current_seq == between_seq
    ));

    // RequestEndFreeTable creates PendingDestructiveOp.
    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    let p1_op_snap = recv_snapshot(&mut p1).await;
    let p2_op_snap = recv_snapshot(&mut p2).await;
    let op = p1_op_snap
        .pending_destructive_op
        .expect("must have pending destructive op");
    assert_eq!(op.kind, DestructiveOpKind::EndFreeTable);
    assert_eq!(op.requested_by, Seat::East);
    assert_eq!(op.required_seats, vec![Seat::East, Seat::West]);
    assert_eq!(op.confirmed_seats, vec![Seat::East]);
    assert_eq!(p2_op_snap.pending_destructive_op, Some(op.clone()));

    // P2 declines -> clears pending_destructive_op and stays InMatch.
    send_msg(
        &mut p2,
        &ClientMessage::ConfirmDestructiveOp {
            op_id: op.op_id,
            accept: false,
        },
    )
    .await;
    let p1_cleared = recv_snapshot(&mut p1).await;
    let p2_cleared = recv_snapshot(&mut p2).await;
    assert_eq!(p1_cleared.phase, TablePhase::InMatch);
    assert!(p1_cleared.pending_destructive_op.is_none());
    assert!(p2_cleared.pending_destructive_op.is_none());

    // P1 requests again, P2 accepts -> MatchFinished EventBatch + TableSnapshot(Finished).
    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    let _ = recv_snapshot(&mut p1).await;
    let p2_op2_snap = recv_snapshot(&mut p2).await;
    let op2 = p2_op2_snap.pending_destructive_op.unwrap();
    send_msg(
        &mut p2,
        &ClientMessage::ConfirmDestructiveOp {
            op_id: op2.op_id,
            accept: true,
        },
    )
    .await;
    for client in [&mut p1, &mut p2] {
        let (_, _, evs, _) = recv_event_batch(client).await;
        assert!(matches!(evs[0].kind, PlayerEventKind::MatchFinished { .. }));
        let fin = recv_snapshot(client).await;
        assert_eq!(fin.phase, TablePhase::Finished);
        assert!(fin.pending_destructive_op.is_none());
    }

    // Rematch directly from Finished ("原班人马再开一桌").
    send_msg(&mut p2, &ClientMessage::StartMatch).await;
    let rematch1 = recv_snapshot(&mut p1).await;
    let rematch2 = recv_snapshot(&mut p2).await;
    assert_eq!(rematch1.phase, TablePhase::InMatch);
    assert_eq!(rematch2.phase, TablePhase::InMatch);
    assert_eq!(rematch1.seq, 2);

    // Mid-match RequestDisbandMatch remains pending across Playing actions until confirmed or unlocked!
    send_msg(&mut p1, &ClientMessage::RequestDisbandMatch).await;
    let disband_snap1 = recv_snapshot(&mut p1).await;
    let disband_snap2 = recv_snapshot(&mut p2).await;
    assert_eq!(
        disband_snap1.pending_destructive_op.as_ref().unwrap().kind,
        DestructiveOpKind::DisbandMatch
    );
    assert!(disband_snap2.pending_destructive_op.is_some());

    send_msg(
        &mut p1,
        &ClientMessage::SubmitAction {
            expected_seq: rematch1.seq,
            client_action_id: "discard-while-disband-pending".to_string(),
            action: PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: false,
            },
        },
    )
    .await;
    let _ = recv_event_batch(&mut p1).await;
    let _ = recv_event_batch(&mut p2).await;
    send_msg(&mut p1, &ClientMessage::RequestSnapshot).await;
    let still_pending = loop {
        match recv_msg(&mut p1).await {
            ServerMessage::TableSnapshot(s) => break s,
            ServerMessage::EventBatch { .. } => continue,
            other => panic!("unexpected msg: {other:?}"),
        }
    };
    assert_eq!(
        still_pending.pending_destructive_op.as_ref().unwrap().kind,
        DestructiveOpKind::DisbandMatch
    );

    send_msg(&mut p2, &ClientMessage::ClientLeaving).await;
    let _ = p2.close(None).await;

    let unlocked_lobby = loop {
        match recv_msg(&mut p1).await {
            ServerMessage::TableSnapshot(s) => break s,
            ServerMessage::EventBatch { .. } | ServerMessage::LobbyUpdated { .. } => continue,
            other => panic!("unexpected msg: {other:?}"),
        }
    };
    assert_eq!(unlocked_lobby.phase, TablePhase::Lobby);
    assert!(unlocked_lobby.pending_destructive_op.is_none());
    assert_eq!(unlocked_lobby.seq, 0);
    assert!(unlocked_lobby.player_view.is_none());
    assert_eq!(
        unlocked_lobby
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Empty)
            .count(),
        2
    );

    let _ = p1.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn two_player_two_ai_full_east_only_with_reconnect_then_free_table_rematch_and_return_to_lobby(
) {
    let server = spawn_seeded_server(1234, None).await;
    let addr = server.addr();

    let mut p1 = connect_and_hello(addr, "2p-a", "Alice").await;
    let mut p2 = connect_and_hello(addr, "2p-b", "Bob").await;
    send_msg(&mut p1, &ClientMessage::JoinSeat { seat: None }).await;
    let snap1_join = recv_snapshot(&mut p1).await;
    let table_id = snap1_join.table_id.clone();
    let token1 = snap1_join.seat_token.clone().unwrap();

    send_msg(&mut p2, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut p2).await;

    send_msg(
        &mut p1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Fast,
        },
    )
    .await;
    send_msg(&mut p1, &ClientMessage::StartMatch).await;
    let s1 = recv_snapshot(&mut p1).await;
    let s2 = recv_snapshot(&mut p2).await;
    assert_eq!(s1.your_seat, Some(Seat::East));
    assert_eq!(s2.your_seat, Some(Seat::West));

    // Test mid-match disconnect & reconnect restoration of PlayerView for p1.
    send_msg(&mut p1, &ClientMessage::ClientLeaving).await;
    let _ = p1.close(None).await;
    // Drain LobbyUpdated on p2 from p1 leaving.
    loop {
        match recv_msg(&mut p2).await {
            ServerMessage::LobbyUpdated { seats, .. } => {
                if !seats[Seat::East.index()].connected {
                    break;
                }
            }
            ServerMessage::EventBatch { .. } => {}
            other => panic!("unexpected msg on p2 during p1 disconnect: {other:?}"),
        }
    }

    let mut p1 = connect_ws(addr).await;
    send_msg(
        &mut p1,
        &ClientMessage::Hello {
            client_id: "2p-a".to_string(),
            nickname: "Alice".to_string(),
            table_id: Some(table_id),
            seat_token: Some(token1),
        },
    )
    .await;
    let restored = recv_snapshot(&mut p1).await;
    assert_eq!(restored.phase, TablePhase::InMatch);
    assert_eq!(restored.your_seat, Some(Seat::East));
    let restored_view = restored.player_view.expect("reconnect restores PlayerView");
    assert_eq!(restored_view.viewer, Seat::East);

    async fn recv_snapshot_until<F>(ws: &mut WsClient, pred: F) -> TableSnapshot
    where
        F: Fn(&TableSnapshot) -> bool,
    {
        loop {
            match recv_msg(ws).await {
                ServerMessage::TableSnapshot(snap) if pred(&snap) => return snap,
                ServerMessage::TableSnapshot(_)
                | ServerMessage::EventBatch { .. }
                | ServerMessage::LobbyUpdated { .. } => continue,
                other => panic!("unexpected ws message while awaiting snapshot: {other:?}"),
            }
        }
    }

    async fn drive_two_player_match(
        p1: &mut WsClient,
        p2: &mut WsClient,
        seat1: Seat,
        seat2: Seat,
        free_table_target_hands: Option<u32>,
    ) -> TableSnapshot {
        let mut rng = ChaCha8Rng::seed_from_u64(4242);
        let mut step_id = 0u64;
        let mut settled_seqs: HashSet<u64> = HashSet::new();

        loop {
            // Poll snapshots for both humans to act cleanly whenever either has legal actions.
            let mut finished_snap: Option<TableSnapshot> = None;
            let mut acted = false;
            for (ws, seat) in [(&mut *p1, seat1), (&mut *p2, seat2)] {
                send_msg(ws, &ClientMessage::RequestSnapshot).await;
                let snap = loop {
                    match recv_msg(ws).await {
                        ServerMessage::TableSnapshot(s) => break s,
                        ServerMessage::EventBatch { events, .. } => {
                            for ev in &events {
                                if matches!(ev.kind, PlayerEventKind::HandSettled { .. }) {
                                    settled_seqs.insert(ev.seq);
                                }
                            }
                            continue;
                        }
                        ServerMessage::LobbyUpdated { .. } => continue,
                        other => panic!("unexpected ws message for {seat:?}: {other:?}"),
                    }
                };
                if snap.phase == TablePhase::Finished {
                    finished_snap = Some(snap);
                    break;
                }
                let seq = snap.seq;
                let view = snap.player_view.as_ref().unwrap();
                assert_eq!(view.viewer, seat);

                // In FreeTable mode, stop once target hands have settled and we are BetweenHands.
                if let Some(target) = free_table_target_hands {
                    if view.phase == PublicPhase::BetweenHands
                        && view.last_settlement.is_some()
                        && (settled_seqs.len() as u32) >= target
                    {
                        return snap;
                    }
                }

                if !view.legal_actions.is_empty() {
                    step_id += 1;
                    let action = if view.legal_actions.contains(&PlayerAction::ConfirmNextHand) {
                        PlayerAction::ConfirmNextHand
                    } else {
                        TileEfficiencyBot::decide(view, &mut rng)
                    };
                    send_msg(
                        ws,
                        &ClientMessage::SubmitAction {
                            expected_seq: seq,
                            client_action_id: format!("2p-{step_id}"),
                            action,
                        },
                    )
                    .await;
                    // Drain result (EventBatch / TableSnapshot / ActionRejected if AI raced)
                    match recv_msg(ws).await {
                        ServerMessage::EventBatch { events, .. } => {
                            for ev in &events {
                                if matches!(ev.kind, PlayerEventKind::HandSettled { .. }) {
                                    settled_seqs.insert(ev.seq);
                                }
                            }
                        }
                        ServerMessage::TableSnapshot(fin) if fin.phase == TablePhase::Finished => {
                            return fin;
                        }
                        ServerMessage::ActionRejected { .. }
                        | ServerMessage::LobbyUpdated { .. } => {}
                        other => panic!("unexpected response after SubmitAction: {other:?}"),
                    }
                    acted = true;
                }
            }
            if let Some(fin) = finished_snap {
                return fin;
            }
            if !acted {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }

    let east_only_fin =
        drive_two_player_match(&mut p1, &mut p2, Seat::East, Seat::West, None).await;
    assert_eq!(east_only_fin.phase, TablePhase::Finished);
    let summary = east_only_fin.player_view.unwrap().match_summary.unwrap();
    assert_eq!(summary.mode, MatchMode::EastOnly);
    assert!(summary.hands_played >= 4);

    // Rematch directly from Finished switching to FreeTable ("原班人马再开一桌").
    send_msg(
        &mut p2,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut p2, &ClientMessage::StartMatch).await;
    let free_s1 = recv_snapshot_until(&mut p1, |s| s.phase == TablePhase::InMatch).await;
    let free_s2 = recv_snapshot_until(&mut p2, |s| s.phase == TablePhase::InMatch).await;
    assert_eq!(free_s1.your_seat, Some(Seat::East));
    assert_eq!(free_s2.your_seat, Some(Seat::South));

    // Play 2 hands of FreeTable then end mutually via RequestEndFreeTable + ConfirmDestructiveOp.
    let _between = drive_two_player_match(&mut p1, &mut p2, Seat::East, Seat::South, Some(2)).await;

    send_msg(&mut p1, &ClientMessage::RequestEndFreeTable).await;
    let p2_req_snap = recv_snapshot_until(&mut p2, |s| s.pending_destructive_op.is_some()).await;
    let op = p2_req_snap
        .pending_destructive_op
        .expect("FreeTable end op must be pending");
    send_msg(
        &mut p2,
        &ClientMessage::ConfirmDestructiveOp {
            op_id: op.op_id,
            accept: true,
        },
    )
    .await;
    let free_fin1 = recv_snapshot_until(&mut p1, |s| s.phase == TablePhase::Finished).await;
    let free_fin2 = recv_snapshot_until(&mut p2, |s| s.phase == TablePhase::Finished).await;
    assert_eq!(free_fin1.phase, TablePhase::Finished);
    assert_eq!(free_fin2.phase, TablePhase::Finished);
    assert!(
        free_fin1
            .player_view
            .unwrap()
            .match_summary
            .unwrap()
            .hands_played
            >= 2
    );

    // Seated human submitting SubmitAction after match transitioned to Finished receives ActionRejected (not locking UI).
    send_msg(
        &mut p1,
        &ClientMessage::SubmitAction {
            expected_seq: free_fin1.seq,
            client_action_id: "late-after-finish".to_string(),
            action: PlayerAction::ConfirmNextHand,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut p1).await,
        ServerMessage::ActionRejected { current_seq, .. } if current_seq == free_fin1.seq
    ));

    // ReturnToLobby from Finished.
    send_msg(&mut p1, &ClientMessage::ReturnToLobby).await;
    let lobby1 = recv_snapshot_until(&mut p1, |s| s.phase == TablePhase::Lobby).await;
    let lobby2 = recv_snapshot_until(&mut p2, |s| s.phase == TablePhase::Lobby).await;
    assert_eq!(lobby1.phase, TablePhase::Lobby);
    assert_eq!(lobby2.phase, TablePhase::Lobby);
    assert_eq!(lobby1.seq, 0);
    assert!(lobby1.player_view.is_none());
    assert_eq!(
        lobby1
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Human)
            .count(),
        2
    );
    assert_eq!(
        lobby1
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Empty)
            .count(),
        2
    );

    let _ = p1.close(None).await;
    let _ = p2.close(None).await;
    server.shutdown().await;
}

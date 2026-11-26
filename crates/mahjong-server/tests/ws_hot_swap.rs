mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use common::{
    connect_and_hello, connect_ws, no_reaction_wall, recv_event_batch,
    recv_non_ping_msg as recv_msg, recv_snapshot, send_msg, tile, tiles, WsClient,
};
use mahjong_ai::{AiDifficulty, AiStyle};
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, PublicPhase,
    Seat,
};
use mahjong_server::protocol::{
    ClientMessage, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::{PacingMode, ServerConfig, ServerHandle};

async fn spawn_test_server(
    seed: u64,
    pacing: PacingMode,
    factory: WallProviderFactory,
) -> ServerHandle {
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_ai_threads(2)
        .with_llm_client(common::stub_llm_client())
        .spawn_with_options(TableOptions::with_wall_factory(seed, factory).with_pacing(pacing))
        .await
        .expect("spawn server")
}

async fn recv_lobby_updated_matching<F>(ws: &mut WsClient, mut pred: F) -> TablePhase
where
    F: FnMut(&[mahjong_server::protocol::SeatInfo; 4]) -> bool,
{
    loop {
        match recv_msg(ws).await {
            ServerMessage::LobbyUpdated { phase, seats, .. } if pred(&seats) => return phase,
            ServerMessage::LobbyUpdated { .. } | ServerMessage::TableBanter { .. } => continue,
            other => panic!("expected matching LobbyUpdated, got {other:?}"),
        }
    }
}

async fn assert_no_pending_events_via_ping(ws: &mut WsClient, ts_ms: u64) {
    send_msg(ws, &ClientMessage::ClientPing { ts_ms }).await;
    assert_eq!(recv_msg(ws).await, ServerMessage::ServerPong { ts_ms });
}

async fn recv_error_notice(ws: &mut WsClient) -> String {
    loop {
        match recv_msg(ws).await {
            ServerMessage::ErrorNotice { message } => return message,
            ServerMessage::LobbyUpdated { .. } => continue,
            other => panic!("expected ErrorNotice, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn disconnect_pauses_indefinitely_and_toggle_ai_assist_plays_and_reclaims() {
    let wall = no_reaction_wall();
    let wall_hand2 = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall.clone(), wall_hand2.clone()],
        )))
    });
    let server = spawn_test_server(201, PacingMode::Immediate, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "p-east", "东家").await;
    let mut south = connect_and_hello(addr, "p-south", "南家").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;

    send_msg(
        &mut south,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let south_lobby = recv_snapshot(&mut south).await;
    let table_id = south_lobby.table_id.clone();
    let south_token = south_lobby.seat_token.clone().expect("south token");

    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;

    let east_snap = recv_snapshot(&mut east).await;
    let south_snap = recv_snapshot(&mut south).await;
    assert_eq!(east_snap.phase, TablePhase::InMatch);
    assert_eq!(east_snap.your_seat, Some(Seat::East));
    assert_eq!(south_snap.your_seat, Some(Seat::South));

    // East discards C -> South draws and it becomes South's turn.
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: east_snap.seq,
            client_action_id: "e-disc-1".to_string(),
            action: PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        },
    )
    .await;
    let (_, _, _, _) = recv_event_batch(&mut east).await;
    let (_, _, _, south_legal) = recv_event_batch(&mut south).await;
    assert!(!south_legal.is_empty());

    // South disconnects -> East sees South offline with ai_assisted=false.
    drop(south);
    recv_lobby_updated_matching(&mut east, |seats| {
        seats[Seat::South.index()].kind == SeatKind::Human && !seats[Seat::South.index()].connected
    })
    .await;

    // Wait briefly and verify no AI auto-play happens while paused.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_no_pending_events_via_ping(&mut east, 11).await;

    // East enables temporary AI assist for offline South.
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::South,
            enabled: true,
        },
    )
    .await;
    recv_lobby_updated_matching(&mut east, |seats| {
        let s = &seats[Seat::South.index()];
        s.ai_assisted
            && s.ai_difficulty == Some(AiDifficulty::Strong)
            && s.ai_style == Some(AiStyle::Balanced)
    })
    .await;

    // South AI + West AI + North AI play until East draws a tile.
    loop {
        let (_, _, events, legal) = recv_event_batch(&mut east).await;
        if events.iter().any(|e| {
            matches!(
                e.kind,
                PlayerEventKind::TileDrawn {
                    seat: Seat::East,
                    ..
                }
            )
        }) {
            assert!(!legal.is_empty());
            break;
        }
    }

    // Original South reconnects with saved credential; still sees ai_assisted=true until reclaimed.
    let mut south_re = connect_ws(addr).await;
    send_msg(
        &mut south_re,
        &ClientMessage::Hello {
            client_id: "p-south".to_string(),
            nickname: "南家".to_string(),
            table_id: Some(table_id),
            seat_token: Some(south_token),
        },
    )
    .await;
    let south_re_snap = recv_snapshot(&mut south_re).await;
    assert_eq!(south_re_snap.your_seat, Some(Seat::South));
    assert!(south_re_snap.seats[Seat::South.index()].connected);
    assert!(south_re_snap.seats[Seat::South.index()].ai_assisted);

    // East discards -> South draws while still AI-assisted, wait until AI plays or reclaim immediately.
    // Let's first test reclaiming control before discarding so South acts manually!
    send_msg(
        &mut south_re,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::South,
            enabled: false,
        },
    )
    .await;
    let reclaimed_snap = loop {
        match recv_msg(&mut south_re).await {
            ServerMessage::TableSnapshot(snap) => break snap,
            ServerMessage::LobbyUpdated { .. } => continue,
            other => panic!("expected TableSnapshot after reclaim, got {other:?}"),
        }
    };
    assert!(!reclaimed_snap.seats[Seat::South.index()].ai_assisted);
    assert_eq!(
        reclaimed_snap.seats[Seat::South.index()].ai_difficulty,
        None
    );

    // East discards -> South receives turn and acts manually.
    send_msg(&mut east, &ClientMessage::RequestSnapshot).await;
    let east_now = recv_snapshot(&mut east).await;
    let east_drawn = east_now
        .player_view
        .expect("east view")
        .hand
        .drawn_tile
        .expect("east drawn");
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: east_now.seq,
            client_action_id: "e-disc-2".to_string(),
            action: PlayerAction::Discard {
                tile: east_drawn,
                from_drawn: true,
            },
        },
    )
    .await;
    let (_, _, _, _) = recv_event_batch(&mut east).await;
    let (_, to_seq, _, south_legal2) = recv_event_batch(&mut south_re).await;
    assert!(!south_legal2.is_empty());

    send_msg(
        &mut south_re,
        &ClientMessage::SubmitAction {
            expected_seq: to_seq,
            client_action_id: "s-disc-manual".to_string(),
            action: south_legal2
                .into_iter()
                .find(|a| matches!(a, PlayerAction::Discard { .. }))
                .expect("discard legal"),
        },
    )
    .await;
    let (_, _, events, _) = recv_event_batch(&mut south_re).await;
    assert!(events.iter().any(|e| {
        matches!(
            e.kind,
            PlayerEventKind::TileDiscarded {
                seat: Seat::South,
                ..
            }
        )
    }));

    let _ = east.close(None).await;
    let _ = south_re.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn toggle_ai_assist_auto_confirms_between_hands_and_rejects_invalid_targets() {
    // East starts with a ready-to-tsumo 14-tile hand: 111m 222m 333m 444m CC
    let east_14 = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("5m 8m 9m 1s 4s 7s 1p 4p 7p E S W N");
    let wall = build_preset_wall(&east_14, &other_13, &other_13, &other_13, &[], &[])
        .expect("preset wall");
    let wall_hand2 = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall.clone(), wall_hand2.clone()],
        )))
    });
    let server = spawn_test_server(202, PacingMode::Immediate, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "bh-e", "东家").await;
    let mut south = connect_and_hello(addr, "bh-s", "南家").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;
    send_msg(
        &mut south,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let _ = recv_snapshot(&mut south).await;

    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let east_snap = recv_snapshot(&mut east).await;
    let _ = recv_snapshot(&mut south).await;

    // Invalid targets while South is online:
    // 1. East cannot enable AI assist for online South.
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::South,
            enabled: true,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut east).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // 2. East cannot toggle AI assist on an AI seat (West).
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::West,
            enabled: true,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut east).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // East wins by Tsumo -> BetweenHands (West & North AI auto-confirm).
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: east_snap.seq,
            client_action_id: "e-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    let (_, to_seq, events, _) = recv_event_batch(&mut east).await;
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, PlayerEventKind::HandSettled { .. })));
    let _ = recv_event_batch(&mut south).await;

    // South disconnects; East confirms next hand -> stays BetweenHands waiting for South.
    drop(south);
    recv_lobby_updated_matching(&mut east, |seats| !seats[Seat::South.index()].connected).await;

    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: to_seq,
            client_action_id: "e-confirm-1".to_string(),
            action: PlayerAction::ConfirmNextHand,
        },
    )
    .await;
    let (_, _, events, _) = recv_event_batch(&mut east).await;
    assert!(events.iter().any(|e| matches!(
        e.kind,
        PlayerEventKind::NextHandConfirmed { seat: Seat::East }
    )));
    assert!(!events
        .iter()
        .any(|e| matches!(e.kind, PlayerEventKind::HandStarted { .. })));

    // East enables AI assist for offline South -> South immediately auto-confirms and Hand 2 starts!
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::South,
            enabled: true,
        },
    )
    .await;
    recv_lobby_updated_matching(&mut east, |seats| seats[Seat::South.index()].ai_assisted).await;
    let (_, _, events, _) = recv_event_batch(&mut east).await;
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, PlayerEventKind::HandStarted { .. })));

    send_msg(&mut east, &ClientMessage::RequestSnapshot).await;
    let hand2_snap = recv_snapshot(&mut east).await;
    assert_eq!(
        hand2_snap.player_view.expect("view").phase,
        PublicPhase::Playing
    );

    let _ = east.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn take_over_offline_human_rotates_token_invalidates_old_device_and_reconciles_pending_op() {
    let wall = no_reaction_wall();
    let wall_hand2 = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall.clone(), wall_hand2.clone()],
        )))
    });
    let server = spawn_test_server(203, PacingMode::Immediate, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "tk-e", "东家").await;
    let mut south = connect_and_hello(addr, "tk-s", "南家").await;
    let mut west = connect_and_hello(addr, "tk-w", "西家").await;
    let mut observer = connect_and_hello(addr, "tk-obs", "跨设备").await;

    for (ws, seat) in [
        (&mut east, Seat::East),
        (&mut south, Seat::South),
        (&mut west, Seat::West),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }

    // In Lobby: confirm=false and online human takeover are rejected.
    send_msg(
        &mut observer,
        &ClientMessage::TakeOverSeat {
            seat: Seat::South,
            confirm: false,
        },
    )
    .await;
    assert!(recv_error_notice(&mut observer)
        .await
        .contains("confirm: true"));
    send_msg(
        &mut observer,
        &ClientMessage::TakeOverSeat {
            seat: Seat::South,
            confirm: true,
        },
    )
    .await;
    assert!(recv_error_notice(&mut observer).await.contains("online"));

    // Seated human cannot TakeOverSeat.
    send_msg(
        &mut east,
        &ClientMessage::TakeOverSeat {
            seat: Seat::North,
            confirm: true,
        },
    )
    .await;
    assert!(recv_error_notice(&mut east)
        .await
        .contains("already seated"));

    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let _ = recv_snapshot(&mut east).await;
    let south_snap = recv_snapshot(&mut south).await;
    let _ = recv_snapshot(&mut west).await;
    let _ = recv_snapshot(&mut observer).await;

    let table_id = south_snap.table_id.clone();
    let old_south_token = south_snap.seat_token.clone().expect("old south token");

    // South disconnects, and East enables AI assist for South, then East requests DisbandMatch.
    drop(south);
    recv_lobby_updated_matching(&mut east, |seats| !seats[Seat::South.index()].connected).await;
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::South,
            enabled: true,
        },
    )
    .await;
    recv_lobby_updated_matching(&mut east, |seats| seats[Seat::South.index()].ai_assisted).await;

    // Pending destructive op requires East and West (South is offline).
    send_msg(&mut east, &ClientMessage::RequestDisbandMatch).await;
    let east_op_snap = recv_snapshot(&mut east).await;
    let _ = recv_snapshot(&mut west).await;
    let _ = recv_snapshot(&mut observer).await;
    let op = east_op_snap
        .pending_destructive_op
        .expect("pending disband op");
    assert_eq!(op.required_seats, vec![Seat::East, Seat::West]);

    // Unseated observer takes over offline South seat with confirm=true.
    send_msg(
        &mut observer,
        &ClientMessage::TakeOverSeat {
            seat: Seat::South,
            confirm: true,
        },
    )
    .await;
    let taken_snap = recv_snapshot(&mut observer).await;
    assert_eq!(taken_snap.your_seat, Some(Seat::South));
    let new_south_token = taken_snap.seat_token.clone().expect("new south token");
    assert_ne!(new_south_token, old_south_token);
    assert!(taken_snap.seats[Seat::South.index()].connected);
    assert!(!taken_snap.seats[Seat::South.index()].ai_assisted);
    assert_eq!(taken_snap.seats[Seat::South.index()].nickname, "南家");
    let updated_op = taken_snap
        .pending_destructive_op
        .expect("pending op preserved");
    assert!(updated_op.required_seats.contains(&Seat::South));

    // Old device attempts to reconnect with old_south_token -> cannot reclaim seat!
    let mut old_south = connect_ws(addr).await;
    send_msg(
        &mut old_south,
        &ClientMessage::Hello {
            client_id: "tk-s".to_string(),
            nickname: "南家".to_string(),
            table_id: Some(table_id),
            seat_token: Some(old_south_token),
        },
    )
    .await;
    let old_re_snap = recv_snapshot(&mut old_south).await;
    assert_eq!(old_re_snap.your_seat, None);
    assert_ne!(
        old_re_snap.seat_token.as_deref(),
        Some(new_south_token.as_str())
    );

    let _ = east.close(None).await;
    let _ = west.close(None).await;
    let _ = observer.close(None).await;
    let _ = old_south.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn mid_match_hot_takeover_of_ai_seat_cancels_in_flight_ai_and_grants_immediate_control() {
    let wall = no_reaction_wall();
    let wall_hand2 = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall.clone(), wall_hand2.clone()],
        )))
    });
    // Use Humanized pacing so AI decision enters the delay window, guaranteeing an in-flight AI job!
    let server = spawn_test_server(204, PacingMode::Humanized, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "ai-take-e", "东家").await;
    let mut joiner = connect_and_hello(addr, "ai-take-j", "小红").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;

    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let east_snap = recv_snapshot(&mut east).await;
    let joiner_lobby_snap = recv_snapshot(&mut joiner).await;
    assert_eq!(joiner_lobby_snap.your_seat, None);
    assert_eq!(
        joiner_lobby_snap.seats[Seat::South.index()].kind,
        SeatKind::Ai
    );

    // East discards C -> beat 1 (seq=3), beat 2 (seq=4, South draws) arrives ~200ms later.
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: east_snap.seq,
            client_action_id: "e-disc-humanized".to_string(),
            action: PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        },
    )
    .await;
    let _ = recv_event_batch(&mut east).await; // beat 1: TileDiscarded
    let (_, draw_seq, draw_evs, _) = recv_event_batch(&mut east).await; // beat 2: TileDrawn(South)
    assert!(matches!(
        draw_evs[0].kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            ..
        }
    ));

    // Immediately while South AI is in its humanized delay window, unseated joiner takes over South!
    send_msg(
        &mut joiner,
        &ClientMessage::TakeOverSeat {
            seat: Seat::South,
            confirm: true,
        },
    )
    .await;
    let joiner_snap = recv_snapshot(&mut joiner).await;
    assert_eq!(joiner_snap.your_seat, Some(Seat::South));
    assert_eq!(joiner_snap.seq, draw_seq);
    assert_eq!(joiner_snap.seats[Seat::South.index()].kind, SeatKind::Human);
    assert_eq!(joiner_snap.seats[Seat::South.index()].nickname, "小红");
    assert!(!joiner_snap.seats[Seat::South.index()].ai_assisted);
    let joiner_view = joiner_snap.player_view.expect("immediate PlayerView");
    assert!(!joiner_view.legal_actions.is_empty());
    recv_lobby_updated_matching(&mut east, |seats| {
        seats[Seat::South.index()].kind == SeatKind::Human
            && seats[Seat::South.index()].nickname == "小红"
    })
    .await;

    // Wait longer than Standard AI think/action delay (850ms) and verify ZERO ghost moves!
    tokio::time::sleep(Duration::from_millis(950)).await;
    assert_no_pending_events_via_ping(&mut east, 91).await;
    assert_no_pending_events_via_ping(&mut joiner, 92).await;

    // Now human joiner discards manually -> South's discard is accepted!
    let action = joiner_view
        .legal_actions
        .into_iter()
        .find(|a| matches!(a, PlayerAction::Discard { .. }))
        .expect("discard action");
    send_msg(
        &mut joiner,
        &ClientMessage::SubmitAction {
            expected_seq: joiner_snap.seq,
            client_action_id: "j-manual-disc".to_string(),
            action,
        },
    )
    .await;
    let (_, _, south_disc_evs, _) = recv_event_batch(&mut joiner).await;
    assert!(south_disc_evs.iter().any(|e| matches!(
        e.kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            ..
        }
    )));

    let _ = east.close(None).await;
    let _ = joiner.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn reclaim_ai_assist_during_humanized_delay_cancels_ghost_move() {
    let wall = no_reaction_wall();
    let wall_hand2 = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall.clone(), wall_hand2.clone()],
        )))
    });
    let server = spawn_test_server(205, PacingMode::Humanized, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "rec-e", "东家").await;
    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;
    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let east_snap = recv_snapshot(&mut east).await;

    // East enables AI assist on East's own seat while it's East's turn (Humanized 650ms+ delay).
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
    )
    .await;
    recv_lobby_updated_matching(&mut east, |seats| seats[Seat::East.index()].ai_assisted).await;

    // Immediately reclaim control before AI completes its humanized delay.
    tokio::time::sleep(Duration::from_millis(50)).await;
    send_msg(
        &mut east,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: false,
        },
    )
    .await;
    let reclaimed = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableSnapshot(snap) => break snap,
            ServerMessage::LobbyUpdated { .. } => continue,
            other => panic!("expected TableSnapshot on reclaim, got {other:?}"),
        }
    };
    assert_eq!(reclaimed.seq, east_snap.seq);
    assert!(!reclaimed.seats[Seat::East.index()].ai_assisted);

    // Wait past the AI window (>1500ms after HandStarted) and verify zero ghost move happened!
    tokio::time::sleep(Duration::from_millis(1600)).await;
    assert_no_pending_events_via_ping(&mut east, 93).await;

    let _ = east.close(None).await;
    server.shutdown().await;
}

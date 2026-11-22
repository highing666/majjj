mod common;

use std::time::Duration;

use common::{connect_ws, fetch_health, recv_msg, recv_snapshot, send_msg, spawn_server, WsClient};
use futures_util::{SinkExt, StreamExt};
use mahjong_ai::AiDifficulty;
use mahjong_core::{MatchMode, PlayerAction, Seat};
use mahjong_server::protocol::{
    ClientMessage, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn recv_lobby_updated(ws: &mut WsClient) -> ServerMessage {
    loop {
        match recv_msg(ws).await {
            msg @ ServerMessage::LobbyUpdated { .. } => return msg,
            ServerMessage::TableSnapshot(_) => continue,
            other => panic!("expected LobbyUpdated, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn hello_issues_credentials_and_handles_ping_and_request_snapshot() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    let mut ws = connect_ws(addr).await;

    // Must reject actions before Hello without disconnecting.
    send_msg(&mut ws, &ClientMessage::JoinSeat { seat: None }).await;
    assert!(matches!(
        recv_msg(&mut ws).await,
        ServerMessage::ErrorNotice { .. }
    ));

    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            client_id: "device-a".to_string(),
            nickname: "  Alice ".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;

    let snap = recv_snapshot(&mut ws).await;
    assert!(!snap.table_id.is_empty());
    assert_eq!(snap.phase, TablePhase::Lobby);
    assert_eq!(snap.your_seat, None);
    let token = snap
        .seat_token
        .clone()
        .expect("Hello must issue seat_token");
    assert!(!token.is_empty());
    assert_eq!(snap.seq, 0);
    assert_eq!(snap.server_version, env!("CARGO_PKG_VERSION"));
    assert!(snap.host_is_loopback);
    assert!(snap
        .seats
        .iter()
        .all(|s| s.kind == SeatKind::Empty && !s.connected));

    // ClientPing -> ServerPong
    send_msg(&mut ws, &ClientMessage::ClientPing { ts_ms: 987654 }).await;
    assert_eq!(
        recv_msg(&mut ws).await,
        ServerMessage::ServerPong { ts_ms: 987654 }
    );

    // RequestSnapshot -> identical authoritative snapshot
    send_msg(&mut ws, &ClientMessage::RequestSnapshot).await;
    let snap2 = recv_snapshot(&mut ws).await;
    assert_eq!(snap2, snap);

    // Connection carrying non-loopback Host header (e.g. Vite LAN proxy) reports host_is_loopback = false.
    let mut lan_req = format!("ws://{addr}/ws").into_client_request().unwrap();
    lan_req
        .headers_mut()
        .insert("Host", "192.168.1.10:5173".parse().unwrap());
    let (mut ws_lan, _) = connect_async(lan_req).await.expect("connect proxied ws");
    send_msg(
        &mut ws_lan,
        &ClientMessage::Hello {
            client_id: "device-lan".to_string(),
            nickname: "Phone".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let lan_snap = recv_snapshot(&mut ws_lan).await;
    assert!(!lan_snap.host_is_loopback);
    let _ = ws_lan.close(None).await;

    let _ = ws.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn multi_client_join_leave_reconnect_and_peer_config_broadcast() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    let mut ws1 = connect_ws(addr).await;
    let mut ws2 = connect_ws(addr).await;
    let mut ws3 = connect_ws(addr).await;

    send_msg(
        &mut ws1,
        &ClientMessage::Hello {
            client_id: "c1".to_string(),
            nickname: "Alice".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let snap1 = recv_snapshot(&mut ws1).await;
    let table_id = snap1.table_id.clone();
    let token1 = snap1.seat_token.clone().unwrap();

    send_msg(
        &mut ws2,
        &ClientMessage::Hello {
            client_id: "c2".to_string(),
            nickname: "Bob".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let _snap2 = recv_snapshot(&mut ws2).await;

    send_msg(
        &mut ws3,
        &ClientMessage::Hello {
            client_id: "c3".to_string(),
            nickname: "Observer".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let _snap3 = recv_snapshot(&mut ws3).await;

    // Client 1 joins South; all clients receive LobbyUpdated and Client 1 sees your_seat = South.
    send_msg(
        &mut ws1,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let snap1_seated = recv_snapshot(&mut ws1).await;
    assert_eq!(snap1_seated.your_seat, Some(Seat::South));
    assert_eq!(
        snap1_seated.seats[Seat::South.index()].kind,
        SeatKind::Human
    );
    assert_eq!(snap1_seated.seats[Seat::South.index()].nickname, "Alice");
    assert!(snap1_seated.seats[Seat::South.index()].connected);

    let lu2 = recv_lobby_updated(&mut ws2).await;
    let lu3 = recv_lobby_updated(&mut ws3).await;
    assert_eq!(lu2, lu3);
    if let ServerMessage::LobbyUpdated { seats, .. } = lu2 {
        assert_eq!(seats[Seat::South.index()].kind, SeatKind::Human);
        assert!(seats[Seat::South.index()].connected);
    }

    // Client 2 joins first open seat (East).
    send_msg(&mut ws2, &ClientMessage::JoinSeat { seat: None }).await;
    let snap2_seated = recv_snapshot(&mut ws2).await;
    assert_eq!(snap2_seated.your_seat, Some(Seat::East));
    let _ = recv_lobby_updated(&mut ws1).await;
    let _ = recv_lobby_updated(&mut ws3).await;

    let health = fetch_health(addr).await;
    assert_eq!(health.online_humans, 2);
    assert_eq!(health.phase, TablePhase::Lobby);

    // Client 3 cannot claim occupied seat East.
    send_msg(
        &mut ws3,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut ws3).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // Unseated Client 3 cannot mutate lobby config.
    send_msg(
        &mut ws3,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Fast,
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut ws3).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // Seated Client 1 updates lobby config; all clients receive LobbyUpdated.
    send_msg(
        &mut ws1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Intermediate,
            pace: TablePace::Fast,
        },
    )
    .await;
    for client in [&mut ws1, &mut ws2, &mut ws3] {
        match recv_lobby_updated(client).await {
            ServerMessage::LobbyUpdated { config, .. } => {
                assert_eq!(config.mode, MatchMode::FreeTable);
                assert_eq!(config.seating, SeatingRule::FixedOpposite);
                assert_eq!(config.ai_difficulty, AiDifficulty::Intermediate);
                assert_eq!(config.pace, TablePace::Fast);
            }
            other => panic!("expected LobbyUpdated, got {other:?}"),
        }
    }

    // Seated Client 2 toggles pace; all clients receive LobbyUpdated.
    send_msg(
        &mut ws2,
        &ClientMessage::SetPace {
            pace: TablePace::Standard,
        },
    )
    .await;
    for client in [&mut ws1, &mut ws2, &mut ws3] {
        match recv_lobby_updated(client).await {
            ServerMessage::LobbyUpdated { config, .. } => {
                assert_eq!(config.pace, TablePace::Standard);
            }
            other => panic!("expected LobbyUpdated, got {other:?}"),
        }
    }

    // Client 2 leaves seat.
    send_msg(&mut ws2, &ClientMessage::LeaveSeat).await;
    let snap2_left = recv_snapshot(&mut ws2).await;
    assert_eq!(snap2_left.your_seat, None);
    let _ = recv_lobby_updated(&mut ws1).await;
    let _ = recv_lobby_updated(&mut ws3).await;
    assert_eq!(fetch_health(addr).await.online_humans, 1);

    // Client 1 sends ClientLeaving -> disconnects cleanly, marks South offline.
    send_msg(&mut ws1, &ClientMessage::ClientLeaving).await;
    let _ = ws1.close(None).await;
    match recv_lobby_updated(&mut ws2).await {
        ServerMessage::LobbyUpdated { seats, .. } => {
            assert_eq!(seats[Seat::South.index()].kind, SeatKind::Human);
            assert!(!seats[Seat::South.index()].connected);
        }
        other => panic!("expected LobbyUpdated on disconnect, got {other:?}"),
    }
    let _ = recv_lobby_updated(&mut ws3).await;
    assert_eq!(fetch_health(addr).await.online_humans, 0);

    // Client 1 reconnects with (table_id, seat_token) and restores Seat::South.
    let mut ws1_re = connect_ws(addr).await;
    send_msg(
        &mut ws1_re,
        &ClientMessage::Hello {
            client_id: "c1".to_string(),
            nickname: "Alice".to_string(),
            table_id: Some(table_id),
            seat_token: Some(token1),
        },
    )
    .await;
    let restored = recv_snapshot(&mut ws1_re).await;
    assert_eq!(restored.your_seat, Some(Seat::South));
    assert!(restored.seats[Seat::South.index()].connected);
    assert_eq!(restored.config.mode, MatchMode::FreeTable);
    match recv_lobby_updated(&mut ws2).await {
        ServerMessage::LobbyUpdated { seats, .. } => {
            assert!(seats[Seat::South.index()].connected);
        }
        other => panic!("expected LobbyUpdated on reconnect, got {other:?}"),
    }
    assert_eq!(fetch_health(addr).await.online_humans, 1);

    let _ = ws1_re.close(None).await;
    let _ = ws2.close(None).await;
    let _ = ws3.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn duplicate_tab_replaces_old_socket_with_session_replaced_and_keeps_seat_online() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    let mut ws_old = connect_ws(addr).await;
    send_msg(
        &mut ws_old,
        &ClientMessage::Hello {
            client_id: "same-browser".to_string(),
            nickname: "Player1".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let snap_old = recv_snapshot(&mut ws_old).await;
    let table_id = snap_old.table_id.clone();
    let seat_token = snap_old.seat_token.clone().unwrap();

    send_msg(
        &mut ws_old,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let seated_old = recv_snapshot(&mut ws_old).await;
    assert_eq!(seated_old.your_seat, Some(Seat::East));

    // Second tab connects with same (client_id, table_id, seat_token).
    let mut ws_new = connect_ws(addr).await;
    send_msg(
        &mut ws_new,
        &ClientMessage::Hello {
            client_id: "same-browser".to_string(),
            nickname: "Player1".to_string(),
            table_id: Some(table_id),
            seat_token: Some(seat_token.clone()),
        },
    )
    .await;

    // Old tab receives SessionReplaced then server closes old socket.
    assert_eq!(recv_msg(&mut ws_old).await, ServerMessage::SessionReplaced);
    let next_frame = tokio::time::timeout(Duration::from_secs(3), ws_old.next())
        .await
        .expect("old socket should close promptly");
    match next_frame {
        None | Some(Ok(WsMessage::Close(_))) => {}
        other => panic!("expected close after SessionReplaced, got {other:?}"),
    }

    // New tab receives full snapshot preserving seat ownership and online status.
    let snap_new = recv_snapshot(&mut ws_new).await;
    assert_eq!(snap_new.your_seat, Some(Seat::East));
    assert_eq!(snap_new.seat_token.as_deref(), Some(seat_token.as_str()));
    assert!(snap_new.seats[Seat::East.index()].connected);

    // Ensure old socket teardown did not race to mark Seat::East disconnected.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(fetch_health(addr).await.online_humans, 1);

    let _ = ws_new.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn reserved_and_unwired_messages_and_invalid_json_return_error_notice_without_disconnect() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    let mut ws = connect_ws(addr).await;
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            client_id: "c-err".to_string(),
            nickname: "Tester".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let _ = recv_snapshot(&mut ws).await;

    // Invalid JSON text frame
    ws.send(WsMessage::Text("{not-json".into())).await.unwrap();
    assert!(matches!(
        recv_msg(&mut ws).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // Reserved Slice 5 messages
    for msg in [
        ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
        ClientMessage::TakeOverSeat {
            seat: Seat::South,
            confirm: true,
        },
        // Ticket #26 match driver messages cleanly rejected in Ticket #25 lobby phase
        ClientMessage::StartMatch,
        ClientMessage::ReturnToLobby,
        ClientMessage::SubmitAction {
            expected_seq: 0,
            client_action_id: "a1".to_string(),
            action: PlayerAction::Pass,
        },
        ClientMessage::RequestEndFreeTable,
        ClientMessage::RequestDisbandMatch,
        ClientMessage::ConfirmDestructiveOp {
            op_id: "op1".to_string(),
            accept: true,
        },
    ] {
        send_msg(&mut ws, &msg).await;
        assert!(
            matches!(recv_msg(&mut ws).await, ServerMessage::ErrorNotice { .. }),
            "expected ErrorNotice for {msg:?}"
        );
    }

    // Connection remains alive and responsive.
    send_msg(&mut ws, &ClientMessage::ClientPing { ts_ms: 42 }).await;
    assert_eq!(
        recv_msg(&mut ws).await,
        ServerMessage::ServerPong { ts_ms: 42 }
    );

    let _ = ws.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn seat_capacity_move_seat_unseated_replacement_and_cross_client_token_isolation() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    let mut clients = Vec::new();
    for i in 0..4 {
        let mut ws = connect_ws(addr).await;
        send_msg(
            &mut ws,
            &ClientMessage::Hello {
                client_id: format!("dev-{i}"),
                nickname: format!("Player{i}"),
                table_id: None,
                seat_token: None,
            },
        )
        .await;
        let snap = recv_snapshot(&mut ws).await;
        clients.push((ws, snap.table_id, snap.seat_token.unwrap()));
    }

    // Fill all 4 seats.
    for (ws, _, _) in &mut clients {
        send_msg(ws, &ClientMessage::JoinSeat { seat: None }).await;
        let snap = recv_snapshot(ws).await;
        assert!(snap.your_seat.is_some());
    }
    assert_eq!(fetch_health(addr).await.online_humans, 4);

    // 5th visitor cannot join any seat, and cannot hijack Player0's token using a different client_id.
    let mut ws_extra = connect_ws(addr).await;
    send_msg(
        &mut ws_extra,
        &ClientMessage::Hello {
            client_id: "intruder".to_string(),
            nickname: "Intruder".to_string(),
            table_id: Some(clients[0].1.clone()),
            seat_token: Some(clients[0].2.clone()),
        },
    )
    .await;
    let extra_snap = recv_snapshot(&mut ws_extra).await;
    assert_eq!(extra_snap.your_seat, None);
    assert_ne!(
        extra_snap.seat_token.as_deref(),
        Some(clients[0].2.as_str())
    );

    send_msg(&mut ws_extra, &ClientMessage::JoinSeat { seat: None }).await;
    assert!(matches!(
        recv_msg(&mut ws_extra).await,
        ServerMessage::ErrorNotice { .. }
    ));

    // Unseated duplicate tab with matching (client_id, table_id, seat_token) replaces old unseated tab.
    let mut ws_extra_tab2 = connect_ws(addr).await;
    send_msg(
        &mut ws_extra_tab2,
        &ClientMessage::Hello {
            client_id: "intruder".to_string(),
            nickname: "Intruder".to_string(),
            table_id: Some(extra_snap.table_id.clone()),
            seat_token: extra_snap.seat_token.clone(),
        },
    )
    .await;
    assert_eq!(
        recv_msg(&mut ws_extra).await,
        ServerMessage::SessionReplaced
    );
    let snap_extra_tab2 = recv_snapshot(&mut ws_extra_tab2).await;
    assert_eq!(snap_extra_tab2.your_seat, None);

    // Seat 3 leaves, Seat 0 moves from East to North.
    send_msg(&mut clients[3].0, &ClientMessage::LeaveSeat).await;
    let left_snap = recv_snapshot(&mut clients[3].0).await;
    assert_eq!(left_snap.your_seat, None);

    send_msg(
        &mut clients[0].0,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::North),
        },
    )
    .await;
    let moved_snap = recv_snapshot(&mut clients[0].0).await;
    assert_eq!(moved_snap.your_seat, Some(Seat::North));
    assert_eq!(moved_snap.seats[Seat::East.index()].kind, SeatKind::Empty);
    assert_eq!(moved_snap.seats[Seat::North.index()].kind, SeatKind::Human);
    assert_eq!(fetch_health(addr).await.online_humans, 3);

    for (mut ws, _, _) in clients {
        let _ = ws.close(None).await;
    }
    let _ = ws_extra_tab2.close(None).await;
    server.shutdown().await;
}

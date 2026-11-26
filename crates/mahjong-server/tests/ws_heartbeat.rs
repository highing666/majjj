mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use common::{connect_and_hello, no_reaction_wall, recv_msg, recv_snapshot, send_msg, tile};
use futures_util::{SinkExt, StreamExt};
use mahjong_ai::AiDifficulty;
use mahjong_core::{MatchMode, PlayerAction, PresetWallProvider, Seat};
use mahjong_server::protocol::{
    ClientMessage, DestructiveOpKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::{HeartbeatConfig, PacingMode, ServerConfig};
use tokio_tungstenite::tungstenite::Message as WsMessage;

#[tokio::test]
async fn heartbeat_timeout_disconnects_silent_client_and_reconciles_pending_destructive_op() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(common::stub_llm_client())
        .with_heartbeat(HeartbeatConfig {
            ping_interval: Duration::from_millis(40),
            activity_timeout: Duration::from_millis(160),
        })
        .spawn_with_options(
            TableOptions::with_wall_factory(48, factory).with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server with heartbeat");
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "hb-east", "East").await;
    let mut south = connect_and_hello(addr, "hb-south", "South").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    send_msg(
        &mut south,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;

    // Drain join snapshots.
    let _ = recv_snapshot(&mut east).await;
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

    let east_start = recv_snapshot(&mut east).await;
    assert_eq!(east_start.phase, TablePhase::InMatch);
    let south_start = recv_snapshot(&mut south).await;
    assert_eq!(south_start.phase, TablePhase::InMatch);

    // East requests DisbandMatch while South is still online -> pending op requires [East, South].
    send_msg(&mut east, &ClientMessage::RequestDisbandMatch).await;
    let pending_snap = recv_snapshot(&mut east).await;
    let op = pending_snap
        .pending_destructive_op
        .expect("pending disband op while South is connected");
    assert_eq!(op.kind, DestructiveOpKind::DisbandMatch);
    assert!(op.required_seats.contains(&Seat::East));
    assert!(op.required_seats.contains(&Seat::South));

    // Keep East alive with ClientPing while South sends nothing at all.
    let mut saw_server_ping = false;
    let mut saw_south_offline = false;
    let mut saw_disbanded_to_lobby = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut keepalive = tokio::time::interval(Duration::from_millis(50));
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    while tokio::time::Instant::now() < deadline && !(saw_south_offline && saw_disbanded_to_lobby) {
        tokio::select! {
            _ = keepalive.tick() => {
                send_msg(&mut east, &ClientMessage::ClientPing { ts_ms: 1 }).await;
            }
            frame = east.next() => {
                let msg = match frame {
                    Some(Ok(WsMessage::Text(text))) => {
                        serde_json::from_str::<ServerMessage>(&text).expect("valid ServerMessage")
                    }
                    other => panic!("east expected active text frame, got {other:?}"),
                };
                match msg {
                    ServerMessage::ServerPing { .. } => {
                        saw_server_ping = true;
                    }
                    ServerMessage::LobbyUpdated { seats, .. } => {
                        if !seats[Seat::South.index()].connected {
                            saw_south_offline = true;
                        }
                    }
                    ServerMessage::TableSnapshot(snap) => {
                        if snap.phase == TablePhase::Lobby && snap.pending_destructive_op.is_none() {
                            saw_disbanded_to_lobby = true;
                            if !snap.seats[Seat::South.index()].connected {
                                saw_south_offline = true;
                            }
                        }
                    }
                    ServerMessage::ServerPong { .. } => {}
                    _ => {}
                }
            }
        }
    }

    assert!(saw_server_ping, "East should receive periodic ServerPing");
    assert!(
        saw_south_offline,
        "South should be marked connected=false after heartbeat timeout"
    );
    assert!(
        saw_disbanded_to_lobby,
        "PendingDestructiveOp should resolve automatically when South times out"
    );

    // Silent South socket should be closed by the server.
    let south_closed = tokio::time::timeout(Duration::from_millis(500), async {
        while let Some(frame) = south.next().await {
            if matches!(frame, Ok(WsMessage::Close(_)) | Err(_)) {
                return true;
            }
        }
        true
    })
    .await
    .expect("South socket should close on timeout");
    assert!(south_closed);

    server.shutdown().await;
}

#[tokio::test]
async fn valid_inbound_frames_refresh_liveness_while_invalid_frames_do_not() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(common::stub_llm_client())
        .with_heartbeat(HeartbeatConfig {
            ping_interval: Duration::from_millis(35),
            activity_timeout: Duration::from_millis(120),
        })
        .spawn_with_options(
            TableOptions::with_wall_factory(49, factory).with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server");
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "hb-active", "East").await;
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
    let snap = recv_snapshot(&mut east).await;
    assert_eq!(snap.phase, TablePhase::InMatch);

    // Total elapsed > 2x activity_timeout (120ms): keep alive via ClientPing, WS Pong, and SubmitAction.
    tokio::time::sleep(Duration::from_millis(70)).await;
    send_msg(&mut east, &ClientMessage::ClientPing { ts_ms: 42 }).await;

    tokio::time::sleep(Duration::from_millis(70)).await;
    east.send(WsMessage::Pong(Vec::new().into()))
        .await
        .expect("send ws pong");

    tokio::time::sleep(Duration::from_millis(70)).await;
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: snap.seq,
            client_action_id: "hb-disc-1".to_string(),
            action: PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        },
    )
    .await;

    let mut saw_pong = false;
    let mut saw_event_batch = false;
    let check_deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while tokio::time::Instant::now() < check_deadline && !(saw_pong && saw_event_batch) {
        match recv_msg(&mut east).await {
            ServerMessage::ServerPong { ts_ms: 42 } => saw_pong = true,
            ServerMessage::EventBatch { .. } => saw_event_batch = true,
            ServerMessage::ServerPing { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::TableBanter { .. } => {}
            other => panic!("unexpected message on active connection: {other:?}"),
        }
    }
    assert!(saw_pong, "should receive ServerPong for ClientPing");
    assert!(
        saw_event_batch,
        "should receive EventBatch for SubmitAction without being kicked"
    );

    // Invalid JSON and binary frames must NOT reset the liveness timeout.
    let mut bad_client = connect_and_hello(addr, "hb-bad", "Bad").await;
    tokio::time::sleep(Duration::from_millis(55)).await;
    bad_client
        .send(WsMessage::Text("not valid json".into()))
        .await
        .expect("send invalid text");
    tokio::time::sleep(Duration::from_millis(35)).await;
    bad_client
        .send(WsMessage::Binary(vec![1, 2, 3].into()))
        .await
        .expect("send binary");

    let closed_timely = tokio::time::timeout(Duration::from_millis(120), async {
        while let Some(frame) = bad_client.next().await {
            if matches!(frame, Ok(WsMessage::Close(_)) | Err(_)) {
                return true;
            }
        }
        true
    })
    .await
    .expect("connection sending only invalid frames should still time out on schedule");
    assert!(closed_timely);

    server.shutdown().await;
}

#[test]
fn heartbeat_defaults_match_production_10s_ping_and_25s_timeout() {
    let cfg = ServerConfig::default();
    assert_eq!(cfg.heartbeat.ping_interval, Duration::from_secs(10));
    assert_eq!(cfg.heartbeat.activity_timeout, Duration::from_secs(25));
}

mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{
    connect_and_hello, no_reaction_wall, recv_event_batch, recv_msg, recv_snapshot, send_msg, tile,
    tiles,
};
use mahjong_ai::{AiDifficulty, DecisionBudget};
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, PublicPhase,
    Seat,
};
use mahjong_server::protocol::{
    ClientMessage, LobbyConfig, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::ServerConfig;

async fn spawn_humanized_server(
    seed: u64,
    factory: WallProviderFactory,
) -> mahjong_server::ServerHandle {
    ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir: None,
        ai_threads: 2,
    }
    .spawn_with_options(TableOptions::with_wall_factory(seed, factory))
    .await
    .expect("spawn humanized server")
}

#[tokio::test]
async fn real_ws_two_beat_unclaimed_discard_set_pace_snapshot_and_disband_safety() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let server = spawn_humanized_server(100, factory).await;
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "ws-e", "East").await;
    let mut south = connect_and_hello(addr, "ws-s", "South").await;
    let mut west = connect_and_hello(addr, "ws-w", "West").await;
    let mut north = connect_and_hello(addr, "ws-n", "North").await;

    for (ws, seat) in [
        (&mut east, Seat::East),
        (&mut south, Seat::South),
        (&mut west, Seat::West),
        (&mut north, Seat::North),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }

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
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let snap = recv_snapshot(ws).await;
        assert_eq!(snap.phase, TablePhase::InMatch);
        assert_eq!(snap.seq, 2);
    }

    // 1. Standard pace unclaimed discard: beat 1 at ~0ms, beat 2 (TileDrawn) at ~200ms.
    let t0 = Instant::now();
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: 2,
            client_action_id: "ws-disc-1".to_string(),
            action: PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            },
        },
    )
    .await;

    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let (from_seq, to_seq, evs, legal) = recv_event_batch(ws).await;
        assert_eq!((from_seq, to_seq), (3, 3));
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0].kind, PlayerEventKind::TileDiscarded { .. }));
        assert!(legal.is_empty());
    }

    // Mid-window RequestSnapshot & premature SubmitAction reflect seq=3 with no TileDrawn leak.
    send_msg(&mut south, &ClientMessage::RequestSnapshot).await;
    let mid_snap = recv_snapshot(&mut south).await;
    assert_eq!(mid_snap.seq, 3);
    let mid_view = mid_snap.player_view.expect("south mid view");
    assert_eq!(mid_view.phase, PublicPhase::Playing);
    assert_eq!(mid_view.hand.drawn_tile, None);
    assert!(mid_view.legal_actions.is_empty());

    send_msg(
        &mut south,
        &ClientMessage::SubmitAction {
            expected_seq: 3,
            client_action_id: "premature-south".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    assert!(matches!(
        recv_msg(&mut south).await,
        ServerMessage::ActionRejected { current_seq: 3, .. }
    ));

    // Wait for beat 2 on all 4 sockets.
    for (idx, ws) in [&mut east, &mut south, &mut west, &mut north]
        .into_iter()
        .enumerate()
    {
        let (from_seq, to_seq, evs, legal) = recv_event_batch(ws).await;
        assert_eq!((from_seq, to_seq), (4, 4));
        assert!(matches!(
            evs[0].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::South,
                ..
            }
        ));
        assert_eq!(!legal.is_empty(), idx == Seat::South.index());
    }
    let standard_elapsed = t0.elapsed();
    assert!(
        standard_elapsed >= Duration::from_millis(160)
            && standard_elapsed <= Duration::from_millis(450),
        "Standard two-beat elapsed {standard_elapsed:?} out of expected range"
    );

    // 2. Switch live to Fast -> next discard releases around 120ms, then disband & rematch safely.
    send_msg(
        &mut east,
        &ClientMessage::SetPace {
            pace: TablePace::Fast,
        },
    )
    .await;
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        loop {
            if let ServerMessage::LobbyUpdated {
                config:
                    LobbyConfig {
                        pace: TablePace::Fast,
                        ..
                    },
                ..
            } = recv_msg(ws).await
            {
                break;
            }
        }
    }

    send_msg(&mut south, &ClientMessage::RequestSnapshot).await;
    let south_snap = recv_snapshot(&mut south).await;
    let drawn = south_snap
        .player_view
        .expect("south view")
        .hand
        .drawn_tile
        .expect("south drawn");
    let t_fast = Instant::now();
    send_msg(
        &mut south,
        &ClientMessage::SubmitAction {
            expected_seq: 4,
            client_action_id: "ws-disc-2".to_string(),
            action: PlayerAction::Discard {
                tile: drawn,
                from_drawn: true,
            },
        },
    )
    .await;

    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let (from_seq, to_seq, evs, legal) = recv_event_batch(ws).await;
        assert_eq!((from_seq, to_seq), (5, 5));
        assert!(matches!(evs[0].kind, PlayerEventKind::TileDiscarded { .. }));
        assert!(legal.is_empty());
    }
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let (from_seq, to_seq, evs, _) = recv_event_batch(ws).await;
        assert_eq!((from_seq, to_seq), (6, 6));
        assert!(matches!(
            evs[0].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::West,
                ..
            }
        ));
    }
    let fast_elapsed = t_fast.elapsed();
    assert!(
        fast_elapsed >= Duration::from_millis(85) && fast_elapsed <= Duration::from_millis(320),
        "Fast two-beat elapsed {fast_elapsed:?} out of expected range"
    );

    // 3. Trigger a pending release and immediately disband + rematch -> zero stale leak.
    send_msg(&mut west, &ClientMessage::RequestSnapshot).await;
    let west_snap = recv_snapshot(&mut west).await;
    let west_drawn = west_snap
        .player_view
        .expect("west view")
        .hand
        .drawn_tile
        .expect("west drawn");
    send_msg(
        &mut west,
        &ClientMessage::SubmitAction {
            expected_seq: 6,
            client_action_id: "ws-disc-3".to_string(),
            action: PlayerAction::Discard {
                tile: west_drawn,
                from_drawn: true,
            },
        },
    )
    .await;
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let _ = recv_event_batch(ws).await;
    }

    send_msg(&mut east, &ClientMessage::RequestDisbandMatch).await;
    let mut op_id = None;
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let s = recv_snapshot(ws).await;
        op_id = s.pending_destructive_op.map(|op| op.op_id);
    }
    let op_id = op_id.expect("disband op");
    for ws in [&mut south, &mut west, &mut north] {
        send_msg(
            ws,
            &ClientMessage::ConfirmDestructiveOp {
                op_id: op_id.clone(),
                accept: true,
            },
        )
        .await;
    }
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        loop {
            if recv_snapshot(ws).await.phase == TablePhase::Lobby {
                break;
            }
        }
    }

    send_msg(&mut east, &ClientMessage::StartMatch).await;
    for ws in [&mut east, &mut south, &mut west, &mut north] {
        let s = recv_snapshot(ws).await;
        assert_eq!(s.phase, TablePhase::InMatch);
        assert_eq!(s.seq, 2);
    }

    tokio::time::sleep(Duration::from_millis(250)).await;
    send_msg(&mut east, &ClientMessage::ClientPing { ts_ms: 777 }).await;
    assert_eq!(
        recv_msg(&mut east).await,
        ServerMessage::ServerPong { ts_ms: 777 }
    );

    for mut c in [east, south, west, north] {
        let _ = c.close(None).await;
    }
    server.shutdown().await;
}

#[tokio::test]
async fn real_ws_ai_only_all_pass_alignment_and_realtime_overlapped_thinking() {
    // South AI holds 0-shanten SevenPairs so discarding 1s opens an AI-only reaction window
    // where South AI passes, releasing South's TileDrawn aligned to ~200ms (Standard),
    // and then South AI's Realtime turn decision overlaps with its humanized window.
    let wall = build_preset_wall(
        &tiles("1s 1m 4m 7m 2s 4s 7s 1p 4p 7p E S W N"),
        &tiles("1s 1s 2s 2s 3m 3m 4m 4m 5p 5p 6p 6p 7p"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p E S W N"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p C F B E"),
        &tiles("3p 9m 9m 9m"),
        &[],
    )
    .expect("valid wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    // Explicitly test default Realtime budget + Humanized pacing over real WebSocket.
    let server = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir: None,
        ai_threads: 2,
    }
    .spawn_with_options(
        TableOptions::with_wall_factory(101, factory).with_ai_budget(DecisionBudget::Realtime {
            max_samples: 128,
            soft_ms: 120,
            hard_ms: 180,
        }),
    )
    .await
    .expect("spawn realtime humanized server");
    let addr = server.addr();

    let mut east = connect_and_hello(addr, "rt-e", "East").await;
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

    let discard_at = Instant::now();
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: snap.seq,
            client_action_id: "rt-disc-1s".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1s"),
                from_drawn: false,
            },
        },
    )
    .await;

    let (f1, t1, ev1, legal1) = recv_event_batch(&mut east).await;
    assert_eq!((f1, t1), (3, 3));
    assert!(matches!(ev1[0].kind, PlayerEventKind::TileDiscarded { .. }));
    assert!(legal1.is_empty());

    let (f2, t2, ev2, _) = recv_event_batch(&mut east).await;
    assert_eq!((f2, t2), (4, 4));
    assert!(matches!(
        ev2[0].kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            ..
        }
    ));
    let pass_release_elapsed = discard_at.elapsed();
    assert!(
        pass_release_elapsed >= Duration::from_millis(160)
            && pass_release_elapsed <= Duration::from_millis(450),
        "AI-only Pass alignment elapsed {pass_release_elapsed:?} out of expected range"
    );

    // Wait for South AI's turn action (overlapped with Realtime budget <= 180ms hard cap):
    // total time from TileDrawn should stay inside [350ms..1100ms] (overlapped, NOT think+delay stacked).
    let draw_at = Instant::now();
    let (f3, t3, ev3, _) = recv_event_batch(&mut east).await;
    assert_eq!((f3, t3), (5, 5));
    assert!(matches!(
        ev3[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            ..
        }
    ));
    let ai_turn_elapsed = draw_at.elapsed();
    assert!(
        ai_turn_elapsed >= Duration::from_millis(320)
            && ai_turn_elapsed <= Duration::from_millis(1100),
        "Overlapped AI turn elapsed {ai_turn_elapsed:?} out of expected range"
    );

    let _ = east.close(None).await;
    server.shutdown().await;
}

mod common;

use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use common::{
    connect_and_hello, http_get, http_post_raw, recv_event_batch, recv_snapshot, send_msg, tile,
    tiles, TempDirGuard, WsClient,
};
use hyper::header::CACHE_CONTROL;
use hyper::StatusCode;
use mahjong_ai::AiDifficulty;
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, PublicPhase,
    Seat,
};
use mahjong_server::history_dto::{
    ClearHistoryResponse, HandRecord, HistoryExportResponse, MatchRecord, PreferencesData,
    StatsResponse,
};
use mahjong_server::protocol::{
    ClientMessage, LobbyConfig, SeatKind, SeatingRule, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions};
use mahjong_server::{PacingMode, ServerConfig, ServerHandle};

async fn spawn_history_server(
    data_dir: PathBuf,
    seed: u64,
    walls: Vec<Vec<mahjong_core::Tile>>,
) -> ServerHandle {
    let factory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(Seat::East, walls.clone())))
    });
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(data_dir)
        .spawn_with_options(
            TableOptions::with_wall_factory(seed, factory).with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn history server")
}

fn east_tsumo_wall() -> Vec<mahjong_core::Tile> {
    let east_14 = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    build_preset_wall(&east_14, &other_13, &other_13, &other_13, &[], &[]).expect("tsumo wall")
}

fn dealer_discards_c_seat1_wins_ron_wall() -> Vec<mahjong_core::Tile> {
    // Dealer starts with 14 tiles and discards C; seat1 (dealer.next()) waits on C to Ron.
    let dealer_14 = tiles("258m 147s 147p E S W N C");
    let seat1_13 = tiles("111m 222m 333m 444m C");
    let other_13 = tiles("6789m 258s 258p E S W");
    build_preset_wall(&dealer_14, &seat1_13, &other_13, &other_13, &[], &[]).expect("ron wall")
}

async fn wait_for_hand_settled(ws: &mut WsClient) -> u64 {
    loop {
        let (_, to_seq, events, _) = recv_event_batch(ws).await;
        if events
            .iter()
            .any(|e| matches!(e.kind, PlayerEventKind::HandSettled { .. }))
        {
            return to_seq;
        }
    }
}

#[tokio::test]
async fn between_hands_persists_hands_free_table_persists_matches_and_restart_recovers_with_truncated_tail(
) {
    let temp = TempDirGuard::new("majjj_history_free_table");
    let data_dir = temp.path().join("sub").join("data");

    let server = spawn_history_server(data_dir.clone(), 11, vec![east_tsumo_wall()]).await;
    let addr = server.addr();

    let mut ws = connect_and_hello(addr, "c1", "  Alice  ").await;
    send_msg(
        &mut ws,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut ws).await;

    let desired_config = LobbyConfig {
        mode: MatchMode::FreeTable,
        seating: SeatingRule::FixedAdjacent,
        ai_difficulty: AiDifficulty::Intermediate,
        pace: TablePace::Fast,
    };
    send_msg(
        &mut ws,
        &ClientMessage::UpdateLobbyConfig {
            mode: desired_config.mode,
            seating: desired_config.seating,
            ai_difficulty: desired_config.ai_difficulty,
            pace: desired_config.pace,
        },
    )
    .await;
    send_msg(&mut ws, &ClientMessage::StartMatch).await;
    let snap = recv_snapshot(&mut ws).await;
    assert_eq!(snap.phase, TablePhase::InMatch);

    // Enable then disable AI assist on East during Hand 1 so assisted_by_ai latches true.
    send_msg(
        &mut ws,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
    )
    .await;
    send_msg(
        &mut ws,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: false,
        },
    )
    .await;
    let snap_reclaimed = recv_snapshot(&mut ws).await;

    send_msg(
        &mut ws,
        &ClientMessage::SubmitAction {
            expected_seq: snap_reclaimed.seq,
            client_action_id: "tsumo-1".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    let _ = wait_for_hand_settled(&mut ws).await;

    // Immediately on BetweenHands, hands.jsonl must already contain 1 line on disk!
    let hands_path = data_dir.join("hands.jsonl");
    let matches_path = data_dir.join("matches.jsonl");
    let hands_raw = fs::read_to_string(&hands_path).expect("hands.jsonl created on BetweenHands");
    let hand_lines: Vec<&str> = hands_raw.lines().collect();
    assert_eq!(hand_lines.len(), 1);
    let persisted_hand: HandRecord =
        serde_json::from_str(hand_lines[0]).expect("valid HandRecord JSONL");
    assert_eq!(persisted_hand.seats[0].nickname, "Alice");
    assert_eq!(persisted_hand.seats[0].seat_kind, SeatKind::Human);
    assert!(persisted_hand.seats[0].assisted_by_ai);
    assert!(!persisted_hand.aborted);
    assert!(!matches_path.exists(), "FreeTable not ended yet");

    // End FreeTable cleanly after 1 settled hand -> writes matches.jsonl.
    send_msg(&mut ws, &ClientMessage::RequestEndFreeTable).await;
    let (_, _, _, _) = recv_event_batch(&mut ws).await;
    let finished_snap = recv_snapshot(&mut ws).await;
    assert_eq!(finished_snap.phase, TablePhase::Finished);

    let matches_raw =
        fs::read_to_string(&matches_path).expect("matches.jsonl written on EndFreeTable");
    let match_lines: Vec<&str> = matches_raw.lines().collect();
    assert_eq!(match_lines.len(), 1);
    let persisted_match: MatchRecord =
        serde_json::from_str(match_lines[0]).expect("valid MatchRecord JSONL");
    assert_eq!(persisted_match.hands_played, 1);
    assert_eq!(persisted_match.seats[0].nickname, "Alice");
    assert!(persisted_match.seats[0].assisted_by_ai);

    drop(ws);
    server.shutdown().await;

    // Simulate power loss leaving truncated trailing lines in both JSONL files.
    for path in [&hands_path, &matches_path] {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open for truncated append");
        f.write_all(br#"{"schema_version":1,"match_id":"partial"#)
            .expect("write partial line");
    }

    // Restart server on the same data_dir.
    let restarted = spawn_history_server(data_dir.clone(), 12, vec![east_tsumo_wall()]).await;
    let addr2 = restarted.addr();

    // LobbyConfig and recent_nicknames restored from preferences.json.
    let ws2 = connect_ws_and_hello_snap(addr2, "c2", "Bob").await;
    assert_eq!(ws2.1.config, desired_config);
    drop(ws2.0);

    // GET /api/stats (no-store)
    let (status, headers, body) = http_get(addr2, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let stats: StatsResponse = serde_json::from_slice(&body).expect("StatsResponse JSON");
    assert_eq!(stats.total_matches, 1);
    assert_eq!(stats.total_hands, 1);
    let alice = stats
        .players
        .iter()
        .find(|p| p.nickname == "Alice")
        .expect("Alice stats");
    assert_eq!(alice.completed_matches, 1);
    assert_eq!(alice.settled_hands, 1);
    assert_eq!(alice.wins, 1);
    assert_eq!(alice.tsumo_wins, 1);
    assert_eq!(alice.ai_assisted_hands, 1);

    // GET /api/history/export (no-store)
    let (status, headers, body) = http_get(addr2, "/api/history/export").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let export: HistoryExportResponse =
        serde_json::from_slice(&body).expect("HistoryExportResponse JSON");
    assert_eq!(export.matches.len(), 1);
    assert_eq!(export.hands.len(), 1);
    assert_eq!(export.preferences.default_mode, MatchMode::FreeTable);
    assert!(export
        .preferences
        .recent_nicknames
        .contains(&"Alice".to_string()));

    // GET /api/preferences (no-store)
    let (status, headers, body) = http_get(addr2, "/api/preferences").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let prefs: PreferencesData = serde_json::from_slice(&body).expect("PreferencesData JSON");
    assert_eq!(prefs.default_mode, MatchMode::FreeTable);
    assert!(prefs.recent_nicknames.contains(&"Alice".to_string()));

    // POST /api/history/clear rejects missing or false confirm with 400 Bad Request + no-store.
    for bad_body in [
        None,
        Some(br#"{"confirm":false}"#.as_slice()),
        Some(br#"{}"#.as_slice()),
    ] {
        let (status, headers, _) = http_post_raw(addr2, "/api/history/clear", bad_body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
    }

    // Confirmed clear empties hands & matches, preserves preferences.json.
    let (status, headers, body) =
        http_post_raw(addr2, "/api/history/clear", Some(br#"{"confirm":true}"#)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let clear_resp: ClearHistoryResponse =
        serde_json::from_slice(&body).expect("ClearHistoryResponse JSON");
    assert!(clear_resp.cleared);
    assert_eq!(clear_resp.cleared_matches, 1);
    assert_eq!(clear_resp.cleared_hands, 1);

    let (_, _, stats_after_body) = http_get(addr2, "/api/stats").await;
    let stats_after: StatsResponse =
        serde_json::from_slice(&stats_after_body).expect("StatsResponse after clear");
    assert_eq!(stats_after.total_matches, 0);
    assert_eq!(stats_after.total_hands, 0);

    let (_, _, export_after_body) = http_get(addr2, "/api/history/export").await;
    let export_after: HistoryExportResponse =
        serde_json::from_slice(&export_after_body).expect("Export after clear");
    assert_eq!(export_after.preferences.default_mode, MatchMode::FreeTable);

    restarted.shutdown().await;
}

async fn connect_ws_and_hello_snap(
    addr: SocketAddr,
    client_id: &str,
    nickname: &str,
) -> (WsClient, mahjong_server::protocol::TableSnapshot) {
    let mut ws = common::connect_ws(addr).await;
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            client_id: client_id.to_string(),
            nickname: nickname.to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let snap = recv_snapshot(&mut ws).await;
    (ws, snap)
}

#[tokio::test]
async fn disbanded_east_only_marks_settled_hands_aborted_and_skips_matches_jsonl() {
    let temp = TempDirGuard::new("majjj_history_abort");
    let data_dir = temp.path().join("data");
    let server = spawn_history_server(
        data_dir.clone(),
        21,
        vec![east_tsumo_wall(), east_tsumo_wall()],
    )
    .await;
    let addr = server.addr();

    let mut ws = connect_and_hello(addr, "c1", "Alice").await;
    send_msg(
        &mut ws,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut ws).await;
    send_msg(
        &mut ws,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut ws, &ClientMessage::StartMatch).await;
    let snap = recv_snapshot(&mut ws).await;

    // Settle Hand 1.
    send_msg(
        &mut ws,
        &ClientMessage::SubmitAction {
            expected_seq: snap.seq,
            client_action_id: "h1-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    let seq_bh = wait_for_hand_settled(&mut ws).await;

    // Advance to Hand 2, then disband mid-match.
    send_msg(
        &mut ws,
        &ClientMessage::SubmitAction {
            expected_seq: seq_bh,
            client_action_id: "h1-confirm".to_string(),
            action: PlayerAction::ConfirmNextHand,
        },
    )
    .await;
    loop {
        let (_, _, events, _) = recv_event_batch(&mut ws).await;
        if events
            .iter()
            .any(|e| matches!(e.kind, PlayerEventKind::HandStarted { .. }))
        {
            break;
        }
    }

    send_msg(&mut ws, &ClientMessage::RequestDisbandMatch).await;
    let lobby_snap = recv_snapshot(&mut ws).await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);

    // hands.jsonl retains Hand 1 with aborted: true; matches.jsonl does not exist / is empty.
    let hands_raw = fs::read_to_string(data_dir.join("hands.jsonl")).expect("hands.jsonl exists");
    let lines: Vec<&str> = hands_raw.lines().collect();
    assert_eq!(lines.len(), 1);
    let rec: HandRecord = serde_json::from_str(lines[0]).expect("valid HandRecord");
    assert!(rec.aborted);
    assert!(!data_dir.join("matches.jsonl").exists());

    let (_, _, stats_body) = http_get(addr, "/api/stats").await;
    let stats: StatsResponse = serde_json::from_slice(&stats_body).expect("StatsResponse");
    assert_eq!(stats.total_matches, 0);
    assert_eq!(stats.total_hands, 1);
    assert_eq!(stats.players[0].completed_matches, 0);
    assert_eq!(stats.players[0].settled_hands, 1);
    assert_eq!(stats.players[0].aborted_hands, 1);
    assert!(stats.recent_hands[0].aborted);

    drop(ws);
    server.shutdown().await;
}

#[tokio::test]
async fn completed_east_only_and_mid_hand_ai_takeover_attribute_from_takeover_hand_onward() {
    let temp = TempDirGuard::new("majjj_history_takeover");
    let data_dir = temp.path().join("data");
    let walls = vec![
        dealer_discards_c_seat1_wins_ron_wall(),
        dealer_discards_c_seat1_wins_ron_wall(),
        dealer_discards_c_seat1_wins_ron_wall(),
        dealer_discards_c_seat1_wins_ron_wall(),
    ];
    let server = spawn_history_server(data_dir.clone(), 31, walls).await;
    let addr = server.addr();

    let mut c1 = connect_and_hello(addr, "c1", "Alice").await;
    let mut c2 = connect_and_hello(addr, "c2", "Bob").await;
    let mut c3 = connect_and_hello(addr, "c3", "Carol").await;
    let mut c4 = connect_and_hello(addr, "c4", "Dave").await;

    for (ws, seat) in [
        (&mut c1, Seat::East),
        (&mut c2, Seat::South),
        (&mut c3, Seat::West),
    ] {
        send_msg(ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(ws).await;
    }
    send_msg(
        &mut c1,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut c1, &ClientMessage::StartMatch).await;
    let s1 = recv_snapshot(&mut c1).await;
    let _ = recv_snapshot(&mut c2).await;
    let _ = recv_snapshot(&mut c3).await;
    let _ = recv_snapshot(&mut c4).await;
    assert_eq!(s1.seats[Seat::North.index()].kind, SeatKind::Ai);

    // Helper to play one hand where `dealer_ws` discards C and `winner_ws` wins Ron.
    async fn play_ron_hand(
        all: &mut [&mut WsClient; 4],
        seated_indices: &[usize],
        dealer_idx: usize,
        winner_idx: usize,
        seq_start: u64,
        tag: &str,
    ) -> u64 {
        send_msg(
            all[dealer_idx],
            &ClientMessage::SubmitAction {
                expected_seq: seq_start,
                client_action_id: format!("{tag}-disc"),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
        let mut seq_after_disc = seq_start;
        for &idx in seated_indices {
            let (_, to_seq, _, _) = recv_event_batch(all[idx]).await;
            seq_after_disc = to_seq;
        }
        send_msg(
            all[winner_idx],
            &ClientMessage::SubmitAction {
                expected_seq: seq_after_disc,
                client_action_id: format!("{tag}-ron"),
                action: PlayerAction::Ron { tile: tile("C") },
            },
        )
        .await;
        let mut seq_settled = seq_after_disc;
        for &idx in seated_indices {
            seq_settled = wait_for_hand_settled(all[idx]).await;
        }
        seq_settled
    }

    async fn confirm_all(
        all: &mut [&mut WsClient; 4],
        seated_indices: &[usize],
        mut seq_cur: u64,
        tag: &str,
    ) -> u64 {
        for &idx in seated_indices {
            send_msg(
                all[idx],
                &ClientMessage::SubmitAction {
                    expected_seq: seq_cur,
                    client_action_id: format!("{tag}-ok-{idx}"),
                    action: PlayerAction::ConfirmNextHand,
                },
            )
            .await;
            for &recv_idx in seated_indices {
                let (_, to_seq, _, _) = recv_event_batch(all[recv_idx]).await;
                seq_cur = to_seq;
            }
        }
        seq_cur
    }

    let mut all = [&mut c1, &mut c2, &mut c3, &mut c4];
    // Hand 1: East (Alice) discards C, South (Bob) wins Ron; North is AI 1.
    let seq1 = play_ron_hand(&mut all, &[0, 1, 2], 0, 1, s1.seq, "h1").await;
    let seq2_start = confirm_all(&mut all, &[0, 1, 2], seq1, "h1").await;

    // Hand 2: South (Bob) discards C, West (Carol) wins Ron; North is still AI 1.
    let seq2 = play_ron_hand(&mut all, &[0, 1, 2], 1, 2, seq2_start, "h2").await;
    let seq3_start = confirm_all(&mut all, &[0, 1, 2], seq2, "h2").await;

    // Hand 3: Unseated Dave takes over North AI seat mid-hand before West discards C!
    send_msg(
        all[3],
        &ClientMessage::TakeOverSeat {
            seat: Seat::North,
            confirm: true,
        },
    )
    .await;
    let dave_snap = recv_snapshot(all[3]).await;
    assert_eq!(dave_snap.your_seat, Some(Seat::North));
    assert_eq!(
        dave_snap.player_view.expect("view").phase,
        PublicPhase::Playing
    );

    let seq3 = play_ron_hand(&mut all, &[0, 1, 2, 3], 2, 3, seq3_start, "h3").await;
    let seq4_start = confirm_all(&mut all, &[0, 1, 2, 3], seq3, "h3").await;

    // Hand 4: North (Dave) discards C, East (Alice) wins Ron -> EastOnly finishes!
    let _ = play_ron_hand(&mut all, &[0, 1, 2, 3], 3, 0, seq4_start, "h4").await;
    for ws in all.iter_mut() {
        let snap = recv_snapshot(ws).await;
        assert_eq!(snap.phase, TablePhase::Finished);
    }

    let (_, _, stats_body) = http_get(addr, "/api/stats").await;
    let stats: StatsResponse = serde_json::from_slice(&stats_body).expect("StatsResponse");
    assert_eq!(stats.total_matches, 1);
    assert_eq!(stats.total_hands, 4);

    // Dave took over on Hand 3: Hands 1-2 belong to AI 1, Hands 3-4 belong to Dave!
    let dave = stats
        .players
        .iter()
        .find(|p| p.nickname == "Dave")
        .expect("Dave stats");
    assert_eq!(dave.completed_matches, 1);
    assert_eq!(dave.settled_hands, 2);
    assert_eq!(dave.wins, 1);
    assert_eq!(dave.deal_ins, 1);

    let alice = stats
        .players
        .iter()
        .find(|p| p.nickname == "Alice")
        .expect("Alice stats");
    assert_eq!(alice.completed_matches, 1);
    assert_eq!(alice.settled_hands, 4);
    assert_eq!(alice.wins, 1);
    assert_eq!(alice.deal_ins, 1);

    server.shutdown().await;
}

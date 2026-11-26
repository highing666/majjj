mod common;

use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use common::{
    connect_and_hello, http_get, http_post_raw, recv_event_batch, recv_snapshot, send_msg, tiles,
    ScriptedLlmClient, TempDirGuard, WsClient,
};
use hyper::header::CACHE_CONTROL;
use hyper::StatusCode;
use mahjong_ai::AiDifficulty;
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, Seat, Tile,
};
use mahjong_server::llm::{BanterGenerationRequest, BanterOutput};
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::persona_memory::{
    ClearPersonaMemoryResponse, PersonaMemoryNoteTrigger, PersonaMemoryResponse,
    PersonaMemoryStore, MAX_GRUDGE_NOTES, PERSONA_MEMORY_SCHEMA_VERSION,
};
use mahjong_server::protocol::{ClientMessage, SeatingRule, TablePace, TablePhase};
use mahjong_server::session::{BoxedWallProvider, TableOptions};
use mahjong_server::{PacingMode, ServerConfig, ServerHandle};

fn recording_memory_llm() -> Arc<ScriptedLlmClient> {
    let note_seq = Arc::new(AtomicU64::new(0));
    ScriptedLlmClient::with_handlers(
        |_| {
            Ok(BanterOutput {
                mood: PersonaMood::Mocking,
                text: "上回的账我都给你记在小本本上了！".to_string(),
            })
        },
        move |request| {
            let idx = note_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let is_wang = request.user_prompt.contains("「小王」");
            let is_li = request.user_prompt.contains("「老李」");
            let note = if is_wang {
                format!("小王大番账#{idx}：回头必讨回来")
            } else if is_li {
                format!("老李秘密恩怨#{idx}：绝不能外传")
            } else {
                format!("牌桌恩怨记录#{idx}")
            };
            Ok(note)
        },
    )
}

fn high_fan_east_tsumo_wall() -> Vec<Tile> {
    // East holds 清一色 + 碰碰胡 + 自摸 (>= 5 fan): 111m 222m 333m 444m 55m.
    let east_14 = tiles("111m 222m 333m 444m 55m");
    let other_13 = tiles("68m 147s 147p E S W N B");
    build_preset_wall(&east_14, &other_13, &other_13, &other_13, &[], &[])
        .expect("high-fan tsumo wall")
}

async fn spawn_memory_server(
    data_dir: PathBuf,
    seed: u64,
    walls: Vec<Vec<Tile>>,
    llm: Arc<ScriptedLlmClient>,
) -> ServerHandle {
    let factory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(Seat::East, walls.clone())))
    });
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(data_dir)
        .with_llm_client(llm)
        .spawn_with_options(
            TableOptions::with_wall_factory(seed, factory).with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn memory server")
}

async fn wait_for_hand_settled(ws: &mut WsClient) {
    loop {
        let (_, _, events, _) = recv_event_batch(ws).await;
        if events
            .iter()
            .any(|e| matches!(e.kind, PlayerEventKind::HandSettled { .. }))
        {
            return;
        }
    }
}

async fn fetch_persona_memory(addr: std::net::SocketAddr) -> PersonaMemoryResponse {
    let (status, headers, body) = http_get(addr, "/api/persona-memory").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    serde_json::from_slice(&body).expect("valid PersonaMemoryResponse JSON")
}

async fn wait_for_memory_notes(
    addr: std::net::SocketAddr,
    persona: PersonaId,
    player_nickname: &str,
    min_notes: usize,
) -> PersonaMemoryResponse {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let resp = fetch_persona_memory(addr).await;
        let count = resp
            .personas
            .iter()
            .find(|p| p.persona_id == persona)
            .and_then(|p| {
                p.players
                    .iter()
                    .find(|m| m.player_nickname == player_nickname)
            })
            .map_or(0, |m| m.grudge_notes.len());
        if count >= min_notes {
            return resp;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for >= {min_notes} grudge notes for {persona:?}/{player_nickname}; got {count}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_banter_requests(
    llm: &ScriptedLlmClient,
    min_requests: usize,
) -> Vec<BanterGenerationRequest> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let reqs = llm.captured();
        if reqs.len() >= min_requests {
            return reqs;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for >= {min_requests} banter requests"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn play_single_high_fan_free_table(
    addr: std::net::SocketAddr,
    client_id: &str,
    nickname: &str,
) {
    let mut ws = connect_and_hello(addr, client_id, nickname).await;
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
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Fast,
        },
    )
    .await;
    for (seat, persona) in [
        (Seat::South, PersonaId::TeaSarcasm),
        (Seat::West, PersonaId::QuickHothead),
        (Seat::North, PersonaId::TiltedMaster),
    ] {
        send_msg(
            &mut ws,
            &ClientMessage::SetSeatPersona {
                seat,
                persona_id: Some(persona),
            },
        )
        .await;
    }
    send_msg(&mut ws, &ClientMessage::StartMatch).await;
    let snap = recv_snapshot(&mut ws).await;
    assert_eq!(snap.phase, TablePhase::InMatch);

    send_msg(
        &mut ws,
        &ClientMessage::SubmitAction {
            expected_seq: snap.seq,
            client_action_id: format!("tsumo-{client_id}"),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    wait_for_hand_settled(&mut ws).await;

    send_msg(&mut ws, &ClientMessage::RequestEndFreeTable).await;
    let _ = recv_event_batch(&mut ws).await;
    let finished = recv_snapshot(&mut ws).await;
    assert_eq!(finished.phase, TablePhase::Finished);

    send_msg(&mut ws, &ClientMessage::ReturnToLobby).await;
    let lobby = recv_snapshot(&mut ws).await;
    assert_eq!(lobby.phase, TablePhase::Lobby);

    send_msg(&mut ws, &ClientMessage::LeaveSeat).await;
    let _ = recv_snapshot(&mut ws).await;
    drop(ws);
}

#[tokio::test]
async fn persona_memory_persists_across_restart_injects_scoped_banter_context_and_clears_via_http()
{
    let temp = TempDirGuard::new("majjj_persona_memory_e2e");
    let data_dir = temp.path().join("data");

    // 1. First server run: 小王 plays one high-fan FreeTable match -> ledger + 2 grudge notes (HighFan + MatchFinished).
    let llm1 = recording_memory_llm();
    let server1 = spawn_memory_server(
        data_dir.clone(),
        101,
        vec![high_fan_east_tsumo_wall()],
        Arc::clone(&llm1),
    )
    .await;
    let addr1 = server1.addr();

    play_single_high_fan_free_table(addr1, "c-wang-1", "  小王  ").await;

    let mem1 = wait_for_memory_notes(addr1, PersonaId::TeaSarcasm, "小王", 2).await;
    assert_eq!(mem1.schema_version, PERSONA_MEMORY_SCHEMA_VERSION);
    assert_eq!(mem1.personas.len(), 5);
    assert_eq!(mem1.total_pairs, 3); // 3 AI personas x 1 human player

    let tea_wang = mem1
        .personas
        .iter()
        .find(|p| p.persona_id == PersonaId::TeaSarcasm)
        .expect("TeaSarcasm group")
        .players
        .iter()
        .find(|m| m.player_nickname == "小王")
        .expect("小王 record under TeaSarcasm");
    assert_eq!(tea_wang.ledger.hands_played, 1);
    assert_eq!(tea_wang.ledger.player_wins_against_persona, 1);
    assert_eq!(tea_wang.ledger.persona_wins_against_player, 0);
    assert!(tea_wang.ledger.highest_fan_seen >= 4);
    assert!(tea_wang.ledger.net_score_delta < 0);
    assert!(tea_wang
        .grudge_notes
        .iter()
        .any(|n| n.trigger == PersonaMemoryNoteTrigger::HighFan));
    assert!(tea_wang
        .grudge_notes
        .iter()
        .any(|n| n.trigger == PersonaMemoryNoteTrigger::MatchFinished));
    assert!(data_dir.join("persona_memory.json").exists());

    server1.shutdown().await;

    // 2. Restart server on the same data_dir: memory must survive restart intact!
    let llm2 = recording_memory_llm();
    let server2 = spawn_memory_server(
        data_dir.clone(),
        102,
        vec![high_fan_east_tsumo_wall(), high_fan_east_tsumo_wall()],
        Arc::clone(&llm2),
    )
    .await;
    let addr2 = server2.addr();

    let restored = fetch_persona_memory(addr2).await;
    assert_eq!(restored.total_pairs, 3);
    let restored_tea_wang = restored
        .personas
        .iter()
        .find(|p| p.persona_id == PersonaId::TeaSarcasm)
        .unwrap()
        .players
        .iter()
        .find(|m| m.player_nickname == "小王")
        .expect("小王 restored after restart");
    assert_eq!(restored_tea_wang.ledger.hands_played, 1);
    assert!(restored_tea_wang.grudge_notes.len() >= 2);

    // 3. Another player 老李 plays a match -> 老李 gets their own secret memory notes.
    play_single_high_fan_free_table(addr2, "c-li-1", "老李").await;
    let _ = wait_for_memory_notes(addr2, PersonaId::TeaSarcasm, "老李", 2).await;

    // 4. Now 小王 sits back down for a new match:
    // Banter prompt MUST include 小王's historical ledger & grudge notes, and MUST NOT leak 老李!
    llm2.clear_captured();
    play_single_high_fan_free_table(addr2, "c-wang-2", "小王").await;
    let wang_banter = wait_for_banter_requests(&llm2, 1).await;
    assert!(
        wang_banter.iter().any(|req| {
            req.user_prompt.contains("牌友记忆")
                && req.user_prompt.contains("小王")
                && req.user_prompt.contains("小王大番账#")
        }),
        "expected 小王's persisted ledger & grudge notes in banter prompts, got: {wang_banter:#?}"
    );
    for req in &wang_banter {
        assert!(
            !req.user_prompt.contains("老李") && !req.user_prompt.contains("老李秘密恩怨"),
            "non-seated player 老李 leaked into 小王's banter prompt: {}",
            req.user_prompt
        );
    }

    // 5. HTTP POST /api/persona-memory/clear rejects missing/false confirm with 400 + no-store.
    for bad_body in [
        None,
        Some(br#"{"confirm":false}"#.as_slice()),
        Some(br#"{}"#.as_slice()),
    ] {
        let (status, headers, _) =
            http_post_raw(addr2, "/api/persona-memory/clear", bad_body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
    }

    // 6. Confirmed clear empties both in-memory store and disk file.
    let (status, headers, body) = http_post_raw(
        addr2,
        "/api/persona-memory/clear",
        Some(br#"{"confirm":true}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let cleared: ClearPersonaMemoryResponse =
        serde_json::from_slice(&body).expect("ClearPersonaMemoryResponse");
    assert!(cleared.cleared);
    assert_eq!(cleared.cleared_pairs, 6); // 3 personas x 2 players (小王, 老李)
    assert!(cleared.cleared_notes >= 6);

    let after_clear = fetch_persona_memory(addr2).await;
    assert_eq!(after_clear.total_pairs, 0);
    assert_eq!(after_clear.total_notes, 0);

    server2.shutdown().await;

    // 7. Restarting after clear still sees empty memory.
    let llm3 = recording_memory_llm();
    let server3 = spawn_memory_server(data_dir.clone(), 103, vec![], Arc::clone(&llm3)).await;
    let after_restart = fetch_persona_memory(server3.addr()).await;
    assert_eq!(after_restart.total_pairs, 0);
    assert_eq!(after_restart.total_notes, 0);
    server3.shutdown().await;

    // 8. Corrupt persona_memory.json on disk must NOT block server startup.
    fs::write(data_dir.join("persona_memory.json"), "{not-valid-json")
        .expect("write corrupt persona_memory.json");
    let llm4 = recording_memory_llm();
    let server4 = spawn_memory_server(data_dir.clone(), 104, vec![], Arc::clone(&llm4)).await;
    let recovered = fetch_persona_memory(server4.addr()).await;
    assert_eq!(recovered.total_pairs, 0);
    assert_eq!(recovered.total_notes, 0);
    server4.shutdown().await;
}

#[tokio::test]
async fn rolling_eight_note_cap_evicts_oldest_notes_over_disk_and_http() {
    let temp = TempDirGuard::new("majjj_persona_memory_cap");
    let data_dir = temp.path().join("data");

    // Pre-populate 7 notes on disk via PersonaMemoryStore, then play a live match adding 2 more (HighFan + MatchFinished) -> total 9 -> truncated FIFO to 8!
    {
        let mut store = PersonaMemoryStore::open(&data_dir).expect("open store");
        for i in 1..=7u64 {
            store.record_grudge_note(
                PersonaId::TeaSarcasm,
                "小王",
                &format!("历史恩怨#{i}"),
                PersonaMemoryNoteTrigger::MatchFinished,
                i * 1000,
            );
        }
    }

    let llm = recording_memory_llm();
    let server = spawn_memory_server(
        data_dir.clone(),
        201,
        vec![high_fan_east_tsumo_wall()],
        Arc::clone(&llm),
    )
    .await;
    let addr = server.addr();

    play_single_high_fan_free_table(addr, "c-cap", "小王").await;

    // Wait until the newest note ("小王大番账#") has landed and FIFO cap kept 8 notes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let final_notes = loop {
        let resp = fetch_persona_memory(addr).await;
        let notes = resp
            .personas
            .iter()
            .find(|p| p.persona_id == PersonaId::TeaSarcasm)
            .and_then(|p| p.players.iter().find(|m| m.player_nickname == "小王"))
            .map(|m| m.grudge_notes.clone())
            .unwrap_or_default();
        let new_count = notes
            .iter()
            .filter(|n| n.summary.starts_with("小王大番账#"))
            .count();
        if new_count >= 2 {
            break notes;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for 2 new notes; current notes: {notes:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };

    assert_eq!(final_notes.len(), MAX_GRUDGE_NOTES);
    assert!(
        !final_notes.iter().any(|n| n.summary == "历史恩怨#1"),
        "oldest note #1 must be evicted by 8-note FIFO cap: {final_notes:?}"
    );
    assert_eq!(final_notes[0].summary, "历史恩怨#2");
    assert!(final_notes.iter().all(|n| n.summary.chars().count() <= 30));

    server.shutdown().await;
}

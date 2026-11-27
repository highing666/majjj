mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::TempDirGuard;
use mahjong_ai::{AiDifficulty, AiStyle};
use mahjong_core::{MatchError, MatchMode, PlayerAction, Seat, Suit, Tile, TurnSource};
use mahjong_server::history_dto::{
    ClearHistoryRequest, ClearHistoryResponse, HistoryExportResponse, StatsResponse,
};
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::persona_memory::{
    ClearPersonaMemoryRequest, ClearPersonaMemoryResponse, PersonaGrudgeNote, PersonaMemoryEntry,
    PersonaMemoryNoteTrigger, PersonaMemoryResponse, PersonaPlayerLedger, PersonaPlayerMemory,
};
use mahjong_server::protocol::{
    ClientMessage, DestructiveOpKind, HealthResponse, LanUrlInfo, LobbyConfig,
    PendingDestructiveOp, QuickEmote, RemoteAccessStatusResponse, RemoteAuthRequest,
    RemoteAuthResponse, SeatInfo, SeatKind, SeatingRule, ServerMessage, SetPublicUrlRequest,
    TablePace, TablePhase, TableSnapshot,
};
use mahjong_server::public_url::PublicUrlSource;
use ts_rs::TS;

fn generated_bindings_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ parent")
        .parent()
        .expect("repo root")
        .join("web/src/lib/protocol/generated")
}

fn export_all_contract_types(out_dir: &Path) {
    ClientMessage::export_all_to(out_dir).expect("export ClientMessage bindings");
    ServerMessage::export_all_to(out_dir).expect("export ServerMessage bindings");
    HealthResponse::export_all_to(out_dir).expect("export HealthResponse bindings");
    RemoteAuthRequest::export_all_to(out_dir).expect("export RemoteAuthRequest bindings");
    RemoteAuthResponse::export_all_to(out_dir).expect("export RemoteAuthResponse bindings");
    RemoteAccessStatusResponse::export_all_to(out_dir)
        .expect("export RemoteAccessStatusResponse bindings");
    SetPublicUrlRequest::export_all_to(out_dir).expect("export SetPublicUrlRequest bindings");
    StatsResponse::export_all_to(out_dir).expect("export StatsResponse bindings");
    HistoryExportResponse::export_all_to(out_dir).expect("export HistoryExportResponse bindings");
    ClearHistoryRequest::export_all_to(out_dir).expect("export ClearHistoryRequest bindings");
    ClearHistoryResponse::export_all_to(out_dir).expect("export ClearHistoryResponse bindings");
    PersonaMemoryResponse::export_all_to(out_dir).expect("export PersonaMemoryResponse bindings");
    ClearPersonaMemoryRequest::export_all_to(out_dir)
        .expect("export ClearPersonaMemoryRequest bindings");
    ClearPersonaMemoryResponse::export_all_to(out_dir)
        .expect("export ClearPersonaMemoryResponse bindings");
    TurnSource::export_all_to(out_dir).expect("export TurnSource binding");
    Suit::export_all_to(out_dir).expect("export Suit binding");
    MatchError::export_all_to(out_dir).expect("export MatchError binding");
}

fn collect_ts_files(dir: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(dir).expect("read bindings directory") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .expect("utf-8 filename")
                .to_owned();
            let contents = fs::read_to_string(&path).expect("read generated .ts file");
            files.insert(name, contents);
        }
    }
    files
}

fn sync_bindings_dir(src_dir: &Path, dest_dir: &Path) {
    fs::create_dir_all(dest_dir).expect("create generated bindings dir");
    let src_files = collect_ts_files(src_dir);
    if dest_dir.exists() {
        for entry in fs::read_dir(dest_dir).expect("read existing bindings dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_file() {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .expect("utf-8 filename");
                if !src_files.contains_key(name) {
                    fs::remove_file(path).expect("remove stale generated file");
                }
            }
        }
    }
    for (name, contents) in &src_files {
        fs::write(dest_dir.join(name), contents).expect("write generated binding file");
    }
}

#[test]
fn server_and_core_ts_bindings_match_committed_files_with_zero_drift() {
    let temp = TempDirGuard::new("majjj_server_ts_contract_drift");
    export_all_contract_types(temp.path());

    let expected_files = collect_ts_files(temp.path());
    let expected_names: Vec<&str> = expected_files.keys().map(String::as_str).collect();
    assert_eq!(
        expected_names,
        vec![
            "AiDifficulty.ts",
            "AiStyle.ts",
            "ClearHistoryRequest.ts",
            "ClearHistoryResponse.ts",
            "ClearPersonaMemoryRequest.ts",
            "ClearPersonaMemoryResponse.ts",
            "ClientMessage.ts",
            "DestructiveOpKind.ts",
            "DiscardEntry.ts",
            "HandOutcome.ts",
            "HandRecord.ts",
            "HandSeatRecord.ts",
            "HandSettlement.ts",
            "HealthResponse.ts",
            "HistoryExportResponse.ts",
            "KongKind.ts",
            "KongTransfer.ts",
            "LanUrlInfo.ts",
            "LobbyConfig.ts",
            "MatchError.ts",
            "MatchMode.ts",
            "MatchRecord.ts",
            "MatchSeatRecord.ts",
            "MatchSummary.ts",
            "MeldView.ts",
            "PendingDestructiveOp.ts",
            "PersonaGrudgeNote.ts",
            "PersonaId.ts",
            "PersonaMemoryEntry.ts",
            "PersonaMemoryNoteTrigger.ts",
            "PersonaMemoryResponse.ts",
            "PersonaMood.ts",
            "PersonaPlayerLedger.ts",
            "PersonaPlayerMemory.ts",
            "PlayerAction.ts",
            "PlayerEvent.ts",
            "PlayerEventKind.ts",
            "PlayerHandView.ts",
            "PlayerStats.ts",
            "PlayerView.ts",
            "PreferencesData.ts",
            "PublicPhase.ts",
            "PublicSeatView.ts",
            "PublicUrlSource.ts",
            "QuickEmote.ts",
            "RemoteAccessStatusResponse.ts",
            "RemoteAuthRequest.ts",
            "RemoteAuthResponse.ts",
            "RevealedHand.ts",
            "RevealedMeld.ts",
            "RuleProfile.ts",
            "ScoreItem.ts",
            "Seat.ts",
            "SeatInfo.ts",
            "SeatKind.ts",
            "SeatingRule.ts",
            "ServerMessage.ts",
            "SetPublicUrlRequest.ts",
            "StatsResponse.ts",
            "Suit.ts",
            "TablePace.ts",
            "TablePhase.ts",
            "TableSnapshot.ts",
            "Tile.ts",
            "TurnSource.ts",
            "TurnView.ts",
            "WinKind.ts",
            "Wind.ts",
            "WinnerSettlement.ts",
        ],
        "exact set of 69 exported server, remote control/auth, history, persona memory, AI, persona, and core TypeScript files"
    );

    for (name, contents) in &expected_files {
        assert!(
            !contents.contains("bigint"),
            "generated {name} must not use bigint over JSON WebSocket:\n{contents}"
        );
    }

    let committed_dir = generated_bindings_dir();
    if std::env::var_os("UPDATE_TS_BINDINGS").is_some_and(|v| v == "1") {
        sync_bindings_dir(temp.path(), &committed_dir);
    }

    assert!(
        committed_dir.exists(),
        "committed bindings directory {} does not exist (run with UPDATE_TS_BINDINGS=1 to generate)",
        committed_dir.display()
    );

    let actual_files = collect_ts_files(&committed_dir);
    let actual_names: Vec<&str> = actual_files.keys().map(String::as_str).collect();
    assert_eq!(
        actual_names, expected_names,
        "committed TypeScript filename set drifted from mahjong-server"
    );

    for (name, expected_contents) in &expected_files {
        let actual_contents = actual_files
            .get(name)
            .unwrap_or_else(|| panic!("missing {name} in {}", committed_dir.display()));
        assert_eq!(
            actual_contents, expected_contents,
            "TypeScript contract drift detected in {name}; run UPDATE_TS_BINDINGS=1 cargo test -p mahjong-server --test ts_contract_drift"
        );
    }
}

#[test]
fn frozen_server_protocol_types_roundtrip_serde_json_without_loss() {
    let config = LobbyConfig {
        mode: MatchMode::FreeTable,
        seating: SeatingRule::FixedOpposite,
        ai_difficulty: AiDifficulty::Intermediate,
        pace: TablePace::Fast,
    };
    let seats = [
        SeatInfo {
            seat: Seat::East,
            kind: SeatKind::Human,
            nickname: "Alice".to_string(),
            connected: true,
            ai_assisted: false,
            ai_difficulty: None,
            ai_style: None,
            persona_id: None,
            mood: None,
            voice_pack_id: Some("human_male".to_string()),
        },
        SeatInfo {
            seat: Seat::South,
            kind: SeatKind::Ai,
            nickname: "急眼快攻哥".to_string(),
            connected: true,
            ai_assisted: false,
            ai_difficulty: Some(AiDifficulty::Strong),
            ai_style: Some(AiStyle::AggressiveFast),
            persona_id: Some(PersonaId::QuickHothead),
            mood: Some(PersonaMood::Smug),
            voice_pack_id: Some("quick_hothead".to_string()),
        },
        SeatInfo {
            seat: Seat::West,
            kind: SeatKind::Empty,
            nickname: String::new(),
            connected: false,
            ai_assisted: false,
            ai_difficulty: None,
            ai_style: None,
            persona_id: Some(PersonaId::TeaSarcasm),
            mood: None,
            voice_pack_id: None,
        },
        SeatInfo {
            seat: Seat::North,
            kind: SeatKind::Human,
            nickname: "Bob".to_string(),
            connected: false,
            ai_assisted: true,
            ai_difficulty: Some(AiDifficulty::Intermediate),
            ai_style: Some(AiStyle::DefensiveBigHand),
            persona_id: None,
            mood: None,
            voice_pack_id: None,
        },
    ];
    let lan_urls = LanUrlInfo {
        preferred_ipv4_url: Some("http://192.168.1.10:8789".to_string()),
        ipv4_urls: vec![
            "http://192.168.1.10:8789".to_string(),
            "http://10.0.0.5:8789".to_string(),
        ],
        local_hostname_url: Some("http://macbook.local:8789".to_string()),
    };
    let pending_op = PendingDestructiveOp {
        op_id: "op-1".to_string(),
        kind: DestructiveOpKind::DisbandMatch,
        requested_by: Seat::East,
        required_seats: vec![Seat::East, Seat::North],
        confirmed_seats: vec![Seat::East],
    };

    let client_messages = [
        ClientMessage::Hello {
            client_id: "client-1".to_string(),
            nickname: "Alice".to_string(),
            table_id: Some("table-1".to_string()),
            seat_token: Some("token-1".to_string()),
        },
        ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
        ClientMessage::JoinSeat { seat: None },
        ClientMessage::LeaveSeat,
        ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::EastOnly,
            seating: SeatingRule::FixedAdjacent,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
        ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::TeaSarcasm),
        },
        ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: None,
        },
        ClientMessage::SetSeatVoicePack {
            voice_pack_id: "human_female".to_string(),
        },
        ClientMessage::StartMatch,
        ClientMessage::ReturnToLobby,
        ClientMessage::SubmitAction {
            expected_seq: 42,
            client_action_id: "act-1".to_string(),
            action: PlayerAction::Discard {
                tile: Tile::new(0).unwrap(),
                from_drawn: true,
            },
        },
        ClientMessage::SetPace {
            pace: TablePace::Fast,
        },
        ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
        ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
        ClientMessage::SetSlowThinkNudge { enabled: false },
        ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
        ClientMessage::TakeOverSeat {
            seat: Seat::North,
            confirm: true,
        },
        ClientMessage::RequestEndFreeTable,
        ClientMessage::RequestDisbandMatch,
        ClientMessage::ConfirmDestructiveOp {
            op_id: "op-1".to_string(),
            accept: true,
        },
        ClientMessage::RequestSnapshot,
        ClientMessage::ClientPing {
            ts_ms: 1_700_000_000_000,
        },
        ClientMessage::ClientLeaving,
    ];
    for msg in &client_messages {
        let json = serde_json::to_string(msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, msg);
    }

    let snapshot = TableSnapshot {
        table_id: "table-1".to_string(),
        phase: TablePhase::Lobby,
        config,
        pace: TablePace::Fast,
        slow_think_nudge_enabled: true,
        seats: seats.clone(),
        your_seat: Some(Seat::East),
        seat_token: Some("token-1".to_string()),
        player_view: None,
        pending_destructive_op: Some(pending_op),
        seq: 0,
        server_version: "0.1.0".to_string(),
        lan_urls,
        host_is_loopback: true,
        client_is_remote: false,
    };

    let server_messages = [
        ServerMessage::TableSnapshot(snapshot),
        ServerMessage::EventBatch {
            from_seq: 1,
            to_seq: 2,
            events: Vec::new(),
            updated_legal_actions: vec![PlayerAction::Pass],
        },
        ServerMessage::LobbyUpdated {
            phase: TablePhase::Lobby,
            config,
            seats,
            slow_think_nudge_enabled: false,
        },
        ServerMessage::ActionRejected {
            reason: "stale seq".to_string(),
            current_seq: 5,
        },
        ServerMessage::ServerPing { ts_ms: 1234 },
        ServerMessage::ServerPong { ts_ms: 1234 },
        ServerMessage::SessionReplaced,
        ServerMessage::RemoteAccessDisabled {
            message: "宿主已关闭远程联机，连接已断开".to_string(),
        },
        ServerMessage::ErrorNotice {
            message: "not enabled".to_string(),
        },
        ServerMessage::TableBanter {
            banter_id: "b-1".to_string(),
            seat: Seat::South,
            persona_id: PersonaId::QuickHothead,
            mood: PersonaMood::Smug,
            text: "五巡就胡，你们太慢了！".to_string(),
            event_seq: 18,
            reply_to_seat: Some(Seat::East),
        },
        ServerMessage::TableBanterAudioReady {
            banter_id: "b-1".to_string(),
            seat: Seat::South,
            audio_url: "/api/banter-audio/b-1".to_string(),
            duration_ms: 1420,
        },
        ServerMessage::QuickEmoteBroadcast {
            seat: Seat::East,
            emote: QuickEmote::DealerRigged,
            text: QuickEmote::DealerRigged.label().to_string(),
        },
        ServerMessage::LlmStatusNotice {
            available: false,
            message: "局域网推理服务异常：请求超时".to_string(),
        },
        ServerMessage::TtsStatusNotice {
            available: false,
            message: "局域网台词语音服务暂不可用，已自动退回漫画气泡与角色语音包".to_string(),
        },
    ];
    for msg in &server_messages {
        let json = serde_json::to_string(msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, msg);
    }

    let health = HealthResponse {
        app: "majjj".to_string(),
        version: "0.1.0".to_string(),
        phase: TablePhase::Lobby,
        online_humans: 2,
        port: 8789,
    };
    let health_back: HealthResponse =
        serde_json::from_str(&serde_json::to_string(&health).unwrap()).unwrap();
    assert_eq!(health_back, health);

    let auth_req = RemoteAuthRequest {
        code: "ABCD2345".to_string(),
    };
    let auth_req_back: RemoteAuthRequest =
        serde_json::from_str(&serde_json::to_string(&auth_req).unwrap()).unwrap();
    assert_eq!(auth_req_back, auth_req);

    let auth_resp = RemoteAuthResponse {
        authenticated: false,
        message: "尝试次数过多，请 60 秒后再试".to_string(),
        retry_after_secs: Some(60),
    };
    let auth_resp_back: RemoteAuthResponse =
        serde_json::from_str(&serde_json::to_string(&auth_resp).unwrap()).unwrap();
    assert_eq!(auth_resp_back, auth_resp);

    let set_public_url_req = SetPublicUrlRequest {
        public_url: Some("http://majjj.example.com:8789".to_string()),
    };
    let set_public_url_back: SetPublicUrlRequest =
        serde_json::from_str(&serde_json::to_string(&set_public_url_req).unwrap()).unwrap();
    assert_eq!(set_public_url_back, set_public_url_req);

    let remote_status = RemoteAccessStatusResponse {
        enabled: true,
        passcode: Some("ABCD2345".to_string()),
        active_remote_connections: 2,
        public_url_override: Some("http://majjj.example.com:8789".to_string()),
        public_url_source: PublicUrlSource::Preferences,
        persisted_public_url: Some("http://majjj.example.com:8789".to_string()),
        preferred_ipv6_url: Some("http://[2001:db9:1::10]:8789".to_string()),
        ipv6_urls: vec!["http://[2001:db9:1::10]:8789".to_string()],
        invite_base_url: Some("http://majjj.example.com:8789".to_string()),
        invite_url: Some("http://majjj.example.com:8789/?code=ABCD2345".to_string()),
    };
    let remote_status_back: RemoteAccessStatusResponse =
        serde_json::from_str(&serde_json::to_string(&remote_status).unwrap()).unwrap();
    assert_eq!(remote_status_back, remote_status);

    let memory_resp = PersonaMemoryResponse {
        schema_version: 1,
        total_pairs: 1,
        total_notes: 1,
        personas: vec![PersonaMemoryEntry {
            persona_id: PersonaId::TeaSarcasm,
            players: vec![PersonaPlayerMemory {
                player_nickname: "小王".to_string(),
                ledger: PersonaPlayerLedger {
                    hands_played: 3,
                    persona_wins_against_player: 1,
                    player_wins_against_persona: 2,
                    persona_dealt_into_player: 1,
                    player_dealt_into_persona: 1,
                    highest_fan_seen: 4,
                    net_score_delta: -12,
                    player_pon_count: 2,
                    player_kong_count: 1,
                },
                grudge_notes: vec![PersonaGrudgeNote {
                    summary: "小王上次抢杠胡让我记住他了".to_string(),
                    created_at_ms: 1_700_000_000_000,
                    trigger: PersonaMemoryNoteTrigger::RobKongHu,
                }],
                updated_at_ms: 1_700_000_000_000,
            }],
        }],
    };
    let memory_back: PersonaMemoryResponse =
        serde_json::from_str(&serde_json::to_string(&memory_resp).unwrap()).unwrap();
    assert_eq!(memory_back, memory_resp);

    let clear_req = ClearPersonaMemoryRequest { confirm: true };
    let clear_req_back: ClearPersonaMemoryRequest =
        serde_json::from_str(&serde_json::to_string(&clear_req).unwrap()).unwrap();
    assert_eq!(clear_req_back, clear_req);

    let clear_resp = ClearPersonaMemoryResponse {
        cleared: true,
        cleared_pairs: 1,
        cleared_notes: 1,
    };
    let clear_resp_back: ClearPersonaMemoryResponse =
        serde_json::from_str(&serde_json::to_string(&clear_resp).unwrap()).unwrap();
    assert_eq!(clear_resp_back, clear_resp);
}

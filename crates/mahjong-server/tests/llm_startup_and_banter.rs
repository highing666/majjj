mod common;

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use common::{connect_and_hello, recv_msg, recv_snapshot, send_msg, tile, tiles, TempDirGuard};
use mahjong_ai::AiDifficulty;
use mahjong_core::{build_preset_wall, MatchMode, PlayerAction, PresetWallProvider, Seat};
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{ClientMessage, SeatingRule, ServerMessage, TablePace, TablePhase};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::{LlmClient, PacingMode, ServerConfig, ServerError};
use tokio::net::TcpListener;
use tokio::sync::watch;

#[derive(Debug, Clone)]
enum FakeCompletionMode {
    Ok {
        mood: &'static str,
        text: &'static str,
    },
    ThinkingLeak,
    Slow(Duration),
    Http500,
}

#[derive(Clone)]
struct FakeLlmState {
    healthy: Arc<Mutex<bool>>,
    models: Arc<Mutex<Vec<String>>>,
    mode: Arc<Mutex<FakeCompletionMode>>,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}

struct FakeLlamaServer {
    addr: SocketAddr,
    state: FakeLlmState,
    shutdown_tx: watch::Sender<bool>,
}

impl FakeLlamaServer {
    async fn start(initial_mode: FakeCompletionMode) -> Self {
        let state = FakeLlmState {
            healthy: Arc::new(Mutex::new(true)),
            models: Arc::new(Mutex::new(vec!["Qwen3.5-9B-Q5_K_M.gguf".to_string()])),
            mode: Arc::new(Mutex::new(initial_mode)),
            requests: Arc::new(Mutex::new(Vec::new())),
        };
        let router = Router::new()
            .route("/health", get(fake_health))
            .route("/v1/models", get(fake_models))
            .route("/v1/chat/completions", post(fake_chat_completions))
            .with_state(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake llama-server");
        let addr = listener.local_addr().expect("local addr");
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.wait_for(|&stopped| stopped).await;
                })
                .await;
        });

        Self {
            addr,
            state,
            shutdown_tx,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn set_mode(&self, mode: FakeCompletionMode) {
        *self.state.mode.lock().expect("lock mode") = mode;
    }

    fn set_models(&self, models: Vec<String>) {
        *self.state.models.lock().expect("lock models") = models;
    }

    fn recorded_requests(&self) -> Vec<serde_json::Value> {
        self.state.requests.lock().expect("lock requests").clone()
    }

    fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
    }
}

async fn fake_health(State(state): State<FakeLlmState>) -> impl IntoResponse {
    if *state.healthy.lock().expect("lock healthy") {
        (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "loading model" })),
        )
    }
}

async fn fake_models(State(state): State<FakeLlmState>) -> impl IntoResponse {
    let models = state.models.lock().expect("lock models").clone();
    let data: Vec<serde_json::Value> = models
        .into_iter()
        .map(|id| serde_json::json!({ "id": id, "object": "model" }))
        .collect();
    (StatusCode::OK, Json(serde_json::json!({ "data": data })))
}

async fn fake_chat_completions(
    State(state): State<FakeLlmState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    state.requests.lock().expect("lock requests").push(body);
    let mode = state.mode.lock().expect("lock mode").clone();
    match mode {
        FakeCompletionMode::Ok { mood, text } => {
            let content = serde_json::json!({ "mood": mood, "text": text }).to_string();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "choices": [{
                        "message": { "role": "assistant", "content": content }
                    }]
                })),
            )
        }
        FakeCompletionMode::ThinkingLeak => (
            StatusCode::OK,
            Json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "<think>secret</think>{\"mood\":\"Smug\",\"text\":\"穿帮了\"}"
                    }
                }]
            })),
        ),
        FakeCompletionMode::Slow(delay) => {
            tokio::time::sleep(delay).await;
            let content = serde_json::json!({ "mood": "Deadpan", "text": "迟到了" }).to_string();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "choices": [{
                        "message": { "role": "assistant", "content": content }
                    }]
                })),
            )
        }
        FakeCompletionMode::Http500 => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": { "message": "GPU OOM" } })),
        ),
    }
}

fn unused_local_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn startup_gate_enforces_reachable_llm_precedence_and_binary_exit_on_failure() {
    // 1. Unreachable LLM endpoint -> ServerConfig::spawn fails fast with clear Chinese ServerError::LlmUnavailable.
    let dead_port = unused_local_port();
    let dead_url = format!("http://127.0.0.1:{dead_port}");
    let err = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_url(Some(dead_url.clone()))
        .spawn()
        .await
        .expect_err("unreachable LLM must fail startup");
    assert!(
        matches!(err, ServerError::LlmUnavailable { .. }),
        "expected ServerError::LlmUnavailable, got {err:?}"
    );
    let err_text = err.to_string();
    assert!(
        err_text.contains("无法连接局域网推理服务") && err_text.contains(&dead_url),
        "Chinese startup error must include reason and endpoint, got: {err_text}"
    );

    // 2. Reachable /health but empty /v1/models -> must also fail startup!
    let fake = FakeLlamaServer::start(FakeCompletionMode::Ok {
        mood: "Smug",
        text: "测试台词",
    })
    .await;
    fake.set_models(Vec::new());
    let empty_models_err = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_url(Some(fake.base_url()))
        .spawn()
        .await
        .expect_err("empty /v1/models must fail startup");
    assert!(matches!(
        empty_models_err,
        ServerError::LlmUnavailable { .. }
    ));

    // 3. Restore model -> preferences.json `llm_base_url` works when CLI/env is absent.
    fake.set_models(vec!["Qwen3.5-9B-Q5_K_M.gguf".to_string()]);
    let temp = TempDirGuard::new("majjj_llm_prefs");
    fs::write(
        temp.path().join("preferences.json"),
        serde_json::json!({
            "schema_version": 1,
            "recent_nicknames": [],
            "default_mode": "EastOnly",
            "default_seating": "KeepLobbySeats",
            "default_ai_difficulty": "Strong",
            "default_pace": "Standard",
            "llm_base_url": fake.base_url()
        })
        .to_string(),
    )
    .expect("write preferences.json");

    let handle = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .spawn()
        .await
        .expect("server must start using preferences.json llm_base_url");
    handle.shutdown().await;

    // 4. Explicit `--llm-url` / `with_llm_url` overrides `preferences.json`.
    let override_err = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_url(Some(dead_url.clone()))
        .spawn()
        .await
        .expect_err("explicit llm_url must override preferences.json");
    assert!(override_err.to_string().contains(&dead_url));

    // 5. Real binary exits non-zero with Chinese stderr when `--llm-url` is unreachable.
    let bin = env!("CARGO_BIN_EXE_mahjong-server");
    let dead_url_for_bin = dead_url.clone();
    let out = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                "0",
                "--no-open",
                "--llm-url",
                &dead_url_for_bin,
            ])
            .output()
            .expect("run mahjong-server binary")
    })
    .await
    .unwrap();
    assert!(
        !out.status.success(),
        "binary must fail fast when llama-server is unreachable"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("无法连接局域网推理服务"),
        "binary stderr should contain Chinese error, got: {stderr}"
    );

    fake.shutdown();
}

#[tokio::test]
async fn fake_llama_server_end_to_end_json_schema_banter_timeout_and_recovery() {
    let fake = FakeLlamaServer::start(FakeCompletionMode::Ok {
        mood: "Smug",
        text: "开局手牌顺得很，你们都别想胡！",
    })
    .await;

    // Hand 1: East instant Tsumo (verifies HandStarted + HandSettled revealed hands).
    // Hand 2: East discards 1m, South Pons 1m (verifies MeldDeclared short sync + outage/recovery).
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall1 =
        build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[]).expect("wall1");

    let east_hand2 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let south_hand2 = tiles("11m 222m 333m 55s 67s 9p");
    let west_hand2 = tiles("6m 9m 2s 5s 8s 2p 5p 8p E S W N B");
    let north_hand2 = tiles("6m 9m 3s 6s 9s 3p 6p 9p E S W N F");
    let wall2 = build_preset_wall(
        &east_hand2,
        &south_hand2,
        &west_hand2,
        &north_hand2,
        &[],
        &[],
    )
    .expect("wall2");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    });

    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_url(Some(fake.base_url()))
        .spawn_with_options(
            TableOptions::with_wall_factory(42, factory)
                .with_pacing(PacingMode::Immediate)
                .with_banter_sync_cap(Duration::from_millis(350)),
        )
        .await
        .expect("spawn server against local fake llama-server");

    let mut east = connect_and_hello(server.addr(), "e2e-east", "小王").await;
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
        &ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::QuickHothead),
        },
    )
    .await;
    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let start_snap = recv_snapshot(&mut east).await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);

    // 1. Receive opening TableBanter from real OpenAiLlmClient -> fake llama-server.
    let opening_banter = loop {
        match recv_msg(&mut east).await {
            msg @ ServerMessage::TableBanter { .. } => break msg,
            ServerMessage::LobbyUpdated { .. } | ServerMessage::ServerPing { .. } => continue,
            other => panic!("expected opening TableBanter, got {other:?}"),
        }
    };
    match opening_banter {
        ServerMessage::TableBanter { mood, text, .. } => {
            assert_eq!(mood, PersonaMood::Smug);
            assert_eq!(text, "开局手牌顺得很，你们都别想胡！");
        }
        _ => unreachable!(),
    }

    // Verify exact OpenAI request body sent to fake llama-server.
    let reqs = fake.recorded_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["model"], "Qwen3.5-9B-Q5_K_M.gguf");
    assert_eq!(reqs[0]["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(reqs[0]["response_format"]["type"], "json_schema");
    assert_eq!(reqs[0]["response_format"]["json_schema"]["strict"], true);

    // 2. East wins Hand 1 by Tsumo -> HandSettled triggers banter with all 4 revealed hands!
    fake.set_mode(FakeCompletionMode::Ok {
        mood: "Mocking",
        text: "哟哟哟，第一手就自摸红中，发牌员是你亲戚吧？",
    });
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "win-hand-1".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;

    let mut settled_seq = start_snap.seq;
    let mut saw_settlement_banter = false;
    while !saw_settlement_banter {
        match recv_msg(&mut east).await {
            ServerMessage::EventBatch { to_seq, .. } => {
                settled_seq = to_seq;
            }
            ServerMessage::TableBanter { mood, text, .. } => {
                assert_eq!(mood, PersonaMood::Mocking);
                assert!(text.contains("红中"));
                saw_settlement_banter = true;
            }
            ServerMessage::ServerPing { .. } | ServerMessage::LobbyUpdated { .. } => {}
            other => panic!("unexpected message on HandSettled: {other:?}"),
        }
    }
    let reqs_after_settle = fake.recorded_requests();
    let settle_user_prompt = reqs_after_settle[1]["messages"][1]["content"]
        .as_str()
        .expect("settlement user prompt");
    assert!(
        settle_user_prompt.contains("四家摊开手牌") && settle_user_prompt.contains("白板"),
        "HandSettled prompt must reveal all 4 hands: {settle_user_prompt}"
    );

    // 3. Switch fake server to ThinkingLeak -> confirm Hand 2 -> must emit LlmStatusNotice { available: false }!
    fake.set_mode(FakeCompletionMode::ThinkingLeak);
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: settled_seq,
            client_action_id: "confirm-h2".to_string(),
            action: PlayerAction::ConfirmNextHand,
        },
    )
    .await;

    let mut hand2_seq = settled_seq;
    let mut saw_outage_notice = false;
    while !saw_outage_notice {
        match recv_msg(&mut east).await {
            ServerMessage::EventBatch { to_seq, .. } => {
                hand2_seq = to_seq;
            }
            ServerMessage::LlmStatusNotice { available, message } => {
                assert!(!available);
                assert!(message.contains("思考链"));
                saw_outage_notice = true;
            }
            ServerMessage::ServerPing { .. } | ServerMessage::LobbyUpdated { .. } => {}
            other => panic!("unexpected message on thinking leak: {other:?}"),
        }
    }

    // 4. Switch fake server back to Ok -> East discards 1m -> South Pons 1m -> emits recovery LlmStatusNotice + TableBanter!
    fake.set_mode(FakeCompletionMode::Ok {
        mood: "Tilted",
        text: "碰你这张一万，看你还怎么装！",
    });
    send_msg(
        &mut east,
        &ClientMessage::SubmitAction {
            expected_seq: hand2_seq,
            client_action_id: "h2-discard-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;

    let mut saw_recovery = false;
    let mut saw_pon_banter = false;
    while !(saw_recovery && saw_pon_banter) {
        match recv_msg(&mut east).await {
            ServerMessage::LlmStatusNotice { available, .. } if available => {
                saw_recovery = true;
            }
            ServerMessage::TableBanter {
                seat,
                persona_id,
                mood,
                text,
                ..
            } => {
                assert_eq!(seat, Seat::South);
                assert_eq!(persona_id, PersonaId::QuickHothead);
                assert_eq!(mood, PersonaMood::Tilted);
                assert_eq!(text, "碰你这张一万，看你还怎么装！");
                saw_pon_banter = true;
            }
            ServerMessage::EventBatch { .. }
            | ServerMessage::ServerPing { .. }
            | ServerMessage::LobbyUpdated { .. } => {}
            other => panic!("unexpected message during recovery: {other:?}"),
        }
    }

    // 5. Verify Slow (> sync cap) and Http500 directly through OpenAiLlmClient & real HTTP server.
    fake.set_mode(FakeCompletionMode::Http500);
    let http_client = mahjong_server::OpenAiLlmClient::new(&fake.base_url())
        .expect("build client")
        .with_request_timeout(Duration::from_millis(200));
    let err_500 = http_client
        .generate_banter(mahjong_server::BanterGenerationRequest::new("sys", "usr"))
        .await
        .expect_err("HTTP 500 must fail");
    assert!(matches!(
        err_500,
        mahjong_server::LlmError::HttpStatus { status: 500, .. }
    ));

    fake.set_mode(FakeCompletionMode::Slow(Duration::from_secs(2)));
    let err_timeout = http_client
        .generate_banter(mahjong_server::BanterGenerationRequest::new("sys", "usr"))
        .await
        .expect_err("Slow completion must hit timeout");
    assert!(matches!(
        err_timeout,
        mahjong_server::LlmError::Timeout { .. }
    ));

    let _ = east.close(None).await;
    server.shutdown().await;
    fake.shutdown();
}

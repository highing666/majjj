mod common;

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use common::{
    connect_and_hello, http_get, make_pcm16_mono_wav, no_reaction_wall, recv_msg, recv_snapshot,
    send_msg, stub_llm_client, TempDirGuard,
};
use mahjong_core::{PresetWallProvider, Seat};
use mahjong_server::banter_audio::BanterAudioCache;
use mahjong_server::protocol::{ClientMessage, QuickEmote, ServerMessage, TablePhase};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::{PacingMode, ServerConfig};
use tokio::net::TcpListener;
use tokio::sync::watch;

#[derive(Clone)]
struct FakeTtsState {
    healthy: Arc<Mutex<bool>>,
    wav_bytes: Arc<Mutex<Vec<u8>>>,
    duration_ms: Arc<Mutex<u32>>,
    synth_delay: Arc<Mutex<Duration>>,
    return_408_once: Arc<Mutex<bool>>,
    health_requests: Arc<Mutex<usize>>,
    synth_requests: Arc<Mutex<Vec<serde_json::Value>>>,
}

struct FakeTtsServer {
    addr: SocketAddr,
    state: FakeTtsState,
    shutdown_tx: watch::Sender<bool>,
}

impl FakeTtsServer {
    async fn start(wav_bytes: Vec<u8>, duration_ms: u32) -> Self {
        let state = FakeTtsState {
            healthy: Arc::new(Mutex::new(true)),
            wav_bytes: Arc::new(Mutex::new(wav_bytes)),
            duration_ms: Arc::new(Mutex::new(duration_ms)),
            synth_delay: Arc::new(Mutex::new(Duration::ZERO)),
            return_408_once: Arc::new(Mutex::new(false)),
            health_requests: Arc::new(Mutex::new(0)),
            synth_requests: Arc::new(Mutex::new(Vec::new())),
        };
        let router = Router::new()
            .route("/health", get(fake_tts_health))
            .route("/v1/tts/synthesize", post(fake_tts_synthesize))
            .with_state(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake tts server");
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

    fn health_count(&self) -> usize {
        *self
            .state
            .health_requests
            .lock()
            .expect("lock health count")
    }

    fn recorded_synth_requests(&self) -> Vec<serde_json::Value> {
        self.state
            .synth_requests
            .lock()
            .expect("lock synth requests")
            .clone()
    }

    fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
    }
}

async fn fake_tts_health(State(state): State<FakeTtsState>) -> impl IntoResponse {
    *state.health_requests.lock().expect("lock health") += 1;
    if *state.healthy.lock().expect("lock healthy") {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "ready": true,
                "backend": "qwen3-tts-0.6b-customvoice",
                "model": "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
                "queue_depth": 0,
                "personas_cached": 5
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "loading", "ready": false })),
        )
    }
}

async fn fake_tts_synthesize(
    State(state): State<FakeTtsState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    state
        .synth_requests
        .lock()
        .expect("lock synth requests")
        .push(body);
    let delay = *state.synth_delay.lock().expect("lock synth delay");
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let should_408 = {
        let mut flag = state.return_408_once.lock().expect("lock 408 flag");
        if *flag {
            *flag = false;
            true
        } else {
            false
        }
    };
    if should_408 {
        return (
            StatusCode::REQUEST_TIMEOUT,
            Json(serde_json::json!({
                "error": "deadline_exceeded",
                "message": "queue wait exceeded deadline_ms"
            })),
        )
            .into_response();
    }
    let wav = state.wav_bytes.lock().expect("lock wav").clone();
    let dur = *state.duration_ms.lock().expect("lock dur");
    let mut resp = (StatusCode::OK, wav).into_response();
    resp.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
    resp.headers_mut().insert(
        "x-tts-duration-ms",
        HeaderValue::from_str(&dur.to_string()).expect("duration header"),
    );
    resp
}

fn unused_local_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().unwrap().port()
}

fn no_reaction_factory() -> WallProviderFactory {
    let wall = no_reaction_wall();
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    })
}

#[tokio::test]
async fn startup_with_unreachable_tts_still_boots_plays_and_notifies_clients() {
    let dead_url = format!("http://127.0.0.1:{}", unused_local_port());
    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(dead_url))
        .spawn_with_options(
            TableOptions::with_wall_factory(11, no_reaction_factory())
                .with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("server MUST boot cleanly even when LAN TTS is unreachable");

    let mut east = connect_and_hello(server.addr(), "u1", "小王").await;
    // Receive TtsStatusNotice(available=false) either immediately from Hello or from completing startup probe.
    let notice = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TtsStatusNotice { available, message } => break (available, message),
            ServerMessage::LobbyUpdated { .. } | ServerMessage::ServerPing { .. } => continue,
            other => panic!("expected TtsStatusNotice(false), got {other:?}"),
        }
    };
    assert!(!notice.0);
    assert!(
        notice.1.contains("台词语音"),
        "expected Chinese notice mentioning 台词语音, got: {}",
        notice.1
    );

    // Match starts and delivers TableBanter normally with banter_id despite TTS outage.
    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let snap = recv_snapshot(&mut east).await;
    assert_eq!(snap.phase, TablePhase::InMatch);

    let banter_id = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanter { banter_id, .. } => break banter_id,
            ServerMessage::LobbyUpdated { .. } | ServerMessage::ServerPing { .. } => continue,
            other => panic!("expected TableBanter during TTS outage, got {other:?}"),
        }
    };
    assert_eq!(banter_id, "b-1");

    let _ = east.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn single_synthesis_shared_by_multiple_ws_clients_over_http_and_404_on_unknown_or_evicted() {
    let expected_wav = make_pcm16_mono_wav(12_000, 0x5A);
    let fake_tts = FakeTtsServer::start(expected_wav.clone(), 1350).await;

    // 1-entry cache so a second banter evicts the first to verify HTTP 404 on evicted IDs.
    let small_cache = Arc::new(Mutex::new(BanterAudioCache::with_limits(
        1,
        Duration::from_secs(60),
        5 * 1024 * 1024,
    )));

    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(fake_tts.base_url()))
        .spawn_with_options(
            TableOptions::with_wall_factory(22, no_reaction_factory())
                .with_pacing(PacingMode::Immediate)
                .with_banter_audio_cache(small_cache),
        )
        .await
        .expect("spawn server against local fake TTS");

    let mut east = connect_and_hello(server.addr(), "c-east", "小王").await;
    let mut west = connect_and_hello(server.addr(), "c-west", "小李").await;
    let mut north = connect_and_hello(server.addr(), "c-north", "小张").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;
    send_msg(
        &mut west,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::West),
        },
    )
    .await;
    let _ = recv_snapshot(&mut west).await;
    send_msg(
        &mut north,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::North),
        },
    )
    .await;
    let _ = recv_snapshot(&mut north).await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let _ = recv_snapshot(&mut east).await;
    let _ = recv_snapshot(&mut west).await;
    let _ = recv_snapshot(&mut north).await;

    // Both clients receive TableBanter and TableBanterAudioReady for b-1.
    let mut east_ready = None;
    let mut west_ready = None;
    while east_ready.is_none() {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioReady {
                banter_id,
                audio_url,
                duration_ms,
                ..
            } => east_ready = Some((banter_id, audio_url, duration_ms)),
            ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected east msg: {other:?}"),
        }
    }
    while west_ready.is_none() {
        match recv_msg(&mut west).await {
            ServerMessage::TableBanterAudioReady {
                banter_id,
                audio_url,
                duration_ms,
                ..
            } => west_ready = Some((banter_id, audio_url, duration_ms)),
            ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected west msg: {other:?}"),
        }
    }

    let (banter_id, audio_url, duration_ms) = east_ready.unwrap();
    assert_eq!(
        west_ready.unwrap(),
        (banter_id.clone(), audio_url.clone(), duration_ms)
    );
    assert_eq!(banter_id, "b-1");
    assert_eq!(audio_url, "/api/banter-audio/b-1");
    assert_eq!(duration_ms, 1350);

    // Upstream FakeTtsServer synthesized ONCE for both connected clients and received exact contract!
    let synth_reqs = fake_tts.recorded_synth_requests();
    assert_eq!(synth_reqs.len(), 1);
    assert_eq!(synth_reqs[0]["request_id"], "b-1");
    assert_eq!(synth_reqs[0]["format"], "wav");
    assert_eq!(synth_reqs[0]["stream"], false);
    assert_eq!(synth_reqs[0]["deadline_ms"], 5000);

    // Both clients fetch GET /api/banter-audio/b-1 and receive exact WAV bytes + headers.
    for _ in 0..2 {
        let (status, headers, body) = http_get(server.addr(), &audio_url).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()),
            Some("audio/wav")
        );
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(body, expected_wav);
    }

    // Unknown and invalid/traversal IDs return 404.
    let (unknown_status, _, _) = http_get(server.addr(), "/api/banter-audio/b-9999").await;
    assert_eq!(unknown_status, StatusCode::NOT_FOUND);
    let (bad_status, _, _) = http_get(server.addr(), "/api/banter-audio/..%2Fsecret").await;
    assert_eq!(bad_status, StatusCode::NOT_FOUND);

    // Trigger a second banter via quick emote -> cache capacity 1 evicts b-1 -> GET /api/banter-audio/b-1 returns 404!
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    let b2_url = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioReady {
                banter_id,
                audio_url,
                ..
            } if banter_id != "b-1" => break audio_url,
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg waiting for b-2 audio: {other:?}"),
        }
    };
    let (evicted_status, _, _) = http_get(server.addr(), "/api/banter-audio/b-1").await;
    assert_eq!(evicted_status, StatusCode::NOT_FOUND);
    let (fresh_status, _, fresh_body) = http_get(server.addr(), &b2_url).await;
    assert_eq!(fresh_status, StatusCode::OK);
    assert_eq!(fresh_body, expected_wav);

    let _ = east.close(None).await;
    let _ = west.close(None).await;
    let _ = north.close(None).await;
    server.shutdown().await;
    fake_tts.shutdown();
}

#[tokio::test]
async fn tts_url_precedence_honors_cli_over_preferences_and_no_tts_disables_all_calls() {
    let pref_tts = FakeTtsServer::start(make_pcm16_mono_wav(4_800, 0x11), 200).await;
    let cli_tts = FakeTtsServer::start(make_pcm16_mono_wav(4_800, 0x22), 200).await;

    let temp = TempDirGuard::new("majjj_tts_prefs");
    fs::write(
        temp.path().join("preferences.json"),
        serde_json::json!({
            "schema_version": 1,
            "recent_nicknames": [],
            "default_mode": "EastOnly",
            "default_seating": "KeepLobbySeats",
            "default_ai_difficulty": "Strong",
            "default_pace": "Standard",
            "tts_base_url": pref_tts.base_url()
        })
        .to_string(),
    )
    .expect("write preferences.json");

    // 1. preferences.json `tts_base_url` is used when CLI `--tts-url` is absent.
    let server1 = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_client(stub_llm_client())
        .spawn()
        .await
        .expect("spawn server1");
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        pref_tts.health_count() >= 1,
        "preferences.json tts_base_url must be probed on startup"
    );
    assert_eq!(cli_tts.health_count(), 0);
    server1.shutdown().await;

    // 2. Explicit `with_tts_url` overrides `preferences.json`.
    let prev_pref_count = pref_tts.health_count();
    let server2 = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(cli_tts.base_url()))
        .spawn()
        .await
        .expect("spawn server2");
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        cli_tts.health_count() >= 1,
        "explicit tts_url must override preferences.json"
    );
    assert_eq!(pref_tts.health_count(), prev_pref_count);
    server2.shutdown().await;

    // 3. `with_no_tts(true)` disables all TTS probes even when both `tts_url` and `preferences.json` exist,
    // and sends TtsStatusNotice(available=false) on Hello so clients auto-restore bubbles.
    let prev_cli_count = cli_tts.health_count();
    let server3 = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(cli_tts.base_url()))
        .with_no_tts(true)
        .spawn()
        .await
        .expect("spawn server3");
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(cli_tts.health_count(), prev_cli_count);
    assert_eq!(pref_tts.health_count(), prev_pref_count);

    let mut no_tts_client = connect_and_hello(server3.addr(), "u-no-tts", "老李").await;
    let disabled_notice = loop {
        match recv_msg(&mut no_tts_client).await {
            ServerMessage::TtsStatusNotice { available, message } => break (available, message),
            ServerMessage::LobbyUpdated { .. } | ServerMessage::ServerPing { .. } => continue,
            other => panic!("expected TtsStatusNotice(false) under --no-tts, got {other:?}"),
        }
    };
    assert!(!disabled_notice.0);
    assert_eq!(
        disabled_notice.1,
        "局域网语音合成已关闭，当前仅显示漫画气泡与离线角色语音包"
    );
    let _ = no_tts_client.close(None).await;
    server3.shutdown().await;

    pref_tts.shutdown();
    cli_tts.shutdown();
}

#[tokio::test]
async fn http_tts_client_decouples_request_timeout_from_deadline_and_ws_408_emits_fallback() {
    use mahjong_server::persona::{PersonaId, PersonaMood};
    use mahjong_server::tts::{HttpTtsClient, TtsClient, TtsSynthesisRequest};

    let expected_wav = make_pcm16_mono_wav(6_000, 0x3C);
    let fake_tts = FakeTtsServer::start(expected_wav.clone(), 500).await;

    // 1. HttpTtsClient uses its 8s request_timeout ceiling rather than clamping to request.deadline_ms.
    *fake_tts.state.synth_delay.lock().expect("lock delay") = Duration::from_millis(250);
    let client = HttpTtsClient::new(&fake_tts.base_url()).expect("create http tts client");
    let out = client
        .synthesize(TtsSynthesisRequest::new(
            "b-decoupled",
            PersonaId::QuickHothead,
            PersonaMood::Smug,
            "请求超时上限与deadline解耦验证",
            120,
        ))
        .await
        .expect("HttpTtsClient must not prematurely abort at deadline_ms=120");
    assert_eq!(out.wav_bytes, expected_wav);
    assert_eq!(out.duration_ms, 500);

    // 2. Over live WS, upstream HTTP 408 emits TableBanterAudioFallback without TtsStatusNotice,
    // and the following synthesis succeeds immediately.
    *fake_tts.state.synth_delay.lock().expect("lock delay") = Duration::ZERO;
    *fake_tts.state.return_408_once.lock().expect("lock 408") = true;

    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(fake_tts.base_url()))
        .spawn_with_options(
            TableOptions::with_wall_factory(23, no_reaction_factory())
                .with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server against local fake TTS");

    let mut east = connect_and_hello(server.addr(), "c-east", "小王").await;
    let mut west = connect_and_hello(server.addr(), "c-west", "小李").await;
    let mut north = connect_and_hello(server.addr(), "c-north", "小张").await;
    for (client_ws, seat) in [
        (&mut east, Seat::East),
        (&mut west, Seat::West),
        (&mut north, Seat::North),
    ] {
        send_msg(client_ws, &ClientMessage::JoinSeat { seat: Some(seat) }).await;
        let _ = recv_snapshot(client_ws).await;
    }
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let _ = recv_snapshot(&mut east).await;

    // Opening banter b-1 hits HTTP 408 -> client receives TableBanterAudioFallback("b-1"), NOT TtsStatusNotice.
    loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioFallback { banter_id, .. } if banter_id == "b-1" => {
                break;
            }
            ServerMessage::TtsStatusNotice { .. } => {
                panic!("HTTP 408 must NOT emit TtsStatusNotice");
            }
            ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg waiting for 408 fallback: {other:?}"),
        }
    }

    // Immediate next quick emote succeeds without cooldown -> TableBanterAudioReady!
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id != "b-1" => break,
            ServerMessage::TtsStatusNotice { .. } => {
                panic!("healthy synthesis after 408 must NOT emit TtsStatusNotice");
            }
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg waiting for post-408 audio ready: {other:?}"),
        }
    }

    let _ = east.close(None).await;
    let _ = west.close(None).await;
    let _ = north.close(None).await;
    server.shutdown().await;
    fake_tts.shutdown();
}

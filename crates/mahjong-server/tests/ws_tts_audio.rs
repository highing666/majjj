mod common;

use std::collections::VecDeque;
use std::convert::Infallible;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::{HeaderName, CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, SET_COOKIE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use common::{
    connect_and_hello, http_get, make_pcm16_mono_wav, no_reaction_wall, recv_msg, recv_snapshot,
    send_msg, stub_llm_client, TempDirGuard,
};
use futures_util::stream;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use mahjong_core::{PresetWallProvider, Seat};
use mahjong_server::banter_audio::BanterAudioCache;
use mahjong_server::protocol::{ClientMessage, QuickEmote, ServerMessage, TablePhase};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::tts::DEFAULT_TTS_SAMPLE_RATE;
use mahjong_server::{PacingMode, ServerConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

#[derive(Clone)]
struct FakeTtsState {
    healthy: Arc<Mutex<bool>>,
    wav_bytes: Arc<Mutex<Vec<u8>>>,
    pcm_bytes: Arc<Mutex<Vec<u8>>>,
    duration_ms: Arc<Mutex<u32>>,
    synth_delay: Arc<Mutex<Duration>>,
    return_408_once: Arc<Mutex<bool>>,
    gated_streams: Arc<Mutex<VecDeque<mpsc::Receiver<Bytes>>>>,
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
        let pcm_bytes = if wav_bytes.len() > 44 && &wav_bytes[0..4] == b"RIFF" {
            wav_bytes[44..].to_vec()
        } else {
            wav_bytes.clone()
        };
        let state = FakeTtsState {
            healthy: Arc::new(Mutex::new(true)),
            wav_bytes: Arc::new(Mutex::new(wav_bytes)),
            pcm_bytes: Arc::new(Mutex::new(pcm_bytes)),
            duration_ms: Arc::new(Mutex::new(duration_ms)),
            synth_delay: Arc::new(Mutex::new(Duration::ZERO)),
            return_408_once: Arc::new(Mutex::new(false)),
            gated_streams: Arc::new(Mutex::new(VecDeque::new())),
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

    fn push_gated_stream(&self, rx: mpsc::Receiver<Bytes>) {
        self.state
            .gated_streams
            .lock()
            .expect("lock gated streams")
            .push_back(rx);
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
    let is_stream = body["stream"].as_bool().unwrap_or(false);
    let format = body["format"].as_str().unwrap_or("wav").to_string();
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

    if is_stream && format == "pcm" {
        let gated_rx = state
            .gated_streams
            .lock()
            .expect("lock gated streams")
            .pop_front();
        let body = if let Some(rx) = gated_rx {
            let chunk_stream = stream::unfold(rx, |mut rx| async move {
                let bytes = rx.recv().await?;
                Some((Ok::<Bytes, Infallible>(bytes), rx))
            });
            Body::from_stream(chunk_stream)
        } else {
            let pcm = state.pcm_bytes.lock().expect("lock pcm").clone();
            let mid = (pcm.len() / 2) & !1;
            let chunks = if mid > 0 && mid < pcm.len() {
                vec![
                    Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&pcm[..mid])),
                    Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&pcm[mid..])),
                ]
            } else {
                vec![Ok::<Bytes, Infallible>(Bytes::from(pcm))]
            };
            Body::from_stream(stream::iter(chunks))
        };
        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        resp.headers_mut().insert(
            HeaderName::from_static("x-audio-format"),
            HeaderValue::from_static("pcm_s16le"),
        );
        resp.headers_mut().insert(
            HeaderName::from_static("x-audio-sample-rate"),
            HeaderValue::from_static("24000"),
        );
        resp.headers_mut().insert(
            HeaderName::from_static("x-audio-channels"),
            HeaderValue::from_static("1"),
        );
        return resp;
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

    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    let banter_id = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanter { banter_id, .. } => break banter_id,
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => continue,
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
    let expected_pcm = expected_wav[44..].to_vec();
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

    // Trigger b-1 via East's QuickEmote (only South is AI).
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;

    // Both clients receive TableBanter and TableBanterAudioReady for b-1.
    let mut east_ready = None;
    let mut west_ready = None;
    while east_ready.is_none() {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioReady {
                banter_id,
                audio_url,
                sample_rate,
                ..
            } => east_ready = Some((banter_id, audio_url, sample_rate)),
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
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
                sample_rate,
                ..
            } => west_ready = Some((banter_id, audio_url, sample_rate)),
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected west msg: {other:?}"),
        }
    }

    let (banter_id, audio_url, sample_rate) = east_ready.unwrap();
    assert_eq!(
        west_ready.unwrap(),
        (banter_id.clone(), audio_url.clone(), sample_rate)
    );
    assert_eq!(banter_id, "b-1");
    assert_eq!(audio_url, "/api/banter-audio/b-1");
    assert_eq!(sample_rate, DEFAULT_TTS_SAMPLE_RATE);

    // Upstream FakeTtsServer synthesized ONCE for both connected clients and received streaming PCM contract!
    let synth_reqs = fake_tts.recorded_synth_requests();
    assert_eq!(synth_reqs.len(), 1);
    assert_eq!(synth_reqs[0]["request_id"], "b-1");
    assert_eq!(synth_reqs[0]["format"], "pcm");
    assert_eq!(synth_reqs[0]["stream"], true);
    assert_eq!(synth_reqs[0]["deadline_ms"], 5000);

    // Both clients fetch GET /api/banter-audio/b-1 and receive exact PCM16 bytes + streaming headers.
    for _ in 0..2 {
        let (status, headers, body) = http_get(server.addr(), &audio_url).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()),
            Some("application/octet-stream")
        );
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            headers.get("x-audio-format").and_then(|v| v.to_str().ok()),
            Some("pcm_s16le")
        );
        assert_eq!(
            headers
                .get("x-audio-sample-rate")
                .and_then(|v| v.to_str().ok()),
            Some("24000")
        );
        assert_eq!(
            headers
                .get("x-audio-channels")
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
        assert_eq!(body, expected_pcm);
    }

    // Unknown and invalid/traversal IDs return 404.
    let (unknown_status, _, _) = http_get(server.addr(), "/api/banter-audio/b-9999").await;
    assert_eq!(unknown_status, StatusCode::NOT_FOUND);
    let (bad_status, _, _) = http_get(server.addr(), "/api/banter-audio/..%2Fsecret").await;
    assert_eq!(bad_status, StatusCode::NOT_FOUND);

    // Trigger a second banter via West's quick emote -> cache capacity 1 evicts b-1 -> GET /api/banter-audio/b-1 returns 404!
    send_msg(
        &mut west,
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
    assert_eq!(fresh_body, expected_pcm);

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

    // Trigger b-1 via East's quick emote -> hits HTTP 408 -> TableBanterAudioFallback("b-1"), NOT TtsStatusNotice.
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;
    loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioFallback { banter_id, .. } if banter_id == "b-1" => {
                break;
            }
            ServerMessage::TtsStatusNotice { .. } => {
                panic!("HTTP 408 must NOT emit TtsStatusNotice");
            }
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg waiting for 408 fallback: {other:?}"),
        }
    }

    // Immediate next quick emote (from West) succeeds without cooldown -> TableBanterAudioReady!
    send_msg(
        &mut west,
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

async fn open_http_stream(
    addr: SocketAddr,
    uri_path: &str,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, hyper::HeaderMap, hyper::body::Incoming) {
    let stream = TcpStream::connect(addr).await.expect("connect server");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("http1 handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let mut builder = hyper::Request::builder().method("GET").uri(uri_path);
    let mut has_host = false;
    for &(name, val) in extra_headers {
        if name.eq_ignore_ascii_case("host") {
            has_host = true;
        }
        builder = builder.header(name, val);
    }
    if !has_host {
        builder = builder.header(HOST, addr.to_string());
    }
    let req = builder
        .body(Full::new(Bytes::new()))
        .expect("build stream request");
    let res = sender.send_request(req).await.expect("send stream request");
    (res.status(), res.headers().clone(), res.into_body())
}

async fn read_next_data_chunk(body: &mut hyper::body::Incoming) -> Option<Bytes> {
    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    if !data.is_empty() {
                        return Some(data);
                    }
                }
            }
            Some(Err(err)) => panic!("unexpected stream frame error: {err}"),
            None => return None,
        }
    }
}

async fn read_exact_stream_bytes(body: &mut hyper::body::Incoming, expected_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected_len);
    while out.len() < expected_len {
        let chunk = tokio::time::timeout(Duration::from_secs(3), read_next_data_chunk(body))
            .await
            .expect("timed out waiting for live streamed PCM chunk")
            .expect("stream ended before expected byte count");
        out.extend_from_slice(&chunk);
    }
    out
}

async fn drain_stream_to_eof(body: &mut hyper::body::Incoming) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(3), read_next_data_chunk(body))
        .await
        .expect("timed out waiting for stream EOF")
    {
        out.extend_from_slice(&chunk);
    }
    out
}

async fn post_remote_auth_cookie(addr: SocketAddr, public_host: &str, passcode: &str) -> String {
    let stream = TcpStream::connect(addr).await.expect("connect remote auth");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("remote auth handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let payload = serde_json::json!({ "code": passcode }).to_string();
    let req = hyper::Request::builder()
        .method("POST")
        .uri("/api/remote-auth")
        .header(HOST, public_host)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(payload)))
        .expect("build remote auth request");
    let res = sender.send_request(req).await.expect("send remote auth");
    assert_eq!(res.status(), StatusCode::OK);
    let set_cookie = res
        .headers()
        .get(SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("Set-Cookie header on remote auth");
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .trim()
        .to_string()
}

#[tokio::test]
async fn streaming_fanout_delivers_live_chunked_pcm_to_early_and_late_clients_with_remote_cookie_auth(
) {
    let fake_tts = FakeTtsServer::start(make_pcm16_mono_wav(120, 0x10), 100).await;
    let (chunk_tx, chunk_rx) = mpsc::channel::<Bytes>(8);
    fake_tts.push_gated_stream(chunk_rx);

    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_allow_remote(true)
        .with_llm_client(stub_llm_client())
        .with_tts_url(Some(fake_tts.base_url()))
        .spawn_with_options(
            TableOptions::with_wall_factory(24, no_reaction_factory())
                .with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server with remote access and streaming fake TTS");

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

    // Trigger b-1 -> upstream HTTP request starts and blocks waiting on chunk_tx.
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;

    // Send ONLY chunk 1 -> server must immediately broadcast TableBanterAudioReady while stream is still live!
    let chunk1 = vec![0x11_u8; 640];
    let chunk2 = vec![0x22_u8; 640];
    let chunk3 = vec![0x33_u8; 640];
    chunk_tx
        .send(Bytes::from(chunk1.clone()))
        .await
        .expect("send chunk 1");

    let audio_url = loop {
        match recv_msg(&mut east).await {
            ServerMessage::TableBanterAudioReady {
                banter_id,
                audio_url,
                sample_rate,
                ..
            } => {
                assert_eq!(banter_id, "b-1");
                assert_eq!(sample_rate, DEFAULT_TTS_SAMPLE_RATE);
                break audio_url;
            }
            ServerMessage::QuickEmoteBroadcast { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg waiting for streaming audio ready: {other:?}"),
        }
    };

    // 1. Early local client opens GET /api/banter-audio/b-1 and receives chunk 1 LIVE before chunk 2 is sent!
    let (early_status, early_headers, mut early_body) =
        open_http_stream(server.addr(), &audio_url, &[]).await;
    assert_eq!(early_status, StatusCode::OK);
    assert_eq!(
        early_headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        early_headers
            .get(CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        early_headers
            .get("x-audio-format")
            .and_then(|v| v.to_str().ok()),
        Some("pcm_s16le")
    );
    assert_eq!(
        early_headers
            .get("x-audio-sample-rate")
            .and_then(|v| v.to_str().ok()),
        Some("24000")
    );
    assert_eq!(
        early_headers
            .get("x-audio-channels")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );

    let early_part1 = read_exact_stream_bytes(&mut early_body, chunk1.len()).await;
    assert_eq!(early_part1, chunk1);

    // 2. Send chunk 2 while stream is still live -> early client gets chunk 2 live.
    chunk_tx
        .send(Bytes::from(chunk2.clone()))
        .await
        .expect("send chunk 2");
    let early_part2 = read_exact_stream_bytes(&mut early_body, chunk2.len()).await;
    assert_eq!(early_part2, chunk2);

    // 3. Unauthenticated Remote client is rejected with 403 Forbidden.
    let public_host = format!("majjj.example.com:{}", server.addr().port());
    let (unauthed_status, _, _) =
        open_http_stream(server.addr(), &audio_url, &[(HOST.as_str(), &public_host)]).await;
    assert_eq!(unauthed_status, StatusCode::FORBIDDEN);

    // 4. Authenticated Remote client joins LATE (after chunk 1 & 2) and catches up from byte 0!
    let passcode = server
        .remote_access()
        .passcode()
        .expect("remote passcode enabled");
    let cookie = post_remote_auth_cookie(server.addr(), &public_host, &passcode).await;
    let (remote_status, remote_headers, mut remote_body) = open_http_stream(
        server.addr(),
        &audio_url,
        &[
            (HOST.as_str(), &public_host),
            (COOKIE.as_str(), cookie.as_str()),
        ],
    )
    .await;
    assert_eq!(remote_status, StatusCode::OK);
    assert_eq!(
        remote_headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    let remote_catchup =
        read_exact_stream_bytes(&mut remote_body, chunk1.len() + chunk2.len()).await;
    let mut expected_12 = Vec::new();
    expected_12.extend_from_slice(&chunk1);
    expected_12.extend_from_slice(&chunk2);
    assert_eq!(remote_catchup, expected_12);

    // 5. Send final chunk 3 and close upstream -> both early LAN and late Remote clients get chunk 3 + EOF!
    chunk_tx
        .send(Bytes::from(chunk3.clone()))
        .await
        .expect("send chunk 3");
    drop(chunk_tx);

    let early_tail = drain_stream_to_eof(&mut early_body).await;
    let remote_tail = drain_stream_to_eof(&mut remote_body).await;
    assert_eq!(early_tail, chunk3);
    assert_eq!(remote_tail, chunk3);

    // Single upstream synthesis served both concurrent clients!
    assert_eq!(fake_tts.recorded_synth_requests().len(), 1);

    // 6. Post-completion client reads full concatenated PCM16 from bounded LRU cache.
    let mut expected_all = expected_12;
    expected_all.extend_from_slice(&chunk3);
    let (cached_status, _, cached_body) = http_get(server.addr(), &audio_url).await;
    assert_eq!(cached_status, StatusCode::OK);
    assert_eq!(cached_body, expected_all);

    let _ = east.close(None).await;
    let _ = west.close(None).await;
    let _ = north.close(None).await;
    server.shutdown().await;
    fake_tts.shutdown();
}

//! Shared HTTP, WebSocket, temporary-directory, and tile-notation helpers for server tests.

use std::collections::VecDeque;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use mahjong_core::{build_preset_wall, PlayerAction, PlayerEvent, Tile};
use mahjong_server::llm::{
    BanterGenerationRequest, BanterOutput, LlmClient, LlmError, LlmFuture, LlmHealthStatus,
    MemoryNoteRequest,
};
use mahjong_server::persona::PersonaMood;
use mahjong_server::protocol::{ClientMessage, HealthResponse, ServerMessage, TableSnapshot};
use mahjong_server::session::{OutboundSignal, TableOptions, TableRuntime, WallProviderFactory};
use mahjong_server::{PacingMode, ServerConfig, ServerHandle};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type BanterResponder =
    Box<dyn Fn(&BanterGenerationRequest) -> Result<BanterOutput, LlmError> + Send + Sync>;
type MemoryNoteResponder =
    Box<dyn Fn(&MemoryNoteRequest) -> Result<String, LlmError> + Send + Sync>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum PlannedReply {
    Immediate(Result<BanterOutput, LlmError>),
    Delayed(Duration, Result<BanterOutput, LlmError>),
    Gated(Arc<Notify>, Result<BanterOutput, LlmError>),
}

#[allow(dead_code)]
pub struct ScriptedLlmClient {
    replies: Mutex<VecDeque<PlannedReply>>,
    banter_fallback: Option<BanterResponder>,
    banter_gate: Option<Arc<Notify>>,
    memory_note_handler: Option<MemoryNoteResponder>,
    captured_banter: Mutex<Vec<BanterGenerationRequest>>,
    captured_memory_notes: Mutex<Vec<MemoryNoteRequest>>,
}

impl std::fmt::Debug for ScriptedLlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedLlmClient").finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl ScriptedLlmClient {
    pub fn from_replies(replies: Vec<PlannedReply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            banter_fallback: None,
            banter_gate: None,
            memory_note_handler: None,
            captured_banter: Mutex::new(Vec::new()),
            captured_memory_notes: Mutex::new(Vec::new()),
        })
    }

    pub fn from_outputs(outputs: Vec<BanterOutput>) -> Arc<Self> {
        Self::from_replies(
            outputs
                .into_iter()
                .map(|output| PlannedReply::Immediate(Ok(output)))
                .collect(),
        )
    }

    pub fn with_banter_fn(
        banter_fn: impl Fn(&BanterGenerationRequest) -> Result<BanterOutput, LlmError>
            + Send
            + Sync
            + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(VecDeque::new()),
            banter_fallback: Some(Box::new(banter_fn)),
            banter_gate: None,
            memory_note_handler: None,
            captured_banter: Mutex::new(Vec::new()),
            captured_memory_notes: Mutex::new(Vec::new()),
        })
    }

    pub fn with_gated_banter_fn(
        gate: Arc<Notify>,
        banter_fn: impl Fn(&BanterGenerationRequest) -> Result<BanterOutput, LlmError>
            + Send
            + Sync
            + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(VecDeque::new()),
            banter_fallback: Some(Box::new(banter_fn)),
            banter_gate: Some(gate),
            memory_note_handler: None,
            captured_banter: Mutex::new(Vec::new()),
            captured_memory_notes: Mutex::new(Vec::new()),
        })
    }

    pub fn with_handlers(
        banter_fn: impl Fn(&BanterGenerationRequest) -> Result<BanterOutput, LlmError>
            + Send
            + Sync
            + 'static,
        note_fn: impl Fn(&MemoryNoteRequest) -> Result<String, LlmError> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(VecDeque::new()),
            banter_fallback: Some(Box::new(banter_fn)),
            banter_gate: None,
            memory_note_handler: Some(Box::new(note_fn)),
            captured_banter: Mutex::new(Vec::new()),
            captured_memory_notes: Mutex::new(Vec::new()),
        })
    }

    pub fn captured(&self) -> Vec<BanterGenerationRequest> {
        self.captured_banter
            .lock()
            .expect("lock captured banter")
            .clone()
    }

    pub fn clear_captured(&self) {
        self.captured_banter
            .lock()
            .expect("lock captured banter")
            .clear();
    }

    pub fn captured_memory_notes(&self) -> Vec<MemoryNoteRequest> {
        self.captured_memory_notes
            .lock()
            .expect("lock captured memory notes")
            .clone()
    }
}

impl LlmClient for ScriptedLlmClient {
    fn endpoint(&self) -> &str {
        "http://virtual-llm.local:8080"
    }

    fn health_check(&self) -> LlmFuture<'_, Result<LlmHealthStatus, LlmError>> {
        Box::pin(async {
            Ok(LlmHealthStatus {
                endpoint: "http://virtual-llm.local:8080".to_string(),
                model_id: "virtual-qwen".to_string(),
            })
        })
    }

    fn generate_banter(
        &self,
        request: BanterGenerationRequest,
    ) -> LlmFuture<'_, Result<BanterOutput, LlmError>> {
        let scripted = self.replies.lock().expect("lock replies").pop_front();
        let fallback_res = if scripted.is_none() {
            Some(match &self.banter_fallback {
                Some(handler) => handler(&request),
                None => Ok(BanterOutput {
                    mood: PersonaMood::Deadpan,
                    text: "默认补位吐槽".to_string(),
                }),
            })
        } else {
            None
        };
        let gate = self.banter_gate.clone();
        self.captured_banter
            .lock()
            .expect("lock captured banter")
            .push(request);

        Box::pin(async move {
            if let Some(next) = scripted {
                match next {
                    PlannedReply::Immediate(res) => res,
                    PlannedReply::Delayed(delay, res) => {
                        tokio::time::sleep(delay).await;
                        res
                    }
                    PlannedReply::Gated(notify, res) => {
                        notify.notified().await;
                        res
                    }
                }
            } else {
                if let Some(notify) = gate {
                    notify.notified().await;
                }
                fallback_res.expect("fallback computed when queue empty")
            }
        })
    }

    fn generate_memory_note(
        &self,
        request: MemoryNoteRequest,
    ) -> LlmFuture<'_, Result<String, LlmError>> {
        let res = match &self.memory_note_handler {
            Some(handler) => handler(&request),
            None => Ok(String::new()),
        };
        self.captured_memory_notes
            .lock()
            .expect("lock captured memory notes")
            .push(request);
        Box::pin(async move { res })
    }
}

#[allow(dead_code)]
pub struct VirtualClient {
    pub conn_id: u64,
    tx: mpsc::UnboundedSender<OutboundSignal>,
    rx: mpsc::UnboundedReceiver<OutboundSignal>,
}

#[allow(dead_code)]
impl VirtualClient {
    pub async fn connect(runtime: &TableRuntime, client_id: &str, nickname: &str) -> Self {
        let conn_id = runtime.allocate_conn_id().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let mut client = Self { conn_id, tx, rx };
        client
            .send(
                runtime,
                &ClientMessage::Hello {
                    client_id: client_id.to_string(),
                    nickname: nickname.to_string(),
                    table_id: None,
                    seat_token: None,
                },
            )
            .await;
        let _ = client.recv_snapshot().await;
        client
    }

    pub async fn send(&self, runtime: &TableRuntime, msg: &ClientMessage) {
        let raw = serde_json::to_string(msg).expect("serialize ClientMessage");
        let _ = runtime
            .handle_raw_text(self.conn_id, &raw, true, &self.tx)
            .await;
    }

    pub fn drain_all(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        while let Ok(sig) = self.rx.try_recv() {
            match sig {
                OutboundSignal::Message(m) => out.push(*m),
                OutboundSignal::ReplaceAndClose => out.push(ServerMessage::SessionReplaced),
            }
        }
        out
    }

    pub async fn recv_snapshot(&mut self) -> TableSnapshot {
        loop {
            match self.rx.recv().await.expect("outbound msg") {
                OutboundSignal::Message(msg) => match *msg {
                    ServerMessage::TableSnapshot(snap) => return snap,
                    ServerMessage::LobbyUpdated { .. }
                    | ServerMessage::TableBanter { .. }
                    | ServerMessage::TableBanterAudioReady { .. }
                    | ServerMessage::LlmStatusNotice { .. }
                    | ServerMessage::TtsStatusNotice { .. } => continue,
                    other => panic!("expected TableSnapshot, got {other:?}"),
                },
                OutboundSignal::ReplaceAndClose => panic!("unexpected SessionReplaced"),
            }
        }
    }
}

#[allow(dead_code)]
pub async fn yield_tasks() {
    for _ in 0..12 {
        tokio::task::yield_now().await;
    }
}

#[allow(dead_code)]
pub async fn pump_ai_turn() {
    for _ in 0..40 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct FastStubLlmClient;

impl LlmClient for FastStubLlmClient {
    fn endpoint(&self) -> &str {
        "http://stub-llm.local:8080"
    }

    fn health_check(&self) -> LlmFuture<'_, Result<LlmHealthStatus, LlmError>> {
        Box::pin(async {
            Ok(LlmHealthStatus {
                endpoint: "http://stub-llm.local:8080".to_string(),
                model_id: "stub-qwen".to_string(),
            })
        })
    }

    fn generate_banter(
        &self,
        _request: BanterGenerationRequest,
    ) -> LlmFuture<'_, Result<BanterOutput, LlmError>> {
        Box::pin(async {
            Ok(BanterOutput {
                mood: PersonaMood::Smug,
                text: "这招够狠，你们接着看！".to_string(),
            })
        })
    }
}

#[allow(dead_code)]
pub fn stub_llm_client() -> Arc<dyn LlmClient> {
    Arc::new(FastStubLlmClient)
}

#[allow(dead_code)]
pub type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub struct TempDirGuard {
    path: PathBuf,
}

#[allow(dead_code)]
impl TempDirGuard {
    pub fn new(prefix: &str) -> Self {
        let id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), id));
        if path.exists() {
            fs::remove_dir_all(&path).expect("clean stale temp dir");
        }
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[allow(dead_code)]
pub async fn spawn_server(static_dir: Option<PathBuf>) -> ServerHandle {
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_static_dir(static_dir)
        .with_llm_client(stub_llm_client())
        .with_no_tts(true)
        .spawn()
        .await
        .expect("spawn server")
}

#[allow(dead_code)]
pub async fn spawn_seeded_server(seed: u64, static_dir: Option<PathBuf>) -> ServerHandle {
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_static_dir(static_dir)
        .with_llm_client(stub_llm_client())
        .with_no_tts(true)
        .spawn_with_options(TableOptions::seeded(seed).with_pacing(PacingMode::Immediate))
        .await
        .expect("spawn seeded server")
}

#[allow(dead_code)]
pub async fn spawn_with_wall_factory(seed: u64, factory: WallProviderFactory) -> ServerHandle {
    ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_llm_client(stub_llm_client())
        .with_no_tts(true)
        .spawn_with_options(
            TableOptions::with_wall_factory(seed, factory).with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server with wall factory")
}

#[allow(dead_code)]
pub async fn http_get(addr: SocketAddr, uri_path: &str) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    http_request_with_body("GET", addr, uri_path, None).await
}

#[allow(dead_code)]
pub async fn http_post(
    addr: SocketAddr,
    uri_path: &str,
) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    http_request_with_body("POST", addr, uri_path, None).await
}

#[allow(dead_code)]
pub async fn http_post_raw(
    addr: SocketAddr,
    uri_path: &str,
    body: Option<&[u8]>,
) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    http_request_with_body("POST", addr, uri_path, body).await
}

#[allow(dead_code)]
pub async fn http_request_with_body(
    method: &str,
    addr: SocketAddr,
    uri_path: &str,
    body: Option<&[u8]>,
) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    let stream = TcpStream::connect(addr).await.expect("connect to server");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("http1 handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let payload = body.map(Bytes::copy_from_slice).unwrap_or_default();
    let mut builder = Request::builder()
        .method(method)
        .uri(uri_path)
        .header("Host", addr.to_string());
    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    let req = builder.body(Full::new(payload)).expect("build request");
    let res = sender.send_request(req).await.expect("send request");
    let status = res.status();
    let headers = res.headers().clone();
    let body = res
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

#[allow(dead_code)]
pub async fn fetch_health(addr: SocketAddr) -> HealthResponse {
    let (status, headers, body) = http_get(addr, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.contains("application/json"),
        "unexpected /api/health content-type: {content_type}"
    );
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    serde_json::from_slice(&body).expect("valid HealthResponse JSON")
}

#[allow(dead_code)]
pub async fn connect_ws(addr: SocketAddr) -> WsClient {
    let url = format!("ws://{addr}/ws");
    let (ws, _) = connect_async(&url).await.expect("connect websocket");
    ws
}

#[allow(dead_code)]
pub async fn send_msg(ws: &mut WsClient, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).expect("serialize ClientMessage");
    ws.send(WsMessage::Text(text.into()))
        .await
        .expect("send ws message");
}

#[allow(dead_code)]
pub async fn recv_msg(ws: &mut WsClient) -> ServerMessage {
    let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("timed out waiting for server ws message")
        .expect("ws stream ended unexpectedly")
        .expect("ws frame error");
    match frame {
        WsMessage::Text(text) => {
            serde_json::from_str::<ServerMessage>(&text).expect("valid ServerMessage JSON")
        }
        other => panic!("expected text frame, got {other:?}"),
    }
}

#[allow(dead_code)]
pub async fn recv_non_ping_msg(ws: &mut WsClient) -> ServerMessage {
    loop {
        match recv_msg(ws).await {
            ServerMessage::ServerPing { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::LlmStatusNotice { .. }
            | ServerMessage::TtsStatusNotice { .. } => continue,
            other => return other,
        }
    }
}

#[allow(dead_code)]
pub async fn recv_snapshot(ws: &mut WsClient) -> TableSnapshot {
    loop {
        match recv_non_ping_msg(ws).await {
            ServerMessage::TableSnapshot(snap) => return snap,
            ServerMessage::LobbyUpdated { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::LlmStatusNotice { .. }
            | ServerMessage::TtsStatusNotice { .. } => continue,
            other => panic!("expected TableSnapshot, got {other:?}"),
        }
    }
}

#[allow(dead_code)]
pub async fn recv_event_batch(
    ws: &mut WsClient,
) -> (u64, u64, Vec<PlayerEvent>, Vec<PlayerAction>) {
    loop {
        match recv_non_ping_msg(ws).await {
            ServerMessage::EventBatch {
                from_seq,
                to_seq,
                events,
                updated_legal_actions,
            } => return (from_seq, to_seq, events, updated_legal_actions),
            ServerMessage::LobbyUpdated { .. }
            | ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::LlmStatusNotice { .. }
            | ServerMessage::TtsStatusNotice { .. } => continue,
            other => panic!("expected EventBatch, got {other:?}"),
        }
    }
}

#[allow(dead_code)]
pub fn no_reaction_wall() -> Vec<Tile> {
    let base_13 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N");
    let mut east_14 = base_13.clone();
    east_14.push(tile("C"));
    build_preset_wall(&east_14, &base_13, &base_13, &base_13, &[], &[]).expect("valid 136 wall")
}

#[allow(dead_code)]
pub async fn connect_and_hello(addr: SocketAddr, client_id: &str, nickname: &str) -> WsClient {
    let mut ws = connect_ws(addr).await;
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
    let _ = recv_snapshot(&mut ws).await;
    ws
}

/// Parses compact whitespace-separated mahjong notation into a `Vec<Tile>`.
///
/// Supported tokens:
/// - Number suits (`1`..`9` followed by `m` = 万, `s` = 条, `p` = 筒), e.g. `"111m 234s 55p"`
/// - Letter honors (`E`/`S`/`W`/`N` winds, `C`/`F`/`B` dragons), e.g. `"EE CFB"`
/// - Numeric honors (`1`..`7` followed by `z` mapping `1z=E`..`7z=B`), e.g. `"1155z"`
#[allow(dead_code)]
pub fn tiles(spec: &str) -> Vec<Tile> {
    let mut out = Vec::new();
    for token in spec.split_whitespace() {
        parse_token(token, &mut out);
    }
    out
}

#[allow(dead_code)]
pub fn tile(token: &str) -> Tile {
    let parsed = tiles(token);
    assert_eq!(
        parsed.len(),
        1,
        "expected exactly 1 tile from token {token:?}, got {}",
        parsed.len()
    );
    parsed[0]
}

fn parse_token(token: &str, out: &mut Vec<Tile>) {
    let bytes = token.as_bytes();
    if let Some((&suffix, digits)) = bytes.split_last() {
        if matches!(suffix, b'm' | b's' | b'p' | b'z') && !digits.is_empty() {
            for &d in digits {
                let raw = match (suffix, d) {
                    (b'm', b'1'..=b'9') => d - b'1',
                    (b's', b'1'..=b'9') => 9 + (d - b'1'),
                    (b'p', b'1'..=b'9') => 18 + (d - b'1'),
                    (b'z', b'1'..=b'7') => 27 + (d - b'1'),
                    _ => panic!("invalid digit {:?} in tile token {token:?}", d as char),
                };
                out.push(Tile::new(raw).expect("validated tile index in 0..34"));
            }
            return;
        }
    }

    for &ch in bytes {
        let raw = match ch {
            b'E' => 27,
            b'S' => 28,
            b'W' => 29,
            b'N' => 30,
            b'C' => 31,
            b'F' => 32,
            b'B' => 33,
            _ => panic!(
                "invalid honor tile char {:?} in token {token:?}",
                ch as char
            ),
        };
        out.push(Tile::new(raw).expect("validated honor index in 27..34"));
    }
}

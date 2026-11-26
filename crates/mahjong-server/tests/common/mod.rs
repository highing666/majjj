//! Shared HTTP, WebSocket, temporary-directory, and tile-notation helpers for server tests.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use mahjong_core::{build_preset_wall, PlayerAction, PlayerEvent, Tile};
use mahjong_server::protocol::{ClientMessage, HealthResponse, ServerMessage, TableSnapshot};
use mahjong_server::session::{TableOptions, WallProviderFactory};
use mahjong_server::{PacingMode, ServerConfig, ServerHandle};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

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
    ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir,
        ..ServerConfig::default()
    }
    .spawn()
    .await
    .expect("spawn server")
}

#[allow(dead_code)]
pub async fn spawn_seeded_server(seed: u64, static_dir: Option<PathBuf>) -> ServerHandle {
    ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir,
        ..ServerConfig::default()
    }
    .spawn_with_options(TableOptions::seeded(seed).with_pacing(PacingMode::Immediate))
    .await
    .expect("spawn seeded server")
}

#[allow(dead_code)]
pub async fn spawn_with_wall_factory(seed: u64, factory: WallProviderFactory) -> ServerHandle {
    ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir: None,
        ..ServerConfig::default()
    }
    .spawn_with_options(
        TableOptions::with_wall_factory(seed, factory).with_pacing(PacingMode::Immediate),
    )
    .await
    .expect("spawn server with wall factory")
}

#[allow(dead_code)]
pub async fn http_get(addr: SocketAddr, uri_path: &str) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    http_request("GET", addr, uri_path).await
}

#[allow(dead_code)]
pub async fn http_post(
    addr: SocketAddr,
    uri_path: &str,
) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    http_request("POST", addr, uri_path).await
}

async fn http_request(
    method: &str,
    addr: SocketAddr,
    uri_path: &str,
) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    let stream = TcpStream::connect(addr).await.expect("connect to server");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("http1 handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method(method)
        .uri(uri_path)
        .header("Host", addr.to_string())
        .body(http_body_util::Empty::<hyper::body::Bytes>::new())
        .expect("build request");
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
            ServerMessage::ServerPing { .. } => continue,
            other => return other,
        }
    }
}

#[allow(dead_code)]
pub async fn recv_snapshot(ws: &mut WsClient) -> TableSnapshot {
    loop {
        match recv_non_ping_msg(ws).await {
            ServerMessage::TableSnapshot(snap) => return snap,
            ServerMessage::LobbyUpdated { .. } => continue,
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
            ServerMessage::LobbyUpdated { .. } => continue,
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

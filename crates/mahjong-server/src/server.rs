//! Axum HTTP/WebSocket router, server lifecycle, and health/static endpoints.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::ai_pool::default_ai_threads;
use crate::history_dto::ClearHistoryRequest;
use crate::lan::read_local_hostname;
use crate::protocol::{HealthResponse, ServerMessage};
use crate::session::{OutboundSignal, TableOptions, TableRuntime};
use crate::static_assets::{
    DiskStaticAssets, NoopStaticAssets, StaticAssetError, StaticAssetSource,
};

const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(10);
const DEFAULT_ACTIVITY_TIMEOUT: Duration = Duration::from_secs(25);
const WS_CLOSE_FLUSH_TIMEOUT: Duration = Duration::from_millis(200);

/// WebSocket application-layer heartbeat timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatConfig {
    /// Interval between server-initiated [`ServerMessage::ServerPing`] frames (default `10s`).
    pub ping_interval: Duration,
    /// Duration without valid inbound activity before closing the connection (default `25s`).
    pub activity_timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            ping_interval: DEFAULT_PING_INTERVAL,
            activity_timeout: DEFAULT_ACTIVITY_TIMEOUT,
        }
    }
}

/// Configuration for starting `mahjong-server`.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// IP address to bind (defaults to `0.0.0.0`).
    pub bind: IpAddr,
    /// TCP port to bind (defaults to `8789`; `0` may be passed explicitly in tests).
    pub port: u16,
    /// Optional built frontend static asset directory.
    pub static_dir: Option<PathBuf>,
    /// Optional persistent directory for `hands.jsonl`, `matches.jsonl`, and `preferences.json`.
    pub data_dir: Option<PathBuf>,
    /// Worker thread count for the dedicated Rayon AI thinking pool (defaults to [`default_ai_threads`]).
    pub ai_threads: usize,
    /// WebSocket application-layer ping interval and inbound activity timeout.
    pub heartbeat: HeartbeatConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8789,
            static_dir: None,
            data_dir: None,
            ai_threads: default_ai_threads(),
            heartbeat: HeartbeatConfig::default(),
        }
    }
}

/// Errors returned while binding or initializing the server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// TCP listener could not bind the configured address/port.
    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    /// Configured static directory failed validation or canonicalization.
    #[error("invalid static directory {path}: {source}")]
    StaticDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Configured persistent data directory could not be initialized.
    #[error("invalid data directory {path}: {source}")]
    DataDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Configured AI thread pool size is invalid (e.g. `0`).
    #[error("invalid AI thread count {0}: must be at least 1")]
    InvalidAiThreads(usize),
    /// Dedicated Rayon AI thread pool could not be constructed.
    #[error("failed to initialize AI thread pool: {0}")]
    AiPool(String),
}

/// Running server handle returned by [`ServerConfig::spawn`].
pub struct ServerHandle {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: JoinHandle<Result<(), std::io::Error>>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl ServerHandle {
    /// Returns the actual bound socket address.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Gracefully shuts down the running server and awaits task completion.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.join_handle.await;
    }
}

#[derive(Clone)]
struct AppState {
    port: u16,
    session: TableRuntime,
    static_assets: Arc<dyn StaticAssetSource>,
    heartbeat: HeartbeatConfig,
}

impl ServerConfig {
    /// Overrides the WebSocket heartbeat send interval and inbound activity timeout.
    #[must_use]
    pub fn with_heartbeat(mut self, heartbeat: HeartbeatConfig) -> Self {
        self.heartbeat = heartbeat;
        self
    }

    /// Overrides the persistent data directory for history and preferences.
    #[must_use]
    pub fn with_data_dir(mut self, data_dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(data_dir.into());
        self
    }

    /// Binds the TCP socket immediately (failing on port conflicts without fallback) and
    /// spawns the Axum HTTP/WebSocket server on the current Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::StaticDir`] if `static_dir` cannot be opened, or
    /// [`ServerError::Bind`] if binding `bind:port` fails.
    pub async fn spawn(self) -> Result<ServerHandle, ServerError> {
        self.spawn_with_options(TableOptions::default()).await
    }

    /// Binds the TCP socket immediately and spawns the server with custom [`TableOptions`]
    /// (e.g. deterministic wall and seating seeds in tests).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidAiThreads`] if `ai_threads == 0`,
    /// [`ServerError::AiPool`] if the AI thread pool cannot be constructed,
    /// [`ServerError::StaticDir`] if `static_dir` cannot be opened, or
    /// [`ServerError::Bind`] if binding `bind:port` fails.
    pub async fn spawn_with_options(
        self,
        mut options: TableOptions,
    ) -> Result<ServerHandle, ServerError> {
        if self.ai_threads == 0 {
            return Err(ServerError::InvalidAiThreads(0));
        }
        if options.data_dir().is_none() {
            if let Some(dir) = &self.data_dir {
                options = options.with_data_dir(dir.clone());
            }
        }

        let static_assets: Arc<dyn StaticAssetSource> =
            match &self.static_dir {
                Some(dir) => Arc::new(DiskStaticAssets::new(dir).map_err(|source| {
                    ServerError::StaticDir {
                        path: dir.clone(),
                        source,
                    }
                })?),
                None => Arc::new(NoopStaticAssets),
            };

        let bind_addr = SocketAddr::new(self.bind, self.port);
        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|source| ServerError::Bind {
                addr: bind_addr,
                source,
            })?;
        let local_addr = listener.local_addr().map_err(|source| ServerError::Bind {
            addr: bind_addr,
            source,
        })?;

        let local_hostname = tokio::task::spawn_blocking(read_local_hostname)
            .await
            .ok()
            .flatten();

        let session = TableRuntime::try_with_options_and_threads(
            local_addr.port(),
            local_hostname,
            options,
            self.ai_threads,
        )?;
        let state = AppState {
            port: local_addr.port(),
            session,
            static_assets,
            heartbeat: self.heartbeat,
        };
        let router = build_router(state);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let join_handle = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        Ok(ServerHandle {
            addr: local_addr,
            shutdown_tx: Some(shutdown_tx),
            join_handle,
        })
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/history/export", get(history_export_handler))
        .route("/api/history/clear", post(history_clear_handler))
        .route("/api/{*rest}", any(api_not_found))
        .route("/api", any(api_not_found))
        .route("/ws", get(ws_handler))
        .fallback(get(static_asset_handler))
        .with_state(state)
}

fn json_with_no_store<T: Serialize>(status: StatusCode, payload: &T) -> Response {
    let mut response = (status, Json(payload)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn health_handler(State(state): State<AppState>) -> Response {
    let (phase, online_humans) = state.session.health_snapshot().await;
    let payload = HealthResponse {
        app: "majjj".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        phase,
        online_humans,
        port: state.port,
    };
    json_with_no_store(StatusCode::OK, &payload)
}

async fn stats_handler(State(state): State<AppState>) -> Response {
    let payload = state.session.history_stats().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn history_export_handler(State(state): State<AppState>) -> Response {
    let payload = state.session.history_export().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn history_clear_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let Ok(req) = serde_json::from_slice::<ClearHistoryRequest>(&body) else {
        return json_with_no_store(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": "expected JSON body { \"confirm\": true }" }),
        );
    };
    if !req.confirm {
        return json_with_no_store(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": "confirm must be true" }),
        );
    }
    match state.session.history_clear().await {
        Ok(resp) => json_with_no_store(StatusCode::OK, &resp),
        Err(err) => json_with_no_store(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "error": format!("failed to clear history: {err}") }),
        ),
    }
}

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    let host_is_loopback = compute_host_is_loopback(Some(peer), &headers);
    let runtime = state.session.clone();
    let heartbeat = state.heartbeat;
    ws.on_upgrade(move |socket| handle_ws_connection(socket, runtime, host_is_loopback, heartbeat))
}

async fn handle_ws_connection(
    socket: WebSocket,
    runtime: TableRuntime,
    host_is_loopback: bool,
    heartbeat: HeartbeatConfig,
) {
    let conn_id = runtime.allocate_conn_id().await;
    let (tx, mut rx) = mpsc::unbounded_channel::<OutboundSignal>();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let ping_period = heartbeat.ping_interval.max(Duration::from_millis(1));
    let activity_timeout = heartbeat.activity_timeout.max(Duration::from_millis(1));
    let mut ping_ticker = tokio::time::interval_at(Instant::now() + ping_period, ping_period);
    ping_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let timeout_sleep = tokio::time::sleep_until(Instant::now() + activity_timeout);
    tokio::pin!(timeout_sleep);

    loop {
        tokio::select! {
            biased;
            outbound = rx.recv() => {
                match outbound {
                    Some(OutboundSignal::Message(msg)) => {
                        if send_server_message(&mut ws_tx, &msg).await.is_err() {
                            break;
                        }
                    }
                    Some(OutboundSignal::ReplaceAndClose) => {
                        let _ = send_server_message(&mut ws_tx, &ServerMessage::SessionReplaced).await;
                        close_ws_gracefully(&mut ws_tx).await;
                        break;
                    }
                    None => break,
                }
            }
            inbound = ws_rx.next() => {
                match inbound {
                    Some(Ok(WsMessage::Text(text))) => {
                        let outcome = runtime
                            .handle_raw_text(conn_id, text.as_str(), host_is_loopback, &tx)
                            .await;
                        if outcome.valid_inbound_message {
                            timeout_sleep.as_mut().reset(Instant::now() + activity_timeout);
                        }
                        if outcome.close_socket {
                            close_ws_gracefully(&mut ws_tx).await;
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        timeout_sleep.as_mut().reset(Instant::now() + activity_timeout);
                        if ws_tx.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        timeout_sleep.as_mut().reset(Instant::now() + activity_timeout);
                    }
                    Some(Ok(WsMessage::Binary(_))) => {
                        let _ = tx.send(OutboundSignal::Message(Box::new(
                            ServerMessage::ErrorNotice {
                                message: "binary WebSocket frames are not supported".to_string(),
                            },
                        )));
                    }
                    Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
                }
            }
            _ = ping_ticker.tick() => {
                let ping = ServerMessage::ServerPing {
                    ts_ms: current_unix_ms(),
                };
                if send_server_message(&mut ws_tx, &ping).await.is_err() {
                    break;
                }
            }
            _ = &mut timeout_sleep => {
                close_ws_gracefully(&mut ws_tx).await;
                break;
            }
        }
    }

    runtime.on_disconnect(conn_id).await;
}

async fn close_ws_gracefully(ws_tx: &mut futures_util::stream::SplitSink<WebSocket, WsMessage>) {
    let _ = tokio::time::timeout(WS_CLOSE_FLUSH_TIMEOUT, async {
        let _ = ws_tx.send(WsMessage::Close(None)).await;
        let _ = ws_tx.close().await;
    })
    .await;
}

fn current_unix_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

async fn send_server_message(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, WsMessage>,
    msg: &ServerMessage,
) -> Result<(), ()> {
    let Ok(json) = serde_json::to_string(msg) else {
        return Err(());
    };
    ws_tx
        .send(WsMessage::Text(json.into()))
        .await
        .map_err(|_| ())
}

fn compute_host_is_loopback(peer: Option<SocketAddr>, headers: &HeaderMap) -> bool {
    let peer_loopback = peer.is_some_and(|addr| addr.ip().to_canonical().is_loopback());
    if !peer_loopback {
        return false;
    }
    if let Some(host_val) = headers.get(HOST).and_then(|v| v.to_str().ok()) {
        return is_loopback_host_header(host_val);
    }
    true
}

fn is_loopback_host_header(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            let inner = &trimmed[1..end];
            return inner == "::1";
        }
        return false;
    }
    let host_only = match trimmed.rsplit_once(':') {
        Some((h, port)) if !h.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => trimmed,
    };
    host_only.eq_ignore_ascii_case("localhost") || host_only == "127.0.0.1" || host_only == "::1"
}

async fn static_asset_handler(State(state): State<AppState>, uri: Uri) -> Response {
    let uri_path = uri.path().to_owned();
    let source = Arc::clone(&state.static_assets);
    let resolved = tokio::task::spawn_blocking(move || source.resolve(&uri_path)).await;

    match resolved {
        Ok(Ok(Some(asset))) => {
            let mut resp = (StatusCode::OK, asset.bytes).into_response();
            resp.headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(asset.content_type));
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static(asset.cache_control));
            resp
        }
        Ok(Ok(None)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(StaticAssetError::InvalidPath)) => StatusCode::BAD_REQUEST.into_response(),
        Ok(Err(StaticAssetError::Io(_))) | Err(_) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

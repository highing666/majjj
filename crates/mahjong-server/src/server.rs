//! Axum HTTP/WebSocket router, server lifecycle, and health/static endpoints.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::extract::{ConnectInfo, Path, Request, State, WebSocketUpgrade};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::ai_pool::default_ai_threads;
use crate::banter_audio::is_valid_banter_id;
use crate::history_dto::ClearHistoryRequest;
use crate::lan::read_local_hostname;
use crate::lifecycle::{
    is_loopback_peer, spawn_idle_shutdown_monitor, IdleTimeouts, WsConnectionTracker,
};
use crate::llm::{resolve_llm_base_url, LlmClient, OpenAiLlmClient};
use crate::persona_memory::ClearPersonaMemoryRequest;
use crate::protocol::{HealthResponse, ServerMessage};
use crate::public_url::{normalize_public_url, PublicUrlError};
use crate::session::{OutboundSignal, TableOptions, TableRuntime};
use crate::static_assets::{
    DiskStaticAssets, EmbeddedStaticAssets, StaticAssetError, StaticAssetSource,
};
use crate::storage::{current_unix_ms, load_preferences_for_dir};
use crate::tts::{resolve_tts_base_url, HttpTtsClient, TtsClient};

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
#[non_exhaustive]
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
    /// Skip opening the local browser automatically on startup (desktop mode).
    pub no_open: bool,
    /// Run in 7x24 headless mode (implies `no_open`, disables `POST /api/shutdown` and idle auto-exit).
    pub headless: bool,
    /// Split idle auto-exit thresholds for desktop mode (`!headless`).
    pub idle_timeouts: IdleTimeouts,
    /// Optional explicit LAN `llama-server` base URL override (`--llm-url` / `MAJJJ_LLM_URL`).
    pub llm_url: Option<String>,
    /// Optional injected [`LlmClient`] for automated tests (bypasses constructing `OpenAiLlmClient`).
    pub llm_client: Option<Arc<dyn LlmClient>>,
    /// Optional explicit LAN emotional TTS base URL override (`--tts-url` / `MAJJJ_TTS_URL`).
    pub tts_url: Option<String>,
    /// Explicitly disables LAN emotional TTS synthesis (`--no-tts` / `MAJJJ_NO_TTS`).
    pub no_tts: bool,
    /// Optional injected [`TtsClient`] for automated tests (bypasses constructing `HttpTtsClient`).
    pub tts_client: Option<Arc<dyn TtsClient>>,
    /// Optional explicit custom public origin override (`--public-url` / `MAJJJ_PUBLIC_URL`).
    pub public_url: Option<String>,
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
            no_open: false,
            headless: false,
            idle_timeouts: IdleTimeouts::default(),
            llm_url: None,
            llm_client: None,
            tts_url: None,
            no_tts: false,
            tts_client: None,
            public_url: None,
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
    /// Mandatory LAN LLM inference server failed startup reachability or model readiness probe.
    #[error(
        "无法连接局域网推理服务（{endpoint}）：{reason}。请确认 Windows 主机上的 llama-server 已启动且局域网可达，或通过 --llm-url / MAJJJ_LLM_URL 指定正确地址"
    )]
    LlmUnavailable { endpoint: String, reason: String },
    /// Configured custom public origin (`--public-url` / `MAJJJ_PUBLIC_URL`) failed strict validation.
    #[error("自定义公网入口配置无效：{0}")]
    InvalidPublicUrl(#[from] PublicUrlError),
}

/// Running server handle returned by [`ServerConfig::spawn`].
pub struct ServerHandle {
    addr: SocketAddr,
    cli_public_url: Option<String>,
    shutdown_tx: watch::Sender<bool>,
    join_handle: Option<JoinHandle<Result<(), std::io::Error>>>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("addr", &self.addr)
            .field("cli_public_url", &self.cli_public_url)
            .finish_non_exhaustive()
    }
}

impl ServerHandle {
    /// Returns the actual bound socket address.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the validated CLI/ENV custom public origin override, if configured.
    #[must_use]
    pub fn cli_public_url(&self) -> Option<&str> {
        self.cli_public_url.as_deref()
    }

    /// Waits until the server stops on its own (e.g. via `POST /api/shutdown` or idle auto-exit).
    ///
    /// # Errors
    ///
    /// Returns any I/O error reported by `axum::serve` or the spawned server task.
    pub async fn wait(&mut self) -> Result<(), std::io::Error> {
        let Some(jh) = self.join_handle.as_mut() else {
            return Ok(());
        };
        let joined = jh.await;
        self.join_handle = None;
        match joined {
            Ok(res) => res,
            Err(err) => Err(std::io::Error::other(err)),
        }
    }

    /// Gracefully shuts down the running server and awaits task completion.
    pub async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.await;
        }
    }
}

#[derive(Clone)]
struct AppState {
    port: u16,
    headless: bool,
    session: TableRuntime,
    static_assets: Arc<dyn StaticAssetSource>,
    heartbeat: HeartbeatConfig,
    shutdown_tx: watch::Sender<bool>,
    ws_tracker: WsConnectionTracker,
}

impl ServerConfig {
    /// Overrides the IP address to bind.
    #[must_use]
    pub fn with_bind(mut self, bind: IpAddr) -> Self {
        self.bind = bind;
        self
    }

    /// Overrides the TCP port to bind (`0` selects an ephemeral port in tests).
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Overrides the optional external static asset directory.
    #[must_use]
    pub fn with_static_dir(mut self, static_dir: impl Into<Option<PathBuf>>) -> Self {
        self.static_dir = static_dir.into();
        self
    }

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

    /// Overrides the Rayon AI pool worker thread count.
    #[must_use]
    pub fn with_ai_threads(mut self, ai_threads: usize) -> Self {
        self.ai_threads = ai_threads;
        self
    }

    /// Skips automatically opening the browser on startup in desktop mode.
    #[must_use]
    pub fn with_no_open(mut self, no_open: bool) -> Self {
        self.no_open = no_open;
        self
    }

    /// Enables or disables 7x24 headless mode (`headless` also disables `/api/shutdown` and idle exit).
    #[must_use]
    pub fn with_headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        if headless {
            self.no_open = true;
        }
        self
    }

    /// Overrides the split desktop idle auto-exit thresholds.
    #[must_use]
    pub fn with_idle_timeouts(mut self, idle_timeouts: IdleTimeouts) -> Self {
        self.idle_timeouts = idle_timeouts;
        self
    }

    /// Overrides the LAN `llama-server` base URL (`--llm-url` / `MAJJJ_LLM_URL`).
    #[must_use]
    pub fn with_llm_url(mut self, llm_url: impl Into<Option<String>>) -> Self {
        self.llm_url = llm_url.into();
        self
    }

    /// Injects a custom [`LlmClient`] implementation (primarily for offline automated tests).
    #[must_use]
    pub fn with_llm_client(mut self, llm_client: Arc<dyn LlmClient>) -> Self {
        self.llm_client = Some(llm_client);
        self
    }

    /// Overrides the optional LAN emotional TTS base URL (`--tts-url` / `MAJJJ_TTS_URL`).
    #[must_use]
    pub fn with_tts_url(mut self, tts_url: impl Into<Option<String>>) -> Self {
        self.tts_url = tts_url.into();
        self
    }

    /// Explicitly disables LAN emotional TTS synthesis (`--no-tts` / `MAJJJ_NO_TTS`).
    #[must_use]
    pub fn with_no_tts(mut self, no_tts: bool) -> Self {
        self.no_tts = no_tts;
        self
    }

    /// Injects a custom [`TtsClient`] implementation (primarily for automated tests).
    #[must_use]
    pub fn with_tts_client(mut self, tts_client: Arc<dyn TtsClient>) -> Self {
        self.tts_client = Some(tts_client);
        self
    }

    /// Overrides the custom public origin for remote invitation links (`--public-url` / `MAJJJ_PUBLIC_URL`).
    #[must_use]
    pub fn with_public_url(mut self, public_url: impl Into<Option<String>>) -> Self {
        self.public_url = public_url.into();
        self
    }

    /// Binds the TCP socket immediately (failing on port conflicts without fallback) and
    /// spawns the Axum HTTP/WebSocket server on the current Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::StaticDir`] if `static_dir` cannot be opened,
    /// [`ServerError::Bind`] if binding `bind:port` fails, or
    /// [`ServerError::LlmUnavailable`] if the mandatory LAN LLM health probe fails.
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
    /// [`ServerError::StaticDir`] if `static_dir` cannot be opened,
    /// [`ServerError::Bind`] if binding `bind:port` fails, or
    /// [`ServerError::LlmUnavailable`] if the mandatory LAN LLM health probe fails.
    pub async fn spawn_with_options(
        self,
        mut options: TableOptions,
    ) -> Result<ServerHandle, ServerError> {
        if self.ai_threads == 0 {
            return Err(ServerError::InvalidAiThreads(0));
        }
        let cli_public_url = match self.public_url.as_deref() {
            Some(raw) => Some(normalize_public_url(raw)?),
            None => None,
        };
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
                None => Arc::new(EmbeddedStaticAssets),
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

        let (persisted_llm_url, persisted_tts_url) = match options.data_dir() {
            Some(dir) => {
                let prefs =
                    load_preferences_for_dir(dir).map_err(|source| ServerError::DataDir {
                        path: dir.to_path_buf(),
                        source,
                    })?;
                (prefs.llm_base_url, prefs.tts_base_url)
            }
            None => (None, None),
        };

        let llm_client: Arc<dyn LlmClient> = match self.llm_client {
            Some(injected) => injected,
            None => {
                let effective_url =
                    resolve_llm_base_url(self.llm_url.as_deref(), persisted_llm_url.as_deref());
                let client = OpenAiLlmClient::new(&effective_url).map_err(|err| {
                    ServerError::LlmUnavailable {
                        endpoint: effective_url.clone(),
                        reason: err.to_string(),
                    }
                })?;
                Arc::new(client)
            }
        };

        // Mandatory startup gate: fail fast without offline fallback if LLM is unreachable.
        llm_client
            .health_check()
            .await
            .map_err(|err| ServerError::LlmUnavailable {
                endpoint: llm_client.endpoint().to_string(),
                reason: err.to_string(),
            })?;

        let local_hostname = tokio::task::spawn_blocking(read_local_hostname)
            .await
            .ok()
            .flatten();

        if options.llm_client().is_none() {
            options = options.with_llm_client(Arc::clone(&llm_client));
        }

        // Optional graceful-degradation TTS wiring: never block or fail startup.
        if !self.no_tts && options.tts_client().is_none() {
            if let Some(injected_tts) = self.tts_client {
                options = options.with_tts_client(injected_tts);
            } else {
                let effective_tts_url =
                    resolve_tts_base_url(self.tts_url.as_deref(), persisted_tts_url.as_deref());
                match HttpTtsClient::new(&effective_tts_url) {
                    Ok(client) => {
                        options = options.with_tts_client(Arc::new(client));
                    }
                    Err(err) => {
                        eprintln!("warning: invalid LAN TTS endpoint '{effective_tts_url}': {err}");
                        options = options.with_tts_startup_error(err.to_string());
                    }
                }
            }
        }

        let session = TableRuntime::try_with_options_and_threads(
            local_addr.port(),
            local_hostname,
            options,
            self.ai_threads,
        )?;
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let ws_tracker = WsConnectionTracker::new();
        let monitor_handle = if self.headless {
            None
        } else {
            let phase_rx = session.subscribe_phase().await;
            Some(spawn_idle_shutdown_monitor(
                self.idle_timeouts,
                ws_tracker.subscribe(),
                phase_rx,
                shutdown_tx.clone(),
            ))
        };

        let state = AppState {
            port: local_addr.port(),
            headless: self.headless,
            session,
            static_assets,
            heartbeat: self.heartbeat,
            shutdown_tx: shutdown_tx.clone(),
            ws_tracker,
        };
        let router = build_router(state);

        let join_handle = tokio::spawn(async move {
            let serve_res = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.wait_for(|&stopped| stopped).await;
            })
            .await;
            if let Some(monitor) = monitor_handle {
                let _ = monitor.await;
            }
            serve_res
        });

        Ok(ServerHandle {
            addr: local_addr,
            cli_public_url,
            shutdown_tx,
            join_handle: Some(join_handle),
        })
    }
}

fn build_router(state: AppState) -> Router {
    let api_router = Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/preferences", get(preferences_handler))
        .route("/api/history/export", get(history_export_handler))
        .route("/api/history/clear", post(history_clear_handler))
        .route("/api/persona-memory", get(persona_memory_handler))
        .route(
            "/api/persona-memory/clear",
            post(persona_memory_clear_handler),
        )
        .route("/api/banter-audio/{banter_id}", get(banter_audio_handler))
        .route("/api/shutdown", post(shutdown_handler))
        .route("/api/{*rest}", any(api_not_found))
        .route("/api", any(api_not_found))
        .layer(middleware::from_fn(api_no_store_middleware));

    Router::new()
        .merge(api_router)
        .route("/ws", get(ws_handler))
        .fallback(get(static_asset_handler))
        .with_state(state)
}

async fn api_no_store_middleware(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn json_with_no_store<T: Serialize>(status: StatusCode, payload: &T) -> Response {
    let mut response = (status, Json(payload)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn shutdown_handler(
    extensions: axum::http::Extensions,
    State(state): State<AppState>,
) -> Response {
    let peer = extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr);
    let status = if state.headless || !is_loopback_peer(peer) {
        StatusCode::FORBIDDEN
    } else {
        let _ = state.shutdown_tx.send(true);
        StatusCode::OK
    };
    let mut response = status.into_response();
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

async fn preferences_handler(State(state): State<AppState>) -> Response {
    let payload = state.session.history_preferences().await;
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

async fn persona_memory_handler(State(state): State<AppState>) -> Response {
    let payload = state.session.persona_memory_snapshot().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn persona_memory_clear_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let Ok(req) = serde_json::from_slice::<ClearPersonaMemoryRequest>(&body) else {
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
    match state.session.persona_memory_clear().await {
        Ok(resp) => json_with_no_store(StatusCode::OK, &resp),
        Err(err) => json_with_no_store(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "error": format!("failed to clear persona memory: {err}") }),
        ),
    }
}

async fn banter_audio_handler(
    Path(banter_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if !is_valid_banter_id(&banter_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(wav_bytes) = state.session.get_banter_audio(&banter_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = (StatusCode::OK, wav_bytes).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
    response
}

async fn api_not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
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
    let ws_tracker = state.ws_tracker.clone();
    let shutdown_rx = state.shutdown_tx.subscribe();
    ws.on_upgrade(move |socket| {
        handle_ws_connection(
            socket,
            runtime,
            host_is_loopback,
            heartbeat,
            ws_tracker,
            shutdown_rx,
        )
    })
}

async fn handle_ws_connection(
    socket: WebSocket,
    runtime: TableRuntime,
    host_is_loopback: bool,
    heartbeat: HeartbeatConfig,
    ws_tracker: WsConnectionTracker,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let _ws_guard = ws_tracker.enter();
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
        if *shutdown_rx.borrow_and_update() {
            let _ = ws_tx.send(WsMessage::Close(None)).await;
            let _ = ws_tx.close().await;
            break;
        }
        tokio::select! {
            biased;
            res = shutdown_rx.changed() => {
                if res.is_err() || *shutdown_rx.borrow() {
                    let _ = ws_tx.send(WsMessage::Close(None)).await;
                    let _ = ws_tx.close().await;
                    break;
                }
            }
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
    if !is_loopback_peer(peer) {
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::sync::Arc;

    use axum::extract::{ConnectInfo, State};
    use axum::http::header::CACHE_CONTROL;
    use axum::http::{Extensions, StatusCode};
    use tokio::sync::watch;

    use super::{shutdown_handler, AppState, HeartbeatConfig};
    use crate::lifecycle::WsConnectionTracker;
    use crate::session::TableRuntime;
    use crate::static_assets::NoopStaticAssets;

    fn extensions_with_peer(peer: Option<SocketAddr>) -> Extensions {
        let mut ext = Extensions::new();
        if let Some(addr) = peer {
            ext.insert(ConnectInfo(addr));
        }
        ext
    }

    fn test_state(headless: bool) -> (AppState, watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let state = AppState {
            port: 8789,
            headless,
            session: TableRuntime::new(8789),
            static_assets: Arc::new(NoopStaticAssets),
            heartbeat: HeartbeatConfig::default(),
            shutdown_tx,
            ws_tracker: WsConnectionTracker::new(),
        };
        (state, shutdown_rx)
    }

    #[tokio::test]
    async fn shutdown_handler_allows_loopback_and_rejects_non_loopback_and_headless() {
        // Non-loopback IPv4 -> 403 + no-store + does not shut down
        let (state, rx) = test_state(false);
        let resp = shutdown_handler(
            extensions_with_peer(Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
                44444,
            ))),
            State(state),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            resp.headers()
                .get(CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(!*rx.borrow());

        // Non-loopback IPv4-mapped IPv6 -> 403 + no-store
        let (state, rx) = test_state(false);
        let resp = shutdown_handler(
            extensions_with_peer(Some(SocketAddr::new(
                IpAddr::V6(Ipv4Addr::new(10, 0, 0, 9).to_ipv6_mapped()),
                44444,
            ))),
            State(state),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            resp.headers()
                .get(CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(!*rx.borrow());

        // Missing ConnectInfo -> 403 + no-store
        let (state, rx) = test_state(false);
        let resp = shutdown_handler(extensions_with_peer(None), State(state)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(!*rx.borrow());

        // Headless mode -> 403 + no-store even for 127.0.0.1
        let (state, rx) = test_state(true);
        let resp = shutdown_handler(
            extensions_with_peer(Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                44444,
            ))),
            State(state),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            resp.headers()
                .get(CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(!*rx.borrow());

        // IPv6 loopback ::1 -> 200 + no-store + shutdown triggered
        let (state, rx) = test_state(false);
        let resp = shutdown_handler(
            extensions_with_peer(Some(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                44444,
            ))),
            State(state),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(*rx.borrow());
    }
}

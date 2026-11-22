use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use http_body_util::BodyExt;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use mahjong_server::protocol::{HealthResponse, TablePhase};
use mahjong_server::{ServerConfig, ServerHandle};
use tokio::net::TcpStream;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), id));
        if path.exists() {
            fs::remove_dir_all(&path).expect("clean stale temp dir");
        }
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

async fn http_get(addr: SocketAddr, uri_path: &str) -> (StatusCode, hyper::HeaderMap, Vec<u8>) {
    let stream = TcpStream::connect(addr).await.expect("connect to server");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("http1 handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let req = Request::builder()
        .method("GET")
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

async fn spawn_test_server(static_dir: Option<PathBuf>) -> ServerHandle {
    ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        static_dir,
    }
    .spawn()
    .await
    .expect("spawn server")
}

#[tokio::test]
async fn health_endpoint_returns_no_store_and_active_metadata() {
    let server = spawn_test_server(None).await;
    let addr = server.addr();

    let (status, headers, body) = http_get(addr, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.contains("application/json"),
        "unexpected content-type: {content_type}"
    );

    let parsed: HealthResponse = serde_json::from_slice(&body).expect("valid HealthResponse JSON");
    assert_eq!(parsed.app, "majjj");
    assert_eq!(parsed.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed.phase, TablePhase::Lobby);
    assert_eq!(parsed.online_humans, 0);
    assert_eq!(parsed.port, addr.port());

    server.shutdown().await;
}

#[tokio::test]
async fn static_hosting_enforces_cache_headers_spa_fallback_and_traversal_guard() {
    let parent_temp = TempDirGuard::new("majjj_static_host");
    let secret_file = parent_temp.path().join("secret.txt");
    fs::write(&secret_file, "top-secret-outside-root").unwrap();

    let dist_dir = parent_temp.path().join("dist");
    let assets_dir = dist_dir.join("assets");
    fs::create_dir_all(&assets_dir).unwrap();
    fs::write(
        dist_dir.join("index.html"),
        "<!doctype html><html><body>Majjj</body></html>",
    )
    .unwrap();
    fs::write(dist_dir.join("manifest.webmanifest"), r#"{"name":"majjj"}"#).unwrap();
    fs::write(dist_dir.join("manifest.json"), r#"{"name":"majjj"}"#).unwrap();
    fs::write(assets_dir.join("app-deadbeef.js"), "console.log('majjj');").unwrap();
    fs::write(assets_dir.join("style-123456.css"), "body { margin: 0; }").unwrap();

    let server = spawn_test_server(Some(dist_dir)).await;
    let addr = server.addr();

    // Root & index.html -> no-cache
    for path in ["/", "/index.html"] {
        let (status, headers, body) = http_get(addr, path).await;
        assert_eq!(status, StatusCode::OK, "path={path}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-cache"),
            "path={path}"
        );
        assert!(headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .contains("text/html"));
        assert!(String::from_utf8_lossy(&body).contains("Majjj"));
    }

    // Manifest files -> no-cache
    for path in ["/manifest.webmanifest", "/manifest.json"] {
        let (status, headers, _) = http_get(addr, path).await;
        assert_eq!(status, StatusCode::OK, "path={path}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-cache"),
            "path={path}"
        );
    }

    // Hashed assets under /assets/ -> immutable
    let (status, headers, body) = http_get(addr, "/assets/app-deadbeef.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );
    assert!(headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .contains("javascript"));
    assert_eq!(body, b"console.log('majjj');");

    let (status, headers, _) = http_get(addr, "/assets/style-123456.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );
    assert!(headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .contains("text/css"));

    // Missing asset under /assets/ must 404 instead of returning index.html
    let (status, _, _) = http_get(addr, "/assets/missing-999.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Unknown /api/* route must 404 instead of SPA fallback
    let (status, _, _) = http_get(addr, "/api/unknown").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // SPA route fallback to index.html with no-cache
    let (status, headers, body) = http_get(addr, "/lobby/seat").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );
    assert!(String::from_utf8_lossy(&body).contains("Majjj"));

    // Path traversal & null byte guards
    for evil in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/assets/%2e%2e/secret.txt",
        "/index.html%00.js",
    ] {
        let (status, _, body) = http_get(addr, evil).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
            "evil path {evil} returned unexpected status {status}"
        );
        assert!(
            !String::from_utf8_lossy(&body).contains("top-secret-outside-root"),
            "evil path {evil} leaked file outside static root"
        );
    }

    server.shutdown().await;
}

#[tokio::test]
async fn bind_conflict_fails_explicitly_without_falling_back_to_ephemeral_port() {
    let blocker = TcpListener::bind("127.0.0.1:0").expect("bind blocker port");
    let occupied_port = blocker.local_addr().unwrap().port();

    let err = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: occupied_port,
        static_dir: None,
    }
    .spawn()
    .await
    .expect_err("occupied port must return an explicit bind error");

    let msg = err.to_string();
    assert!(
        msg.contains(&occupied_port.to_string()),
        "error message should mention port {occupied_port}, got: {msg}"
    );
    drop(blocker);
}

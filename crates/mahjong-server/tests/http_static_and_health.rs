mod common;

use std::fs;
use std::net::{IpAddr, Ipv4Addr, TcpListener};

use common::{http_get, spawn_server, TempDirGuard};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::StatusCode;
use mahjong_server::protocol::{HealthResponse, TablePhase};
use mahjong_server::ServerConfig;

#[tokio::test]
async fn health_endpoint_returns_no_store_and_active_metadata() {
    let server = spawn_server(None).await;
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
    let nested_dir = dist_dir.join("nested");
    fs::create_dir_all(&assets_dir).unwrap();
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(
        dist_dir.join("index.html"),
        "<!doctype html><html><body>Majjj</body></html>",
    )
    .unwrap();
    fs::write(dist_dir.join("manifest.webmanifest"), r#"{"name":"majjj"}"#).unwrap();
    fs::write(dist_dir.join("manifest.json"), r#"{"name":"majjj"}"#).unwrap();
    fs::write(
        nested_dir.join("manifest.webmanifest"),
        r#"{"name":"majjj"}"#,
    )
    .unwrap();
    fs::write(assets_dir.join("app-deadbeef.js"), "console.log('majjj');").unwrap();
    fs::write(assets_dir.join("index-Ab12Cd34.js"), "console.log('vite');").unwrap();
    fs::write(
        assets_dir.join("index-R99xeND-.js"),
        "console.log('b64url');",
    )
    .unwrap();
    fs::write(assets_dir.join("style-123456.css"), "body { margin: 0; }").unwrap();
    fs::write(assets_dir.join("logo.png"), [0x89, 0x50, 0x4E, 0x47]).unwrap();

    let server = spawn_server(Some(dist_dir)).await;
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

    // Manifest files (root and nested) -> no-cache
    for path in [
        "/manifest.webmanifest",
        "/manifest.json",
        "/nested/manifest.webmanifest",
    ] {
        let (status, headers, _) = http_get(addr, path).await;
        assert_eq!(status, StatusCode::OK, "path={path}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-cache"),
            "path={path}"
        );
    }

    // Hashed assets under /assets/ -> immutable
    for (path, expected_type, expected_body) in [
        (
            "/assets/app-deadbeef.js",
            "javascript",
            b"console.log('majjj');".as_slice(),
        ),
        (
            "/assets/index-Ab12Cd34.js",
            "javascript",
            b"console.log('vite');".as_slice(),
        ),
        (
            "/assets/index-R99xeND-.js",
            "javascript",
            b"console.log('b64url');".as_slice(),
        ),
        (
            "/assets/style-123456.css",
            "text/css",
            b"body { margin: 0; }".as_slice(),
        ),
    ] {
        let (status, headers, body) = http_get(addr, path).await;
        assert_eq!(status, StatusCode::OK, "path={path}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("public, max-age=31536000, immutable"),
            "path={path}"
        );
        assert!(
            headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .contains(expected_type),
            "path={path}"
        );
        assert_eq!(body, expected_body, "path={path}");
    }

    // Unhashed file under /assets/ -> no-cache
    let (status, headers, _) = http_get(addr, "/assets/logo.png").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );

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
        ..ServerConfig::default()
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

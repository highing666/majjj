mod common;

use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::process::Command;
use std::time::Duration;

use common::{
    connect_and_hello, fetch_health, http_get, http_post, recv_snapshot, send_msg, spawn_server,
};
use futures_util::StreamExt;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::StatusCode;
use mahjong_server::protocol::{ClientMessage, TablePhase};
use mahjong_server::static_assets::EmbeddedStaticAssets;
use mahjong_server::{IdleTimeouts, PacingMode, ServerConfig, TableOptions};

#[tokio::test]
async fn default_server_serves_embedded_assets_with_cache_headers_and_guards() {
    let server = spawn_server(None).await;
    let addr = server.addr();

    for path in ["/", "/index.html", "/lobby/seat"] {
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

    let js_path = EmbeddedStaticAssets::asset_paths()
        .find(|p| p.starts_with("assets/") && p.ends_with(".js"))
        .expect("embedded JS asset");
    let (status, headers, body) = http_get(addr, &format!("/{js_path}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );
    assert!(!body.is_empty());

    let (status, _, _) = http_get(addr, "/assets/does-not-exist-123456.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = http_get(addr, "/api/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    for evil in ["/%2e%2e/Cargo.toml", "/index.html%00.js"] {
        let (status, _, _) = http_get(addr, evil).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
            "unexpected status {status} for {evil}"
        );
    }

    server.shutdown().await;
}

#[tokio::test]
async fn post_api_shutdown_on_loopback_stops_server_even_with_active_ws_client() {
    let mut server = spawn_server(None).await;
    let addr = server.addr();

    let mut ws = connect_and_hello(addr, "shutdown-ws", "Alice").await;

    let (status, headers, _) = http_post(addr, "/api/shutdown").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    tokio::time::timeout(Duration::from_secs(2), server.wait())
        .await
        .expect("server must shut down promptly after POST /api/shutdown")
        .expect("server exit result");

    // Active WebSocket must be closed cleanly rather than hanging.
    let next = tokio::time::timeout(Duration::from_secs(1), ws.next()).await;
    assert!(next.is_ok(), "ws stream should close promptly");
}

#[tokio::test]
async fn headless_mode_disables_post_shutdown_and_idle_auto_exit() {
    let server = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        headless: true,
        no_open: true,
        idle_timeouts: IdleTimeouts {
            lobby_or_finished: Duration::from_millis(60),
            in_match: Duration::from_millis(60),
        },
        ..ServerConfig::default()
    }
    .spawn()
    .await
    .expect("spawn headless server");
    let addr = server.addr();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let health = fetch_health(addr).await;
    assert_eq!(health.app, "majjj");

    let (status, headers, _) = http_post(addr, "/api/shutdown").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    tokio::time::sleep(Duration::from_millis(120)).await;
    let health_after = fetch_health(addr).await;
    assert_eq!(health_after.app, "majjj");

    server.shutdown().await;
}

#[tokio::test]
async fn desktop_lobby_idle_auto_exit_fires_when_no_ws_connects() {
    let mut server = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        no_open: true,
        idle_timeouts: IdleTimeouts {
            lobby_or_finished: Duration::from_millis(100),
            in_match: Duration::from_secs(10),
        },
        ..ServerConfig::default()
    }
    .spawn()
    .await
    .expect("spawn desktop server");

    tokio::time::timeout(Duration::from_secs(2), server.wait())
        .await
        .expect("desktop server must auto-exit after Lobby idle timeout")
        .expect("server exit result");
}

#[tokio::test]
async fn desktop_idle_exit_keeps_unseated_observer_alive_and_uses_longer_in_match_threshold() {
    let mut server = ServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        no_open: true,
        idle_timeouts: IdleTimeouts {
            lobby_or_finished: Duration::from_millis(120),
            in_match: Duration::from_millis(550),
        },
        ..ServerConfig::default()
    }
    .spawn_with_options(TableOptions::seeded(42).with_pacing(PacingMode::Immediate))
    .await
    .expect("spawn desktop server");
    let addr = server.addr();

    // Unseated observer keeps server alive well past Lobby idle timeout (120ms).
    let mut observer = connect_and_hello(addr, "obs-1", "Observer").await;
    tokio::time::sleep(Duration::from_millis(240)).await;
    assert_eq!(fetch_health(addr).await.phase, TablePhase::Lobby);

    // Join seat and start match -> transitions to InMatch.
    send_msg(&mut observer, &ClientMessage::JoinSeat { seat: None }).await;
    let _ = recv_snapshot(&mut observer).await;
    send_msg(&mut observer, &ClientMessage::StartMatch).await;
    let start_snap = recv_snapshot(&mut observer).await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);

    // Close the only WS connection -> 0 connections in InMatch.
    drop(observer);
    tokio::time::sleep(Duration::from_millis(240)).await;
    // Must still be alive past 120ms lobby timeout because we're in InMatch (550ms).
    assert_eq!(fetch_health(addr).await.phase, TablePhase::InMatch);

    // Reconnect an unseated WS observer before 550ms -> resets & cancels idle timer.
    let observer2 = connect_and_hello(addr, "obs-2", "Observer2").await;
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(fetch_health(addr).await.phase, TablePhase::InMatch);

    // Now disconnect -> waits InMatch idle timeout (550ms) and exits cleanly.
    drop(observer2);
    tokio::time::timeout(Duration::from_secs(2), server.wait())
        .await
        .expect("server must auto-exit after InMatch idle timeout")
        .expect("server exit result");
}

#[test]
fn cli_binary_port_conflict_probes_majjj_and_rejects_non_majjj() {
    let bin = env!("CARGO_BIN_EXE_mahjong-server");

    // 1. Non-majjj listener occupies port -> CLI must exit non-zero with clear error.
    let blocker = TcpListener::bind("127.0.0.1:0").expect("bind blocker");
    let occupied_port = blocker.local_addr().unwrap().port();
    let non_majjj_out = Command::new(bin)
        .args([
            "--bind",
            "127.0.0.1",
            "--port",
            &occupied_port.to_string(),
            "--no-open",
        ])
        .output()
        .expect("run binary against non-majjj port");
    assert!(
        !non_majjj_out.status.success(),
        "must fail when port occupied by non-majjj"
    );
    let stderr = String::from_utf8_lossy(&non_majjj_out.stderr);
    assert!(
        stderr.contains(&occupied_port.to_string()),
        "stderr should mention port {occupied_port}, got: {stderr}"
    );
    drop(blocker);

    // 2. Existing majjj instance occupies port -> desktop `--no-open` probes /api/health and exits 0;
    // `--headless` exits non-zero.
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let existing = ServerConfig {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            no_open: true,
            ..ServerConfig::default()
        }
        .spawn()
        .await
        .expect("spawn existing majjj");
        let port = existing.addr().port();

        let reuse_out = tokio::task::spawn_blocking(move || {
            Command::new(bin)
                .args([
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--no-open",
                ])
                .output()
                .expect("run binary against existing majjj")
        })
        .await
        .unwrap();
        assert!(
            reuse_out.status.success(),
            "desktop mode should exit 0 when existing majjj answers /api/health; stderr={}",
            String::from_utf8_lossy(&reuse_out.stderr)
        );
        let stdout = String::from_utf8_lossy(&reuse_out.stdout);
        assert!(
            stdout.contains("already running"),
            "expected 'already running' message, got: {stdout}"
        );

        let headless_out = tokio::task::spawn_blocking(move || {
            Command::new(bin)
                .args([
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--headless",
                ])
                .output()
                .expect("run headless binary against occupied port")
        })
        .await
        .unwrap();
        assert!(
            !headless_out.status.success(),
            "headless mode must fail if port is already occupied"
        );

        existing.shutdown().await;
    });
}

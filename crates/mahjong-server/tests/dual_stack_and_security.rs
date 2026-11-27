mod common;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::process::Command;
use std::time::Duration;

use common::{connect_ws, fetch_health, raw_http_request, recv_snapshot, send_msg};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HOST};
use hyper::StatusCode;
use mahjong_server::protocol::ClientMessage;
use mahjong_server::ServerConfig;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[tokio::test]
async fn default_dual_stack_listener_accepts_both_ipv4_and_ipv6_loopback_and_enforces_guards() {
    // ServerConfig::default() uses dual-stack [::] with IPV6_V6ONLY = false.
    let mut server = ServerConfig::default()
        .with_port(0)
        .with_no_open(true)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .spawn()
        .await
        .expect("spawn default dual-stack server");

    let bound = server.addr();
    assert_eq!(
        bound.ip(),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        "default server should bind dual-stack [::]"
    );
    let port = bound.port();
    let v4_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let v6_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);

    // 1. Both 127.0.0.1 and [::1] reach GET /api/health on the single bound port.
    let health_v4 = fetch_health(v4_addr).await;
    let health_v6 = fetch_health(v6_addr).await;
    assert_eq!(health_v4.app, "majjj");
    assert_eq!(health_v4.port, port);
    assert_eq!(health_v6.app, "majjj");
    assert_eq!(health_v6.port, port);

    // 2. Both 127.0.0.1 and [::1] can establish WebSocket sessions with host_is_loopback = true.
    for (connect_addr, client_id, origin) in [
        (v4_addr, "v4-client", format!("http://127.0.0.1:{port}")),
        (v6_addr, "v6-client", format!("http://[::1]:{port}")),
    ] {
        let mut req = format!("ws://{connect_addr}/ws")
            .into_client_request()
            .expect("ws client request");
        req.headers_mut()
            .insert("Origin", origin.parse().expect("origin header"));
        let (mut ws, _) = connect_async(req)
            .await
            .unwrap_or_else(|err| panic!("same-origin ws connect to {connect_addr} failed: {err}"));
        send_msg(
            &mut ws,
            &ClientMessage::Hello {
                client_id: client_id.to_string(),
                nickname: "Host".to_string(),
                table_id: None,
                seat_token: None,
            },
        )
        .await;
        let snap = recv_snapshot(&mut ws).await;
        assert!(
            snap.host_is_loopback,
            "client over {connect_addr} must report host_is_loopback = true"
        );
        let _ = ws.close(None).await;
    }

    // 3. Cross-origin WebSocket upgrade is rejected with 403.
    let mut evil_ws_req = format!("ws://{v4_addr}/ws")
        .into_client_request()
        .expect("evil ws req");
    evil_ws_req.headers_mut().insert(
        "Origin",
        format!("http://evil.example:{port}").parse().unwrap(),
    );
    let err = connect_async(evil_ws_req)
        .await
        .expect_err("cross-origin WebSocket must be rejected");
    let err_text = err.to_string();
    assert!(
        err_text.contains("403"),
        "expected HTTP 403 for cross-origin WS, got: {err_text}"
    );

    // 4. Local reverse-proxy Host (127.0.0.1 peer + public domain Host) is classified Remote -> minimal 403.
    let (status, headers, body) = raw_http_request(
        v4_addr,
        "GET",
        "/api/health",
        &[(HOST.as_str(), &format!("majjj.example.com:{port}"))],
        b"",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert!(body.is_empty());

    // 5. Mutating POST without application/json is rejected with 415 and does not shut down.
    let (status, headers, _) = raw_http_request(
        v4_addr,
        "POST",
        "/api/shutdown",
        &[
            (HOST.as_str(), &format!("127.0.0.1:{port}")),
            (CONTENT_TYPE.as_str(), "text/plain"),
        ],
        b"",
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(fetch_health(v4_addr).await.app, "majjj");

    // 6. Dual-stack port conflict fails explicitly with AddrInUse rather than falling back.
    let conflict_err = ServerConfig::default()
        .with_port(port)
        .with_llm_client(common::stub_llm_client())
        .spawn()
        .await
        .expect_err("second default dual-stack bind on same port must fail");
    assert!(
        conflict_err.to_string().contains(&port.to_string()),
        "conflict error should mention port {port}, got: {conflict_err}"
    );

    // 7. Existing dual-stack majjj instance is detected and reused by the CLI binary without --bind.
    let bin = env!("CARGO_BIN_EXE_mahjong-server");
    let reuse_out = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["--port", &port.to_string(), "--no-open"])
            .output()
            .expect("run CLI binary against existing dual-stack server")
    })
    .await
    .unwrap();
    assert!(
        reuse_out.status.success(),
        "CLI binary should detect and reuse existing dual-stack majjj; stderr={}",
        String::from_utf8_lossy(&reuse_out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&reuse_out.stdout).contains("already running"),
        "expected 'already running' output, got: {}",
        String::from_utf8_lossy(&reuse_out.stdout)
    );

    // 8. POST /api/shutdown over IPv6 [::1] with Content-Type: application/json shuts down cleanly.
    let mut ws = connect_ws(v6_addr).await;
    let (status, headers, _) = raw_http_request(
        v6_addr,
        "POST",
        "/api/shutdown",
        &[
            (HOST.as_str(), &format!("[::1]:{port}")),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        b"",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    tokio::time::timeout(Duration::from_secs(2), server.wait())
        .await
        .expect("server must shut down promptly after IPv6 loopback POST /api/shutdown")
        .expect("server exit cleanly");
    let _ = ws.close(None).await;
}

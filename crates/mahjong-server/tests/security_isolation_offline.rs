mod common;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, HOST, ORIGIN};
use axum::http::{Method, StatusCode};
use common::{assert_minimal_403, send_router_request};
use mahjong_server::static_assets::EmbeddedStaticAssets;
use mahjong_server::{ServerConfig, TableOptions, TestRouterHandle};

async fn build_test_router(headless: bool) -> TestRouterHandle {
    ServerConfig::default()
        .with_port(8789)
        .with_headless(headless)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .build_router_with_options(TableOptions::default())
        .await
        .expect("build full-stack test router")
}

#[tokio::test]
async fn remote_sources_are_rejected_with_minimal_403_across_all_paths_by_default() {
    let handle = build_test_router(false).await;
    let embedded_js = EmbeddedStaticAssets::asset_paths()
        .find(|p| p.starts_with("assets/") && p.ends_with(".js"))
        .expect("embedded JS asset");
    let embedded_js_uri = format!("/{embedded_js}");

    struct RemoteScenario<'a> {
        label: &'a str,
        peer: Option<SocketAddr>,
        headers: Vec<(&'a str, &'a str)>,
    }

    let remote_scenarios = vec![
        RemoteScenario {
            label: "global IPv6 peer with IPv6 Host",
            peer: Some(SocketAddr::new(
                "240e:1234:5678::42".parse().unwrap(),
                51000,
            )),
            headers: vec![("Host", "[240e:1234:5678::1]:8789")],
        },
        RemoteScenario {
            label: "same-/64 global IPv6 peer",
            peer: Some(SocketAddr::new(
                "2001:db8:abcd:0001::99".parse().unwrap(),
                51001,
            )),
            headers: vec![("Host", "[2001:db8:abcd:0001::1]:8789")],
        },
        RemoteScenario {
            label: "public IPv4 peer",
            peer: Some(SocketAddr::new("203.0.113.25".parse().unwrap(), 51002)),
            headers: vec![("Host", "203.0.113.10:8789")],
        },
        RemoteScenario {
            label: "local reverse proxy (127.0.0.1 peer + public domain Host)",
            peer: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51003)),
            headers: vec![("Host", "majjj.example.com:8789")],
        },
        RemoteScenario {
            label: "local IPv6 reverse proxy (::1 peer + public IPv6 Host)",
            peer: Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 51004)),
            headers: vec![("Host", "[240e:1234::1]:8789")],
        },
        RemoteScenario {
            label: "remote peer spoofing localhost Host and proxy forwarding headers",
            peer: Some(SocketAddr::new("240e:1234::88".parse().unwrap(), 51005)),
            headers: vec![
                ("Host", "localhost:8789"),
                ("X-Forwarded-For", "127.0.0.1"),
                ("X-Real-IP", "127.0.0.1"),
                ("Forwarded", "for=127.0.0.1;host=localhost:8789"),
            ],
        },
        RemoteScenario {
            label: "missing ConnectInfo fails closed to Remote",
            peer: None,
            headers: vec![("Host", "127.0.0.1:8789")],
        },
        RemoteScenario {
            label: "loopback peer with missing Host fails closed to Remote",
            peer: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51006)),
            headers: vec![],
        },
        RemoteScenario {
            label: "loopback peer with duplicate Host fails closed to Remote",
            peer: Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51007)),
            headers: vec![("Host", "127.0.0.1:8789"), ("Host", "127.0.0.1:8789")],
        },
    ];

    let paths: Vec<(Method, &str)> = vec![
        (Method::GET, "/"),
        (Method::GET, "/index.html"),
        (Method::GET, "/lobby/seat"),
        (Method::GET, embedded_js_uri.as_str()),
        (Method::GET, "/api/health"),
        (Method::GET, "/api/stats"),
        (Method::GET, "/api/preferences"),
        (Method::GET, "/api/history/export"),
        (Method::POST, "/api/history/clear"),
        (Method::GET, "/api/persona-memory"),
        (Method::POST, "/api/persona-memory/clear"),
        (Method::GET, "/api/banter-audio/sample-id"),
        (Method::POST, "/api/shutdown"),
        (Method::GET, "/api/shutdown"),
        (Method::GET, "/api/nonexistent"),
        (Method::GET, "/ws"),
    ];

    for scenario in &remote_scenarios {
        for (method, uri) in &paths {
            let mut hdrs = scenario.headers.clone();
            if *method == Method::POST {
                hdrs.push(("Content-Type", "application/json"));
            }
            let (status, headers, body) = send_router_request(
                &handle.router,
                scenario.peer,
                method.clone(),
                uri,
                &hdrs,
                Body::empty(),
            )
            .await;
            assert_minimal_403(
                status,
                &headers,
                &body,
                &format!("{} -> {method} {uri}", scenario.label),
            );
        }
    }

    assert!(
        !*handle.shutdown_rx.borrow(),
        "remote POST /api/shutdown must never trigger shutdown"
    );
}

#[tokio::test]
async fn lan_and_loopback_trust_matrix_preserves_lan_access_and_locks_shutdown_to_loopback() {
    let handle = build_test_router(false).await;

    let lan_scenarios: Vec<(&str, SocketAddr, &str)> = vec![
        (
            "RFC1918 IPv4 peer + IPv4 Host",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 5, 21)), 52000),
            "192.168.5.10:8789",
        ),
        (
            "IPv4-mapped IPv6 LAN peer + .local Host",
            SocketAddr::new(
                IpAddr::V6(Ipv4Addr::new(192, 168, 5, 22).to_ipv6_mapped()),
                52001,
            ),
            "host-mac.local:8789",
        ),
        (
            "IPv6 link-local peer + link-local Host",
            SocketAddr::new("fe80::1234".parse().unwrap(), 52002),
            "[fe80::1]:8789",
        ),
        (
            "IPv6 ULA peer + ULA Host",
            SocketAddr::new("fd00:1234::2".parse().unwrap(), 52003),
            "[fd00:1234::1]:8789",
        ),
        (
            "LAN peer spoofing localhost Host and X-Forwarded-For",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 15)), 52004),
            "localhost:8789",
        ),
        (
            "Loopback peer with LAN Host (e.g. local Vite LAN proxy)",
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52005),
            "192.168.5.10:5173",
        ),
    ];

    for (label, peer, host) in lan_scenarios {
        // LAN clients retain zero-friction access to SPA and LAN APIs
        for uri in [
            "/",
            "/api/health",
            "/api/stats",
            "/api/preferences",
            "/api/persona-memory",
        ] {
            let (status, _, _) = send_router_request(
                &handle.router,
                Some(peer),
                Method::GET,
                uri,
                &[("Host", host), ("X-Forwarded-For", "127.0.0.1")],
                Body::empty(),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{label} -> GET {uri}");
        }

        // LAN clients are strictly forbidden from calling POST /api/shutdown
        let (status, headers, _) = send_router_request(
            &handle.router,
            Some(peer),
            Method::POST,
            "/api/shutdown",
            &[
                ("Host", host),
                ("Content-Type", "application/json"),
                ("X-Forwarded-For", "127.0.0.1"),
            ],
            Body::empty(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{label} -> POST /api/shutdown"
        );
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(
            !*handle.shutdown_rx.borrow(),
            "{label} must not trigger shutdown"
        );
    }

    // Loopback peer + loopback Host (including IPv4-mapped ::ffff:127.0.0.1) succeeds on shutdown
    for (peer, host) in [
        (
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 53000),
            "127.0.0.1:8789",
        ),
        (
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53001),
            "[::1]:8789",
        ),
        (
            SocketAddr::new(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()), 53002),
            "localhost:8789",
        ),
    ] {
        let loop_handle = build_test_router(false).await;
        let (status, headers, _) = send_router_request(
            &loop_handle.router,
            Some(peer),
            Method::POST,
            "/api/shutdown",
            &[
                ("Host", host),
                ("Content-Type", "application/json; charset=utf-8"),
            ],
            Body::empty(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "peer={peer} host={host}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        assert!(*loop_handle.shutdown_rx.borrow(), "peer={peer} host={host}");
    }

    // Headless mode still returns 403 even on full Loopback + JSON Content-Type
    let headless_handle = build_test_router(true).await;
    let (status, headers, _) = send_router_request(
        &headless_handle.router,
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 53010)),
        Method::POST,
        "/api/shutdown",
        &[
            ("Host", "127.0.0.1:8789"),
            ("Content-Type", "application/json"),
        ],
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert!(!*headless_handle.shutdown_rx.borrow());
}

#[tokio::test]
async fn mutating_post_endpoints_enforce_application_json_content_type() {
    let handle = build_test_router(false).await;
    let loopback = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54000));

    let bad_content_types: [Option<&str>; 4] = [
        None,
        Some("text/plain"),
        Some("text/plain; charset=utf-8"),
        Some("application/x-www-form-urlencoded"),
    ];

    for endpoint in [
        "/api/shutdown",
        "/api/history/clear",
        "/api/persona-memory/clear",
    ] {
        for bad_ct in bad_content_types {
            let mut hdrs = vec![("Host", "127.0.0.1:8789")];
            if let Some(ct) = bad_ct {
                hdrs.push(("Content-Type", ct));
            }
            let (status, headers, _) = send_router_request(
                &handle.router,
                loopback,
                Method::POST,
                endpoint,
                &hdrs,
                Body::from(r#"{"confirm":true}"#),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "endpoint={endpoint} content-type={bad_ct:?}"
            );
            assert_eq!(
                headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
                Some("no-store"),
                "endpoint={endpoint} content-type={bad_ct:?}"
            );
        }
    }
    assert!(
        !*handle.shutdown_rx.borrow(),
        "rejected Content-Type must never trigger shutdown"
    );

    // Valid application/json; charset=utf-8 succeeds on clear endpoints
    for endpoint in ["/api/history/clear", "/api/persona-memory/clear"] {
        let (status, headers, _) = send_router_request(
            &handle.router,
            loopback,
            Method::POST,
            endpoint,
            &[
                ("Host", "127.0.0.1:8789"),
                ("Content-Type", "application/json; charset=utf-8"),
            ],
            Body::from(r#"{"confirm":true}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "endpoint={endpoint}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
    }
}

#[tokio::test]
async fn ws_route_rejects_cross_origin_requests_with_403_and_allows_same_origin_or_absent_origin() {
    let handle = build_test_router(false).await;
    let loopback = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55000));
    let lan = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(192, 168, 5, 30)),
        55001,
    ));

    // Cross-origin and malformed Origin headers are rejected with 403 + no-store before WS upgrade
    for (peer, host, origin) in [
        (loopback, "127.0.0.1:8789", "http://evil.example:8789"),
        (loopback, "localhost:8789", "http://127.0.0.1:8789"),
        (loopback, "localhost:8789", "http://localhost:9999"),
        (loopback, "[::1]:8789", "http://127.0.0.1:8789"),
        (loopback, "localhost:8789", "null"),
        (loopback, "localhost:8789", "http://localhost:8789/ws"),
        (lan, "my-mac.local:8789", "http://evil.example:8789"),
    ] {
        let (status, headers, body) = send_router_request(
            &handle.router,
            peer,
            Method::GET,
            "/ws",
            &[("Host", host), ("Origin", origin)],
            Body::empty(),
        )
        .await;
        assert_minimal_403(
            status,
            &headers,
            &body,
            &format!("Host={host} Origin={origin}"),
        );
    }

    // Duplicate Origin headers are rejected with 403
    let (status, headers, body) = send_router_request(
        &handle.router,
        loopback,
        Method::GET,
        "/ws",
        &[
            ("Host", "localhost:8789"),
            ("Origin", "http://localhost:8789"),
            ("Origin", "http://evil.example:8789"),
        ],
        Body::empty(),
    )
    .await;
    assert_minimal_403(status, &headers, &body, "duplicate Origin");

    // Absent Origin and matching same-origin pass the Origin guard (reaching Axum's WS upgrade check -> 400, NOT 403)
    for (peer, host, origin) in [
        (loopback, "127.0.0.1:8789", None),
        (loopback, "localhost:8789", Some("http://localhost:8789")),
        (loopback, "[::1]:8789", Some("http://[::1]:8789")),
        (loopback, "localhost:80", Some("http://localhost")),
        (lan, "my-mac.local:8789", Some("http://MY-MAC.LOCAL:8789")),
    ] {
        let mut hdrs = vec![(HOST.as_str(), host)];
        if let Some(orig) = origin {
            hdrs.push((ORIGIN.as_str(), orig));
        }
        let (status, _, _) = send_router_request(
            &handle.router,
            peer,
            Method::GET,
            "/ws",
            &hdrs,
            Body::empty(),
        )
        .await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "allowed WS request should pass Origin guard: Host={host} Origin={origin:?}"
        );
    }
}

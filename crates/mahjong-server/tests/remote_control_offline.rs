mod common;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use common::TempDirGuard;
use http_body_util::BodyExt;
use mahjong_server::lan::PublicIpv6UrlInfo;
use mahjong_server::protocol::{RemoteAccessStatusResponse, RemoteAuthResponse};
use mahjong_server::public_url::PublicUrlSource;
use mahjong_server::storage::HistoryStore;
use mahjong_server::{ServerConfig, TableOptions, TestRouterHandle};
use tower::ServiceExt;

async fn build_control_router(
    allow_remote: bool,
    cli_public_url: Option<&str>,
    ipv6_urls: Vec<&str>,
    data_dir: Option<&std::path::Path>,
) -> TestRouterHandle {
    let urls: Vec<String> = ipv6_urls.into_iter().map(str::to_owned).collect();
    let ipv6_info = PublicIpv6UrlInfo {
        preferred_ipv6_url: urls.first().cloned(),
        ipv6_urls: urls,
    };
    let mut cfg = ServerConfig::default()
        .with_port(8789)
        .with_allow_remote(allow_remote)
        .with_public_url(cli_public_url.map(str::to_owned))
        .with_public_ipv6_urls(ipv6_info)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true);
    if let Some(dir) = data_dir {
        cfg = cfg.with_data_dir(dir.to_path_buf());
    }
    cfg.build_router_with_options(TableOptions::default())
        .await
        .expect("build full-stack remote-control test router")
}

async fn send_router_request(
    router: &axum::Router,
    peer: Option<SocketAddr>,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: impl Into<Body>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    for &(name, value) in headers {
        builder = builder.header(name, value);
    }
    let mut req = builder.body(body.into()).expect("build router request");
    if let Some(addr) = peer {
        req.extensions_mut().insert(ConnectInfo(addr));
    }
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("router oneshot response");
    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect response body")
        .to_bytes()
        .to_vec();
    (status, resp_headers, bytes)
}

fn assert_minimal_403(status: StatusCode, headers: &HeaderMap, body: &[u8], context: &str) {
    assert_eq!(status, StatusCode::FORBIDDEN, "{context}");
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "{context}: expected Cache-Control: no-store"
    );
    assert!(
        body.is_empty(),
        "{context}: minimal 403 must not leak body, got {:?}",
        String::from_utf8_lossy(body)
    );
}

#[tokio::test]
async fn host_remote_control_endpoints_reject_all_non_loopback_and_enforce_json_content_type() {
    let handle = build_control_router(true, None, vec!["http://[2001:db9:1::10]:8789"], None).await;
    let passcode = handle
        .remote_access
        .passcode()
        .expect("enabled router has passcode");

    let remote_peer = Some(SocketAddr::new("2001:db9:2::99".parse().unwrap(), 51000));
    let remote_host = "[2001:db9:1::10]:8789";

    // Authenticate a remote client to prove even authenticated Remote gets 403 on every control endpoint.
    let (auth_status, auth_headers, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", remote_host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{passcode}"}}"#)),
    )
    .await;
    assert_eq!(auth_status, StatusCode::OK);
    let remote_cookie = auth_headers
        .get(SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    let endpoints: [(Method, &str, &str); 5] = [
        (Method::GET, "/api/remote-access", ""),
        (Method::POST, "/api/remote-access/enable", "{}"),
        (Method::POST, "/api/remote-access/disable", "{}"),
        (Method::POST, "/api/remote-access/reset-code", "{}"),
        (
            Method::POST,
            "/api/remote-access/public-url",
            r#"{"public_url":"http://majjj.example.com:8789"}"#,
        ),
    ];

    let non_loopback_callers: Vec<(&str, Option<SocketAddr>, &str, Option<&str>)> = vec![
        (
            "LAN IPv4",
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 168, 5, 20)),
                52000,
            )),
            "192.168.5.10:8789",
            None,
        ),
        (
            "LAN peer spoofing localhost Host",
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 168, 5, 20)),
                52001,
            )),
            "127.0.0.1:8789",
            None,
        ),
        (
            "Loopback peer with LAN Host",
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52002)),
            "macbook.local:8789",
            None,
        ),
        (
            "Local reverse proxy (127.0.0.1 peer + public Host)",
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52003)),
            "majjj.example.com:8789",
            Some(remote_cookie.as_str()),
        ),
        (
            "Authenticated Remote peer",
            remote_peer,
            remote_host,
            Some(remote_cookie.as_str()),
        ),
        ("Missing ConnectInfo", None, "127.0.0.1:8789", None),
    ];

    for (label, peer, host, cookie) in &non_loopback_callers {
        for (method, uri, body) in &endpoints {
            let mut hdrs = vec![("Host", *host)];
            if *method == Method::POST {
                hdrs.push(("Content-Type", "application/json"));
            }
            if let Some(c) = cookie {
                hdrs.push((COOKIE.as_str(), *c));
            }
            let (status, headers, resp_body) = send_router_request(
                &handle.router,
                *peer,
                method.clone(),
                uri,
                &hdrs,
                Body::from(*body),
            )
            .await;
            assert_minimal_403(
                status,
                &headers,
                &resp_body,
                &format!("{label} -> {method} {uri}"),
            );
        }
    }

    // All POST control endpoints must reject missing or non-JSON Content-Type with 415 even on Loopback.
    let loopback_v6 = Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 53000));
    for (uri, body) in [
        ("/api/remote-access/enable", "{}"),
        ("/api/remote-access/disable", "{}"),
        ("/api/remote-access/reset-code", "{}"),
        (
            "/api/remote-access/public-url",
            r#"{"public_url":"http://majjj.example.com:8789"}"#,
        ),
    ] {
        for bad_ct in [
            None,
            Some("text/plain"),
            Some("application/x-www-form-urlencoded"),
        ] {
            let mut hdrs = vec![("Host", "[::1]:8789")];
            if let Some(ct) = bad_ct {
                hdrs.push((CONTENT_TYPE.as_str(), ct));
            }
            let (status, headers, _) = send_router_request(
                &handle.router,
                loopback_v6,
                Method::POST,
                uri,
                &hdrs,
                Body::from(body),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "POST {uri} with Content-Type={bad_ct:?} must return 415"
            );
            assert_eq!(
                headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
                Some("no-store")
            );
        }
    }
}

#[tokio::test]
async fn host_remote_control_lifecycle_revocation_semantics_and_public_url_persistence() {
    let data_dir = TempDirGuard::new("majjj_remote_control_prefs");
    let ipv6_primary = "http://[2001:db9:1::10]:8789";
    let ipv6_secondary = "http://[2001:db9:1::20]:8789";
    let handle = build_control_router(
        false,
        None,
        vec![ipv6_primary, ipv6_secondary],
        Some(data_dir.path()),
    )
    .await;

    let loopback = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54000));
    let loopback_host = "127.0.0.1:8789";
    let remote_peer = Some(SocketAddr::new("2001:db9:9::77".parse().unwrap(), 54100));
    let remote_host = "[2001:db9:1::10]:8789";

    // 1. Initial GET /api/remote-access starts disabled with discovered IPv6 URLs and no passcode/invite_url.
    let (status, headers, body) = send_router_request(
        &handle.router,
        loopback,
        Method::GET,
        "/api/remote-access",
        &[("Host", loopback_host)],
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let initial: RemoteAccessStatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(!initial.enabled);
    assert_eq!(initial.passcode, None);
    assert_eq!(initial.active_remote_connections, 0);
    assert_eq!(initial.public_url_override, None);
    assert_eq!(initial.public_url_source, PublicUrlSource::None);
    assert_eq!(initial.persisted_public_url, None);
    assert_eq!(initial.preferred_ipv6_url.as_deref(), Some(ipv6_primary));
    assert_eq!(
        initial.ipv6_urls,
        vec![ipv6_primary.to_string(), ipv6_secondary.to_string()]
    );
    assert_eq!(initial.invite_base_url.as_deref(), Some(ipv6_primary));
    assert_eq!(initial.invite_url, None);

    // Resetting code while disabled returns 409 Conflict with Chinese error message.
    let (reset_err_st, _, reset_err_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/reset-code",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from("{}"),
    )
    .await;
    assert_eq!(reset_err_st, StatusCode::CONFLICT);
    let reset_err_text = String::from_utf8(reset_err_body).unwrap();
    assert!(reset_err_text.contains("远程联机"));

    // 2. POST /api/remote-access/enable generates 8-char passcode and composes IPv6 invite_url.
    let (en_status, _, en_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/enable",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json; charset=utf-8"),
        ],
        Body::from("{}"),
    )
    .await;
    assert_eq!(en_status, StatusCode::OK);
    let enabled: RemoteAccessStatusResponse = serde_json::from_slice(&en_body).unwrap();
    assert!(enabled.enabled);
    let code1 = enabled
        .passcode
        .clone()
        .expect("enabled must have passcode");
    assert_eq!(code1.len(), 8);
    assert_eq!(
        enabled.invite_url.as_deref(),
        Some(format!("{ipv6_primary}/?code={code1}").as_str())
    );

    // 3. Remote client authenticates with code1 and obtains a valid session cookie.
    let (auth_st, auth_hdrs, auth_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", remote_host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code1}"}}"#)),
    )
    .await;
    assert_eq!(auth_st, StatusCode::OK);
    let auth_resp: RemoteAuthResponse = serde_json::from_slice(&auth_body).unwrap();
    assert!(auth_resp.authenticated);
    let remote_cookie = auth_hdrs
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    // Verify active remote WS counter surfaces via GET /api/remote-access.
    let (ws_guard, _rev_rx) = handle.remote_access.enter_remote_ws();
    let (cnt_st, _, cnt_body) = send_router_request(
        &handle.router,
        loopback,
        Method::GET,
        "/api/remote-access",
        &[("Host", loopback_host)],
        Body::empty(),
    )
    .await;
    assert_eq!(cnt_st, StatusCode::OK);
    let counted: RemoteAccessStatusResponse = serde_json::from_slice(&cnt_body).unwrap();
    assert_eq!(counted.active_remote_connections, 1);

    // 4. POST /api/remote-access/reset-code rotates passcode while KEEPING existing remote cookie alive.
    let (rst_st, _, rst_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/reset-code",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from("{}"),
    )
    .await;
    assert_eq!(rst_st, StatusCode::OK);
    let rotated: RemoteAccessStatusResponse = serde_json::from_slice(&rst_body).unwrap();
    assert!(rotated.enabled);
    let code2 = rotated.passcode.clone().expect("rotated passcode");
    assert_ne!(code1, code2);
    assert_eq!(rotated.active_remote_connections, 1);

    // Old passcode is now invalid (401), but existing remote session cookie still accesses SPA / (200).
    let (old_auth_st, _, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", remote_host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code1}"}}"#)),
    )
    .await;
    assert_eq!(old_auth_st, StatusCode::UNAUTHORIZED);

    let (still_authed_st, _, still_authed_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/",
        &[
            ("Host", remote_host),
            (COOKIE.as_str(), remote_cookie.as_str()),
        ],
        Body::empty(),
    )
    .await;
    assert_eq!(still_authed_st, StatusCode::OK);
    let spa_html = String::from_utf8(still_authed_body).unwrap();
    assert!(
        !spa_html.contains("请输入入桌口令"),
        "existing remote cookie must remain valid after reset-code"
    );

    // 5. POST /api/remote-access/public-url validates, normalizes, and atomically persists to preferences.json.
    let (bad_url_st, _, bad_url_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/public-url",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from(r#"{"public_url":"http://majjj.example.com:8789/bad-path"}"#),
    )
    .await;
    assert_eq!(bad_url_st, StatusCode::BAD_REQUEST);
    let bad_url_text = String::from_utf8(bad_url_body).unwrap();
    assert!(
        bad_url_text.contains("不能包含路径"),
        "expected Chinese validation error, got: {bad_url_text}"
    );

    let (save_st, _, save_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/public-url",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from(r#"{"public_url":"  HTTP://Majjj.Example.COM:8789/  "}"#),
    )
    .await;
    assert_eq!(save_st, StatusCode::OK);
    let saved: RemoteAccessStatusResponse = serde_json::from_slice(&save_body).unwrap();
    assert_eq!(
        saved.public_url_override.as_deref(),
        Some("http://majjj.example.com:8789")
    );
    assert_eq!(saved.public_url_source, PublicUrlSource::Preferences);
    assert_eq!(
        saved.persisted_public_url.as_deref(),
        Some("http://majjj.example.com:8789")
    );
    assert_eq!(
        saved.invite_base_url.as_deref(),
        Some("http://majjj.example.com:8789")
    );
    assert_eq!(
        saved.invite_url.as_deref(),
        Some(format!("http://majjj.example.com:8789/?code={code2}").as_str())
    );
    // Persisted on disk in preferences.json.
    let reopened = HistoryStore::open(data_dir.path()).expect("reopen preferences.json");
    assert_eq!(
        reopened.preferences().public_url_override.as_deref(),
        Some("http://majjj.example.com:8789")
    );

    // Clearing with null restores IPv6 fallback and clears preferences.json.
    let (clear_st, _, clear_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/public-url",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from(r#"{"public_url":null}"#),
    )
    .await;
    assert_eq!(clear_st, StatusCode::OK);
    let cleared: RemoteAccessStatusResponse = serde_json::from_slice(&clear_body).unwrap();
    assert_eq!(cleared.public_url_override, None);
    assert_eq!(cleared.public_url_source, PublicUrlSource::None);
    assert_eq!(cleared.persisted_public_url, None);
    assert_eq!(cleared.invite_base_url.as_deref(), Some(ipv6_primary));
    assert_eq!(
        cleared.invite_url.as_deref(),
        Some(format!("{ipv6_primary}/?code={code2}").as_str())
    );
    let reopened_cleared = HistoryStore::open(data_dir.path()).expect("reopen cleared prefs");
    assert_eq!(reopened_cleared.preferences().public_url_override, None);

    // 6. POST /api/remote-access/disable clears passcode and immediately invalidates all remote cookies.
    let (dis_st, _, dis_body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/disable",
        &[
            ("Host", loopback_host),
            ("Content-Type", "application/json"),
        ],
        Body::from("{}"),
    )
    .await;
    assert_eq!(dis_st, StatusCode::OK);
    let disabled: RemoteAccessStatusResponse = serde_json::from_slice(&dis_body).unwrap();
    assert!(!disabled.enabled);
    assert_eq!(disabled.passcode, None);
    assert_eq!(disabled.invite_url, None);

    let (revoked_st, revoked_hdrs, revoked_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/",
        &[
            ("Host", remote_host),
            (COOKIE.as_str(), remote_cookie.as_str()),
        ],
        Body::empty(),
    )
    .await;
    assert_minimal_403(
        revoked_st,
        &revoked_hdrs,
        &revoked_body,
        "remote cookie must be invalidated after disable",
    );

    drop(ws_guard);
}

#[tokio::test]
async fn host_remote_control_reports_cli_public_url_source_even_when_preferences_updated() {
    let data_dir = TempDirGuard::new("majjj_remote_control_cli_override");
    let handle = build_control_router(
        true,
        Some("https://cli.example.com:8443/"),
        vec!["http://[2001:db9:1::10]:8789"],
        Some(data_dir.path()),
    )
    .await;

    let loopback = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55000));
    let (st, _, body) = send_router_request(
        &handle.router,
        loopback,
        Method::POST,
        "/api/remote-access/public-url",
        &[
            ("Host", "localhost:8789"),
            ("Content-Type", "application/json"),
        ],
        Body::from(r#"{"public_url":"http://pref.example.com:8789"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let resp: RemoteAccessStatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        resp.public_url_override.as_deref(),
        Some("https://cli.example.com:8443")
    );
    assert_eq!(resp.public_url_source, PublicUrlSource::Cli);
    assert_eq!(
        resp.persisted_public_url.as_deref(),
        Some("http://pref.example.com:8789")
    );
    assert_eq!(
        resp.invite_base_url.as_deref(),
        Some("https://cli.example.com:8443")
    );
    let code = resp.passcode.expect("passcode");
    assert_eq!(
        resp.invite_url.as_deref(),
        Some(format!("https://cli.example.com:8443/?code={code}").as_str())
    );
}

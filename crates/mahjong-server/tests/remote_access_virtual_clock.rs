mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, ORIGIN, RETRY_AFTER, SET_COOKIE};
use axum::http::{Method, StatusCode};
use common::{
    assert_minimal_403, no_reaction_wall, send_router_request, yield_tasks, ScriptedLlmClient,
    TempDirGuard, VirtualClient,
};
use mahjong_ai::AiDifficulty;
use mahjong_core::{build_preset_wall, MatchMode, PlayerAction, PresetWallProvider, Seat, Tile};
use mahjong_server::banter::QUICK_EMOTE_COOLDOWN;
use mahjong_server::llm::BanterOutput;
use mahjong_server::persona::PersonaMood;
use mahjong_server::protocol::{
    ClientMessage, LanUrlInfo, QuickEmote, RemoteAuthResponse, SeatKind, SeatingRule,
    ServerMessage, TablePace, TablePhase,
};
use mahjong_server::remote_access::{
    GLOBAL_FAILURE_THRESHOLD, GLOBAL_LOCK_DURATION, REMOTE_SESSION_COOKIE_NAME,
    SOURCE_FAILURE_THRESHOLD, SOURCE_LOCK_DURATION,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use mahjong_server::static_assets::EmbeddedStaticAssets;
use mahjong_server::{
    BanterAudioCache, ClientTrustTier, PacingMode, ServerConfig, TestRouterHandle,
};

async fn build_remote_router(
    allow_remote: bool,
    banter_cache: Option<Arc<Mutex<BanterAudioCache>>>,
) -> TestRouterHandle {
    let mut options = TableOptions::default();
    if let Some(cache) = banter_cache {
        options = options.with_banter_audio_cache(cache);
    }
    ServerConfig::default()
        .with_port(8789)
        .with_allow_remote(allow_remote)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .build_router_with_options(options)
        .await
        .expect("build full-stack remote test router")
}

fn extract_cookie_pair(headers: &axum::http::HeaderMap) -> String {
    let raw = headers
        .get(SET_COOKIE)
        .expect("expected Set-Cookie header")
        .to_str()
        .expect("valid UTF-8 Set-Cookie");
    assert!(
        raw.contains("HttpOnly") && raw.contains("SameSite=Lax") && raw.contains("Path=/"),
        "Set-Cookie missing required security attributes: {raw}"
    );
    let pair = raw.split(';').next().expect("cookie name=value").trim();
    assert!(
        pair.starts_with(&format!("{REMOTE_SESSION_COOKIE_NAME}=")),
        "unexpected cookie name: {pair}"
    );
    pair.to_string()
}

#[tokio::test(start_paused = true)]
async fn remote_gate_cookie_auth_api_isolation_and_revocation_semantics() {
    let audio_cache = Arc::new(Mutex::new(BanterAudioCache::new()));
    audio_cache.lock().unwrap().insert(
        "remote-banter-1".to_string(),
        b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec(),
        tokio::time::Instant::now(),
    );

    let handle = build_remote_router(true, Some(Arc::clone(&audio_cache))).await;
    assert!(
        handle.remote_access.is_enabled(),
        "--allow-remote must initialize remote access enabled"
    );
    let passcode = handle
        .remote_access
        .passcode()
        .expect("enabled remote access must have an 8-char passcode");
    assert_eq!(passcode.len(), 8);

    let remote_peer = Some(SocketAddr::new(
        "2001:db8:4242:0001::55".parse().unwrap(),
        50100,
    ));
    let remote_host = "[2001:db8:4242:0001::1]:8789";
    let embedded_js = EmbeddedStaticAssets::asset_paths()
        .find(|p| p.starts_with("assets/") && p.ends_with(".js"))
        .expect("embedded JS asset");
    let embedded_js_uri = format!("/{embedded_js}");

    // 1. Unauthenticated Remote GET / and GET /?code=... return minimal inline Chinese gate HTML
    //    with Cache-Control: no-store and zero SPA bundle/version leakage.
    for uri in ["/", "/?code=ABCD2345"] {
        let (status, headers, body) = send_router_request(
            &handle.router,
            remote_peer,
            Method::GET,
            uri,
            &[("Host", remote_host)],
            Body::empty(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "gate page for {uri}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("no-store")
        );
        let ct = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("text/html"), "expected text/html, got {ct}");
        let html = String::from_utf8(body).expect("gate HTML must be valid UTF-8");
        assert!(html.contains("入桌口令"), "gate HTML must be Chinese");
        assert!(
            html.contains("/api/remote-auth") && html.contains("window.location.replace(\"/\")"),
            "gate HTML must auto-post to /api/remote-auth and replace URL"
        );
        assert!(
            !html.contains("/assets/")
                && !html.contains("<link")
                && !html.contains("<script src")
                && !html.contains(env!("CARGO_PKG_VERSION")),
            "gate HTML must not leak SPA assets, external links, or server version"
        );
    }

    // 2. Unauthenticated Remote requests to all non-root paths return minimal 403.
    for (method, uri) in [
        (Method::GET, "/index.html"),
        (Method::GET, "/lobby"),
        (Method::GET, embedded_js_uri.as_str()),
        (Method::GET, "/ws"),
        (Method::GET, "/api/health"),
        (Method::GET, "/api/stats"),
        (Method::GET, "/api/preferences"),
        (Method::GET, "/api/history/export"),
        (Method::POST, "/api/history/clear"),
        (Method::GET, "/api/persona-memory"),
        (Method::POST, "/api/persona-memory/clear"),
        (Method::GET, "/api/banter-audio/remote-banter-1"),
        (Method::POST, "/api/shutdown"),
    ] {
        let mut hdrs = vec![("Host", remote_host)];
        if method == Method::POST {
            hdrs.push(("Content-Type", "application/json"));
        }
        let (status, headers, body) = send_router_request(
            &handle.router,
            remote_peer,
            method.clone(),
            uri,
            &hdrs,
            Body::empty(),
        )
        .await;
        assert_minimal_403(status, &headers, &body, &format!("unauthed {method} {uri}"));
    }

    // 3. POST /api/remote-auth enforces Content-Type: application/json (rejects text/plain).
    let (status, headers, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", remote_host), ("Content-Type", "text/plain")],
        Body::from(format!(r#"{{"code":"{passcode}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    // 4. POST /api/remote-auth with trimmed lowercase passcode succeeds and sets HttpOnly cookie.
    let padded_lower = format!("  {}  ", passcode.to_ascii_lowercase());
    let (status, headers, body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[
            ("Host", remote_host),
            ("Content-Type", "application/json; charset=utf-8"),
        ],
        Body::from(format!(r#"{{"code":"{padded_lower}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let auth_resp: RemoteAuthResponse =
        serde_json::from_slice(&body).expect("parse RemoteAuthResponse");
    assert!(auth_resp.authenticated);
    assert_eq!(auth_resp.retry_after_secs, None);
    let cookie = extract_cookie_pair(&headers);

    // 5. Authenticated Remote client accesses SPA /, /assets/*, /api/banter-audio/*, and passes /ws auth.
    let (status, _, root_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/",
        &[("Host", remote_host), (COOKIE.as_str(), cookie.as_str())],
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let spa_html = String::from_utf8(root_body).expect("SPA index.html");
    assert!(
        spa_html.contains("id=\"app\""),
        "authenticated GET / must serve SPA index.html, not gate page"
    );

    let (status, _, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        embedded_js_uri.as_str(),
        &[("Host", remote_host), (COOKIE.as_str(), cookie.as_str())],
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "authenticated JS asset access");

    let (status, audio_headers, audio_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/api/banter-audio/remote-banter-1",
        &[("Host", remote_host), (COOKIE.as_str(), cookie.as_str())],
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        audio_headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        audio_headers
            .get("x-audio-format")
            .and_then(|v| v.to_str().ok()),
        Some("pcm_s16le")
    );
    assert_eq!(
        audio_headers
            .get("x-audio-sample-rate")
            .and_then(|v| v.to_str().ok()),
        Some("24000")
    );
    assert_eq!(
        audio_headers
            .get("x-audio-channels")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    assert!(audio_body.starts_with(b"RIFF"));

    // /ws with valid cookie + matching Origin passes auth & Origin guards (Axum upgrade returns 400, not 403).
    let (ws_ok_status, _, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/ws",
        &[
            ("Host", remote_host),
            (COOKIE.as_str(), cookie.as_str()),
            (ORIGIN.as_str(), "http://[2001:db8:4242:0001::1]:8789"),
        ],
        Body::empty(),
    )
    .await;
    assert_ne!(ws_ok_status, StatusCode::FORBIDDEN);

    // /ws with valid cookie + mismatched Origin is still rejected with 403 (CSWSH guard).
    let (ws_bad_status, ws_bad_hdrs, ws_bad_body) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/ws",
        &[
            ("Host", remote_host),
            (COOKIE.as_str(), cookie.as_str()),
            (ORIGIN.as_str(), "http://evil.example:8789"),
        ],
        Body::empty(),
    )
    .await;
    assert_minimal_403(
        ws_bad_status,
        &ws_bad_hdrs,
        &ws_bad_body,
        "authenticated WS with cross-origin Origin",
    );

    // 6. Authenticated Remote client is STILL strictly forbidden from all LAN/Loopback management APIs.
    for (method, uri, payload) in [
        (Method::GET, "/api/health", ""),
        (Method::GET, "/api/stats", ""),
        (Method::GET, "/api/preferences", ""),
        (Method::GET, "/api/history/export", ""),
        (Method::POST, "/api/history/clear", r#"{"confirm":true}"#),
        (Method::GET, "/api/persona-memory", ""),
        (
            Method::POST,
            "/api/persona-memory/clear",
            r#"{"confirm":true}"#,
        ),
        (Method::POST, "/api/shutdown", "{}"),
        (Method::GET, "/api/unknown", ""),
    ] {
        let mut hdrs = vec![("Host", remote_host), (COOKIE.as_str(), cookie.as_str())];
        if method == Method::POST {
            hdrs.push(("Content-Type", "application/json"));
        }
        let (status, headers, body) = send_router_request(
            &handle.router,
            remote_peer,
            method.clone(),
            uri,
            &hdrs,
            Body::from(payload.to_string()),
        )
        .await;
        assert_minimal_403(
            status,
            &headers,
            &body,
            &format!("authenticated remote {method} {uri}"),
        );
    }
    assert!(!*handle.shutdown_rx.borrow());

    // 7. reset_code rotates passcode (old code rejected) while preserving existing valid cookie!
    let next_code = handle
        .remote_access
        .reset_code()
        .expect("reset_code returns new code");
    assert_ne!(next_code, passcode);

    let (old_code_status, _, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", remote_host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{passcode}"}}"#)),
    )
    .await;
    assert_eq!(old_code_status, StatusCode::UNAUTHORIZED);

    let (kept_cookie_status, _, _) = send_router_request(
        &handle.router,
        remote_peer,
        Method::GET,
        "/api/banter-audio/remote-banter-1",
        &[("Host", remote_host), (COOKIE.as_str(), cookie.as_str())],
        Body::empty(),
    )
    .await;
    assert_eq!(
        kept_cookie_status,
        StatusCode::OK,
        "reset_code must preserve already-authenticated remote cookies"
    );

    // 8. disable() wipes sessions immediately -> all remote paths (including GET / and /api/remote-auth) return 403.
    handle.remote_access.disable();
    for (method, uri, body_str) in [
        (Method::GET, "/", ""),
        (Method::GET, "/api/banter-audio/remote-banter-1", ""),
        (
            Method::POST,
            "/api/remote-auth",
            &format!(r#"{{"code":"{next_code}"}}"#),
        ),
    ] {
        let mut hdrs = vec![("Host", remote_host), (COOKIE.as_str(), cookie.as_str())];
        if method == Method::POST {
            hdrs.push(("Content-Type", "application/json"));
        }
        let (status, headers, body) = send_router_request(
            &handle.router,
            remote_peer,
            method.clone(),
            uri,
            &hdrs,
            Body::from(body_str.to_string()),
        )
        .await;
        assert_minimal_403(
            status,
            &headers,
            &body,
            &format!("after disable {method} {uri}"),
        );
    }
}

#[tokio::test(start_paused = true)]
async fn remote_auth_enforces_dual_layer_rate_limit_over_http_with_virtual_clock() {
    let handle = build_remote_router(true, None).await;
    let code = handle.remote_access.passcode().expect("active passcode");
    let host = "majjj.example.com:8789";

    // Layer 1A: IPv6 /64 prefix bucketing — rotating suffixes in the SAME /64 still trips the 5-failure lock.
    for i in 1..SOURCE_FAILURE_THRESHOLD {
        let peer = Some(SocketAddr::new(
            format!("2001:db8:cafe:0001::{i:x}").parse().unwrap(),
            41000 + i as u16,
        ));
        let (status, _, body) = send_router_request(
            &handle.router,
            peer,
            Method::POST,
            "/api/remote-auth",
            &[("Host", host), ("Content-Type", "application/json")],
            Body::from(r#"{"code":"WRONG111"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let parsed: RemoteAuthResponse = serde_json::from_slice(&body).unwrap();
        assert!(!parsed.authenticated);
        assert!(parsed.message.contains("口令"));
    }

    // 5th failure from yet another suffix in the SAME /64 triggers 429 + Retry-After: 300 + Chinese message.
    let fifth_peer = Some(SocketAddr::new(
        "2001:db8:cafe:0001::dead".parse().unwrap(),
        41005,
    ));
    let (status, headers, body) = send_router_request(
        &handle.router,
        fifth_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(r#"{"code":"WRONG111"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        headers.get(RETRY_AFTER).and_then(|v| v.to_str().ok()),
        Some("300")
    );
    let parsed: RemoteAuthResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.retry_after_secs, Some(300));
    assert!(parsed.message.contains("尝试次数过多"));

    // Valid code from the locked /64 is still rejected with 429 until 5m expires.
    let (locked_status, _, _) = send_router_request(
        &handle.router,
        Some(SocketAddr::new(
            "2001:db8:cafe:0001::beef".parse().unwrap(),
            41006,
        )),
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(locked_status, StatusCode::TOO_MANY_REQUESTS);

    // Different /64 succeeds immediately.
    let (other_status, _, _) = send_router_request(
        &handle.router,
        Some(SocketAddr::new(
            "2001:db8:cafe:0002::1".parse().unwrap(),
            41007,
        )),
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(other_status, StatusCode::OK);

    // Layer 1B: IPv4 /32 (and IPv4-mapped IPv6) share one bucket; success clears failures.
    let v4_peer = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 44)),
        42000,
    ));
    let v4_mapped_peer = Some(SocketAddr::new(
        IpAddr::V6(Ipv4Addr::new(203, 0, 113, 44).to_ipv6_mapped()),
        42001,
    ));
    for _ in 0..2 {
        let (st, _, _) = send_router_request(
            &handle.router,
            v4_peer,
            Method::POST,
            "/api/remote-auth",
            &[("Host", host), ("Content-Type", "application/json")],
            Body::from(r#"{"code":"WRONG222"}"#),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
    for _ in 0..2 {
        let (st, _, _) = send_router_request(
            &handle.router,
            v4_mapped_peer,
            Method::POST,
            "/api/remote-auth",
            &[("Host", host), ("Content-Type", "application/json")],
            Body::from(r#"{"code":"WRONG222"}"#),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
    // 5th attempt succeeds -> clears that IPv4 /32 bucket so subsequent single failures do NOT lock!
    let (ok_st, _, _) = send_router_request(
        &handle.router,
        v4_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(ok_st, StatusCode::OK);
    let (after_reset_st, _, _) = send_router_request(
        &handle.router,
        v4_mapped_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(r#"{"code":"WRONG222"}"#),
    )
    .await;
    assert_eq!(after_reset_st, StatusCode::UNAUTHORIZED);

    // Advance virtual clock 5 minutes: earlier /64 lock expires and global 1-minute window clears.
    tokio::time::advance(SOURCE_LOCK_DURATION + Duration::from_secs(1)).await;
    let (unlocked_st, _, _) = send_router_request(
        &handle.router,
        fifth_peer,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(unlocked_st, StatusCode::OK);

    // Layer 2: Global 20 failures across 20 distinct /64 prefixes within 1 minute triggers 60s cooldown.
    for idx in 1..GLOBAL_FAILURE_THRESHOLD {
        let peer = Some(SocketAddr::new(
            format!("2001:db8:5555:{idx:04x}::1").parse().unwrap(),
            43000,
        ));
        let (st, _, _) = send_router_request(
            &handle.router,
            peer,
            Method::POST,
            "/api/remote-auth",
            &[("Host", host), ("Content-Type", "application/json")],
            Body::from(r#"{"code":"WRONG999"}"#),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "global failure #{idx}");
    }
    let (st20, hdrs20, _) = send_router_request(
        &handle.router,
        Some(SocketAddr::new(
            "2001:db8:5555:ffff::1".parse().unwrap(),
            43020,
        )),
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(r#"{"code":"WRONG999"}"#),
    )
    .await;
    assert_eq!(st20, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        hdrs20.get(RETRY_AFTER).and_then(|v| v.to_str().ok()),
        Some("60")
    );

    // Fresh unrelated peer is blocked during global cooldown, then unlocked after 60s.
    let innocent = Some(SocketAddr::new(
        "2001:db8:7777:0001::1".parse().unwrap(),
        43099,
    ));
    let (blocked_st, _, _) = send_router_request(
        &handle.router,
        innocent,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(blocked_st, StatusCode::TOO_MANY_REQUESTS);

    tokio::time::advance(GLOBAL_LOCK_DURATION + Duration::from_secs(1)).await;
    let (recovered_st, _, _) = send_router_request(
        &handle.router,
        innocent,
        Method::POST,
        "/api/remote-auth",
        &[("Host", host), ("Content-Type", "application/json")],
        Body::from(format!(r#"{{"code":"{code}"}}"#)),
    )
    .await;
    assert_eq!(recovered_st, StatusCode::OK);
}

fn immediate_tsumo_wall_factory() -> WallProviderFactory {
    let mut east_14 = Vec::new();
    for idx in [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 31, 31] {
        east_14.push(Tile::new(idx).unwrap());
    }
    let other_13: Vec<Tile> = [4, 7, 8, 9, 12, 15, 18, 21, 24, 27, 28, 29, 30]
        .into_iter()
        .map(|idx| Tile::new(idx).unwrap())
        .collect();
    let wall =
        build_preset_wall(&east_14, &other_13, &other_13, &other_13, &[], &[]).expect("tsumo wall");
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    })
}

fn no_reaction_factory() -> WallProviderFactory {
    let wall = no_reaction_wall();
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    })
}

#[tokio::test(start_paused = true)]
async fn remote_session_sanitizes_snapshot_blocks_offline_human_takeover_allows_ai_takeover_and_cooldowns_emote(
) {
    let llm = ScriptedLlmClient::with_banter_fn(|_| {
        Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Smug,
            text: "远程来的手气还挺硬！".to_string(),
        })
    });
    let options = TableOptions::with_wall_factory(808, no_reaction_factory())
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, Some("host-mac".to_string()), options);

    let mut host =
        VirtualClient::connect_with_tier(&runtime, "host-1", "宿主阿东", ClientTrustTier::Loopback)
            .await;
    let mut lan_friend =
        VirtualClient::connect_with_tier(&runtime, "lan-1", "家人小南", ClientTrustTier::Lan).await;
    let mut remote_friend =
        VirtualClient::connect_with_tier(&runtime, "rem-1", "远程阿西", ClientTrustTier::Remote)
            .await;

    // Verify snapshot flags & LAN URL sanitization across all three tiers.
    host.send(&runtime, &ClientMessage::RequestSnapshot).await;
    let host_snap = host.recv_snapshot().await;
    assert!(host_snap.host_is_loopback);
    assert!(!host_snap.client_is_remote);
    assert_eq!(
        host_snap.lan_urls.local_hostname_url.as_deref(),
        Some("http://host-mac.local:8789")
    );

    lan_friend
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let lan_snap = lan_friend.recv_snapshot().await;
    assert!(!lan_snap.host_is_loopback);
    assert!(!lan_snap.client_is_remote);
    assert_eq!(
        lan_snap.lan_urls.local_hostname_url.as_deref(),
        Some("http://host-mac.local:8789")
    );

    remote_friend
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let remote_snap = remote_friend.recv_snapshot().await;
    assert!(!remote_snap.host_is_loopback);
    assert!(remote_snap.client_is_remote);
    assert_eq!(
        remote_snap.lan_urls,
        LanUrlInfo::default(),
        "remote TableSnapshot must strip all lan_urls"
    );

    // Host sits East, LAN friend sits South, start match (West & North are AI).
    host.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = host.recv_snapshot().await;
    lan_friend
        .send(
            &runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::South),
            },
        )
        .await;
    let _ = lan_friend.recv_snapshot().await;
    host.send(
        &runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    host.send(&runtime, &ClientMessage::StartMatch).await;
    let _ = host.recv_snapshot().await;
    let _ = lan_friend.recv_snapshot().await;
    let _ = remote_friend.recv_snapshot().await;
    yield_tasks().await;
    let _ = host.drain_all();
    let _ = lan_friend.drain_all();
    let _ = remote_friend.drain_all();

    // LAN friend goes offline; remote observer attempts TakeOverSeat on offline human South -> MUST BE REJECTED!
    runtime.on_disconnect(lan_friend.conn_id).await;
    let _ = remote_friend.drain_all();
    remote_friend
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: Seat::South,
                confirm: true,
            },
        )
        .await;
    let reject_msgs = remote_friend.drain_all();
    assert!(
        reject_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::ErrorNotice { message } if message.contains("远程") && message.contains("离线人类")
        )),
        "remote TakeOverSeat on offline human seat must be rejected with Chinese ErrorNotice, got {reject_msgs:?}"
    );

    // Verify remote observer is STILL unseated and South is still offline LAN friend.
    remote_friend
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let after_reject = remote_friend.recv_snapshot().await;
    assert_eq!(after_reject.your_seat, None);
    assert_eq!(after_reject.seats[Seat::South.index()].nickname, "家人小南");
    assert!(!after_reject.seats[Seat::South.index()].connected);

    // Remote observer CAN take over mid-match AI seat West with confirm: true!
    remote_friend
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: Seat::West,
                confirm: true,
            },
        )
        .await;
    let took_over_snap = remote_friend.recv_snapshot().await;
    assert_eq!(took_over_snap.your_seat, Some(Seat::West));
    assert!(took_over_snap.client_is_remote);
    assert_eq!(took_over_snap.lan_urls, LanUrlInfo::default());
    assert_eq!(
        took_over_snap.seats[Seat::West.index()].kind,
        SeatKind::Human
    );
    assert_eq!(
        took_over_snap.seats[Seat::West.index()].nickname,
        "远程阿西"
    );
    let remote_table_id = took_over_snap.table_id.clone();
    let remote_seat_token = took_over_snap
        .seat_token
        .clone()
        .expect("remote seat_token");

    // Remote player quick emote cooldown: first emote broadcasts, rapid follow-up is rejected without LLM call.
    let _ = remote_friend.drain_all();
    remote_friend
        .send(
            &runtime,
            &ClientMessage::SendQuickEmote {
                emote: QuickEmote::NicePlay,
            },
        )
        .await;
    yield_tasks().await;
    let first_emote = remote_friend.drain_all();
    assert!(first_emote.iter().any(|m| matches!(
        m,
        ServerMessage::QuickEmoteBroadcast {
            seat: Seat::West,
            emote: QuickEmote::NicePlay,
            ..
        }
    )));

    // Even if the remote player disconnects and reconnects with their own seat_token,
    // they restore West seamlessly AND still cannot bypass the per-seat quick-emote cooldown!
    runtime.on_disconnect(remote_friend.conn_id).await;

    // While West (remote human) is offline, another remote observer ALSO cannot take over West!
    let mut remote_intruder = VirtualClient::connect_with_tier(
        &runtime,
        "rem-2",
        "另一个远程访客",
        ClientTrustTier::Remote,
    )
    .await;
    remote_intruder
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: Seat::West,
                confirm: true,
            },
        )
        .await;
    let intruder_msgs = remote_intruder.drain_all();
    assert!(intruder_msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::ErrorNotice { .. })));

    // Original remote player reconnects with own seat_token -> seamlessly restores West!
    let mut remote_reconnected =
        VirtualClient::connect_with_tier(&runtime, "rem-1", "远程阿西", ClientTrustTier::Remote)
            .await;
    remote_reconnected
        .send(
            &runtime,
            &ClientMessage::Hello {
                client_id: "rem-1".to_string(),
                nickname: "远程阿西".to_string(),
                table_id: Some(remote_table_id),
                seat_token: Some(remote_seat_token),
            },
        )
        .await;
    let restored_snap = remote_reconnected.recv_snapshot().await;
    assert_eq!(restored_snap.your_seat, Some(Seat::West));
    assert!(restored_snap.client_is_remote);
    assert_eq!(restored_snap.lan_urls, LanUrlInfo::default());
    let _ = remote_reconnected.drain_all();

    // Still inside QUICK_EMOTE_COOLDOWN (only 1s elapsed) -> rejected without LLM call!
    tokio::time::advance(Duration::from_secs(1)).await;
    let prompts_before = llm.captured().len();
    remote_reconnected
        .send(
            &runtime,
            &ClientMessage::SendQuickEmote {
                emote: QuickEmote::HurryUp,
            },
        )
        .await;
    yield_tasks().await;
    let spam_msgs = remote_reconnected.drain_all();
    assert!(
        spam_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::ActionRejected { reason, .. } if reason.contains("冷却")
        )),
        "remote quick emote inside cooldown must be rejected: {spam_msgs:?}"
    );
    assert_eq!(llm.captured().len(), prompts_before);

    // After cooldown expires -> allowed again.
    tokio::time::advance(QUICK_EMOTE_COOLDOWN).await;
    remote_reconnected
        .send(
            &runtime,
            &ClientMessage::SendQuickEmote {
                emote: QuickEmote::HurryUp,
            },
        )
        .await;
    yield_tasks().await;
    let after_cd = remote_reconnected.drain_all();
    assert!(after_cd.iter().any(|m| matches!(
        m,
        ServerMessage::QuickEmoteBroadcast {
            seat: Seat::West,
            emote: QuickEmote::HurryUp,
            ..
        }
    )));
}

#[tokio::test(start_paused = true)]
async fn remote_human_match_records_into_history_and_persona_memory() {
    let temp = TempDirGuard::new("majjj_remote_history_and_memory");
    let llm = ScriptedLlmClient::with_handlers(
        |_| {
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Tilted,
                text: "起手自摸？远程外援这么凶！".to_string(),
            })
        },
        |_| Ok("远程外援小张开局就自摸，得防着点。".to_string()),
    );
    let options = TableOptions::with_wall_factory(909, immediate_tsumo_wall_factory())
        .with_pacing(PacingMode::Immediate)
        .with_data_dir(temp.path())
        .with_llm_client(llm);
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut remote_east = VirtualClient::connect_with_tier(
        &runtime,
        "rem-east",
        "远程外援小张",
        ClientTrustTier::Remote,
    )
    .await;
    remote_east
        .send(
            &runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::East),
            },
        )
        .await;
    let _ = remote_east.recv_snapshot().await;
    remote_east
        .send(
            &runtime,
            &ClientMessage::UpdateLobbyConfig {
                mode: MatchMode::FreeTable,
                seating: SeatingRule::KeepLobbySeats,
                ai_difficulty: AiDifficulty::Strong,
                pace: TablePace::Standard,
            },
        )
        .await;
    remote_east.send(&runtime, &ClientMessage::StartMatch).await;
    let start_snap = remote_east.recv_snapshot().await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);
    assert!(start_snap.client_is_remote);
    assert_eq!(start_snap.lan_urls, LanUrlInfo::default());

    // Remote player wins by immediate Tsumo and ends FreeTable match.
    remote_east
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: start_snap.seq,
                client_action_id: "remote-tsumo".to_string(),
                action: PlayerAction::Tsumo,
            },
        )
        .await;
    yield_tasks().await;

    remote_east
        .send(&runtime, &ClientMessage::RequestEndFreeTable)
        .await;
    yield_tasks().await;

    // Verify history stats and persona memory recorded the remote human player's nickname and hand.
    let stats = runtime.history_stats().await;
    assert_eq!(stats.total_hands, 1);
    assert_eq!(stats.total_matches, 1);
    assert!(
        stats
            .players
            .iter()
            .any(|p| p.nickname == "远程外援小张" && p.settled_hands == 1 && p.tsumo_wins == 1),
        "remote player must be recorded in history stats: {:?}",
        stats.players
    );

    let memory = runtime.persona_memory_snapshot().await;
    assert!(
        memory.total_pairs >= 1,
        "remote player must be recorded in persona memory: {memory:?}"
    );
    assert!(
        memory
            .personas
            .iter()
            .flat_map(|entry| entry.players.iter())
            .any(|p| p.player_nickname == "远程外援小张" && p.ledger.hands_played >= 1),
        "remote player ledger missing from persona memory: {memory:?}"
    );
}

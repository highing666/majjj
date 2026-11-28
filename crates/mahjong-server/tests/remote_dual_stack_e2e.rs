mod common;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{connect_ws, raw_http_request, recv_snapshot, send_msg};
use futures_util::StreamExt;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, ORIGIN, SET_COOKIE};
use hyper::StatusCode;
use mahjong_server::protocol::{
    ClientMessage, LanUrlInfo, RemoteAccessStatusResponse, RemoteAuthResponse, ServerMessage,
};
use mahjong_server::{BanterAudioCache, IdleTimeouts, ServerConfig, TableOptions};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn recv_pong<S>(ws: &mut S, expected_ts_ms: u64)
where
    S: futures_util::Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    for _ in 0..8 {
        let frame = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("WS must respond with ServerPong")
            .expect("ws item")
            .expect("ws ok");
        if let WsMessage::Text(text) = frame {
            match serde_json::from_str::<ServerMessage>(&text).unwrap() {
                ServerMessage::ServerPong { ts_ms } if ts_ms == expected_ts_ms => return,
                ServerMessage::TtsStatusNotice { .. }
                | ServerMessage::LlmStatusNotice { .. }
                | ServerMessage::LobbyUpdated { .. }
                | ServerMessage::ServerPing { .. } => continue,
                other => panic!(
                    "unexpected frame while waiting for ServerPong({expected_ts_ms}): {other:?}"
                ),
            }
        }
    }
    panic!("did not receive ServerPong({expected_ts_ms})");
}

#[tokio::test]
async fn real_dual_stack_remote_cookie_ws_audio_idle_keepalive_and_instant_disable_e2e() {
    let audio_cache = Arc::new(Mutex::new(BanterAudioCache::new()));
    audio_cache.lock().unwrap().insert(
        "e2e-banter-wav".to_string(),
        b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec(),
        tokio::time::Instant::now(),
    );

    let options = TableOptions::default().with_banter_audio_cache(Arc::clone(&audio_cache));
    let server = ServerConfig::default()
        .with_port(0)
        .with_no_open(true)
        .with_allow_remote(true)
        .with_idle_timeouts(IdleTimeouts {
            lobby_or_finished: Duration::from_millis(120),
            in_match: Duration::from_millis(120),
        })
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .spawn_with_options(options)
        .await
        .expect("spawn dual-stack server with allow_remote=true");

    let port = server.addr().port();
    let v6_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    let v4_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    // Connecting to [::1] / 127.0.0.1 with a public domain Host classifies strictly as `Remote`.
    let public_host = format!("majjj.example.com:{port}");
    let public_origin = format!("http://{public_host}");
    let passcode = server
        .remote_access()
        .passcode()
        .expect("allow_remote=true must initialize active passcode");

    // 1. Unauthenticated Remote GET / returns gate HTML; /ws and /api/banter-audio return 403.
    let (gate_status, gate_hdrs, gate_body) = raw_http_request(
        v6_addr,
        "GET",
        "/?code=TEST",
        &[(HOST.as_str(), public_host.as_str())],
        b"",
    )
    .await;
    assert_eq!(gate_status, StatusCode::OK);
    assert_eq!(
        gate_hdrs.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let gate_html = String::from_utf8(gate_body).unwrap();
    assert!(gate_html.contains("入桌口令"));

    let (unauthed_audio_st, _, _) = raw_http_request(
        v6_addr,
        "GET",
        "/api/banter-audio/e2e-banter-wav",
        &[(HOST.as_str(), public_host.as_str())],
        b"",
    )
    .await;
    assert_eq!(unauthed_audio_st, StatusCode::FORBIDDEN);

    let mut unauthed_ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("ws req");
    unauthed_ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    unauthed_ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    let unauthed_ws_err = connect_async(unauthed_ws_req)
        .await
        .expect_err("unauthenticated remote WS must fail");
    assert!(unauthed_ws_err.to_string().contains("403"));

    // 2. Exchange passcode via POST /api/remote-auth over IPv6 -> receive HttpOnly cookie.
    let (auth_status, auth_hdrs, auth_body) = raw_http_request(
        v6_addr,
        "POST",
        "/api/remote-auth",
        &[
            (HOST.as_str(), public_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        format!(r#"{{"code":"{passcode}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(auth_status, StatusCode::OK);
    let auth_json: RemoteAuthResponse = serde_json::from_slice(&auth_body).unwrap();
    assert!(auth_json.authenticated);
    let cookie_pair = auth_hdrs
        .get(SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    // 3. Authenticated Remote can fetch /api/banter-audio/* and SPA /, but NOT /api/stats or /api/health.
    let (audio_st, audio_hdrs, audio_bytes) = raw_http_request(
        v4_addr,
        "GET",
        "/api/banter-audio/e2e-banter-wav",
        &[
            (HOST.as_str(), public_host.as_str()),
            (COOKIE.as_str(), cookie_pair.as_str()),
        ],
        b"",
    )
    .await;
    assert_eq!(audio_st, StatusCode::OK);
    assert_eq!(
        audio_hdrs.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        audio_hdrs
            .get("x-audio-format")
            .and_then(|v| v.to_str().ok()),
        Some("pcm_s16le")
    );
    assert_eq!(
        audio_hdrs
            .get("x-audio-sample-rate")
            .and_then(|v| v.to_str().ok()),
        Some("24000")
    );
    assert_eq!(
        audio_hdrs
            .get("x-audio-channels")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    assert!(audio_bytes.starts_with(b"RIFF"));

    for forbidden_uri in [
        "/api/health",
        "/api/stats",
        "/api/preferences",
        "/api/persona-memory",
    ] {
        let (st, _, _) = raw_http_request(
            v4_addr,
            "GET",
            forbidden_uri,
            &[
                (HOST.as_str(), public_host.as_str()),
                (COOKIE.as_str(), cookie_pair.as_str()),
            ],
            b"",
        )
        .await;
        assert_eq!(
            st,
            StatusCode::FORBIDDEN,
            "remote must not access {forbidden_uri}"
        );
    }

    // 4. Authenticated Remote WS connects cleanly over IPv6 with cookie + matching Origin.
    let mut remote_ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("remote ws req");
    remote_ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    remote_ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    remote_ws_req
        .headers_mut()
        .insert(COOKIE, cookie_pair.parse().unwrap());
    let (mut remote_ws, _) = connect_async(remote_ws_req)
        .await
        .expect("authenticated remote WS must connect");

    send_msg(
        &mut remote_ws,
        &ClientMessage::Hello {
            client_id: "remote-e2e".to_string(),
            nickname: "远方来客".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let snap = recv_snapshot(&mut remote_ws).await;
    assert!(snap.client_is_remote);
    assert!(!snap.host_is_loopback);
    assert_eq!(snap.lan_urls, LanUrlInfo::default());
    assert_eq!(server.remote_access().active_remote_connections(), 1);

    // 5. Desktop idle shutdown monitor counts active remote WS and keeps server alive past 120ms!
    tokio::time::sleep(Duration::from_millis(260)).await;
    send_msg(&mut remote_ws, &ClientMessage::ClientPing { ts_ms: 42 }).await;
    recv_pong(&mut remote_ws, 42).await;

    // 6. reset_code keeps active remote WS open while rejecting old passcode for new logins.
    let _new_code = server.remote_access().reset_code().expect("new passcode");
    let (old_auth_st, _, _) = raw_http_request(
        v6_addr,
        "POST",
        "/api/remote-auth",
        &[
            (HOST.as_str(), public_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        format!(r#"{{"code":"{passcode}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(old_auth_st, StatusCode::UNAUTHORIZED);

    send_msg(&mut remote_ws, &ClientMessage::ClientPing { ts_ms: 99 }).await;
    recv_pong(&mut remote_ws, 99).await;

    // 7. Connect a loopback WS too, then call disable():
    //    - Remote WS immediately receives ServerMessage::RemoteAccessDisabled and closes.
    //    - Loopback WS stays alive!
    //    - active_remote_connections() returns to 0.
    //    - Reconnecting remote WS with old cookie is rejected with 403.
    let mut loopback_ws = connect_ws(v4_addr).await;
    send_msg(
        &mut loopback_ws,
        &ClientMessage::Hello {
            client_id: "host-e2e".to_string(),
            nickname: "宿主".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let host_snap = recv_snapshot(&mut loopback_ws).await;
    assert!(host_snap.host_is_loopback);
    assert!(!host_snap.client_is_remote);

    server.remote_access().disable();

    let mut saw_disabled_notice = false;
    let mut saw_close = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_secs(2), remote_ws.next())
            .await
            .expect("remote WS must close promptly on disable")
        {
            Some(Ok(WsMessage::Text(text))) => {
                if matches!(
                    serde_json::from_str::<ServerMessage>(&text).unwrap(),
                    ServerMessage::RemoteAccessDisabled { .. }
                ) {
                    saw_disabled_notice = true;
                }
            }
            Some(Ok(WsMessage::Close(_))) | None => {
                saw_close = true;
                break;
            }
            _ => {}
        }
    }
    assert!(
        saw_disabled_notice,
        "remote WS must receive ServerMessage::RemoteAccessDisabled on disable()"
    );
    assert!(saw_close, "remote WS must close promptly on disable()");

    // Wait briefly for server disconnect cleanup to decrement active_remote_connections.
    for _ in 0..40 {
        if server.remote_access().active_remote_connections() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(server.remote_access().active_remote_connections(), 0);

    // Loopback WS stays alive and responsive!
    send_msg(&mut loopback_ws, &ClientMessage::ClientPing { ts_ms: 777 }).await;
    recv_pong(&mut loopback_ws, 777).await;

    // Reconnecting remote WS with old cookie is rejected with 403.
    let mut retry_ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("retry ws req");
    retry_ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    retry_ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    retry_ws_req
        .headers_mut()
        .insert(COOKIE, cookie_pair.parse().unwrap());
    let retry_err = connect_async(retry_ws_req)
        .await
        .expect_err("old remote cookie must be rejected after disable()");
    assert!(retry_err.to_string().contains("403"));

    let _ = loopback_ws.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn host_http_control_endpoints_drive_live_remote_ws_reset_code_and_instant_disable_e2e() {
    let server = ServerConfig::default()
        .with_port(0)
        .with_no_open(true)
        .with_allow_remote(false)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .spawn()
        .await
        .expect("spawn dual-stack server with remote disabled");

    let port = server.addr().port();
    let v4_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let v6_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    let loopback_host = format!("127.0.0.1:{port}");
    let public_host = format!("majjj.example.com:{port}");
    let public_origin = format!("http://{public_host}");

    // 1. Host enables remote access via HTTP POST /api/remote-access/enable on Loopback.
    let (en_st, _, en_body) = raw_http_request(
        v4_addr,
        "POST",
        "/api/remote-access/enable",
        &[
            (HOST.as_str(), loopback_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        b"{}",
    )
    .await;
    assert_eq!(en_st, StatusCode::OK);
    let enabled: RemoteAccessStatusResponse = serde_json::from_slice(&en_body).unwrap();
    assert!(enabled.enabled);
    assert_eq!(enabled.active_remote_connections, 0);
    let code1 = enabled.passcode.expect("passcode");

    // 2. Remote client authenticates over IPv6 and opens a live WS.
    let (auth_st, auth_hdrs, _) = raw_http_request(
        v6_addr,
        "POST",
        "/api/remote-auth",
        &[
            (HOST.as_str(), public_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        format!(r#"{{"code":"{code1}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(auth_st, StatusCode::OK);
    let cookie_pair = auth_hdrs
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    let mut ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("ws req");
    ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    ws_req
        .headers_mut()
        .insert(COOKIE, cookie_pair.parse().unwrap());
    let (mut remote_ws, _) = connect_async(ws_req).await.expect("remote ws connect");
    send_msg(
        &mut remote_ws,
        &ClientMessage::Hello {
            client_id: "remote-http-ctrl".to_string(),
            nickname: "远客".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let _ = recv_snapshot(&mut remote_ws).await;

    // 3. Host GET /api/remote-access reflects active_remote_connections == 1.
    let (st_get, _, get_body) = raw_http_request(
        v4_addr,
        "GET",
        "/api/remote-access",
        &[(HOST.as_str(), loopback_host.as_str())],
        b"",
    )
    .await;
    assert_eq!(st_get, StatusCode::OK);
    let status_live: RemoteAccessStatusResponse = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(status_live.active_remote_connections, 1);

    // 4. Host POST /api/remote-access/reset-code rotates passcode WITHOUT closing active remote WS.
    let (st_rst, _, rst_body) = raw_http_request(
        v4_addr,
        "POST",
        "/api/remote-access/reset-code",
        &[
            (HOST.as_str(), loopback_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        b"{}",
    )
    .await;
    assert_eq!(st_rst, StatusCode::OK);
    let rotated: RemoteAccessStatusResponse = serde_json::from_slice(&rst_body).unwrap();
    assert_ne!(rotated.passcode.as_deref(), Some(code1.as_str()));
    assert_eq!(rotated.active_remote_connections, 1);

    send_msg(&mut remote_ws, &ClientMessage::ClientPing { ts_ms: 321 }).await;
    recv_pong(&mut remote_ws, 321).await;

    // 5. Host POST /api/remote-access/disable immediately pushes RemoteAccessDisabled and closes WS.
    let (st_dis, _, dis_body) = raw_http_request(
        v4_addr,
        "POST",
        "/api/remote-access/disable",
        &[
            (HOST.as_str(), loopback_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        b"{}",
    )
    .await;
    assert_eq!(st_dis, StatusCode::OK);
    let disabled: RemoteAccessStatusResponse = serde_json::from_slice(&dis_body).unwrap();
    assert!(!disabled.enabled);
    assert_eq!(disabled.passcode, None);

    let mut saw_notice = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_secs(2), remote_ws.next())
            .await
            .expect("remote WS must close promptly")
        {
            Some(Ok(WsMessage::Text(text))) => {
                if matches!(
                    serde_json::from_str::<ServerMessage>(&text).unwrap(),
                    ServerMessage::RemoteAccessDisabled { .. }
                ) {
                    saw_notice = true;
                }
            }
            Some(Ok(WsMessage::Close(_))) | None => break,
            _ => {}
        }
    }
    assert!(saw_notice, "remote WS must receive RemoteAccessDisabled");

    server.shutdown().await;
}

#[tokio::test]
async fn remote_ws_still_disconnects_on_immediate_disable_then_enable() {
    let server = ServerConfig::default()
        .with_port(0)
        .with_no_open(true)
        .with_allow_remote(true)
        .with_llm_client(common::stub_llm_client())
        .with_no_tts(true)
        .spawn()
        .await
        .expect("spawn dual-stack server with remote enabled");

    let port = server.addr().port();
    let v6_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
    let public_host = format!("majjj.example.com:{port}");
    let public_origin = format!("http://{public_host}");
    let code1 = server.remote_access().passcode().expect("initial passcode");

    let (auth_st, auth_hdrs, _) = raw_http_request(
        v6_addr,
        "POST",
        "/api/remote-auth",
        &[
            (HOST.as_str(), public_host.as_str()),
            (CONTENT_TYPE.as_str(), "application/json"),
        ],
        format!(r#"{{"code":"{code1}"}}"#).as_bytes(),
    )
    .await;
    assert_eq!(auth_st, StatusCode::OK);
    let old_cookie = auth_hdrs
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string();

    let mut ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("ws req");
    ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    ws_req
        .headers_mut()
        .insert(COOKIE, old_cookie.parse().unwrap());
    let (mut remote_ws, _) = connect_async(ws_req).await.expect("remote ws connect");
    send_msg(
        &mut remote_ws,
        &ClientMessage::Hello {
            client_id: "remote-fast-toggle".to_string(),
            nickname: "快切远客".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let _ = recv_snapshot(&mut remote_ws).await;
    assert_eq!(server.remote_access().active_remote_connections(), 1);

    // Immediately disable() and re-enable() back-to-back without yielding.
    server.remote_access().disable();
    let code2 = server.remote_access().enable();
    assert!(server.remote_access().is_enabled());
    assert_ne!(code1, code2);

    // The pre-revocation remote WS must STILL receive RemoteAccessDisabled and close!
    let mut saw_notice = false;
    let mut saw_close = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_secs(2), remote_ws.next())
            .await
            .expect("revoked remote WS must close even after immediate re-enable")
        {
            Some(Ok(WsMessage::Text(text))) => {
                if matches!(
                    serde_json::from_str::<ServerMessage>(&text).unwrap(),
                    ServerMessage::RemoteAccessDisabled { .. }
                ) {
                    saw_notice = true;
                }
            }
            Some(Ok(WsMessage::Close(_))) | None => {
                saw_close = true;
                break;
            }
            _ => {}
        }
    }
    assert!(
        saw_notice,
        "revoked remote WS must receive ServerMessage::RemoteAccessDisabled on fast disable->enable"
    );
    assert!(
        saw_close,
        "revoked remote WS must close on fast disable->enable"
    );

    for _ in 0..40 {
        if server.remote_access().active_remote_connections() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(server.remote_access().active_remote_connections(), 0);

    // Reconnecting with the wiped old cookie must still be rejected even though remote is re-enabled.
    let mut stale_ws_req = format!("ws://{v6_addr}/ws")
        .into_client_request()
        .expect("stale ws req");
    stale_ws_req
        .headers_mut()
        .insert(HOST, public_host.parse().unwrap());
    stale_ws_req
        .headers_mut()
        .insert(ORIGIN, public_origin.parse().unwrap());
    stale_ws_req
        .headers_mut()
        .insert(COOKIE, old_cookie.parse().unwrap());
    let stale_err = connect_async(stale_ws_req)
        .await
        .expect_err("old cookie must stay revoked after disable->enable");
    assert!(stale_err.to_string().contains("403"));

    server.shutdown().await;
}

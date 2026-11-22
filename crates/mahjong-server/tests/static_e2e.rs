mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{
    connect_ws, fetch_health, http_get, recv_msg, recv_snapshot, send_msg, spawn_seeded_server,
};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::StatusCode;
use mahjong_core::{PlayerAction, PlayerEventKind, PublicPhase};
use mahjong_server::protocol::{ClientMessage, SeatKind, ServerMessage, TablePhase};

fn web_dist_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ parent")
        .parent()
        .expect("repo root")
        .join("web/dist")
}

struct BuiltDist {
    dir: PathBuf,
    index_html: String,
    hashed_assets: Vec<String>,
    referenced_assets: Vec<String>,
}

fn discover_built_dist() -> BuiltDist {
    let dir = web_dist_dir();
    assert!(
        dir.is_dir(),
        "expected built frontend directory at {}; run `(cd web && npm run build)` or `./scripts/check.sh` first",
        dir.display()
    );

    let index_path = dir.join("index.html");
    assert!(
        index_path.is_file(),
        "expected built frontend entrypoint at {}; run `(cd web && npm run build)` or `./scripts/check.sh` first",
        index_path.display()
    );
    let index_html = fs::read_to_string(&index_path).expect("read web/dist/index.html");

    let assets_dir = dir.join("assets");
    assert!(
        assets_dir.is_dir(),
        "expected built assets directory at {}",
        assets_dir.display()
    );

    let mut hashed_assets = Vec::new();
    for entry in fs::read_dir(&assets_dir).expect("read web/dist/assets") {
        let entry = entry.expect("asset directory entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let is_bundle = (name.ends_with(".js") || name.ends_with(".css")) && name.contains('-');
        if is_bundle {
            hashed_assets.push(name.to_owned());
        }
    }
    hashed_assets.sort();
    assert!(
        !hashed_assets.is_empty(),
        "expected hashed .js or .css assets under {}",
        assets_dir.display()
    );

    let referenced_assets: Vec<String> = hashed_assets
        .iter()
        .filter(|name| index_html.contains(&format!("/assets/{name}")))
        .cloned()
        .collect();
    assert!(
        !referenced_assets.is_empty(),
        "expected web/dist/index.html to reference at least one hashed asset under /assets/; index.html:\n{index_html}"
    );

    BuiltDist {
        dir,
        index_html,
        hashed_assets,
        referenced_assets,
    }
}

#[tokio::test]
async fn production_static_hosting_serves_built_dist_health_and_same_origin_ws_match() {
    let dist = discover_built_dist();
    let server = spawn_seeded_server(29, Some(dist.dir.clone())).await;
    let addr = server.addr();

    // 1. Root HTML returns 200, Cache-Control: no-cache, and built asset references.
    let (root_status, root_headers, root_body) = http_get(addr, "/").await;
    assert_eq!(root_status, StatusCode::OK);
    assert_eq!(
        root_headers
            .get(CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );
    assert!(root_headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .contains("text/html"));
    let root_text = String::from_utf8(root_body).expect("index.html must be UTF-8");
    assert_eq!(root_text, dist.index_html);
    for asset_name in &dist.referenced_assets {
        assert!(
            root_text.contains(&format!("/assets/{asset_name}")),
            "served index.html missing asset reference /assets/{asset_name}"
        );
    }

    // 2. Hashed assets under /assets/ return 200 and immutable cache headers.
    for asset_name in &dist.hashed_assets {
        let uri_path = format!("/assets/{asset_name}");
        let (status, headers, body) = http_get(addr, &uri_path).await;
        assert_eq!(status, StatusCode::OK, "asset={uri_path}");
        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok()),
            Some("public, max-age=31536000, immutable"),
            "asset={uri_path}"
        );
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if asset_name.ends_with(".js") {
            assert!(
                content_type.contains("javascript"),
                "unexpected JS content-type for {uri_path}: {content_type}"
            );
        } else if asset_name.ends_with(".css") {
            assert!(
                content_type.contains("text/css"),
                "unexpected CSS content-type for {uri_path}: {content_type}"
            );
        }
        let expected_bytes =
            fs::read(dist.dir.join("assets").join(asset_name)).expect("read asset on disk");
        assert_eq!(body, expected_bytes, "asset body mismatch for {uri_path}");
    }

    // 3. Health endpoint returns valid JSON HealthResponse and Cache-Control: no-store.
    let initial_health = fetch_health(addr).await;
    assert_eq!(initial_health.app, "majjj");
    assert_eq!(initial_health.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(initial_health.phase, TablePhase::Lobby);
    assert_eq!(initial_health.online_humans, 0);
    assert_eq!(initial_health.port, addr.port());

    // 4. Same-origin /ws completes Hello -> JoinSeat -> StartMatch against real TableSession + core + AI.
    let mut ws = connect_ws(addr).await;
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            client_id: "static-e2e-client".to_string(),
            nickname: "Host".to_string(),
            table_id: None,
            seat_token: None,
        },
    )
    .await;
    let hello_snap = recv_snapshot(&mut ws).await;
    assert_eq!(hello_snap.phase, TablePhase::Lobby);
    assert_eq!(hello_snap.your_seat, None);
    assert!(hello_snap.seat_token.is_some());

    send_msg(&mut ws, &ClientMessage::JoinSeat { seat: None }).await;
    let joined_snap = recv_snapshot(&mut ws).await;
    assert_eq!(joined_snap.phase, TablePhase::Lobby);
    let lobby_seat = joined_snap.your_seat.expect("human should be seated");
    assert_eq!(joined_snap.seats[lobby_seat.index()].kind, SeatKind::Human);
    assert!(joined_snap.seats[lobby_seat.index()].connected);

    let lobby_health = fetch_health(addr).await;
    assert_eq!(lobby_health.phase, TablePhase::Lobby);
    assert_eq!(lobby_health.online_humans, 1);

    send_msg(&mut ws, &ClientMessage::StartMatch).await;
    let match_snap = recv_snapshot(&mut ws).await;
    assert_eq!(match_snap.phase, TablePhase::InMatch);
    assert!(match_snap.your_seat.is_some());
    assert_eq!(
        match_snap
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Human)
            .count(),
        1
    );
    assert_eq!(
        match_snap
            .seats
            .iter()
            .filter(|s| s.kind == SeatKind::Ai && s.ai_style.is_some())
            .count(),
        3
    );

    let view = match_snap
        .player_view
        .expect("seated human must receive authoritative PlayerView");
    assert_eq!(view.phase, PublicPhase::Playing);
    assert_eq!(view.seq, match_snap.seq);

    let match_health = fetch_health(addr).await;
    assert_eq!(match_health.phase, TablePhase::InMatch);
    assert_eq!(match_health.online_humans, 1);

    // Wait for first legal human action (AI acts automatically if an AI seat is dealer), then submit one action.
    let mut current_seq = match_snap.seq;
    let mut legal_actions = view.legal_actions;
    let mut observed_ai_event_batch = false;
    while legal_actions.is_empty() {
        match recv_msg(&mut ws).await {
            ServerMessage::EventBatch {
                to_seq,
                events,
                updated_legal_actions,
                ..
            } => {
                assert!(!events.is_empty(), "AI step must emit PlayerEvent(s)");
                observed_ai_event_batch = true;
                current_seq = to_seq;
                legal_actions = updated_legal_actions;
            }
            ServerMessage::TableSnapshot(snap) => {
                current_seq = snap.seq;
                legal_actions = snap
                    .player_view
                    .map_or_else(Vec::new, |pv| pv.legal_actions);
            }
            other => panic!("unexpected WS message while waiting for turn: {other:?}"),
        }
    }

    let chosen_action = *legal_actions
        .iter()
        .find(|action| matches!(action, PlayerAction::Discard { .. }))
        .unwrap_or(&legal_actions[0]);
    send_msg(
        &mut ws,
        &ClientMessage::SubmitAction {
            expected_seq: current_seq,
            client_action_id: "static-e2e-act-1".to_string(),
            action: chosen_action,
        },
    )
    .await;

    match recv_msg(&mut ws).await {
        ServerMessage::EventBatch { to_seq, .. } => {
            assert!(to_seq >= current_seq);
            current_seq = to_seq;
        }
        other => panic!("expected EventBatch after SubmitAction, got {other:?}"),
    }

    // If the human was initial dealer, wait for at least one AI-driven EventBatch afterward.
    while !observed_ai_event_batch {
        match recv_msg(&mut ws).await {
            ServerMessage::EventBatch { to_seq, events, .. } => {
                assert!(to_seq >= current_seq);
                if !events.is_empty() {
                    observed_ai_event_batch = true;
                }
                if events
                    .iter()
                    .any(|e| matches!(e.kind, PlayerEventKind::HandSettled { .. }))
                {
                    break;
                }
            }
            ServerMessage::TableSnapshot(_) => {}
            other => panic!("unexpected WS message during AI progression: {other:?}"),
        }
    }
    assert!(
        observed_ai_event_batch,
        "expected mahjong-ai to advance the match via EventBatch"
    );

    let _ = ws.close(None).await;
    server.shutdown().await;
}

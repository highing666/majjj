//! Loopback-only host control plane for `远程联机` lifecycle, passcode rotation,
//! public IPv6 discovery reporting, and custom public entry persistence.

use axum::body::Bytes;
use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;

use crate::lan::{discover_system_public_ipv6_urls, PublicIpv6UrlInfo};
use crate::protocol::{RemoteAccessStatusResponse, SetPublicUrlRequest};
use crate::public_url::resolve_public_entry;
use crate::remote_access::RemoteAccessManager;
use crate::security::{
    json_with_no_store, require_json_post_middleware, require_loopback_middleware,
};
use crate::session::TableRuntime;
use crate::storage::PublicUrlSaveError;

/// Shared state slice injected into Loopback-only host remote-access control handlers.
#[derive(Clone)]
pub(crate) struct RemoteControlState {
    pub(crate) port: u16,
    pub(crate) cli_public_url: Option<String>,
    pub(crate) public_ipv6_override: Option<PublicIpv6UrlInfo>,
    pub(crate) session: TableRuntime,
    pub(crate) remote_access: RemoteAccessManager,
}

impl RemoteControlState {
    async fn discover_public_ipv6_urls(&self) -> PublicIpv6UrlInfo {
        if let Some(injected) = &self.public_ipv6_override {
            return injected.clone();
        }
        let port = self.port;
        tokio::task::spawn_blocking(move || discover_system_public_ipv6_urls(port))
            .await
            .unwrap_or_default()
    }

    async fn build_status_response(&self) -> RemoteAccessStatusResponse {
        let runtime = self.remote_access.status_snapshot();
        let prefs = self.session.history_preferences().await;
        let persisted_public_url = prefs.public_url_override;
        let ipv6_info = self.discover_public_ipv6_urls().await;
        let resolved = resolve_public_entry(
            self.cli_public_url.as_deref(),
            persisted_public_url.as_deref(),
            ipv6_info.preferred_ipv6_url.as_deref(),
        );
        let invite_url = match (&runtime.passcode, &resolved.invite_base_url) {
            (Some(code), Some(base)) if runtime.enabled => Some(compose_invite_url(base, code)),
            _ => None,
        };
        RemoteAccessStatusResponse {
            enabled: runtime.enabled,
            passcode: runtime.passcode,
            active_remote_connections: u32::try_from(runtime.active_remote_connections)
                .unwrap_or(u32::MAX),
            public_url_override: resolved.public_url_override,
            public_url_source: resolved.public_url_source,
            persisted_public_url,
            preferred_ipv6_url: ipv6_info.preferred_ipv6_url,
            ipv6_urls: ipv6_info.ipv6_urls,
            invite_base_url: resolved.invite_base_url,
            invite_url,
        }
    }
}

/// Builds the sub-router exposing all Loopback-only `/api/remote-access*` endpoints.
pub(crate) fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    RemoteControlState: FromRef<S>,
{
    Router::new()
        .route("/api/remote-access", get(get_status_handler))
        .route("/api/remote-access/enable", post(enable_handler))
        .route("/api/remote-access/disable", post(disable_handler))
        .route("/api/remote-access/reset-code", post(reset_code_handler))
        .route(
            "/api/remote-access/public-url",
            post(set_public_url_handler),
        )
        .route_layer(middleware::from_fn(require_json_post_middleware))
        .route_layer(middleware::from_fn(require_loopback_middleware))
}

fn compose_invite_url(base_url: &str, passcode: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/?code={passcode}")
}

async fn get_status_handler(State(state): State<RemoteControlState>) -> Response {
    let payload = state.build_status_response().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn enable_handler(State(state): State<RemoteControlState>) -> Response {
    let _ = state.remote_access.enable();
    let payload = state.build_status_response().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn disable_handler(State(state): State<RemoteControlState>) -> Response {
    state.remote_access.disable();
    let payload = state.build_status_response().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn reset_code_handler(State(state): State<RemoteControlState>) -> Response {
    if state.remote_access.reset_code().is_none() {
        return json_with_no_store(
            StatusCode::CONFLICT,
            &serde_json::json!({ "error": "远程联机当前未开启，请先开启后再重置入桌口令" }),
        );
    }
    let payload = state.build_status_response().await;
    json_with_no_store(StatusCode::OK, &payload)
}

async fn set_public_url_handler(State(state): State<RemoteControlState>, body: Bytes) -> Response {
    let Ok(req) = serde_json::from_slice::<SetPublicUrlRequest>(&body) else {
        return json_with_no_store(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": "请求格式无效，请提供 JSON 字段 public_url" }),
        );
    };
    match state
        .session
        .set_public_url_override(req.public_url.as_deref())
        .await
    {
        Ok(_) => {
            let payload = state.build_status_response().await;
            json_with_no_store(StatusCode::OK, &payload)
        }
        Err(PublicUrlSaveError::Invalid(err)) => json_with_no_store(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": err.to_string() }),
        ),
        Err(PublicUrlSaveError::Io(err)) => json_with_no_store(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "error": format!("保存自定义公网入口失败：{err}") }),
        ),
    }
}

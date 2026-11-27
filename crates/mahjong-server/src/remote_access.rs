//! In-memory remote access gate state: high-entropy invite passcodes (`入桌口令`),
//! 256-bit session tokens, dual-layer brute-force rate limiting, and active remote WS revocation.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use axum::http::header::COOKIE;
use axum::http::{HeaderMap, HeaderValue};
use tokio::sync::watch;
use tokio::time::Instant;

/// Unambiguous 32-character Crockford-like alphabet (excludes `0`, `1`, `I`, `O`).
pub const PASSCODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Fixed character length of each generated `入桌口令` (40 bits of entropy).
pub const PASSCODE_LEN: usize = 8;

/// Byte length of each randomly generated remote session cookie token (256 bits).
pub const SESSION_TOKEN_BYTES: usize = 32;

/// Cookie name used for authenticated remote sessions.
pub const REMOTE_SESSION_COOKIE_NAME: &str = "majjj_remote_session";

/// Per-source sliding window for failed passcode attempts (5 minutes).
pub const SOURCE_FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Consecutive failures within [`SOURCE_FAILURE_WINDOW`] before locking a source bucket.
pub const SOURCE_FAILURE_THRESHOLD: usize = 5;

/// Lock duration imposed on a source bucket once [`SOURCE_FAILURE_THRESHOLD`] is reached (5 minutes).
pub const SOURCE_LOCK_DURATION: Duration = Duration::from_secs(5 * 60);

/// Global sliding window for failed passcode attempts across all sources (1 minute).
pub const GLOBAL_FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// Total global failures within [`GLOBAL_FAILURE_WINDOW`] before activating global cooldown.
pub const GLOBAL_FAILURE_THRESHOLD: usize = 20;

/// Global cooldown duration imposed once [`GLOBAL_FAILURE_THRESHOLD`] is reached (60 seconds).
pub const GLOBAL_LOCK_DURATION: Duration = Duration::from_secs(60);

/// Upper bound on tracked per-source rate-limit buckets to prevent unbounded memory growth.
const MAX_TRACKED_SOURCE_BUCKETS: usize = 2048;

/// Snapshot of runtime remote-access state exposed to host management endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAccessRuntimeStatus {
    /// Whether `远程联机` is currently enabled.
    pub enabled: bool,
    /// Active 8-character `入桌口令` (`Some` only when `enabled == true`).
    pub passcode: Option<String>,
    /// Number of currently connected remote WebSocket clients.
    pub active_remote_connections: usize,
}

/// Outcome of attempting to exchange a `入桌口令` for a remote session cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAuthOutcome {
    /// Remote access is currently disabled (`403`).
    Disabled,
    /// Passcode verified; a fresh 256-bit hex session token was issued (`200`).
    Authenticated { session_token: String },
    /// Passcode did not match (`401`).
    InvalidCode,
    /// Source bucket or global rate limiter rejected the request (`429`).
    RateLimited { retry_after_secs: u32 },
}

/// RAII guard tracking one active remote WebSocket connection.
#[derive(Debug)]
pub struct RemoteWsGuard {
    active_remote_ws: Arc<AtomicUsize>,
}

impl Drop for RemoteWsGuard {
    fn drop(&mut self) {
        self.active_remote_ws.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RateLimitKey {
    Ipv4(Ipv4Addr),
    Ipv6Prefix64([u16; 4]),
    Unknown,
}

#[derive(Debug, Default)]
struct SourceBucket {
    failures: Vec<Instant>,
    locked_until: Option<Instant>,
}

impl SourceBucket {
    fn prune(&mut self, now: Instant) {
        self.failures
            .retain(|&ts| now.saturating_duration_since(ts) < SOURCE_FAILURE_WINDOW);
        if self.locked_until.is_some_and(|until| until <= now) {
            self.locked_until = None;
        }
    }

    fn is_idle(&self) -> bool {
        self.failures.is_empty() && self.locked_until.is_none()
    }
}

#[derive(Debug, Default)]
struct RateLimiterState {
    sources: HashMap<RateLimitKey, SourceBucket>,
    global_failures: Vec<Instant>,
    global_locked_until: Option<Instant>,
}

impl RateLimiterState {
    fn active_lock_remaining(&mut self, key: RateLimitKey, now: Instant) -> Option<Duration> {
        if self.global_locked_until.is_some_and(|until| until <= now) {
            self.global_locked_until = None;
        }
        let global_rem = self
            .global_locked_until
            .and_then(|until| until.checked_duration_since(now));

        let source_rem = if let Some(bucket) = self.sources.get_mut(&key) {
            bucket.prune(now);
            let rem = bucket
                .locked_until
                .and_then(|until| until.checked_duration_since(now));
            if bucket.is_idle() {
                self.sources.remove(&key);
            }
            rem
        } else {
            None
        };

        match (source_rem, global_rem) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn record_failure(&mut self, key: RateLimitKey, now: Instant) -> Option<Duration> {
        self.prune_if_oversized(now);

        let bucket = self.sources.entry(key).or_default();
        bucket.prune(now);
        bucket.failures.push(now);
        let source_lock = if bucket.failures.len() >= SOURCE_FAILURE_THRESHOLD {
            bucket.failures.clear();
            bucket.locked_until = Some(now + SOURCE_LOCK_DURATION);
            Some(SOURCE_LOCK_DURATION)
        } else {
            None
        };

        self.global_failures
            .retain(|&ts| now.saturating_duration_since(ts) < GLOBAL_FAILURE_WINDOW);
        self.global_failures.push(now);
        let global_lock = if self.global_failures.len() >= GLOBAL_FAILURE_THRESHOLD {
            self.global_failures.clear();
            self.global_locked_until = Some(now + GLOBAL_LOCK_DURATION);
            Some(GLOBAL_LOCK_DURATION)
        } else {
            None
        };

        match (source_lock, global_lock) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn clear_source(&mut self, key: RateLimitKey) {
        self.sources.remove(&key);
    }

    fn prune_if_oversized(&mut self, now: Instant) {
        if self.sources.len() < MAX_TRACKED_SOURCE_BUCKETS {
            return;
        }
        self.sources.retain(|_, bucket| {
            bucket.prune(now);
            !bucket.is_idle()
        });
    }
}

#[derive(Debug, Default)]
struct RemoteAccessState {
    enabled: bool,
    passcode: Option<String>,
    sessions: HashSet<String>,
    rate_limiter: RateLimiterState,
}

#[derive(Debug)]
struct RemoteAccessInner {
    state: StdMutex<RemoteAccessState>,
    active_remote_ws: Arc<AtomicUsize>,
    revocation_tx: watch::Sender<u64>,
}

/// Pure in-memory coordinator for `远程联机` passcodes, session cookies, brute-force protection,
/// and instant remote WebSocket revocation.
///
/// Neither passcodes nor session tokens are ever persisted to disk.
#[derive(Debug, Clone)]
pub struct RemoteAccessManager {
    inner: Arc<RemoteAccessInner>,
}

impl Default for RemoteAccessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteAccessManager {
    /// Creates a new manager initialized in the disabled state (`远程联机` off).
    #[must_use]
    pub fn new() -> Self {
        let (revocation_tx, _) = watch::channel(0_u64);
        Self {
            inner: Arc::new(RemoteAccessInner {
                state: StdMutex::new(RemoteAccessState::default()),
                active_remote_ws: Arc::new(AtomicUsize::new(0)),
                revocation_tx,
            }),
        }
    }

    /// Returns `true` when `远程联机` is currently enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.enabled
    }

    /// Returns the active 8-character `入桌口令`, or `None` when disabled.
    #[must_use]
    pub fn passcode(&self) -> Option<String> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.enabled {
            state.passcode.clone()
        } else {
            None
        }
    }

    /// Enables `远程联机`. When transitioning from disabled to enabled, generates a fresh 8-character
    /// cryptographically random `入桌口令`. If already enabled, returns the current passcode.
    #[must_use]
    pub fn enable(&self) -> String {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.enabled {
            if let Some(existing) = &state.passcode {
                return existing.clone();
            }
        }
        let code = generate_passcode();
        state.enabled = true;
        state.passcode = Some(code.clone());
        code
    }

    /// Immediately disables `远程联机`, wipes the active passcode and all remote session tokens,
    /// and notifies all connected remote WebSocket tasks to disconnect immediately.
    pub fn disable(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.enabled = false;
        state.passcode = None;
        state.sessions.clear();
        self.inner.revocation_tx.send_modify(|epoch| {
            *epoch = epoch.wrapping_add(1);
        });
    }

    /// Rotates the active `入桌口令` while preserving all already-authenticated remote sessions
    /// and active WebSocket connections. Returns `None` if `远程联机` is currently disabled.
    #[must_use]
    pub fn reset_code(&self) -> Option<String> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.enabled {
            return None;
        }
        let prev = state.passcode.as_deref();
        let mut next = generate_passcode();
        if prev.is_some_and(|old| old == next) {
            next = generate_passcode();
        }
        state.passcode = Some(next.clone());
        Some(next)
    }

    /// Verifies a candidate `入桌口令` (trimmed, ASCII case-insensitive, constant-time compared)
    /// against the active passcode under dual-layer brute-force rate limiting.
    #[must_use]
    pub fn verify_and_issue_session(
        &self,
        raw_code: &str,
        peer: Option<SocketAddr>,
    ) -> RemoteAuthOutcome {
        let now = Instant::now();
        let key = rate_limit_key(peer);

        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !state.enabled {
            return RemoteAuthOutcome::Disabled;
        }

        if let Some(remaining) = state.rate_limiter.active_lock_remaining(key, now) {
            return RemoteAuthOutcome::RateLimited {
                retry_after_secs: ceil_seconds(remaining),
            };
        }

        let Some(expected) = state.passcode.as_deref() else {
            return RemoteAuthOutcome::Disabled;
        };

        let normalized = raw_code.trim().to_ascii_uppercase();
        if constant_time_eq(normalized.as_bytes(), expected.as_bytes()) {
            state.rate_limiter.clear_source(key);
            let session_token = generate_session_token();
            state.sessions.insert(session_token.clone());
            return RemoteAuthOutcome::Authenticated { session_token };
        }

        if let Some(lock_duration) = state.rate_limiter.record_failure(key, now) {
            RemoteAuthOutcome::RateLimited {
                retry_after_secs: ceil_seconds(lock_duration),
            }
        } else {
            RemoteAuthOutcome::InvalidCode
        }
    }

    /// Returns `true` when `远程联机` is enabled and `token` matches an active remote session.
    #[must_use]
    pub fn is_valid_session(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.enabled && state.sessions.contains(token)
    }

    /// Returns `true` when `远程联机` is enabled and the HTTP `Cookie` header(s) contain a valid
    /// `majjj_remote_session` token.
    #[must_use]
    pub fn has_valid_session_cookie(&self, headers: &HeaderMap) -> bool {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        find_valid_session_token(&state, headers).is_some()
    }

    /// Subscribes to the remote-access revocation watch channel (`disable()` increments the epoch).
    #[must_use]
    pub fn subscribe_revocation(&self) -> watch::Receiver<u64> {
        self.inner.revocation_tx.subscribe()
    }

    /// Validates the `majjj_remote_session` cookie in `headers` and atomically subscribes to
    /// revocation signals under the state lock so no `disable()` -> `enable()` window can be missed
    /// between upgrade validation and WebSocket task polling.
    #[must_use]
    pub fn validate_session_cookie_with_revocation(
        &self,
        headers: &HeaderMap,
    ) -> Option<(String, watch::Receiver<u64>)> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let token = find_valid_session_token(&state, headers)?;
        let revocation_rx = self.inner.revocation_tx.subscribe();
        Some((token, revocation_rx))
    }

    /// Tracks one active remote WebSocket connection via an RAII counter guard.
    #[must_use]
    pub fn track_remote_ws(&self) -> RemoteWsGuard {
        self.inner.active_remote_ws.fetch_add(1, Ordering::AcqRel);
        RemoteWsGuard {
            active_remote_ws: Arc::clone(&self.inner.active_remote_ws),
        }
    }

    /// Registers an active remote WebSocket connection and returns an RAII guard together with
    /// a revocation receiver that triggers immediately if `disable()` is called.
    #[must_use]
    pub fn enter_remote_ws(&self) -> (RemoteWsGuard, watch::Receiver<u64>) {
        let revocation_rx = self.subscribe_revocation();
        let guard = self.track_remote_ws();
        (guard, revocation_rx)
    }

    /// Returns the number of currently active remote WebSocket connections.
    #[must_use]
    pub fn active_remote_connections(&self) -> usize {
        self.inner.active_remote_ws.load(Ordering::Acquire)
    }

    /// Returns an atomic snapshot of runtime remote-access state for host status endpoints.
    #[must_use]
    pub fn status_snapshot(&self) -> RemoteAccessRuntimeStatus {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        RemoteAccessRuntimeStatus {
            enabled: state.enabled,
            passcode: if state.enabled {
                state.passcode.clone()
            } else {
                None
            },
            active_remote_connections: self.active_remote_connections(),
        }
    }
}

/// Formats the `Set-Cookie` header value for an issued remote session token.
///
/// # Panics
///
/// Never panics for hex-encoded tokens produced by [`RemoteAccessManager`].
#[must_use]
pub fn build_session_set_cookie_header(session_token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{REMOTE_SESSION_COOKIE_NAME}={session_token}; HttpOnly; SameSite=Lax; Path=/"
    ))
    .expect("hex remote session token produces valid Set-Cookie header")
}

fn find_valid_session_token(state: &RemoteAccessState, headers: &HeaderMap) -> Option<String> {
    if !state.enabled || state.sessions.is_empty() {
        return None;
    }
    for cookie_header in headers.get_all(COOKIE) {
        let Ok(raw) = cookie_header.to_str() else {
            continue;
        };
        for part in raw.split(';') {
            let Some((name, value)) = part.split_once('=') else {
                continue;
            };
            if name.trim() == REMOTE_SESSION_COOKIE_NAME {
                let token = value.trim();
                if !token.is_empty() && state.sessions.contains(token) {
                    return Some(token.to_owned());
                }
            }
        }
    }
    None
}

fn rate_limit_key(peer: Option<SocketAddr>) -> RateLimitKey {
    match peer.map(|addr| addr.ip().to_canonical()) {
        Some(IpAddr::V4(v4)) => RateLimitKey::Ipv4(v4),
        Some(IpAddr::V6(v6)) => {
            let segs = v6.segments();
            RateLimitKey::Ipv6Prefix64([segs[0], segs[1], segs[2], segs[3]])
        }
        None => RateLimitKey::Unknown,
    }
}

fn ceil_seconds(duration: Duration) -> u32 {
    let secs = duration.as_secs();
    let rounded = if duration.subsec_nanos() > 0 {
        secs.saturating_add(1)
    } else {
        secs
    };
    u32::try_from(rounded.max(1)).unwrap_or(u32::MAX)
}

fn generate_passcode() -> String {
    let mut bytes = [0_u8; PASSCODE_LEN];
    fill_os_random(&mut bytes);
    let mut out = String::with_capacity(PASSCODE_LEN);
    // 32 divides 256 evenly, so masking with 0x1f is completely uniform without modulo bias.
    for byte in bytes {
        out.push(char::from(PASSCODE_ALPHABET[usize::from(byte & 0x1f)]));
    }
    out
}

fn generate_session_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; SESSION_TOKEN_BYTES];
    fill_os_random(&mut bytes);
    let mut out = String::with_capacity(SESSION_TOKEN_BYTES * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn fill_os_random(buf: &mut [u8]) {
    getrandom::fill(buf).expect("OS cryptographic random source must succeed");
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut byte_diff = 0_u8;
    for i in 0..max_len {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        byte_diff |= std::hint::black_box(x ^ y);
    }
    let total_diff = (a.len() ^ b.len()) | usize::from(byte_diff);
    std::hint::black_box(total_diff) == 0
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    use axum::http::header::COOKIE;
    use axum::http::{HeaderMap, HeaderValue};

    use super::{
        build_session_set_cookie_header, constant_time_eq, RemoteAccessManager, RemoteAuthOutcome,
        GLOBAL_FAILURE_THRESHOLD, GLOBAL_LOCK_DURATION, PASSCODE_ALPHABET, PASSCODE_LEN,
        REMOTE_SESSION_COOKIE_NAME, SESSION_TOKEN_BYTES, SOURCE_FAILURE_THRESHOLD,
        SOURCE_LOCK_DURATION,
    };

    #[test]
    fn passcode_and_session_token_format_and_case_insensitive_trim_matching() {
        let mgr = RemoteAccessManager::new();
        assert!(!mgr.is_enabled());
        assert_eq!(mgr.passcode(), None);
        assert_eq!(mgr.reset_code(), None);

        let code = mgr.enable();
        assert!(mgr.is_enabled());
        assert_eq!(code.len(), PASSCODE_LEN);
        assert!(code.bytes().all(|b| PASSCODE_ALPHABET.contains(&b)));
        // Re-calling enable while already enabled keeps the active passcode.
        assert_eq!(mgr.enable(), code);

        let peer = Some(SocketAddr::new(
            "2001:db8:1111:2222::1".parse::<IpAddr>().unwrap(),
            50000,
        ));
        let lowercase_padded = format!("  {} \t\n", code.to_ascii_lowercase());
        let outcome = mgr.verify_and_issue_session(&lowercase_padded, peer);
        let RemoteAuthOutcome::Authenticated { session_token } = outcome else {
            panic!("expected Authenticated, got {outcome:?}");
        };
        assert_eq!(session_token.len(), SESSION_TOKEN_BYTES * 2);
        assert!(session_token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert!(mgr.is_valid_session(&session_token));

        let set_cookie = build_session_set_cookie_header(&session_token);
        assert_eq!(
            set_cookie.to_str().unwrap(),
            format!("{REMOTE_SESSION_COOKIE_NAME}={session_token}; HttpOnly; SameSite=Lax; Path=/")
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!(
                "other=1; {REMOTE_SESSION_COOKIE_NAME}={session_token}; theme=light"
            ))
            .unwrap(),
        );
        assert!(mgr.has_valid_session_cookie(&headers));
    }

    #[test]
    fn reset_code_preserves_existing_sessions_while_disable_wipes_everything_and_signals_ws() {
        let mgr = RemoteAccessManager::new();
        let code1 = mgr.enable();
        let peer = Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)),
            40000,
        ));
        let RemoteAuthOutcome::Authenticated { session_token } =
            mgr.verify_and_issue_session(&code1, peer)
        else {
            panic!("auth must succeed");
        };

        let (guard, rev_rx) = mgr.enter_remote_ws();
        assert_eq!(mgr.active_remote_connections(), 1);
        assert!(!rev_rx.has_changed().unwrap());

        // reset_code rotates passcode without revoking existing sessions or signaling WS disconnect.
        let code2 = mgr.reset_code().expect("reset code when enabled");
        assert_ne!(code1, code2);
        assert!(mgr.is_valid_session(&session_token));
        assert!(!rev_rx.has_changed().unwrap());
        assert_eq!(
            mgr.verify_and_issue_session(&code1, peer),
            RemoteAuthOutcome::InvalidCode
        );
        assert!(matches!(
            mgr.verify_and_issue_session(&code2, peer),
            RemoteAuthOutcome::Authenticated { .. }
        ));

        // disable wipes passcode, sessions, and triggers immediate revocation signal.
        mgr.disable();
        assert!(!mgr.is_enabled());
        assert_eq!(mgr.passcode(), None);
        assert!(!mgr.is_valid_session(&session_token));
        assert!(rev_rx.has_changed().unwrap());
        assert_eq!(
            mgr.verify_and_issue_session(&code2, peer),
            RemoteAuthOutcome::Disabled
        );

        drop(guard);
        assert_eq!(mgr.active_remote_connections(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn ipv6_slash64_prefix_bucketing_success_reset_and_global_cooldown() {
        let mgr = RemoteAccessManager::new();
        let code = mgr.enable();

        // 4 failures from 4 different suffixes in the SAME /64 do not lock yet.
        for suffix in 1..SOURCE_FAILURE_THRESHOLD {
            let peer = Some(SocketAddr::new(
                format!("2001:db8:aaaa:bbbb::{suffix}").parse().unwrap(),
                50000 + suffix as u16,
            ));
            assert_eq!(
                mgr.verify_and_issue_session("WRONGCOD", peer),
                RemoteAuthOutcome::InvalidCode
            );
        }

        // Success from the same /64 clears that source bucket.
        let good_peer = Some(SocketAddr::new(
            "2001:db8:aaaa:bbbb::99".parse().unwrap(),
            51000,
        ));
        assert!(matches!(
            mgr.verify_and_issue_session(&code, good_peer),
            RemoteAuthOutcome::Authenticated { .. }
        ));

        // Now 4 more failures still return InvalidCode; the 5th locks the /64 for 5 minutes.
        for suffix in 10..(10 + SOURCE_FAILURE_THRESHOLD - 1) {
            let peer = Some(SocketAddr::new(
                format!("2001:db8:aaaa:bbbb::{suffix:x}").parse().unwrap(),
                52000,
            ));
            assert_eq!(
                mgr.verify_and_issue_session("WRONGCOD", peer),
                RemoteAuthOutcome::InvalidCode
            );
        }
        let fifth_peer = Some(SocketAddr::new(
            "2001:db8:aaaa:bbbb::ffff".parse().unwrap(),
            52005,
        ));
        assert_eq!(
            mgr.verify_and_issue_session("WRONGCOD", fifth_peer),
            RemoteAuthOutcome::RateLimited {
                retry_after_secs: 300
            }
        );

        // Even the valid passcode from another suffix in the locked /64 is rejected until lock expires.
        assert!(matches!(
            mgr.verify_and_issue_session(&code, good_peer),
            RemoteAuthOutcome::RateLimited { .. }
        ));

        // A different /64 is unaffected.
        let other_prefix = Some(SocketAddr::new(
            "2001:db8:aaaa:cccc::1".parse().unwrap(),
            53000,
        ));
        assert!(matches!(
            mgr.verify_and_issue_session(&code, other_prefix),
            RemoteAuthOutcome::Authenticated { .. }
        ));

        // Advance past SOURCE_LOCK_DURATION -> unlocked.
        tokio::time::advance(SOURCE_LOCK_DURATION).await;
        assert!(matches!(
            mgr.verify_and_issue_session(&code, good_peer),
            RemoteAuthOutcome::Authenticated { .. }
        ));

        // Trigger global cooldown with 20 failures across 20 distinct /64 prefixes within 1 minute.
        for idx in 1..GLOBAL_FAILURE_THRESHOLD {
            let peer = Some(SocketAddr::new(
                format!("2001:db8:1000:{idx:04x}::1").parse().unwrap(),
                54000,
            ));
            assert_eq!(
                mgr.verify_and_issue_session("WRONGCOD", peer),
                RemoteAuthOutcome::InvalidCode
            );
        }
        let twentieth = Some(SocketAddr::new(
            "2001:db8:1000:ffff::1".parse().unwrap(),
            54020,
        ));
        assert_eq!(
            mgr.verify_and_issue_session("WRONGCOD", twentieth),
            RemoteAuthOutcome::RateLimited {
                retry_after_secs: 60
            }
        );

        // Brand-new innocent source is also blocked during the 60s global cooldown.
        let innocent = Some(SocketAddr::new(
            "2001:db8:9999:0001::1".parse().unwrap(),
            55000,
        ));
        assert!(matches!(
            mgr.verify_and_issue_session(&code, innocent),
            RemoteAuthOutcome::RateLimited { .. }
        ));

        tokio::time::advance(GLOBAL_LOCK_DURATION + Duration::from_millis(1)).await;
        assert!(matches!(
            mgr.verify_and_issue_session(&code, innocent),
            RemoteAuthOutcome::Authenticated { .. }
        ));
    }

    #[test]
    fn constant_time_eq_distinguishes_equal_unequal_and_different_lengths() {
        assert!(constant_time_eq(b"ABCD2345", b"ABCD2345"));
        assert!(!constant_time_eq(b"ABCD2345", b"ABCD2346"));
        assert!(!constant_time_eq(b"ABCD2345", b"ABCD234"));
        assert!(!constant_time_eq(b"", b"A"));
        assert!(constant_time_eq(b"", b""));
    }
}

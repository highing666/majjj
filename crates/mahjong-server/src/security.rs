//! Three-tier (`Loopback` / `Lan` / `Remote`) `peer + Host` trust classification and
//! Web security guards (CSRF `Content-Type: application/json`, WebSocket same-origin `Origin`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::extract::{ConnectInfo, Request};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST, ORIGIN};
use axum::http::{Extensions, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Three-tier network trust boundary derived from TCP peer address AND HTTP `Host` header.
///
/// Ordered from most trusted (`Loopback`) to least trusted (`Remote`) so combining peer and
/// `Host` classifications simply takes the stricter (`max`) tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClientTrustTier {
    /// Local host access: both canonical TCP peer and HTTP `Host` header are loopback.
    Loopback,
    /// Home LAN access: both peer and `Host` are loopback or LAN-scoped (not both loopback).
    Lan,
    /// Public/untrusted access: any global IP (including all `2000::/3` IPv6), any public-domain
    /// `Host` (even over a local `127.0.0.1` reverse proxy), or missing/malformed peer or `Host`.
    Remote,
}

impl ClientTrustTier {
    /// Returns `true` strictly when the request is `Loopback` (`peer + Host` both loopback).
    #[must_use]
    pub const fn is_loopback(self) -> bool {
        matches!(self, Self::Loopback)
    }

    /// Returns `true` when the request is `Loopback` or `Lan`.
    #[must_use]
    pub const fn is_lan_or_loopback(self) -> bool {
        matches!(self, Self::Loopback | Self::Lan)
    }

    /// Returns `true` when the request is classified as `Remote`.
    #[must_use]
    pub const fn is_remote(self) -> bool {
        matches!(self, Self::Remote)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NormalizedHost {
    Ip(IpAddr),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedAuthority {
    host: NormalizedHost,
    port: Option<u16>,
}

/// Classifies a single IP address (after `to_canonical()` normalization) into `ClientTrustTier`.
///
/// - `Loopback`: `127.0.0.0/8`, `::1`, and IPv4-mapped `::ffff:127.0.0.0/104`.
/// - `Lan`: RFC1918 private IPv4 (`10/8`, `172.16/12`, `192.168/16`), IPv4 link-local
///   (`169.254/16`), IPv6 link-local (`fe80::/10`), and IPv6 ULA (`fc00::/7`).
/// - `Remote`: everything else, explicitly including all global unicast IPv6 (`2000::/3`,
///   even within the host's own `/64` prefix) and public/CGNAT IPv4.
#[must_use]
pub fn classify_ip(ip: IpAddr) -> ClientTrustTier {
    match ip.to_canonical() {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                ClientTrustTier::Loopback
            } else if v4.is_private() || v4.is_link_local() {
                ClientTrustTier::Lan
            } else {
                ClientTrustTier::Remote
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                ClientTrustTier::Loopback
            } else if is_ipv6_link_local(v6) || is_ipv6_ula(v6) {
                ClientTrustTier::Lan
            } else {
                ClientTrustTier::Remote
            }
        }
    }
}

/// Classifies the TCP peer socket address (`None` fails closed to `Remote`).
#[must_use]
pub fn classify_peer(peer: Option<SocketAddr>) -> ClientTrustTier {
    peer.map_or(ClientTrustTier::Remote, |addr| classify_ip(addr.ip()))
}

/// Classifies the HTTP `Host` header (missing, duplicate, or malformed `Host` fails closed to `Remote`).
#[must_use]
pub fn classify_host_header(headers: &HeaderMap) -> ClientTrustTier {
    let Some(auth) = extract_single_host_authority(headers) else {
        return ClientTrustTier::Remote;
    };
    match auth.host {
        NormalizedHost::Ip(ip) => classify_ip(ip),
        NormalizedHost::Name(name) => {
            if name == "localhost" {
                ClientTrustTier::Loopback
            } else if name.ends_with(".local") {
                ClientTrustTier::Lan
            } else {
                ClientTrustTier::Remote
            }
        }
    }
}

/// Computes the effective [`ClientTrustTier`] by combining TCP peer and HTTP `Host` header
/// classifications (taking the stricter tier).
///
/// Forwarding headers (`X-Forwarded-For`, `Forwarded`, `X-Real-IP`, etc.) are intentionally
/// ignored so untrusted peers cannot spoof loopback/LAN identity.
#[must_use]
pub fn classify_request_trust_tier(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> ClientTrustTier {
    classify_peer(peer).max(classify_host_header(headers))
}

/// Returns `true` when the request carries a single `Content-Type` header whose media type is
/// `application/json` (case-insensitive; optional parameters such as `; charset=utf-8` allowed).
#[must_use]
pub fn has_json_content_type(headers: &HeaderMap) -> bool {
    let mut iter = headers.get_all(CONTENT_TYPE).iter();
    let Some(first) = iter.next() else {
        return false;
    };
    if iter.next().is_some() {
        return false;
    }
    let Ok(raw) = first.to_str() else {
        return false;
    };
    let media_type = raw.split(';').next().unwrap_or_default().trim();
    media_type.eq_ignore_ascii_case("application/json")
}

/// Validates the WebSocket upgrade `Origin` header against the HTTP `Host` header (CSWSH guard).
///
/// - Absent `Origin` header (non-browser clients / CLI / tests): allowed (`true`).
/// - Present `Origin` header: must be a single valid `http://` or `https://` origin whose
///   normalized `host[:port]` strictly matches the request's `Host` header (accounting for
///   case-insensitive hostnames and scheme default ports `80`/`443`).
#[must_use]
pub fn is_same_origin_ws_request(headers: &HeaderMap) -> bool {
    let mut origin_iter = headers.get_all(ORIGIN).iter();
    let Some(first_origin) = origin_iter.next() else {
        return true;
    };
    if origin_iter.next().is_some() {
        return false;
    }
    let Ok(raw_origin) = first_origin.to_str() else {
        return false;
    };
    let trimmed_origin = raw_origin.trim();
    let (default_port, authority_part) =
        if let Some(rest) = strip_prefix_ignore_ascii_case(trimmed_origin, "http://") {
            (80_u16, rest)
        } else if let Some(rest) = strip_prefix_ignore_ascii_case(trimmed_origin, "https://") {
            (443_u16, rest)
        } else {
            return false;
        };

    let Some(origin_auth) = parse_authority(authority_part, false) else {
        return false;
    };
    let Some(host_auth) = extract_single_host_authority(headers) else {
        return false;
    };

    origin_auth.host == host_auth.host
        && origin_auth.port.unwrap_or(default_port) == host_auth.port.unwrap_or(default_port)
}

/// Returns a minimal, zero-fingerprint `403 Forbidden` response with `Cache-Control: no-store`.
#[must_use]
pub fn minimal_forbidden_response() -> Response {
    let mut response = StatusCode::FORBIDDEN.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Serializes `payload` as JSON with `status` and `Cache-Control: no-store`.
#[must_use]
pub(crate) fn json_with_no_store<T: Serialize>(status: StatusCode, payload: &T) -> Response {
    let mut response = (status, Json(payload)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Extracts the cached [`ClientTrustTier`] inserted by the top-level guard middleware, falling
/// back to classifying from `ConnectInfo<SocketAddr>` + `Host` header when invoked directly.
#[must_use]
pub(crate) fn extract_request_trust_tier(
    extensions: &Extensions,
    headers: &HeaderMap,
) -> ClientTrustTier {
    extensions
        .get::<ClientTrustTier>()
        .copied()
        .unwrap_or_else(|| {
            let peer = extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(addr)| *addr);
            classify_request_trust_tier(peer, headers)
        })
}

/// Middleware restricting a route tree strictly to `Loopback` (`peer + Host` both loopback).
pub(crate) async fn require_loopback_middleware(req: Request, next: Next) -> Response {
    let tier = extract_request_trust_tier(req.extensions(), req.headers());
    if !tier.is_loopback() {
        return minimal_forbidden_response();
    }
    next.run(req).await
}

/// Middleware enforcing `Content-Type: application/json` on `POST` requests (CSRF guard).
pub(crate) async fn require_json_post_middleware(req: Request, next: Next) -> Response {
    if *req.method() == Method::POST && !has_json_content_type(req.headers()) {
        return unsupported_media_type_json_response();
    }
    next.run(req).await
}

/// Returns a `415 Unsupported Media Type` JSON response with `Cache-Control: no-store` for
/// mutating `POST /api/*` requests missing `Content-Type: application/json`.
#[must_use]
pub fn unsupported_media_type_json_response() -> Response {
    json_with_no_store(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        &serde_json::json!({ "error": "Content-Type must be application/json" }),
    )
}

pub(crate) const fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

pub(crate) const fn is_ipv6_ula(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn strip_prefix_ignore_ascii_case<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input.len() < prefix.len() {
        return None;
    }
    let (head, tail) = input.split_at(prefix.len());
    head.eq_ignore_ascii_case(prefix).then_some(tail)
}

fn extract_single_host_authority(headers: &HeaderMap) -> Option<ParsedAuthority> {
    let mut iter = headers.get_all(HOST).iter();
    let first = iter.next()?;
    if iter.next().is_some() {
        return None;
    }
    let raw = first.to_str().ok()?;
    parse_authority(raw, true)
}

fn parse_authority(raw: &str, allow_bare_ipv6: bool) -> Option<ParsedAuthority> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.bytes().any(|b| {
            b.is_ascii_control()
                || b.is_ascii_whitespace()
                || matches!(b, b'/' | b'?' | b'#' | b'@' | b'\\')
        })
    {
        return None;
    }

    if let Some(after_open) = trimmed.strip_prefix('[') {
        let (inside, rest) = after_open.split_once(']')?;
        let port = if rest.is_empty() {
            None
        } else {
            let port_str = rest.strip_prefix(':')?;
            Some(parse_port(port_str)?)
        };
        let v6: Ipv6Addr = inside.parse().ok()?;
        return Some(ParsedAuthority {
            host: NormalizedHost::Ip(IpAddr::V6(v6)),
            port,
        });
    }

    let colon_count = trimmed.bytes().filter(|&b| b == b':').count();
    if colon_count > 1 {
        if !allow_bare_ipv6 {
            return None;
        }
        let v6: Ipv6Addr = trimmed.parse().ok()?;
        return Some(ParsedAuthority {
            host: NormalizedHost::Ip(IpAddr::V6(v6)),
            port: None,
        });
    }

    let (host_part, port) = if let Some((h, p)) = trimmed.split_once(':') {
        (h, Some(parse_port(p)?))
    } else {
        (trimmed, None)
    };

    if host_part.is_empty() {
        return None;
    }

    if let Ok(v4) = host_part.parse::<Ipv4Addr>() {
        return Some(ParsedAuthority {
            host: NormalizedHost::Ip(IpAddr::V4(v4)),
            port,
        });
    }

    if !is_valid_hostname(host_part) {
        return None;
    }

    Some(ParsedAuthority {
        host: NormalizedHost::Name(host_part.to_ascii_lowercase()),
        port,
    })
}

fn parse_port(raw: &str) -> Option<u16> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u16>().ok().filter(|&port| port != 0)
}

fn is_valid_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use axum::http::header::{CONTENT_TYPE, HOST, ORIGIN};
    use axum::http::{HeaderMap, HeaderValue};

    use super::{
        classify_host_header, classify_peer, classify_request_trust_tier, has_json_content_type,
        is_same_origin_ws_request, ClientTrustTier,
    };

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for &(k, v) in pairs {
            map.append(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn peer_classification_handles_canonical_loopback_lan_and_global_ipv6() {
        // Loopback
        assert_eq!(
            classify_peer(Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))),
            ClientTrustTier::Loopback
        );
        assert_eq!(
            classify_peer(Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 1234))),
            ClientTrustTier::Loopback
        );
        assert_eq!(
            classify_peer(Some(SocketAddr::new(
                IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()),
                1234
            ))),
            ClientTrustTier::Loopback
        );

        // LAN
        for lan_ip in [
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 20)),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8)),
            IpAddr::V4(Ipv4Addr::new(172, 16, 1, 2)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 10, 20)),
            IpAddr::V6(Ipv4Addr::new(192, 168, 5, 20).to_ipv6_mapped()),
            "fe80::1".parse::<IpAddr>().unwrap(),
            "fc00::1".parse::<IpAddr>().unwrap(),
            "fd12:3456:789a::1".parse::<IpAddr>().unwrap(),
        ] {
            assert_eq!(
                classify_peer(Some(SocketAddr::new(lan_ip, 5000))),
                ClientTrustTier::Lan,
                "expected Lan for {lan_ip}"
            );
        }

        // Remote (including all global unicast 2000::/3, public IPv4, CGNAT, None)
        assert_eq!(classify_peer(None), ClientTrustTier::Remote);
        for remote_ip in [
            "2001:db8::1".parse::<IpAddr>().unwrap(),
            "240e:1234:5678:9abc::1".parse::<IpAddr>().unwrap(),
            "203.0.113.5".parse::<IpAddr>().unwrap(),
            "100.64.0.1".parse::<IpAddr>().unwrap(),
            IpAddr::V6(Ipv4Addr::new(203, 0, 113, 5).to_ipv6_mapped()),
        ] {
            assert_eq!(
                classify_peer(Some(SocketAddr::new(remote_ip, 5000))),
                ClientTrustTier::Remote,
                "expected Remote for {remote_ip}"
            );
        }
    }

    #[test]
    fn host_header_classification_handles_loopback_lan_remote_and_malformed() {
        for host in [
            "localhost",
            "LOCALHOST:8789",
            "127.0.0.1",
            "127.0.0.1:8789",
            "[::1]",
            "[::1]:8789",
            "::1",
            "[::ffff:127.0.0.1]:8789",
        ] {
            assert_eq!(
                classify_host_header(&headers_with(&[("host", host)])),
                ClientTrustTier::Loopback,
                "expected Loopback for Host={host}"
            );
        }

        for host in [
            "192.168.5.10:8789",
            "10.0.0.2",
            "172.16.0.1:8789",
            "169.254.1.2:8789",
            "[fe80::1]:8789",
            "[fc00::1]:8789",
            "my-macbook.local:8789",
            "HOME-SERVER.LOCAL",
        ] {
            assert_eq!(
                classify_host_header(&headers_with(&[("host", host)])),
                ClientTrustTier::Lan,
                "expected Lan for Host={host}"
            );
        }

        // Missing Host, duplicate Host, public domains, public IPs, malformed hosts -> Remote
        assert_eq!(
            classify_host_header(&HeaderMap::new()),
            ClientTrustTier::Remote
        );
        assert_eq!(
            classify_host_header(&headers_with(&[
                ("host", "localhost:8789"),
                ("host", "localhost:8789"),
            ])),
            ClientTrustTier::Remote
        );
        for host in [
            "",
            ".local",
            "evil.local.example.com",
            "majjj.example.com:8789",
            "[240e:1234::1]:8789",
            "203.0.113.5:8789",
            "localhost:0",
            "127.0.0.1:0",
            "[::1]:0",
            "192.168.5.10:0",
            "localhost:notaport",
            "localhost:99999",
            "user@localhost:8789",
            "localhost/path",
        ] {
            assert_eq!(
                classify_host_header(&headers_with(&[("host", host)])),
                ClientTrustTier::Remote,
                "expected Remote for Host={host}"
            );
        }
    }

    #[test]
    fn combined_trust_tier_takes_stricter_and_ignores_proxy_headers() {
        let loopback_peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000));
        let lan_peer = Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 5, 22)),
            50000,
        ));
        let remote_peer = Some(SocketAddr::new(
            "240e:1234::5678".parse::<IpAddr>().unwrap(),
            50000,
        ));

        // Local reverse proxy (127.0.0.1 peer + public domain Host) -> Remote!
        assert_eq!(
            classify_request_trust_tier(
                loopback_peer,
                &headers_with(&[("host", "majjj.example.com:8789")]),
            ),
            ClientTrustTier::Remote
        );

        // Remote peer spoofing X-Forwarded-For / Forwarded / X-Real-IP + localhost Host -> Remote!
        assert_eq!(
            classify_request_trust_tier(
                remote_peer,
                &headers_with(&[
                    ("host", "localhost:8789"),
                    ("x-forwarded-for", "127.0.0.1"),
                    ("x-real-ip", "127.0.0.1"),
                    ("forwarded", "for=127.0.0.1"),
                ]),
            ),
            ClientTrustTier::Remote
        );

        // LAN peer spoofing localhost Host + X-Forwarded-For -> Lan (never Loopback)
        assert_eq!(
            classify_request_trust_tier(
                lan_peer,
                &headers_with(&[("host", "localhost:8789"), ("x-forwarded-for", "127.0.0.1"),]),
            ),
            ClientTrustTier::Lan
        );
    }

    #[test]
    fn json_content_type_validator_accepts_json_with_charset_and_rejects_simple_csrf_types() {
        for valid in [
            "application/json",
            "Application/JSON",
            "application/json; charset=utf-8",
            "application/json;charset=UTF-8",
        ] {
            let mut h = HeaderMap::new();
            h.insert(CONTENT_TYPE, HeaderValue::from_str(valid).unwrap());
            assert!(has_json_content_type(&h), "should accept {valid}");
        }

        assert!(!has_json_content_type(&HeaderMap::new()));
        for invalid in [
            "",
            "text/plain",
            "text/plain; charset=utf-8",
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=foo",
            "application/json+patch",
        ] {
            let mut h = HeaderMap::new();
            h.insert(CONTENT_TYPE, HeaderValue::from_str(invalid).unwrap());
            assert!(!has_json_content_type(&h), "should reject {invalid}");
        }
    }

    #[test]
    fn ws_origin_validator_enforces_same_origin_when_present() {
        // Missing Origin is allowed
        assert!(is_same_origin_ws_request(&headers_with(&[(
            "host",
            "127.0.0.1:8789"
        )])));

        // Matching origins
        for (origin, host) in [
            ("http://127.0.0.1:8789", "127.0.0.1:8789"),
            ("http://LOCALHOST:8789", "localhost:8789"),
            ("http://[::1]:8789", "[::1]:8789"),
            ("http://localhost", "localhost:80"),
            ("http://localhost:80", "localhost"),
            ("https://majjj.example.com", "majjj.example.com:443"),
            ("http://my-mac.local:8789", "MY-MAC.LOCAL:8789"),
        ] {
            let mut h = HeaderMap::new();
            h.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
            h.insert(HOST, HeaderValue::from_str(host).unwrap());
            assert!(
                is_same_origin_ws_request(&h),
                "expected match for Origin={origin} Host={host}"
            );
        }

        // Mismatched / malformed origins
        for (origin, host) in [
            ("http://evil.example:8789", "127.0.0.1:8789"),
            ("http://127.0.0.1:8789", "localhost:8789"),
            ("http://localhost:9999", "localhost:8789"),
            ("https://localhost", "localhost:80"),
            ("null", "localhost:8789"),
            ("file://localhost", "localhost:8789"),
            ("http://localhost:0", "localhost:0"),
            ("http://localhost:8789/extra", "localhost:8789"),
            ("http://[::1]:8789", "127.0.0.1:8789"),
        ] {
            let mut h = HeaderMap::new();
            h.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
            h.insert(HOST, HeaderValue::from_str(host).unwrap());
            assert!(
                !is_same_origin_ws_request(&h),
                "expected reject for Origin={origin} Host={host}"
            );
        }
    }
}

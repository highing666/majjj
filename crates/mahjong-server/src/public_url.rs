//! Strict normalization, validation, and precedence resolution for optional custom public origins.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Indicates where the currently effective custom public entry URL came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PublicUrlSource {
    /// Provided via `--public-url` CLI flag or `MAJJJ_PUBLIC_URL` environment variable.
    Cli,
    /// Loaded from persisted `preferences.json`.
    Preferences,
    /// No custom public origin override is configured.
    None,
}

/// Effective public entry resolution result (`CLI/ENV > preferences.json > preferred IPv6`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPublicEntry {
    /// Active normalized custom public origin override, if configured by CLI/ENV or preferences.
    pub public_url_override: Option<String>,
    /// Origin source of `public_url_override`.
    pub public_url_source: PublicUrlSource,
    /// Preferred base URL used to build remote invitation links (`public_url_override` > `preferred_ipv6_url`).
    pub invite_base_url: Option<String>,
}

/// Validation failure returned when a custom public origin is malformed or disallowed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{reason}")]
pub struct PublicUrlError {
    /// Clear Chinese validation reason suitable for CLI and HTTP error responses.
    pub reason: String,
}

impl PublicUrlError {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// Strictly validates and normalizes a custom public origin URL.
///
/// Only valid `http://` or `https://` origins (scheme + host + optional port) are accepted.
/// Credentials (`@`), paths other than an optional single trailing `/`, query strings (`?`),
/// fragments (`#`), backslashes, and control/whitespace characters are rejected with clear Chinese errors.
///
/// # Errors
///
/// Returns [`PublicUrlError`] when `raw` is empty or violates origin syntax rules.
pub fn normalize_public_url(raw: &str) -> Result<String, PublicUrlError> {
    if raw.chars().any(char::is_control) {
        return Err(PublicUrlError::new("自定义公网入口不能包含换行或控制字符"));
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PublicUrlError::new("自定义公网入口不能为空"));
    }

    if trimmed.chars().any(char::is_whitespace) {
        return Err(PublicUrlError::new("自定义公网入口不能包含空白字符"));
    }

    if trimmed.contains('\\') {
        return Err(PublicUrlError::new("自定义公网入口不能包含反斜杠"));
    }

    let (scheme, default_port, rest) = parse_scheme(trimmed)?;
    if rest.is_empty() {
        return Err(PublicUrlError::new("自定义公网入口缺少主机地址"));
    }

    if rest.contains('?') || rest.contains('#') {
        return Err(PublicUrlError::new(
            "自定义公网入口不能包含查询参数或片段（? 或 #）",
        ));
    }

    let (authority, path_part) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };

    if !path_part.is_empty() && path_part != "/" {
        return Err(PublicUrlError::new(
            "自定义公网入口不能包含路径，仅允许填写协议、主机与可选端口",
        ));
    }

    if authority.is_empty() {
        return Err(PublicUrlError::new("自定义公网入口缺少主机地址"));
    }

    if authority.contains('@') {
        return Err(PublicUrlError::new(
            "自定义公网入口不能包含用户名或密码（@）",
        ));
    }

    let (normalized_host, port) = parse_authority(authority)?;
    let port_suffix = match port {
        Some(p) if p != default_port => format!(":{p}"),
        _ => String::new(),
    };

    Ok(format!("{scheme}://{normalized_host}{port_suffix}"))
}

/// Normalizes an optional public origin where `None` or blank whitespace clears the override (`Ok(None)`).
///
/// # Errors
///
/// Returns [`PublicUrlError`] when a non-blank value is not a valid `http://` / `https://` origin.
pub fn normalize_optional_public_url(raw: Option<&str>) -> Result<Option<String>, PublicUrlError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    if value.chars().any(char::is_control) {
        return Err(PublicUrlError::new("自定义公网入口不能包含换行或控制字符"));
    }
    if value.trim().is_empty() {
        return Ok(None);
    }
    normalize_public_url(value).map(Some)
}

/// Resolves the effective public entry with strict precedence:
/// `CLI/ENV > preferences.json > preferred stable public IPv6`.
#[must_use]
pub fn resolve_public_entry(
    cli_or_env: Option<&str>,
    persisted_pref: Option<&str>,
    preferred_ipv6_url: Option<&str>,
) -> ResolvedPublicEntry {
    if let Ok(Some(cli_url)) = normalize_optional_public_url(cli_or_env) {
        return ResolvedPublicEntry {
            public_url_override: Some(cli_url.clone()),
            public_url_source: PublicUrlSource::Cli,
            invite_base_url: Some(cli_url),
        };
    }

    if let Ok(Some(pref_url)) = normalize_optional_public_url(persisted_pref) {
        return ResolvedPublicEntry {
            public_url_override: Some(pref_url.clone()),
            public_url_source: PublicUrlSource::Preferences,
            invite_base_url: Some(pref_url),
        };
    }

    let fallback = preferred_ipv6_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    ResolvedPublicEntry {
        public_url_override: None,
        public_url_source: PublicUrlSource::None,
        invite_base_url: fallback,
    }
}

fn parse_scheme(input: &str) -> Result<(&'static str, u16, &str), PublicUrlError> {
    let Some((scheme_raw, rest)) = input.split_once("://") else {
        return Err(PublicUrlError::new(
            "自定义公网入口仅支持 http:// 或 https:// 协议",
        ));
    };
    if scheme_raw.eq_ignore_ascii_case("http") {
        Ok(("http", 80, rest))
    } else if scheme_raw.eq_ignore_ascii_case("https") {
        Ok(("https", 443, rest))
    } else {
        Err(PublicUrlError::new(
            "自定义公网入口仅支持 http:// 或 https:// 协议",
        ))
    }
}

fn parse_authority(authority: &str) -> Result<(String, Option<u16>), PublicUrlError> {
    if let Some(bracket_rest) = authority.strip_prefix('[') {
        let Some(end_idx) = bracket_rest.find(']') else {
            return Err(PublicUrlError::new("IPv6 地址缺少右方括号"));
        };
        let ipv6_raw = &bracket_rest[..end_idx];
        if ipv6_raw.is_empty() {
            return Err(PublicUrlError::new("IPv6 地址不能为空"));
        }
        let ipv6: Ipv6Addr = ipv6_raw.parse().map_err(|_| {
            PublicUrlError::new("IPv6 地址格式无效，请检查方括号内的地址（不支持网卡后缀）")
        })?;
        let after = &bracket_rest[end_idx + 1..];
        let port = if after.is_empty() {
            None
        } else if let Some(port_raw) = after.strip_prefix(':') {
            Some(parse_port(port_raw)?)
        } else {
            return Err(PublicUrlError::new("IPv6 地址后的端口格式不合法"));
        };
        return Ok((format!("[{ipv6}]"), port));
    }

    if authority.contains('[') || authority.contains(']') {
        return Err(PublicUrlError::new(
            "IPv6 地址必须使用方括号包裹，例如 http://[2001:db9::1]:8789",
        ));
    }

    let colon_count = authority.bytes().filter(|&b| b == b':').count();
    if colon_count > 1 {
        return Err(PublicUrlError::new(
            "IPv6 地址必须使用方括号包裹，例如 http://[2001:db9::1]:8789",
        ));
    }

    let (host_raw, port) = match authority.split_once(':') {
        Some((h, port_raw)) => (h, Some(parse_port(port_raw)?)),
        None => (authority, None),
    };

    if host_raw.is_empty() {
        return Err(PublicUrlError::new("自定义公网入口缺少主机地址"));
    }

    if let Ok(ipv4) = host_raw.parse::<Ipv4Addr>() {
        return Ok((ipv4.to_string(), port));
    }

    let hostname = validate_and_normalize_hostname(host_raw)?;
    Ok((hostname, port))
}

fn parse_port(port_raw: &str) -> Result<u16, PublicUrlError> {
    if port_raw.is_empty() || !port_raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PublicUrlError::new("端口号无效，必须为 1 到 65535 的数字"));
    }
    let port: u16 = port_raw
        .parse()
        .map_err(|_| PublicUrlError::new("端口号超出合法范围（1 到 65535）"))?;
    if port == 0 {
        return Err(PublicUrlError::new("端口号不能为 0，必须为 1 到 65535"));
    }
    Ok(port)
}

fn validate_and_normalize_hostname(host_raw: &str) -> Result<String, PublicUrlError> {
    if host_raw.len() > 253 || host_raw.starts_with('.') || host_raw.ends_with('.') {
        return Err(PublicUrlError::new("主机名格式不合法"));
    }

    let lower = host_raw.to_ascii_lowercase();
    let mut last_label = "";
    for label in lower.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(PublicUrlError::new("主机名格式不合法"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(PublicUrlError::new("主机名标签不能以连字符开头或结尾"));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(PublicUrlError::new("主机名包含非法字符"));
        }
        last_label = label;
    }

    // Reject all-numeric final labels (e.g. malformed IPv4 like `1.2.3.999` or bare numbers).
    if !last_label.bytes().any(|b| b.is_ascii_alphabetic()) {
        return Err(PublicUrlError::new("主机名或 IPv4 地址格式不合法"));
    }

    Ok(lower)
}

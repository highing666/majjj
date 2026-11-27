mod common;

use std::fs;
use std::net::IpAddr;

use common::TempDirGuard;
use mahjong_server::history_dto::HISTORY_SCHEMA_VERSION;
use mahjong_server::lan::{discover_public_ipv6_urls, NetworkInterface};
use mahjong_server::public_url::{
    normalize_optional_public_url, normalize_public_url, resolve_public_entry, PublicUrlSource,
    ResolvedPublicEntry,
};
use mahjong_server::storage::HistoryStore;
use mahjong_server::{ServerConfig, ServerError};

#[test]
fn normalizes_valid_http_and_https_origins() {
    assert_eq!(
        normalize_public_url("http://majjj.example.com:8789").expect("valid http origin"),
        "http://majjj.example.com:8789"
    );
    assert_eq!(
        normalize_public_url("  HTTPS://Majjj.Example.COM:8443/  ")
            .expect("normalizes scheme, host case, and trailing slash"),
        "https://majjj.example.com:8443"
    );
    assert_eq!(
        normalize_public_url("https://majjj.example.com/").expect("strips trailing slash"),
        "https://majjj.example.com"
    );
    assert_eq!(
        normalize_public_url("http://majjj.example.com:80/").expect("strips default http port"),
        "http://majjj.example.com"
    );
    assert_eq!(
        normalize_public_url("https://majjj.example.com:443").expect("strips default https port"),
        "https://majjj.example.com"
    );
    assert_eq!(
        normalize_public_url("http://[2001:DB9::0001]:8789/")
            .expect("canonicalizes bracketed ipv6"),
        "http://[2001:db9::1]:8789"
    );
    assert_eq!(
        normalize_public_url("https://[2001:db9::1]").expect("https bracketed ipv6 without port"),
        "https://[2001:db9::1]"
    );
    assert_eq!(
        normalize_public_url("http://203.0.113.10:8789/").expect("valid ipv4 origin"),
        "http://203.0.113.10:8789"
    );
}

#[test]
fn rejects_invalid_public_urls_with_clear_chinese_errors() {
    let invalid_cases = [
        "",
        "   ",
        "majjj.example.com:8789",
        "ws://majjj.example.com:8789",
        "wss://majjj.example.com:8443",
        "ftp://majjj.example.com",
        "javascript:alert(1)",
        "http://user:pass@majjj.example.com:8789",
        "http://user@majjj.example.com:8789",
        "http://majjj.example.com:8789/table",
        "http://majjj.example.com:8789//",
        "https://majjj.example.com/a/b",
        "http://majjj.example.com:8789?code=ABCD1234",
        "http://majjj.example.com:8789/?code=ABCD1234",
        "http://majjj.example.com:8789#section",
        "http://majjj.example.com:8789/#section",
        "http://majjj example.com:8789",
        "http://majjj.example.com:8789\n",
        "http://majjj.example.com\\@evil.example",
        "http://2001:db9::1:8789",
        "http://2001:db9::1",
        "http://[2001:db9::1:8789",
        "http://[not-ipv6]:8789",
        "http://[]:8789",
        "http://[2001:db9::1%en0]:8789",
        "http://majjj.example.com:0",
        "http://majjj.example.com:70000",
        "http://majjj.example.com:",
        "http://majjj.example.com:+8789",
        "http://majjj.example.com:abc",
        "http://.example.com",
        "http://example.com.",
        "http://bad..example.com",
        "http://-bad.example.com",
        "http://bad-.example.com",
        "http://1.2.3.999:8789",
        "http://bad_host.example.com",
    ];

    for raw in invalid_cases {
        let err = normalize_public_url(raw).expect_err(&format!(
            "expected invalid public URL to be rejected: {raw:?}"
        ));
        let msg = err.to_string();
        assert!(
            msg.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "error message for {raw:?} must be clear Chinese, got: {msg}"
        );
    }
}

#[test]
fn normalize_optional_treats_blank_as_none_and_rejects_invalid() {
    assert_eq!(normalize_optional_public_url(None).expect("none"), None);
    assert_eq!(
        normalize_optional_public_url(Some("")).expect("empty"),
        None
    );
    assert_eq!(
        normalize_optional_public_url(Some("   ")).expect("whitespace"),
        None
    );
    assert_eq!(
        normalize_optional_public_url(Some("https://majjj.example.com:8443/"))
            .expect("valid optional"),
        Some("https://majjj.example.com:8443".to_string())
    );
    assert!(normalize_optional_public_url(Some("http://majjj.example.com/path")).is_err());
}

#[test]
fn resolves_public_entry_with_strict_cli_over_preferences_over_ipv6_precedence() {
    let ipv6 = "http://[2001:db9:1::1]:8789";

    // 1. CLI/ENV wins over preferences.json and preferred IPv6.
    assert_eq!(
        resolve_public_entry(
            Some("https://cli.example.com:8443/"),
            Some("http://pref.example.com:8789"),
            Some(ipv6),
        ),
        ResolvedPublicEntry {
            public_url_override: Some("https://cli.example.com:8443".to_string()),
            public_url_source: PublicUrlSource::Cli,
            invite_base_url: Some("https://cli.example.com:8443".to_string()),
        }
    );

    // 2. preferences.json wins over preferred IPv6 when CLI/ENV is absent.
    assert_eq!(
        resolve_public_entry(None, Some("http://pref.example.com:8789/"), Some(ipv6)),
        ResolvedPublicEntry {
            public_url_override: Some("http://pref.example.com:8789".to_string()),
            public_url_source: PublicUrlSource::Preferences,
            invite_base_url: Some("http://pref.example.com:8789".to_string()),
        }
    );

    // 3. Preferred stable public IPv6 is used when neither CLI nor preferences specifies an override.
    assert_eq!(
        resolve_public_entry(None, None, Some(ipv6)),
        ResolvedPublicEntry {
            public_url_override: None,
            public_url_source: PublicUrlSource::None,
            invite_base_url: Some(ipv6.to_string()),
        }
    );

    // 4. All absent yields None.
    assert_eq!(
        resolve_public_entry(None, None, None),
        ResolvedPublicEntry {
            public_url_override: None,
            public_url_source: PublicUrlSource::None,
            invite_base_url: None,
        }
    );

    // 5. Invalid persisted preference is safely ignored and falls back to preferred IPv6.
    assert_eq!(
        resolve_public_entry(None, Some("ftp://bad.example.com"), Some(ipv6)),
        ResolvedPublicEntry {
            public_url_override: None,
            public_url_source: PublicUrlSource::None,
            invite_base_url: Some(ipv6.to_string()),
        }
    );
}

#[test]
fn preferences_public_url_override_is_backward_compatible_validated_and_persisted() {
    let dir = TempDirGuard::new("majjj_public_url_prefs");

    // 1. Legacy preferences.json lacking `public_url_override` loads cleanly with schema_version = 1.
    fs::write(
        dir.path().join("preferences.json"),
        r#"{"schema_version":1,"recent_nicknames":["Alice"],"default_mode":"EastOnly","default_seating":"KeepLobbySeats","default_ai_difficulty":"Strong","default_pace":"Standard"}"#,
    )
    .expect("write legacy preferences");

    let mut store = HistoryStore::open(dir.path()).expect("open legacy preferences");
    assert_eq!(store.preferences().schema_version, HISTORY_SCHEMA_VERSION);
    assert_eq!(store.preferences().public_url_override, None);
    assert_eq!(store.preferences().recent_nicknames, vec!["Alice"]);

    // 2. Setting a valid origin normalizes and persists atomically.
    let saved = store
        .set_public_url_override(Some("HTTPS://Majjj.Example.COM:8443/"))
        .expect("persist valid public_url_override");
    assert_eq!(saved.as_deref(), Some("https://majjj.example.com:8443"));
    assert_eq!(
        store.preferences().public_url_override.as_deref(),
        Some("https://majjj.example.com:8443")
    );

    let reopened = HistoryStore::open(dir.path()).expect("reopen preferences");
    assert_eq!(
        reopened.preferences().schema_version,
        HISTORY_SCHEMA_VERSION
    );
    assert_eq!(
        reopened.preferences().public_url_override.as_deref(),
        Some("https://majjj.example.com:8443")
    );

    // 3. Attempting to save an invalid value returns Chinese error and preserves previous value on disk & in memory.
    let err = store
        .set_public_url_override(Some("http://majjj.example.com:8789/bad-path"))
        .expect_err("reject invalid public_url_override");
    let err_msg = err.to_string();
    assert!(
        err_msg
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
        "save error must be clear Chinese: {err_msg}"
    );
    assert_eq!(
        store.preferences().public_url_override.as_deref(),
        Some("https://majjj.example.com:8443")
    );

    // 4. Resolution combines persisted preference with discovered IPv6 and CLI override.
    let interfaces = vec![NetworkInterface {
        name: "en0".to_string(),
        addr: "2001:db9:1::10".parse::<IpAddr>().expect("valid ipv6"),
        is_temporary: false,
        is_deprecated: false,
    }];
    let ipv6_info = discover_public_ipv6_urls(&interfaces, 8789);
    let from_pref = resolve_public_entry(
        None,
        reopened.preferences().public_url_override.as_deref(),
        ipv6_info.preferred_ipv6_url.as_deref(),
    );
    assert_eq!(from_pref.public_url_source, PublicUrlSource::Preferences);
    assert_eq!(
        from_pref.invite_base_url.as_deref(),
        Some("https://majjj.example.com:8443")
    );

    let from_cli = resolve_public_entry(
        Some("http://cli.example.com:8789/"),
        reopened.preferences().public_url_override.as_deref(),
        ipv6_info.preferred_ipv6_url.as_deref(),
    );
    assert_eq!(from_cli.public_url_source, PublicUrlSource::Cli);
    assert_eq!(
        from_cli.invite_base_url.as_deref(),
        Some("http://cli.example.com:8789")
    );

    // 5. Clearing via whitespace or None persists None and falls back to preferred IPv6.
    assert_eq!(
        store
            .set_public_url_override(Some("   "))
            .expect("blank clears override"),
        None
    );
    let cleared = HistoryStore::open(dir.path()).expect("reopen cleared preferences");
    assert_eq!(cleared.preferences().public_url_override, None);
    let fallback = resolve_public_entry(
        None,
        cleared.preferences().public_url_override.as_deref(),
        ipv6_info.preferred_ipv6_url.as_deref(),
    );
    assert_eq!(fallback.public_url_source, PublicUrlSource::None);
    assert_eq!(
        fallback.invite_base_url.as_deref(),
        Some("http://[2001:db9:1::10]:8789")
    );

    // 6. Hand-edited invalid public_url_override on disk is safely ignored on load while preserving other fields.
    fs::write(
        dir.path().join("preferences.json"),
        r#"{"schema_version":1,"recent_nicknames":["Bob"],"default_mode":"FreeTable","default_seating":"KeepLobbySeats","default_ai_difficulty":"Strong","default_pace":"Standard","public_url_override":"http://majjj.example.com/not-origin"}"#,
    )
    .expect("write preferences with invalid public_url_override");
    let sanitized = HistoryStore::open(dir.path()).expect("reopen sanitized preferences");
    assert_eq!(sanitized.preferences().public_url_override, None);
    assert_eq!(sanitized.preferences().recent_nicknames, vec!["Bob"]);
}

#[tokio::test]
async fn server_config_rejects_invalid_public_url_before_binding() {
    let err = ServerConfig::default()
        .with_public_url(Some(
            "http://majjj.example.com:8789/not-allowed".to_string(),
        ))
        .spawn()
        .await
        .expect_err("invalid public_url must fail fast before binding");
    assert!(
        matches!(err, ServerError::InvalidPublicUrl(_)),
        "expected ServerError::InvalidPublicUrl, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
        "ServerError::InvalidPublicUrl must be clear Chinese: {msg}"
    );
}

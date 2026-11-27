use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use mahjong_server::lan::{
    discover_lan_urls, discover_public_ipv6_urls, discover_system_public_ipv6_urls,
    NetworkInterface,
};

fn iface(name: &str, ip: &str) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        addr: ip.parse::<IpAddr>().expect("valid test ip"),
        is_temporary: false,
        is_deprecated: false,
    }
}

fn ipv6_iface(name: &str, ip: &str, is_temporary: bool, is_deprecated: bool) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        addr: ip.parse::<IpAddr>().expect("valid test ipv6"),
        is_temporary,
        is_deprecated,
    }
}

#[test]
fn filters_virtual_interfaces_and_reserved_ranges_and_prioritizes_rfc1918() {
    let interfaces = vec![
        iface("lo0", "127.0.0.1"),
        iface("LO1", "10.99.0.1"),
        iface("utun4", "10.8.0.2"),
        iface("UTUN9", "192.168.64.2"),
        iface("bridge100", "192.168.2.1"),
        iface("awdl0", "192.168.3.1"),
        iface("llw0", "172.16.1.1"),
        iface("en0", "169.254.10.20"),
        iface("en0", "100.64.0.1"),
        iface("en0", "100.127.255.254"),
        iface("en0", "198.18.0.5"),
        iface("en0", "198.19.12.34"),
        iface("en0", "172.15.255.1"),
        iface("en0", "172.32.0.1"),
        iface("en0", "8.8.8.8"),
        iface("en0", "0.0.0.0"),
        iface("en0", "224.0.0.1"),
        iface("en0", "255.255.255.255"),
        NetworkInterface {
            name: "en0".to_string(),
            addr: IpAddr::V6(Ipv6Addr::LOCALHOST),
            is_temporary: false,
            is_deprecated: false,
        },
        NetworkInterface {
            name: "en0".to_string(),
            addr: IpAddr::V4(Ipv4Addr::new(172, 20, 10, 4)),
            is_temporary: false,
            is_deprecated: false,
        },
        iface("en1", "10.0.0.12"),
        iface("en0", "192.168.1.42"),
        iface("en2", "192.168.1.42"), // duplicate IP should deduplicate
        iface("en3", "172.16.5.9"),
        iface("en4", "172.31.255.9"),
    ];

    let info = discover_lan_urls(&interfaces, Some("  My-MacBook.local. \n"), 8789);

    assert_eq!(
        info.preferred_ipv4_url.as_deref(),
        Some("http://192.168.1.42:8789")
    );
    assert_eq!(
        info.ipv4_urls,
        vec![
            "http://192.168.1.42:8789",
            "http://10.0.0.12:8789",
            "http://172.16.5.9:8789",
            "http://172.20.10.4:8789",
            "http://172.31.255.9:8789",
        ]
    );
    assert_eq!(
        info.local_hostname_url.as_deref(),
        Some("http://My-MacBook.local:8789")
    );
}

#[test]
fn handles_empty_interfaces_and_various_local_hostnames() {
    let only_10 = vec![iface("en0", "10.12.34.56")];
    let info = discover_lan_urls(&only_10, Some("Studio.LOCAL"), 9000);
    assert_eq!(
        info.preferred_ipv4_url.as_deref(),
        Some("http://10.12.34.56:9000")
    );
    assert_eq!(info.ipv4_urls, vec!["http://10.12.34.56:9000"]);
    assert_eq!(
        info.local_hostname_url.as_deref(),
        Some("http://Studio.local:9000")
    );

    let none_info = discover_lan_urls(&[], Some("   "), 8789);
    assert_eq!(none_info.preferred_ipv4_url, None);
    assert!(none_info.ipv4_urls.is_empty());
    assert_eq!(none_info.local_hostname_url, None);

    let plain_info = discover_lan_urls(&[], Some("mac-mini"), 8789);
    assert_eq!(
        plain_info.local_hostname_url.as_deref(),
        Some("http://mac-mini.local:8789")
    );
}

#[test]
fn discovers_public_ipv6_filtering_virtual_reserved_deprecated_and_prioritizing_stable() {
    let interfaces = vec![
        // Excluded virtual interfaces even if carrying global unicast IPv6
        ipv6_iface("lo0", "2001:db9::1", false, false),
        ipv6_iface("LO1", "2001:db9::2", false, false),
        ipv6_iface("utun4", "2001:db9::3", false, false),
        ipv6_iface("UTUN9", "2001:db9::4", false, false),
        ipv6_iface("bridge100", "2001:db9::5", false, false),
        ipv6_iface("awdl0", "2001:db9::6", false, false),
        ipv6_iface("llw0", "2001:db9::7", false, false),
        // Excluded non-global or reserved IPv6 addresses
        ipv6_iface("en0", "::1", false, false),
        ipv6_iface("en0", "::", false, false),
        ipv6_iface("en0", "fe80::1234", false, false),
        ipv6_iface("en0", "febf::1", false, false),
        ipv6_iface("en0", "fc00::1", false, false),
        ipv6_iface("en0", "fd12:3456:789a::1", false, false),
        ipv6_iface("en0", "ff02::1", false, false),
        ipv6_iface("en0", "2001:db8::1", false, false),
        ipv6_iface("en0", "2001:db8:abcd::42", false, false),
        ipv6_iface("en0", "::ffff:192.168.1.10", false, false),
        ipv6_iface("en0", "::ffff:203.0.113.1", false, false),
        // IPv4 addresses are ignored
        iface("en0", "192.168.1.42"),
        // Deprecated global unicast addresses must be excluded
        ipv6_iface("en0", "2001:db9::dead", false, true),
        ipv6_iface("en0", "2001:db9::beef", true, true),
        // Temporary global unicast addresses (fallback after stable)
        ipv6_iface("en0", "2001:db9::20", true, false),
        ipv6_iface("en0", "2001:db9::10", true, false),
        // Stable global unicast addresses (preferred before temporary)
        ipv6_iface("en0", "2001:db9:1::2", false, false),
        ipv6_iface("en1", "2001:db9:1::1", false, false),
        // Duplicate IP appearing as temporary on another interface must deduplicate as stable
        ipv6_iface("en2", "2001:db9:1::1", true, false),
        // Duplicate temporary IP must deduplicate
        ipv6_iface("en2", "2001:db9::10", true, false),
    ];

    let info = discover_public_ipv6_urls(&interfaces, 8789);

    assert_eq!(
        info.preferred_ipv6_url.as_deref(),
        Some("http://[2001:db9:1::1]:8789")
    );
    assert_eq!(
        info.ipv6_urls,
        vec![
            "http://[2001:db9:1::1]:8789",
            "http://[2001:db9:1::2]:8789",
            "http://[2001:db9::10]:8789",
            "http://[2001:db9::20]:8789",
        ]
    );
}

#[test]
fn public_ipv6_discovery_falls_back_to_temporary_and_handles_empty() {
    let only_temp = vec![
        ipv6_iface("en0", "2001:db9::99", true, false),
        ipv6_iface("en0", "2001:db9::88", true, false),
    ];
    let temp_info = discover_public_ipv6_urls(&only_temp, 9001);
    assert_eq!(
        temp_info.preferred_ipv6_url.as_deref(),
        Some("http://[2001:db9::88]:9001")
    );
    assert_eq!(
        temp_info.ipv6_urls,
        vec!["http://[2001:db9::88]:9001", "http://[2001:db9::99]:9001",]
    );

    let none_info = discover_public_ipv6_urls(&[ipv6_iface("en0", "fe80::1", false, false)], 8789);
    assert_eq!(none_info.preferred_ipv6_url, None);
    assert!(none_info.ipv6_urls.is_empty());
}

#[test]
fn system_public_ipv6_discovery_returns_well_formed_bracketed_urls() {
    let info = discover_system_public_ipv6_urls(8789);
    assert_eq!(info.preferred_ipv6_url, info.ipv6_urls.first().cloned());
    for url in &info.ipv6_urls {
        assert!(
            url.starts_with("http://[") && url.ends_with("]:8789"),
            "unexpected system public IPv6 URL format: {url}"
        );
    }
}

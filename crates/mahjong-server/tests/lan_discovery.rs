use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use mahjong_server::lan::{discover_lan_urls, NetworkInterface};

fn iface(name: &str, ip: &str) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        addr: ip.parse::<IpAddr>().expect("valid test ip"),
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
        },
        NetworkInterface {
            name: "en0".to_string(),
            addr: IpAddr::V4(Ipv4Addr::new(172, 20, 10, 4)),
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

//! Pure-Rust LAN IPv4 and Bonjour `.local` access URL discovery.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
use std::process::Command;

use crate::protocol::LanUrlInfo;

const EXCLUDED_INTERFACE_PREFIXES: [&str; 5] = ["utun", "bridge", "awdl", "llw", "lo"];

/// Snapshot of a single network interface address used as the testable LAN discovery seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterface {
    /// Operating system interface name (e.g. `en0`, `utun4`, `lo0`).
    pub name: String,
    /// Assigned IP address on that interface.
    pub addr: IpAddr,
}

/// Discovers prioritized LAN IPv4 URLs and `.local` URL from an explicit interface snapshot.
#[must_use]
pub fn discover_lan_urls(
    interfaces: &[NetworkInterface],
    local_hostname: Option<&str>,
    port: u16,
) -> LanUrlInfo {
    let mut ranked: BTreeSet<(u8, Ipv4Addr)> = BTreeSet::new();

    for iface in interfaces {
        if is_excluded_interface(&iface.name) {
            continue;
        }
        let IpAddr::V4(ip) = iface.addr else {
            continue;
        };
        if let Some(priority) = lan_ipv4_priority(ip) {
            ranked.insert((priority, ip));
        }
    }

    let ipv4_urls: Vec<String> = ranked
        .into_iter()
        .map(|(_, ip)| format!("http://{ip}:{port}"))
        .collect();
    let preferred_ipv4_url = ipv4_urls.first().cloned();
    let local_hostname_url = local_hostname.and_then(|raw| format_local_hostname_url(raw, port));

    LanUrlInfo {
        preferred_ipv4_url,
        ipv4_urls,
        local_hostname_url,
    }
}

/// Reads active host network interfaces via `if-addrs`.
#[must_use]
pub fn read_system_interfaces() -> Vec<NetworkInterface> {
    let Ok(addrs) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    addrs
        .into_iter()
        .map(|iface| NetworkInterface {
            name: iface.name,
            addr: iface.addr.ip(),
        })
        .collect()
}

/// Resolves the host's native `.local` name (`scutil --get LocalHostName` on macOS, falling back
/// to `hostname` / `HOSTNAME`).
#[must_use]
pub fn read_local_hostname() -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Some(name) = run_command_stdout("scutil", &["--get", "LocalHostName"]) {
        return Some(name);
    }

    if let Some(name) = run_command_stdout("hostname", &[]) {
        return Some(name);
    }

    std::env::var("HOSTNAME")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Discovers LAN URLs from the live host OS network interfaces and hostname.
#[must_use]
pub fn discover_system_lan_urls(port: u16) -> LanUrlInfo {
    let interfaces = read_system_interfaces();
    let local_hostname = read_local_hostname();
    discover_lan_urls(&interfaces, local_hostname.as_deref(), port)
}

fn run_command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn is_excluded_interface(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    EXCLUDED_INTERFACE_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn lan_ipv4_priority(ip: Ipv4Addr) -> Option<u8> {
    if ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
    {
        return None;
    }

    let [a, b, _, _] = ip.octets();
    // Exclude CGNAT (100.64.0.0/10) and benchmark (198.18.0.0/15) ranges explicitly.
    if (a == 100 && (64..=127).contains(&b)) || (a == 198 && (b == 18 || b == 19)) {
        return None;
    }

    if a == 192 && b == 168 {
        Some(0)
    } else if a == 10 {
        Some(1)
    } else if a == 172 && (16..=31).contains(&b) {
        Some(2)
    } else {
        None
    }
}

fn format_local_hostname_url(raw: &str, port: u16) -> Option<String> {
    let mut name = raw.trim().trim_end_matches('.');
    if name.len() >= 6 && name[name.len() - 6..].eq_ignore_ascii_case(".local") {
        name = &name[..name.len() - 6];
    }
    let clean = name.trim().trim_end_matches('.');
    if clean.is_empty() || clean.chars().any(char::is_whitespace) {
        return None;
    }
    Some(format!("http://{clean}.local:{port}"))
}

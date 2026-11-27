//! Pure-Rust LAN IPv4, Bonjour `.local`, and public IPv6 access URL discovery.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::process::Command;

use crate::protocol::LanUrlInfo;
use crate::security::{is_ipv6_link_local, is_ipv6_ula};

const EXCLUDED_INTERFACE_PREFIXES: [&str; 5] = ["utun", "bridge", "awdl", "llw", "lo"];

/// Snapshot of a single network interface address used as the testable network discovery seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterface {
    /// Operating system interface name (e.g. `en0`, `utun4`, `lo0`).
    pub name: String,
    /// Assigned IP address on that interface.
    pub addr: IpAddr,
    /// `true` when the OS flags this IPv6 address as temporary (`IN6_IFF_TEMPORARY` on macOS).
    pub is_temporary: bool,
    /// `true` when the OS flags this IPv6 address as deprecated (`IN6_IFF_DEPRECATED` on macOS).
    pub is_deprecated: bool,
}

impl NetworkInterface {
    /// Constructs an interface snapshot with default (`false`) IPv6 address flags.
    #[must_use]
    pub fn new(name: impl Into<String>, addr: IpAddr) -> Self {
        Self {
            name: name.into(),
            addr,
            is_temporary: false,
            is_deprecated: false,
        }
    }
}

/// Discovered global-unicast public IPv6 access URLs for remote invitations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicIpv6UrlInfo {
    /// Preferred stable (non-temporary) public IPv6 URL, falling back to a temporary URL if none exist.
    pub preferred_ipv6_url: Option<String>,
    /// Deduplicated `http://[<ipv6>]:<port>` list ordered with stable addresses before temporary ones.
    pub ipv6_urls: Vec<String>,
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

/// Discovers deduplicated global-unicast public IPv6 URLs (`http://[<ipv6>]:<port>`),
/// filtering virtual interfaces, non-global/reserved ranges, and deprecated addresses,
/// and sorting stable (non-temporary) addresses ahead of temporary privacy addresses.
#[must_use]
pub fn discover_public_ipv6_urls(interfaces: &[NetworkInterface], port: u16) -> PublicIpv6UrlInfo {
    let mut best_rank_by_ip: BTreeMap<Ipv6Addr, u8> = BTreeMap::new();

    for iface in interfaces {
        if is_excluded_interface(&iface.name) || iface.is_deprecated {
            continue;
        }
        let IpAddr::V6(ip) = iface.addr else {
            continue;
        };
        if !is_public_unicast_ipv6(ip) {
            continue;
        }
        let rank = u8::from(iface.is_temporary);
        best_rank_by_ip
            .entry(ip)
            .and_modify(|existing| *existing = (*existing).min(rank))
            .or_insert(rank);
    }

    let mut ranked: Vec<(u8, Ipv6Addr)> = best_rank_by_ip
        .into_iter()
        .map(|(ip, rank)| (rank, ip))
        .collect();
    ranked.sort_unstable();

    let ipv6_urls: Vec<String> = ranked
        .into_iter()
        .map(|(_, ip)| format!("http://[{ip}]:{port}"))
        .collect();
    let preferred_ipv6_url = ipv6_urls.first().cloned();

    PublicIpv6UrlInfo {
        preferred_ipv6_url,
        ipv6_urls,
    }
}

/// Reads active host network interfaces via `if-addrs`, enriching IPv6 address flags on macOS.
#[must_use]
pub fn read_system_interfaces() -> Vec<NetworkInterface> {
    let Ok(addrs) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let flag_reader = Ipv6FlagReader::open();
    addrs
        .into_iter()
        .map(|iface| {
            let addr = iface.addr.ip();
            let (is_temporary, is_deprecated) = match addr {
                IpAddr::V6(v6) => flag_reader.query(&iface.name, v6),
                IpAddr::V4(_) => (false, false),
            };
            NetworkInterface {
                name: iface.name,
                addr,
                is_temporary,
                is_deprecated,
            }
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

/// Discovers public global-unicast IPv6 URLs from the live host OS network interfaces.
#[must_use]
pub fn discover_system_public_ipv6_urls(port: u16) -> PublicIpv6UrlInfo {
    let interfaces = read_system_interfaces();
    discover_public_ipv6_urls(&interfaces, port)
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

fn is_public_unicast_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || ip.to_ipv4_mapped().is_some()
    {
        return false;
    }

    let segments = ip.segments();
    let is_documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let is_global_unicast = (segments[0] & 0xe000) == 0x2000;

    is_global_unicast && !is_ipv6_link_local(ip) && !is_ipv6_ula(ip) && !is_documentation
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

#[cfg(target_os = "macos")]
mod ipv6_flags {
    use std::mem::{size_of, zeroed};
    use std::net::Ipv6Addr;

    const IN6_IFF_DEPRECATED: libc::c_int = 0x0010;
    const IN6_IFF_TEMPORARY: libc::c_int = 0x0080;

    // macOS `<netinet6/in6_var.h>`: `struct in6_ifreq` is 16-byte `ifr_name` + 272-byte union = 288 bytes.
    #[repr(C)]
    union In6Ifru {
        addr: libc::sockaddr_in6,
        flags6: libc::c_int,
        _pad: [u64; 34],
    }

    #[repr(C)]
    struct In6Ifreq {
        ifr_name: [libc::c_char; libc::IFNAMSIZ],
        ifr_ifru: In6Ifru,
    }

    // `_IOWR('i', 73, struct in6_ifreq)` on macOS = 0xc1206949.
    const SIOCGIFAFLAG_IN6: libc::c_ulong = 0xc000_0000
        | ((size_of::<In6Ifreq>() as libc::c_ulong) << 16)
        | ((b'i' as libc::c_ulong) << 8)
        | 73;

    pub(super) struct Ipv6FlagReader {
        fd: Option<libc::c_int>,
    }

    impl Ipv6FlagReader {
        pub(super) fn open() -> Self {
            // SAFETY: Creating a datagram IPv6 socket with standard constants has no memory preconditions.
            let fd = unsafe { libc::socket(libc::AF_INET6, libc::SOCK_DGRAM, 0) };
            if fd < 0 {
                Self { fd: None }
            } else {
                Self { fd: Some(fd) }
            }
        }

        pub(super) fn query(&self, iface_name: &str, ip: Ipv6Addr) -> (bool, bool) {
            let Some(fd) = self.fd else {
                return (false, false);
            };
            let name_bytes = iface_name.as_bytes();
            if name_bytes.is_empty() || name_bytes.len() >= libc::IFNAMSIZ {
                return (false, false);
            }

            // SAFETY: `In6Ifreq` is a plain C struct/union where all-zero bytes are valid.
            let mut req: In6Ifreq = unsafe { zeroed() };
            for (dst, &src) in req.ifr_name.iter_mut().zip(name_bytes.iter()) {
                *dst = src as libc::c_char;
            }

            // SAFETY: `sockaddr_in6` is a plain C struct where all-zero bytes are valid.
            let mut sin6: libc::sockaddr_in6 = unsafe { zeroed() };
            sin6.sin6_len = size_of::<libc::sockaddr_in6>() as u8;
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_addr.s6_addr = ip.octets();
            req.ifr_ifru.addr = sin6;

            // SAFETY: `fd` is a valid AF_INET6 socket and `req` matches macOS `struct in6_ifreq` ABI.
            let rc = unsafe { libc::ioctl(fd, SIOCGIFAFLAG_IN6, &mut req) };
            if rc != 0 {
                return (false, false);
            }
            // SAFETY: On success `SIOCGIFAFLAG_IN6` writes `ifru_flags6` (`c_int`) into `ifr_ifru`.
            let flags = unsafe { req.ifr_ifru.flags6 };
            decode_in6_flags(flags)
        }
    }

    impl Drop for Ipv6FlagReader {
        fn drop(&mut self) {
            if let Some(fd) = self.fd.take() {
                // SAFETY: `fd` is owned exclusively by `Ipv6FlagReader` and closed once here.
                unsafe {
                    libc::close(fd);
                }
            }
        }
    }

    pub(super) fn decode_in6_flags(flags: libc::c_int) -> (bool, bool) {
        (
            (flags & IN6_IFF_TEMPORARY) != 0,
            (flags & IN6_IFF_DEPRECATED) != 0,
        )
    }

    #[cfg(test)]
    mod tests {
        use std::mem::size_of;

        use super::{
            decode_in6_flags, In6Ifreq, IN6_IFF_DEPRECATED, IN6_IFF_TEMPORARY, SIOCGIFAFLAG_IN6,
        };

        #[test]
        fn macos_in6_ifreq_abi_and_flag_decoding_match_sdk() {
            assert_eq!(size_of::<In6Ifreq>(), 288);
            assert_eq!(SIOCGIFAFLAG_IN6, 0xc120_6949);
            assert_eq!(decode_in6_flags(0), (false, false));
            assert_eq!(decode_in6_flags(IN6_IFF_TEMPORARY), (true, false));
            assert_eq!(decode_in6_flags(IN6_IFF_DEPRECATED), (false, true));
            assert_eq!(
                decode_in6_flags(IN6_IFF_TEMPORARY | IN6_IFF_DEPRECATED),
                (true, true)
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod ipv6_flags {
    use std::net::Ipv6Addr;

    pub(super) struct Ipv6FlagReader;

    impl Ipv6FlagReader {
        pub(super) fn open() -> Self {
            Self
        }

        pub(super) fn query(&self, _iface_name: &str, _ip: Ipv6Addr) -> (bool, bool) {
            (false, false)
        }
    }
}

use ipv6_flags::Ipv6FlagReader;

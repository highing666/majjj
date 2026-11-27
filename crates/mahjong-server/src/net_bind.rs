//! Dual-stack IPv6 (`[::]`) TCP binding via `socket2` with graceful IPv4 fallback.

use std::future::Future;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

use crate::server::ServerError;

const LISTEN_BACKLOG: i32 = 1024;

/// Returns `true` when `err` indicates the host OS/kernel lacks IPv6 support or cannot assign `[::]`.
///
/// Port conflicts (`AddrInUse`) and permission errors (`PermissionDenied`) return `false` so
/// callers never silently mask them with an IPv4 fallback.
#[must_use]
pub(crate) fn is_ipv6_unavailable_error(err: &std::io::Error) -> bool {
    if matches!(
        err.kind(),
        ErrorKind::AddrNotAvailable | ErrorKind::Unsupported
    ) {
        return true;
    }
    if let Some(code) = err.raw_os_error() {
        #[cfg(target_os = "macos")]
        if matches!(code, 43 | 47 | 49) {
            // EPROTONOSUPPORT (43), EAFNOSUPPORT (47), EADDRNOTAVAIL (49)
            return true;
        }
        #[cfg(target_os = "linux")]
        if matches!(code, 93 | 97 | 99) {
            // EPROTONOSUPPORT (93), EAFNOSUPPORT (97), EADDRNOTAVAIL (99)
            return true;
        }
        #[cfg(target_os = "windows")]
        if matches!(code, 10043 | 10047 | 10049) {
            // WSAEPROTONOSUPPORT, WSAEAFNOSUPPORT, WSAEADDRNOTAVAIL
            return true;
        }
    }
    false
}

/// Binds a single dual-stack TCP socket on an IPv6 wildcard address (`[::]:port`) with
/// `IPV6_V6ONLY = false` so both IPv4 and IPv6 clients connect on the same port.
fn bind_dual_stack_v6_listener(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    socket.set_nonblocking(true)?;
    let std_listener: std::net::TcpListener = socket.into();
    TcpListener::from_std(std_listener)
}

async fn bind_single_addr(addr: SocketAddr) -> std::io::Result<TcpListener> {
    match addr {
        SocketAddr::V6(v6) if v6.ip().is_unspecified() => bind_dual_stack_v6_listener(addr),
        _ => TcpListener::bind(addr).await,
    }
}

/// Binds the server TCP listener, using dual-stack `[::]` by default and falling back to
/// `0.0.0.0:<port>` only when `[::]` was implicit (`!bind_explicit`) and IPv6 is unavailable.
pub(crate) async fn bind_tcp_listener(
    bind: IpAddr,
    port: u16,
    bind_explicit: bool,
) -> Result<(TcpListener, SocketAddr), ServerError> {
    let listener = bind_tcp_listener_with(bind, port, bind_explicit, bind_single_addr).await?;
    let local_addr = listener.local_addr().map_err(|source| ServerError::Bind {
        addr: SocketAddr::new(bind, port),
        source,
    })?;
    Ok((listener, local_addr))
}

async fn bind_tcp_listener_with<T, F, Fut>(
    bind: IpAddr,
    port: u16,
    bind_explicit: bool,
    mut binder: F,
) -> Result<T, ServerError>
where
    F: FnMut(SocketAddr) -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    let primary = SocketAddr::new(bind, port);
    match binder(primary).await {
        Ok(listener) => Ok(listener),
        Err(err)
            if !bind_explicit
                && bind == IpAddr::V6(Ipv6Addr::UNSPECIFIED)
                && is_ipv6_unavailable_error(&err) =>
        {
            let fallback = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
            eprintln!(
                "warning: dual-stack IPv6 bind on {primary} unavailable ({err}); falling back to {fallback}"
            );
            binder(fallback).await.map_err(|source| ServerError::Bind {
                addr: fallback,
                source,
            })
        }
        Err(source) => Err(ServerError::Bind {
            addr: primary,
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    use super::{bind_tcp_listener_with, is_ipv6_unavailable_error};
    use crate::server::ServerError;

    #[test]
    fn ipv6_unavailable_classifier_distinguishes_unsupported_from_conflicts() {
        assert!(is_ipv6_unavailable_error(&std::io::Error::new(
            ErrorKind::AddrNotAvailable,
            "address not available",
        )));
        assert!(is_ipv6_unavailable_error(&std::io::Error::new(
            ErrorKind::Unsupported,
            "address family not supported",
        )));
        assert!(!is_ipv6_unavailable_error(&std::io::Error::new(
            ErrorKind::AddrInUse,
            "address already in use",
        )));
        assert!(!is_ipv6_unavailable_error(&std::io::Error::new(
            ErrorKind::PermissionDenied,
            "permission denied",
        )));
    }

    #[tokio::test]
    async fn implicit_dual_stack_falls_back_to_ipv4_when_ipv6_unavailable() {
        let mut attempted = Vec::new();
        let bound =
            bind_tcp_listener_with(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789, false, |addr| {
                attempted.push(addr);
                async move {
                    if addr.is_ipv6() {
                        Err(std::io::Error::new(ErrorKind::Unsupported, "EAFNOSUPPORT"))
                    } else {
                        Ok(addr)
                    }
                }
            })
            .await
            .expect("should fall back to 0.0.0.0:8789");

        assert_eq!(
            attempted,
            vec![
                SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8789),
            ]
        );
        assert_eq!(
            bound,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8789)
        );
    }

    #[tokio::test]
    async fn implicit_dual_stack_never_falls_back_on_addr_in_use() {
        let mut attempted = Vec::new();
        let err = bind_tcp_listener_with(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789, false, |addr| {
            attempted.push(addr);
            async move {
                Err::<SocketAddr, _>(std::io::Error::new(ErrorKind::AddrInUse, "port occupied"))
            }
        })
        .await
        .expect_err("AddrInUse must fail immediately without IPv4 fallback");

        assert_eq!(
            attempted,
            vec![SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789)]
        );
        assert!(matches!(
            err,
            ServerError::Bind { source, .. } if source.kind() == ErrorKind::AddrInUse
        ));
    }

    #[tokio::test]
    async fn explicit_ipv6_bind_never_falls_back_even_when_ipv6_unavailable() {
        let mut attempted = Vec::new();
        let err = bind_tcp_listener_with(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789, true, |addr| {
            attempted.push(addr);
            async move {
                Err::<SocketAddr, _>(std::io::Error::new(
                    ErrorKind::AddrNotAvailable,
                    "EADDRNOTAVAIL",
                ))
            }
        })
        .await
        .expect_err("explicit --bind :: must not silently fall back to IPv4");

        assert_eq!(
            attempted,
            vec![SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8789)]
        );
        assert!(matches!(
            err,
            ServerError::Bind { source, .. } if source.kind() == ErrorKind::AddrNotAvailable
        ));
    }
}

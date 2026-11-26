//! Server lifecycle helpers: loopback peer checks and split idle auto-shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::protocol::TablePhase;

/// Default idle timeout for `Lobby` and `Finished` phases (15 minutes).
pub const DEFAULT_LOBBY_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Default idle timeout for `InMatch` phase (4 hours).
pub const DEFAULT_IN_MATCH_IDLE_TIMEOUT: Duration = Duration::from_secs(4 * 3600);

/// Configurable idle auto-exit thresholds for desktop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleTimeouts {
    /// Grace period at 0 active WebSocket connections in `Lobby` or `Finished`.
    pub lobby_or_finished: Duration,
    /// Grace period at 0 active WebSocket connections in `InMatch`.
    pub in_match: Duration,
}

impl Default for IdleTimeouts {
    fn default() -> Self {
        Self {
            lobby_or_finished: DEFAULT_LOBBY_IDLE_TIMEOUT,
            in_match: DEFAULT_IN_MATCH_IDLE_TIMEOUT,
        }
    }
}

impl IdleTimeouts {
    /// Returns the idle threshold corresponding to `phase`.
    #[must_use]
    pub const fn for_phase(self, phase: TablePhase) -> Duration {
        match phase {
            TablePhase::Lobby | TablePhase::Finished => self.lobby_or_finished,
            TablePhase::InMatch => self.in_match,
        }
    }
}

/// Returns `true` when `peer` has a canonical loopback IP (`127.0.0.1` or `::1`).
#[must_use]
pub fn is_loopback_peer(peer: Option<SocketAddr>) -> bool {
    peer.is_some_and(|addr| addr.ip().to_canonical().is_loopback())
}

/// Tracks active WebSocket connections across the Axum router.
#[derive(Clone, Debug)]
pub struct WsConnectionTracker {
    tx: Arc<watch::Sender<usize>>,
}

impl Default for WsConnectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl WsConnectionTracker {
    /// Creates a new tracker starting at 0 active connections.
    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = watch::channel(0);
        Self { tx: Arc::new(tx) }
    }

    /// Subscribes to active WebSocket connection count updates.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<usize> {
        self.tx.subscribe()
    }

    /// Returns the current active WebSocket connection count.
    #[must_use]
    pub fn count(&self) -> usize {
        *self.tx.borrow()
    }

    /// Increments the active connection count and returns an RAII guard that decrements on drop.
    #[must_use]
    pub fn enter(&self) -> WsConnectionGuard {
        self.tx.send_modify(|count| *count += 1);
        WsConnectionGuard {
            tx: Arc::clone(&self.tx),
        }
    }
}

/// RAII guard decrementing the active WebSocket connection count when dropped.
#[derive(Debug)]
pub struct WsConnectionGuard {
    tx: Arc<watch::Sender<usize>>,
}

impl Drop for WsConnectionGuard {
    fn drop(&mut self) {
        self.tx.send_modify(|count| {
            *count = count.saturating_sub(1);
        });
    }
}

/// Spawns the desktop-mode split idle auto-shutdown monitor task.
pub fn spawn_idle_shutdown_monitor(
    timeouts: IdleTimeouts,
    mut ws_rx: watch::Receiver<usize>,
    mut phase_rx: watch::Receiver<TablePhase>,
    shutdown_tx: watch::Sender<bool>,
) -> tokio::task::JoinHandle<()> {
    let mut shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut idle_since: Option<tokio::time::Instant> = None;

        loop {
            if *shutdown_rx.borrow_and_update() {
                break;
            }
            let active_ws = *ws_rx.borrow_and_update();
            let phase = *phase_rx.borrow_and_update();

            if active_ws > 0 {
                idle_since = None;
                tokio::select! {
                    res = ws_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                    }
                    res = shutdown_rx.changed() => {
                        if res.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
                continue;
            }

            let start = *idle_since.get_or_insert_with(tokio::time::Instant::now);
            let deadline = start + timeouts.for_phase(phase);
            let sleep = tokio::time::sleep_until(deadline);
            tokio::pin!(sleep);

            tokio::select! {
                () = &mut sleep => {
                    if *ws_rx.borrow() == 0 {
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                }
                res = ws_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                    idle_since = None;
                }
                res = phase_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                }
                res = shutdown_rx.changed() => {
                    if res.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::time::Duration;

    use tokio::sync::watch;

    use super::{is_loopback_peer, spawn_idle_shutdown_monitor, IdleTimeouts, WsConnectionTracker};
    use crate::protocol::TablePhase;

    #[test]
    fn loopback_peer_check_accepts_canonical_loopback_and_rejects_lan() {
        assert!(is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            8080
        ))));
        assert!(is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            8080
        ))));
        assert!(is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()),
            8080
        ))));

        assert!(!is_loopback_peer(None));
        assert!(!is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            8080
        ))));
        assert!(!is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V6(Ipv4Addr::new(192, 168, 1, 20).to_ipv6_mapped()),
            8080
        ))));
    }

    #[tokio::test(start_paused = true)]
    async fn idle_monitor_uses_split_thresholds_resets_on_connect_and_applies_phase_change() {
        let tracker = WsConnectionTracker::new();
        let (phase_tx, phase_rx) = watch::channel(TablePhase::Lobby);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let timeouts = IdleTimeouts {
            lobby_or_finished: Duration::from_secs(15 * 60),
            in_match: Duration::from_secs(4 * 3600),
        };
        let handle = spawn_idle_shutdown_monitor(
            timeouts,
            tracker.subscribe(),
            phase_rx,
            shutdown_tx.clone(),
        );
        tokio::task::yield_now().await;

        // 1. Advance 14m in Lobby -> still alive; connect at 14m -> resets timer.
        tokio::time::advance(Duration::from_secs(14 * 60)).await;
        tokio::task::yield_now().await;
        assert!(!*shutdown_rx.borrow());

        let guard = tracker.enter();
        tokio::task::yield_now().await;

        // Even after 1 hour with 1 active connection, server does not shut down.
        tokio::time::advance(Duration::from_secs(3600)).await;
        tokio::task::yield_now().await;
        assert!(!*shutdown_rx.borrow());

        // 2. Switch to InMatch and disconnect -> uses 4h threshold.
        phase_tx.send_replace(TablePhase::InMatch);
        drop(guard);
        tokio::task::yield_now().await;

        // After 1 hour at 0 connections in InMatch -> still alive.
        tokio::time::advance(Duration::from_secs(3600)).await;
        tokio::task::yield_now().await;
        assert!(!*shutdown_rx.borrow());

        // 3. Transition InMatch -> Finished while at 0 connections -> applies 15m threshold and triggers shutdown.
        phase_tx.send_replace(TablePhase::Finished);
        tokio::task::yield_now().await;
        assert!(*shutdown_rx.borrow());

        handle.await.expect("monitor exits cleanly");
    }

    #[tokio::test(start_paused = true)]
    async fn idle_monitor_exits_after_full_in_match_threshold_if_phase_stays_in_match() {
        let tracker = WsConnectionTracker::new();
        let (_phase_tx, phase_rx) = watch::channel(TablePhase::InMatch);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let timeouts = IdleTimeouts {
            lobby_or_finished: Duration::from_secs(15 * 60),
            in_match: Duration::from_secs(4 * 3600),
        };
        let handle =
            spawn_idle_shutdown_monitor(timeouts, tracker.subscribe(), phase_rx, shutdown_tx);
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(4 * 3600 - 1)).await;
        tokio::task::yield_now().await;
        assert!(!*shutdown_rx.borrow());

        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(*shutdown_rx.borrow());

        handle.await.expect("monitor exits cleanly");
    }
}

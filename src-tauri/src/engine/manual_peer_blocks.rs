//! Persistent, application-scoped peer IP blocks.
//!
//! Transmission does not expose a per-peer/per-torrent deny-list RPC. Nebulae
//! therefore persists blocked IPs and repeatedly disconnects matching peers
//! through Transmission's `peer-remove` RPC. This is app-level reconnect
//! enforcement, not an operating-system firewall rule; unsupported engines or
//! daemons report the capability as unavailable.

use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use serde_json::Value;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::{db::Database, engine::transmission::TransmissionRpcClient};

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const MAX_REMOVALS_PER_CYCLE: usize = 50;

pub fn spawn(
    db: Arc<Database>,
    rpc: Arc<TransmissionRpcClient>,
    peer_remove_unsupported: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        sleep(POLL_INTERVAL).await;
        loop {
            if peer_remove_unsupported.load(Ordering::Relaxed) {
                sleep(POLL_INTERVAL).await;
                continue;
            }

            let blocked_ips: HashSet<String> = match db.list_blocked_peer_ips() {
                Ok(entries) => entries.into_iter().map(|entry| entry.ip).collect(),
                Err(error) => {
                    warn!("[peer-block] could not read persisted blocked IPs: {error}");
                    sleep(POLL_INTERVAL).await;
                    continue;
                }
            };

            if blocked_ips.is_empty() {
                sleep(POLL_INTERVAL).await;
                continue;
            }

            let rpc = Arc::clone(&rpc);
            let unsupported = Arc::clone(&peer_remove_unsupported);
            if let Err(error) = tokio::task::spawn_blocking(move || {
                disconnect_blocked_peers(&rpc, &blocked_ips, &unsupported)
            })
            .await
            {
                warn!("[peer-block] enforcement worker failed: {error}");
            }
            sleep(POLL_INTERVAL).await;
        }
    });
}

fn disconnect_blocked_peers(
    rpc: &TransmissionRpcClient,
    blocked_ips: &HashSet<String>,
    peer_remove_unsupported: &AtomicBool,
) {
    let torrents = match rpc.torrent_peers_all() {
        Ok(torrents) => torrents,
        Err(error) => {
            warn!("[peer-block] could not inspect active peers: {error}");
            return;
        }
    };

    let mut removals = 0usize;
    'torrents: for torrent in &torrents {
        let Some(torrent_id) = torrent.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let Some(peers) = torrent.get("peers").and_then(Value::as_array) else {
            continue;
        };

        for peer in peers {
            if removals >= MAX_REMOVALS_PER_CYCLE {
                debug!("[peer-block] per-cycle disconnect cap reached; remaining peers deferred");
                break 'torrents;
            }

            let Some(ip) = peer.get("address").and_then(Value::as_str) else {
                continue;
            };
            let Ok(ip_addr) = ip.parse::<IpAddr>() else {
                continue;
            };
            if !blocked_ips.contains(&ip_addr.to_string()) {
                continue;
            }
            let Some(port) = peer.get("port").and_then(Value::as_u64) else {
                continue;
            };
            let Ok(port) = u16::try_from(port) else {
                continue;
            };
            let address = SocketAddr::new(ip_addr, port).to_string();

            match rpc.peer_remove(torrent_id, &address) {
                Ok(()) => removals += 1,
                Err(error) if error.contains("peer-remove") => {
                    peer_remove_unsupported.store(true, Ordering::Relaxed);
                    warn!(
                        "[peer-block] Transmission peer-remove is unavailable; persistent peer blocking is disabled"
                    );
                    return;
                }
                Err(_) => {
                    // The peer may have disconnected between listing and removal.
                }
            }
        }
    }

    if removals > 0 {
        debug!("[peer-block] disconnected {removals} matching peer connection(s)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_address_format_brackets_ipv6_and_preserves_ipv4() {
        assert_eq!(
            SocketAddr::new("2001:db8::1".parse().unwrap(), 41413).to_string(),
            "[2001:db8::1]:41413"
        );
        assert_eq!(
            SocketAddr::new("192.0.2.1".parse().unwrap(), 41413).to_string(),
            "192.0.2.1:41413"
        );
    }
}

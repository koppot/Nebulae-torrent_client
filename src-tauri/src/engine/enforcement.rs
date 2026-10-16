//! Tracker enforcement feed poller.
//!
//! Polls `/api/blocked_peers/<passkey>` on the ADC tracker every
//! [`POLL_INTERVAL_SECS`] seconds and issues Transmission `peer-remove` RPC
//! calls for any connected leecher whose IP appears in the response.
//!
//! # Design
//!
//! - Runs as a single background Tokio task, spawned once by the engine.
//! - Uses the seeder's encrypted passkey (read from `AppSettings`) for auth.
//! - Authenticates to the tracker with the same passkey format used by
//!   announce — the tracker decrypts it server-side to identify the seeder.
//! - `peer-remove` calls are issued via the synchronous `TransmissionRpcClient`
//!   from inside `tokio::task::spawn_blocking` to avoid blocking the async runtime.
//! - If `peer-remove` is unsupported by the daemon (error contains "peer-remove"),
//!   the poller marks the RPC as unavailable and logs a warning — it does NOT
//!   keep retrying a known-unsupported call.
//! - On any tracker HTTP error the cycle is silently skipped; no retry storm.
//!
//! # Privacy
//!
//! The tracker endpoint returns only IP addresses of currently-active blocked
//! leechers. No usernames, account details or block reasons are transmitted.
//! The seeder's passkey is sent over HTTPS and is the same credential already
//! used for every announce request.

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::Deserialize;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::engine::transmission::TransmissionRpcClient;

/// Polling interval in seconds. Tracker rate-limits to 1 call/60s per passkey;
/// we poll every 120s to stay comfortably within that limit.
const POLL_INTERVAL_SECS: u64 = 120;

/// Maximum peer-remove calls per poll cycle across all torrents.
/// Protects against a pathological blocked-IP list causing a thundering-herd
/// of RPC calls on a single cycle.
const MAX_REMOVALS_PER_CYCLE: usize = 50;

#[derive(Debug, Deserialize)]
struct BlockedPeersResponse {
    blocked_ips: Vec<String>,
}

/// Spawn the enforcement poller as a detached Tokio task.
///
/// # Arguments
/// - `tracker_base_url` – e.g. `"https://asiandvdclub.org"` (no trailing slash)
/// - `encrypted_passkey` – the seeder's encrypted passkey (as used in announce URLs)
/// - `rpc_client` – shared reference to the Transmission RPC client
///
/// The task runs until the process exits. It is fire-and-forget: the caller
/// does not need to await or join it.
pub fn spawn(
    tracker_base_url: String,
    encrypted_passkey: String,
    rpc_client: Arc<TransmissionRpcClient>,
) {
    tokio::spawn(async move {
        run_poller(tracker_base_url, encrypted_passkey, rpc_client).await;
    });
}

async fn run_poller(
    tracker_base_url: String,
    encrypted_passkey: String,
    rpc_client: Arc<TransmissionRpcClient>,
) {
    // Set to true if the daemon returns an error indicating peer-remove is not
    // supported. Prevents repeated futile RPC calls.
    let peer_remove_unsupported = Arc::new(AtomicBool::new(false));

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let feed_url = format!(
        "{}/api/blocked_peers/{}",
        tracker_base_url.trim_end_matches('/'),
        encrypted_passkey
    );

    info!("[enforcement] poller started — feed: {}", feed_url);

    loop {
        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;

        if peer_remove_unsupported.load(Ordering::Relaxed) {
            // RPC unavailable: keep sleeping so the loop stays alive in case
            // a future daemon upgrade enables peer-remove, but skip all work.
            warn!("[enforcement] peer-remove unsupported by daemon — poller idle");
            continue;
        }

        // ── 1. Fetch blocked IP list from tracker ────────────────────────────
        let blocked_ips: HashSet<String> = match http.get(&feed_url).send().await {
            Err(e) => {
                warn!("[enforcement] tracker fetch error (skipping cycle): {e}");
                continue;
            }
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    warn!("[enforcement] rate-limited by tracker — skipping cycle");
                    continue;
                }
                if !status.is_success() {
                    warn!("[enforcement] tracker returned {status} — skipping cycle");
                    continue;
                }
                match resp.json::<BlockedPeersResponse>().await {
                    Ok(body) => body.blocked_ips.into_iter().collect(),
                    Err(e) => {
                        warn!("[enforcement] failed to parse tracker response: {e}");
                        continue;
                    }
                }
            }
        };

        if blocked_ips.is_empty() {
            // Nothing to do this cycle.
            continue;
        }

        info!(
            "[enforcement] {} blocked IP(s) active in swarm",
            blocked_ips.len()
        );

        // ── 2. Fetch current peers from Transmission ─────────────────────────
        let rpc = Arc::clone(&rpc_client);
        let blocked_clone = blocked_ips.clone();
        let peer_remove_flag = Arc::clone(&peer_remove_unsupported);

        let result = tokio::task::spawn_blocking(move || {
            disconnect_blocked_peers(&rpc, &blocked_clone, &peer_remove_flag)
        })
        .await;

        if let Err(e) = result {
            warn!("[enforcement] spawn_blocking join error: {e}");
        }
    }
}

/// Iterate every active torrent's peer list; for any peer whose IP is in
/// `blocked_ips`, issue a `peer-remove` RPC call.
///
/// Runs in a blocking thread pool thread (called via `spawn_blocking`).
fn disconnect_blocked_peers(
    rpc: &TransmissionRpcClient,
    blocked_ips: &HashSet<String>,
    peer_remove_unsupported: &AtomicBool,
) {
    let torrents = match rpc.torrent_get(None) {
        Ok(t) => t,
        Err(e) => {
            warn!("[enforcement] torrent_get failed: {e}");
            return;
        }
    };

    let mut removal_count = 0usize;

    'outer: for torrent in &torrents {
        let torrent_id = match torrent.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => continue,
        };

        let peers = match torrent.get("peers").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => continue,
        };

        for peer in peers {
            if removal_count >= MAX_REMOVALS_PER_CYCLE {
                warn!(
                    "[enforcement] removal cap ({MAX_REMOVALS_PER_CYCLE}) reached — \
                     remaining blocked peers deferred to next cycle"
                );
                break 'outer;
            }

            let address = match peer.get("address").and_then(|a| a.as_str()) {
                Some(a) => a,
                None => continue,
            };
            let port = peer.get("port").and_then(|p| p.as_u64()).unwrap_or(0);

            // The IP in the tracker response has no port; compare IP only.
            let ip = address; // Transmission's "address" field is just the IP
            if !blocked_ips.contains(ip) {
                continue;
            }

            let peer_addr = format!("{ip}:{port}");
            match rpc.peer_remove(torrent_id, &peer_addr) {
                Ok(()) => {
                    info!(
                        "[enforcement] disconnected blocked peer {peer_addr} \
                         from torrent {torrent_id}"
                    );
                    removal_count += 1;
                }
                Err(e) if e.contains("peer-remove") => {
                    // Daemon does not support this RPC method.
                    peer_remove_unsupported.store(true, Ordering::Relaxed);
                    warn!(
                        "[enforcement] peer-remove RPC not supported by Transmission daemon: {e}. \
                         Enforcement poller will go idle. Upgrade Transmission to 4.x+ to enable."
                    );
                    return;
                }
                Err(e) => {
                    warn!("[enforcement] peer-remove failed for {peer_addr}: {e}");
                }
            }
        }
    }

    if removal_count > 0 {
        info!("[enforcement] cycle complete — {removal_count} peer(s) disconnected");
    }
}

//! Background TCP RTT prober for peer latency estimation.
//!
//! # Why TCP instead of ICMP
//!
//! Sandboxed macOS apps cannot send raw ICMP packets (`ping`). TCP connect
//! attempts to the peer's port are the lowest-overhead alternative available
//! without special entitlements.
//!
//! # Rate limiting
//!
//! - Maximum 5 concurrent probes (Tokio semaphore).
//! - Minimum interval between probes to the same peer: 60 seconds.
//! - Probes are opt-in (controlled by `PeeringSettings.probe_rtt_enabled`).
//!
//! # Privacy
//!
//! The prober makes outbound TCP SYN connections to peer IPs on their
//! BitTorrent port — the same port used by the torrent engine. This does not
//! reveal anything not already implied by the active peer connection. The
//! privacy notice in the Settings modal explains this.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Maximum simultaneous outbound TCP probe connections.
const MAX_CONCURRENT_PROBES: usize = 5;

/// Minimum interval between probes to the same peer IP:port.
const MIN_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// TCP connect timeout per probe.
const PROBE_TIMEOUT: Duration = Duration::from_millis(2000);

/// Stores the most recently measured RTT (ms) for each `<ip>:<port>`.
pub type RttCache = Arc<Mutex<HashMap<String, u32>>>;

/// State tracking when each peer was last probed.
type LastProbed = Arc<Mutex<HashMap<String, Instant>>>;

/// Handle returned by `spawn()`. Clone cheaply to read RTT values.
#[derive(Clone)]
pub struct RttProber {
    cache: RttCache,
    last_probed: LastProbed,
    semaphore: Arc<Semaphore>,
}

impl RttProber {
    /// Create a new prober. Call `probe()` to trigger individual measurements.
    pub fn new() -> Self {
        RttProber {
            cache: Arc::new(Mutex::new(HashMap::new())),
            last_probed: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_PROBES)),
        }
    }

    /// Look up the most recent RTT measurement for `ip:port`, or `None` if
    /// no measurement exists yet.
    pub fn get_rtt(&self, ip: &str, port: u16) -> Option<u32> {
        let key = format!("{ip}:{port}");
        self.cache.lock().ok()?.get(&key).copied()
    }

    /// Schedule an async RTT probe to `ip:port`. Returns immediately (fire-and-forget).
    ///
    /// The probe is rate-limited: if the same `ip:port` was probed within
    /// `MIN_PROBE_INTERVAL`, this is a no-op.
    ///
    /// Does nothing if `ip` is a private/local address (LAN peers have trivially
    /// low RTT and do not need probing).
    pub fn probe(&self, ip: &str, port: u16) {
        if crate::utils::geo::is_local_ip(ip) {
            return; // LAN peers need no probe
        }

        let key = format!("{ip}:{port}");

        // Rate-limit check
        {
            let mut last = match self.last_probed.lock() {
                Ok(l) => l,
                Err(p) => p.into_inner(),
            };
            if let Some(t) = last.get(&key) {
                if t.elapsed() < MIN_PROBE_INTERVAL {
                    return; // too soon
                }
            }
            last.insert(key.clone(), Instant::now());
        }

        let addr_str = format!("{ip}:{port}");
        let cache = Arc::clone(&self.cache);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            let Ok(addr) = SocketAddr::from_str(&addr_str) else {
                return;
            };
            let Ok(_permit) = semaphore.acquire().await else {
                return;
            };

            let start = Instant::now();
            match timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await {
                Ok(Ok(_stream)) => {
                    let rtt_ms = start.elapsed().as_millis() as u32;
                    debug!("[rtt_prober] {addr_str} → {rtt_ms}ms");
                    if let Ok(mut c) = cache.lock() {
                        c.insert(addr_str, rtt_ms);
                    }
                }
                Ok(Err(e)) => {
                    // Connection refused / reset — peer is reachable but not
                    // on that port. RTT is still measurable from the ICMP RST timing.
                    // TCP RST arrives fast so the elapsed time is meaningful.
                    let rtt_ms = start.elapsed().as_millis() as u32;
                    if rtt_ms < PROBE_TIMEOUT.as_millis() as u32 {
                        debug!("[rtt_prober] {addr_str} RST in {rtt_ms}ms ({e})");
                        if let Ok(mut c) = cache.lock() {
                            c.insert(addr_str, rtt_ms);
                        }
                    }
                    // Otherwise the probe just timed out — no useful data
                }
                Err(_timeout) => {
                    warn!(
                        "[rtt_prober] {addr_str} timed out after {}ms",
                        PROBE_TIMEOUT.as_millis()
                    );
                    // Do not cache a timeout — leave the previous measurement intact
                }
            }
            // _permit dropped here, freeing the semaphore slot
        });
    }

    /// Evict stale entries from the cache for an IP that is no longer connected.
    /// Call when a peer disconnects or a torrent is removed.
    pub fn evict(&self, ip: &str) {
        let ip_prefix = format!("{ip}:");
        if let Ok(mut c) = self.cache.lock() {
            c.retain(|k, _| !k.starts_with(&ip_prefix));
        }
        if let Ok(mut l) = self.last_probed.lock() {
            l.retain(|k, _| !k.starts_with(&ip_prefix));
        }
    }
}

impl Default for RttProber {
    fn default() -> Self {
        Self::new()
    }
}

/// Check that an IPv4 address parses correctly (used in unit tests).
pub fn parse_ip(ip: &str) -> Option<IpAddr> {
    IpAddr::from_str(ip).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtt_prober_local_ip_skipped() {
        let prober = RttProber::new();
        // Probing a LAN address should be a no-op (no entry in last_probed)
        prober.probe("192.168.1.50", 41413);
        assert!(prober.last_probed.lock().unwrap().is_empty());
    }

    #[test]
    fn test_rtt_prober_rate_limit() {
        let prober = RttProber::new();
        // Insert a very-recent probe timestamp to trigger the rate-limit guard
        prober
            .last_probed
            .lock()
            .unwrap()
            .insert("1.2.3.4:6881".to_string(), Instant::now());
        // Second probe to same address should not update last_probed timestamp
        let before = *prober
            .last_probed
            .lock()
            .unwrap()
            .get("1.2.3.4:6881")
            .unwrap();
        prober.probe("1.2.3.4", 6881);
        let after = prober
            .last_probed
            .lock()
            .unwrap()
            .get("1.2.3.4:6881")
            .copied()
            .unwrap();
        // Timestamp must be identical — the rate-limit guard must not have
        // updated `last_probed` for this address.
        assert_eq!(before, after);
    }
}

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
//! - Probes are enabled by default and explicitly configurable via
//!   `PeeringSettings.probe_rtt_enabled`.
//!
//! # Privacy
//!
//! The prober makes outbound TCP connects to peer IPs on their BitTorrent
//! port. It never queues work while all probe slots are busy, and callers
//! disable it when a configured proxy/VPN route cannot be honored.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
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
/// Bound cached peer keys even when a daemon reports heavy peer churn.
const MAX_TRACKED_PEERS: usize = 4096;

/// Minimum interval between probes to the same peer IP:port.
const MIN_PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// TCP connect timeout per probe.
const PROBE_TIMEOUT: Duration = Duration::from_millis(2000);
/// Expire measurements and rate-limit entries that have gone quiet.
const PEER_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

/// Stores the most recently measured RTT (ms) for each `<ip>:<port>`.
type RttCache = Arc<Mutex<HashMap<String, RttMeasurement>>>;

#[derive(Clone, Copy)]
struct RttMeasurement {
    milliseconds: u32,
    measured_at: Instant,
}

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
        let key = peer_socket_addr(ip, port)?.to_string();
        let cache = self.cache.lock().ok()?;
        let measurement = cache.get(&key)?;
        (measurement.measured_at.elapsed() <= PEER_CACHE_TTL).then_some(measurement.milliseconds)
    }

    /// Schedule an async RTT probe to `ip:port`. Returns immediately (fire-and-forget).
    ///
    /// The probe is rate-limited: if the same `ip:port` was probed within
    /// `MIN_PROBE_INTERVAL`, this is a no-op.
    ///
    /// Does nothing if `ip` is a private/local address (LAN peers have trivially
    /// low RTT and do not need probing).
    pub fn probe(&self, ip: &str, port: u16) {
        let Some(addr) = peer_socket_addr(ip, port) else {
            return;
        };
        if !is_probeable_peer_ip(addr.ip()) {
            return; // Local, private, and shared-space peers need no probe
        }

        // Acquire before spawning so there can be at most MAX_CONCURRENT_PROBES
        // tasks, rather than an unbounded queue waiting on the semaphore.
        let Ok(permit) = Arc::clone(&self.semaphore).try_acquire_owned() else {
            return;
        };
        let key = addr.to_string();
        let now = Instant::now();

        {
            let mut last = match self.last_probed.lock() {
                Ok(l) => l,
                Err(p) => p.into_inner(),
            };
            last.retain(|_, timestamp| timestamp.elapsed() <= PEER_CACHE_TTL);
            if last
                .get(&key)
                .is_some_and(|timestamp| timestamp.elapsed() < MIN_PROBE_INTERVAL)
            {
                return;
            }
            last.remove(&key);
            while last.len() >= MAX_TRACKED_PEERS {
                let oldest = last
                    .iter()
                    .min_by_key(|(_, timestamp)| **timestamp)
                    .map(|(peer, _)| peer.clone());
                if let Some(oldest) = oldest {
                    last.remove(&oldest);
                } else {
                    break;
                }
            }
            last.insert(key.clone(), now);

            let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            cache.retain(|peer, measurement| {
                last.contains_key(peer) && measurement.measured_at.elapsed() <= PEER_CACHE_TTL
            });
        }

        let addr_str = key.clone();
        let cache = Arc::clone(&self.cache);
        let last_probed = Arc::clone(&self.last_probed);

        tokio::spawn(async move {
            let _permit = permit;

            let start = Instant::now();
            match timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await {
                Ok(Ok(_stream)) => {
                    let rtt_ms = start.elapsed().as_millis() as u32;
                    debug!("[rtt_prober] {addr_str} → {rtt_ms}ms");
                    cache_measurement(&cache, &last_probed, &addr_str, rtt_ms);
                }
                Ok(Err(e)) => {
                    // Connection refused / reset — peer is reachable but not
                    // on that port. RTT is still measurable from the ICMP RST timing.
                    // TCP RST arrives fast so the elapsed time is meaningful.
                    let rtt_ms = start.elapsed().as_millis() as u32;
                    if rtt_ms < PROBE_TIMEOUT.as_millis() as u32 {
                        debug!("[rtt_prober] {addr_str} RST in {rtt_ms}ms ({e})");
                        cache_measurement(&cache, &last_probed, &addr_str, rtt_ms);
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
}

fn peer_socket_addr(ip: &str, port: u16) -> Option<SocketAddr> {
    Some(SocketAddr::new(IpAddr::from_str(ip).ok()?, port))
}

fn is_probeable_peer_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(address) => {
            let octets = address.octets();
            let carrier_shared = octets[0] == 100 && (64..=127).contains(&octets[1]);
            let protocol_assignment = octets[0] == 192 && octets[1] == 0 && octets[2] == 0;
            let documentation = (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113);
            let benchmarking = octets[0] == 198 && (18..=19).contains(&octets[1]);
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_broadcast()
                && !address.is_unspecified()
                && !address.is_multicast()
                && !carrier_shared
                && !protocol_assignment
                && !documentation
                && !benchmarking
                && octets[0] != 0
                && octets[0] < 240
        }
        IpAddr::V6(address) => {
            let octets = address.octets();
            let ipv4_mapped = octets[..10] == [0; 10] && octets[10..12] == [0xff, 0xff];
            if ipv4_mapped {
                let mapped = Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
                return is_probeable_peer_ip(IpAddr::V4(mapped));
            }

            let link_local = octets[0] == 0xfe && octets[1] & 0xc0 == 0x80;
            let unique_local = octets[0] & 0xfe == 0xfc;
            let documentation =
                (octets[0] == 0x20 && octets[1] == 0x01 && octets[2] == 0x0d && octets[3] == 0xb8)
                    || (octets[0] == 0x3f && octets[1] == 0xff && octets[2] & 0xf0 == 0);
            !address.is_loopback()
                && !address.is_unspecified()
                && !address.is_multicast()
                && !link_local
                && !unique_local
                && !documentation
        }
    }
}

fn cache_measurement(cache: &RttCache, last_probed: &LastProbed, key: &str, milliseconds: u32) {
    let retained = last_probed
        .lock()
        .map(|last| last.contains_key(key))
        .unwrap_or(false);
    if retained {
        cache.lock().unwrap_or_else(|p| p.into_inner()).insert(
            key.to_string(),
            RttMeasurement {
                milliseconds,
                measured_at: Instant::now(),
            },
        );
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

    #[test]
    fn test_rtt_prober_skips_non_public_peer_ranges() {
        for ip in [
            "10.1.2.3",
            "172.20.1.2",
            "192.168.1.2",
            "169.254.1.2",
            "100.64.1.2",
            "0.1.2.3",
            "192.0.2.1",
            "198.18.0.1",
            "203.0.113.17",
            "240.0.0.1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "3fff::1",
            "::ffff:192.168.1.2",
            "::ffff:203.0.113.17",
        ] {
            assert!(
                !is_probeable_peer_ip(IpAddr::from_str(ip).unwrap()),
                "must not probe {ip}"
            );
        }
        for ip in ["8.8.8.8", "2001:4860:4860::8888"] {
            assert!(
                is_probeable_peer_ip(IpAddr::from_str(ip).unwrap()),
                "public peer should remain probeable: {ip}"
            );
        }
    }
}

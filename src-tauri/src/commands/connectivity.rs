use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::connectivity::{ConnectivityReport, NatType, PortStatus};
use crate::utils::network::AdcLink;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

async fn probe_upnp() -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(1500), async {
        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(_) => return false,
        };
        let msg = b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";
        if socket.send_to(msg, "239.255.255.250:1900").await.is_err() {
            return false;
        }
        let mut buf = [0u8; 1024];
        socket.recv_from(&mut buf).await.is_ok()
    })
    .await
    .unwrap_or(false)
}

async fn probe_nat_pmp() -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(1500), async {
        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(_) => return false,
        };
        let msg = [0u8, 0u8];
        for gw in &["192.168.1.1:5351", "192.168.0.1:5351", "10.0.0.1:5351"] {
            let _ = socket.send_to(&msg, gw).await;
        }
        let mut buf = [0u8; 32];
        socket.recv_from(&mut buf).await.is_ok()
    })
    .await
    .unwrap_or(false)
}

async fn probe_ipv4() -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(1500), async {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok()?;
        socket.connect("1.1.1.1:53").await.ok()?;
        Some(true)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

async fn probe_ipv6() -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(1500), async {
        let socket = tokio::net::UdpSocket::bind("[::]:0").await.ok()?;
        socket.connect("[2606:4700:4700::1111]:53").await.ok()?;
        Some(true)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}

async fn probe_dns_latency() -> u32 {
    let start = std::time::Instant::now();
    let lookup = tokio::time::timeout(
        std::time::Duration::from_millis(3000),
        tokio::net::lookup_host("one.one.one.one:53"),
    )
    .await;
    match lookup {
        Ok(Ok(_)) => start.elapsed().as_millis().max(1) as u32,
        _ => 999,
    }
}

async fn probe_tracker_reachability() -> bool {
    tokio::time::timeout(std::time::Duration::from_millis(2500), async {
        tokio::net::TcpStream::connect("asiandvdclub.org:443")
            .await
            .is_ok()
            || tokio::net::TcpStream::connect("tracker.asiandvdclub.org:443")
                .await
                .is_ok()
            || tokio::net::TcpStream::connect("tracker.asiandvdclub.org:80")
                .await
                .is_ok()
            || tokio::net::TcpStream::connect("asiandvdclub.org:80")
                .await
                .is_ok()
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
pub async fn run_connectivity_doctor_test(
    db: State<'_, Arc<Database>>,
    engine: State<'_, SharedEngine>,
) -> Result<ConnectivityReport, String> {
    let listening_port = db
        .get_setting("user_settings_json")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<crate::commands::settings::UserSettings>(&s).ok())
        .map(|s| s.listening_port)
        .unwrap_or(41413);
    // The stored settings may predate the 41413 port migration; probe and
    // report the port the engine actually uses, never the stale stock one.
    let listening_port = if listening_port == 51413 {
        41413
    } else {
        listening_port
    };

    // Run probes concurrently
    let (upnp, nat_pmp, ipv4, ipv6, dns_ms, tracker_ok) = tokio::join!(
        probe_upnp(),
        probe_nat_pmp(),
        probe_ipv4(),
        probe_ipv6(),
        probe_dns_latency(),
        probe_tracker_reachability()
    );

    let status = engine.engine_status();
    let vpn_bound = status.bound_interface.is_some();
    let vpn_adapter = status.bound_interface.clone();

    // Determine port status: check if local listener is accepting or port in use
    let local_bind_test = std::net::TcpListener::bind(("0.0.0.0", listening_port));
    let port_listening = match local_bind_test {
        Ok(_) => false, // Not bound by active engine
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => true,
        Err(_) => false,
    };

    let active_torrents = engine.list_torrents().unwrap_or_default();
    let total_inbound_peers: u32 = active_torrents.iter().map(|t| t.peers).sum();
    let total_outbound_peers: u32 = active_torrents.iter().filter_map(|t| t.seeds).sum();

    let port_open_from_engine = engine.check_port_open();
    let port_status = if let Some(open) = port_open_from_engine {
        if open {
            PortStatus::Open
        } else {
            PortStatus::Closed
        }
    } else if port_listening || total_inbound_peers > 0 {
        PortStatus::Open
    } else {
        PortStatus::Closed
    };

    // Record which signal produced the verdict — the three signals measure
    // very different things and blending them produced misleading "Open"
    // reports (see port_open_evidence docs on the report struct).
    let port_open_evidence = if port_open_from_engine.is_some() {
        Some("transmission-self-test".to_string())
    } else if total_inbound_peers > 0 {
        Some("inbound-peers".to_string())
    } else if port_listening {
        Some("local-listener".to_string())
    } else {
        None
    };

    let nat_type = if upnp || nat_pmp {
        NatType::FullCone
    } else {
        NatType::Unknown
    };

    let certificate_text = ConnectivityReport::generate_certificate(
        listening_port,
        port_status,
        ipv6,
        tracker_ok,
        vpn_bound,
        vpn_adapter.as_deref(),
    );

    let mut plain_language_findings = Vec::new();
    if port_status == PortStatus::Open {
        // Be explicit about WHAT the verdict is based on — a transmission
        // self-test or connected peers is NOT proof that new external peers
        // can reach the port right now.
        match port_open_evidence.as_deref() {
            Some("transmission-self-test") => plain_language_findings.push(format!(
                "Port {listening_port}: the engine's self-test passed, but this test connects from your own machine and can pass even when the internet cannot reach the port (no NAT loopback). Treat it as unverified."
            )),
            Some("inbound-peers") => plain_language_findings.push(format!(
                "Port {listening_port}: {total_inbound_peers} peers are connected, but they may have connected earlier or over IPv6; this does not prove new inbound IPv4 connections work."
            )),
            Some("local-listener") => plain_language_findings.push(format!(
                "Port {listening_port} is bound locally, but nothing has verified that the internet can reach it."
            )),
            _ => plain_language_findings.push(format!(
                "Incoming BitTorrent port {listening_port} is open and active."
            )),
        }
    } else {
        plain_language_findings.push(format!(
            "Port {listening_port} is not currently open for incoming connections."
        ));
    }

    if upnp {
        plain_language_findings
            .push("UPnP automatic port mapping is active on your router.".to_string());
    } else if nat_pmp {
        plain_language_findings
            .push("NAT-PMP port mapping is supported on your router.".to_string());
    } else {
        plain_language_findings.push(
            "Automatic port mapping (UPnP/NAT-PMP) was not detected on your gateway.".to_string(),
        );
    }

    if ipv6 {
        plain_language_findings
            .push("Native IPv6 dual-stack connectivity is active and ready.".to_string());
    } else {
        plain_language_findings.push("Operating in IPv4-only mode; IPv6 not detected.".to_string());
    }

    plain_language_findings.push(format!("DNS resolution latency measured at {dns_ms}ms."));

    if tracker_ok {
        plain_language_findings.push("Tracker host is reachable.".to_string());
    } else {
        plain_language_findings
            .push("Tracker endpoint did not respond to connection probe.".to_string());
    }

    let mut suggested_fixes = Vec::new();
    if port_status != PortStatus::Open && !upnp && !nat_pmp {
        suggested_fixes.push(format!(
            "Enable UPnP in your router settings or configure a port forwarding rule for TCP/UDP port {listening_port}."
        ));
    } else {
        suggested_fixes.push("Network connectivity is configured correctly.".to_string());
    }

    Ok(ConnectivityReport {
        listening_port,
        port_status,
        upnp_available: upnp,
        nat_pmp_available: nat_pmp,
        pcp_available: false,
        nat_type,
        ipv4_healthy: ipv4,
        ipv6_healthy: ipv6,
        dns_health_ms: dns_ms,
        vpn_interface_bound: vpn_bound,
        vpn_adapter_name: vpn_adapter,
        tracker_reachable: tracker_ok,
        tracker_auth_ok: tracker_ok,
        inbound_connections: total_inbound_peers,
        outbound_connections: total_outbound_peers,
        port_open_evidence,
        certificate_text,
        plain_language_findings,
        suggested_fixes,
    })
}

/// The port negotiator's latest verdict, for the front end's port-forward
/// warning dialog. Written by `negotiate_reachable_port` at engine spawn.
#[derive(Debug, Clone, Serialize)]
pub struct PortNegotiationResult {
    pub status: String,
    pub configured_port: u16,
    pub tried_ports: Vec<u16>,
    pub checked_at_epoch_ms: u64,
}

#[tauri::command]
pub fn get_port_negotiation_result(
    db: State<'_, Arc<Database>>,
) -> Result<Option<PortNegotiationResult>, String> {
    let raw = db.get_setting("port_negotiation_result").ok().flatten();
    let Some(raw) = raw else { return Ok(None) };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => Ok(Some(PortNegotiationResult {
            status: v
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string(),
            configured_port: v
                .get("configured_port")
                .and_then(|p| p.as_u64())
                .unwrap_or(0) as u16,
            tried_ports: v
                .get("tried_ports")
                .and_then(|p| p.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_u64().map(|n| n as u16))
                        .collect()
                })
                .unwrap_or_default(),
            checked_at_epoch_ms: v
                .get("checked_at_epoch_ms")
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
        })),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub fn retest_listening_port(port: u16) -> Result<bool, String> {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                Ok(true)
            } else {
                Err(format!("Port test failed: {e}"))
            }
        }
    }
}

#[tauri::command]
pub async fn get_connectivity_certificate(
    db: State<'_, Arc<Database>>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let report = run_connectivity_doctor_test(db, engine).await?;
    Ok(report.certificate_text)
}

/// Live Tailscale (Secure Network) detection for the status bar: reports the
/// tailnet interface and member IP when the VPN data plane is up, plus a
/// live probe of the ADC server over the tailnet. `adc_link` says how far
/// the probe got: a host that answered but does not accept :443 on the probed
/// address is not the same as one that did not answer at all, no outcome
/// claims the host is offline, and neither the TCP result nor either failure
/// says anything about HTTPS health. When the path to ADC is not confirmed
/// the user must be told, since traffic then falls back to the default
/// connection (or the Nebula VPN).
#[derive(Debug, Clone, Serialize)]
pub struct TailscaleStatus {
    pub connected: bool,
    pub interface: Option<String>,
    pub tailnet_ip: Option<String>,
    /// `Some(true)` only when a TCP connection to the ADC host's :443 was
    /// accepted; `Some(false)` for both other `adc_link` outcomes.
    pub adc_reachable: Option<bool>,
    /// Outcome of the live probe; `None` while it could not be run.
    pub adc_link: Option<AdcLink>,
    pub adc_node: Option<String>,
}

/// Freshness window for a healthy ADC probe result: trusted for this long,
/// then the next status poll re-probes. Every write is the latest probe's
/// result, keyed by the probed address so a route/peer change invalidates
/// the cache.
const ADC_PROBE_FRESH_SECS: u64 = 60;
/// Window for anything but a healthy result. It stays under the 15 s status
/// poll interval so a host or route that comes back shows on the next poll
/// without restarting the app, yet a failure still replaces a previous
/// healthy indication as soon as a probe sees it.
const ADC_PROBE_RETRY_SECS: u64 = 10;

type AdcProbeCache = std::sync::Mutex<Option<(String, AdcLink, std::time::Instant)>>;

static ADC_PROBE_CACHE: AdcProbeCache = std::sync::Mutex::new(None);

fn adc_probe_is_fresh(link: AdcLink, age: std::time::Duration) -> bool {
    let window = if link.tcp_open() {
        ADC_PROBE_FRESH_SECS
    } else {
        ADC_PROBE_RETRY_SECS
    };
    age.as_secs() < window
}

/// Cache and bookkeeping around one live probe. `probe` runs only on a cache
/// miss; when it cannot say anything (`None`) the cache is emptied so no
/// stale indication survives.
fn live_adc_probe_with(
    cache: &AdcProbeCache,
    target: Option<&str>,
    node: Option<String>,
    probe: impl FnOnce(&str) -> Option<AdcLink>,
) -> (Option<String>, Option<AdcLink>) {
    let Some(ip) = target else {
        // No ADC node resolvable at all: do not serve a stale "reachable".
        if let Ok(mut c) = cache.lock() {
            *c = None;
        }
        return (node, None);
    };
    if let Ok(c) = cache.lock() {
        if let Some((cip, link, at)) = c.as_ref() {
            if cip == ip && adc_probe_is_fresh(*link, at.elapsed()) {
                return (node.or_else(|| Some(cip.clone())), Some(*link));
            }
        }
    }
    let link = probe(ip);
    if let Ok(mut c) = cache.lock() {
        *c = link.map(|l| (ip.to_string(), l, std::time::Instant::now()));
    }
    (node, link)
}

/// Bounded live check of the ADC node over the tailnet, using the EMBEDDED
/// daemon's own peer/route data (no system `tailscale` CLI — embedded-only
/// installs do not have one on PATH). A TCP connect to `target`:443 through
/// the daemon's proxy, and when that fails a tailnet ping tells a host that
/// answers from one that does not; TLS is validated by the actual HTTPS
/// clients that use the route (the doctor), never faked here. Returns
/// `(node, Some(link))` when a node was resolvable, `(node, None)` when it
/// could not be resolved at all or the daemon's proxy was unusable, and
/// falls back to the last cached result while it is still fresh.
fn live_adc_probe(target: Option<&str>, node: Option<String>) -> (Option<String>, Option<AdcLink>) {
    let node_ip = node.clone();
    live_adc_probe_with(&ADC_PROBE_CACHE, target, node, |ip| {
        crate::tailscale::probe_adc_link(ip, node_ip.as_deref())
    })
}

/// Truthful tailscale status for the status bar: the EMBEDDED daemon's
/// state first (userspace, no TUN — "interface" is descriptive); the legacy
/// system-TUN detection is only a fallback for an embedded daemon that has
/// not started.
#[tauri::command]
pub async fn get_tailscale_status() -> Result<TailscaleStatus, String> {
    let ts = crate::tailscale::status();
    // The app ALWAYS runs the embedded userspace daemon, so any non-empty
    // backend state means THIS app owns a daemon and its state is the
    // truth. Report it directly — a separate SYSTEM Tailscale (observed
    // live on macOS: Tailscale.app + network-extension + utun4 with a
    // 100.x address) must never leak into the app's status. The old
    // legacy fallback could report the SYSTEM's interface as the app's
    // "Tailnet connected" while the embedded daemon was logged out
    // (mixed-stack defect, 2026-10-20).
    if !ts.backend_state.is_empty() {
        let running = ts.backend_state == "Running";
        let (adc_node, adc_link) = if running {
            // Owner defect (2026-10-20): a Running daemon with a CACHED
            // route entry reported ADC as reachable while the ADC Linux
            // host was offline. Reachability must be a live bounded probe
            // with a freshness window, never a cached netmap entry.
            // Furthermore, private verification must NOT accept public
            // fallback: only target the peer's advertised subnet route
            // when routes_accepted is true. Without accepted routes,
            // fall back to probing the CGNAT tailnet IP (which honestly fails).
            let target = if ts.routes_accepted {
                ts.adc_route_ip.clone().or_else(|| ts.adc_node.clone())
            } else {
                ts.adc_node.clone()
            };
            // The probe can block for several seconds (proxy connect, then a
            // tailnet ping): keep it off the async workers.
            let node = ts.adc_node;
            tokio::task::spawn_blocking(move || live_adc_probe(target.as_deref(), node))
                .await
                .map_err(|e| format!("join: {e}"))?
        } else {
            // Not running: clear any cached probe result so a stale
            // healthy indication cannot survive a daemon stop/logout.
            if let Ok(mut c) = ADC_PROBE_CACHE.lock() {
                *c = None;
            }
            (ts.adc_node, None)
        };
        return Ok(TailscaleStatus {
            connected: running,
            interface: running.then(|| "nebulae-embedded (userspace)".to_string()),
            tailnet_ip: running.then_some(ts.tailnet_ip).flatten(),
            adc_reachable: adc_link.map(AdcLink::tcp_open),
            adc_link,
            adc_node,
        });
    }

    // The embedded daemon has never been probed in this process (backend
    // state empty): fall back to system-TUN detection for legacy installs.
    tokio::task::spawn_blocking(|| {
        if crate::utils::network::detect_tailscale().is_none() {
            // Stopped at login (observed after network-extension updates):
            // one bounded auto-heal attempt, then re-detect. Uses the
            // EMBEDDED tailscale CLI, never a system one on PATH.
            if let Ok(paths) = crate::tailscale::Paths::resolve() {
                if let Some(heal) =
                    crate::utils::network::heal_stopped_tailscale(&paths.tailscale())
                {
                    tracing::info!("[tailscale] {heal}");
                }
            }
        }
        Ok(match crate::utils::network::detect_tailscale() {
            Some((interface, tailnet_ip)) => {
                let probe = crate::utils::network::probe_adc_over_tailnet();
                TailscaleStatus {
                    connected: true,
                    interface: Some(interface),
                    tailnet_ip: Some(tailnet_ip),
                    adc_reachable: probe.as_ref().map(|(_, l)| l.tcp_open()),
                    adc_link: probe.as_ref().map(|(_, l)| *l),
                    adc_node: probe.map(|(n, _)| n),
                }
            }
            None => TailscaleStatus {
                connected: false,
                interface: None,
                tailnet_ip: None,
                adc_reachable: None,
                adc_link: None,
                adc_node: None,
            },
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn empty_cache() -> AdcProbeCache {
        std::sync::Mutex::new(None)
    }

    #[test]
    fn healthy_result_is_trusted_for_a_minute_failures_for_less_than_a_poll() {
        assert!(adc_probe_is_fresh(
            AdcLink::Reachable,
            Duration::from_secs(59)
        ));
        assert!(!adc_probe_is_fresh(
            AdcLink::Reachable,
            Duration::from_secs(60)
        ));
        for link in [AdcLink::HostAnswered, AdcLink::Unavailable] {
            assert!(adc_probe_is_fresh(link, Duration::from_secs(9)), "{link:?}");
            assert!(
                !adc_probe_is_fresh(link, Duration::from_secs(10)),
                "{link:?} must be re-probed before the 15 s status poll"
            );
        }
    }

    #[test]
    fn unresolved_target_reports_nothing_and_drops_the_cache() {
        let cache = empty_cache();
        let (_, first) = live_adc_probe_with(&cache, Some("203.0.113.5"), None, |_| {
            Some(AdcLink::Reachable)
        });
        assert_eq!(first, Some(AdcLink::Reachable));
        let (node, link) = live_adc_probe_with(&cache, None, Some("100.64.0.9".into()), |_| {
            panic!("no target, no probe")
        });
        assert_eq!((node.as_deref(), link), (Some("100.64.0.9"), None));
        assert!(cache.lock().unwrap().is_none());
    }

    #[test]
    fn fresh_result_is_served_without_probing_again() {
        let cache = empty_cache();
        let probe_once = |link| {
            live_adc_probe_with(
                &cache,
                Some("203.0.113.5"),
                Some("100.64.0.9".into()),
                |_| Some(link),
            )
        };
        assert_eq!(
            probe_once(AdcLink::HostAnswered).1,
            Some(AdcLink::HostAnswered)
        );
        let (node, link) = live_adc_probe_with(
            &cache,
            Some("203.0.113.5"),
            Some("100.64.0.9".into()),
            |_| panic!("a fresh result must not be probed again"),
        );
        assert_eq!(
            (node.as_deref(), link),
            (Some("100.64.0.9"), Some(AdcLink::HostAnswered))
        );
    }

    #[test]
    fn a_different_target_is_probed_even_when_the_cache_is_fresh() {
        let cache = empty_cache();
        live_adc_probe_with(&cache, Some("203.0.113.5"), None, |_| {
            Some(AdcLink::Reachable)
        });
        let (_, link) = live_adc_probe_with(&cache, Some("100.64.0.9"), None, |_| {
            Some(AdcLink::Unavailable)
        });
        assert_eq!(link, Some(AdcLink::Unavailable));
        let cached = cache.lock().unwrap();
        assert_eq!(
            cached.as_ref().map(|(ip, l, _)| (ip.as_str(), *l)),
            Some(("100.64.0.9", AdcLink::Unavailable))
        );
    }

    /// Cache entry that was written `age` ago.
    fn aged_cache(ip: &str, link: AdcLink, age: Duration) -> Option<AdcProbeCache> {
        let at = std::time::Instant::now().checked_sub(age)?;
        Some(std::sync::Mutex::new(Some((ip.to_string(), link, at))))
    }

    #[test]
    fn recovery_after_a_failure_shows_on_the_next_status_poll() {
        // The status poll runs every 15 s. A failure older than the 10 s
        // retry window is probed again, so a host that came back (for
        // example after a reboot) is reported without restarting the app.
        for failed in [AdcLink::Unavailable, AdcLink::HostAnswered] {
            let Some(cache) = aged_cache("203.0.113.5", failed, Duration::from_secs(15)) else {
                return;
            };
            let (_, link) = live_adc_probe_with(&cache, Some("203.0.113.5"), None, |_| {
                Some(AdcLink::Reachable)
            });
            assert_eq!(link, Some(AdcLink::Reachable), "after {failed:?}");
            let cached = cache.lock().unwrap();
            assert_eq!(
                cached.as_ref().map(|(_, l, _)| *l),
                Some(AdcLink::Reachable)
            );
        }
    }

    #[test]
    fn a_healthy_result_replaced_by_a_failure_once_its_window_ends() {
        let Some(cache) = aged_cache("203.0.113.5", AdcLink::Reachable, Duration::from_secs(61))
        else {
            return;
        };
        let (_, link) = live_adc_probe_with(&cache, Some("203.0.113.5"), None, |_| {
            Some(AdcLink::Unavailable)
        });
        assert_eq!(link, Some(AdcLink::Unavailable));
    }

    #[test]
    fn a_healthy_result_is_trusted_inside_its_window_without_probing() {
        let Some(cache) = aged_cache("203.0.113.5", AdcLink::Reachable, Duration::from_secs(30))
        else {
            return;
        };
        let (_, link) = live_adc_probe_with(&cache, Some("203.0.113.5"), None, |_| {
            panic!("a healthy result inside its window must not be probed again")
        });
        assert_eq!(link, Some(AdcLink::Reachable));
    }

    #[test]
    fn an_unusable_proxy_is_unverified_and_keeps_nothing() {
        let cache = empty_cache();
        let (node, link) = live_adc_probe_with(
            &cache,
            Some("203.0.113.5"),
            Some("100.64.0.9".into()),
            |_| None,
        );
        assert_eq!((node.as_deref(), link), (Some("100.64.0.9"), None));
        assert!(cache.lock().unwrap().is_none());
    }

    #[test]
    fn status_payload_carries_the_three_way_result_and_the_tcp_flag() {
        let status = TailscaleStatus {
            connected: true,
            interface: None,
            tailnet_ip: None,
            adc_reachable: Some(AdcLink::HostAnswered.tcp_open()),
            adc_link: Some(AdcLink::HostAnswered),
            adc_node: Some("100.64.0.9".into()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["adc_link"], "host_answered");
        assert_eq!(json["adc_reachable"], false);
    }
}

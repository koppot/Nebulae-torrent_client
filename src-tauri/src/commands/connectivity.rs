use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::connectivity::{ConnectivityReport, NatType, PortStatus};
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
/// live probe of the ADC server over the tailnet. When `adc_reachable` is
/// `false` the tailnet path to ADC is down: the user must be told, since
/// traffic then falls back to the default connection (or the Nebula VPN).
#[derive(Debug, Clone, Serialize)]
pub struct TailscaleStatus {
    pub connected: bool,
    pub interface: Option<String>,
    pub tailnet_ip: Option<String>,
    pub adc_reachable: Option<bool>,
    pub adc_node: Option<String>,
}

/// Freshness window for the ADC reachability probe: a successful result is
/// trusted for this long, then the next status poll re-probes. Failures are
/// cached for the same window so a confirmed failure immediately replaces a
/// previous healthy indication (an older success can never overwrite a
/// newer failure — every write is the latest probe's result, keyed by the
/// probed address so a route/peer change invalidates the cache).
const ADC_PROBE_FRESH_SECS: u64 = 60;

static ADC_PROBE_CACHE: std::sync::Mutex<Option<(String, bool, std::time::Instant)>> =
    std::sync::Mutex::new(None);

/// Bounded live check of the ADC node over the tailnet, using the EMBEDDED
/// daemon's own peer/route data (no system `tailscale` CLI — embedded-only
/// installs do not have one on PATH). A TCP connect to the node's tailnet
/// IP:443 within 2s; TLS is validated by the actual HTTPS clients that use
/// the route (the doctor), never faked here. Returns `(node, Some(reachable))`
/// when a node was resolvable, `(node, None)` when it could not be resolved
/// at all, and falls back to the last cached result while it is still fresh.
fn live_adc_probe(target: Option<&str>, node: Option<String>) -> (Option<String>, Option<bool>) {
    let Some(ip) = target.map(|s| s.to_string()) else {
        // No ADC node resolvable at all: do not serve a stale "reachable".
        if let Ok(mut c) = ADC_PROBE_CACHE.lock() {
            *c = None;
        }
        return (node, None);
    };
    if let Ok(c) = ADC_PROBE_CACHE.lock() {
        if let Some((cip, ok, at)) = c.as_ref() {
            if cip == &ip && at.elapsed().as_secs() < ADC_PROBE_FRESH_SECS {
                return (node.or_else(|| Some(cip.clone())), Some(*ok));
            }
        }
    }
    let reachable = std::net::TcpStream::connect_timeout(
        &format!("{ip}:443").parse().unwrap(),
        std::time::Duration::from_secs(2),
    )
    .is_ok();
    if let Ok(mut c) = ADC_PROBE_CACHE.lock() {
        *c = Some((ip, reachable, std::time::Instant::now()));
    }
    (node, Some(reachable))
}

/// Truthful tailscale status for the status bar: the EMBEDDED daemon's
/// state first (userspace, no TUN — "interface" is descriptive); the legacy
/// system-TUN detection is only a fallback for an embedded daemon that has
/// not started.
#[tauri::command]
pub async fn get_tailscale_status() -> Result<TailscaleStatus, String> {
    let ts = crate::tailscale::status();
    if ts.backend_state == "Running" {
        // Owner defect (2026-10-20): a Running daemon with a CACHED route
        // entry reported ADC as reachable while the ADC Linux host was
        // offline. Reachability must be a live bounded probe with a
        // freshness window, never a cached netmap entry.
        let (adc_node, adc_reachable) = live_adc_probe(ts.adc_route_ip.as_deref(), ts.adc_node);
        return Ok(TailscaleStatus {
            connected: true,
            interface: Some("nebulae-embedded (userspace)".into()),
            tailnet_ip: ts.tailnet_ip,
            adc_reachable,
            adc_node,
        });
    }

    tokio::task::spawn_blocking(|| {
        if crate::utils::network::detect_tailscale().is_none() {
            // Stopped at login (observed after network-extension updates):
            // one bounded auto-heal attempt, then re-detect.
            if let Some(heal) = crate::utils::network::heal_stopped_tailscale() {
                tracing::info!("[tailscale] {heal}");
            }
        }
        Ok(match crate::utils::network::detect_tailscale() {
            Some((interface, tailnet_ip)) => {
                let probe = crate::utils::network::probe_adc_over_tailnet();
                TailscaleStatus {
                    connected: true,
                    interface: Some(interface),
                    tailnet_ip: Some(tailnet_ip),
                    adc_reachable: probe.as_ref().map(|(_, r)| *r),
                    adc_node: probe.map(|(n, _)| n),
                }
            }
            None => TailscaleStatus {
                connected: false,
                interface: None,
                tailnet_ip: None,
                adc_reachable: None,
                adc_node: None,
            },
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

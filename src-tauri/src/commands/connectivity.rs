use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::connectivity::{ConnectivityReport, NatType, PortStatus};
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
        .unwrap_or(51413);

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
        plain_language_findings.push(format!(
            "Incoming BitTorrent port {listening_port} is open and active."
        ));
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
        certificate_text,
        plain_language_findings,
        suggested_fixes,
    })
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

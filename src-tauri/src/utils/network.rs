use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ip_addresses: Vec<String>,
    pub is_up: bool,
    pub is_vpn: bool,
}

pub fn list_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    let interfaces = match NetworkInterface::show() {
        Ok(list) => list,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    for iface in interfaces {
        let is_vpn = is_vpn_interface_name(&iface.name);
        let ips: Vec<String> = iface.addr.iter().map(|a| a.ip().to_string()).collect();

        result.push(NetworkInterfaceInfo {
            name: iface.name,
            ip_addresses: ips,
            is_up: true,
            is_vpn,
        });
    }

    result
}

pub fn is_vpn_interface_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("utun")
        || lower.starts_with("tun")
        || lower.starts_with("tap")
        || lower.starts_with("wg")
        || lower.starts_with("wireguard")
        || lower.starts_with("ppp")
        || lower.starts_with("ipsec")
        || lower.contains("vpn")
        || lower.contains("wintun")
}

/// The local source address the OS actually uses for outbound internet
/// traffic: open a probe socket to a public anycast address and read its
/// local address. This is the honest "current address through which the
/// client accesses other servers" — it follows the default route and
/// switches to the VPN's tunnel address the moment a VPN claims egress.
pub fn egress_source_ip() -> Option<String> {
    let addr: std::net::SocketAddr = "1.1.1.1:53".parse().ok()?;
    let stream =
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(3)).ok()?;
    stream.local_addr().ok().map(|a| a.ip().to_string())
}

pub fn find_active_vpn_interface() -> Option<NetworkInterfaceInfo> {
    list_network_interfaces()
        .into_iter()
        .find(|iface| iface.is_vpn && !iface.ip_addresses.is_empty())
}

/// Proves real internet egress through the LOCAL address `ip`: one DNS A
/// query to 1.1.1.1:53, source-bound to that address, with a short reply
/// window. A mesh VPN (e.g. Tailscale without an exit node) accepts the
/// bind but cannot carry the packet out, which is exactly the failure mode
/// that stalled peer traffic — a route lookup alone cannot distinguish
/// that, so an actual reply is required.
fn has_egress_through(ip: &str) -> bool {
    use std::net::UdpSocket;
    use std::time::Duration;

    let socket = match UdpSocket::bind((ip, 0)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if socket
        .set_read_timeout(Some(Duration::from_millis(1500)))
        .is_err()
    {
        return false;
    }
    if socket.connect(("1.1.1.1", 53)).is_err() {
        return false;
    }
    // Minimal well-formed DNS query: example.com A, recursion desired.
    let query: Vec<u8> = vec![
        0x1a, 0x2b, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e', b'x',
        b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    if socket.send(&query).is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    socket.recv_from(&mut buf).is_ok()
}

/// Like `find_active_vpn_interface`, but only returns interfaces through
/// which internet traffic can ACTUALLY leave the machine. Binding the
/// torrent engine to a mesh-VPN interface without egress (Tailscale member
/// addresses in 100.64.0.0/10 with no exit node) makes every peer
/// connection impossible while tracker announces still appear to work —
/// the exact "torrent stalls with zero peers" signature this guards
/// against.
pub fn find_active_egress_vpn_interface() -> Option<NetworkInterfaceInfo> {
    list_network_interfaces()
        .into_iter()
        .filter(|iface| iface.is_vpn && iface.is_up)
        .find(|iface| {
            iface
                .ip_addresses
                .iter()
                .any(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok() && has_egress_through(ip))
        })
}

/// Tailscale assigns member addresses from the CGNAT block 100.64.0.0/10.
pub fn is_tailnet_ip(ip: &str) -> bool {
    let octets: Vec<&str> = ip.split('.').collect();
    octets.len() == 4
        && octets[0] == "100"
        && octets[1]
            .parse::<u8>()
            .map(|o| (64..=127).contains(&o))
            .unwrap_or(false)
}

/// Live probe of the ADC server over the tailnet.
///
/// The owner requirement: when the tailnet path to the ADC server is NOT
/// routed through, the user must be told — traffic then falls back to the
/// default connection (or the Nebula VPN when one is active).
///
/// Returns `Some((node_label, reachable))` when a tailnet member whose name
/// contains "asiandvdclub" is known to the local Tailscale client, with the
/// second element describing whether its HTTPS endpoint answered within the
/// bounded window. `None` means the Tailscale client could not be queried
/// at all.
pub fn probe_adc_over_tailnet() -> Option<(String, bool)> {
    use std::io::Read;
    use std::net::{SocketAddr, TcpStream};
    use std::process::Stdio;
    use std::time::Duration;

    // Resolve the CLI explicitly: the app ships its own tailscale binary;
    // relying on PATH fails under macOS GUI-app PATH constraints (the
    // bundled binary is NOT on the default GUI PATH). Also check standard
    // system install paths if bundled is absent.
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("tailscale").join("tailscale"));
        }
    }
    candidates.push(std::path::PathBuf::from("tailscale"));
    candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/tailscale"));
    candidates.push(std::path::PathBuf::from("/usr/local/bin/tailscale"));
    candidates.push(std::path::PathBuf::from(
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ));

    let mut child = candidates.iter().find_map(|bin| {
        crate::utils::process::create_hidden_command(bin.to_string_lossy().as_ref())
            .arg("status")
            .arg("--json")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    })?;

    // Bounded wait: kill a hung client instead of blocking the caller.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let output = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                break child
                    .stdout
                    .take()
                    .and_then(|mut out| {
                        let mut buf = Vec::new();
                        out.read_to_end(&mut buf).ok()?;
                        Some(buf)
                    })
                    .filter(|_| status.success())
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }?;
    let v: serde_json::Value = serde_json::from_slice(&output).ok()?;

    // Locate the ADC member: match hostname or DNS name containing
    // "asiandvdclub" (case-insensitive).
    let mut node_label: Option<String> = None;
    let mut node_ip: Option<String> = None;
    if let Some(peers) = v.get("Peer").and_then(|p| p.as_object()) {
        for peer in peers.values() {
            let dns = peer
                .get("DNSName")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .trim_end_matches('.')
                .to_lowercase();
            let host = peer
                .get("HostName")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_lowercase();
            if dns.contains("asiandvdclub") || host.contains("asiandvdclub") {
                node_label = Some(
                    peer.get("HostName")
                        .and_then(|h| h.as_str())
                        .unwrap_or("asiandvdclub")
                        .to_string(),
                );
                node_ip = peer
                    .get("TailscaleIPs")
                    .and_then(|ips| ips.as_array())
                    .and_then(|a| a.first())
                    .and_then(|ip| ip.as_str())
                    .map(|s| s.to_string());
                break;
            }
        }
    }
    let (node_label, node_ip) = (node_label?, node_ip?);

    let addr: SocketAddr = format!("{node_ip}:443").parse().ok()?;
    let reachable = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok();
    Some((node_label, reachable))
}

/// Detects a running Tailscale client: a VPN-type interface carrying an
/// address inside the tailnet CGNAT range. Returns (interface, tailnet IP).
pub fn detect_tailscale() -> Option<(String, String)> {
    list_network_interfaces()
        .into_iter()
        .filter(|iface| iface.is_vpn && iface.is_up)
        .find_map(|iface| {
            iface
                .ip_addresses
                .iter()
                .find(|ip| is_tailnet_ip(ip))
                .map(|ip| (iface.name.clone(), ip.clone()))
        })
}

/// One-shot-per-process auto-heal for Tailscale sitting in the "Stopped"
/// state at login (observed after the app's network-extension auto-update:
/// the login auto-connect path is interrupted and nothing retries the
/// connection). When the backend is Stopped — NOT NeedsLogin, which would
/// open an interactive browser auth — issue a single `tailscale up`. The
/// persisted wants-running flag then makes normal logins auto-connect.
pub fn heal_stopped_tailscale() -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static HEALED_THIS_SESSION: AtomicBool = AtomicBool::new(false);
    if HEALED_THIS_SESSION.swap(true, Ordering::SeqCst) {
        return None; // one attempt per process; never flap the VPN
    }

    let output = crate::utils::process::create_hidden_command("tailscale")
        .arg("status")
        .arg("--json")
        .output()
        .ok()?;
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let backend = parsed
        .get("BackendState")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if backend != "Stopped" {
        return None; // Running / NeedsLogin / Starting — leave it alone
    }

    tracing::warn!("[tailscale] backend Stopped at startup — issuing one `tailscale up` auto-heal");
    let up = crate::utils::process::create_hidden_command("tailscale")
        .arg("up")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if up.success() {
        Some("healed: tailscale up".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_vpn_interface_name() {
        assert!(is_vpn_interface_name("utun3"));
        assert!(is_vpn_interface_name("tun0"));
        assert!(is_vpn_interface_name("wg0"));
        assert!(is_vpn_interface_name("WireGuard"));
        assert!(is_vpn_interface_name("tap_adapter"));
        assert!(!is_vpn_interface_name("en0"));
        assert!(!is_vpn_interface_name("eth0"));
        assert!(!is_vpn_interface_name("lo0"));
        assert!(!is_vpn_interface_name("wlan0"));
    }

    #[test]
    fn test_is_tailnet_ip() {
        // 100.64.0.0/10 -> second octet 64..=127
        assert!(is_tailnet_ip("100.64.0.1"));
        assert!(is_tailnet_ip("100.101.112.13"));
        assert!(is_tailnet_ip("100.127.255.254"));
        assert!(!is_tailnet_ip("100.63.255.254")); // below CGNAT block
        assert!(!is_tailnet_ip("100.128.0.1")); // above CGNAT block
        assert!(!is_tailnet_ip("192.168.1.50")); // LAN
        assert!(!is_tailnet_ip("100.64.0.1.5")); // malformed
    }

    #[test]
    fn test_list_interfaces_runs() {
        let interfaces = list_network_interfaces();
        // Should execute without panicking on any platform
        assert!(!interfaces.is_empty());
    }
}

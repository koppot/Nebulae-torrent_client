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

pub fn find_active_vpn_interface() -> Option<NetworkInterfaceInfo> {
    list_network_interfaces()
        .into_iter()
        .find(|iface| iface.is_vpn && !iface.ip_addresses.is_empty())
}

/// Tailscale assigns member addresses from the CGNAT block 100.64.0.0/10.
fn is_tailnet_ip(ip: &str) -> bool {
    let octets: Vec<&str> = ip.split('.').collect();
    octets.len() == 4
        && octets[0] == "100"
        && octets[1]
            .parse::<u8>()
            .map(|o| (64..=127).contains(&o))
            .unwrap_or(false)
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

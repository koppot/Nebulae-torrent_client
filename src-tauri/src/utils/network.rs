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
    fn test_list_interfaces_runs() {
        let interfaces = list_network_interfaces();
        // Should execute without panicking on any platform
        assert!(!interfaces.is_empty());
    }
}

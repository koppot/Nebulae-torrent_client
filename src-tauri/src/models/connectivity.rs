use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    Open,
    Closed,
    Stealth,
    Testing,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    FullCone,
    RestrictedCone,
    PortRestricted,
    Symmetric,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectivityReport {
    pub listening_port: u16,
    pub port_status: PortStatus,
    pub upnp_available: bool,
    pub nat_pmp_available: bool,
    pub pcp_available: bool,
    pub nat_type: NatType,
    pub ipv4_healthy: bool,
    pub ipv6_healthy: bool,
    pub dns_health_ms: u32,
    pub vpn_interface_bound: bool,
    pub vpn_adapter_name: Option<String>,
    pub tracker_reachable: bool,
    pub tracker_auth_ok: bool,
    pub inbound_connections: u32,
    pub outbound_connections: u32,
    pub certificate_text: String,
    pub plain_language_findings: Vec<String>,
    pub suggested_fixes: Vec<String>,
}

impl ConnectivityReport {
    pub fn generate_certificate(
        port: u16,
        port_status: PortStatus,
        ipv6_healthy: bool,
        tracker_reachable: bool,
        vpn_bound: bool,
        vpn_adapter: Option<&str>,
    ) -> String {
        let connectable_str = if port_status == PortStatus::Open { "Yes" } else { "Limited" };
        let port_str = match port_status {
            PortStatus::Open => format!("Port {port} Reachable"),
            PortStatus::Closed => format!("Port {port} Closed/Firewalled"),
            PortStatus::Stealth => format!("Port {port} Stealth/Blocked"),
            PortStatus::Testing => format!("Port {port} Probing"),
        };
        let ipv6_str = if ipv6_healthy { "IPv6 Ready" } else { "IPv4 Only" };
        let tracker_str = if tracker_reachable { "Tracker Healthy" } else { "Tracker Blocked" };
        let vpn_str = if vpn_bound {
            format!("VPN Bound ({})", vpn_adapter.unwrap_or("Active"))
        } else {
            "Direct Connection".to_string()
        };

        format!(
            "ADC Connectable: {connectable_str} — {port_str} — {ipv6_str} — {tracker_str} — {vpn_str}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_formatting() {
        let cert = ConnectivityReport::generate_certificate(
            6881,
            PortStatus::Open,
            true,
            true,
            true,
            Some("utun3"),
        );

        assert_eq!(
            cert,
            "ADC Connectable: Yes — Port 6881 Reachable — IPv6 Ready — Tracker Healthy — VPN Bound (utun3)"
        );
    }
}

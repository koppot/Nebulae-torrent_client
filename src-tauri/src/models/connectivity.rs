use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    Open,
    /// IPv6 is reachable from the internet but IPv4 is not.
    Partial,
    Closed,
    Unknown,
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
pub struct EngineIpv6Diagnostic {
    pub daemon_path: Option<String>,
    pub daemon_pid: Option<u32>,
    pub bind_address_ipv4: String,
    pub bind_address_ipv6: String,
    pub peer_port: u16,
    pub vpn_mode: String,
    pub is_vpn_required: bool,
    pub active_vpn_interface: Option<String>,
    pub vpn_has_ipv4_egress: bool,
    pub vpn_has_ipv6_egress: bool,
    pub ipv6_listening_sockets: Vec<String>,
    pub established_ipv6_peer_connections: u32,
    pub os_ipv6_reachability: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_ipv6_reachable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_ipv6_status: Option<String>,
    pub dns_health_ms: u32,
    pub vpn_interface_bound: bool,
    pub vpn_adapter_name: Option<String>,
    pub tracker_reachable: bool,
    pub tracker_auth_ok: bool,
    /// Directional connection counts are not exposed by the current engine API.
    pub inbound_connections: Option<u32>,
    pub outbound_connections: Option<u32>,
    /// The current port verdict is based only on Transmission's explicit
    /// `port-is-open` boolean. Local bind attempts and peer counts do not
    /// establish external reachability.
    pub port_open_evidence: Option<String>,
    /// Per-address-family verdicts from the engine's port test; `None` when
    /// that family was not tested or the test was inconclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_open_ipv4: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_open_ipv6: Option<bool>,
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
        let connectable_str = match port_status {
            PortStatus::Open => "Yes",
            PortStatus::Partial => "IPv6 only",
            PortStatus::Closed => "No",
            PortStatus::Stealth | PortStatus::Testing | PortStatus::Unknown => "Unknown",
        };
        let port_str = match port_status {
            PortStatus::Open => format!("Port {port} Reachable"),
            PortStatus::Partial => format!("Port {port} Reachable over IPv6, closed over IPv4"),
            PortStatus::Closed => format!("Port {port} Closed/Firewalled"),
            PortStatus::Unknown => format!("Port {port} Reachability Unknown"),
            PortStatus::Stealth => format!("Port {port} Stealth/Blocked"),
            PortStatus::Testing => format!("Port {port} Probing"),
        };
        let ipv6_str = if ipv6_healthy {
            "IPv6 Ready"
        } else {
            "IPv4 Only"
        };
        let tracker_str = if tracker_reachable {
            "Tracker Healthy"
        } else {
            "Tracker Blocked"
        };
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
            41413,
            PortStatus::Open,
            true,
            true,
            true,
            Some("utun3"),
        );

        assert_eq!(
            cert,
            "ADC Connectable: Yes — Port 41413 Reachable — IPv6 Ready — Tracker Healthy — VPN Bound (utun3)"
        );
    }

    #[test]
    fn partial_port_status_is_reported_as_ipv6_only() {
        let cert = ConnectivityReport::generate_certificate(
            41413,
            PortStatus::Partial,
            true,
            true,
            false,
            None,
        );

        assert_eq!(
            cert,
            "ADC Connectable: IPv6 only — Port 41413 Reachable over IPv6, closed over IPv4 — IPv6 Ready — Tracker Healthy — Direct Connection"
        );
    }

    #[test]
    fn unknown_port_status_does_not_claim_connectability() {
        let cert = ConnectivityReport::generate_certificate(
            41413,
            PortStatus::Unknown,
            false,
            false,
            false,
            None,
        );

        assert_eq!(
            cert,
            "ADC Connectable: Unknown — Port 41413 Reachability Unknown — IPv4 Only — Tracker Blocked — Direct Connection"
        );
    }

    #[test]
    fn test_engine_ipv6_diagnostic_serialization() {
        let diag = EngineIpv6Diagnostic {
            daemon_path: Some("C:\\path\\transmission-daemon.exe".to_string()),
            daemon_pid: Some(1234),
            bind_address_ipv4: "0.0.0.0".to_string(),
            bind_address_ipv6: "::".to_string(),
            peer_port: 41413,
            vpn_mode: "NoVpn".to_string(),
            is_vpn_required: false,
            active_vpn_interface: None,
            vpn_has_ipv4_egress: true,
            vpn_has_ipv6_egress: true,
            ipv6_listening_sockets: vec!["TCP [::]:41413".to_string()],
            established_ipv6_peer_connections: 0,
            os_ipv6_reachability: true,
        };
        let s = serde_json::to_string(&diag).unwrap();
        assert!(s.contains("\"bind_address_ipv6\":\"::\""));
    }
}

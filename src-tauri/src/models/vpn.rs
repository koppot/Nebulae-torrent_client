use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum VpnMode {
    #[default]
    NoVpn,
    SystemVpn,
    AdcOnlyVpn,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum VpnProtocol {
    WireGuard,
    OpenVpn,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VpnProfile {
    pub id: String,
    pub name: String,
    pub protocol: VpnProtocol,
    pub endpoint: String,
    pub interface_address: String,
    pub dns_servers: Vec<String>,
    pub requires_auth: bool,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VpnStatus {
    pub connected: bool,
    pub interface_name: Option<String>,
    pub assigned_ip: Option<String>,
    pub endpoint: Option<String>,
    pub handshake_age_secs: Option<u64>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub kill_switch_active: bool,
    pub kill_switch_triggered: bool,
    pub mode: VpnMode,
}

impl Default for VpnStatus {
    fn default() -> Self {
        Self {
            connected: false,
            interface_name: None,
            assigned_ip: None,
            endpoint: None,
            handshake_age_secs: None,
            bytes_sent: 0,
            bytes_received: 0,
            kill_switch_active: true,
            kill_switch_triggered: false,
            mode: VpnMode::NoVpn,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedWireGuardProfile {
    pub private_key: String,
    pub address: String,
    pub dns: Vec<String>,
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
}

impl ParsedWireGuardProfile {
    pub fn parse_conf(content: &str) -> Result<Self, String> {
        let mut private_key = String::new();
        let mut address = String::new();
        let mut dns = Vec::new();
        let mut public_key = String::new();
        let mut endpoint = String::new();
        let mut allowed_ips = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_lowercase();
                let val = v.trim();

                match key.as_str() {
                    "privatekey" => private_key = val.to_string(),
                    "address" => address = val.to_string(),
                    "dns" => {
                        dns = val.split(',').map(|s| s.trim().to_string()).collect();
                    }
                    "publickey" => public_key = val.to_string(),
                    "endpoint" => endpoint = val.to_string(),
                    "allowedips" => {
                        allowed_ips = val.split(',').map(|s| s.trim().to_string()).collect();
                    }
                    _ => {}
                }
            }
        }

        if private_key.is_empty() {
            return Err("Missing PrivateKey in WireGuard config".to_string());
        }
        if endpoint.is_empty() {
            return Err("Missing Endpoint in WireGuard config".to_string());
        }

        Ok(Self {
            private_key,
            address,
            dns,
            public_key,
            endpoint,
            allowed_ips,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOpenVpnProfile {
    pub endpoint: String,
    pub protocol: String,
    pub port: u16,
    pub dev: String,
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub tls_auth: Option<String>,
    pub dns_servers: Vec<String>,
}

impl ParsedOpenVpnProfile {
    pub fn parse_ovpn(content: &str) -> Result<Self, String> {
        let mut endpoint_host = String::new();
        let mut port_opt: Option<u16> = None;
        let mut protocol = "udp".to_string();
        let mut dev = "tun".to_string();
        let mut dns_servers = Vec::new();

        let extract_block = |tag: &str| -> Option<String> {
            let open_tag = format!("<{tag}>");
            let close_tag = format!("</{tag}>");
            if let Some(start) = content.find(&open_tag) {
                if let Some(end) = content.find(&close_tag) {
                    if end > start + open_tag.len() {
                        let inner = &content[start + open_tag.len()..end];
                        return Some(inner.trim().to_string());
                    }
                }
            }
            None
        };

        let ca_cert = extract_block("ca");
        let client_cert = extract_block("cert");
        let client_key = extract_block("key");
        let tls_auth = extract_block("tls-auth");

        let mut in_block = false;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('<') && !line.starts_with("</") {
                in_block = true;
                continue;
            }
            if line.starts_with("</") {
                in_block = false;
                continue;
            }
            if in_block {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0].to_lowercase().as_str() {
                "remote" => {
                    if parts.len() > 1 && endpoint_host.is_empty() {
                        endpoint_host = parts[1].to_string();
                    }
                    if parts.len() > 2 {
                        if let Ok(p) = parts[2].parse::<u16>() {
                            port_opt = Some(p);
                        }
                    }
                    if parts.len() > 3 {
                        protocol = parts[3].to_lowercase();
                    }
                }
                "port" => {
                    if parts.len() > 1 && port_opt.is_none() {
                        if let Ok(p) = parts[1].parse::<u16>() {
                            port_opt = Some(p);
                        }
                    }
                }
                "proto" => {
                    if parts.len() > 1 {
                        protocol = parts[1].to_lowercase();
                    }
                }
                "dev" => {
                    if parts.len() > 1 {
                        dev = parts[1].to_string();
                    }
                }
                "dhcp-option" if parts.len() > 2 && parts[1].eq_ignore_ascii_case("DNS") => {
                    dns_servers.push(parts[2].to_string());
                }
                _ => {}
            }
        }

        if endpoint_host.is_empty() {
            return Err("Missing 'remote' directive in OpenVPN profile".to_string());
        }

        let port = port_opt.unwrap_or(1194);
        let endpoint = format!("{}:{}", endpoint_host, port);

        Ok(Self {
            endpoint,
            protocol,
            port,
            dev,
            ca_cert,
            client_cert,
            client_key,
            tls_auth,
            dns_servers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wireguard_conf_parser() {
        let conf = r#"
        [Interface]
        PrivateKey = aFakeWireGuardPrivateKeyBase64ForTesting123=
        Address = 10.2.0.2/32
        DNS = 1.1.1.1, 1.0.0.1

        [Peer]
        PublicKey = aFakeWireGuardPublicKeyBase64ForTesting456=
        Endpoint = 198.51.100.1:51820
        AllowedIPs = 0.0.0.0/0
        "#;

        let parsed = ParsedWireGuardProfile::parse_conf(conf).expect("Parsing failed");
        assert_eq!(
            parsed.private_key,
            "aFakeWireGuardPrivateKeyBase64ForTesting123="
        );
        assert_eq!(parsed.address, "10.2.0.2/32");
        assert_eq!(parsed.endpoint, "198.51.100.1:51820");
        assert_eq!(parsed.dns.len(), 2);
    }

    #[test]
    fn test_openvpn_conf_parser() {
        let ovpn = r#"
        client
        dev tun
        proto udp
        remote vpn.asiandvdclub.org 1194
        resolv-retry infinite
        nobind
        dhcp-option DNS 1.1.1.1
        dhcp-option DNS 8.8.8.8

        <ca>
        -----BEGIN CERTIFICATE-----
        MIIFakeCertificateAuthorityData==
        -----END CERTIFICATE-----
        </ca>

        <cert>
        -----BEGIN CERTIFICATE-----
        MIIFakeClientCertificateData==
        -----END CERTIFICATE-----
        </cert>

        <key>
        -----BEGIN PRIVATE KEY-----
        MIIFakeClientPrivateKeyData==
        -----END PRIVATE KEY-----
        </key>
        "#;

        let parsed = ParsedOpenVpnProfile::parse_ovpn(ovpn).expect("OpenVPN parsing failed");
        assert_eq!(parsed.endpoint, "vpn.asiandvdclub.org:1194");
        assert_eq!(parsed.port, 1194);
        assert_eq!(parsed.protocol, "udp");
        assert_eq!(parsed.dev, "tun");
        assert_eq!(parsed.dns_servers.len(), 2);
        assert_eq!(parsed.dns_servers[0], "1.1.1.1");
        assert!(parsed.ca_cert.is_some());
        assert!(parsed.client_cert.is_some());
        assert!(parsed.client_key.is_some());
        assert_eq!(
            parsed.client_key.as_deref(),
            Some("-----BEGIN PRIVATE KEY-----\n        MIIFakeClientPrivateKeyData==\n        -----END PRIVATE KEY-----")
        );
    }
}

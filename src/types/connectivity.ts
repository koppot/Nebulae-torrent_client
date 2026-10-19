export type PortStatus = 'Open' | 'Closed' | 'Stealth' | 'Checking';

export type NatType = 'FullCone' | 'RestrictedCone' | 'PortRestricted' | 'Symmetric' | 'Unknown';

export interface ConnectivityReport {
  listening_port: number;
  port_status: PortStatus;
  upnp_available: boolean;
  nat_pmp_available: boolean;
  pcp_available: boolean;
  nat_type: NatType;
  ipv4_healthy: boolean;
  ipv6_healthy: boolean;
  dns_health_ms: number;
  vpn_interface_bound: boolean;
  vpn_adapter_name?: string;
  tracker_reachable: boolean;
  tracker_auth_ok: boolean;
  inbound_connections: number;
  outbound_connections: number;
  /** What produced the port verdict: transmission-self-test, inbound-peers, local-listener, or null. */
  port_open_evidence?: string | null;
  certificate_text: string;
  plain_language_findings: string[];
  suggested_fixes: string[];
}

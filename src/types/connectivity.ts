export type PortStatus = 'Open' | 'Partial' | 'Closed' | 'Unknown' | 'Stealth' | 'Testing' | 'Checking';

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
  inbound_connections: number | null;
  outbound_connections: number | null;
  /** The engine's explicit port-test result, or null when no verdict was available. */
  port_open_evidence?: string | null;
  /** Per-family engine port-test verdicts; absent when not tested or inconclusive. */
  port_open_ipv4?: boolean | null;
  port_open_ipv6?: boolean | null;
  certificate_text: string;
  plain_language_findings: string[];
  suggested_fixes: string[];
}

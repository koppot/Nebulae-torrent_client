export type VpnMode = 'NoVpn' | 'SystemVpn' | 'AdcOnlyVpn';

export type VpnProtocol = 'WireGuard' | 'OpenVpn';

export interface VpnProfile {
  id: string;
  name: string;
  protocol: VpnProtocol;
  endpoint: string;
  interface_address: string;
  dns_servers: string[];
  requires_auth: boolean;
  created_at: number;
}

export interface VpnStatus {
  connected: boolean;
  interface_name?: string;
  assigned_ip?: string;
  endpoint?: string;
  handshake_age_secs?: number;
  bytes_sent: number;
  bytes_received: number;
  kill_switch_active: boolean;
  kill_switch_triggered: boolean;
  mode: VpnMode;
}

export type PeeringMode = 'Balanced' | 'SpeedFirst' | 'LatencyFirst' | 'LocalFirst' | 'PrivacyFirst';

export type LocalityTier = 'Lan' | 'Asn' | 'Region' | 'Country' | 'Global';

export interface PeerTelemetry {
  peer_id: string;
  ip: string;
  port: number;
  rtt_ms: number;
  req_resp_latency_ms: number;
  sustained_dl_bps: number;
  sustained_ul_bps: number;
  upload_reciprocity: number;
  stability_score: number;
  disconnect_count: number;
  timeout_count: number;
  choke_events: number;
  client_name: string;
  locality_tier: LocalityTier;
  country_code?: string;
  asn?: number;
  utp: boolean;
  encryption: boolean;
  ipv6: boolean;
  dht: boolean;
  pex: boolean;
  connection_age_seconds: number;
  total_downloaded_bytes: number;
  total_uploaded_bytes: number;
}

export interface PeeringSettings {
  mode: PeeringMode;
  /** Mirrors Rust PeeringSettings::locality_enabled */
  locality_enabled: boolean;
  /** Mirrors Rust PeeringSettings::max_peers_per_torrent */
  max_peers_per_torrent: number;
  /** Mirrors Rust PeeringSettings::max_global_peers */
  max_global_peers: number;
  /** Mirrors Rust PeeringSettings::prefer_utp */
  prefer_utp: boolean;
  /** Mirrors Rust PeeringSettings::require_encryption */
  require_encryption: boolean;
}

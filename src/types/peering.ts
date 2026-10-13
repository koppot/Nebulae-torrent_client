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
  enable_locality_preference: boolean;
  enable_geographic_metadata: boolean;
  enable_rate_limited_probes: boolean;
  max_connections_per_torrent: number;
  preferred_country?: string;
}

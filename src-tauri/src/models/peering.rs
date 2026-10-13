use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PeeringMode {
    Balanced,
    SpeedFirst,
    LatencyFirst,
    LocalFirst,
    PrivacyFirst,
}

impl Default for PeeringMode {
    fn default() -> Self {
        PeeringMode::Balanced
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LocalityTier {
    Lan,
    Asn,
    Region,
    Country,
    Global,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeerTelemetry {
    pub peer_id: String,
    pub ip: String,
    pub port: u16,
    pub rtt_ms: u32,
    pub req_resp_latency_ms: u32,
    pub sustained_dl_bps: u64,
    pub sustained_ul_bps: u64,
    pub upload_reciprocity: f32,
    pub stability_score: f32, // 0.0 - 1.0
    pub disconnect_count: u32,
    pub timeout_count: u32,
    pub choke_events: u32,
    pub client_name: String,
    pub locality_tier: LocalityTier,
    pub country_code: Option<String>,
    pub asn: Option<u32>,
    pub utp: bool,
    pub encryption: bool,
    pub ipv6: bool,
    pub dht: bool,
    pub pex: bool,
    pub connection_age_seconds: u64,
    pub total_downloaded_bytes: u64,
    pub total_uploaded_bytes: u64,
}

impl PeerTelemetry {
    pub fn compute_quality_score(&self, mode: PeeringMode) -> f32 {
        // Normalize throughput: 10 MiB/s = 1.0
        let norm_tp = (self.sustained_dl_bps as f32 / (10.0 * 1024.0 * 1024.0)).clamp(0.0, 1.0);
        
        // Normalize latency: 10ms = 1.0, 500ms = 0.0
        let norm_lat = (1.0 - (self.rtt_ms as f32 / 500.0)).clamp(0.0, 1.0);
        
        // Reliability based on stability and timeouts
        let timeout_penalty = (self.timeout_count as f32 * 0.1).clamp(0.0, 0.5);
        let norm_rel = (self.stability_score - timeout_penalty).clamp(0.0, 1.0);
        
        // Locality bonus
        let norm_loc = match self.locality_tier {
            LocalityTier::Lan => 1.0,
            LocalityTier::Asn => 0.8,
            LocalityTier::Region => 0.6,
            LocalityTier::Country => 0.4,
            LocalityTier::Global => 0.1,
        };

        match mode {
            PeeringMode::Balanced => {
                0.35 * norm_tp + 0.25 * norm_lat + 0.25 * norm_rel + 0.15 * norm_loc
            }
            PeeringMode::SpeedFirst => {
                0.60 * norm_tp + 0.15 * norm_lat + 0.20 * norm_rel + 0.05 * norm_loc
            }
            PeeringMode::LatencyFirst => {
                0.20 * norm_tp + 0.55 * norm_lat + 0.20 * norm_rel + 0.05 * norm_loc
            }
            PeeringMode::LocalFirst => {
                0.25 * norm_tp + 0.20 * norm_lat + 0.15 * norm_rel + 0.40 * norm_loc
            }
            PeeringMode::PrivacyFirst => {
                // Privacy First zero-outs locality inference
                0.45 * norm_tp + 0.25 * norm_lat + 0.30 * norm_rel
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeeringSettings {
    pub mode: PeeringMode,
    pub locality_enabled: bool,
    pub max_peers_per_torrent: u32,
    pub max_global_peers: u32,
    pub prefer_utp: bool,
    pub require_encryption: bool,
}

impl Default for PeeringSettings {
    fn default() -> Self {
        Self {
            mode: PeeringMode::Balanced,
            locality_enabled: true,
            max_peers_per_torrent: 80,
            max_global_peers: 500,
            prefer_utp: true,
            require_encryption: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_quality_scoring() {
        let peer = PeerTelemetry {
            peer_id: "test-peer-1".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 6881,
            rtt_ms: 15,
            req_resp_latency_ms: 20,
            sustained_dl_bps: 8 * 1024 * 1024, // 8 MiB/s
            sustained_ul_bps: 1024 * 1024,
            upload_reciprocity: 0.8,
            stability_score: 0.95,
            disconnect_count: 0,
            timeout_count: 0,
            choke_events: 1,
            client_name: "ADC/1.0".to_string(),
            locality_tier: LocalityTier::Lan,
            country_code: Some("US".to_string()),
            asn: None,
            utp: true,
            encryption: true,
            ipv6: false,
            dht: false,
            pex: false,
            connection_age_seconds: 3600,
            total_downloaded_bytes: 500_000_000,
            total_uploaded_bytes: 100_000_000,
        };

        let balanced = peer.compute_quality_score(PeeringMode::Balanced);
        let speed = peer.compute_quality_score(PeeringMode::SpeedFirst);
        let latency = peer.compute_quality_score(PeeringMode::LatencyFirst);
        let local = peer.compute_quality_score(PeeringMode::LocalFirst);
        let privacy = peer.compute_quality_score(PeeringMode::PrivacyFirst);

        assert!(balanced > 0.7);
        assert!(speed > 0.7);
        assert!(latency > 0.7);
        assert!(local > 0.7);
        assert!(privacy > 0.7);
    }
}

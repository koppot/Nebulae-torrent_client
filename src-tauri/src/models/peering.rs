use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeeringMode {
    #[default]
    Balanced,
    SpeedFirst,
    LatencyFirst,
    LocalFirst,
    PrivacyFirst,
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
    /// Sustained download rate in bits per second (bits/s = bytes/s * 8).
    pub sustained_dl_bps: u64,
    /// Sustained upload rate in bits per second (bits/s = bytes/s * 8).
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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PeerBlockCapability {
    Unsupported,
    RepeatedDisconnect,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockedPeerIp {
    pub ip: String,
    pub blocked_at_epoch_ms: u64,
}

/// Local observation history for one peer endpoint on one torrent, including
/// the minimal locally resolved ADC identity used for display and profile links.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeerHistoryEntry {
    pub peer_key: String,
    pub ip: String,
    pub port: u16,
    pub client_name: String,
    pub country_code: Option<String>,
    pub first_seen_epoch_ms: u64,
    pub last_seen_epoch_ms: u64,
    pub downloaded_delta_bytes: u64,
    pub uploaded_delta_bytes: u64,
    pub counter_resets: u64,
    pub was_blocked: bool,
    pub adc_user_id: Option<u64>,
    pub adc_username: Option<String>,
    pub adc_profile_url: Option<String>,
    pub adc_account_revision: Option<i64>,
    pub adc_matched_at_epoch_ms: Option<u64>,
    pub currently_blocked: bool,
}

/// Per-peer exponential moving average state persisted across `get_peer_telemetry` calls.
///
/// The EMA smoothing factor α = 0.3 means each new sample contributes 30% and
/// the history contributes 70%, giving a ~3-sample rolling window. This is a
/// reasonable balance between responsiveness and noise rejection for a 2-second
/// polling interval.
#[derive(Debug, Clone)]
pub struct PeerEmaState {
    /// EMA of `rateToClient` in bytes/s (download from peer).
    pub ema_dl_bps: f64,
    /// EMA of `rateToPeer` in bytes/s (upload to peer).
    pub ema_ul_bps: f64,
    /// Cumulative variance proxy: tracks how much DL rate fluctuates cycle-over-cycle.
    /// Encoded as the EMA of |current - previous|. Higher value = less stable peer.
    pub ema_dl_variance: f64,
    /// How many consecutive polling cycles this peer has been seen.
    pub observation_count: u32,
}

impl PeerTelemetry {
    pub fn compute_quality_score(&self, mode: PeeringMode) -> f32 {
        // Normalize throughput: 10 MiB/s expressed in bits/s = 1.0
        // sustained_dl_bps carries bits/s. 10 MiB/s = 10 * 8 * 1024 * 1024 bits/s = 83886080.
        let norm_tp =
            (self.sustained_dl_bps as f32 / (10.0 * 8.0 * 1024.0 * 1024.0)).clamp(0.0, 1.0);

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
#[serde(default)]
pub struct PeeringSettings {
    pub mode: PeeringMode,
    pub locality_enabled: bool,
    pub max_peers_per_torrent: u32,
    pub max_global_peers: u32,
    pub prefer_utp: bool,
    pub require_encryption: bool,
    /// TCP RTT probing uses bounded outbound TCP connects; false is an explicit opt-out.
    pub probe_rtt_enabled: bool,
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
            probe_rtt_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtt_probe_setting_defaults_on_but_preserves_explicit_opt_out() {
        let legacy: PeeringSettings = serde_json::from_str("{}").unwrap();
        let opted_out: PeeringSettings =
            serde_json::from_str(r#"{"probe_rtt_enabled":false}"#).unwrap();
        assert!(legacy.probe_rtt_enabled);
        assert!(!opted_out.probe_rtt_enabled);
    }

    #[test]
    fn peer_history_payload_contains_minimal_display_identity_fields() {
        let history = PeerHistoryEntry {
            peer_key: "203.0.113.17:51413".into(),
            ip: "203.0.113.17".into(),
            port: 51413,
            client_name: "Transmission 4.0".into(),
            country_code: Some("RO".into()),
            first_seen_epoch_ms: 1,
            last_seen_epoch_ms: 2,
            downloaded_delta_bytes: 3,
            uploaded_delta_bytes: 4,
            counter_resets: 0,
            was_blocked: false,
            adc_user_id: Some(23),
            adc_username: Some("synthetic-member".into()),
            adc_profile_url: Some("https://asiandvdclub.org/profile/23".into()),
            adc_account_revision: Some(2),
            adc_matched_at_epoch_ms: Some(2),
            currently_blocked: false,
        };
        let payload = serde_json::to_value(history).unwrap();
        assert_eq!(payload["client_name"], "Transmission 4.0");
        assert_eq!(payload["country_code"], "RO");
        assert_eq!(payload["adc_username"], "synthetic-member");
        assert_eq!(payload["adc_user_id"], 23);
        assert_eq!(
            payload["adc_profile_url"],
            "https://asiandvdclub.org/profile/23"
        );
        assert!(payload.get("passkey").is_none());
        assert!(payload.get("password").is_none());
        assert!(payload.get("full_profile").is_none());
    }

    #[test]
    fn test_peer_quality_scoring() {
        // The fixture was authored with 8 MiB/s download and 1 MiB/s upload throughput in mind.
        // Under the established bits/s contract (sustained_dl_bps = bytes/s * 8):
        // 8 MiB/s = 8 * 8 * 1024 * 1024 bits/s = 67_108_864 bits/s.
        // 1 MiB/s = 1 * 8 * 1024 * 1024 bits/s = 8_388_608 bits/s.
        let peer = PeerTelemetry {
            peer_id: "test-peer-1".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 41413,
            rtt_ms: 15,
            req_resp_latency_ms: 20,
            sustained_dl_bps: 8 * 8 * 1024 * 1024, // 8 MiB/s actual throughput in bits/s convention
            sustained_ul_bps: 8 * 1024 * 1024,     // 1 MiB/s actual throughput in bits/s convention
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

    fn make_ideal_peer(sustained_dl_bps: u64) -> PeerTelemetry {
        PeerTelemetry {
            peer_id: "test-peer".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 41413,
            rtt_ms: 5,
            req_resp_latency_ms: 5,
            sustained_dl_bps,
            sustained_ul_bps: 1024 * 1024,
            upload_reciprocity: 1.0,
            stability_score: 1.0,
            disconnect_count: 0,
            timeout_count: 0,
            choke_events: 0,
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
        }
    }

    #[test]
    fn test_quality_score_no_saturation_at_10_mbps() {
        // 80 * 1024 * 1024 bits/s = 10 MiB/s in the bits/s convention.
        // norm_tp should be exactly 1.0; SpeedFirst score with ideal lat/rel/loc must be >= 0.95.
        let peer = make_ideal_peer(80 * 1024 * 1024);
        let score = peer.compute_quality_score(PeeringMode::SpeedFirst);
        assert!(
            score >= 0.95,
            "Peer at 10 MiB/s (80 Mib) must score >= 0.95 in SpeedFirst, got {score}"
        );
    }

    #[test]
    fn test_quality_score_low_throughput_below_half() {
        // 4 * 1024 * 1024 bits/s = 0.5 MiB/s actual throughput.
        // SpeedFirst score with ideal lat/rel/loc must be < 0.5 (throughput axis dominates at 0.60).
        let peer = make_ideal_peer(4 * 1024 * 1024);
        let score = peer.compute_quality_score(PeeringMode::SpeedFirst);
        assert!(
            score < 0.5,
            "Peer at 0.5 MiB/s (4 Mib) must score < 0.5 in SpeedFirst, got {score}"
        );
    }

    #[test]
    fn test_quality_score_throughput_differentiates_peers() {
        // High-throughput peer: 80 * 1024 * 1024 bits/s = 10 MiB/s.
        // Low-throughput peer:   8 * 1024 * 1024 bits/s =  1 MiB/s.
        // The high-throughput peer must score at least 0.3 higher in SpeedFirst.
        let high = make_ideal_peer(80 * 1024 * 1024);
        let low = make_ideal_peer(8 * 1024 * 1024);
        let high_score = high.compute_quality_score(PeeringMode::SpeedFirst);
        let low_score = low.compute_quality_score(PeeringMode::SpeedFirst);
        assert!(
            high_score - low_score >= 0.3,
            "High-throughput peer ({high_score}) must score at least 0.3 above low-throughput peer ({low_score})"
        );
    }
}

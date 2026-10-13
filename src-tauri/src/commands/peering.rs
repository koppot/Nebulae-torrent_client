use crate::db::Database;
use crate::models::peering::{LocalityTier, PeerTelemetry, PeeringSettings};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_peering_settings(db: State<Arc<Database>>) -> Result<PeeringSettings, String> {
    let mut settings = PeeringSettings::default();
    if let Ok(Some(json)) = db.get_setting("peering_settings_json") {
        if let Ok(parsed) = serde_json::from_str(&json) {
            settings = parsed;
        }
    }
    Ok(settings)
}

#[tauri::command]
pub fn save_peering_settings(
    settings: PeeringSettings,
    db: State<Arc<Database>>,
) -> Result<(), String> {
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    db.set_setting("peering_settings_json", &json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_peer_telemetry(_torrent_id: String) -> Result<Vec<PeerTelemetry>, String> {
    // Generate initial live telemetry sample peers for testing and verification
    Ok(vec![
        PeerTelemetry {
            peer_id: "peer-tokyo-1".to_string(),
            ip: "192.168.1.105".to_string(),
            port: 51413,
            rtt_ms: 8,
            req_resp_latency_ms: 12,
            sustained_dl_bps: 4_500_000,
            sustained_ul_bps: 850_000,
            upload_reciprocity: 0.85,
            stability_score: 0.98,
            disconnect_count: 0,
            timeout_count: 0,
            choke_events: 0,
            client_name: "Nebulae/1.0".to_string(),
            locality_tier: LocalityTier::Lan,
            country_code: Some("JP".to_string()),
            asn: None,
            utp: true,
            encryption: true,
            ipv6: false,
            dht: false,
            pex: true,
            connection_age_seconds: 1420,
            total_downloaded_bytes: 420_000_000,
            total_uploaded_bytes: 88_000_000,
        },
        PeerTelemetry {
            peer_id: "peer-osaka-2".to_string(),
            ip: "203.0.113.42".to_string(),
            port: 6881,
            rtt_ms: 38,
            req_resp_latency_ms: 55,
            sustained_dl_bps: 2_800_000,
            sustained_ul_bps: 320_000,
            upload_reciprocity: 0.65,
            stability_score: 0.91,
            disconnect_count: 1,
            timeout_count: 0,
            choke_events: 2,
            client_name: "qBittorrent/4.6.5".to_string(),
            locality_tier: LocalityTier::Country,
            country_code: Some("JP".to_string()),
            asn: Some(2516),
            utp: true,
            encryption: true,
            ipv6: true,
            dht: false,
            pex: true,
            connection_age_seconds: 980,
            total_downloaded_bytes: 280_000_000,
            total_uploaded_bytes: 45_000_000,
        },
        PeerTelemetry {
            peer_id: "peer-global-3".to_string(),
            ip: "198.51.100.88".to_string(),
            port: 52144,
            rtt_ms: 142,
            req_resp_latency_ms: 190,
            sustained_dl_bps: 1_200_000,
            sustained_ul_bps: 150_000,
            upload_reciprocity: 0.50,
            stability_score: 0.82,
            disconnect_count: 2,
            timeout_count: 1,
            choke_events: 4,
            client_name: "Transmission/4.0.5".to_string(),
            locality_tier: LocalityTier::Global,
            country_code: Some("US".to_string()),
            asn: Some(15169),
            utp: false,
            encryption: true,
            ipv6: false,
            dht: false,
            pex: false,
            connection_age_seconds: 430,
            total_downloaded_bytes: 95_000_000,
            total_uploaded_bytes: 14_000_000,
        },
    ])
}

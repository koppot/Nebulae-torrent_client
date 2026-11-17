use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::peering::{BlockedPeerIp, PeerBlockCapability, PeerTelemetry, PeeringSettings};
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
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    db.set_setting("peering_settings_json", &json)
        .map_err(|e| e.to_string())?;
    // Push limits to the running engine session. Non-fatal if the engine is
    // not yet started (startup applies them via the same path on init).
    if let Err(e) = engine.apply_peering_settings(&settings) {
        tracing::warn!("Peering settings saved but engine push failed: {e}");
    }
    Ok(())
}

#[tauri::command]
pub async fn get_peer_telemetry(
    torrent_id: String,
    engine: State<'_, SharedEngine>,
) -> Result<Vec<PeerTelemetry>, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.get_peer_telemetry(&torrent_id))
        .await
        .map_err(|e| format!("get_peer_telemetry task failed: {e}"))?
}

#[tauri::command]
pub fn get_peer_history(
    info_hash: String,
    db: State<Arc<Database>>,
) -> Result<Vec<crate::models::peering::PeerHistoryEntry>, String> {
    db.get_peer_history(&info_hash)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_peer_history(info_hash: String, db: State<Arc<Database>>) -> Result<usize, String> {
    db.clear_peer_history(&info_hash)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_peer_block_capability(engine: State<'_, SharedEngine>) -> PeerBlockCapability {
    engine.peer_block_capability()
}

#[tauri::command]
pub fn list_blocked_peer_ips(db: State<Arc<Database>>) -> Result<Vec<BlockedPeerIp>, String> {
    db.list_blocked_peer_ips()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn block_peer_ip(
    torrent_id: String,
    ip: String,
    port: u16,
    db: State<Arc<Database>>,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    if engine.peer_block_capability() == PeerBlockCapability::Unsupported {
        return Err("The active torrent engine cannot enforce manual peer blocks".to_string());
    }
    let ip = normalize_peer_ip(&ip)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let was_already_blocked = db
        .list_blocked_peer_ips()
        .map_err(|error| error.to_string())?
        .iter()
        .any(|entry| entry.ip == ip);

    // Persist first so the background enforcer covers reconnects immediately.
    db.block_peer_ip(&ip, now)
        .map_err(|error| error.to_string())?;
    if let Err(error) = engine.disconnect_peer_for_block(&torrent_id, &ip, port) {
        if !was_already_blocked {
            let _ = db.unblock_peer_ip(&ip);
        }
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub fn unblock_peer_ip(ip: String, db: State<Arc<Database>>) -> Result<(), String> {
    let ip = normalize_peer_ip(&ip)?;
    db.unblock_peer_ip(&ip).map_err(|error| error.to_string())
}

fn normalize_peer_ip(ip: &str) -> Result<String, String> {
    ip.parse::<std::net::IpAddr>()
        .map(|address| address.to_string())
        .map_err(|_| "Invalid peer IP address".to_string())
}

#[cfg(test)]
mod peer_block_tests {
    use super::*;

    #[test]
    fn peer_block_ip_validation_canonicalizes_addresses_and_rejects_names() {
        assert_eq!(
            normalize_peer_ip("2001:0db8:0:0::1").unwrap(),
            "2001:db8::1"
        );
        assert_eq!(
            normalize_peer_ip("example.org"),
            Err("Invalid peer IP address".into())
        );
    }
}

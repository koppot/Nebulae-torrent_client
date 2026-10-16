use crate::db::Database;
use crate::models::peering::{PeerTelemetry, PeeringSettings};
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

use crate::engine::SharedEngine;

#[tauri::command]
pub async fn get_peer_telemetry(
    torrent_id: String,
    engine: State<'_, SharedEngine>,
) -> Result<Vec<PeerTelemetry>, String> {
    engine.get_peer_telemetry(&torrent_id)
}

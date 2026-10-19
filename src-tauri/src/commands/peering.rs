use crate::db::Database;
use crate::engine::SharedEngine;
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
    engine.get_peer_telemetry(&torrent_id)
}

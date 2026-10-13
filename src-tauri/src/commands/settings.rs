use crate::db::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserSettings {
    pub download_directory: String,
    pub max_download_speed_kib: u32,
    pub max_upload_speed_kib: u32,
    pub theme: String,
    pub enable_splash: bool,
    pub splash_message: String,
    pub splash_url: String,
    pub listening_port: u16,
    pub enable_upnp: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        let default_download = dirs::download_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        Self {
            download_directory: default_download,
            max_download_speed_kib: 0,
            max_upload_speed_kib: 0,
            theme: "dark".to_string(),
            enable_splash: true,
            splash_message: "Consider joining our community\nasiandvdclub.org".to_string(),
            splash_url: "https://asiandvdclub.org".to_string(),
            listening_port: 6881,
            enable_upnp: true,
        }
    }
}

#[tauri::command]
pub fn load_user_settings(db: State<Arc<Database>>) -> Result<UserSettings, String> {
    let mut settings = UserSettings::default();

    if let Ok(Some(val)) = db.get_setting("user_settings_json") {
        if let Ok(parsed) = serde_json::from_str::<UserSettings>(&val) {
            settings = parsed;
        }
    }

    Ok(settings)
}

#[tauri::command]
pub fn save_user_settings(settings: UserSettings, db: State<Arc<Database>>) -> Result<(), String> {
    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    db.set_setting("user_settings_json", &json)
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    Ok(())
}

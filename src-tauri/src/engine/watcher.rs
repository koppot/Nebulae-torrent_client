use crate::commands::settings::UserSettings;
use crate::db::Database;
use crate::engine::SharedEngine;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub fn spawn_watched_folder_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));

        loop {
            interval.tick().await;

            let watched_dir = if let Ok(Some(json)) = db.get_setting("user_settings_json") {
                if let Ok(settings) = serde_json::from_str::<UserSettings>(&json) {
                    if !settings.watched_folder_directory.trim().is_empty() {
                        Some(settings.watched_folder_directory)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let dir_path = match watched_dir {
                Some(p) => p,
                None => continue,
            };

            let path = Path::new(&dir_path);
            if !path.is_dir() {
                continue;
            }

            let entries = match std::fs::read_dir(path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_file() {
                    if let Some(ext) = file_path.extension() {
                        if ext.to_string_lossy().eq_ignore_ascii_case("torrent") {
                            // Attempt to read bytes and add to engine
                            match std::fs::read(&file_path) {
                                Ok(bytes) => match engine.add_torrent_from_bytes(&bytes, None) {
                                    Ok(id) => {
                                        info!(
                                            "Watched folder: auto-loaded torrent {} from {}",
                                            id,
                                            file_path.display()
                                        );
                                        let mut loaded_name = file_path.clone();
                                        loaded_name.set_extension("torrent.loaded");
                                        let _ = std::fs::rename(&file_path, loaded_name);
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Watched folder: failed to parse/add torrent {}: {e}",
                                            file_path.display()
                                        );
                                    }
                                },
                                Err(e) => {
                                    warn!(
                                        "Watched folder: could not read file {}: {e}",
                                        file_path.display()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

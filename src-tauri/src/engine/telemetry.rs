use crate::db::Database;
use crate::engine::SharedEngine;
use crate::status_bar;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn spawn_telemetry_loop(app_handle: AppHandle, engine: SharedEngine) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;

            match engine.list_torrents() {
                Ok(torrents) => {
                    let _ = app_handle.emit("nebulae://telemetry", &torrents);

                    // Calculate aggregate transfer rates for macOS status bar and Windows toolbar
                    let (upload_rate, download_rate) =
                        torrents.iter().fold((0u64, 0u64), |acc, torrent| {
                            (acc.0 + torrent.upload_rate, acc.1 + torrent.download_rate)
                        });

                    // Update status bar on macOS
                    status_bar::update_status_bar(&app_handle, upload_rate, download_rate);

                    // Windows taskbar toolbar speed band (adjacent to Nebulae application icon)
                    crate::taskbar_speed::update_taskbar_speed(
                        &app_handle,
                        upload_rate,
                        download_rate,
                    );
                }
                Err(_) => {
                    // Clear stale rates on engine stop or failure
                    status_bar::clear_status_bar(&app_handle);
                    crate::taskbar_speed::clear_taskbar_speed(&app_handle);
                }
            }
        }
    });
}

/// Persist peer observations for all torrents with live peers independently
/// of which manager tab is visible.
pub fn spawn_peer_history_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let engine = Arc::clone(&engine);
            let db = Arc::clone(&db);
            if let Err(error) = tokio::task::spawn_blocking(move || {
                let torrents = match engine.list_torrents() {
                    Ok(torrents) => torrents,
                    Err(error) => {
                        tracing::debug!("Peer history poll skipped: {error}");
                        return;
                    }
                };

                for torrent in torrents.into_iter().filter(|torrent| torrent.peers > 0) {
                    let observed_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_millis() as u64)
                        .unwrap_or(0);
                    let peers = match engine.get_peer_telemetry(&torrent.id) {
                        Ok(peers) => peers,
                        Err(error) => {
                            tracing::debug!("Peer history sample skipped: {error}");
                            continue;
                        }
                    };
                    if let Err(error) =
                        db.record_peer_history(&torrent.info_hash, &peers, observed_at)
                    {
                        tracing::warn!("Could not persist local peer history: {error}");
                    }
                }

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                if let Err(error) = db.prune_peer_history(now) {
                    tracing::warn!("Could not prune local peer history: {error}");
                }
            })
            .await
            {
                tracing::warn!("Peer history sampling worker failed: {error}");
            }
        }
    });
}

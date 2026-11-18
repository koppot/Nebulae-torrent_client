use crate::commands::torrent::TorrentItem;
use crate::db::Database;
use crate::engine::SharedEngine;
use crate::status_bar;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub fn spawn_telemetry_loop(
    app_handle: AppHandle,
    engine: SharedEngine,
    scoped: Arc<crate::engine::ScopedEngine>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last_emitted_torrents: Option<Vec<TorrentItem>> = None;
        let mut last_emitted_at = Instant::now()
            .checked_sub(Duration::from_secs(10))
            .unwrap_or_else(Instant::now);
        let mut last_rates: Option<(u64, u64)> = None;
        let mut last_scope_remote = false;

        loop {
            interval.tick().await;

            let engine_clone = Arc::clone(&engine);
            let poll_result =
                tokio::task::spawn_blocking(move || engine_clone.list_torrents()).await;

            match poll_result {
                Ok(Ok(torrents)) => {
                    // Rates below always describe this computer; the list shown in the
                    // window follows the active tab (Manager = local, Server = remote).
                    let remote_scope = scoped.is_remote_active();
                    let ui_torrents = if remote_scope {
                        match scoped.remote_engine() {
                            Some(remote) => {
                                tokio::task::spawn_blocking(move || remote.list_torrents())
                                    .await
                                    .ok()
                                    .and_then(|r| r.ok())
                            }
                            None => None,
                        }
                    } else {
                        Some(torrents.clone())
                    };
                    if last_scope_remote != remote_scope {
                        last_scope_remote = remote_scope;
                        last_emitted_torrents = None;
                    }
                    let heartbeat_due = last_emitted_at.elapsed() >= Duration::from_secs(5);

                    if let Some(ui) = ui_torrents {
                        let has_changed = last_emitted_torrents.as_ref() != Some(&ui);
                        if has_changed || heartbeat_due {
                            let _ = app_handle.emit(
                                "nebulae://telemetry",
                                serde_json::json!({ "remote": remote_scope, "torrents": &ui }),
                            );
                            last_emitted_torrents = Some(ui);
                            last_emitted_at = Instant::now();
                        }
                    }

                    // Calculate aggregate transfer rates for macOS status bar and Windows toolbar
                    let (upload_rate, download_rate) =
                        torrents.iter().fold((0u64, 0u64), |acc, torrent| {
                            (acc.0 + torrent.upload_rate, acc.1 + torrent.download_rate)
                        });

                    let current_rates = (upload_rate, download_rate);
                    if last_rates != Some(current_rates) {
                        last_rates = Some(current_rates);

                        // Update status bar on macOS
                        status_bar::update_status_bar(&app_handle, upload_rate, download_rate);

                        // Windows taskbar toolbar speed band (adjacent to Nebulae application icon)
                        crate::taskbar_speed::update_taskbar_speed(
                            &app_handle,
                            upload_rate,
                            download_rate,
                        );
                    }
                    status_bar::ensure_displayed(&app_handle);
                }
                Ok(Err(_)) | Err(_) => {
                    last_emitted_torrents = None;
                    last_rates = None;
                    // Clear stale rates on engine stop or failure
                    status_bar::forget_rates();
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

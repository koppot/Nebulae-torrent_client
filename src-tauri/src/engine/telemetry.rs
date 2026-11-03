use crate::engine::SharedEngine;
use crate::status_bar;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn spawn_telemetry_loop(app_handle: AppHandle, engine: SharedEngine) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;

            if let Ok(torrents) = engine.list_torrents() {
                let _ = app_handle.emit("nebulae://telemetry", &torrents);

                // Calculate aggregate transfer rates for status bar
                let (upload_rate, download_rate) =
                    torrents.iter().fold((0u64, 0u64), |acc, torrent| {
                        (acc.0 + torrent.upload_rate, acc.1 + torrent.download_rate)
                    });

                // Update status bar on macOS
                status_bar::update_status_bar(&app_handle, upload_rate, download_rate);
            }
        }
    });
}

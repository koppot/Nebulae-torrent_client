use crate::engine::SharedEngine;
use crate::status_bar;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn spawn_telemetry_loop(app_handle: AppHandle, engine: SharedEngine) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        // Windows-only: tray speed readout driven by the SAME telemetry tick
        // (no duplicate polling, no since-start averages). State lives in this
        // single async task, so no locking is needed.
        #[cfg(target_os = "windows")]
        let mut tray_speed = crate::tray_speed::TraySpeedState::new();
        loop {
            interval.tick().await;

            if let Ok(torrents) = engine.list_torrents() {
                let _ = app_handle.emit("nebulae://telemetry", &torrents);

                // Calculate aggregate transfer rates for macOS status bar
                let (upload_rate, download_rate) =
                    torrents.iter().fold((0u64, 0u64), |acc, torrent| {
                        (acc.0 + torrent.upload_rate, acc.1 + torrent.download_rate)
                    });

                // Update status bar on macOS
                status_bar::update_status_bar(&app_handle, upload_rate, download_rate);

                // Windows-only: tray title speed readout
                #[cfg(target_os = "windows")]
                crate::tray_speed::update_tray_title(&app_handle, &torrents, &mut tray_speed);
            }
        }
    });
}

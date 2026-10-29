use crate::engine::SharedEngine;
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
                #[cfg(target_os = "windows")]
                crate::tray_speed::update_tray_title(&app_handle, &torrents, &mut tray_speed);
            }
        }
    });
}

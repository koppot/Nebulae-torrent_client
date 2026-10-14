use crate::engine::SharedEngine;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn spawn_telemetry_loop(app_handle: AppHandle, engine: SharedEngine) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;

            if let Ok(torrents) = engine.list_torrents() {
                let _ = app_handle.emit("nebulae://telemetry", &torrents);
            }
        }
    });
}

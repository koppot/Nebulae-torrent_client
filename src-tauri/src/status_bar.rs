use crate::engine::SharedEngine;
use std::time::Instant;
use tauri::AppHandle;
use tracing::info;

/// Global state for status bar management
pub(crate) struct StatusBarState {
    last_update: Instant,
    tray_id: Option<String>,
}

impl StatusBarState {
    fn new() -> Self {
        Self {
            last_update: Instant::now(),
            tray_id: None,
        }
    }
}

/// Thread-safe wrapper for status bar state
#[cfg(target_os = "macos")]
pub(crate) static STATUS_BAR_STATE: std::sync::OnceLock<
    std::sync::Arc<std::sync::Mutex<StatusBarState>>,
> = std::sync::OnceLock::new();

/// Initialize the status bar tracking state
pub fn init_status_bar(_app_handle: &AppHandle) {
    STATUS_BAR_STATE
        .get_or_init(|| std::sync::Arc::new(std::sync::Mutex::new(StatusBarState::new())));
}

/// Set the tray ID from the actual tray icon
pub fn set_tray_id(tray_id: String) {
    if let Some(s) = STATUS_BAR_STATE.get() {
        let mut s = s.lock().unwrap();
        s.tray_id = Some(tray_id);
    }
}

/// Update tray icon title with current rates (bytes per second converted to KB/s)
/// Uses two-line format: upload on top, download on bottom
pub fn update_status_bar(app_handle: &AppHandle, upload_rate: u64, download_rate: u64) {
    #[cfg(target_os = "macos")]
    {
        // Get the tray ID - use the stored one or default to "nebulae-tray"
        let tray_id_opt = {
            let s = STATUS_BAR_STATE.get();
            s.and_then(|s| s.lock().ok())
                .and_then(|s| s.tray_id.clone())
        };

        // Early return if no tray is configured yet
        let tray_id = match tray_id_opt {
            Some(id) => id,
            None => {
                info!("[status_bar] Tray not configured yet, skipping update");
                return;
            }
        };

        info!(
            "[status_bar] Attempting to update tray '{}' with rates: upload={:?} download={:?}",
            tray_id, upload_rate, download_rate
        );

        // Convert bytes/s to KB/s
        let upload_kib = upload_rate as f64 / 1024.0;
        let download_kib = download_rate as f64 / 1024.0;

        // Throttle updates: no more than once per second
        let state = STATUS_BAR_STATE.get();
        let now = Instant::now();
        let should_update = if let Some(s) = state {
            match s.lock() {
                Ok(mut s) => {
                    let elapsed = now.duration_since(s.last_update).as_secs();
                    s.last_update = now;
                    elapsed >= 1
                }
                Err(_) => false,
            }
        } else {
            true
        };

        if !should_update {
            return;
        }

        // Use unicode arrows with line break for two-line display
        // Format: ▲ upload_rate KB/s\n▼ download_rate KB/s
        // Using KB/s with one decimal place for compact adaptive display
        let upload_text = format!("{:.1} KB/s", upload_kib);
        let download_text = format!("{:.1} KB/s", download_kib);
        // Line break between upload and download
        let text = format!("▲ {}\n▼ {}", upload_text, download_text);

        // Update tray icon title - this displays text beside the icon on macOS
        if let Some(tray) = app_handle.tray_by_id(&tray_id) {
            if let Err(e) = tray.set_title(Some(&text)) {
                info!("[status_bar] set_title failed: {}", e);
            }
        } else {
            info!("[status_bar] Tray not found");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
        let _upload_rate = upload_rate;
        let _download_rate = download_rate;
    }
}

/// Clear the status bar text - called on idle or engine stop
pub fn clear_status_bar(app_handle: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        // Clear title, preserve icon
        let tray_id = {
            let s = STATUS_BAR_STATE.get().and_then(|s| s.lock().ok());
            s.and_then(|s| s.tray_id.clone())
                .unwrap_or_else(|| "nebulae-tray".to_string())
        };
        info!("[status_bar] clearing status");
        if let Some(tray) = app_handle.tray_by_id(&tray_id) {
            if let Err(e) = tray.set_title(Some("")) {
                info!("[status_bar] clear_status_bar set_title failed: {}", e);
            }
        } else {
            info!("[status_bar] Tray not found for clear");
        }
    }
}

/// Start the status bar telemetry loop on macOS (kept for compatibility but unused)
pub fn spawn_status_bar_loop(_app_handle: AppHandle, _engine: SharedEngine) {
    #[cfg(target_os = "macos")]
    {
        // Status bar updates are now driven by the telemetry loop (500ms, throttled to 1s)
        // This function is kept as a placeholder but does nothing
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = _app_handle;
        let _ = _engine;
    }
}

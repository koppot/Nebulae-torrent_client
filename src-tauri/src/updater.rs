use std::sync::Arc;

#[cfg(desktop)]
pub type StagedUpdatePayload = Option<(tauri_plugin_updater::Update, Vec<u8>)>;

#[cfg(desktop)]
#[derive(Clone, Default)]
pub struct StagedUpdateState(pub Arc<std::sync::Mutex<StagedUpdatePayload>>);

#[cfg(not(desktop))]
#[derive(Clone, Default)]
pub struct StagedUpdateState;

/// Spawns a silent, background auto-updater check and download.
/// Respects the user's `auto_update_enabled` preference with a strict early-return
/// before performing any network requests or DNS lookups when disabled.
#[cfg(desktop)]
pub fn spawn_silent_background_updater(app: &tauri::AppHandle, db: Arc<crate::db::Database>) {
    #[cfg(windows)]
    use tauri::Manager;
    use tauri_plugin_updater::UpdaterExt;
    use tracing::{debug, info, warn};

    // 1. Strict zero-telemetry check: verify toggle from SQLite settings BEFORE any network or DNS call!
    let auto_update_enabled = match db.get_setting("user_settings_json") {
        Ok(Some(json)) => {
            if let Ok(settings) =
                serde_json::from_str::<crate::commands::settings::UserSettings>(&json)
            {
                settings.auto_update_enabled
            } else {
                true
            }
        }
        _ => true,
    };

    if !auto_update_enabled {
        info!(
            "Silent auto-updater disabled in user settings; zero network requests or DNS queries will be made."
        );
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Build updater with on_before_exit hook
        let updater_res = handle
            .updater_builder()
            .on_before_exit(|| {
                info!("Running updater on_before_exit hook before installer execution.");
            })
            .build();

        let updater = match updater_res {
            Ok(u) => u,
            Err(e) => {
                warn!("Failed to initialize silent updater: {e}");
                return;
            }
        };

        debug!("Checking for silent signed release via GitHub Releases...");
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            Ok(None) => {
                debug!("Nebulae is up to date; no update available.");
                return;
            }
            Err(e) => {
                warn!("Silent update check failed: {e}");
                return;
            }
        };

        info!(
            "Silent update available: {} (current: {}). Downloading in background...",
            update.version, update.current_version
        );

        // Download silently in background without showing any UI or progress dialog
        let bytes = match update.download(|_chunk, _total| {}, || {}).await {
            Ok(b) => b,
            Err(e) => {
                warn!("Silent update download failed: {e}");
                return;
            }
        };

        info!(
            "Silent update downloaded and verified ({} bytes). Staging...",
            bytes.len()
        );

        #[cfg(not(windows))]
        {
            // On macOS / Linux AppImage, install() swaps/stages the application bundle or AppImage
            // on disk non-destructively while the current process keeps running until quit.
            if let Err(e) = update.install(&bytes) {
                warn!("Failed to install staged update: {e}");
            } else {
                info!("Silent update staged on disk. Will take effect on next launch.");
            }
        }

        #[cfg(windows)]
        {
            // On Windows, install() executes the NSIS quiet installer and terminates the process.
            // We stage the update in state so it is only triggered when the user naturally quits
            // the application, preventing any interruption of active torrent transfers.
            if let Some(state) = handle.try_state::<StagedUpdateState>() {
                if let Ok(mut lock) = state.0.lock() {
                    *lock = Some((update, bytes));
                    info!("Silent update staged in memory. Will apply automatically on app quit.");
                }
            }
        }
    });
}

#[cfg(not(desktop))]
pub fn spawn_silent_background_updater(
    _app: &tauri::AppHandle,
    _db: std::sync::Arc<crate::db::Database>,
) {
}

/// Applies staged update on natural application exit if an update is staged.
/// On Windows, this launches the quiet NSIS installer and completes the upgrade.
#[cfg(all(desktop, windows))]
pub fn apply_staged_update_if_present(app: &tauri::AppHandle) {
    use tauri::Manager;
    use tracing::info;
    if let Some(state) = app.try_state::<StagedUpdateState>() {
        if let Ok(mut lock) = state.0.lock() {
            if let Some((update, bytes)) = lock.take() {
                info!("Natural quit detected: executing staged Windows silent installer...");
                let _ = update.install(&bytes);
            }
        }
    }
}

#[cfg(not(all(desktop, windows)))]
pub fn apply_staged_update_if_present(_app: &tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staged_update_state_default() {
        let state = StagedUpdateState::default();
        #[cfg(desktop)]
        {
            let lock = state.0.lock().expect("Lock must succeed");
            assert!(lock.is_none());
        }
        let _ = state;
    }
}

use std::sync::Arc;

#[cfg(desktop)]
pub type StagedUpdatePayload = Option<(tauri_plugin_updater::Update, Vec<u8>)>;

#[cfg(desktop)]
#[derive(Clone, Default)]
pub struct StagedUpdateState(pub Arc<std::sync::Mutex<StagedUpdatePayload>>);

#[cfg(not(desktop))]
#[derive(Clone, Default)]
pub struct StagedUpdateState;

#[cfg(desktop)]
static UPDATER_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Spawns a silent, background auto-updater check and download.
/// Respects the user's `auto_update_enabled` preference with a strict early-return
/// before performing any network requests or DNS lookups when disabled.
/// Checks at startup (after a 10s backoff) and periodically every 4 hours while
/// the application remains running, using an atomic guard to prevent overlapping checks.
#[cfg(desktop)]
pub fn spawn_silent_background_updater(app: &tauri::AppHandle, db: Arc<crate::db::Database>) {
    let handle = app.clone();
    let db_clone = db.clone();
    tauri::async_runtime::spawn(async move {
        // Initial delay to avoid resource contention at boot
        #[cfg(feature = "test-updater")]
        let initial_delay = std::time::Duration::from_millis(500);
        #[cfg(not(feature = "test-updater"))]
        let initial_delay = std::time::Duration::from_secs(10);
        tokio::time::sleep(initial_delay).await;

        loop {
            run_single_silent_update_check(&handle, &db_clone).await;

            // Periodic check cadence: every 4 hours while the app remains open
            tokio::time::sleep(std::time::Duration::from_secs(4 * 3600)).await;
        }
    });
}

#[cfg(desktop)]
async fn run_single_silent_update_check(handle: &tauri::AppHandle, db: &Arc<crate::db::Database>) {
    #[cfg(windows)]
    use tauri::Manager;
    use tauri_plugin_updater::UpdaterExt;
    use tracing::{debug, info, warn};

    // Concurrency guard: prevent overlapping checks or downloads
    if UPDATER_IN_PROGRESS
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        debug!("Silent update check/download already in progress; skipping duplicate run.");
        return;
    }

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            UPDATER_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _guard = Guard;

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
        debug!(
            "Silent auto-updater disabled in user settings; zero network requests or DNS queries will be made."
        );
        return;
    }

    // Build updater with on_before_exit hook
    // In normal release builds, builder is strictly un-overridden:
    // HTTPS is strictly enforced by Tauri, endpoints come from tauri.conf.json,
    // and the trusted Minisign pubkey cannot be swapped by environment variables.
    #[allow(unused_mut)]
    let mut builder = handle.updater_builder();
    #[cfg(feature = "test-updater")]
    {
        let default_test_endpoint =
            "https://asiandvdclub.org/app-update-test-a7f92e4c91b5/latest.json";
        let endpoint_str = std::env::var("NEBULAE_UPDATER_TEST_ENDPOINT")
            .unwrap_or_else(|_| default_test_endpoint.to_string());
        if let Ok(url) = url::Url::parse(&endpoint_str) {
            match builder.endpoints(vec![url]) {
                Ok(b) => builder = b,
                Err(e) => {
                    warn!("Failed to override test updater endpoint: {e}");
                    return;
                }
            }
        }
        if let Ok(pubkey) = std::env::var("NEBULAE_UPDATER_TEST_PUBKEY") {
            builder = builder.pubkey(pubkey);
        }
    }

    let updater_res = builder
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

    debug!("Checking for silent signed release via updater endpoint...");
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            info!("Nebulae is up to date; no update available.");
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
            #[cfg(feature = "test-updater")]
            if std::env::var("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED").as_deref() == Ok("1") {
                info!("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED active: exiting cleanly to allow next launch verification.");
                handle.exit(0);
            }
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
                #[cfg(feature = "test-updater")]
                if std::env::var("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED").as_deref() == Ok("1") {
                    info!("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED active: exiting cleanly on Windows to trigger staged installer.");
                    apply_staged_update_if_present(handle);
                    handle.exit(0);
                }
            }
        }
    }
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

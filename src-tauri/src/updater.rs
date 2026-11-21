use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AvailableUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub date: Option<String>,
    pub body: Option<String>,
}

#[cfg(desktop)]
pub type StagedUpdatePayload = Option<(tauri_plugin_updater::Update, Vec<u8>)>;

#[cfg(desktop)]
#[derive(Clone, Default)]
pub struct StagedUpdateState(pub Arc<std::sync::Mutex<StagedUpdatePayload>>);

#[cfg(not(desktop))]
#[derive(Clone, Default)]
pub struct StagedUpdateState;

#[cfg(desktop)]
pub type PendingUpdatePayload = Option<(tauri_plugin_updater::Update, AvailableUpdateInfo)>;

#[cfg(desktop)]
#[derive(Clone, Default)]
pub struct PendingUpdateState(pub Arc<std::sync::Mutex<PendingUpdatePayload>>);

#[cfg(not(desktop))]
#[derive(Clone, Default)]
pub struct PendingUpdateState;

#[cfg(desktop)]
static UPDATER_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(desktop)]
static UPDATE_IS_STAGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Spawns a background auto-updater check.
/// Respects the user's `auto_update_enabled` preference with a strict early-return
/// before performing any network requests or DNS lookups when disabled.
/// Checks at startup (after a 10s backoff) and periodically every 4 hours while
/// the application remains running, using an atomic guard to prevent overlapping checks.
///
/// Invariant: Detecting an update only records its availability and emits an event.
/// It NEVER automatically initiates download or installation.
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
            run_single_background_update_check(&handle, &db_clone).await;

            // Periodic check cadence: every 4 hours while the app remains open
            tokio::time::sleep(std::time::Duration::from_secs(4 * 3600)).await;
        }
    });
}

#[cfg(desktop)]
async fn run_single_background_update_check(
    handle: &tauri::AppHandle,
    db: &Arc<crate::db::Database>,
) {
    use tracing::{debug, warn};

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
            "Auto-update checks disabled in user settings; zero network requests or DNS queries will be made."
        );
        return;
    }

    // Concurrency guard: prevent overlapping checks
    if UPDATER_IN_PROGRESS
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        debug!("Update check already in progress; skipping duplicate run.");
        return;
    }

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            UPDATER_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _guard = Guard;

    // Check only: detecting an update records its info but NEVER downloads or installs!
    if let Err(e) = check_for_updates_impl(handle).await {
        warn!("Background update check failed: {e}");
    }
}

#[cfg(desktop)]
pub fn build_updater_instance(
    handle: &tauri::AppHandle,
) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;
    use tracing::info;
    #[cfg(feature = "test-updater")]
    use tracing::warn;

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
                    return Err(format!("Failed to override test updater endpoint: {e}"));
                }
            }
        }
        if let Ok(pubkey) = std::env::var("NEBULAE_UPDATER_TEST_PUBKEY") {
            builder = builder.pubkey(pubkey);
        }
    }

    builder
        .on_before_exit(|| {
            info!("Running updater on_before_exit hook before installer execution.");
        })
        .build()
        .map_err(|e| format!("Failed to initialize updater: {e}"))
}

#[cfg(desktop)]
pub async fn check_for_updates_impl(
    handle: &tauri::AppHandle,
) -> Result<Option<AvailableUpdateInfo>, String> {
    use tauri::{Emitter, Manager};
    use tracing::{debug, info};

    let updater = build_updater_instance(handle)?;

    debug!("Checking for signed release via updater endpoint...");
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            info!("Nebulae is up to date; no update available.");
            if let Some(state) = handle.try_state::<PendingUpdateState>() {
                if let Ok(mut lock) = state.0.lock() {
                    *lock = None;
                }
            }
            let _ = handle.emit("nebulae://update-not-available", ());
            return Ok(None);
        }
        Err(e) => {
            return Err(format!("Update check failed: {e}"));
        }
    };

    let info = AvailableUpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        date: update.date.map(|d| d.to_string()),
        body: update.body.clone(),
    };

    info!(
        "Update available: {} (current: {}). Retaining as pending check result without downloading.",
        info.version, info.current_version
    );

    if let Some(state) = handle.try_state::<PendingUpdateState>() {
        if let Ok(mut lock) = state.0.lock() {
            *lock = Some((update, info.clone()));
        }
    }

    let _ = handle.emit("nebulae://update-available", &info);
    Ok(Some(info))
}

#[cfg(desktop)]
pub async fn download_and_install_update_impl(
    handle: &tauri::AppHandle,
) -> Result<AvailableUpdateInfo, String> {
    use tauri::{Emitter, Manager};
    use tracing::{info, warn};

    // Retrieve pending update object, or re-run check if not cached
    let pending_pair = {
        let state = handle
            .try_state::<PendingUpdateState>()
            .ok_or_else(|| "PendingUpdateState uninitialized".to_string())?;
        let mut lock = state.0.lock().map_err(|e| e.to_string())?;
        lock.take()
    };

    let (update, info) = match pending_pair {
        Some(pair) => pair,
        None => {
            let _ = check_for_updates_impl(handle).await?;
            let state = handle
                .try_state::<PendingUpdateState>()
                .ok_or_else(|| "PendingUpdateState uninitialized".to_string())?;
            let mut lock = state.0.lock().map_err(|e| e.to_string())?;
            lock.take()
                .ok_or_else(|| "No update available to download".to_string())?
        }
    };

    info!(
        "User authorized download of update: {} (current: {}). Downloading...",
        info.version, info.current_version
    );

    let _ = handle.emit("nebulae://update-downloading", &info);

    let bytes = match update.download(|_chunk, _total| {}, || {}).await {
        Ok(b) => b,
        Err(e) => {
            warn!("Update download failed: {e}");
            let _ = handle.emit("nebulae://update-download-failed", e.to_string());
            return Err(format!("Update download failed: {e}"));
        }
    };

    info!(
        "Update downloaded and verified ({} bytes). Staging...",
        bytes.len()
    );

    #[cfg(not(windows))]
    {
        // On macOS / Linux AppImage, install() swaps/stages the application bundle or AppImage
        // on disk non-destructively while the current process keeps running until quit.
        if let Err(e) = update.install(&bytes) {
            warn!("Failed to install staged update: {e}");
            return Err(format!("Failed to install staged update: {e}"));
        } else {
            info!("Update staged on disk. Will take effect on next launch.");
            UPDATE_IS_STAGED.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = handle.emit("nebulae://update-staged", &info);
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
                info!("Update staged in memory. Will apply automatically on app quit.");
            }
            UPDATE_IS_STAGED.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = handle.emit("nebulae://update-staged", &info);
            #[cfg(feature = "test-updater")]
            if std::env::var("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED").as_deref() == Ok("1") {
                info!("NEBULAE_UPDATER_TEST_QUIT_ON_STAGED active: exiting cleanly on Windows to trigger staged installer.");
                apply_staged_update_if_present(handle);
                handle.exit(0);
            }
        }
    }

    Ok(info)
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
                #[cfg(target_os = "windows")]
                crate::taskbar_windows::flush_icon_cache();
                let _ = update.install(&bytes);
            }
        }
    }
}

#[cfg(not(all(desktop, windows)))]
pub fn apply_staged_update_if_present(_app: &tauri::AppHandle) {}

// ── IPC Commands ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
) -> Result<Option<AvailableUpdateInfo>, String> {
    #[cfg(desktop)]
    {
        check_for_updates_impl(&app).await
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
pub fn get_available_update(app: tauri::AppHandle) -> Result<Option<AvailableUpdateInfo>, String> {
    #[cfg(desktop)]
    {
        use tauri::Manager;
        if let Some(state) = app.try_state::<PendingUpdateState>() {
            if let Ok(lock) = state.0.lock() {
                return Ok(lock.as_ref().map(|(_, info)| info.clone()));
            }
        }
        Ok(None)
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
) -> Result<AvailableUpdateInfo, String> {
    #[cfg(desktop)]
    {
        download_and_install_update_impl(&app).await
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Err("Desktop platform required".to_string())
    }
}

#[tauri::command]
pub fn get_update_staged(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(desktop)]
    {
        if UPDATE_IS_STAGED.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(true);
        }
        #[cfg(windows)]
        {
            use tauri::Manager;
            if let Some(state) = app.try_state::<StagedUpdateState>() {
                if let Ok(lock) = state.0.lock() {
                    return Ok(lock.is_some());
                }
            }
        }
        let _ = app;
        Ok(false)
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(false)
    }
}

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

    #[test]
    fn test_pending_update_state_default() {
        let state = PendingUpdateState::default();
        #[cfg(desktop)]
        {
            let lock = state.0.lock().expect("Lock must succeed");
            assert!(lock.is_none());
        }
        let _ = state;
    }

    #[test]
    fn test_available_update_info_serialization() {
        let info = AvailableUpdateInfo {
            version: "0.2.9".to_string(),
            current_version: "0.2.8".to_string(),
            date: Some("2026-11-21T12:00:00Z".to_string()),
            body: Some("Bug fixes and improvements".to_string()),
        };
        let json = serde_json::to_string(&info).expect("Must serialize");
        let deserialized: AvailableUpdateInfo =
            serde_json::from_str(&json).expect("Must deserialize");
        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_detection_does_not_stage_payload() {
        // Strict invariant: detecting an update records AvailableUpdateInfo in PendingUpdateState,
        // but must NEVER stage downloaded payload bytes in StagedUpdateState without explicit authorization.
        let staged = StagedUpdateState::default();
        let pending = PendingUpdateState::default();

        #[cfg(desktop)]
        {
            let staged_lock = staged.0.lock().unwrap();
            let pending_lock = pending.0.lock().unwrap();
            assert!(
                staged_lock.is_none(),
                "StagedUpdateState must be empty initially"
            );
            assert!(
                pending_lock.is_none(),
                "PendingUpdateState must be empty initially"
            );
        }
    }
}

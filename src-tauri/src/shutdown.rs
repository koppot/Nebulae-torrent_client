use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tracing::info;

static SHUTDOWN_INITIATED: AtomicBool = AtomicBool::new(false);

/// Returns true if an explicit application shutdown has been initiated.
pub fn is_shutting_down() -> bool {
    SHUTDOWN_INITIATED.load(Ordering::SeqCst)
}

/// Marks that an explicit application shutdown is underway.
pub fn mark_shutting_down() {
    SHUTDOWN_INITIATED.store(true, Ordering::SeqCst);
}

/// Shared, intentional shutdown path for Nebulae.
///
/// Guaranteed to execute at most once. Performs graceful teardown:
/// 1. Removes the tray/menu-bar icon.
/// 2. Stops active player session.
/// 3. Gracefully stops Transmission (saving torrent resume state with bounded wait).
/// 4. Gracefully stops Tailscale daemon.
/// 5. Flushes SQLite WAL to preserve settings and state.
/// 6. Applies staged auto-update payload if present.
/// 7. Exits the process cleanly without indefinite blocking.
pub fn initiate_shutdown(app: &tauri::AppHandle) {
    if SHUTDOWN_INITIATED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!("Initiating shared intentional shutdown...");

    // Safety watchdog: ensure process terminates within 5 seconds even if an external thread wedges
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
        std::process::exit(0);
    });

    // 1. Remove tray / menu-bar icon
    #[cfg(target_os = "macos")]
    {
        crate::status_bar::clear_status_bar(app);
    }
    let _ = app.remove_tray_by_id("nebulae-tray");

    // 2. Stop player session
    if let Some(player_state) = app.try_state::<crate::player::PlayerState>() {
        crate::commands::media::stop_session(app, &player_state);
    }

    // 3. Gracefully stop Transmission engine (bounded wait for resume data flush)
    if let Some(engine) = app.try_state::<crate::SharedEngine>() {
        engine.shutdown();
    }

    // 4. Gracefully stop Tailscale supervisor and daemon
    crate::tailscale::shutdown();

    // 5. Persist Database WAL
    if let Some(db) = app.try_state::<std::sync::Arc<crate::db::Database>>() {
        let _ = db.flush_wal();
    }

    // 6. Apply staged auto-update if present (e.g. Windows quiet NSIS staging)
    crate::updater::apply_staged_update_if_present(app);

    // 7. Clean exit
    app.exit(0);
    std::process::exit(0);
}

#[cfg(target_os = "macos")]
pub fn install_macos_terminate_hook(app_handle: tauri::AppHandle) {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    static HOOKED_APP: OnceLock<tauri::AppHandle> = OnceLock::new();
    let _ = HOOKED_APP.set(app_handle);

    extern "C" {
        fn objc_getClass(name: *const std::ffi::c_char) -> *mut c_void;
        fn sel_registerName(name: *const std::ffi::c_char) -> *mut c_void;
        fn class_getInstanceMethod(cls: *mut c_void, name: *mut c_void) -> *mut c_void;
        fn method_setImplementation(
            m: *mut c_void,
            imp: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
        ) -> *const c_void;
    }

    static ORIGINAL_TERMINATE: OnceLock<usize> = OnceLock::new();

    unsafe extern "C" fn hooked_terminate(
        this: *mut c_void,
        sel: *mut c_void,
        sender: *mut c_void,
    ) {
        tracing::info!("macOS [NSApplication terminate:] intercepted");
        if let Some(app) = HOOKED_APP.get() {
            crate::shutdown::initiate_shutdown(app);
        }
        if let Some(&orig_ptr) = ORIGINAL_TERMINATE.get() {
            if orig_ptr != 0 {
                let orig_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(orig_ptr);
                orig_fn(this, sel, sender);
            }
        }
    }

    unsafe {
        let cls_name = std::ffi::CString::new("NSApplication").unwrap();
        let cls = objc_getClass(cls_name.as_ptr());
        if cls.is_null() {
            return;
        }
        let sel_name = std::ffi::CString::new("terminate:").unwrap();
        let sel = sel_registerName(sel_name.as_ptr());
        if sel.is_null() {
            return;
        }
        let method = class_getInstanceMethod(cls, sel);
        if method.is_null() {
            return;
        }
        let old_imp = method_setImplementation(method, hooked_terminate);
        let _ = ORIGINAL_TERMINATE.set(old_imp as usize);
        tracing::info!("macOS [NSApplication terminate:] hook installed successfully");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_flag_lifecycle() {
        assert!(!is_shutting_down() || is_shutting_down()); // Valid bool read
        mark_shutting_down();
        assert!(is_shutting_down());
    }
}

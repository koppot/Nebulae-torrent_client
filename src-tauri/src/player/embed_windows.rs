//! Windows: native video overlay layered BELOW the webview.
//!
//! A plain black child window (STATIC class) is created beneath the
//! webview sibling; libVLC renders into it via `libvlc_media_player_set_hwnd`.
//! Because the overlay sits BELOW the transparent webview in Z-order,
//! all mouse events (clicks on player buttons, volume/seek tracks,
//! popovers, video pause/double-click fullscreen) and keyboard shortcuts
//! reach the frontend webview first. All work happens on Tauri's main
//! thread; the frontend drives position/size with `player_sync_viewport`.

use std::ffi::c_void;
use std::sync::mpsc;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SetWindowPos, ShowWindow, HWND_BOTTOM, SET_WINDOW_POS_FLAGS, SHOW_WINDOW_CMD,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOW, WINDOW_STYLE, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_VISIBLE,
};

/// Static AppHandle storage for cross-module use (EDR retry, etc.).
/// Set once by `overlay_app_init` during `ensure_overlay`.
static OVERLAY_APP: OnceLock<AppHandle> = OnceLock::new();

/// Returns the cached `AppHandle` if `ensure_overlay` has already run.
pub(crate) fn overlay_app_handle() -> Option<&'static AppHandle> {
    OVERLAY_APP.get()
}

/// Called from `ensure_overlay` to cache the `AppHandle` for later use
/// by the HDR/EDR retry infrastructure.
pub(crate) fn overlay_app_init(app: AppHandle) {
    let _ = OVERLAY_APP.set(app);
}

/// Windows equivalent of macOS `apply_edr_to_video_layer`.
///
/// On Windows there is no Core Animation layer to tag with a colorspace.
/// Instead we use libVLC's `var_SetChecked` to enforce the tone-mapping
/// variables on the media player object, ensuring the Hable filmic curve
/// is active for HDR/UHD content even if `set_hwnd` reset any
/// player-object variables.
pub(crate) unsafe fn apply_edr_to_video_layer(app: &AppHandle, is_hdr: bool) -> bool {
    let state = match app.try_state::<crate::player::PlayerState>() {
        Some(s) => s,
        None => return false,
    };
    let mp = state.media_player.lock().map(|p| *p).unwrap_or(0);
    if mp == 0 {
        return false;
    }
    let lib = match state.lib.lock() {
        Ok(l) => l.clone(),
        Err(_) => return false,
    };
    let Some(lib) = lib else {
        return false;
    };
    let var_set_checked = lib.var_set_checked;

    let mp_ptr = mp as *mut c_void;
    let cstr = |s: &str| std::ffi::CString::new(s).ok();

    if is_hdr {
        // tone-mapping = 3 (Hable filmic), VLC_VAR_INTEGER = 2
        if let Some(name) = cstr("tone-mapping") {
            let _ = var_set_checked(mp_ptr, name.as_ptr(), 2, 3i64 as usize);
        }
        // tone-mapping-param = 1.0, VLC_VAR_FLOAT = 4
        if let Some(name) = cstr("tone-mapping-param") {
            let _ = var_set_checked(mp_ptr, name.as_ptr(), 4, 1.0_f32.to_bits() as usize);
        }
        // tone-mapping-desat = 0.5, VLC_VAR_FLOAT = 4
        if let Some(name) = cstr("tone-mapping-desat") {
            let _ = var_set_checked(mp_ptr, name.as_ptr(), 4, 0.5_f32.to_bits() as usize);
        }
    }
    true
}

/// Poll-driven HDR retry for Windows. Ensures tone-mapping is applied to
/// the media player when HDR content is playing. Latched per session via
/// `edr_applied`; no timers or sleeps.
pub(crate) fn maybe_apply_edr(app: &AppHandle) {
    let state = match app.try_state::<crate::player::PlayerState>() {
        Some(s) => s,
        None => return,
    };
    if state.edr_applied.load(Ordering::SeqCst) {
        return;
    }
    if !state.is_hdr.load(Ordering::SeqCst) {
        return;
    }
    let mp = state.media_player.lock().map(|p| *p).unwrap_or(0);
    if mp == 0 {
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if unsafe { apply_edr_to_video_layer(&app2, true) } {
            app2.state::<crate::player::PlayerState>()
                .edr_applied
                .store(true, Ordering::SeqCst);
        }
    });
}

/// Force-apply HDR on Windows, resetting the latched state first.
pub(crate) fn force_apply_edr(app: &AppHandle) {
    let state = match app.try_state::<crate::player::PlayerState>() {
        Some(s) => s,
        None => return,
    };
    state.edr_applied.store(false, Ordering::SeqCst);
    if !state.is_hdr.load(Ordering::SeqCst) {
        return;
    }
    let mp = state.media_player.lock().map(|p| *p).unwrap_or(0);
    if mp == 0 {
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if unsafe { apply_edr_to_video_layer(&app2, true) } {
            app2.state::<crate::player::PlayerState>()
                .edr_applied
                .store(true, Ordering::SeqCst);
        }
    });
}

pub fn ensure_overlay(app: &AppHandle) -> Result<usize, String> {
    overlay_app_init(app.clone());
    let existing = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if existing != 0 {
        return Ok(existing);
    }

    let app2 = app.clone();
    let (tx, rx) = mpsc::channel::<Result<usize, String>>();
    app.run_on_main_thread(move || {
        let res = attach_main_window_child(&app2);
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    let hwnd = rx
        .recv()
        .map_err(|_| "main thread callback lost".to_string())??;

    app.state::<crate::player::PlayerState>()
        .overlay_view
        .store(hwnd, std::sync::atomic::Ordering::SeqCst);
    Ok(hwnd)
}

fn attach_main_window_child(app: &AppHandle) -> Result<usize, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let parent = win.hwnd().map_err(|e| format!("no parent HWND: {e}"))?;
    let parent_hwnd = HWND(parent.0);
    let (x, y, w, h) = {
        let size = win
            .inner_size()
            .map_err(|e| format!("no window size: {e}"))?;
        (0, 0, size.width as i32, size.height as i32)
    };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!(""),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
            x,
            y,
            w,
            h,
            Some(parent_hwnd),
            None,
            None,
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW failed: {e}"))?;
    // Place BELOW the webview sibling so the transparent webview receives all
    // mouse clicks (play, pause, seek, volume, controls, popovers) and keyboard input first.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
        );
    }
    let _ = win.set_focus();
    Ok(hwnd.0 as usize)
}

pub fn set_overlay_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    let hwnd = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if hwnd == 0 {
        return Ok(());
    }
    let cmd: SHOW_WINDOW_CMD = if hidden { SW_HIDE } else { SW_SHOW };
    let app2 = app.clone();
    app.run_on_main_thread(move || unsafe {
        let _ = ShowWindow(HWND(hwnd as _), cmd);
        if !hidden {
            let _ = SetWindowPos(
                HWND(hwnd as _),
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
            );
            if let Some(win) = app2.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

pub fn sync_frame(
    app: &AppHandle,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    _bar_height: f64,
) -> Result<(), String> {
    let hwnd = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if hwnd == 0 {
        return Ok(());
    }
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let scale = app2
            .get_webview_window("main")
            .and_then(|w| w.scale_factor().ok())
            .unwrap_or(1.0);
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd as _),
                Some(HWND_BOTTOM),
                (x * scale) as i32,
                (y * scale) as i32,
                (w * scale) as i32,
                (h * scale) as i32,
                SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
            );
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Rounded corners are macOS-specific (owner, Movist style); no-op here.
pub fn apply_window_corner_radius(_app: &AppHandle, _radius: f64) -> Result<(), String> {
    Ok(())
}

pub fn set_window_corner_radius(_app: &AppHandle, _radius: f64) -> Result<(), String> {
    Ok(())
}

/// Window drag: start the native caption-drag move loop ourselves.
///
/// tao's `drag_window` (0.37.1) Posts `WM_NCLBUTTONDOWN` with an LPARAM
/// that points to a stack local which is already gone when the modal
/// move loop reads it — the drag returns Ok(()) but the window never
/// moves (observed on the undecorated main window). Do the canonical
/// dance instead: release the WebView2's mouse capture, then SYNCHRONOUS
/// SendMessage the caption hit-test so the move loop runs with a valid
/// POINTS lparam on the main thread (native title-bar behavior; the
/// modal loop pumps messages, so the app stays responsive).
pub fn begin_window_drag(app: &AppHandle) -> Result<(), String> {
    use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
    };
    let hwnd = main_hwnd(app)?;
    // HWND wraps *mut c_void (not Send) and run_on_main_thread requires
    // FnOnce() + Send: ship the address as usize and rebuild the handle
    // on the main thread inside the closure.
    let hwnd_addr = hwnd.0 as usize;
    app.run_on_main_thread(move || unsafe {
        let hwnd = HWND(hwnd_addr as *mut core::ffi::c_void);
        let _ = ReleaseCapture();
        let mut pt = POINT { x: 0, y: 0 };
        let _ = GetCursorPos(&mut pt);
        // Win32 WM_NCLBUTTONDOWN lParam packs coordinates: LOWORD = x, HIWORD = y.
        // It is NOT a pointer to POINTS; passing a pointer passes a 64-bit stack address.
        let lparam = ((pt.x as u16 as u32) | ((pt.y as u16 as u32) << 16)) as isize;
        SendMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCAPTION as usize)),
            Some(LPARAM(lparam)),
        );
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Fullscreen via the Tauri window API (works for standard windows).
pub fn toggle_fullscreen(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let fs = win.is_fullscreen().unwrap_or(false);
        win.set_fullscreen(!fs)
            .map_err(|e| format!("fullscreen: {e}"))?;
        let hwnd = app
            .state::<crate::player::PlayerState>()
            .overlay_view
            .load(std::sync::atomic::Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd as _),
                    Some(HWND_BOTTOM),
                    0,
                    0,
                    0,
                    0,
                    SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
                );
            }
        }
        let _ = win.set_focus();
        use tauri::Emitter;
        let _ = app.emit("player://fullscreen-changed", !fs);
        return Ok(());
    }
    Err("main window not found".into())
}

/// Minimize via the Tauri window API.
pub fn miniaturize(app: &AppHandle) -> Result<(), String> {
    if minimize_to_tray_enabled(app) {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
            return Ok(());
        }
        return Err("main window not found".into());
    }
    if let Some(win) = app.get_webview_window("main") {
        return win.minimize().map_err(|e| format!("minimize: {e}"));
    }
    Err("main window not found".into())
}

/// Reads the `minimize_to_tray` user setting (defaults to true).
fn minimize_to_tray_enabled(app: &AppHandle) -> bool {
    app.try_state::<std::sync::Arc<crate::db::Database>>()
        .and_then(|db| db.get_setting("user_settings_json").ok().flatten())
        .and_then(|json| {
            serde_json::from_str::<crate::commands::settings::UserSettings>(&json).ok()
        })
        .map(|s| s.minimize_to_tray)
        .unwrap_or(true)
}

/// Native Win32 window controls: the Tauri JS window API is unreliable on
/// the undecorated main window, so Minimize/Maximize/Close go through
/// ShowWindow / WM_CLOSE on the real HWND.
fn main_hwnd(app: &AppHandle) -> Result<HWND, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    win.hwnd().map_err(|e| format!("hwnd: {e}"))
}

pub fn window_minimize_native(app: &AppHandle) -> Result<(), String> {
    if minimize_to_tray_enabled(app) {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
            return Ok(());
        }
        return Err("main window not found".into());
    }
    let hwnd = main_hwnd(app)?;
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
    Ok(())
}

pub fn window_maximize_toggle_native(app: &AppHandle) -> Result<(), String> {
    let hwnd = main_hwnd(app)?;
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{IsZoomed, SW_MAXIMIZE, SW_RESTORE};
        let zoomed = IsZoomed(hwnd).as_bool();
        let _ = ShowWindow(hwnd, if zoomed { SW_RESTORE } else { SW_MAXIMIZE });
    }
    Ok(())
}

pub fn window_close_native(app: &AppHandle) -> Result<(), String> {
    let hwnd = main_hwnd(app)?;
    unsafe {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    Ok(())
}

pub fn window_is_maximized_native(app: &AppHandle) -> Result<bool, String> {
    let hwnd = main_hwnd(app)?;
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::IsZoomed;
        Ok(IsZoomed(hwnd).as_bool())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_app_handle_returns_none_before_init() {
        // OVERLAY_APP is a OnceLock; before overlay_app_init is called it must be empty.
        assert!(overlay_app_handle().is_none());
    }

    #[test]
    fn tone_mapping_bits_match_vlc_expectations() {
        // VLC_VAR_INTEGER = 2, VLC_VAR_FLOAT = 4; tone-mapping values as shipped
        // in the instance-level VLC args (--tone-mapping=3, -param=1.0, -desat=0.5).
        assert_eq!(3i64 as usize, 3);
        assert_eq!(1.0_f32.to_bits() as usize, 0x3F80_0000);
        assert_eq!(0.5_f32.to_bits() as usize, 0x3F00_0000);
    }
}

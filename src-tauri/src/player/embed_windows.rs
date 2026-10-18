//! Windows: native video overlay as a child HWND of the main window.
//!
//! A plain black child window (STATIC class) is created on top of the
//! webview sibling; libVLC renders into it via
//! `libvlc_media_player_set_hwnd`. DVD/Blu-ray menu clicks land on the
//! child window natively. All work happens on Tauri's main thread; the
//! frontend drives position/size with `player_sync_viewport` (CSS px,
//! scaled by the window DPI factor here).

use std::sync::mpsc;
use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, MoveWindow, SetWindowPos, ShowWindow, HWND_TOP, SET_WINDOW_POS_FLAGS,
    SHOW_WINDOW_CMD, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOW, WINDOW_STYLE, WS_CHILD,
    WS_CLIPCHILDREN, WS_VISIBLE,
};

pub fn ensure_overlay(app: &AppHandle) -> Result<usize, String> {
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
            Default::default(),
            w!("STATIC"),
            w!(""),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0),
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
    // Raise above the webview sibling.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0),
        );
    }
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
    app.run_on_main_thread(move || unsafe {
        let _ = ShowWindow(HWND(hwnd as _), cmd);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

pub fn sync_frame(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, _bar_height: f64) -> Result<(), String> {
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
            let _ = MoveWindow(
                HWND(hwnd as _),
                (x * scale) as i32,
                (y * scale) as i32,
                (w * scale) as i32,
                (h * scale) as i32,
                true,
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

/// Window drag: Tauri's startDragging works on Windows.
pub fn begin_window_drag(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        return win.start_dragging().map_err(|e| format!("drag: {e}"));
    }
    Err("main window not found".into())
}

/// Fullscreen via the Tauri window API (works for standard windows).
pub fn toggle_fullscreen(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let fs = win.is_fullscreen().unwrap_or(false);
        return win.set_fullscreen(!fs).map_err(|e| format!("fullscreen: {e}"));
    }
    Err("main window not found".into())
}

/// Minimize via the Tauri window API.
pub fn miniaturize(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        return win.minimize().map_err(|e| format!("minimize: {e}"));
    }
    Err("main window not found".into())
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

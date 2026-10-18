//! Linux (X11/XWayland): native video overlay as an X11 child window.
//!
//! A black X11 subwindow is created inside the main window and raised above
//! the webkit view; libVLC renders into it via `set_xwindow`. Under native
//! Wayland there is no X11 drawable and libVLC 3 has no Wayland vout, so
//! `ensure_overlay` reports an error (there is no external-player fallback).
//!
//! All X11 work happens on Tauri's main thread so it can never race GTK's
//! event loop; the Xlib display handle is opened once and intentionally
//! leaked for the process lifetime.

use raw_window_handle::HasWindowHandle;
use std::sync::mpsc;
use tauri::{AppHandle, Manager};

struct XHandle {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    overlay: u64, // X11 Window id (x11-dl uses u64)
}

// The raw display pointer is only ever dereferenced inside main-thread
// closures, so the shared cell is safe to reach from any thread.
unsafe impl Send for XHandle {}
unsafe impl Sync for XHandle {}

// The Xlib calls are confined to the main thread, so the shared state is a
// Mutex-guarded cell instead of a static mut.
fn xcell() -> &'static std::sync::Mutex<Option<XHandle>> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<Option<XHandle>>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::Mutex::new(None))
}

fn with_x<R: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&XHandle) -> R + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = mpsc::channel::<Result<R, String>>();
    app.run_on_main_thread(move || {
        let res = match xcell().lock() {
            Ok(guard) => match guard.as_ref() {
                Some(xh) => Ok(f(xh)),
                None => Err("X11 overlay not initialized".to_string()),
            },
            Err(_) => Err("X11 overlay state poisoned".to_string()),
        };
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    rx.recv()
        .map_err(|_| "main thread callback lost".to_string())?
}

pub fn ensure_overlay(app: &AppHandle) -> Result<usize, String> {
    let existing = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if existing != 0 {
        return Ok(existing);
    }

    // Resolve the main window's X11 drawable first (fails under Wayland).
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let raw = win
        .window_handle()
        .map_err(|e| format!("no raw window handle: {e}"))?;
    let parent_xid = match raw.as_raw() {
        // raw-window-handle 0.6 names the X11 variant `Xlib`.
        raw_window_handle::RawWindowHandle::Xlib(h) => h.window,
        raw_window_handle::RawWindowHandle::Wayland(_) => {
            return Err(
                "Wayland session: no X11 drawable for the embedded overlay (use an X11/XWayland session)"
                    .to_string(),
            );
        }
        other => {
            return Err(format!("unsupported window handle: {other:?}"));
        }
    };

    let (tx, rx) = mpsc::channel::<Result<usize, String>>();
    app.run_on_main_thread(move || {
        let res = unsafe { create_overlay(parent_xid) };
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    let overlay_xid = rx
        .recv()
        .map_err(|_| "main thread callback lost".to_string())??;

    app.state::<crate::player::PlayerState>()
        .overlay_view
        .store(overlay_xid, std::sync::atomic::Ordering::SeqCst);
    Ok(overlay_xid)
}

unsafe fn create_overlay(parent_xid: u64) -> Result<usize, String> {
    let xlib = x11_dl::xlib::Xlib::open().map_err(|e| format!("libX11 unavailable: {e}"))?;
    let display = (xlib.XOpenDisplay)(std::ptr::null());
    if display.is_null() {
        return Err("XOpenDisplay failed".into());
    }
    let screen = (xlib.XDefaultScreen)(display);
    let overlay = (xlib.XCreateSimpleWindow)(
        display,
        parent_xid,
        0,
        0,
        1,
        1,
        0,
        (xlib.XBlackPixel)(display, screen),
        (xlib.XBlackPixel)(display, screen),
    );
    if overlay == 0 {
        return Err("XCreateSimpleWindow failed".into());
    }
    (xlib.XMapWindow)(display, overlay);
    (xlib.XRaiseWindow)(display, overlay);
    (xlib.XFlush)(display);
    match xcell().lock() {
        Ok(mut guard) => {
            *guard = Some(XHandle {
                xlib,
                display,
                overlay,
            });
        }
        Err(_) => return Err("X11 overlay state poisoned".into()),
    }
    Ok(overlay as usize)
}

pub fn set_overlay_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    with_x(app, move |xh| unsafe {
        if hidden {
            (xh.xlib.XUnmapWindow)(xh.display, xh.overlay);
        } else {
            (xh.xlib.XMapWindow)(xh.display, xh.overlay);
            (xh.xlib.XRaiseWindow)(xh.display, xh.overlay);
        }
        (xh.xlib.XFlush)(xh.display);
    })
    .map(|_| ())
}

pub fn sync_frame(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, _bar_height: f64) -> Result<(), String> {
    let scale = app
        .get_webview_window("main")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    with_x(app, move |xh| unsafe {
        (xh.xlib.XMoveResizeWindow)(
            xh.display,
            xh.overlay,
            (x * scale) as i32,
            (y * scale) as i32,
            (w * scale).max(0.0) as u32,
            (h * scale).max(0.0) as u32,
        );
        (xh.xlib.XFlush)(xh.display);
    })
    .map(|_| ())
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

/// Window controls via the Tauri window API (works on Linux).
pub fn window_minimize_native(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        return win.minimize().map_err(|e| format!("minimize: {e}"));
    }
    Err("main window not found".into())
}

pub fn window_maximize_toggle_native(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let zoomed = win.is_maximized().unwrap_or(false);
        if zoomed {
            return win.unmaximize().map_err(|e| format!("unmaximize: {e}"));
        }
        return win.maximize().map_err(|e| format!("maximize: {e}"));
    }
    Err("main window not found".into())
}

pub fn window_close_native(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        return win.close().map_err(|e| format!("close: {e}"));
    }
    Err("main window not found".into())
}

pub fn window_is_maximized_native(app: &AppHandle) -> Result<bool, String> {
    if let Some(win) = app.get_webview_window("main") {
        return win.is_maximized().map_err(|e| format!("is_maximized: {e}"));
    }
    Err("main window not found".into())
}

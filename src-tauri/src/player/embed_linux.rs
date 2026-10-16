//! Linux (X11/XWayland): native video overlay as an X11 child window.
//!
//! A black X11 subwindow is created inside the main window and raised above
//! the webkit view; libVLC renders into it via `set_xwindow`. Under native
//! Wayland there is no X11 drawable and libVLC 3 has no Wayland vout, so
//! `ensure_overlay` reports failure and playback falls back to launching
//! the user's VLC application.
//!
//! All X11 work happens on Tauri's main thread so it can never race GTK's
//! event loop; the Xlib display handle is opened once and intentionally
//! leaked for the process lifetime.

use std::sync::mpsc;
use raw_window_handle::HasWindowHandle;
use tauri::{AppHandle, Manager};

struct XHandle {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    overlay: u64, // X11 Window id
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

fn with_x<R>(
    app: &AppHandle,
    f: impl FnOnce(&XHandle) -> R + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = mpsc::channel::<Result<R, String>>();
    app.run_on_main_thread(move || {
        let res = match xcell().lock() {
            Ok(guard) => match guard.as_ref() {
                Some(x) => Ok(f(x)),
                None => Err("X11 overlay not initialized".to_string()),
            },
            Err(_) => Err("X11 overlay state poisoned".to_string()),
        };
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    rx.recv().map_err(|_| "main thread callback lost".to_string())?
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
    let xid = match raw.as_raw() {
        raw_window_handle::RawWindowHandle::X11(h) => h.window.get(),
        raw_window_handle::RawWindowHandle::Wayland(_) => {
            return Err("Wayland session: embedded overlay unavailable".to_string())
        }
        other => {
            return Err(format!("unsupported window handle: {other:?}"));
        }
    };

    let app2 = app.clone();
    let (tx, rx) = mpsc::channel::<Result<usize, String>>();
    app.run_on_main_thread(move || {
        let res = unsafe { create_overlay(&app2, xid) };
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    let xid_overlay = rx
        .recv()
        .map_err(|_| "main thread callback lost".to_string())??;

    app.state::<crate::player::PlayerState>()
        .overlay_view
        .store(xid_overlay, std::sync::atomic::Ordering::SeqCst);
    Ok(xid_overlay)
}

unsafe fn create_overlay(_app: &AppHandle, parent_xid: u64) -> Result<usize, String> {
    let xlib = x11_dl::xlib::Xlib::open()
        .map_err(|e| format!("libX11 unavailable: {e}"))?;
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
    Ok(overlay)
}

pub fn set_overlay_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    with_x(app, |x| unsafe {
        if hidden {
            (x.xlib.XUnmapWindow)(x.display, x.overlay);
        } else {
            (x.xlib.XMapWindow)(x.display, x.overlay);
            (x.xlib.XRaiseWindow)(x.display, x.overlay);
        }
        (x.xlib.XFlush)(x.display);
    })
    .map(|_| ())
}

pub fn sync_frame(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let scale = app
        .get_webview_window("main")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    with_x(app, move |x| unsafe {
        (x.xlib.XMoveResizeWindow)(
            x.display,
            x.overlay,
            (x * scale) as i32,
            (y * scale) as i32,
            (w * scale) as i32,
            (h * scale) as i32,
        );
        (x.xlib.XFlush)(x.display);
    })
    .map(|_| ())
}

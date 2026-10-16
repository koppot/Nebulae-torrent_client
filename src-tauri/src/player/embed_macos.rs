//! macOS: native video overlay layered above the webview.
//!
//! The overlay is a plain black NSView added to the main window's content
//! view positioned above the WKWebView. libVLC renders the video INTO this
//! view (`libvlc_media_player_set_nsobject`), so playback happens inside
//! Nebulae's own window. Menu clicks on DVDs/Blu-rays land on the video
//! view itself and are handled by dvdnav/libbluray natively.
//!
//! All view work happens on the main thread (Tauri's `run_on_main_thread`);
//! the frontend drives position/size with `player_sync_viewport`.

use objc2::rc::Retained;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::sync::mpsc;
use tauri::{AppHandle, Manager};

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
        let res = attach_main_window_view(&app2);
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    let view_ptr = rx
        .recv()
        .map_err(|_| "main thread callback lost".to_string())??;

    app.state::<crate::player::PlayerState>()
        .overlay_view
        .store(view_ptr, std::sync::atomic::Ordering::SeqCst);
    Ok(view_ptr)
}

fn attach_main_window_view(app: &AppHandle) -> Result<usize, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let raw = win.ns_window().map_err(|e| format!("no NSWindow: {e}"))?;
    unsafe {
        let ns_win: *mut NSWindow = raw as *mut NSWindow;
        if ns_win.is_null() {
            return Err("NSWindow pointer is null".into());
        }
        let content = (*ns_win)
            .contentView()
            .ok_or_else(|| "window has no contentView".to_string())?;
        let bounds = content.bounds();
        let mtm = MainThreadMarker::new().ok_or("not on main thread")?;
        let view: Retained<NSView> = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        view.setWantsLayer(true);
        view.setHidden(true);
        content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Above, None);
        // Intentionally leak one overlay view for the process lifetime;
        // repeated play/stop reuses it.
        Ok(Retained::into_raw(view) as usize)
    }
}

/// Show/hide the overlay without tearing the view down.
pub fn set_overlay_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    let view_ptr = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if view_ptr == 0 {
        return Ok(());
    }
    app.run_on_main_thread(move || unsafe {
        let view = view_ptr as *mut NSView;
        if !view.is_null() {
            (*view).setHidden(hidden);
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Position the overlay to match a frontend placeholder rect.
///
/// `x`/`y` are CSS-pixel (logical point) coordinates relative to the TOP of
/// the window viewport, as reported by the frontend; Cocoa wants bottom-left
/// origin, so flip using the superview (content view) height.
pub fn sync_frame(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let view_ptr = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if view_ptr == 0 {
        return Ok(());
    }
    app.run_on_main_thread(move || unsafe {
        let view = view_ptr as *mut NSView;
        if view.is_null() {
            return;
        }
        let superview = match (*view).superview() {
            Some(s) => s,
            None => return,
        };
        let content_height = superview.frame().size.height;
        let rect = NSRect::new(NSPoint::new(x, content_height - y - h), NSSize::new(w, h));
        (*view).setFrame(rect);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

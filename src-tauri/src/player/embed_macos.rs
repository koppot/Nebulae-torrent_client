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
use objc2_app_kit::{NSEvent, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::sync::mpsc;
use tauri::{AppHandle, Manager};

// The overlay NSView sits ABOVE the webview, so mouse and keyboard events
// over the video never reach the frontend (owner-reported: dragging,
// double-click fullscreen and keyboard controls were all dead). The view
// is therefore a real subclass:
//   - single click-drag  -> native window drag (performWindowDragWithEvent,
//     the standard borderless-window mechanism);
//   - double click       -> toggle macOS fullscreen;
//   - keyDown            -> forwarded to the frontend ("player-native-key"
//     event) where one unified handler applies the shortcuts (menu
//     navigation, pause, seek, volume, fullscreen).
objc2::define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "NebulaeVideoOverlayView"]
    struct VideoOverlayView;

    impl VideoOverlayView {
        #[unsafe(method(mouseDown:))]
        unsafe fn mouse_down(this: &VideoOverlayView, event: &NSEvent) {
            if event.clickCount() >= 2 {
                // Native fullscreen toggle — works for borderless windows,
                // unlike the window-manager route that failed in testing.
                if let Some(win) = this.window() {
                    let _: () = objc2::msg_send![&*win, toggleFullScreen: std::ptr::null::<objc2::runtime::AnyObject>()];
                }
            } else if let Some(win) = this.window() {
                // Become first responder so keyDown reaches us (keyboard
                // controls).
                let _: bool = objc2::msg_send![&*win, makeFirstResponder: this];
                let _: () = objc2::msg_send![&*win, performWindowDragWithEvent: event];
            }
        }

        #[unsafe(method(keyDown:))]
        unsafe fn key_down(this: &VideoOverlayView, event: &NSEvent) {
            let _ = this;
            if let Some(app) = overlay_app() {
                if let Some(key) = native_key_name(event) {
                    use tauri::Emitter;
                    let _ = app.emit("player-native-key", key);
                }
            }
        }

        #[unsafe(method(acceptsFirstResponder))]
        unsafe fn accepts_first_responder(this: &VideoOverlayView) -> bool {
            let _ = this;
            true // receive keyDown when the user interacts with the video
        }
    }
);

fn overlay_app() -> Option<&'static AppHandle> {
    static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
    APP.get()
}

fn overlay_app_init(app: AppHandle) {
    static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
    let _ = APP.set(app);
}

/// Translate a native key event into the frontend's key-name vocabulary.
fn native_key_name(event: &NSEvent) -> Option<String> {
    let chars = event.charactersIgnoringModifiers()?.to_string();
    match chars.as_str() {
        " " => Some(" ".to_string()),
        "\r" => Some("Enter".to_string()),
        "\x1b" => Some("Escape".to_string()),
        other => Some(other.to_string()),
    }
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
        let view: Retained<VideoOverlayView> =
            objc2::msg_send![VideoOverlayView::alloc(mtm), init];
        view.setFrame(bounds);
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
            if !hidden {
                // Take keyboard focus so arrow keys / space / F / M reach
                // the overlay's keyDown handler (keyboard controls).
                if let Some(win) = (*view).window() {
                    let _: bool = objc2::msg_send![&*win, makeFirstResponder: view];
                }
            }
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

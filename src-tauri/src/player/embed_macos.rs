//! macOS: native video overlay layered BELOW the webview.
//!
//! The overlay is a plain black NSView added to the main window's content
//! view positioned below the WKWebView. libVLC renders the video INTO this
//! view (`libvlc_media_player_set_nsobject`), so playback happens inside
//! Nebulae's own window. The window and webview are transparent while the
//! player is up (the frontend unmounts its opaque shell), so the video
//! shows through and the frontend's floating control bar draws over it.
//!
//! NOTE: the view must NOT be layer-backed — libVLC's `caopengllayer` vout
//! attaches its own layer to the view and refuses/breaks on views that
//! already manage a layer (observed: video renders black).
//!
//! All view work happens on the main thread (Tauri's `run_on_main_thread`);
//! the frontend drives position/size with `player_sync_viewport`.

use objc2::rc::Retained;
use objc2::{ClassType, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use tauri::{AppHandle, Manager};

// The overlay NSView sits BELOW the webview, so mouse and keyboard events
// reach the frontend first; the native subclass stays as a fallback for
// first-responder keyboard input and provides native drag/fullscreen when
// the video view is the responder (e.g. after a menu-click handoff).
objc2::define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "NebulaeVideoOverlayView"]
    struct VideoOverlayView;

    impl VideoOverlayView {
        #[unsafe(method(mouseDown:))]
        unsafe fn mouse_down(this: &VideoOverlayView, event: &NSEvent) {
            if event.clickCount() >= 2 {
                // Borderless windows ignore toggleFullScreen: unless their
                // collection behavior advertises fullscreen support, so use
                // the manual frame/style-mask toggle.
                if let Some(win) = this.window() {
                    toggle_fullscreen_win(&win);
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

        #[unsafe(method(hitTest:))]
        unsafe fn hit_test(this: &VideoOverlayView, point: NSPoint) -> *mut NSView {
            let _ = this;
            let _ = point;
            // The view sits BELOW the webview; webview hit-testing wins.
            objc2::msg_send![this, self]
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

/// Saved window frame for leaving fullscreen (boxed NSRect pointer).
static SAVED_BOX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Enter/leave fullscreen for the borderless player window. The native
/// toggleFullScreen: animation ignores borderless windows, so the frame
/// and style mask are toggled manually.
unsafe fn toggle_fullscreen_win(win: &NSWindow) {
    let mask: u64 = objc2::msg_send![win, styleMask];
    if mask & NS_WINDOW_STYLE_FULLSCREEN != 0 {
        // Leaving fullscreen: restore the saved frame.
        let raw = SAVED_BOX.load(Ordering::SeqCst) as *mut NSRect;
        if !raw.is_null() {
            let f = *raw;
            let _: () = objc2::msg_send![win, setFrame:f display:true];
        }
        let _: () = objc2::msg_send![win, setStyleMask: mask & !NS_WINDOW_STYLE_FULLSCREEN];
    } else {
        let ptr = Box::into_raw(Box::new(win.frame()));
        let old = SAVED_BOX.swap(ptr as usize, Ordering::SeqCst) as *mut NSRect;
        if !old.is_null() {
            let _ = Box::from_raw(old);
        }
        let _: () = objc2::msg_send![win, setStyleMask: mask | NS_WINDOW_STYLE_FULLSCREEN];
        if let Some(screen) = win.screen() {
            let f = screen.frame();
            let _: () = objc2::msg_send![win, setFrame:f display:true];
        }
    }
}

const NS_WINDOW_STYLE_FULLSCREEN: u64 = 1 << 14;

/// Toggle fullscreen for the player window (F key / fullscreen button /
/// double-click). Sets the corner radius to 0 in fullscreen, 3 px out.
pub fn toggle_fullscreen(app: &AppHandle) -> Result<(), String> {
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let Some(win) = app2.get_webview_window("main") else {
            return;
        };
        let Ok(raw) = win.ns_window() else {
            return;
        };
        unsafe {
            let ns_win = raw as *mut NSWindow;
            if ns_win.is_null() {
                return;
            }
            toggle_fullscreen_win(&*ns_win);
            let mask: u64 = objc2::msg_send![ns_win, styleMask];
            let radius = if mask & NS_WINDOW_STYLE_FULLSCREEN != 0 { 0.0 } else { 3.0 };
            apply_window_corner_radius(&app2, radius);
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Begin a native window drag. Tauri's startDragging (async IPC) fails to
/// move borderless windows on current macOS; a synthetic left-mouse-down
/// handed to performWindowDragWithEvent on the MAIN thread works reliably.
pub fn begin_window_drag(app: &AppHandle) -> Result<(), String> {
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let Some(win) = app2.get_webview_window("main") else {
            return;
        };
        let Ok(raw) = win.ns_window() else {
            return;
        };
        unsafe {
            let ns_win = raw as *mut NSWindow;
            if ns_win.is_null() {
                return;
            }
            let start: NSPoint = objc2::msg_send![objc2_app_kit::NSEvent::class(), mouseLocation];
            let origin = (*ns_win).frame().origin;
            // Track the pointer on a worker thread; each tick moves the
            // window on the main thread until the left button is released.
            let win_addr = ns_win as usize;
            std::thread::spawn(move || {
                let ns_win = win_addr as *mut NSWindow;
                let mut last_x = start.x;
                let mut last_y = start.y;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    let now: NSPoint =
                        unsafe { objc2::msg_send![objc2_app_kit::NSEvent::class(), mouseLocation] };
                    let buttons: usize = unsafe {
                        objc2::msg_send![objc2_app_kit::NSEvent::class(), pressedMouseButtons]
                    };
                    if buttons & 1 == 0 {
                        break;
                    }
                    let dx = now.x - last_x;
                    let dy = now.y - last_y;
                    last_x = now.x;
                    last_y = now.y;
                    if dx == 0.0 && dy == 0.0 {
                        continue;
                    }
                    let app3 = app2.clone();
                    let win_addr = ns_win as usize;
                    let _ = app3.run_on_main_thread(move || unsafe {
                        let ns_win = win_addr as *mut NSWindow;
                        let o = (*ns_win).frame().origin;
                        // Screen coords are bottom-left origin; Cocoa delta
                        // already matches, but the trackpad y grows upward in
                        // NSEvent coords while the window origin does too, so
                        // subtract to follow the pointer downward.
                        (*ns_win).setFrameOrigin(NSPoint::new(o.x + dx, o.y + dy));
                    });
                }
                let _ = origin;
            });
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

fn attach_main_window_view(app: &AppHandle) -> Result<usize, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let raw = win.ns_window().map_err(|e| format!("no NSWindow: {e}"))?;
    let webview_raw = win.ns_view().map_err(|e| format!("no NSView: {e}"))?;
    unsafe {
        let ns_win: *mut NSWindow = raw as *mut NSWindow;
        if ns_win.is_null() {
            return Err("NSWindow pointer is null".into());
        }
        let content = (*ns_win)
            .contentView()
            .ok_or_else(|| "window has no contentView".to_string())?;
        // Slightly rounded window corners (owner): the borderless window is
        // square by default; rounding the content view's layer clips the
        // webview and the video overlay to the same radius.
        content.setWantsLayer(true);
        if let Some(layer) = content.layer() {
            layer.setCornerRadius(3.0);
            layer.setMasksToBounds(true);
        }
        let bounds = content.bounds();
        let mtm = MainThreadMarker::new().ok_or("not on main thread")?;
        let view: Retained<VideoOverlayView> =
            objc2::msg_send![VideoOverlayView::alloc(mtm), init];
        view.setFrame(bounds);
        view.setHidden(true);
        // The video view sits BELOW the webview (owner: controls must float
        // OVER the video). The window and webview are transparent while the
        // player is up, so the video shows through and the frontend's
        // floating control bar draws on top.
        let webview_ptr = webview_raw as *mut NSView;
        assert!(!webview_ptr.is_null(), "webview NSView is null");
        let webview = &*webview_ptr;
        content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(webview));
        // Intentionally leak one overlay view for the process lifetime;
        // repeated play/stop reuses it.
        Ok(Retained::into_raw(view) as usize)
    }
}

/// Round the window corners (owner): 4 px for the media/manager UI, 3 px
/// while the player is up, 0 in fullscreen.
pub fn set_window_corner_radius(app: &AppHandle, radius: f64) -> Result<(), String> {
    let app2 = app.clone();
    app.run_on_main_thread(move || unsafe {
        apply_window_corner_radius(&app2, radius);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

unsafe fn apply_window_corner_radius(app: &AppHandle, radius: f64) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let Ok(raw) = win.ns_window() else {
        return;
    };
    let ns_win = raw as *mut NSWindow;
    let Some(content) = (*ns_win).contentView() else {
        return;
    };
    content.setWantsLayer(true);
    if let Some(layer) = content.layer() {
        layer.setCornerRadius(radius);
        layer.setMasksToBounds(true);
    }
}

/// Show/hide the overlay without tearing the view down.
///
/// Waits for the main thread to apply the change: callers are the async
/// VLC lifecycle commands, and the vout must never attach to a view that
/// is still hidden (a dead black rectangle). Never call from the main
/// thread itself — the wait would deadlock.
pub fn set_overlay_hidden(app: &AppHandle, hidden: bool) -> Result<(), String> {
    let view_ptr = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if view_ptr == 0 {
        return Ok(());
    }
    let app2 = app.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    app.run_on_main_thread(move || unsafe {
        let view = view_ptr as *mut NSView;
        if !view.is_null() {
            (*view).setHidden(hidden);
            // Player window: 3 px corners; media/manager UI: 4 px corners.
            apply_window_corner_radius(&app2, if hidden { 4.0 } else { 3.0 });
            // The video view always covers the whole window; the frontend's
            // floating bar draws above it. Full-window frame here guarantees
            // sane geometry even before the first frontend sync arrives.
            if let Some(win) = (*view).window() {
                if let Some(content) = win.contentView() {
                    let b = content.bounds();
                    (*view).setFrame(b);
                }
            }
        }
        let _ = tx.send(());
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    rx.recv()
        .map_err(|_| "main thread callback lost".to_string())
}

/// Position the overlay to match a frontend placeholder rect.
///
/// `x`/`y` are CSS-pixel (logical point) coordinates relative to the TOP of
/// the window viewport, as reported by the frontend; Cocoa wants bottom-left
/// origin, so flip using the superview (content view) height. `bar_height`
/// is accepted for API compatibility; the floating control bar is webview
/// content drawn above the video, so no native cutout is needed.
pub fn sync_frame(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, _bar_height: f64) -> Result<(), String> {
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
        tracing::debug!(
            "sync_frame: viewport=({x},{y},{w}x{h}) -> rect=({:?},{:?},{:?}x{:?})",
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height
        );
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

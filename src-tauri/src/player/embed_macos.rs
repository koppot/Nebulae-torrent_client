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
//!
//! HDR: When the window is on an Apple Silicon XDR display, the content
//! view's layer has `wantsExtendedDynamicRangeContent = YES` enabled so
//! the underlying caopengllayer can emit values above 1.0. The window
//! listens for screen changes and re-applies this when the window moves
//! between displays.

use objc2::rc::Retained;
use objc2::{class, sel, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSView, NSWindow, NSWindowOrderingMode, NSWindowDidChangeScreenNotification};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize};
use objc2_quartz_core::{CALayer, CADynamicRangeHigh};
use std::sync::atomic::Ordering;
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
                    // Notify the frontend — same event emitted by the public
                    // toggle_fullscreen() path (button / F key).
                    if let Some(app) = overlay_app() {
                        use tauri::Emitter;
                        let is_fs = FULLSCREEN_ON.load(Ordering::SeqCst);
                        let _ = app.emit("player://fullscreen-changed", is_fs);
                    }
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

// Single module-level storage for the AppHandle used by native key forwarding.
// Two function-local statics with the same name are distinct memory — using
// separate locals means overlay_app_init writes one cell and overlay_app reads
// a different empty cell, so overlay_app always returns None.
static OVERLAY_APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

fn overlay_app() -> Option<&'static AppHandle> {
    OVERLAY_APP.get()
}

fn overlay_app_init(app: AppHandle) {
    let _ = OVERLAY_APP.set(app);
}

/// Apply EDR (Extended Dynamic Range) settings to a CALayer.
///
/// Tries `preferredDynamicRange = CADynamicRangeHigh` first via
/// `respondsToSelector: sel!(setPreferredDynamicRange:)`. The selector is
/// documented for macOS 26+; the guard makes the call safe on older systems
/// where it simply returns false and we fall back to the deprecated
/// `wantsExtendedDynamicRangeContent`.
/// Source: MacLC 1.3, modules/video_output/apple/VLCSampleBufferDisplay.m:749-765
pub(crate) unsafe fn apply_edr_to_layer(layer: &CALayer) {
    // Try the modern API first. `setPreferredDynamicRange:` selector is
    // documented for macOS 26+. The respondsToSelector check guards against
    // its absence on older systems where the selector simply does not exist.
    let uses_modern_api: bool = unsafe {
        let responds: bool = objc2::msg_send![layer, respondsToSelector: sel!(setPreferredDynamicRange:)];
        responds
    };
    if uses_modern_api {
        unsafe {
            layer.setPreferredDynamicRange(&CADynamicRangeHigh);
        }
    } else {
        #[allow(deprecated)]
        {
            layer.setWantsExtendedDynamicRangeContent(true);
        }
    }
}

/// Apply EDR to the VLC-owned video layer attached to the overlay view.
/// libVLC's caopengllayer vout attaches a CAOpenGLLayer to the VideoOverlayView
/// when set_nsobject is called. The content-view EDR flag alone does not
/// propagate to that layer, so we set preferredDynamicRange on it directly.
/// Safe to call once per session; re-applying is idempotent.
#[cfg(target_os = "macos")]
pub unsafe fn apply_edr_to_video_layer(app: &AppHandle) {
    let state = app.state::<crate::player::PlayerState>();
    let ptr = state.overlay_view.load(std::sync::atomic::Ordering::SeqCst);
    if ptr == 0 {
        return;
    }
    let view = &*(ptr as *mut NSView);
    let Some(layer) = view.layer() else {
        return;
    };
    apply_edr_to_layer(&layer);
}

/// Observer for NSWindow.didChangeScreenNotification. Re-applies EDR settings
/// when the window moves between displays with different dynamic range
/// capabilities.
objc2::define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NebulaeEDRScreenObserver"]
    struct EDRScreenObserver;

    impl EDRScreenObserver {
        #[unsafe(method(screenDidChange:))]
        unsafe fn screen_did_change(&self, _notification: *mut std::ffi::c_void) {
            if let Some(app) = crate::player::embed_macos::overlay_app() {
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(raw) = win.ns_window() {
                        let ns_win = raw as *mut NSWindow;
                        let _ = apply_edr_to_window(&*ns_win);
                    }
                }
                // Re-apply EDR to the VLC-owned video layer on screen change
                // (XDR vs non-XDR displays have different dynamic range).
                let _ = apply_edr_to_video_layer(app);
            }
        }
    }
);

/// Apply EDR settings to a window's content view layer.
unsafe fn apply_edr_to_window(win: &NSWindow) {
    let Some(content) = win.contentView() else {
        return;
    };
    content.setWantsLayer(true);
    if let Some(layer) = content.layer() {
        apply_edr_to_layer(&layer);
    }
}

/// Remove and release the EDR screen-change observer for the main window.
/// Must be called on the main thread.
pub unsafe fn edr_observer_cleanup(app: &AppHandle) {
    let state = app.state::<crate::player::PlayerState>();
    let ptr = state.edr_observer.load(Ordering::SeqCst);
    if ptr == 0 {
        return;
    }
    let observer = Retained::<EDRScreenObserver>::from_raw(ptr as *mut EDRScreenObserver)
        .expect("EDR observer pointer should be valid");
    let nc: *mut objc2::runtime::AnyObject = unsafe {
        objc2::msg_send![class!(NSNotificationCenter), defaultCenter]
    };
    let _: () = unsafe {
        objc2::msg_send![
            nc,
            removeObserver: &*observer as *const EDRScreenObserver as *mut objc2::runtime::AnyObject
        ]
    };
    state.edr_observer.store(0, Ordering::SeqCst);
    // Drop observer to release the retain from from_raw.
    drop(observer);
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
/// Whether the player window is currently in our manual fullscreen.
static FULLSCREEN_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Window frame saved while the player reshapes the window to 16:9.
static PLAYER_FRAME_BOX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe fn restore_player_window_frame(win: &NSWindow) {
    let raw = PLAYER_FRAME_BOX.load(Ordering::SeqCst) as *mut NSRect;
    if !raw.is_null() {
        let f = *raw;
        let _: () = objc2::msg_send![win, setFrame: f, display: true];
        let _ = Box::from_raw(raw);
        PLAYER_FRAME_BOX.store(0, Ordering::SeqCst);
    }
}

/// Enter/leave fullscreen for the borderless player window by moving the
/// frame only. The native toggleFullScreen: animation ignores borderless
/// windows, and setting NSWindowStyleMaskFullScreen on one aborts the
/// process (NSPerformVisuallyAtomicChange) — so never touch the style mask.
unsafe fn toggle_fullscreen_win(win: &NSWindow) {
    if FULLSCREEN_ON.load(Ordering::SeqCst) {
        FULLSCREEN_ON.store(false, Ordering::SeqCst);
        // Leaving fullscreen: restore the saved frame.
        let raw = SAVED_BOX.load(Ordering::SeqCst) as *mut NSRect;
        if !raw.is_null() {
            let f = *raw;
            let _: () = objc2::msg_send![win, setFrame: f, display: true];
        }
    } else {
        let ptr = Box::into_raw(Box::new(win.frame()));
        let old = SAVED_BOX.swap(ptr as usize, Ordering::SeqCst) as *mut NSRect;
        if !old.is_null() {
            let _ = Box::from_raw(old);
        }
        if let Some(screen) = win.screen() {
            let f = screen.frame();
            let _: () = objc2::msg_send![win, setFrame: f, display: true];
        }
        FULLSCREEN_ON.store(true, Ordering::SeqCst);
    }
}

/// Toggle fullscreen for the player window (F key / fullscreen button /
/// double-click). Sets the corner radius to 0 in fullscreen, 3 px out.
/// Miniaturize (yellow traffic light) the main window. Commanded from the
/// webview dot controls on the borderless player window. Honors the
/// `minimize_to_tray` setting: when enabled, hides the window to the tray
/// instead of minimizing it to the Dock.
pub fn miniaturize(app: &AppHandle) -> Result<(), String> {
    let minimize_to_tray = app
        .try_state::<std::sync::Arc<crate::db::Database>>()
        .and_then(|db| db.get_setting("user_settings_json").ok().flatten())
        .and_then(|json| {
            serde_json::from_str::<crate::commands::settings::UserSettings>(&json).ok()
        })
        .map(|s| s.minimize_to_tray)
        .unwrap_or(true);

    if minimize_to_tray {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
            return Ok(());
        }
        return Err("main window not found".into());
    }

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
            let nil: *mut std::ffi::c_void = std::ptr::null_mut();
            let _: () = objc2::msg_send![&*ns_win, miniaturize: nil];
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

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
            let is_fs = FULLSCREEN_ON.load(Ordering::SeqCst);
            let radius = if is_fs { 0.0 } else { 10.0 };
            apply_window_corner_radius(&app2, radius);
            // Notify the frontend: Tauri's win.isFullscreen() always returns
            // false for borderless windows toggled via manual frame resize.
            // The frontend listens on this event to keep its icon in sync.
            use tauri::Emitter;
            let _ = app2.emit("player://fullscreen-changed", is_fs);
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
        let ns_win = raw as *mut NSWindow;
        if ns_win.is_null() {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let ns_app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        if let Some(event) = ns_app.currentEvent() {
            unsafe {
                let _: () = objc2::msg_send![ns_win, performWindowDragWithEvent: &*event];
            }
        } else {
            let _ = win.start_dragging();
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
            layer.setCornerRadius(10.0);
            layer.setMasksToBounds(true);
        }
        let bounds = content.bounds();
        let mtm = MainThreadMarker::new().ok_or("not on main thread")?;
        let view: Retained<VideoOverlayView> = objc2::msg_send![VideoOverlayView::alloc(mtm), init];
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

        // Configure EDR (Extended Dynamic Range) headroom tracking for
        // Apple Silicon XDR displays. We set the content view's layer flag
        // which allows the caopengllayer vout to emit HDR values above 1.0
        // without clipping when the window is on an XDR display.
        // Source: MacLC 1.3, modules/video_output/apple/VLCSampleBufferDisplay.m
        apply_edr_to_window(&*ns_win);

        // Register for window screen-change notifications so EDR settings are
        // re-applied when the window moves between displays with different
        // dynamic range capabilities. The observer is tied to this specific
        // window and stored in PlayerState for cleanup.
        let observer: Retained<EDRScreenObserver> =
            objc2::msg_send![EDRScreenObserver::alloc(mtm), init];
        let nc: *mut objc2::runtime::AnyObject = unsafe {
            objc2::msg_send![class!(NSNotificationCenter), defaultCenter]
        };
        let _: () = unsafe {
            objc2::msg_send![
                nc,
                addObserver: &*observer as *const EDRScreenObserver as *mut objc2::runtime::AnyObject,
                selector: sel!(screenDidChange:),
                name: NSWindowDidChangeScreenNotification,
                object: ns_win as *mut objc2::runtime::AnyObject
            ]
        };
        // Store the observer in PlayerState for cleanup. Leak the raw pointer
        // (Retained is !Send, and the observer is only accessed on the main thread).
        app.state::<crate::player::PlayerState>()
            .edr_observer
            .store(Retained::into_raw(observer) as usize, Ordering::SeqCst);

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

/// # Safety
///
/// Modifies NSView / CALayer properties on macOS and must be invoked on the main Cocoa thread.
pub unsafe fn apply_window_corner_radius(app: &AppHandle, radius: f64) {
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
            apply_window_corner_radius(&app2, 10.0);
            // The video view always covers the whole window; the frontend's
            // floating bar draws above it. Full-window frame here guarantees
            // sane geometry even before the first frontend sync arrives.
            if let Some(win) = (*view).window() {
                if let Some(content) = win.contentView() {
                    let b = content.bounds();
                    (*view).setFrame(b);
                }
                // The native traffic lights are dead on this borderless
                // window (no AX entries, clicks land in the webview): hide
                // them so the working webview controls stand alone.
                for btn_id in [0u64, 1, 2] {
                    let btn: *mut objc2::runtime::AnyObject = objc2::msg_send![
                        &*win,
                        standardWindowButton: btn_id
                    ];
                    if !btn.is_null() {
                        let _: () = objc2::msg_send![btn, setHidden: true];
                    }
                }
                if !hidden {
                    // Bring the player window to key status: without this
                    // the traffic lights render grayed out and the FIRST
                    // click (which macOS spends on activating a
                    // non-key window) never reaches the ✕/bar buttons.
                    let _: () = objc2::msg_send![&*win, makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()];
                    let app_class = objc2::runtime::AnyClass::get(
                        std::ffi::CString::new("NSApplication").unwrap().as_c_str(),
                    )
                    .unwrap();
                    let shared: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![app_class, sharedApplication];
                    if !shared.is_null() {
                        let _: () = objc2::msg_send![shared, activateIgnoringOtherApps: true];
                    }
                }
                // The player window opens at 16:9 so the video fills it
                // edge to edge; the manager/media UI restores its frame.
                if !FULLSCREEN_ON.load(Ordering::SeqCst) {
                    if hidden {
                        restore_player_window_frame(&win);
                    } else {
                        let f = win.frame();
                        let ptr = Box::into_raw(Box::new(f));
                        let old = PLAYER_FRAME_BOX.swap(ptr as usize, Ordering::SeqCst) as *mut NSRect;
                        if !old.is_null() {
                            let _ = Box::from_raw(old);
                        }
                        let new_h = f.size.width * 9.0 / 16.0;
                        let new_frame = NSRect::new(
                            NSPoint::new(f.origin.x, f.origin.y + (f.size.height - new_h)),
                            NSSize::new(f.size.width, new_h),
                        );
                        let _: () = objc2::msg_send![&*win, setFrame: new_frame, display: true];
                    }
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
pub fn sync_frame(
    app: &AppHandle,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    _bar_height: f64,
) -> Result<(), String> {
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

/// Window controls for the undecorated window (Tauri API works on macOS).
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

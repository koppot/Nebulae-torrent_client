//! Windows: native video overlay layered BELOW the webview.
//!
//! A plain black child window (STATIC class) is created beneath the
//! webview sibling; libVLC renders into it via `libvlc_media_player_set_hwnd`.
//! Because the overlay sits BELOW the transparent webview in Z-order,
//! all mouse events (clicks on player buttons, volume/seek tracks,
//! popovers, video pause/double-click fullscreen) and keyboard shortcuts
//! reach the frontend webview first. All work happens on Tauri's main
//! thread; the frontend drives position/size with `player_sync_viewport`.

use std::ffi::{c_void, CString};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrW,
    GetWindowRect, IsIconic, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_WNDPROC, HWND_TOP,
    SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_DISPLAYCHANGE, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_WINDOWPOSCHANGED, WNDPROC, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};

/// Static AppHandle storage for cross-module use (EDR retry, etc.).
/// Set once by `overlay_app_init` during `ensure_overlay`.
static OVERLAY_APP: OnceLock<AppHandle> = OnceLock::new();

/// Cached HWND of the video surface window.
static VIDEO_HWND: AtomicIsize = AtomicIsize::new(0);

/// Saved pre-player window rect for restoring window size on close.
static PRE_PLAYER_RECT: Mutex<Option<RECT>> = Mutex::new(None);

/// Returns the cached `AppHandle` if `ensure_overlay` has already run.
pub(crate) fn overlay_app_handle() -> Option<&'static AppHandle> {
    OVERLAY_APP.get()
}

/// Called from `ensure_overlay` to cache the `AppHandle` for later use
/// by the HDR/EDR retry infrastructure.
pub(crate) fn overlay_app_init(app: AppHandle) {
    let _ = OVERLAY_APP.set(app);
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

    // Register the display-change and position-change handler so EDR/tone-mapping
    // is re-applied and video_hwnd rect stays locked to main_hwnd.
    if let Err(e) = register_display_change_handler(app) {
        tracing::warn!("register_display_change_handler: {e}");
    }

    Ok(hwnd)
}

/// Helper: compute the exact screen coordinates and pixel size of the
/// main window's client area (where WebView2 renders). This eliminates
/// DWM invisible sizing frames and non-client border gaps on Windows 10 & 11.
pub(crate) fn get_client_rect_in_screen(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut cr = RECT::default();
    let mut pt = POINT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut cr);
        let _ = ClientToScreen(hwnd, &mut pt);
    }
    (pt.x, pt.y, cr.right - cr.left, cr.bottom - cr.top)
}

fn attach_main_window_child(app: &AppHandle) -> Result<usize, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let parent = win.hwnd().map_err(|e| format!("no parent HWND: {e}"))?;
    let main_hwnd = HWND(parent.0);

    let (x, y, w, h) = get_client_rect_in_screen(main_hwnd);

    // Create the video surface as an independent WS_POPUP window with no owner,
    // positioned directly behind the transparent main window's client area.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0),
            w!("STATIC"),
            w!("Nebulae Video Surface"),
            WINDOW_STYLE(WS_POPUP.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
            x,
            y,
            w,
            h,
            None,
            None,
            None,
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW failed: {e}"))?;

    VIDEO_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    unsafe {
        // Place main_hwnd at HWND_TOP so the transparent webview with floating controls
        // stays on top of the video surface.
        let _ = SetWindowPos(
            main_hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
        );
        // Place the video window immediately behind main_hwnd with matching dimensions.
        let _ = SetWindowPos(
            hwnd,
            Some(main_hwnd),
            x,
            y,
            w,
            h,
            SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
        );
        // Suppress DWM border and corner rounding on both windows so no visible frame
        // or rounded corner artifact appears over or around the video.
        let no_border: u32 = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let do_not_round: u32 = DWMWCP_DONOTROUND.0 as u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &do_not_round as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            main_hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            main_hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &do_not_round as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }

    // Subclass the video HWND so any keyboard or mouse input that reaches the video
    // surface is forwarded to the frontend webview.
    unsafe {
        let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        let _ = OVERLAY_WNDPROC.set(std::mem::transmute::<isize, WNDPROC>(orig));
        let _ = SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            overlay_wndproc as *const () as usize as isize,
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
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let video_hwnd = HWND(hwnd as _);
        let Ok(main_h) = main_hwnd(&app2) else { return };
        if hidden {
            unsafe {
                let _ = ShowWindow(video_hwnd, SW_HIDE);
                // When player is closed, restore rounded corners on the application window
                let round: u32 = DWMWCP_ROUND.0 as u32;
                let _ = DwmSetWindowAttribute(
                    main_h,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &round as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
            // Restore window rect if previously saved
            if let Ok(mut saved) = PRE_PLAYER_RECT.lock() {
                if let Some(r) = saved.take() {
                    let w = r.right - r.left;
                    let h = r.bottom - r.top;
                    unsafe {
                        let _ = SetWindowPos(
                            main_h,
                            None,
                            r.left,
                            r.top,
                            w,
                            h,
                            SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
                        );
                    }
                }
            }
        } else {
            // Player opening: save current rect and reshape to 16:9 if not fullscreen
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(main_h, &mut r);
            }
            let is_fs = app2
                .get_webview_window("main")
                .and_then(|w| w.is_fullscreen().ok())
                .unwrap_or(false);

            if !is_fs {
                if let Ok(mut saved) = PRE_PLAYER_RECT.lock() {
                    if saved.is_none() {
                        *saved = Some(r);
                    }
                }
                let w = r.right - r.left;
                let h_16_9 = ((w as f64) * 9.0 / 16.0).round() as i32;
                r.bottom = r.top + h_16_9;
                unsafe {
                    let _ = SetWindowPos(
                        main_h,
                        None,
                        r.left,
                        r.top,
                        w,
                        h_16_9,
                        SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
                    );
                }
            }

            let (cx, cy, cw, ch) = get_client_rect_in_screen(main_h);
            unsafe {
                let _ = ShowWindow(video_hwnd, SW_SHOW);
                let _ = SetWindowPos(
                    main_h,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
                );
                let _ = SetWindowPos(
                    video_hwnd,
                    Some(main_h),
                    cx,
                    cy,
                    cw,
                    ch,
                    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
                );
                // During video playback, ensure no DWM border or rounded corners appear around the video screen
                let no_border: u32 = DWMWA_COLOR_NONE;
                let _ = DwmSetWindowAttribute(
                    main_h,
                    DWMWA_BORDER_COLOR,
                    &no_border as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
                let _ = DwmSetWindowAttribute(
                    video_hwnd,
                    DWMWA_BORDER_COLOR,
                    &no_border as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
                let do_not_round: u32 = DWMWCP_DONOTROUND.0 as u32;
                let _ = DwmSetWindowAttribute(
                    main_h,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &do_not_round as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
                let _ = DwmSetWindowAttribute(
                    video_hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &do_not_round as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
            if let Some(win) = app2.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

pub fn sync_frame(
    app: &AppHandle,
    _x: f64,
    _y: f64,
    _w: f64,
    _h: f64,
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
        let video_hwnd = HWND(hwnd as _);
        let Ok(main_h) = main_hwnd(&app2) else { return };
        let (x, y, w, h) = get_client_rect_in_screen(main_h);
        unsafe {
            // Video window matches main_hwnd client area exactly with zero border offset.
            let _ = SetWindowPos(
                video_hwnd,
                Some(main_h),
                x,
                y,
                w,
                h,
                SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
            );
            let _ = SetWindowPos(
                main_h,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
            );
        }
        // Make VLC fill the video surface edge-to-edge so no letterbox /
        // pillarbox black bars appear. The display aspect ratio is pinned to
        // the surface's actual aspect: 16:9 SDR content on a 16:9 surface is
        // a no-op (bit-for-bit identical), while non-16:9 content is stretched
        // to fill instead of being boxed with black bars.
        if w > 0 && h > 0 {
            let state = app2.state::<crate::player::PlayerState>();
            // Copy the player handle out of its guard first so the guard is
            // dropped before we borrow `state.lib` below (avoids a "state does
            // not live long enough" borrow conflict).
            let mp = {
                if let Ok(mp_guard) = state.media_player.lock() {
                    *mp_guard
                } else {
                    0
                }
            };
            if mp != 0 {
                if let Ok(lib_guard) = state.lib.lock() {
                    if let Some(lib) = lib_guard.as_ref() {
                        let aspect = format!("{:.6}", w as f64 / h as f64);
                        if let Ok(c_aspect) = CString::new(aspect.as_str()) {
                            unsafe {
                                (lib.libvlc_video_set_aspect_ratio)(
                                    mp as *mut c_void,
                                    c_aspect.as_ptr(),
                                );
                            }
                        }
                    }
                }
            }
        }
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Apply rounded corners to the main window using DwmSetWindowAttribute
/// (Windows 10 1809+). Best-effort: silently no-ops on older Windows
/// or if the main HWND is not yet available.
pub unsafe fn apply_window_corner_radius(app: &AppHandle, _radius: f64) {
    if let Ok(hwnd) = main_hwnd(app) {
        let preference: u32 = DWMWCP_ROUND.0 as u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        // Suppress 1px DWM system border on the main window so no visible frame appears
        let no_border: u32 = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &no_border as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

pub fn set_window_corner_radius(app: &AppHandle, radius: f64) -> Result<(), String> {
    // On Windows the corner preference is set once at startup; per-session
    // radius changes (player vs media UI) are not applied to avoid
    // flickering. The value is accepted for API compatibility.
    let _ = radius;
    unsafe { apply_window_corner_radius(app, radius) };
    Ok(())
}

/// Apply EDR/tone-mapping to the VLC video output on Windows.
///
/// On Windows, VLC uses Direct3D11 vout. For HDR content (BT.2020 PQ) on
/// an SDR display, we must tell VLC to tone-map the content. This is done
/// by setting the "hdr" variable on the media player to "auto" (let VLC
/// detect display capability and tone-map if needed) or "off" (force SDR
/// output with tone mapping). We use "auto" so VLC handles the display
/// capability detection.
///
/// This mirrors macOS `apply_edr_to_video_layer` which configures the
/// caopengllayer for tone-mapped output.
#[cfg(target_os = "windows")]
pub fn maybe_apply_edr(app: &AppHandle) {
    let app_clone = app.clone();
    let _ = app_clone.clone().run_on_main_thread(move || {
        let state = app_clone.state::<crate::player::PlayerState>();
        if state.edr_applied.load(Ordering::SeqCst) {
            return;
        }
        let mp = {
            let Ok(mp_guard) = state.media_player.lock() else {
                return;
            };
            *mp_guard
        };
        if mp == 0 {
            return;
        }
        let Ok(lib_guard) = state.lib.lock() else {
            return;
        };
        let Some(lib) = lib_guard.as_ref() else {
            return;
        };

        // Set VLC's HDR mode to "auto" so it tone-maps HDR content on SDR displays.
        // var_set_checked signature: int var_set_checked(obj, name, type, value)
        // VLC_VAR_STRING = 0x0040, value is const char* passed as usize.
        let hdr_mode = b"auto\0";
        let rc = unsafe {
            (lib.var_set_checked)(
                mp as *mut c_void,
                c"hdr".as_ptr(),
                0x0040, // VLC_VAR_STRING
                hdr_mode.as_ptr() as usize,
            )
        };
        if rc == 0 {
            tracing::info!("[player] Windows: set VLC hdr mode to 'auto' for tone mapping");
            state.edr_applied.store(true, Ordering::SeqCst);
        } else {
            tracing::warn!("[player] Windows: failed to set VLC hdr mode (rc={rc})");
        }
    });
}

/// Force re-application of EDR/tone-mapping on Windows, bypassing the latch.
/// Used when transitioning from SDR to HDR (e.g. menu to UHD feature).
#[cfg(target_os = "windows")]
pub fn force_apply_edr(app: &AppHandle) {
    let app_clone = app.clone();
    let _ = app_clone.clone().run_on_main_thread(move || {
        let state = app_clone.state::<crate::player::PlayerState>();
        state.edr_applied.store(false, Ordering::SeqCst);
    });
    maybe_apply_edr(app);
}

/// Stored original window procedure for the overlay HWND subclass.
static OVERLAY_WNDPROC: std::sync::OnceLock<WNDPROC> = std::sync::OnceLock::new();

/// Tracks mouse button down position for click vs drag disambiguation.
static CLICK_DOWN_POS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Throttles mouse movement notifications sent to the frontend.
static LAST_MOUSE_MOVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Subclass the overlay window procedure to forward native keyboard and mouse input
/// from the video overlay to the frontend.
unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            let key = windows_key_name(vk, lparam);
            if let Some(app) = overlay_app_handle() {
                if let Some(key) = key {
                    use tauri::Emitter;
                    let _ = app.emit("player-native-key", key);
                }
            }
        }
        WM_MOUSEMOVE => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now.saturating_sub(LAST_MOUSE_MOVE.load(Ordering::Relaxed)) > 150 {
                LAST_MOUSE_MOVE.store(now, Ordering::Relaxed);
                if let Some(app) = overlay_app_handle() {
                    use tauri::Emitter;
                    let _ = app.emit("player://mouse-move", true);
                }
            }
            // If dragging with left mouse button held down, trigger native caption drag
            let down_pos = CLICK_DOWN_POS.load(Ordering::Relaxed);
            if down_pos != 0 {
                let dx = ((lparam.0 as i16) as i32) - (((down_pos & 0xFFFF) as i16) as i32);
                let dy = (((lparam.0 >> 16) as i16) as i32)
                    - ((((down_pos >> 16) & 0xFFFF) as i16) as i32);
                if dx.abs() > 6 || dy.abs() > 6 {
                    CLICK_DOWN_POS.store(0, Ordering::Relaxed);
                    if let Some(app) = overlay_app_handle() {
                        let _ = begin_window_drag(app);
                    }
                }
            }
        }
        WM_LBUTTONDBLCLK => {
            CLICK_DOWN_POS.store(0, Ordering::Relaxed);
            if let Some(app) = overlay_app_handle() {
                use tauri::Emitter;
                let _ = app.emit("player-native-key", "f");
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            CLICK_DOWN_POS.store(lparam.0 as u32, Ordering::SeqCst);
        }
        WM_LBUTTONUP => {
            let down_pos = CLICK_DOWN_POS.swap(0, Ordering::SeqCst);
            if down_pos != 0 {
                let dx = ((lparam.0 as i16) as i32) - (((down_pos & 0xFFFF) as i16) as i32);
                let dy = (((lparam.0 >> 16) as i16) as i32)
                    - ((((down_pos >> 16) & 0xFFFF) as i16) as i32);
                if dx.abs() <= 6 && dy.abs() <= 6 {
                    // Single click on video -> toggle play/pause
                    if let Some(app) = overlay_app_handle() {
                        use tauri::Emitter;
                        let _ = app.emit("player-native-key", " ");
                    }
                }
            }
        }
        _ => {}
    }
    if let Some(orig) = OVERLAY_WNDPROC.get() {
        CallWindowProcW(*orig, hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Stored original window procedure for the main window subclass.
static MAIN_WNDPROC: std::sync::OnceLock<WNDPROC> = std::sync::OnceLock::new();

/// Subclass the main window procedure to intercept `WM_DISPLAYCHANGE`
/// and re-apply EDR/tone-mapping and window corner radius when the window
/// moves between displays with different HDR capabilities.
///
/// Mirrors macOS `EDRScreenObserver` (NSWindowDidChangeScreenNotification).
unsafe extern "system" fn main_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_WINDOWPOSCHANGED {
        let v_hwnd_val = VIDEO_HWND.load(Ordering::Relaxed);
        if v_hwnd_val != 0 {
            let video_hwnd = HWND(v_hwnd_val as _);
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(video_hwnd, SW_HIDE);
            } else {
                let (x, y, w, h) = get_client_rect_in_screen(hwnd);
                let _ = SetWindowPos(
                    video_hwnd,
                    Some(hwnd),
                    x,
                    y,
                    w,
                    h,
                    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
                );
            }
        }
    } else if msg == WM_DISPLAYCHANGE {
        // Re-apply EDR/tone-mapping on screen change. If the window moved
        // to an HDR-capable display, ensure tone-mapping vars are set.
        if let Some(app) = overlay_app_handle() {
            use tauri::Emitter;
            // Reset the latch so the next player_poll / force_apply_edr
            // re-applies tone-mapping for the new display.
            if let Some(state) = app.try_state::<crate::player::PlayerState>() {
                state
                    .edr_applied
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                // Re-apply corner radius (DWM preference may need refreshing).
                let _ = set_window_corner_radius(app, 10.0);
            }
            let _ = app.emit("player://screen-changed", true);
        }
    }
    if let Some(orig) = MAIN_WNDPROC.get() {
        CallWindowProcW(*orig, hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Register the display-change handler on the main window HWND.
///
/// Must be called once after the main window exists (from `ensure_overlay`).
/// Subclasses the main window so `WM_DISPLAYCHANGE` is intercepted and
/// EDR/tone-mapping is re-applied when the window moves between displays.
fn register_display_change_handler(app: &AppHandle) -> Result<(), String> {
    let hwnd = main_hwnd(app)?;
    unsafe {
        let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        let _ = MAIN_WNDPROC.set(std::mem::transmute::<isize, WNDPROC>(orig));
        let _ = SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            main_wndproc as *const () as usize as isize,
        );
    }
    Ok(())
}

/// Map a Windows virtual-key code to the frontend's key-name vocabulary.
fn windows_key_name(vk: u32, lparam: LPARAM) -> Option<String> {
    // Ignore modifier-only keys (Ctrl, Alt, Shift, Win) — the frontend
    // handles modifier combinations itself.
    match vk {
        0x20 => return Some(" ".to_string()),          // VK_SPACE
        0x0D => return Some("Enter".to_string()),      // VK_RETURN
        0x1B => return Some("Escape".to_string()),     // VK_ESCAPE
        0x25 => return Some("ArrowLeft".to_string()),  // VK_LEFT
        0x26 => return Some("ArrowUp".to_string()),    // VK_UP
        0x27 => return Some("ArrowRight".to_string()), // VK_RIGHT
        0x28 => return Some("ArrowDown".to_string()),  // VK_DOWN
        _ => {}
    }
    // For printable characters, translate the virtual key to a character.
    let mut buf = [0u16; 8];
    let scan = (lparam.0 >> 16) & 0xFF;
    let extended = (lparam.0 >> 24) & 1 != 0;
    let lparam_for_name = ((scan as i32) << 16) | if extended { 1 << 24 } else { 0 };
    let len = unsafe { GetKeyNameTextW(lparam_for_name, &mut buf) };
    if len > 0 {
        let s = String::from_utf16_lossy(&buf[..len as usize]);
        if !s.is_empty() && s.len() == 1 {
            return Some(s);
        }
    }
    None
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
        let Ok(main_h) = main_hwnd(app) else {
            return Ok(());
        };
        let hwnd = app
            .state::<crate::player::PlayerState>()
            .overlay_view
            .load(std::sync::atomic::Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                let mut r = RECT::default();
                let _ = GetWindowRect(main_h, &mut r);
                let w = r.right - r.left;
                let h = r.bottom - r.top;
                let _ = SetWindowPos(
                    main_h,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
                );
                let _ = SetWindowPos(
                    HWND(hwnd as _),
                    Some(main_h),
                    r.left,
                    r.top,
                    w,
                    h,
                    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
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
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
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
}

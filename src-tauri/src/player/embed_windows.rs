//! Windows: native video overlay layered BELOW the webview.
//!
//! A plain black child window (STATIC class) is created beneath the
//! webview sibling; libVLC renders into it via `libvlc_media_player_set_hwnd`.
//! Because the overlay sits BELOW the transparent webview in Z-order,
//! player controls receive input through the frontend. When a native
//! disc-menu surface owns input, this window forwards menu mouse input to
//! VLC and keyboard input to the frontend. The frontend drives position/size
//! with `player_sync_viewport`.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, FillRect, GetStockObject, MapWindowPoints, BLACK_BRUSH, HBRUSH, HDC,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindow, LoadCursorW, RegisterClassExW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, CS_DBLCLKS, CS_HREDRAW,
    CS_VREDRAW, GWLP_WNDPROC, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, IDC_ARROW,
    SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    SW_SHOWNA, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_DISPLAYCHANGE, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCACTIVATE, WM_SETFOCUS, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSEXW, WNDPROC,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    WS_VISIBLE,
};

unsafe extern "system" fn surface_wndproc_init(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Lazily registers the dedicated black video surface and per-session slot window classes once.
static SURFACE_CLASS_INIT: OnceLock<()> = OnceLock::new();

fn ensure_surface_class() {
    SURFACE_CLASS_INIT.get_or_init(|| unsafe {
        let black_brush = HBRUSH(GetStockObject(BLACK_BRUSH).0);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(surface_wndproc_init),
            hInstance: HINSTANCE(std::ptr::null_mut()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: black_brush,
            lpszClassName: w!("NebulaeVideoSurface"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let slot_wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(overlay_wndproc),
            hInstance: HINSTANCE(std::ptr::null_mut()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: black_brush,
            lpszClassName: w!("NebulaeVideoSlot"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&slot_wc);
    });
}

/// Static AppHandle storage for cross-module use (EDR retry, etc.).
/// Set once by `overlay_app_init` during `ensure_overlay`.
static OVERLAY_APP: OnceLock<AppHandle> = OnceLock::new();

/// ThreadId of Tauri's Win32 main UI thread, recorded when `NebulaeVideoSurface` is created.
static MAIN_UI_THREAD_ID: OnceLock<std::thread::ThreadId> = OnceLock::new();

/// Cached HWND of the persistent container video surface window (`NebulaeVideoSurface`).
static VIDEO_HWND: AtomicIsize = AtomicIsize::new(0);

/// Active per-session child HWND (`NebulaeVideoSlot`) bound via `libvlc_media_player_set_hwnd`.
///
/// Giving each `VlcSession` its own child HWND prevents VLC 3.0.x `modules/video_output/drawable.c`
/// (`static uintptr_t *used`) from rejecting the new session's HWND as `"busy"` while the previous
/// session is still tearing down BD-J/libbluray on the background `vlc-teardown` thread — which
/// previously caused VLC to fall back to a standalone `"VLC (Direct3D11 output)"` OS window.
static ACTIVE_VIDEO_SLOT: AtomicIsize = AtomicIsize::new(0);

/// Retired per-session child HWNDs whose VLC teardown (`stop` + `set_hwnd(null)` + `release`) has
/// completed and are awaiting `DestroyWindow` on the Win32 creator (main UI) thread.
static RETIRED_VIDEO_SLOTS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

const MAX_RETIRED_SLOTS_QUEUE: usize = 64;

/// Must only be called on the Win32 main UI thread that created the slot HWNDs.
unsafe fn sweep_retired_slots() {
    let slots = match RETIRED_VIDEO_SLOTS.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(p) => std::mem::take(&mut *p.into_inner()),
    };
    let surface_raw = VIDEO_HWND.load(Ordering::SeqCst) as usize;
    for raw in slots {
        if raw == 0 || raw == surface_raw {
            continue;
        }
        let _ =
            ACTIVE_VIDEO_SLOT.compare_exchange(raw as isize, 0, Ordering::SeqCst, Ordering::SeqCst);
        let hwnd = HWND(raw as _);
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Schedule destruction of a retired session's child slot HWND after VLC has completely stopped
/// and released its video output (`drawable.c:Close()`). Safe to call from any thread (including
/// `vlc-teardown`).
pub(crate) fn release_session_surface(slot_hwnd: usize) {
    let surface_raw = VIDEO_HWND.load(Ordering::SeqCst) as usize;
    if slot_hwnd == 0 || slot_hwnd == surface_raw {
        return;
    }
    let _ = ACTIVE_VIDEO_SLOT.compare_exchange(
        slot_hwnd as isize,
        0,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    {
        let mut guard = match RETIRED_VIDEO_SLOTS.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !guard.contains(&slot_hwnd) && guard.len() < MAX_RETIRED_SLOTS_QUEUE {
            guard.push(slot_hwnd);
        }
    }
    if MAIN_UI_THREAD_ID.get() == Some(&std::thread::current().id()) {
        unsafe {
            sweep_retired_slots();
        }
    } else if let Some(app) = overlay_app_handle() {
        let _ = app.run_on_main_thread(|| unsafe {
            sweep_retired_slots();
        });
    }
}

/// Ensure the active session's child slot HWND matches `(0, 0, cw, ch)` inside `NebulaeVideoSurface`.
unsafe fn sync_active_slot_size(cw: i32, ch: i32) {
    if cw <= 0 || ch <= 0 {
        return;
    }
    let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
    if slot_val == 0 {
        return;
    }
    let slot_hwnd = HWND(slot_val as _);
    if !IsWindow(Some(slot_hwnd)).as_bool() {
        return;
    }
    let mut rc = RECT::default();
    if GetClientRect(slot_hwnd, &mut rc).is_ok()
        && (rc.right - rc.left) == cw
        && (rc.bottom - rc.top) == ch
    {
        return;
    }
    let _ = SetWindowPos(
        slot_hwnd,
        None,
        0,
        0,
        cw,
        ch,
        SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOZORDER.0),
    );
}

/// Saved pre-player window rect for restoring window size on close.
static PRE_PLAYER_RECT: Mutex<Option<RECT>> = Mutex::new(None);

/// Set by the main-window WndProc so a non-minimize SIZE_RESTORED message
/// (startup, resize, or player layout change) does not force foreground focus.
static MAIN_WINDOW_WAS_MINIMIZED: AtomicBool = AtomicBool::new(false);

pub fn set_active_crop(_app: &AppHandle, _crop: Option<crate::player::letterbox::Bars>) {}

/// Tracks whether the video surface is currently active/visible.
/// Do not synchronize or reposition a hidden or inactive surface.
static VIDEO_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Main-thread reentrancy guard for surface position updates to prevent feedback loops.
static IN_SURFACE_SYNC: AtomicBool = AtomicBool::new(false);

pub(crate) fn update_video_surface_position(main_h: HWND) {
    if !VIDEO_SURFACE_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let v_hwnd_val = VIDEO_HWND.load(Ordering::Relaxed);
    if v_hwnd_val == 0 {
        return;
    }

    // Reentrancy guard: prevent cascading WM_WINDOWPOSCHANGED feedback
    if IN_SURFACE_SYNC.swap(true, Ordering::AcqRel) {
        return;
    }
    struct SyncGuard;
    impl Drop for SyncGuard {
        fn drop(&mut self) {
            IN_SURFACE_SYNC.store(false, Ordering::Release);
        }
    }
    let _guard = SyncGuard;

    let video_hwnd = HWND(v_hwnd_val as _);
    unsafe {
        if !IsWindow(Some(video_hwnd)).as_bool() {
            return;
        }
        if IsIconic(main_h).as_bool() {
            let _ = ShowWindow(video_hwnd, SW_HIDE);
            return;
        }

        let (cx, cy, cw, ch) = get_client_rect_in_screen(main_h);
        if cw <= 0 || ch <= 0 {
            return;
        }

        // Always keep the active session slot sized to (cw, ch), even when video_hwnd's
        // outer screen rect was already updated before this call.
        sync_active_slot_size(cw, ch);

        let _ = ShowWindow(video_hwnd, SW_SHOWNA);

        // Position video_hwnd directly behind main_h in Z-order.
        let _ = SetWindowPos(
            video_hwnd,
            Some(main_h),
            cx,
            cy,
            cw,
            ch,
            SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
        );
    }
}

/// Synchronizes the always-on-top state of the video surface to match the main window.
pub fn set_video_surface_always_on_top(app: &AppHandle, enabled: bool) {
    let hwnd = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if hwnd == 0 {
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let video_hwnd = HWND(hwnd as _);
        let Ok(main_h) = main_hwnd(&app2) else { return };
        unsafe {
            if !IsWindow(Some(video_hwnd)).as_bool() {
                return;
            }
            let insert_after = if enabled {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            let _ = SetWindowPos(
                video_hwnd,
                Some(insert_after),
                0,
                0,
                0,
                0,
                SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0),
            );
            update_video_surface_position(main_h);
        }
    });
}

/// Returns the cached `AppHandle` if `ensure_overlay` has already run.
pub(crate) fn overlay_app_handle() -> Option<&'static AppHandle> {
    OVERLAY_APP.get()
}

/// Called from `ensure_overlay` to cache the `AppHandle` for later use
/// by the HDR/EDR retry infrastructure.
pub(crate) fn overlay_app_init(app: AppHandle) {
    let _ = OVERLAY_APP.set(app);
}

/// Windows counterpart of macOS `apply_edr_to_video_layer`.
///
/// There is nothing to apply on Windows: HDR (BT.2020 PQ / HLG) is converted to SDR inside the
/// Direct3D 11 vout's pixel shader (patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch), chosen per
/// picture from the stream's own transfer function and the swap chain's colour space. The
/// `tone-mapping*` variables this function used to set belong to the OpenGL vout and are never
/// read by the Direct3D 11 vout, so setting them only suggested a behaviour that did not exist.
/// The function stays so the poll-driven retry latch (`edr_applied`) keeps the same shape on both
/// platforms; it reports whether a media player exists.
pub(crate) unsafe fn apply_edr_to_video_layer(app: &AppHandle, _is_hdr: bool) -> bool {
    let Some(state) = app.try_state::<crate::player::PlayerState>() else {
        return false;
    };
    state.media_player.lock().map(|p| *p).unwrap_or(0) != 0
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

fn prepare_session_slot_on_main_thread(app: &AppHandle) -> Result<usize, String> {
    let _ = MAIN_UI_THREAD_ID.set(std::thread::current().id());
    unsafe {
        sweep_retired_slots();
    }

    let mut surface_val = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);

    if surface_val == 0 {
        let created = attach_main_window_child(app)?;
        app.state::<crate::player::PlayerState>()
            .overlay_view
            .store(created, std::sync::atomic::Ordering::SeqCst);
        if let Err(e) = register_display_change_handler(app) {
            tracing::warn!("register_display_change_handler: {e}");
        }
        surface_val = created;
    }

    let surface_hwnd = HWND(surface_val as _);

    // Immediately hide any outgoing session's child slot while its VLC player
    // finishes stopping on the background vlc-teardown thread.
    let prev_slot = ACTIVE_VIDEO_SLOT.swap(0, Ordering::SeqCst);
    if prev_slot != 0 && (prev_slot as usize) != surface_val {
        unsafe {
            let prev_hwnd = HWND(prev_slot as _);
            if IsWindow(Some(prev_hwnd)).as_bool() {
                let _ = ShowWindow(prev_hwnd, SW_HIDE);
            }
        }
    }

    // Pre-compute the target client dimensions (matching set_overlay_hidden(false)'s
    // 16:9 reshape when windowed) so VLC's initial vout `place` matches the final
    // geometry even on still-frame Blu-ray menus (BD_EVENT_STILL_TIME).
    let (slot_w, slot_h) = if let Ok(main_h) = main_hwnd(app) {
        let (cx, cy, cw, ch) = get_client_rect_in_screen(main_h);
        let is_fs = app
            .get_webview_window("main")
            .and_then(|w| w.is_fullscreen().ok())
            .unwrap_or(false);
        let target_w = cw.max(1);
        let target_h = if !is_fs && !VIDEO_SURFACE_ACTIVE.load(Ordering::Relaxed) && cw > 0 {
            ((cw as f64) * 9.0 / 16.0).round() as i32
        } else {
            ch
        }
        .max(1);
        unsafe {
            let _ = SetWindowPos(
                surface_hwnd,
                None,
                cx,
                cy,
                target_w,
                target_h,
                SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOZORDER.0),
            );
        }
        (target_w, target_h)
    } else {
        (1280, 720)
    };

    ensure_surface_class();

    // Create a brand-new child HWND for this VlcSession so VLC 3.0.x drawable.c
    // (`static uintptr_t *used`) never collides with an outgoing session's HWND.
    let slot_res = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0),
            w!("NebulaeVideoSlot"),
            w!(""),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
            0,
            0,
            slot_w,
            slot_h,
            Some(surface_hwnd),
            None,
            None,
            None,
        )
    };

    match slot_res {
        Ok(slot_hwnd) => {
            ACTIVE_VIDEO_SLOT.store(slot_hwnd.0 as isize, Ordering::SeqCst);
            Ok(slot_hwnd.0 as usize)
        }
        Err(e) => {
            tracing::warn!(
                "NebulaeVideoSlot CreateWindowExW failed ({e}); falling back to NebulaeVideoSurface"
            );
            Ok(surface_val)
        }
    }
}

pub fn ensure_overlay(app: &AppHandle) -> Result<usize, String> {
    overlay_app_init(app.clone());
    if MAIN_UI_THREAD_ID.get() == Some(&std::thread::current().id()) {
        return prepare_session_slot_on_main_thread(app);
    }

    let app2 = app.clone();
    let (tx, rx) = mpsc::channel::<Result<usize, String>>();
    app.run_on_main_thread(move || {
        let res = prepare_session_slot_on_main_thread(&app2);
        let _ = tx.send(res);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))?;
    rx.recv()
        .map_err(|_| "main thread callback lost".to_string())?
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
    let _ = MAIN_UI_THREAD_ID.set(std::thread::current().id());
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let parent = win.hwnd().map_err(|e| format!("no parent HWND: {e}"))?;
    let main_hwnd = HWND(parent.0);

    let (x, y, w, h) = get_client_rect_in_screen(main_hwnd);

    ensure_surface_class();

    // Create the video surface as an independent WS_POPUP window with no owner,
    // positioned directly behind the transparent main window's client area with a black background.
    // Initially hidden until `set_overlay_hidden(app, false)` reveals the active session.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0),
            w!("NebulaeVideoSurface"),
            w!("Nebulae Video Surface"),
            WINDOW_STYLE(WS_POPUP.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
            x,
            y,
            w.max(1),
            h.max(1),
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
            w.max(1),
            h.max(1),
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
        unsafe {
            sweep_retired_slots();
        }
        let video_hwnd = HWND(hwnd as _);
        let Ok(main_h) = main_hwnd(&app2) else { return };
        if hidden {
            VIDEO_SURFACE_ACTIVE.store(false, Ordering::SeqCst);
            unsafe {
                let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
                if slot_val != 0 {
                    let slot_h = HWND(slot_val as _);
                    if IsWindow(Some(slot_h)).as_bool() {
                        let _ = ShowWindow(slot_h, SW_HIDE);
                    }
                }
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
            VIDEO_SURFACE_ACTIVE.store(true, Ordering::SeqCst);
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

            unsafe {
                let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
                if slot_val != 0 {
                    let slot_h = HWND(slot_val as _);
                    if IsWindow(Some(slot_h)).as_bool() {
                        let _ = ShowWindow(slot_h, SW_SHOWNA);
                    }
                }
                let _ = ShowWindow(video_hwnd, SW_SHOW);
                // Perform one-time Z-order arrangement when showing playback, outside position handlers.
                let (cx, cy, cw, ch) = get_client_rect_in_screen(main_h);
                if cw > 0 && ch > 0 {
                    sync_active_slot_size(cw, ch);
                    let _ = SetWindowPos(
                        video_hwnd,
                        Some(main_h),
                        cx,
                        cy,
                        cw,
                        ch,
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
            update_video_surface_position(main_h);
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
        let Ok(main_h) = main_hwnd(&app2) else { return };
        update_video_surface_position(main_h);
    })
    .map_err(|e| format!("main thread unavailable: {e}"))
}

/// Apply rounded corners to the main window using DwmSetWindowAttribute
/// (Windows 10 1809+). Best-effort: silently no-ops on older Windows
/// or if the main HWND is not yet available.
pub unsafe fn apply_window_corner_radius(app: &AppHandle, _radius: f64) {
    overlay_app_init(app.clone());
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
        // Install main_wndproc once at startup so WM_SIZE(SIZE_RESTORED) foreground focus
        // restoration works on taskbar restores even before any media session opens.
        let _ = register_display_change_handler(app);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDisposition {
    Posted,
    NoTarget,
    OutsideTarget,
    NativeApiFailure,
}

static CHILD_HIERARCHY_LOGGED: AtomicBool = AtomicBool::new(false);

unsafe fn log_child_hierarchy(surface: HWND) {
    if CHILD_HIERARCHY_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetClientRect, GetWindow, GW_CHILD, GW_HWNDNEXT,
    };
    let mut surface_rect = RECT::default();
    let _ = GetClientRect(surface, &mut surface_rect);
    tracing::warn!(
        "[menu-input] NoTarget: surface HWND={:?} bounds={:?}",
        surface.0,
        surface_rect
    );

    let mut child = match GetWindow(surface, GW_CHILD) {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut index = 0;
    while !child.is_invalid() {
        let mut buf = [0u16; 128];
        let len = GetClassNameW(child, &mut buf);
        let class_name = if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::from("<unknown>")
        };
        let mut child_rect = RECT::default();
        let _ = GetClientRect(child, &mut child_rect);
        tracing::warn!(
            "[menu-input]   child[{}] HWND={:?} class={:?} bounds={:?}",
            index,
            child.0,
            class_name,
            child_rect
        );
        index += 1;
        child = GetWindow(child, GW_HWNDNEXT).unwrap_or_default();
    }
}

unsafe fn get_window_class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut buf);
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::from("<unknown>")
    }
}

/// Walk all descendant windows of `container` searching for VLC's event window.
/// `events.c` registers this HWND with class starting with `VLC video main %p`
/// (or `VLC video output %p` as fallback).
/// Scoping the search to `container` (either the active session slot or the
/// overlay surface) guarantees mouse input is never delivered to an inactive
/// or retiring session.
unsafe fn find_vlc_window_in_tree(container: HWND) -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_CHILD, GW_HWNDNEXT};
    let mut stack = vec![container];
    let mut fallback_output = None;

    while let Some(current) = stack.pop() {
        let mut child = match GetWindow(current, GW_CHILD) {
            Ok(c) => c,
            Err(_) => continue,
        };
        while !child.is_invalid() {
            let mut buf = [0u16; 128];
            let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(child, &mut buf);
            if len > 0 {
                let name = String::from_utf16_lossy(&buf[..len as usize]);
                if is_vlc_input_window_class(&name) {
                    return Some(child);
                }
                if name.starts_with("VLC video output") && fallback_output.is_none() {
                    fallback_output = Some(child);
                }
            }
            stack.push(child);
            child = GetWindow(child, GW_HWNDNEXT).unwrap_or_default();
        }
    }
    fallback_output
}

unsafe fn find_vlc_video_main_window(parent: HWND) -> Option<HWND> {
    let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
    if slot_val != 0 {
        let slot_hwnd = HWND(slot_val as _);
        if IsWindow(Some(slot_hwnd)).as_bool() {
            if let Some(target) = find_vlc_window_in_tree(slot_hwnd) {
                return Some(target);
            }
        }
    }
    find_vlc_window_in_tree(parent)
}

fn is_vlc_input_window_class(name: &str) -> bool {
    name.starts_with("VLC video main")
}

pub(crate) fn is_point_within_client_rect(
    pt_x: i32,
    pt_y: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> bool {
    pt_x >= left && pt_x < right && pt_y >= top && pt_y < bottom
}

pub(crate) fn unpack_mouse_lparam(lparam: isize) -> (i16, i16) {
    let x = lparam as u32 as u16 as i16;
    let y = (lparam as u32 >> 16) as u16 as i16;
    (x, y)
}

static BLURAY_PLUGIN_SCALES_UHD_MOUSE: OnceLock<bool> = OnceLock::new();

fn bluray_plugin_scales_uhd_mouse() -> bool {
    *BLURAY_PLUGIN_SCALES_UHD_MOUSE.get_or_init(|| {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(plugin_dir) = std::env::var("VLC_PLUGIN_PATH") {
            let base = std::path::PathBuf::from(plugin_dir);
            candidates.push(base.join("access").join("liblibbluray_plugin.dll"));
            candidates.push(base.join("liblibbluray_plugin.dll"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("resources/vlc/plugins/access/liblibbluray_plugin.dll"));
                candidates.push(dir.join("vlc/plugins/access/liblibbluray_plugin.dll"));
            }
        }
        for path in candidates {
            if let Ok(bytes) = std::fs::read(&path) {
                let scaled = crate::player::engine::dll_contains_uhd_mouse_scaling(&bytes);
                tracing::info!(
                    path = %path.display(),
                    dll_scales_uhd = scaled,
                    "[menu-input] Inspected liblibbluray_plugin.dll for UHD mouse scaling"
                );
                return scaled;
            }
        }
        false
    })
}

fn should_scale_uhd_bluray_mouse() -> bool {
    let is_hdr = overlay_app_handle()
        .and_then(|app| app.try_state::<crate::player::PlayerState>())
        .map(|state| state.is_hdr.load(Ordering::SeqCst))
        .unwrap_or(false);
    is_hdr
        && crate::player::engine::current_session_is_bluray()
        && !bluray_plugin_scales_uhd_mouse()
}

pub(crate) fn scale_uhd_bluray_menu_point(
    pt_x: i32,
    pt_y: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> (i32, i32) {
    crate::player::engine::scale_uhd_bluray_menu_point(pt_x, pt_y, left, top, right, bottom)
}

unsafe fn find_vlc_video_output_rect_in_target(target: HWND) -> Option<RECT> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindow, GW_CHILD, GW_HWNDNEXT,
    };
    let mut stack = vec![target];
    while let Some(current) = stack.pop() {
        let mut child = match GetWindow(current, GW_CHILD) {
            Ok(c) => c,
            Err(_) => continue,
        };
        while !child.is_invalid() {
            let mut buf = [0u16; 128];
            let len = GetClassNameW(child, &mut buf);
            if len > 0 {
                let name = String::from_utf16_lossy(&buf[..len as usize]);
                if name.starts_with("VLC video output") {
                    let mut cr = RECT::default();
                    if GetClientRect(child, &mut cr).is_ok() {
                        let mut corners = [
                            POINT {
                                x: cr.left,
                                y: cr.top,
                            },
                            POINT {
                                x: cr.right,
                                y: cr.bottom,
                            },
                        ];
                        let _ = MapWindowPoints(Some(child), Some(target), &mut corners);
                        if corners[1].x > corners[0].x && corners[1].y > corners[0].y {
                            return Some(RECT {
                                left: corners[0].x,
                                top: corners[0].y,
                                right: corners[1].x,
                                bottom: corners[1].y,
                            });
                        }
                    }
                }
            }
            stack.push(child);
            child = GetWindow(child, GW_HWNDNEXT).unwrap_or_default();
        }
    }
    None
}

/// Translate a mouse point from the overlay's client area to VLC's main window.
/// Rejects coordinates that fall outside the target window's client bounds.
unsafe fn map_mouse_lparam(
    surface: HWND,
    target: HWND,
    lparam: LPARAM,
) -> Result<(LPARAM, POINT, RECT, RECT, bool), InputDisposition> {
    let (px, py) = unpack_mouse_lparam(lparam.0);
    let mut pt = POINT {
        x: px as i32,
        y: py as i32,
    };
    let _ = MapWindowPoints(Some(surface), Some(target), std::slice::from_mut(&mut pt));
    let mut target_rect = RECT::default();
    if GetClientRect(target, &mut target_rect).is_err() {
        return Err(InputDisposition::NativeApiFailure);
    }
    if !is_point_within_client_rect(
        pt.x,
        pt.y,
        target_rect.left,
        target_rect.top,
        target_rect.right,
        target_rect.bottom,
    ) {
        return Err(InputDisposition::OutsideTarget);
    }
    let active_rect = find_vlc_video_output_rect_in_target(target).unwrap_or(target_rect);
    let scaled = should_scale_uhd_bluray_mouse();
    if scaled {
        let (sx, sy) = scale_uhd_bluray_menu_point(
            pt.x,
            pt.y,
            active_rect.left,
            active_rect.top,
            active_rect.right,
            active_rect.bottom,
        );
        pt.x = sx;
        pt.y = sy;
    }
    let mapped = LPARAM(((pt.x as u16 as u32) | ((pt.y as u16 as u32) << 16)) as isize);
    Ok((mapped, pt, target_rect, active_rect, scaled))
}

fn mouse_point_lparam(x: f64, y: f64) -> LPARAM {
    let clamp = |value: f64| {
        if value.is_finite() {
            value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
        } else {
            0
        }
    };
    let x = clamp(x) as u16 as u32;
    let y = clamp(y) as u16 as u32;
    LPARAM((x | (y << 16)) as isize)
}

static LAST_MOVE_LOG_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LAST_MOVE_LOG_DISP: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0xff);

unsafe fn log_move_disposition(
    disp: InputDisposition,
    target: Option<HWND>,
    raw: (f64, f64),
    pt: Option<POINT>,
    client_rect: Option<RECT>,
    video_rect: Option<RECT>,
    scaled: bool,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let disp_u8 = match disp {
        InputDisposition::Posted => 0,
        InputDisposition::NoTarget => 1,
        InputDisposition::OutsideTarget => 2,
        InputDisposition::NativeApiFailure => 3,
    };
    let prev_disp = LAST_MOVE_LOG_DISP.swap(disp_u8, Ordering::Relaxed);
    let prev_time = LAST_MOVE_LOG_TIME.load(Ordering::Relaxed);

    let changed = prev_disp != disp_u8;
    let throttled = now.saturating_sub(prev_time) >= 500;

    if changed || (disp != InputDisposition::Posted && throttled) || throttled {
        LAST_MOVE_LOG_TIME.store(now, Ordering::Relaxed);
        let class_name = target.map(|t| get_window_class_name(t)).unwrap_or_default();
        let target_raw = target.map(|t| t.0 as isize).unwrap_or(0);
        let pt_str = pt
            .map(|p| format!("({},{})", p.x, p.y))
            .unwrap_or_else(|| "none".to_string());
        let client_str = client_rect
            .map(|r| format!("{}x{}", r.right - r.left, r.bottom - r.top))
            .unwrap_or_else(|| "none".to_string());
        let video_str = video_rect
            .map(|r| format!("{}x{}", r.right - r.left, r.bottom - r.top))
            .unwrap_or_else(|| "none".to_string());
        let forwarded = disp == InputDisposition::Posted;
        tracing::info!(
            "[menu-input] forward_mouse_move: raw=({:.1}, {:.1}) mapped={} client_size={} video_size={} scaled={} forwarded={} disp={:?} target_hwnd={:?} class={:?}",
            raw.0,
            raw.1,
            pt_str,
            client_str,
            video_str,
            scaled,
            forwarded,
            disp,
            target_raw,
            class_name
        );
    }
}

unsafe fn post_vlc_mouse_move(
    surface: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
    raw: (f64, f64),
) -> InputDisposition {
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_MOUSEMOVE};
    let target = match find_vlc_video_main_window(surface) {
        Some(t) => t,
        None => {
            log_child_hierarchy(surface);
            log_move_disposition(
                InputDisposition::NoTarget,
                None,
                raw,
                None,
                None,
                None,
                false,
            );
            return InputDisposition::NoTarget;
        }
    };
    let (mapped, pt, target_rect, active_rect, scaled) =
        match map_mouse_lparam(surface, target, lparam) {
            Ok(v) => v,
            Err(disp) => {
                log_move_disposition(disp, Some(target), raw, None, None, None, false);
                return disp;
            }
        };
    let disp = match PostMessageW(Some(target), WM_MOUSEMOVE, wparam, mapped) {
        Ok(_) => InputDisposition::Posted,
        Err(e) => {
            tracing::warn!("[menu-input] PostMessageW WM_MOUSEMOVE failed: {e}");
            InputDisposition::NativeApiFailure
        }
    };
    log_move_disposition(
        disp,
        Some(target),
        raw,
        Some(pt),
        Some(target_rect),
        Some(active_rect),
        scaled,
    );
    disp
}

unsafe fn post_vlc_mouse_click(surface: HWND, lparam: LPARAM, raw: (f64, f64)) -> InputDisposition {
    use windows::Win32::UI::WindowsAndMessaging::{
        PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };
    let target = match find_vlc_video_main_window(surface) {
        Some(t) => t,
        None => {
            log_child_hierarchy(surface);
            tracing::warn!(
                "[menu-input] click rejected: NoTarget (surface HWND={:?})",
                surface.0
            );
            return InputDisposition::NoTarget;
        }
    };
    let class_name = get_window_class_name(target);
    let (mapped, pt, target_rect, active_rect, scaled) =
        match map_mouse_lparam(surface, target, lparam) {
            Ok(v) => v,
            Err(disp) => {
                tracing::warn!(
                    "[menu-input] click rejected: disposition={:?} target HWND={:?} class={:?}",
                    disp,
                    target.0,
                    class_name
                );
                return disp;
            }
        };
    const MK_LBUTTON: usize = 0x0001;
    let mut disp = InputDisposition::Posted;
    if let Err(e) = PostMessageW(Some(target), WM_MOUSEMOVE, WPARAM(0), mapped) {
        tracing::warn!("[menu-input] click WM_MOUSEMOVE failed: {e}");
        disp = InputDisposition::NativeApiFailure;
    } else if let Err(e) = PostMessageW(Some(target), WM_LBUTTONDOWN, WPARAM(MK_LBUTTON), mapped) {
        tracing::warn!("[menu-input] click WM_LBUTTONDOWN failed: {e}");
        disp = InputDisposition::NativeApiFailure;
    } else if let Err(e) = PostMessageW(Some(target), WM_LBUTTONUP, WPARAM(0), mapped) {
        tracing::warn!("[menu-input] click WM_LBUTTONUP failed: {e}");
        disp = InputDisposition::NativeApiFailure;
    }
    let forwarded = disp == InputDisposition::Posted;
    let client_w = target_rect.right - target_rect.left;
    let client_h = target_rect.bottom - target_rect.top;
    let video_w = active_rect.right - active_rect.left;
    let video_h = active_rect.bottom - active_rect.top;
    tracing::info!(
        "[menu-input] forward_mouse_click: raw=({:.1}, {:.1}) mapped=({},{}) client_size=({}x{}) video_size=({}x{}) scaled={} forwarded={} disp={:?} target_hwnd={:?} class={:?}",
        raw.0,
        raw.1,
        pt.x,
        pt.y,
        client_w,
        client_h,
        video_w,
        video_h,
        scaled,
        forwarded,
        disp,
        target.0,
        class_name
    );
    disp
}

/// Stored original window procedure for the overlay HWND subclass.
static OVERLAY_WNDPROC: std::sync::OnceLock<WNDPROC> = std::sync::OnceLock::new();

/// Tracks mouse button down position for click vs drag disambiguation.
static CLICK_DOWN_POS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Distinguishes a real click at client coordinate (0, 0) from no button down.
static CLICK_DOWN_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Throttles mouse movement notifications sent to the frontend.
static LAST_MOUSE_MOVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Throttles native mouse moves forwarded to VLC while an authored menu is open.
static LAST_MENU_MOUSE_MOVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Throttles native wheel navigation while an authored menu is open.
static LAST_MENU_MOUSE_WHEEL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Subclass the overlay window procedure to forward native keyboard and mouse input
/// from the video overlay to the frontend.
unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as _);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let black_brush = HBRUSH(GetStockObject(BLACK_BRUSH).0);
            let _ = FillRect(hdc, &rc, black_brush);
            return LRESULT(1);
        }
        WM_MOUSEACTIVATE => {
            // Prevent Win32 from bringing NebulaeVideoSurface to the foreground when
            // the user clicks on the video area.  Without this, DefWindowProcW returns
            // MA_ACTIVATE, which reorders the Z-stack and buries main_hwnd (the
            // transparent WebView2 window that hosts all player controls).
            // Instead, activate main_hwnd and synchronize the video surface directly behind it.
            if let Some(app) = overlay_app_handle() {
                if let Ok(main_h) = main_hwnd(app) {
                    let _ = SetForegroundWindow(main_h);
                    update_video_surface_position(main_h);
                }
            }
            return LRESULT(3); // MA_NOACTIVATE
        }
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
            let at_menu = overlay_app_handle()
                .and_then(|app| app.try_state::<crate::player::PlayerState>())
                .map(|s| s.at_menu.load(Ordering::SeqCst))
                .unwrap_or(false);
            if at_menu && now.saturating_sub(LAST_MENU_MOUSE_MOVE.load(Ordering::Relaxed)) >= 40 {
                LAST_MENU_MOUSE_MOVE.store(now, Ordering::Relaxed);
                // The native video surface owns mouse input on Windows, so the
                // WebView does not reliably receive pointer coordinates. Forward
                // the native client point directly to VLC's video child.
                let (px, py) = unpack_mouse_lparam(lparam.0);
                let _ = post_vlc_mouse_move(hwnd, WPARAM(0), lparam, (px as f64, py as f64));
            }
            if now.saturating_sub(LAST_MOUSE_MOVE.load(Ordering::Relaxed)) > 150 {
                LAST_MOUSE_MOVE.store(now, Ordering::Relaxed);
                if let Some(app) = overlay_app_handle() {
                    use tauri::Emitter;
                    let _ = app.emit("player://mouse-move", true);
                }
            }
            // If dragging with left mouse button held down, trigger native caption drag
            let down_pos = CLICK_DOWN_POS.load(Ordering::Relaxed);
            if CLICK_DOWN_ACTIVE.load(Ordering::Relaxed) {
                let dx = ((lparam.0 as i16) as i32) - (((down_pos & 0xFFFF) as i16) as i32);
                let dy = (((lparam.0 >> 16) as i16) as i32)
                    - ((((down_pos >> 16) & 0xFFFF) as i16) as i32);
                if dx.abs() > 6 || dy.abs() > 6 {
                    CLICK_DOWN_POS.store(0, Ordering::Relaxed);
                    CLICK_DOWN_ACTIVE.store(false, Ordering::Relaxed);
                    if let Some(app) = overlay_app_handle() {
                        let _ = begin_window_drag(app);
                    }
                }
            }
        }
        WM_MOUSEWHEEL => {
            let at_menu = overlay_app_handle()
                .and_then(|app| app.try_state::<crate::player::PlayerState>())
                .map(|s| s.at_menu.load(Ordering::SeqCst))
                .unwrap_or(false);
            if at_menu {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
                if delta != 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if now.saturating_sub(LAST_MENU_MOUSE_WHEEL.load(Ordering::Relaxed)) >= 300 {
                        LAST_MENU_MOUSE_WHEEL.store(now, Ordering::Relaxed);
                        if let Some(app) = overlay_app_handle() {
                            use tauri::Emitter;
                            let key = if delta > 0 { "ArrowUp" } else { "ArrowDown" };
                            let _ = app.emit("player-native-key", key);
                        }
                    }
                }
                return LRESULT(0);
            }
        }
        WM_LBUTTONDBLCLK => {
            CLICK_DOWN_POS.store(0, Ordering::Relaxed);
            CLICK_DOWN_ACTIVE.store(false, Ordering::Relaxed);
            let at_menu = overlay_app_handle()
                .and_then(|app| app.try_state::<crate::player::PlayerState>())
                .map(|s| s.at_menu.load(Ordering::SeqCst))
                .unwrap_or(false);
            if at_menu {
                // The first up already activated the menu item. Suppress the
                // second half of a double click so it cannot toggle fullscreen.
                return LRESULT(0);
            }
            if let Some(app) = overlay_app_handle() {
                use tauri::Emitter;
                let _ = app.emit("player-native-key", "f");
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            CLICK_DOWN_POS.store(lparam.0 as u32, Ordering::SeqCst);
            CLICK_DOWN_ACTIVE.store(true, Ordering::SeqCst);
        }
        WM_LBUTTONUP => {
            let down_pos = CLICK_DOWN_POS.swap(0, Ordering::SeqCst);
            if CLICK_DOWN_ACTIVE.swap(false, Ordering::SeqCst) {
                let dx = ((lparam.0 as i16) as i32) - (((down_pos & 0xFFFF) as i16) as i32);
                let dy = (((lparam.0 >> 16) as i16) as i32)
                    - ((((down_pos >> 16) & 0xFFFF) as i16) as i32);
                if dx.abs() <= 6 && dy.abs() <= 6 {
                    // During an interactive disc menu, forward the click to the VLC
                    // video output child so VLC activates the focused menu button.
                    // Suppress the play/pause toggle unconditionally in this branch.
                    let at_menu = overlay_app_handle()
                        .and_then(|app| app.try_state::<crate::player::PlayerState>())
                        .map(|s| s.at_menu.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if at_menu {
                        // The WebView may not receive this click because the
                        // native video surface owns hit-testing. Forward this
                        // native client point exactly once to VLC's video child.
                        let (px, py) = unpack_mouse_lparam(lparam.0);
                        let _ = post_vlc_mouse_click(hwnd, lparam, (px as f64, py as f64));
                    } else {
                        // Single click on video -> toggle play/pause
                        if let Some(app) = overlay_app_handle() {
                            use tauri::Emitter;
                            let _ = app.emit("player-native-key", " ");
                        }
                    }
                }
            }
        }
        _ => {}
    }
    if hwnd.0 as isize == VIDEO_HWND.load(Ordering::Relaxed) {
        if let Some(orig) = OVERLAY_WNDPROC.get() {
            return CallWindowProcW(*orig, hwnd, msg, wparam, lparam);
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Stored original window procedure for the main window subclass.
static MAIN_WNDPROC: std::sync::OnceLock<WNDPROC> = std::sync::OnceLock::new();

fn should_activate_after_restore(size_type: u32, was_minimized: &AtomicBool) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SIZE_MINIMIZED, SIZE_RESTORED};
    if size_type == SIZE_MINIMIZED {
        was_minimized.store(true, Ordering::SeqCst);
        false
    } else if size_type == SIZE_RESTORED {
        was_minimized.swap(false, Ordering::SeqCst)
    } else {
        false
    }
}

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
    match msg {
        WM_ACTIVATE => {
            let activate_state = (wparam.0 & 0xFFFF) as u32;
            if activate_state != 0 {
                update_video_surface_position(hwnd);
            }
        }
        WM_NCACTIVATE => {
            if wparam.0 != 0 {
                update_video_surface_position(hwnd);
            }
        }
        WM_SETFOCUS => {
            update_video_surface_position(hwnd);
        }
        WM_WINDOWPOSCHANGED => {
            if VIDEO_SURFACE_ACTIVE.load(Ordering::Relaxed) {
                let v_hwnd_val = VIDEO_HWND.load(Ordering::Relaxed);
                if v_hwnd_val != 0 {
                    update_video_surface_position(hwnd);
                }
            }
        }
        WM_DISPLAYCHANGE => {
            // Re-apply EDR/tone-mapping on screen change. If the window moved
            // to another display, refresh the per-window state below.
            if let Some(app) = overlay_app_handle() {
                use tauri::Emitter;
                // Reset the latch so the next player_poll / force_apply_edr
                // re-checks the player for the new display.
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
        WM_SIZE => {
            // When the window is restored from the taskbar (taskbar button click or
            // SC_RESTORE), Win32 delivers WM_SIZE(SIZE_RESTORED) but does NOT set
            // foreground focus on a decorations(false)+transparent(true) window.
            // Without focus, the WebView2 compositor treats the window as inactive
            // and suppresses mouse hit-testing, leaving the custom Close and Minimize
            // buttons unresponsive.  Calling SetForegroundWindow here restores the
            // normal click-to-activate behaviour without interfering with minimize or
            // display-change paths.
            if should_activate_after_restore(wparam.0 as u32, &MAIN_WINDOW_WAS_MINIMIZED) {
                let _ = SetForegroundWindow(hwnd);
            }
            if VIDEO_SURFACE_ACTIVE.load(Ordering::Relaxed) {
                update_video_surface_position(hwnd);
            }
        }
        _ => {}
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
    if MAIN_WNDPROC.get().is_some() {
        return Ok(());
    }
    let hwnd = main_hwnd(app)?;
    unsafe {
        let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if MAIN_WNDPROC
            .set(std::mem::transmute::<isize, WNDPROC>(orig))
            .is_ok()
        {
            let _ = SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                main_wndproc as *const () as usize as isize,
            );
        }
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

pub fn leave_fullscreen_if_active(_app: &AppHandle) {}
pub fn update_blackout_state(_app: &AppHandle) {}

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
            let (x, y, w, h) = get_client_rect_in_screen(main_h);
            unsafe {
                sync_active_slot_size(w, h);
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
                    x,
                    y,
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

/// Reads the `minimize_to_tray` user setting (defaults to false).
fn minimize_to_tray_enabled(app: &AppHandle) -> bool {
    app.try_state::<std::sync::Arc<crate::db::Database>>()
        .and_then(|db| db.get_setting("user_settings_json").ok().flatten())
        .and_then(|json| {
            serde_json::from_str::<crate::commands::settings::UserSettings>(&json).ok()
        })
        .map(|s| s.minimize_to_tray)
        .unwrap_or(false)
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
    if let Some(win) = app.get_webview_window("main") {
        return win.minimize().map_err(|e| format!("minimize: {e}"));
    }
    let hwnd = main_hwnd(app)?;
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
    }
    Ok(())
}

pub fn window_maximize_toggle_native(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let zoomed = main_hwnd(app)
            .map(|hwnd| unsafe {
                use windows::Win32::UI::WindowsAndMessaging::IsZoomed;
                IsZoomed(hwnd).as_bool()
            })
            .unwrap_or_else(|_| win.is_maximized().unwrap_or(false));
        if zoomed {
            return win.unmaximize().map_err(|e| format!("unmaximize: {e}"));
        } else {
            return win.maximize().map_err(|e| format!("maximize: {e}"));
        }
    }
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

pub fn fit_player_window_to_aspect(_app: &AppHandle, _aspect: f64) -> Result<(), String> {
    Ok(())
}

pub fn forward_mouse_move(app: &AppHandle, x: f64, y: f64) -> Result<InputDisposition, String> {
    let hwnd_val = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val == 0 {
        return Ok(InputDisposition::NoTarget);
    }
    use windows::Win32::Foundation::{HWND, WPARAM};
    let surface = HWND(hwnd_val as _);

    // Prefer the active per-session slot HWND as the source coordinate system,
    // falling back to the parent container surface.
    let source_hwnd = {
        let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
        if slot_val != 0 && unsafe { IsWindow(Some(HWND(slot_val as _))).as_bool() } {
            HWND(slot_val as _)
        } else {
            surface
        }
    };

    let disp =
        unsafe { post_vlc_mouse_move(source_hwnd, WPARAM(0), mouse_point_lparam(x, y), (x, y)) };
    Ok(disp)
}

pub fn forward_mouse_click(app: &AppHandle, x: f64, y: f64) -> Result<InputDisposition, String> {
    let hwnd_val = app
        .state::<crate::player::PlayerState>()
        .overlay_view
        .load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val == 0 {
        return Ok(InputDisposition::NoTarget);
    }
    use windows::Win32::Foundation::HWND;
    let surface = HWND(hwnd_val as _);

    // Prefer the active per-session slot HWND as the source coordinate system,
    // falling back to the parent container surface.
    let source_hwnd = {
        let slot_val = ACTIVE_VIDEO_SLOT.load(Ordering::Acquire);
        if slot_val != 0 && unsafe { IsWindow(Some(HWND(slot_val as _))).as_bool() } {
            HWND(slot_val as _)
        } else {
            surface
        }
    };

    let disp = unsafe { post_vlc_mouse_click(source_hwnd, mouse_point_lparam(x, y), (x, y)) };
    Ok(disp)
}

#[cfg(test)]
fn scale_client_point(
    x: i32,
    y: i32,
    from_w: i32,
    from_h: i32,
    to_w: i32,
    to_h: i32,
) -> (i32, i32) {
    let scaled_x = if from_w > 0 {
        ((x as i64 * to_w as i64) / from_w as i64) as i32
    } else {
        x
    };
    let scaled_y = if from_h > 0 {
        ((y as i64 * to_h as i64) / from_h as i64) as i32
    } else {
        y
    };
    (scaled_x, scaled_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_point_maps_between_overlay_and_vlc_client_sizes() {
        assert_eq!(
            scale_client_point(320, 180, 640, 360, 1280, 720),
            (640, 360)
        );
        assert_eq!(scale_client_point(320, 180, 1280, 720, 640, 360), (160, 90));
        assert_eq!(scale_client_point(-10, 5, 100, 50, 200, 100), (-20, 10));
    }

    #[test]
    fn mouse_point_lparam_pack_unpack_round_trip() {
        // (0, 0) origin
        let lp = mouse_point_lparam(0.0, 0.0);
        assert_eq!(unpack_mouse_lparam(lp.0), (0, 0));

        // 1080p FHD coordinate
        let lp = mouse_point_lparam(1920.0, 1080.0);
        assert_eq!(unpack_mouse_lparam(lp.0), (1920, 1080));

        // 4K UHD coordinate
        let lp = mouse_point_lparam(3840.0, 2160.0);
        assert_eq!(unpack_mouse_lparam(lp.0), (3840, 2160));

        // Negative coordinates after letterbox/drag offset
        let lp = mouse_point_lparam(-42.0, -18.0);
        assert_eq!(unpack_mouse_lparam(lp.0), (-42, -18));

        // Rounding fractional values
        let lp = mouse_point_lparam(123.4, 567.8);
        assert_eq!(unpack_mouse_lparam(lp.0), (123, 568));

        // Clamping non-finite / NaN values to 0
        let lp = mouse_point_lparam(f64::NAN, f64::INFINITY);
        assert_eq!(unpack_mouse_lparam(lp.0), (0, 0));

        // Clamping to i16 boundaries
        let lp = mouse_point_lparam(-50000.0, 50000.0);
        assert_eq!(unpack_mouse_lparam(lp.0), (i16::MIN, i16::MAX));
    }

    #[test]
    fn point_within_client_rect_boundary_and_rejection() {
        // Standard 1080p client rect
        let (left, top, right, bottom) = (0, 0, 1920, 1080);

        // Origin and interior points
        assert!(is_point_within_client_rect(0, 0, left, top, right, bottom));
        assert!(is_point_within_client_rect(
            960, 540, left, top, right, bottom
        ));
        assert!(is_point_within_client_rect(
            1919, 1079, left, top, right, bottom
        ));

        // Out of bounds: right and bottom boundary (exclusive)
        assert!(!is_point_within_client_rect(
            1920, 540, left, top, right, bottom
        ));
        assert!(!is_point_within_client_rect(
            960, 1080, left, top, right, bottom
        ));

        // Out of bounds: negative coordinates (left and top)
        assert!(!is_point_within_client_rect(
            -1, 540, left, top, right, bottom
        ));
        assert!(!is_point_within_client_rect(
            960, -1, left, top, right, bottom
        ));

        // Large 8K surface (7680 x 4320)
        assert!(is_point_within_client_rect(7679, 4319, 0, 0, 7680, 4320));
        assert!(!is_point_within_client_rect(7680, 4320, 0, 0, 7680, 4320));

        // Empty rect
        assert!(!is_point_within_client_rect(0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn vlc_input_window_class_is_the_direct_main_window() {
        assert!(is_vlc_input_window_class("VLC video main 000001234567"));
        assert!(!is_vlc_input_window_class("VLC video output 000001234567"));
    }

    #[test]
    fn foreground_activation_requires_a_minimized_to_restored_transition() {
        use windows::Win32::UI::WindowsAndMessaging::{
            SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED,
        };
        let was_minimized = AtomicBool::new(false);
        assert!(!should_activate_after_restore(
            SIZE_RESTORED,
            &was_minimized
        ));
        assert!(!should_activate_after_restore(
            SIZE_MINIMIZED,
            &was_minimized
        ));
        assert!(should_activate_after_restore(SIZE_RESTORED, &was_minimized));
        assert!(!should_activate_after_restore(
            SIZE_RESTORED,
            &was_minimized
        ));
        assert!(!should_activate_after_restore(
            SIZE_MAXIMIZED,
            &was_minimized
        ));
    }

    #[test]
    fn overlay_app_handle_returns_none_before_init() {
        // OVERLAY_APP is a OnceLock; before overlay_app_init is called it must be empty.
        assert!(overlay_app_handle().is_none());
    }

    #[test]
    fn release_session_surface_protects_container_and_retires_active_slot() {
        VIDEO_HWND.store(0x1000, Ordering::SeqCst);
        ACTIVE_VIDEO_SLOT.store(0x2000, Ordering::SeqCst);
        RETIRED_VIDEO_SLOTS.lock().unwrap().clear();

        // Container surface and 0 must never be retired.
        release_session_surface(0);
        release_session_surface(0x1000);
        assert!(RETIRED_VIDEO_SLOTS.lock().unwrap().is_empty());
        assert_eq!(ACTIVE_VIDEO_SLOT.load(Ordering::SeqCst), 0x2000);

        // Retiring the active slot clears ACTIVE_VIDEO_SLOT and queues it once.
        release_session_surface(0x2000);
        release_session_surface(0x2000);
        assert_eq!(ACTIVE_VIDEO_SLOT.load(Ordering::SeqCst), 0);
        assert_eq!(RETIRED_VIDEO_SLOTS.lock().unwrap().as_slice(), &[0x2000]);

        RETIRED_VIDEO_SLOTS.lock().unwrap().clear();
        VIDEO_HWND.store(0, Ordering::SeqCst);
    }

    #[test]
    fn uhd_bluray_menu_point_scales_half_relative_to_active_rect() {
        // Full 1920x1080 client rect -> halved so VLC's 3840x2160 multiplier maps to 1920x1080.
        assert_eq!(
            scale_uhd_bluray_menu_point(960, 540, 0, 0, 1920, 1080),
            (480, 270)
        );
        // Letterboxed VLC video output inside larger window: preserve video origin offset.
        assert_eq!(
            scale_uhd_bluray_menu_point(900, 500, 100, 50, 1700, 950),
            (500, 275)
        );
        // Degenerate/empty rect leaves coordinates untouched.
        assert_eq!(
            scale_uhd_bluray_menu_point(123, 456, 0, 0, 0, 0),
            (123, 456)
        );
    }
}

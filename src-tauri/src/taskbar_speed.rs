//! Windows Taskbar Toolbar Speed Band Presentation for Nebulae.
//!
//! Presents upload/download transfer rates on a single layered toolbar item:
//!
//! ┌──────────────────────┐
//! │         1.25 MB/s ▲  │
//! │         8.40 MB/s ▼  │
//! └──────────────────────┘
//!
//! Positioned at the right side of the taskbar with consistent padding,
//! rendered inside a dark translucent pill backdrop for maximum legibility
//! across light, dark, and patterned wallpapers.
//!
//! Architecture:
//! - Speed text at `text_x = pill_pad_x` inside the pill.
//! - `band_layout` owns all dimensions; `render_band` reads them.
//! - `format_stacked_line` emits exactly 12 characters:
//!   5-char-padded-value + space + 4-char-unit + space + indicator.

/// Pixel geometry of the toolbar speed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct BandLayout {
    pub width: i32,
    pub height: i32,
    /// Left edge of the speed-text block inside the band.
    pub text_x: i32,
    /// Reserved pixel width for the speed-text block.
    pub text_reserve_w: i32,
    pub font_px: i32,
    /// Estimated rendered line height (~1.35x em) used to stack the two lines.
    pub line_h: i32,
    pub up_y: i32,
    pub down_y: i32,
    pub pill_pad_x: i32,
    pub pill_pad_y: i32,
}

/// Compute geometry derived from taskbar thickness `tray_h` and DPI scale.
///
/// # Scaling guarantee
/// Every dimension is derived from `dpi_scale` by a single `round()`.  At
/// integer DPI multiples (1×, 2×, 3×…) all pixel values are exact multiples
/// of their 1× base, so `band_layout(80).width == 2 * band_layout(40).width`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn band_layout(tray_h: i32) -> BandLayout {
    let tray_h = tray_h.max(30);
    let dpi_scale = (tray_h as f32 / 40.0).max(1.0);

    // Text reservation: 114 px at 1×, wide enough for 12 glyphs of
    // format_stacked_line output (5-char value + space + 4-char unit + space + indicator)
    // at ~9.5 px average advance.
    let text_reserve_w = (114.0 * dpi_scale).round() as i32;
    let pill_pad_x = (8.0 * dpi_scale).round() as i32;
    let pill_pad_y = (3.0 * dpi_scale).round() as i32;

    let width = text_reserve_w + pill_pad_x * 2;
    let text_x = pill_pad_x;

    // C17 (D13): derive the font and the stacked two-line block from the tray
    // height so the text is vertically centered at any taskbar thickness
    // (Windows 10 default 40px, small-buttons ~32px, high-DPI). Segoe UI line
    // height is ~1.35x the em size. The font shrinks to fit half-height lines
    // on thin taskbars instead of overflowing.
    let line_ratio = 1.35f32;
    let max_font = ((tray_h as f32 / 2.0) / line_ratio).floor() as i32;
    let font_px = (((13.0 * dpi_scale).round() as i32).max(9)).min(max_font.max(9));
    let line_h = (font_px as f32 * line_ratio).round() as i32;
    let block_h = line_h * 2;
    let up_y = ((tray_h - block_h) / 2).max(0);
    let down_y = up_y + line_h;

    BandLayout {
        width,
        height: tray_h,
        text_x,
        text_reserve_w,
        font_px,
        line_h,
        up_y,
        down_y,
        pill_pad_x,
        pill_pad_y,
    }
}

/// Left edge for the band inside the free span `[left_wall, right_wall]` of the
/// taskbar (between the last task button and whatever occupies the right end),
/// or `None` when the span cannot hold the whole band.  The band hugs the right
/// wall so it sits next to the neighbouring control rather than floating.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn band_x_in_free_span(
    left_wall: i32,
    right_wall: i32,
    band_w: i32,
    padding: i32,
) -> Option<i32> {
    if right_wall - left_wall < band_w {
        return None;
    }
    Some((right_wall - band_w - padding).max(left_wall))
}

#[cfg(not(target_os = "windows"))]
pub fn init_taskbar_speed(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn update_taskbar_speed(_app: &tauri::AppHandle, _up: u64, _down: u64) {}

#[cfg(not(target_os = "windows"))]
pub fn clear_taskbar_speed(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn set_taskbar_speed_enabled(_app: &tauri::AppHandle, _enabled: bool) {}

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU64, Ordering};
    use std::sync::{Once, OnceLock};
    use std::time::Instant;
    use tauri::{AppHandle, Manager};
    use windows::core::{w, Interface, BOOL, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetDC, GetMonitorInfoW,
        GetTextMetricsW, MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
        TextOutW, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, MONITORINFO,
        MONITOR_DEFAULTTONULL, OUT_DEFAULT_PRECIS, TEXTMETRICW, TRANSPARENT,
    };
    use windows::Win32::System::Com::{CoInitializeEx, IDispatch, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        AccessibleChildren, AccessibleObjectFromWindow, IAccessible,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, FindWindowExW, FindWindowW, GetCursorPos, GetForegroundWindow,
        GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassExW,
        SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu, UpdateLayeredWindow,
        HCURSOR, HICON, HWND_TOPMOST, MF_STRING, MSG, OBJID_CLIENT, PM_REMOVE, SWP_NOACTIVATE,
        SW_HIDE, SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, ULW_ALPHA,
        WM_APP, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_LBUTTONUP,
        WM_RBUTTONUP, WM_SETTINGCHANGE, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    const WM_RENDER_BAND: u32 = WM_APP + 1;
    /// Re-evaluates placement, fullscreen and free-space suppression on the band thread.
    const WM_REPOSITION_BAND: u32 = WM_APP + 2;
    static UP_RATE: AtomicU64 = AtomicU64::new(0);
    static DOWN_RATE: AtomicU64 = AtomicU64::new(0);
    static BAND_HWND: AtomicIsize = AtomicIsize::new(0);
    static BAND_W: AtomicI32 = AtomicI32::new(136);
    static BAND_H: AtomicI32 = AtomicI32::new(40);
    static INIT_ONCE: Once = Once::new();
    static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
    static OVERLAY_ENABLED: AtomicBool = AtomicBool::new(true);
    /// Set to true while a fullscreen window covers the band's monitor.
    /// Independent of OVERLAY_ENABLED — the band is hidden when either is
    /// true, but only OVERLAY_ENABLED reflects the user's Settings choice.
    static FULLSCREEN_SUPPRESSED: AtomicBool = AtomicBool::new(false);

    const CLASS_NAME: PCWSTR = w!("NebulaeToolbarBand");

    fn is_overlay_enabled(app: &AppHandle) -> bool {
        app.try_state::<std::sync::Arc<crate::db::Database>>()
            .and_then(|db| db.get_setting("user_settings_json").ok().flatten())
            .and_then(|json| {
                serde_json::from_str::<crate::commands::settings::UserSettings>(&json).ok()
            })
            .map(|s| s.show_taskbar_speed_overlay)
            .unwrap_or(true)
    }

    pub fn init_taskbar_speed(app: &AppHandle) {
        let _ = APP_HANDLE.set(app.clone());
        INIT_ONCE.call_once(|| {
            let initial_enabled = is_overlay_enabled(app);
            OVERLAY_ENABLED.store(initial_enabled, Ordering::SeqCst);
            std::thread::spawn(move || {
                run_toolbar_band_window();
            });
        });
    }

    pub fn set_taskbar_speed_enabled(app: &AppHandle, enabled: bool) {
        init_taskbar_speed(app);
        let _prev = OVERLAY_ENABLED.swap(enabled, Ordering::SeqCst);
        let hwnd_val = BAND_HWND.load(Ordering::Relaxed);
        if hwnd_val != 0 {
            let hwnd = HWND(hwnd_val as *mut _);
            unsafe {
                if enabled {
                    // Start from "suppressed" and let the band thread decide: it shows the
                    // band only when no fullscreen window covers its monitor and the taskbar
                    // has room, so re-enabling never flashes the band over fullscreen content.
                    FULLSCREEN_SUPPRESSED.store(true, Ordering::Relaxed);
                    let _ = PostMessageW(Some(hwnd), WM_REPOSITION_BAND, WPARAM(0), LPARAM(0));
                } else {
                    // User disabled: hide and clear any pending suppression state
                    // so the next enable starts clean.
                    FULLSCREEN_SUPPRESSED.store(false, Ordering::Relaxed);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }

    pub fn update_taskbar_speed(app: &AppHandle, up: u64, down: u64) {
        init_taskbar_speed(app);
        let prev_up = UP_RATE.swap(up, Ordering::Relaxed);
        let prev_down = DOWN_RATE.swap(down, Ordering::Relaxed);

        if !OVERLAY_ENABLED.load(Ordering::Relaxed) || FULLSCREEN_SUPPRESSED.load(Ordering::Relaxed)
        {
            return;
        }

        // Optimization: skip posting WM_RENDER_BAND when rates are identical
        if prev_up == up && prev_down == down {
            return;
        }

        let hwnd_val = BAND_HWND.load(Ordering::Relaxed);
        if hwnd_val != 0 {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_val as *mut _)),
                    WM_RENDER_BAND,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }

    pub fn clear_taskbar_speed(app: &AppHandle) {
        update_taskbar_speed(app, 0, 0);
    }

    fn run_toolbar_band_window() {
        unsafe {
            let hinstance = HINSTANCE(core::ptr::null_mut());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(3), // CS_HREDRAW | CS_VREDRAW
                lpfnWndProc: Some(band_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: HICON(core::ptr::null_mut()),
                hCursor: HCURSOR(core::ptr::null_mut()),
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(core::ptr::null_mut()),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: CLASS_NAME,
                hIconSm: HICON(core::ptr::null_mut()),
            };

            // The accessibility client calls used for placement need COM on this thread.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let _ = RegisterClassExW(&wc);

            let p = calculate_taskbar_position();
            BAND_W.store(p.layout.width, Ordering::Relaxed);
            BAND_H.store(p.layout.height, Ordering::Relaxed);

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_LAYERED,
                CLASS_NAME,
                w!("Nebulae Toolbar Item"),
                WS_POPUP | WS_CLIPSIBLINGS,
                p.x,
                p.y,
                p.w,
                p.h,
                None,
                None,
                Some(hinstance),
                None,
            );

            if let Ok(hwnd) = hwnd {
                BAND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                render_band(hwnd);
                // Only show at startup if overlay is enabled AND there is room.
                if OVERLAY_ENABLED.load(Ordering::Relaxed) && !p.hide_no_space {
                    let _ = ShowWindow(hwnd, SW_SHOWNA);
                } else {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }

                let mut msg = MSG::default();
                let mut last_pos_check = Instant::now();

                loop {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                            return;
                        }
                        let _ = DispatchMessageW(&msg);
                    }

                    if last_pos_check.elapsed().as_secs() >= 1 {
                        reposition_band(hwnd);
                        last_pos_check = Instant::now();
                    }

                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }

    /// Result of `calculate_taskbar_position`.
    ///
    /// `hide_no_space` is set when the free space between the last task button and
    /// the right-hand taskbar controls cannot hold the band, meaning it cannot be
    /// placed without overlapping adjacent taskbar controls.  The
    /// caller must hide the band in this case regardless of `OVERLAY_ENABLED`.
    struct BandPlacement {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        layout: super::BandLayout,
        /// True when the band must be suppressed due to insufficient taskbar space.
        hide_no_space: bool,
        /// Free span between the last task button and the right-hand controls (diagnostics).
        #[allow(dead_code)]
        span: (i32, i32),
    }

    /// Right edge (screen px) of the last real task button, read from the taskbar's
    /// accessibility tree.  The rebar and task-list windows are stretched to fill the
    /// whole taskbar, so their rectangles say nothing about where buttons end.
    /// Returns the task list's own left edge when it holds no buttons, and `None`
    /// when the tree cannot be read.
    fn task_buttons_right(tasklist: HWND, tasklist_rect: &RECT) -> Option<i32> {
        unsafe {
            let mut raw: *mut core::ffi::c_void = core::ptr::null_mut();
            AccessibleObjectFromWindow(
                tasklist,
                OBJID_CLIENT.0 as u32,
                &IAccessible::IID,
                &mut raw,
            )
            .ok()?;
            if raw.is_null() {
                return None;
            }
            let acc = IAccessible::from_raw(raw);
            let count = acc.accChildCount().ok()?;
            if !(0..=512).contains(&count) {
                return None;
            }
            let mut children = vec![VARIANT::default(); count as usize];
            let mut got = 0i32;
            AccessibleChildren(&acc, 0, &mut children, &mut got).ok()?;

            let mut right = tasklist_rect.left;
            for child in children.iter().take(got.max(0) as usize) {
                let (mut x, mut y, mut w, mut h) = (0i32, 0i32, 0i32, 0i32);
                let located = match IDispatch::try_from(child) {
                    Ok(disp) => disp.cast::<IAccessible>().ok().and_then(|c| {
                        c.accLocation(&mut x, &mut y, &mut w, &mut h, &VARIANT::from(0i32))
                            .ok()
                    }),
                    Err(_) => acc.accLocation(&mut x, &mut y, &mut w, &mut h, child).ok(),
                };
                // Group/header entries report an empty rectangle.
                if located.is_some() && w > 0 && h > 0 {
                    right = right.max(x + w);
                }
            }
            Some(right)
        }
    }

    fn calculate_taskbar_position() -> BandPlacement {
        unsafe {
            let h_tray = match FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => {
                    let l = super::band_layout(40);
                    return BandPlacement {
                        x: 100,
                        y: 1000,
                        w: l.width,
                        h: l.height,
                        layout: l,
                        hide_no_space: false,
                        span: (0, 0),
                    };
                }
            };

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);
            let dpi_scale = (tray_h as f32 / 40.0).max(1.0);
            let layout = super::band_layout(tray_h);
            let band_w = layout.width;
            let padding = (6.0 * dpi_scale).round() as i32;

            let h_rebar =
                FindWindowExW(Some(h_tray), None, w!("ReBarWindow32"), PCWSTR::null()).ok();

            // Right edge of the rebar: only a fallback, because the rebar spans all the
            // way to the notification area whether or not it is full.
            let rebar_right = if let Some(rebar) = h_rebar {
                let mut r = RECT::default();
                if GetWindowRect(rebar, &mut r).is_ok() && r.right > tray_rect.left {
                    r.right
                } else {
                    tray_rect.left
                }
            } else {
                tray_rect.left
            };

            // Locate TrafficMonitor (#32770) within ReBarWindow32 or Shell_TrayWnd
            let mut tm_hwnd = None;
            if let Some(rebar) = h_rebar {
                if let Ok(tm) = FindWindowExW(
                    Some(rebar),
                    None,
                    w!("#32770"),
                    w!("TrafficMonitorTaskbarWindow"),
                ) {
                    tm_hwnd = Some(tm);
                } else if let Ok(tm) =
                    FindWindowExW(Some(rebar), None, w!("#32770"), PCWSTR::null())
                {
                    tm_hwnd = Some(tm);
                }
            }
            if tm_hwnd.is_none() {
                if let Ok(tm) = FindWindowExW(
                    Some(h_tray),
                    None,
                    w!("#32770"),
                    w!("TrafficMonitorTaskbarWindow"),
                ) {
                    tm_hwnd = Some(tm);
                }
            }

            // Locate TrayNotifyWnd (notification area / clock / system icons on the right)
            let h_notify =
                FindWindowExW(Some(h_tray), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok();
            let mut notify_rect = RECT::default();
            let notify_found = if let Some(notify) = h_notify {
                GetWindowRect(notify, &mut notify_rect).is_ok()
            } else {
                false
            };

            // The right wall: left edge of TrayNotifyWnd, or tray right.
            let mut right_wall = if notify_found && notify_rect.left > tray_rect.left + 50 {
                notify_rect.left
            } else {
                tray_rect.right
            };

            // A taskbar widget that sits against the notification area (TrafficMonitor)
            // owns the space to its right, so the band goes on its left.
            if let Some(tm) = tm_hwnd {
                if IsWindowVisible(tm).as_bool() {
                    let mut tm_rect = RECT::default();
                    if GetWindowRect(tm, &mut tm_rect).is_ok()
                        && tm_rect.left > tray_rect.left
                        && tm_rect.left < right_wall
                        && tm_rect.right >= right_wall - padding * 3
                    {
                        right_wall = tm_rect.left;
                    }
                }
            }

            // The left wall: the end of the last task button.  Without a readable
            // accessibility tree fall back to the task list's right edge, then the rebar's.
            let tasklist = h_rebar.and_then(|rebar| {
                let sw =
                    FindWindowExW(Some(rebar), None, w!("MSTaskSwWClass"), PCWSTR::null()).ok()?;
                FindWindowExW(Some(sw), None, w!("MSTaskListWClass"), PCWSTR::null()).ok()
            });
            let left_wall = match tasklist {
                Some(tl) => {
                    let mut tl_rect = RECT::default();
                    if GetWindowRect(tl, &mut tl_rect).is_ok() {
                        match task_buttons_right(tl, &tl_rect) {
                            Some(r) => r + padding,
                            None => tl_rect.right,
                        }
                    } else {
                        rebar_right.max(tray_rect.left + 20)
                    }
                }
                None => rebar_right.max(tray_rect.left + 20),
            };

            match super::band_x_in_free_span(left_wall, right_wall, band_w, padding) {
                Some(x) => BandPlacement {
                    x,
                    y: tray_rect.top,
                    w: band_w,
                    h: tray_h,
                    layout,
                    hide_no_space: false,
                    span: (left_wall, right_wall),
                },
                None => BandPlacement {
                    x: left_wall,
                    y: tray_rect.top,
                    w: band_w,
                    h: tray_h,
                    layout,
                    hide_no_space: true,
                    span: (left_wall, right_wall),
                },
            }
        }
    }

    /// Return true if the foreground window on the same monitor as `band_hwnd`
    /// covers the full monitor work area (i.e. is running exclusive fullscreen).
    ///
    /// Uses GetForegroundWindow + MonitorFromWindow + GetMonitorInfoW so the
    /// check is confined to the monitor the band lives on; fullscreen windows
    /// on other monitors do not suppress the band.
    fn foreground_is_fullscreen_on_band_monitor(band_hwnd: HWND) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.0.is_null() {
                return false;
            }
            // Skip our own window.
            if fg == band_hwnd {
                return false;
            }
            // Monitor the band is on.
            let band_mon = MonitorFromWindow(band_hwnd, MONITOR_DEFAULTTONULL);
            if band_mon.is_invalid() {
                return false;
            }
            // Monitor the foreground window is on.
            let fg_mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
            if fg_mon.is_invalid() || fg_mon != band_mon {
                // Fullscreen on a different monitor — do not suppress.
                return false;
            }
            // Get the monitor's full screen rect (not the work area).
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(band_mon, &mut mi).as_bool() {
                return false;
            }
            // Get the foreground window's rect.
            let mut fg_rect = RECT::default();
            if GetWindowRect(fg, &mut fg_rect).is_err() {
                return false;
            }
            // The window is fullscreen when it fills the entire monitor rect
            // (not just the work area).  A ±2 px tolerance handles rounding.
            let mon = mi.rcMonitor;
            fg_rect.left <= mon.left + 2
                && fg_rect.top <= mon.top + 2
                && fg_rect.right >= mon.right - 2
                && fg_rect.bottom >= mon.bottom - 2
        }
    }

    fn reposition_band(hwnd: HWND) {
        // 1. If overlay is disabled, hide once if currently visible and do no positioning work
        if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
            unsafe {
                if IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    FULLSCREEN_SUPPRESSED.store(false, Ordering::Relaxed);
                }
            }
            return;
        }

        // 2. The band must be hidden when it has no room to fit without overlapping controls,
        // or when a fullscreen window covers its monitor.
        let p = calculate_taskbar_position();
        let now_fullscreen = foreground_is_fullscreen_on_band_monitor(hwnd);
        let want_hidden = p.hide_no_space || now_fullscreen;
        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
        FULLSCREEN_SUPPRESSED.store(want_hidden, Ordering::Relaxed);

        if want_hidden {
            if visible {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                tracing::info!(
                    hidden = true,
                    no_space = p.hide_no_space,
                    fullscreen = now_fullscreen,
                    x = p.x,
                    width = p.w,
                    "taskbar speed band suppressed and hidden"
                );
            }
            return;
        }

        // 3. Only position and assert topmost when enabled and not suppressed
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                p.x,
                p.y,
                p.w,
                p.h,
                windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(
                    SWP_NOACTIVATE.0 | windows::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER.0,
                ),
            );
        }
        BAND_W.store(p.layout.width, Ordering::Relaxed);
        BAND_H.store(p.layout.height, Ordering::Relaxed);

        unsafe {
            if !visible {
                let _ = ShowWindow(hwnd, SW_SHOWNA);
                let _ = PostMessageW(Some(hwnd), WM_RENDER_BAND, WPARAM(0), LPARAM(0));
            }
        }
    }

    unsafe extern "system" fn band_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_RENDER_BAND => {
                render_band(hwnd);
                LRESULT(0)
            }
            WM_REPOSITION_BAND => {
                reposition_band(hwnd);
                let _ = PostMessageW(Some(hwnd), WM_RENDER_BAND, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                activate_main_window();
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                show_context_menu(hwnd);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 as u16 {
                    1001 => activate_main_window(),
                    1002 => PostQuitMessage(0),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            // Reposition immediately on display resolution change, DPI change, or
            // taskbar-layout change (e.g. auto-hide toggle, taskbar moved to side).
            // These events arrive on the band's own message queue because the window
            // is a top-level HWND; we do not need to register for them separately.
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                reposition_band(hwnd);
                let _ = PostMessageW(Some(hwnd), WM_RENDER_BAND, WPARAM(0), LPARAM(0));
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn render_band(hwnd: HWND) {
        let width = BAND_W.load(Ordering::Relaxed).max(60);
        let height = BAND_H.load(Ordering::Relaxed).max(30);
        let layout = super::band_layout(height);

        let hdc_screen = GetDC(None);
        let mem_dc = windows::Win32::Graphics::Gdi::CreateCompatibleDC(Some(hdc_screen));

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // Top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
        };

        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let dib = match CreateDIBSection(Some(hdc_screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        {
            Ok(d) => d,
            Err(_) => {
                let _ = DeleteDC(mem_dc);
                ReleaseDC(None, hdc_screen);
                return;
            }
        };

        let old_bmp = SelectObject(mem_dc, dib.into());
        let len = (width * height * 4) as usize;
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, len);
        pixels.fill(0); // Fully transparent background: no solid or dark pill backdrop

        // 1. Draw stacked transfer rates directly onto the transparent surface.
        //
        // format_stacked_line emits exactly 12 characters:
        // "<5-char val> <4-char unit> <indicator>" e.g. "    0  B/s ▲"
        let up = UP_RATE.load(Ordering::Relaxed);
        let down = DOWN_RATE.load(Ordering::Relaxed);

        let up_line = crate::speed_format::format_stacked_line('\u{25b2}', up); // ▲
        let down_line = crate::speed_format::format_stacked_line('\u{25bc}', down); // ▼

        let font = CreateFontW(
            layout.font_px,
            0,
            0,
            0,
            700, // FW_BOLD for crisp legibility over any background
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(mem_dc, font.into());
        let _ = SetBkMode(mem_dc, TRANSPARENT);

        let text_x = layout.text_x;

        // C17 (D13): center the stacked lines on the ACTUAL font metrics of the
        // selected font, so the block is vertically centered whatever font GDI
        // resolved (Segoe UI at any DPI, or a fallback). layout.up_y/down_y are
        // the estimate; this measured centering wins when metrics are available.
        let (up_y, down_y) = {
            let mut tm = TEXTMETRICW::default();
            let measured_line = if GetTextMetricsW(mem_dc, &mut tm).as_bool() && tm.tmHeight > 0 {
                tm.tmHeight
            } else {
                layout.line_h
            };
            let measured_line = measured_line.max(layout.font_px);
            let block = measured_line * 2;
            let top = ((height - block) / 2).max(0);
            (top, top + measured_line)
        };

        // Upper line: Upload rate in vibrant Warm Amber/Orange (BGR 0x0032AAFF = RGB 255, 170, 50)
        let _ = SetTextColor(mem_dc, COLORREF(0x0032AAFF));
        let up_wide: Vec<u16> = up_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, up_y, &up_wide);

        // Lower line: Download rate in vibrant Aqua/Cyan (BGR 0x00FFDC3C = RGB 60, 220, 255)
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFDC3C));
        let down_wide: Vec<u16> = down_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, down_y, &down_wide);

        let _ = GdiFlush();

        // 2. Build text glyph mask from GDI rendered pixels.
        // GDI antialiasing writes color channels over the black (0,0,0) buffer.
        let mut text_mask = vec![false; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let b = pixels[idx];
                let g = pixels[idx + 1];
                let r = pixels[idx + 2];
                if r > 18 || g > 18 || b > 18 {
                    text_mask[(y * width + x) as usize] = true;
                }
            }
        }

        // 3. Post-pass: Apply outline and drop-shadow halo for legibility without any background box.
        // - Text pixels: Opaque (alpha 255) vibrant color.
        // - 1px outline/shadow neighbor: Black with alpha ~220 (~86% darkening).
        // - 2px drop-shadow neighbor: Black with alpha ~120 (~47% soft falloff).
        // - All other pixels: Alpha 0 (completely transparent).
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let pos = (y * width + x) as usize;

                if text_mask[pos] {
                    // Crisp foreground text
                    pixels[idx + 3] = 255;
                } else {
                    // Check for nearby text to generate outline/shadow
                    let mut near_dist1 = false;
                    let mut near_dist2 = false;

                    // 8-way 1px outline and 1px drop shadow
                    'outer1: for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = x + dx;
                            let ny = y + dy;
                            if nx >= 0
                                && nx < width
                                && ny >= 0
                                && ny < height
                                && text_mask[(ny * width + nx) as usize]
                            {
                                near_dist1 = true;
                                break 'outer1;
                            }
                        }
                    }

                    if !near_dist1 {
                        // Directional drop shadow at distance 2 (down and right)
                        for (dx, dy) in [(0, -2), (-1, -2), (-2, -1), (-2, 0), (-2, -2)] {
                            let nx = x + dx;
                            let ny = y + dy;
                            if nx >= 0
                                && nx < width
                                && ny >= 0
                                && ny < height
                                && text_mask[(ny * width + nx) as usize]
                            {
                                near_dist2 = true;
                                break;
                            }
                        }
                    }

                    if near_dist1 {
                        pixels[idx] = 0;
                        pixels[idx + 1] = 0;
                        pixels[idx + 2] = 0;
                        pixels[idx + 3] = 220; // High contrast outline
                    } else if near_dist2 {
                        pixels[idx] = 0;
                        pixels[idx + 1] = 0;
                        pixels[idx + 2] = 0;
                        pixels[idx + 3] = 120; // Soft shadow fringe
                    } else {
                        pixels[idx] = 0;
                        pixels[idx + 1] = 0;
                        pixels[idx + 2] = 0;
                        pixels[idx + 3] = 0; // Fully transparent
                    }
                }
            }
        }

        let size = SIZE {
            cx: width,
            cy: height,
        };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let _ = UpdateLayeredWindow(
            hwnd,
            Some(hdc_screen),
            None,
            Some(&size),
            Some(mem_dc),
            Some(&src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        let _ = SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font.into());
        let _ = SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, hdc_screen);
    }

    unsafe extern "system" fn enum_find_nebulae(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut proc_id = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut proc_id));
        if proc_id != std::process::id() {
            return BOOL(1);
        }

        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(hwnd, &mut buf);
            if read > 0 {
                let title = String::from_utf16_lossy(&buf[..read as usize]);
                // Ignore the video surface child and toolbar band
                if title.contains("Nebulae")
                    && !title.contains("Toolbar")
                    && !title.contains("Video Surface")
                {
                    *(lparam.0 as *mut HWND) = hwnd;
                    return BOOL(0);
                }
            }
        }
        BOOL(1)
    }

    fn activate_main_window() {
        if let Some(app) = APP_HANDLE.get() {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
                if let Ok(hwnd) = win.hwnd() {
                    unsafe {
                        let _ = SetForegroundWindow(hwnd);
                    }
                }
                return;
            }
        }

        // Secondary fallback if AppHandle is uninitialized
        unsafe {
            let mut target_hwnd = HWND(core::ptr::null_mut());
            let _ = windows::Win32::UI::WindowsAndMessaging::EnumWindows(
                Some(enum_find_nebulae),
                LPARAM(&mut target_hwnd as *mut _ as isize),
            );
            if !target_hwnd.0.is_null() {
                let _ = ShowWindow(target_hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(target_hwnd);
            }
        }
    }

    unsafe fn show_context_menu(hwnd: HWND) {
        let hmenu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return,
        };

        let _ = AppendMenuW(hmenu, MF_STRING, 1001, w!("Open Nebulae"));
        let _ = AppendMenuW(hmenu, MF_STRING, 1002, w!("Quit Nebulae"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            hmenu,
            TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::band_layout;

    #[test]
    fn geometry_scales_with_taskbar_thickness() {
        let l40 = band_layout(40);
        let l80 = band_layout(80);

        assert_eq!(l40.height, 40);
        assert_eq!(l80.height, 80);
        assert_eq!(l80.width, l40.width * 2, "width");
        assert_eq!(l80.text_reserve_w, l40.text_reserve_w * 2, "text_reserve_w");
        assert_eq!(l80.text_x, l40.text_x * 2, "text_x");
        assert_eq!(l80.pill_pad_x, l40.pill_pad_x * 2, "pill_pad_x");
        assert_eq!(l80.pill_pad_y, l40.pill_pad_y * 2, "pill_pad_y");
        assert!(l80.font_px > l40.font_px, "font_px must grow");
    }

    #[test]
    fn free_span_requires_room_for_the_whole_band() {
        use super::band_x_in_free_span;
        // Windows 10 at 200%: last task button ends at 2071+12, TrafficMonitor starts at 2577.
        let band_w = band_layout(80).width;
        assert_eq!(band_w, 260);
        assert_eq!(
            band_x_in_free_span(2083, 2577, band_w, 12),
            Some(2577 - 260 - 12)
        );
        // Exactly fitting: no padding available, hug the right wall without crossing the left.
        assert_eq!(band_x_in_free_span(2317, 2577, band_w, 12), Some(2317));
        // One pixel short: hidden rather than overlapping a task button.
        assert_eq!(band_x_in_free_span(2318, 2577, band_w, 12), None);
        // Taskbar full of buttons.
        assert_eq!(band_x_in_free_span(2570, 2577, band_w, 12), None);
    }

    #[test]
    fn text_x_and_width_consistency() {
        for tray_h in [30, 40, 48, 60, 80, 96] {
            let l = band_layout(tray_h);
            assert_eq!(l.text_x, l.pill_pad_x);
            assert_eq!(l.width, l.text_reserve_w + l.pill_pad_x * 2);
            assert!(l.text_x + l.text_reserve_w <= l.width);
        }
    }

    #[test]
    fn layout_reserves_space_without_overlap() {
        for tray_h in [30, 40, 48, 60, 80, 96] {
            let l = band_layout(tray_h);
            assert!(l.down_y > l.up_y);
            assert!(l.down_y + l.font_px <= l.height + 2);
        }
    }

    #[test]
    fn c17_stacked_block_is_vertically_centered() {
        // C17 (D13): the two-line text block must be centered in the band at
        // every taskbar thickness, not parked at fixed offsets from the top
        // (the old 40px-only assumption behind the user's misalignment report).
        for tray_h in [30, 32, 34, 36, 40, 48, 60, 80, 96] {
            let l = band_layout(tray_h);
            let top_gap = l.up_y;
            let bottom_gap = tray_h - (l.down_y + l.line_h);
            assert!(
                (top_gap - bottom_gap).abs() <= 2,
                "tray_h={tray_h}: top_gap={top_gap}, bottom_gap={bottom_gap}"
            );
            // The whole block fits inside the band.
            assert!(top_gap >= 0 && bottom_gap >= 0, "tray_h={tray_h}");
            // Lines are stacked with no gap larger than one line.
            assert_eq!(l.down_y - l.up_y, l.line_h, "tray_h={tray_h}");
        }
    }

    #[test]
    fn c17_font_shrinks_to_fit_thin_taskbars() {
        // Small taskbar buttons (~32px at 1x): a 13px font no longer fits two
        // stacked lines; the font must shrink so the block fits the height.
        for tray_h in [30, 32, 34, 36] {
            let l = band_layout(tray_h);
            assert!(
                l.up_y + 2 * l.line_h <= tray_h,
                "tray_h={tray_h}: block overflows (font={}, line_h={})",
                l.font_px,
                l.line_h
            );
            assert!(l.font_px >= 9, "tray_h={tray_h}: font collapsed below 9px");
        }
        // Standard 40px taskbar keeps the 13px font.
        assert_eq!(band_layout(40).font_px, 13);
    }
}

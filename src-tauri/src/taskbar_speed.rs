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

    let font_px = (13.0 * dpi_scale).round() as i32;
    let up_y = (tray_h / 2) - (14.0 * dpi_scale).round() as i32;
    let down_y = (tray_h / 2) + (1.0 * dpi_scale).round() as i32;

    BandLayout {
        width,
        height: tray_h,
        text_x,
        text_reserve_w,
        font_px,
        up_y,
        down_y,
        pill_pad_x,
        pill_pad_y,
    }
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
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetDC, GetMonitorInfoW,
        MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW,
        AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, MONITORINFO,
        MONITOR_DEFAULTTONULL, OUT_DEFAULT_PRECIS, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, FindWindowExW, FindWindowW, GetCursorPos, GetForegroundWindow,
        GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassExW,
        SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu, UpdateLayeredWindow,
        HCURSOR, HICON, HWND_TOPMOST, MF_STRING, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_HIDE,
        SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP,
        WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_CLIPSIBLINGS,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    const WM_RENDER_BAND: u32 = WM_APP + 1;

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
                    // Only show if not currently suppressed by a fullscreen window.
                    if !FULLSCREEN_SUPPRESSED.load(Ordering::Relaxed) {
                        let _ = ShowWindow(hwnd, SW_SHOWNA);
                        let _ = PostMessageW(Some(hwnd), WM_RENDER_BAND, WPARAM(0), LPARAM(0));
                    }
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

            let _ = RegisterClassExW(&wc);

            let (init_x, init_y, init_w, init_h, layout) = calculate_taskbar_position();
            BAND_W.store(layout.width, Ordering::Relaxed);
            BAND_H.store(layout.height, Ordering::Relaxed);

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_LAYERED,
                CLASS_NAME,
                w!("Nebulae Toolbar Item"),
                WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS,
                init_x,
                init_y,
                init_w,
                init_h,
                None,
                None,
                Some(hinstance),
                None,
            );

            if let Ok(hwnd) = hwnd {
                BAND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                render_band(hwnd);
                if OVERLAY_ENABLED.load(Ordering::Relaxed) {
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

    fn calculate_taskbar_position() -> (i32, i32, i32, i32, super::BandLayout) {
        unsafe {
            let h_tray = match FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => {
                    let l = super::band_layout(40);
                    return (100, 1000, l.width, l.height, l);
                }
            };

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);
            let dpi_scale = (tray_h as f32 / 40.0).max(1.0);
            let layout = super::band_layout(tray_h);
            let band_w = layout.width;

            let h_rebar =
                FindWindowExW(Some(h_tray), None, w!("ReBarWindow32"), PCWSTR::null()).ok();

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

            // Locate TrayNotifyWnd (notification area / system tray on right)
            let h_notify =
                FindWindowExW(Some(h_tray), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok();
            let mut notify_rect = RECT::default();
            let notify_found = if let Some(notify) = h_notify {
                GetWindowRect(notify, &mut notify_rect).is_ok()
            } else {
                false
            };

            let padding = (6.0 * dpi_scale).round() as i32;
            let mut target_x = 0;

            // Priority 1: If TrafficMonitor is present and visibly shown, anchor to its left
            if let Some(tm) = tm_hwnd {
                if IsWindowVisible(tm).as_bool() {
                    let mut tm_rect = RECT::default();
                    if GetWindowRect(tm, &mut tm_rect).is_ok()
                        && tm_rect.left > tray_rect.left + 100
                    {
                        target_x = tm_rect.left - band_w - padding;
                    }
                }
            }

            // Priority 2: Anchor immediately to the left of TrayNotifyWnd (notification area / system icons)
            if target_x == 0 && notify_found && notify_rect.left > tray_rect.left + 100 {
                target_x = notify_rect.left - band_w - padding;
            }

            // Priority 3: Fallback inside tray bounds right-aligned
            if target_x == 0 {
                target_x = tray_rect.right - band_w - (10.0 * dpi_scale).round() as i32;
            }

            if target_x < tray_rect.left + 50 {
                target_x = tray_rect.left + 50;
            }

            (target_x, tray_rect.top, band_w, tray_h, layout)
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
        let (x, y, w, h, layout) = calculate_taskbar_position();
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        }
        BAND_W.store(layout.width, Ordering::Relaxed);
        BAND_H.store(layout.height, Ordering::Relaxed);

        // Suppress the overlay while a fullscreen application covers the band's
        // monitor.  Never suppress if the user disabled the overlay entirely.
        if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        let was_suppressed = FULLSCREEN_SUPPRESSED.load(Ordering::Relaxed);
        let now_fullscreen = foreground_is_fullscreen_on_band_monitor(hwnd);
        if now_fullscreen == was_suppressed {
            return; // No state change — nothing to do.
        }
        FULLSCREEN_SUPPRESSED.store(now_fullscreen, Ordering::Relaxed);
        unsafe {
            if now_fullscreen {
                // Hide without taking focus.
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                // Restore without taking focus or activating Nebulae.
                let _ = ShowWindow(hwnd, SW_SHOWNA);
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
                    1002 => {
                        let _ = PostQuitMessage(0);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = PostQuitMessage(0);
                LRESULT(0)
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

        let mut bmi = BITMAPINFO {
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
        let dib = match CreateDIBSection(
            Some(hdc_screen),
            &mut bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        ) {
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

        // Upper line: Upload rate in vibrant Warm Amber/Orange (BGR 0x0032AAFF = RGB 255, 170, 50)
        let _ = SetTextColor(mem_dc, COLORREF(0x0032AAFF));
        let up_wide: Vec<u16> = up_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, layout.up_y, &up_wide);

        // Lower line: Download rate in vibrant Aqua/Cyan (BGR 0x00FFDC3C = RGB 60, 220, 255)
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFDC3C));
        let down_wide: Vec<u16> = down_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, layout.down_y, &down_wide);

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
                            if nx >= 0 && nx < width && ny >= 0 && ny < height {
                                if text_mask[(ny * width + nx) as usize] {
                                    near_dist1 = true;
                                    break 'outer1;
                                }
                            }
                        }
                    }

                    if !near_dist1 {
                        // Directional drop shadow at distance 2 (down and right)
                        for (dx, dy) in [(0, -2), (-1, -2), (-2, -1), (-2, 0), (-2, -2)] {
                            let nx = x + dx;
                            let ny = y + dy;
                            if nx >= 0 && nx < width && ny >= 0 && ny < height {
                                if text_mask[(ny * width + nx) as usize] {
                                    near_dist2 = true;
                                    break;
                                }
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
}

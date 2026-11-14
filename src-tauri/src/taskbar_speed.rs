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

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicU64, Ordering};
    use std::sync::Once;
    use std::time::Instant;
    use tauri::AppHandle;
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject, FillRect,
        GdiFlush, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_ALPHA,
        AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, CLEARTYPE_QUALITY,
        CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, HDC, OUT_DEFAULT_PRECIS, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, PeekMessageW, PostMessageW,
        PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow,
        TrackPopupMenu, UpdateLayeredWindow, HCURSOR, HICON, HWND_TOPMOST, MF_STRING, MSG,
        PM_REMOVE, SWP_NOACTIVATE, SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP,
        WNDCLASSEXW, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    const WM_RENDER_BAND: u32 = WM_APP + 1;

    static UP_RATE: AtomicU64 = AtomicU64::new(0);
    static DOWN_RATE: AtomicU64 = AtomicU64::new(0);
    static BAND_HWND: AtomicIsize = AtomicIsize::new(0);
    static BAND_W: AtomicI32 = AtomicI32::new(136);
    static BAND_H: AtomicI32 = AtomicI32::new(40);
    static INIT_ONCE: Once = Once::new();

    const CLASS_NAME: PCWSTR = w!("NebulaeToolbarBand");

    pub fn init_taskbar_speed(_app: &AppHandle) {
        INIT_ONCE.call_once(|| {
            std::thread::spawn(move || {
                run_toolbar_band_window();
            });
        });
    }

    pub fn update_taskbar_speed(app: &AppHandle, up: u64, down: u64) {
        init_taskbar_speed(app);
        UP_RATE.store(up, Ordering::Relaxed);
        DOWN_RATE.store(down, Ordering::Relaxed);

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
                let _ = ShowWindow(hwnd, SW_SHOWNA);

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

    fn reposition_band(hwnd: HWND) {
        let (x, y, w, h, layout) = calculate_taskbar_position();
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        }
        BAND_W.store(layout.width, Ordering::Relaxed);
        BAND_H.store(layout.height, Ordering::Relaxed);
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
        pixels.fill(0); // Fully transparent background outside pill

        // 1. Draw dark translucent rounded pill background directly into pixels
        let pill_left = layout.pill_pad_x / 2;
        let pill_right = width - (layout.pill_pad_x / 2);
        let pill_top = layout.pill_pad_y;
        let pill_bottom = height - layout.pill_pad_y;
        let pill_radius = ((pill_bottom - pill_top) / 3).max(4);

        // Pre-multiplied colors:
        // Pill background: Dark slate charcoal RGB(18, 20, 24), alpha = 225 (~88% opacity)
        let bg_a: u32 = 225;
        let bg_r: u32 = (18 * bg_a) / 255;
        let bg_g: u32 = (20 * bg_a) / 255;
        let bg_b: u32 = (24 * bg_a) / 255;

        // Pill border: Subtle lighter edge RGB(70, 75, 85), alpha = 180
        let border_a: u32 = 180;
        let border_r: u32 = (70 * border_a) / 255;
        let border_g: u32 = (75 * border_a) / 255;
        let border_b: u32 = (85 * border_a) / 255;

        for y in pill_top..pill_bottom {
            for x in pill_left..pill_right {
                let is_corner_tl = x < pill_left + pill_radius && y < pill_top + pill_radius;
                let is_corner_tr = x >= pill_right - pill_radius && y < pill_top + pill_radius;
                let is_corner_bl = x < pill_left + pill_radius && y >= pill_bottom - pill_radius;
                let is_corner_br = x >= pill_right - pill_radius && y >= pill_bottom - pill_radius;

                let mut in_pill = true;
                let mut is_border =
                    x == pill_left || x == pill_right - 1 || y == pill_top || y == pill_bottom - 1;

                if is_corner_tl {
                    let dx = (pill_left + pill_radius) - x;
                    let dy = (pill_top + pill_radius) - y;
                    let dsq = dx * dx + dy * dy;
                    if dsq > pill_radius * pill_radius {
                        in_pill = false;
                    } else if dsq >= (pill_radius - 1) * (pill_radius - 1) {
                        is_border = true;
                    }
                } else if is_corner_tr {
                    let dx = x - (pill_right - pill_radius - 1);
                    let dy = (pill_top + pill_radius) - y;
                    let dsq = dx * dx + dy * dy;
                    if dsq > pill_radius * pill_radius {
                        in_pill = false;
                    } else if dsq >= (pill_radius - 1) * (pill_radius - 1) {
                        is_border = true;
                    }
                } else if is_corner_bl {
                    let dx = (pill_left + pill_radius) - x;
                    let dy = y - (pill_bottom - pill_radius - 1);
                    let dsq = dx * dx + dy * dy;
                    if dsq > pill_radius * pill_radius {
                        in_pill = false;
                    } else if dsq >= (pill_radius - 1) * (pill_radius - 1) {
                        is_border = true;
                    }
                } else if is_corner_br {
                    let dx = x - (pill_right - pill_radius - 1);
                    let dy = y - (pill_bottom - pill_radius - 1);
                    let dsq = dx * dx + dy * dy;
                    if dsq > pill_radius * pill_radius {
                        in_pill = false;
                    } else if dsq >= (pill_radius - 1) * (pill_radius - 1) {
                        is_border = true;
                    }
                }

                if in_pill {
                    let idx = ((y * width + x) * 4) as usize;
                    if is_border {
                        pixels[idx] = border_b as u8;
                        pixels[idx + 1] = border_g as u8;
                        pixels[idx + 2] = border_r as u8;
                        pixels[idx + 3] = border_a as u8;
                    } else {
                        pixels[idx] = bg_b as u8;
                        pixels[idx + 1] = bg_g as u8;
                        pixels[idx + 2] = bg_r as u8;
                        pixels[idx + 3] = bg_a as u8;
                    }
                }
            }
        }

        // 2. Draw stacked transfer rates inside the pill.
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

        // 3. Alpha post-pass:
        // In the text rendering region, set alpha to 255 for pixels rendered by GDI
        // (where RGB was modified from the dark background).
        let text_x_start = text_x.max(0);
        let text_x_end = (text_x + layout.text_reserve_w).min(width);
        let text_y_start = layout.up_y.max(0);
        let text_y_end = (layout.down_y + layout.font_px + 2).min(height);

        for y in text_y_start..text_y_end {
            for x in text_x_start..text_x_end {
                let idx = ((y * width + x) * 4) as usize;
                let b = pixels[idx] as i32;
                let g = pixels[idx + 1] as i32;
                let r = pixels[idx + 2] as i32;
                if (r - bg_r as i32).abs() > 15
                    || (g - bg_g as i32).abs() > 15
                    || (b - bg_b as i32).abs() > 15
                {
                    pixels[idx + 3] = 255;
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
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(hwnd, &mut buf);
            if read > 0 {
                let title = String::from_utf16_lossy(&buf[..read as usize]);
                if title.contains("Nebulae") && !title.contains("Toolbar") {
                    *(lparam.0 as *mut HWND) = hwnd;
                    return BOOL(0);
                }
            }
        }
        BOOL(1)
    }

    fn activate_main_window() {
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

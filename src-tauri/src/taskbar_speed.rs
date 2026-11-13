//! Windows Taskbar Toolbar Speed Band Presentation for Nebulae.
//!
//! Presents the Nebulae application icon on the LEFT and upload/download
//! transfer rates on the RIGHT of a single layered toolbar item:
//!
//! ┌──────────────────────────────────────┐
//! │  [Icon]                 ▲  1.25 MB/s │
//! │                         ▼  8.40 MB/s │
//! └──────────────────────────────────────┘
//!
//! The speed block is anchored to the RIGHT edge of the band and is entirely
//! independent of the icon position.  Changing number magnitudes (B/s → GB/s)
//! do NOT shift the layout because the text block width is reserved up front.
//!
//! Architecture:
//! - Icon at `icon_x` (left margin).
//! - Speed text at `text_x = width - text_reserve_w` (right-anchored).
//! - `band_layout` owns both coordinates; `render_band` reads them.
//! - `format_stacked_line` always emits exactly 12 characters:
//!   indicator + figure-space + 5-char-padded-value + space + 4-char-unit.

/// Pixel geometry of the unified toolbar speed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct BandLayout {
    pub width: i32,
    pub height: i32,
    pub icon_size: i32,
    pub icon_x: i32,
    pub icon_y: i32,
    /// Left edge of the speed-text block, anchored from the right of the band.
    pub text_x: i32,
    /// Reserved pixel width for the speed-text block (right side of band).
    pub text_reserve_w: i32,
    pub font_px: i32,
    pub up_y: i32,
    pub down_y: i32,
}

/// Compute geometry derived from taskbar thickness `tray_h` and DPI scale.
///
/// # Layout invariant
/// `text_x + text_reserve_w == width`, so the speed block always fills the
/// right side regardless of icon position or rate magnitude.
///
/// # Scaling guarantee
/// Every dimension is derived from `dpi_scale` by a single `round()`.  At
/// integer DPI multiples (1×, 2×, 3×…) all pixel values are exact multiples
/// of their 1× base, so `band_layout(80).width == 2 * band_layout(40).width`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn band_layout(tray_h: i32) -> BandLayout {
    let tray_h = tray_h.max(30);
    let dpi_scale = (tray_h as f32 / 40.0).max(1.0);

    // All base constants are defined at 1× DPI (tray_h = 40).
    // Each is rounded independently so the result is an integer multiple
    // of the base at integer DPI scales.
    let icon_x = (6.0 * dpi_scale).round() as i32; //  6 px at 1×
    let icon_size = (26.0 * dpi_scale).round() as i32; // 26 px at 1×
    let gap = (8.0 * dpi_scale).round() as i32; //  8 px at 1×
    let font_px = (12.5 * dpi_scale).round() as i32; // 13 px at 1×

    // Icon slot: left margin + icon + gap to text.
    let icon_slot_w = icon_x + icon_size + gap; // 40 px at 1×

    // Text reservation: 114 px at 1×, wide enough for 12 glyphs of
    // format_stacked_line output (indicator + 5-char value + 4-char unit)
    // at ~9.5 px average advance.  Derived from dpi_scale directly (not from
    // font_px) so round(114 * 2) == 2 * round(114 * 1) holds exactly.
    let text_reserve_w = (114.0 * dpi_scale).round() as i32;

    let width = icon_slot_w + text_reserve_w;
    let text_x = icon_slot_w; // == width - text_reserve_w

    let icon_y = (tray_h - icon_size) / 2;
    let up_y = (tray_h / 2) - (14.0 * dpi_scale).round() as i32;
    let down_y = (tray_h / 2) + (1.0 * dpi_scale).round() as i32;

    BandLayout {
        width,
        height: tray_h,
        icon_size,
        icon_x,
        icon_y,
        text_x,
        text_reserve_w,
        font_px,
        up_y,
        down_y,
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
        DispatchMessageW, DrawIconEx, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, LoadImageW, PeekMessageW, PostMessageW,
        PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow,
        TrackPopupMenu, UpdateLayeredWindow, DI_NORMAL, HCURSOR, HICON, HWND_TOPMOST, IMAGE_ICON,
        LR_LOADFROMFILE, LR_SHARED, MF_STRING, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_RESTORE,
        SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP, WM_COMMAND,
        WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
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

            let mut target_x = 0;

            // Priority 1: Anchor immediately to the left of TrafficMonitor if present
            if let Some(tm) = tm_hwnd {
                let mut tm_rect = RECT::default();
                if GetWindowRect(tm, &mut tm_rect).is_ok() && tm_rect.left > 100 {
                    target_x = tm_rect.left - band_w - 4;
                }
            }

            // Priority 2: If TrafficMonitor is not found, anchor to the left of TrayNotifyWnd
            if target_x == 0 && notify_found && notify_rect.left > 100 {
                target_x = notify_rect.left - band_w - 4;
            }

            // Priority 3: Fallback inside tray bounds
            if target_x == 0 {
                target_x = tray_rect.right - band_w - 300;
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
        let width = BAND_W.load(Ordering::Relaxed).max(80);
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
        pixels.fill(0); // Fully transparent background

        // 1. Draw Nebulae application icon on the left
        let icon_sz = layout.icon_size;
        let icon_x = layout.icon_x;
        let icon_y = layout.icon_y;

        if let Some(hicon) = load_nebulae_icon(icon_sz) {
            let _ = DrawIconEx(
                mem_dc, icon_x, icon_y, hicon, icon_sz, icon_sz, 0, None, DI_NORMAL,
            );
        } else {
            draw_fallback_n_icon(mem_dc, icon_x, icon_y, icon_sz);
        }

        // 2. Draw stacked transfer rates on the RIGHT side of the band.
        //
        // text_x = width - text_reserve_w (right-anchored, independent of
        // icon position).  format_stacked_line always emits exactly 12 chars
        // (indicator + 5-char-padded value + 4-char unit), so the text block
        // occupies a constant pixel width regardless of rate magnitude.
        let up = UP_RATE.load(Ordering::Relaxed);
        let down = DOWN_RATE.load(Ordering::Relaxed);

        // format_stacked_line produces exactly 12 Unicode characters per line.
        let up_line = crate::speed_format::format_stacked_line('\u{25b2}', up); // ▲
        let down_line = crate::speed_format::format_stacked_line('\u{25bc}', down); // ▼

        let font = CreateFontW(
            layout.font_px,
            0,
            0,
            0,
            600,
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

        // Upper line: Upload rate — full fixed-width string in ADC orange.
        let _ = SetTextColor(mem_dc, COLORREF(0x000F8BF7)); // Orange BGR (▲ + rate)
        let up_wide: Vec<u16> = up_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, layout.up_y, &up_wide);

        // Lower line: Download rate — full fixed-width string in cyan/green.
        let _ = SetTextColor(mem_dc, COLORREF(0x00D0B000)); // Cyan BGR (▼ + rate)
        let down_wide: Vec<u16> = down_line.encode_utf16().collect();
        let _ = TextOutW(mem_dc, text_x, layout.down_y, &down_wide);

        let _ = GdiFlush();

        // Alpha channel: Make glyphs and icon opaque (alpha = 255), transparent elsewhere
        for px in pixels.chunks_exact_mut(4) {
            let r = px[2] as u32;
            let g = px[1] as u32;
            let b = px[0] as u32;
            if r > 0 || g > 0 || b > 0 {
                px[3] = 255;
            } else {
                px[3] = 0;
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

    unsafe fn load_nebulae_icon(size: i32) -> Option<HICON> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(ico_path) = exe
                .parent()
                .map(|p| p.join("app.ico"))
                .filter(|p| p.exists())
            {
                let wide: Vec<u16> = ico_path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                if let Ok(handle) = LoadImageW(
                    None,
                    PCWSTR::from_raw(wide.as_ptr()),
                    IMAGE_ICON,
                    size,
                    size,
                    LR_LOADFROMFILE | LR_SHARED,
                ) {
                    if !handle.is_invalid() {
                        return Some(HICON(handle.0));
                    }
                }
            }
            let wide_exe: Vec<u16> = exe
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            if let Ok(handle) = LoadImageW(
                None,
                PCWSTR::from_raw(wide_exe.as_ptr()),
                IMAGE_ICON,
                size,
                size,
                LR_SHARED,
            ) {
                if !handle.is_invalid() {
                    return Some(HICON(handle.0));
                }
            }
        }
        None
    }

    unsafe fn draw_fallback_n_icon(hdc: HDC, x: i32, y: i32, size: i32) {
        // Dark rounded tile matching Nebulae icon style
        let bg_brush = CreateSolidBrush(COLORREF(0x00242220));
        let r = RECT {
            left: x,
            top: y,
            right: x + size,
            bottom: y + size,
        };
        FillRect(hdc, &r, bg_brush);
        let _ = DeleteObject(bg_brush.into());

        // White "N"
        let font_size = (size as f32 * 0.72).round() as i32;
        let n_font = CreateFontW(
            font_size,
            0,
            0,
            0,
            700,
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
        let old_f = SelectObject(hdc, n_font.into());
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let n_char = [b'N' as u16];
        let _ = TextOutW(hdc, x + (size / 5), y + (size / 12), &n_char);
        let _ = SelectObject(hdc, old_f);
        let _ = DeleteObject(n_font.into());

        // ADC orange dot in top-right
        let dot_brush = CreateSolidBrush(COLORREF(0x000F8BF7));
        let dot_size = (size / 4).max(3);
        let dot_rect = RECT {
            left: x + size - dot_size - 1,
            top: y + 1,
            right: x + size - 1,
            bottom: y + 1 + dot_size,
        };
        FillRect(hdc, &dot_rect, dot_brush);
        let _ = DeleteObject(dot_brush.into());
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
        // At 2× DPI every linear dimension must be exactly double.
        // This holds because all base constants in band_layout are rounded
        // from (base * dpi_scale) independently, and round(x*2) == 2*round(x)
        // for any x that is a multiple of 0.5.
        assert_eq!(l80.width, l40.width * 2, "width");
        assert_eq!(l80.icon_size, l40.icon_size * 2, "icon_size");
        assert_eq!(l80.icon_x, l40.icon_x * 2, "icon_x");
        assert_eq!(l80.text_x, l40.text_x * 2, "text_x");
        assert_eq!(l80.text_reserve_w, l40.text_reserve_w * 2, "text_reserve_w");
        assert!(l80.font_px > l40.font_px, "font_px must grow");
    }

    #[test]
    fn text_x_is_right_anchored() {
        for tray_h in [30, 40, 48, 60, 80, 96] {
            let l = band_layout(tray_h);
            // text_x is anchored from the right: text_x == width - text_reserve_w
            assert_eq!(
                l.text_x,
                l.width - l.text_reserve_w,
                "tray_h={}: text_x not right-anchored (got text_x={}, width={}, text_reserve_w={})",
                tray_h,
                l.text_x,
                l.width,
                l.text_reserve_w
            );
        }
    }

    #[test]
    fn speed_block_does_not_overlap_icon() {
        for tray_h in [30, 40, 48, 60, 80, 96] {
            let l = band_layout(tray_h);
            let icon_right = l.icon_x + l.icon_size;
            assert!(
                l.text_x > icon_right,
                "tray_h={}: text_x ({}) overlaps icon right edge ({})",
                tray_h,
                l.text_x,
                icon_right
            );
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

//! Windows Taskbar Speed Band Presentation.
//!
//! Presents Nebulae's live transfer rates immediately adjacent to its existing
//! application icon on the Windows taskbar toolbar (MSTaskListWClass):
//!                         ↑ 12.34 MB/s
//! [EXISTING NEBULAE ICON] ↓ 56.78 MB/s
//!
//! Requirements:
//! - Positioned right next to the Nebulae icon with 5px padding in between icon and numbers.
//! - Numbers fit within the icon height, forming one unified icon with numbers.
//! - Retains the original application icon at its normal size on the taskbar.
//! - Upload is the upper line; download is the lower line.
//! - No duplicate/miniature icon inside the band (existing application icon is the anchor).
//! - Both lines visible continuously while Nebulae is running, including when minimized.
//! - Preserves normal application activation (left-click focuses/restores) and context menu (right-click).
//! - Shows current Nebulae transfer rates only.

/// Pixel geometry of the speed chip, derived from the taskbar height so it
/// scales with DPI and taskbar size. Kept free of Win32 types so the sizing
/// rules are unit-tested on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct BandLayout {
    pub width: i32,
    pub height: i32,
    /// Em height of the rate text (CreateFontW takes it negated).
    pub font_px: i32,
    /// Edge of the direction triangles.
    pub arrow: i32,
    pub pad_x: i32,
}

/// `tray_h` is the taskbar thickness in pixels. The chip nearly fills it: the
/// two rate lines get about 20 px each on a 48 px taskbar, with enough vertical
/// headroom that the upload and download lines never overlap.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn band_layout(tray_h: i32) -> BandLayout {
    let tray_h = tray_h.max(30);
    let height = ((tray_h as f32) * 0.85).round() as i32;
    let line_h = height / 2;
    let font_px = ((line_h as f32) * 0.74).round() as i32;
    let arrow = ((line_h as f32) * 0.46).round() as i32;
    let pad_x = ((height as f32) * 0.20).round() as i32;
    // Worst-case field is "1023.99 KB/s": 12 glyphs at roughly 0.58 em, plus
    // the arrow column and padding. A fixed width keeps neighbours still.
    let text_w = ((font_px as f32) * 0.58 * 12.0).round() as i32;
    let width = pad_x + arrow + pad_x / 2 + text_w + pad_x;
    BandLayout {
        width,
        height,
        font_px,
        arrow,
        pad_x,
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Once;
    use std::time::Instant;
    use tauri::AppHandle;
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, GdiFlush, GetDC, Polygon, ReleaseDC,
        SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_ALPHA, AC_SRC_OVER,
        ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
        CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, FW_BOLD, HDC, OUT_DEFAULT_PRECIS,
        PS_SOLID, ROP_CODE, SRCCOPY, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, PeekMessageW, PostMessageW, PostQuitMessage,
        RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu,
        UpdateLayeredWindow, HWND_TOPMOST, MF_STRING, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_RESTORE,
        SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP, WM_COMMAND,
        WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_CLIPSIBLINGS, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    /// Posted by `update_taskbar_speed` so the window's own thread redraws.
    const WM_RENDER_BAND: u32 = WM_APP + 1;

    static UP_RATE: AtomicU64 = AtomicU64::new(0);
    static DOWN_RATE: AtomicU64 = AtomicU64::new(0);
    static BAND_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    static INIT_ONCE: Once = Once::new();

    const CLASS_NAME: PCWSTR = w!("NebulaeTaskbarSpeedBand");

    pub fn init_taskbar_speed(app: &AppHandle) {
        INIT_ONCE.call_once(|| {
            let app_clone = app.clone();
            std::thread::Builder::new()
                .name("nebulae-taskbar-speed".into())
                .spawn(move || {
                    run_taskbar_speed_window(app_clone);
                })
                .expect("spawn taskbar speed thread");
        });
    }

    pub fn update_taskbar_speed(app: &AppHandle, up: u64, down: u64) {
        init_taskbar_speed(app);
        UP_RATE.store(up, Ordering::Relaxed);
        DOWN_RATE.store(down, Ordering::Relaxed);

        let hwnd_raw = BAND_HWND.load(Ordering::Relaxed);
        if hwnd_raw != 0 {
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_RENDER_BAND, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn clear_taskbar_speed(app: &AppHandle) {
        update_taskbar_speed(app, 0, 0);
    }

    fn run_taskbar_speed_window(_app: AppHandle) {
        unsafe {
            let hinstance =
                windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(band_wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };

            let _ = RegisterClassExW(&wc);

            // Compute initial position: right next to Nebulae's taskbar icon (5px padding, matching icon height)
            let (init_x, init_y, init_w, init_h) = calculate_taskbar_position();

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_LAYERED,
                CLASS_NAME,
                w!("Nebulae Toolbar Speed"),
                WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS,
                init_x,
                init_y,
                init_w,
                init_h,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            );

            if let Ok(hwnd) = hwnd {
                // Per-pixel alpha (UpdateLayeredWindow): fully transparent
                // background, anti-aliased text, no colour-key fringes.
                render_band(hwnd);
                BAND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                let _ = ShowWindow(hwnd, SW_SHOWNA);

                // Dedicated Win32 message loop with 1-second position reconciliation
                let mut msg = MSG::default();
                let mut last_pos_check = Instant::now();

                loop {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                            return;
                        }
                        let _ = DispatchMessageW(&msg);
                    }

                    if last_pos_check.elapsed() >= std::time::Duration::from_millis(1000) {
                        last_pos_check = Instant::now();
                        reposition_band(hwnd);
                    }

                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }

    fn calculate_taskbar_position() -> (i32, i32, i32, i32) {
        unsafe {
            let h_tray = match FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => {
                    let l = super::band_layout(48);
                    return (100, 1000, l.width, l.height);
                }
            };

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);
            let dpi_scale = (tray_h as f32 / 40.0).max(1.0);

            let layout = super::band_layout(tray_h);
            let icon_h = layout.height;
            let icon_y = tray_rect.top + (tray_h - icon_h) / 2;
            let band_w = layout.width;

            // Look for Windows Taskbar Toolbar container (ReBarWindow32 -> MSTaskSwWClass -> MSTaskListWClass)
            let h_rebar =
                FindWindowExW(Some(h_tray), None, w!("ReBarWindow32"), PCWSTR::null()).ok();
            let h_tasksw = h_rebar.and_then(|rebar| {
                FindWindowExW(Some(rebar), None, w!("MSTaskSwWClass"), PCWSTR::null()).ok()
            });
            let h_tasklist = h_tasksw.and_then(|tasksw| {
                FindWindowExW(Some(tasksw), None, w!("MSTaskListWClass"), PCWSTR::null()).ok()
            });

            let mut task_rect = RECT::default();
            let (target_hwnd, base_x, capture_w, capture_h) = if let Some(tasklist) = h_tasklist {
                if GetWindowRect(tasklist, &mut task_rect).is_ok() {
                    (
                        tasklist,
                        task_rect.left,
                        (task_rect.right - task_rect.left).max(100),
                        (task_rect.bottom - task_rect.top).max(30),
                    )
                } else {
                    (
                        h_tray,
                        tray_rect.left,
                        tray_rect.right - tray_rect.left,
                        tray_h,
                    )
                }
            } else {
                (
                    h_tray,
                    tray_rect.left,
                    tray_rect.right - tray_rect.left,
                    tray_h,
                )
            };

            let mut target_x = 0;

            // Dynamically scan MSTaskListWClass for the Nebulae application icon
            let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(None);
            if !hdc_screen.is_invalid() {
                let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
                let hbm = CreateCompatibleBitmap(hdc_screen, capture_w, capture_h);
                let old_bm = SelectObject(hdc_mem, hbm.into());

                // Try PrintWindow with PW_RENDERFULLCONTENT (2)
                type PfnPrintWindow = unsafe extern "system" fn(HWND, HDC, u32) -> BOOL;
                let mut pw_captured = false;
                if let Ok(user32) =
                    windows::Win32::System::LibraryLoader::GetModuleHandleW(w!("user32.dll"))
                {
                    if let Some(pfn) = windows::Win32::System::LibraryLoader::GetProcAddress(
                        user32,
                        windows::core::s!("PrintWindow"),
                    ) {
                        let print_window: PfnPrintWindow = std::mem::transmute(pfn);
                        pw_captured = print_window(target_hwnd, hdc_mem, 2).as_bool();
                    }
                }

                if !pw_captured {
                    let _ = BitBlt(
                        hdc_mem,
                        0,
                        0,
                        capture_w,
                        capture_h,
                        Some(hdc_screen),
                        base_x,
                        tray_rect.top,
                        ROP_CODE(SRCCOPY.0 | 0x40000000),
                    );
                }

                // Retrieve 32-bit DIB pixel buffer for fast in-memory scanning
                let mut bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: capture_w,
                        biHeight: -capture_h, // top-down DIB
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let buf_size = (capture_w * capture_h * 4) as usize;
                let mut pixel_data = vec![0u8; buf_size];
                let lines = windows::Win32::Graphics::Gdi::GetDIBits(
                    hdc_mem,
                    hbm,
                    0,
                    capture_h as u32,
                    Some(pixel_data.as_mut_ptr() as *mut _),
                    &mut bmi,
                    DIB_RGB_COLORS,
                );

                if lines > 0 {
                    // Fast in-memory scan for Nebulae icon's ADC orange dot and white "N" pixels
                    let y_start = (capture_h / 6).max(0) as usize;
                    let y_end = (5 * capture_h / 6).min(capture_h) as usize;
                    let cap_w = capture_w as usize;

                    let mut orange_cols: Vec<i32> = Vec::new();
                    for x in 0..cap_w {
                        for y in y_start..y_end {
                            let idx = (y * cap_w + x) * 4;
                            let b = pixel_data[idx] as i32;
                            let g = pixel_data[idx + 1] as i32;
                            let r = pixel_data[idx + 2] as i32;

                            // ADC orange dot of Nebulae (r > 190, 80 <= g <= 170, b < 60)
                            if r > 190 && (80..=170).contains(&g) && b < 60 {
                                // Verify white "N" pixels in the icon
                                let max_d = (25.0 * dpi_scale).round() as i32;
                                let mut white_count = 0;
                                let cx_min = (x as i32 - max_d).max(0) as usize;
                                let cx_max = (x as i32 + max_d).min(capture_w - 1) as usize;

                                for cx in cx_min..=cx_max {
                                    for wy in y_start..y_end {
                                        let widx = (wy * cap_w + cx) * 4;
                                        let wb = pixel_data[widx] as i32;
                                        let wg = pixel_data[widx + 1] as i32;
                                        let wr = pixel_data[widx + 2] as i32;
                                        if wr > 200 && wg > 200 && wb > 200 {
                                            white_count += 1;
                                        }
                                    }
                                }

                                if white_count >= (10.0 * dpi_scale).round() as i32 {
                                    orange_cols.push(x as i32);
                                    break;
                                }
                            }
                        }
                    }

                    if !orange_cols.is_empty() {
                        let orange_center_x = orange_cols[orange_cols.len() / 2];

                        // Detect the exact right edge of the dark icon tile
                        let mut icon_right_local =
                            orange_center_x + (5.5 * dpi_scale).round() as i32;
                        let search_max = (orange_center_x + (20.0 * dpi_scale).round() as i32)
                            .min(capture_w - 1);
                        for x in orange_center_x..=search_max {
                            let mut dark_count = 0;
                            for y in y_start..y_end {
                                let widx = (y * cap_w + x as usize) * 4;
                                let wb = pixel_data[widx] as i32;
                                let wg = pixel_data[widx + 1] as i32;
                                let wr = pixel_data[widx + 2] as i32;
                                if (15..=60).contains(&wr)
                                    && (15..=60).contains(&wg)
                                    && (15..=60).contains(&wb)
                                {
                                    dark_count += 1;
                                }
                            }
                            if dark_count >= (8.0 * dpi_scale).round() as i32 {
                                icon_right_local = x;
                            }
                        }

                        // Exactly 5px visual padding between icon and numbers
                        // Accounting for font glyph bearing (2px at 200% DPI, 1px at 100% DPI)
                        let glyph_bearing = if dpi_scale > 1.5 { 2 } else { 1 };
                        let padding = (5 - glyph_bearing).max(1);
                        target_x = base_x + icon_right_local + padding;
                    }
                }

                let _ = SelectObject(hdc_mem, old_bm);
                let _ = DeleteObject(hbm.into());
                let _ = DeleteDC(hdc_mem);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc_screen);
            }

            // Fallback: slot index based on typical running task position if visual scan unavailable
            if target_x == 0 {
                let slot_w = (49.0 * dpi_scale).round() as i32;
                target_x = base_x + (10 * slot_w) + (5.0 * dpi_scale).round() as i32;
            }

            // Keep within taskbar bounds
            if target_x + band_w > tray_rect.right - 10 {
                target_x = tray_rect.right - band_w - 10;
            }
            if target_x < base_x {
                target_x = base_x;
            }

            (target_x, icon_y, band_w, icon_h)
        }
    }

    fn reposition_band(hwnd: HWND) {
        let (x, y, w, h) = calculate_taskbar_position();
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
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
                // Focus / restore Nebulae main application window
                activate_main_window();
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                show_context_menu(hwnd);
                LRESULT(0)
            }
            WM_COMMAND => {
                match wparam.0 as u16 {
                    1001 => {
                        // Open Nebulae
                        activate_main_window();
                    }
                    1002 => {
                        // Quit Nebulae
                        std::process::exit(0);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    /// Draws the two rate lines into a 32-bit DIB and presents it with per-pixel
    /// alpha, so the band has no background at all: only anti-aliased glyphs and
    /// the direction triangles are opaque. Everything drawn is a bright colour
    /// over black, so a pixel's alpha is its brightest channel and the colour is
    /// already premultiplied. A soft dark halo keeps the text readable on light
    /// taskbars. Background pixels get alpha 1 (invisible) so clicks still reach
    /// the band's handlers; fully transparent pixels would pass clicks through.
    unsafe fn render_band(hwnd: HWND) {
        let mut win = RECT::default();
        if GetWindowRect(hwnd, &mut win).is_err() {
            return;
        }
        let width = win.right - win.left;
        let height = win.bottom - win.top;
        if width <= 0 || height <= 0 {
            return;
        }

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let Ok(dib) = dib else {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, hdc_screen);
            return;
        };
        if bits.is_null() {
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, hdc_screen);
            return;
        }
        let old_bmp = SelectObject(mem_dc, dib.into());
        let len = (width * height * 4) as usize;
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, len);

        let layout = super::band_layout(height * 5 / 4);
        let up = UP_RATE.load(Ordering::Relaxed);
        let down = DOWN_RATE.load(Ordering::Relaxed);
        let lines: [Vec<u16>; 2] = [
            crate::speed_format::format_adaptive_rate(up)
                .encode_utf16()
                .collect(),
            crate::speed_format::format_adaptive_rate(down)
                .encode_utf16()
                .collect(),
        ];

        let font = CreateFontW(
            -layout.font_px,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            // Grey-scale anti-aliasing: ClearType needs an opaque backdrop.
            ANTIALIASED_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(mem_dc, font.into());
        let _ = SetBkMode(mem_dc, TRANSPARENT);

        let line_h = height / 2;
        let text_x = layout.pad_x + layout.arrow + layout.pad_x / 2;
        // Em height plus Segoe UI's line gap is about 1.33 em.
        let text_box = ((layout.font_px as f32) * 1.33).round() as i32;
        let text_dy = ((line_h - text_box) / 2).max(0);
        let half = layout.arrow / 2;
        let arrow_cx = layout.pad_x + half;

        // Pass 1: halo mask (white glyphs offset by one pixel).
        std::ptr::write_bytes(pixels.as_mut_ptr(), 0, len);
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        for (idx, text) in lines.iter().enumerate() {
            let y0 = idx as i32 * line_h;
            let _ = TextOutW(mem_dc, text_x + 1, y0 + text_dy + 1, text);
        }
        let _ = GdiFlush();
        let halo: Vec<u8> = pixels
            .chunks_exact(4)
            .map(|p| p[0].max(p[1]).max(p[2]))
            .collect();

        // Pass 2: foreground. Upload is the top line (amber up-triangle),
        // download the bottom line (green down-triangle). Triangles are drawn,
        // not font glyphs, so they do not depend on the font's arrow glyphs.
        std::ptr::write_bytes(pixels.as_mut_ptr(), 0, len);
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        for (idx, (text, color, up_arrow)) in [
            (&lines[0], COLORREF(0x0020B0FF), true),  // #FFB020
            (&lines[1], COLORREF(0x0084DC3D), false), // #3DDC84
        ]
        .into_iter()
        .enumerate()
        {
            let y0 = idx as i32 * line_h;
            let cy = y0 + line_h / 2;
            let tri = if up_arrow {
                [
                    POINT {
                        x: arrow_cx,
                        y: cy - half,
                    },
                    POINT {
                        x: arrow_cx - half,
                        y: cy + half,
                    },
                    POINT {
                        x: arrow_cx + half,
                        y: cy + half,
                    },
                ]
            } else {
                [
                    POINT {
                        x: arrow_cx - half,
                        y: cy - half,
                    },
                    POINT {
                        x: arrow_cx + half,
                        y: cy - half,
                    },
                    POINT {
                        x: arrow_cx,
                        y: cy + half,
                    },
                ]
            };
            let tri_brush = CreateSolidBrush(color);
            let tri_pen = CreatePen(PS_SOLID, 1, color);
            let ob = SelectObject(mem_dc, tri_brush.into());
            let op = SelectObject(mem_dc, tri_pen.into());
            let _ = Polygon(mem_dc, &tri);
            let _ = SelectObject(mem_dc, op);
            let _ = SelectObject(mem_dc, ob);
            let _ = DeleteObject(tri_pen.into());
            let _ = DeleteObject(tri_brush.into());

            let _ = TextOutW(mem_dc, text_x, y0 + text_dy, text);
        }
        let _ = GdiFlush();

        // Composite halo (black, 60 % strength) under the foreground and write
        // premultiplied BGRA.
        for (px, &h) in pixels.chunks_exact_mut(4).zip(halo.iter()) {
            let fg = px[0].max(px[1]).max(px[2]) as u32;
            let halo_a = (h as u32 * 153) / 255;
            let a = (fg + halo_a * (255 - fg) / 255).max(1);
            px[3] = a as u8;
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
                if title.contains("Nebulae") && !title.contains("Toolbar Speed") {
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
    fn text_is_clearly_larger_than_the_old_ten_pixel_band() {
        // Common taskbar thicknesses: Win10 40, Win11 48, 150% 72, 200% 96.
        for (tray_h, min_font) in [(40, 12), (48, 14), (72, 21), (96, 28)] {
            let l = band_layout(tray_h);
            assert!(l.font_px >= min_font, "tray {tray_h}: font {}", l.font_px);
        }
    }

    #[test]
    fn two_lines_always_fit_inside_the_chip_and_the_taskbar() {
        for tray_h in [30, 40, 48, 60, 72, 96, 144] {
            let l = band_layout(tray_h);
            assert!(l.height <= tray_h);
            // Em height plus ~33% line gap per line.
            assert!(((l.font_px as f32) * 1.34) <= (l.height / 2) as f32 + 1.0);
        }
    }

    #[test]
    fn width_is_fixed_per_taskbar_size_and_holds_the_widest_rate() {
        let l = band_layout(48);
        assert_eq!(l, band_layout(48));
        assert!(l.width >= l.pad_x + l.arrow + (l.font_px as f32 * 0.58 * 12.0) as i32);
    }
}

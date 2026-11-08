//! Windows Taskbar Speed Band Presentation.
//!
//! Presents Nebulae's live transfer rates immediately adjacent to its "N" application icon
//! on the bottom Windows toolbar next to TrafficMonitor/notification area:
//!                 ↑ 1.25 MB/s
//! [Nebulae Icon]  ↓ 8.40 MB/s
//!
//! Requirements:
//! - Positioned on the Windows toolbar (ReBarWindow32), adjacent to TrafficMonitor / system tray.
//! - Nebulae icon on the left; readable rate text directly beside it on the right.
//! - Upload on the upper line; download on the lower line.
//! - Both lines visible continuously while Nebulae is running, including when minimized.
//! - Preserves normal application activation (left-click focuses/restores) and context menu (right-click).
//! - Shows current Nebulae transfer rates only.

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
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
        CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, InvalidateRect, SelectObject,
        SetBkMode, SetTextColor, TextOutW, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
        FW_SEMIBOLD, HDC, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, DrawIconEx, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, LoadImageW, PeekMessageW, PostQuitMessage,
        RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu, DI_NORMAL,
        HICON, HWND_TOPMOST, IMAGE_ICON, LR_LOADFROMFILE, LR_SHARED, MF_STRING, MSG, PM_REMOVE,
        SWP_NOACTIVATE, SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
        WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WNDCLASSEXW, WS_CLIPSIBLINGS,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

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
                let _ = InvalidateRect(Some(hwnd), None, false);
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

            // Compute initial position on the toolbar (adjacent to TrafficMonitor / system tray)
            let (init_x, init_y, init_w, init_h) = calculate_taskbar_position();

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
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
                Err(_) => return (100, 1000, 130, 40),
            };

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);
            let dpi_scale = (tray_h as f32 / 40.0).max(1.0);
            let band_w = (130.0 * dpi_scale).round() as i32;

            // Look for Windows Taskbar Toolbar container (ReBarWindow32)
            let h_rebar =
                FindWindowExW(Some(h_tray), None, w!("ReBarWindow32"), PCWSTR::null()).ok();

            // 1. Look for TrafficMonitor on the toolbar
            let mut tm_hwnd = None;
            if let Some(rebar) = h_rebar {
                if let Ok(h) = FindWindowExW(
                    Some(rebar),
                    None,
                    w!("#32770"),
                    w!("TrafficMonitorTaskbarWindow"),
                ) {
                    tm_hwnd = Some(h);
                }
            }
            if tm_hwnd.is_none() {
                if let Ok(h) = FindWindowExW(
                    Some(h_tray),
                    None,
                    w!("#32770"),
                    w!("TrafficMonitorTaskbarWindow"),
                ) {
                    tm_hwnd = Some(h);
                }
            }
            // Fallback: any #32770 window inside ReBarWindow32
            if tm_hwnd.is_none() {
                if let Some(rebar) = h_rebar {
                    if let Ok(h) = FindWindowExW(Some(rebar), None, w!("#32770"), PCWSTR::null()) {
                        tm_hwnd = Some(h);
                    }
                }
            }

            // 2. Look for TrayNotifyWnd (notification area / system tray on right)
            let h_notify =
                FindWindowExW(Some(h_tray), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok();
            let mut notify_rect = RECT::default();
            let notify_found = if let Some(notify) = h_notify {
                GetWindowRect(notify, &mut notify_rect).is_ok()
            } else {
                false
            };

            // 3. Look for MSTaskListWClass (running application buttons)
            let h_tasksw = h_rebar.and_then(|rebar| {
                FindWindowExW(Some(rebar), None, w!("MSTaskSwWClass"), PCWSTR::null()).ok()
            });
            let h_tasklist = h_tasksw.and_then(|tasksw| {
                FindWindowExW(Some(tasksw), None, w!("MSTaskListWClass"), PCWSTR::null()).ok()
            });
            let mut task_rect = RECT::default();
            let task_found = if let Some(tasklist) = h_tasklist {
                GetWindowRect(tasklist, &mut task_rect).is_ok()
            } else {
                false
            };

            let mut target_x = 0;

            // Position on the toolbar immediately to the left of TrafficMonitor
            if let Some(tm) = tm_hwnd {
                let mut tm_rect = RECT::default();
                if GetWindowRect(tm, &mut tm_rect).is_ok() && tm_rect.left > 100 {
                    target_x = tm_rect.left - band_w - 4;
                }
            }

            // If TrafficMonitor not present, position immediately to the left of system tray
            if target_x == 0 && notify_found {
                target_x = notify_rect.left - band_w - 6;
            }

            // Fallback if neither found
            if target_x == 0 {
                target_x = tray_rect.right - band_w - (300.0 * dpi_scale).round() as i32;
            }

            // Ensure band does not overlap MSTaskListWClass
            if task_found && target_x < task_rect.right + 4 {
                target_x = task_rect.right + 4;
            }

            // Ensure band stays bounded within the taskbar tray
            if notify_found && target_x + band_w > notify_rect.left - 4 {
                target_x = notify_rect.left - band_w - 4;
            }

            (target_x, tray_rect.top, band_w, tray_h)
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
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint_band(hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
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

    unsafe fn paint_band(hwnd: HWND, hdc: HDC) {
        let mut rect = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        if width <= 0 || height <= 0 {
            return;
        }

        let mem_dc = CreateCompatibleDC(Some(hdc));
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp.into());

        // Fill background with taskbar-matching dark color
        let bg_brush = CreateSolidBrush(COLORREF(0x00101010));
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush.into());

        let dpi_scale = (height as f32 / 40.0).max(1.0);

        // 1. Draw Nebulae application icon on the toolbar (left side of band)
        let icon_size = (22.0 * dpi_scale).round() as i32;
        let icon_x = (8.0 * dpi_scale).round() as i32;
        let icon_y = (height - icon_size) / 2;

        let icon_h = load_nebulae_icon(icon_size);
        if let Some(hicon) = icon_h {
            let _ = DrawIconEx(
                mem_dc, icon_x, icon_y, hicon, icon_size, icon_size, 0, None, DI_NORMAL,
            );
        } else {
            draw_fallback_n_icon(mem_dc, icon_x, icon_y, icon_size);
        }

        // 2. Draw Stacked Transfer Rates immediately adjacent to the toolbar icon
        let up = UP_RATE.load(Ordering::Relaxed);
        let down = DOWN_RATE.load(Ordering::Relaxed);

        let up_str = format!("↑ {}", crate::speed_format::format_adaptive_rate(up));
        let down_str = format!("↓ {}", crate::speed_format::format_adaptive_rate(down));

        let font_h = (12.0 * dpi_scale).round() as i32;
        let font = CreateFontW(
            font_h,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
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
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFFFFF)); // Crisp white

        let up_wide: Vec<u16> = up_str.encode_utf16().collect();
        let down_wide: Vec<u16> = down_str.encode_utf16().collect();

        // Top line: Upload rate (↑)
        // Bottom line: Download rate (↓)
        let text_x = icon_x + icon_size + (8.0 * dpi_scale).round() as i32;
        let up_y = (height / 2) - (14.0 * dpi_scale).round() as i32;
        let down_y = (height / 2) + (1.0 * dpi_scale).round() as i32;

        let _ = TextOutW(mem_dc, text_x, up_y, &up_wide);
        let _ = TextOutW(mem_dc, text_x, down_y, &down_wide);

        // Blit to screen
        let _ = BitBlt(hdc, 0, 0, width, height, Some(mem_dc), 0, 0, SRCCOPY);

        // Cleanup GDI objects
        let _ = SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font.into());
        let _ = SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp.into());
        let _ = DeleteDC(mem_dc);
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
            // Extract embedded icon from executable
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
        let font_size = (size as f32 * 0.75).round() as i32;
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
            w!("Arial Black"),
        );
        let old_f = SelectObject(hdc, n_font.into());
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let n_char = [b'N' as u16];
        let _ = TextOutW(hdc, x + (size / 6), y + (size / 10), &n_char);
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

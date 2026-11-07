//! Windows Taskbar Speed Band Presentation.
//!
//! Presents Nebulae's live transfer rates immediately adjacent to its "N" application icon
//! on the bottom Windows toolbar/taskbar, forming one visually associated group:
//!             ↑ 1.25 MB/s
//! [Nebulae N] ↓ 8.40 MB/s
//!
//! Requirements:
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
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Once;
    use std::time::Instant;
    use tauri::{AppHandle, Manager};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
        CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, SelectObject, SetBkMode,
        SetTextColor, TextOutW, CLEARTYPE_QUALITY, DEFAULT_CHARSET, FW_SEMIBOLD, HDC, HFONT,
        OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, DrawIconEx, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        InvalidateRect, LoadImageW, PeekMessageW, PostQuitMessage, RegisterClassExW,
        SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu, DI_NORMAL, HICON,
        HWND_TOPMOST, IMAGE_ICON, LR_LOADFROMFILE, LR_SHARED, MF_STRING, MSG, PM_REMOVE,
        SWP_NOACTIVATE, SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
        WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WNDCLASSEXW, WS_CLIPSIBLINGS,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    static UP_RATE: AtomicU64 = AtomicU64::new(0);
    static DOWN_RATE: AtomicU64 = AtomicU64::new(0);
    static BAND_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    static INIT_ONCE: Once = Once::new();

    const BAND_WIDTH: i32 = 126;
    const BAND_HEIGHT: i32 = 40;
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
            let hwnd = HWND(hwnd_raw as _);
            unsafe {
                let _ = InvalidateRect(hwnd, None, false);
            }
        }
    }

    pub fn clear_taskbar_speed(app: &AppHandle) {
        update_taskbar_speed(app, 0, 0);
    }

    fn run_taskbar_speed_window(app: AppHandle) {
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

            // Compute initial position based on Shell_TrayWnd
            let (init_x, init_y, init_h) = calculate_taskbar_position();

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("Nebulae Toolbar Speed"),
                WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS,
                init_x,
                init_y,
                BAND_WIDTH,
                init_h,
                None,
                None,
                hinstance,
                None,
            );

            if let Ok(hwnd) = hwnd {
                BAND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                let _ = ShowWindow(hwnd, SW_SHOWNA);

                // Run dedicated Win32 message loop with 1-second position reconciliation
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

    fn calculate_taskbar_position() -> (i32, i32, i32) {
        unsafe {
            let h_tray = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null());
            if h_tray.0 == 0 {
                return (100, 1000, BAND_HEIGHT);
            }

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);

            // Look for TaskList (MSTaskListWClass) inside ReBarWindow32
            let h_rebar = FindWindowExW(h_tray, None, w!("ReBarWindow32"), PCWSTR::null());
            let h_tasksw = if h_rebar.0 != 0 {
                FindWindowExW(h_rebar, None, w!("MSTaskSwWClass"), PCWSTR::null())
            } else {
                HWND::default()
            };
            let h_tasklist = if h_tasksw.0 != 0 {
                FindWindowExW(h_tasksw, None, w!("MSTaskListWClass"), PCWSTR::null())
            } else {
                HWND::default()
            };

            // Look for TrayNotifyWnd (notification area / system tray on right)
            let h_notify = FindWindowExW(h_tray, None, w!("TrayNotifyWnd"), PCWSTR::null());
            let mut notify_rect = RECT::default();
            let notify_found = if h_notify.0 != 0 {
                GetWindowRect(h_notify, &mut notify_rect).is_ok()
            } else {
                false
            };

            let mut target_x = 0;
            if h_tasklist.0 != 0 {
                let mut task_rect = RECT::default();
                if GetWindowRect(h_tasklist, &mut task_rect).is_ok() {
                    // Position immediately to the right of the task buttons
                    target_x = task_rect.right + 6;
                }
            } else if h_rebar.0 != 0 {
                let mut rebar_rect = RECT::default();
                if GetWindowRect(h_rebar, &mut rebar_rect).is_ok() {
                    target_x = rebar_rect.right + 6;
                }
            }

            if target_x == 0 {
                if notify_found {
                    target_x = notify_rect.left - BAND_WIDTH - 8;
                } else {
                    target_x = tray_rect.right - BAND_WIDTH - 200;
                }
            }

            // Ensure band stays bounded within the taskbar
            if notify_found && target_x + BAND_WIDTH > notify_rect.left - 4 {
                target_x = notify_rect.left - BAND_WIDTH - 4;
            }
            if target_x < tray_rect.left + 50 {
                target_x = tray_rect.left + 50;
            }

            (target_x, tray_rect.top, tray_h)
        }
    }

    fn reposition_band(hwnd: HWND) {
        let (x, y, h) = calculate_taskbar_position();
        unsafe {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, BAND_WIDTH, h, SWP_NOACTIVATE);
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

        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        // Fill background with taskbar-matching dark color
        let bg_brush = CreateSolidBrush(COLORREF(0x00101010));
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // 1. Draw Nebulae "N" icon on left
        let icon_h = load_nebulae_icon();
        let icon_y = (height - 20) / 2;
        if let Some(hicon) = icon_h {
            let _ = DrawIconEx(mem_dc, 6, icon_y, hicon, 20, 20, 0, None, DI_NORMAL);
        } else {
            // Fallback: draw stylish "N" brand emblem badge
            draw_fallback_n_icon(mem_dc, 6, icon_y, 20);
        }

        // 2. Draw Stacked Transfer Rates on right
        let up = UP_RATE.load(Ordering::Relaxed);
        let down = DOWN_RATE.load(Ordering::Relaxed);

        let up_str = format!("↑ {}", crate::speed_format::format_adaptive_rate(up));
        let down_str = format!("↓ {}", crate::speed_format::format_adaptive_rate(down));

        let font = CreateFontW(
            13,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            0,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(mem_dc, font);

        let _ = SetBkMode(mem_dc, TRANSPARENT);
        let _ = SetTextColor(mem_dc, COLORREF(0x00FFFFFF)); // Crisp white

        let up_wide: Vec<u16> = up_str.encode_utf16().collect();
        let down_wide: Vec<u16> = down_str.encode_utf16().collect();

        // Top line: Upload rate
        let text_x = 32;
        let up_y = (height / 2) - 15;
        let down_y = (height / 2) + 1;

        let _ = TextOutW(mem_dc, text_x, up_y, &up_wide);
        let _ = TextOutW(mem_dc, text_x, down_y, &down_wide);

        // Blit to screen
        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        // Cleanup GDI objects
        let _ = SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font);
        let _ = SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp);
        let _ = DeleteDC(mem_dc);
    }

    unsafe fn load_nebulae_icon() -> Option<HICON> {
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
                    20,
                    20,
                    LR_LOADFROMFILE | LR_SHARED,
                ) {
                    if handle.0 != 0 {
                        return Some(HICON(handle.0));
                    }
                }
            }
            // Fall back to extracting icon 0 from executable
            let wide_exe: Vec<u16> = exe
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            if let Ok(handle) = LoadImageW(
                None,
                PCWSTR::from_raw(wide_exe.as_ptr()),
                IMAGE_ICON,
                20,
                20,
                LR_SHARED,
            ) {
                if handle.0 != 0 {
                    return Some(HICON(handle.0));
                }
            }
        }
        None
    }

    unsafe fn draw_fallback_n_icon(hdc: HDC, x: i32, y: i32, size: i32) {
        let badge_brush = CreateSolidBrush(COLORREF(0x001A18E5)); // ADC Crimson Red
        let r = RECT {
            left: x,
            top: y,
            right: x + size,
            bottom: y + size,
        };
        FillRect(hdc, &r, badge_brush);
        let _ = DeleteObject(badge_brush);

        let n_font = CreateFontW(
            14,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            0,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            w!("Arial Black"),
        );
        let old_f = SelectObject(hdc, n_font);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let n_char = [b'N' as u16];
        let _ = TextOutW(hdc, x + 4, y + 2, &n_char);
        let _ = SelectObject(hdc, old_f);
        let _ = DeleteObject(n_font);
    }

    fn activate_main_window() {
        unsafe {
            // Find main Nebulae window by AppUserModelID or title
            let h_neb = FindWindowW(None, w!("Nebulae"));
            if h_neb.0 != 0 {
                let _ = ShowWindow(h_neb, SW_RESTORE);
                let _ = SetForegroundWindow(h_neb);
            }
        }
    }

    unsafe fn show_context_menu(hwnd: HWND) {
        let hmenu = CreatePopupMenu().unwrap_or_default();
        if hmenu.0 == 0 {
            return;
        }

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
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
    }
}

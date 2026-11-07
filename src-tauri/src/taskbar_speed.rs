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
        FW_SEMIBOLD, HDC, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ROP_CODE, SRCCOPY, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        DispatchMessageW, FindWindowExW, FindWindowW, GetCursorPos, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, PeekMessageW, PostQuitMessage, RegisterClassExW,
        SetForegroundWindow, SetWindowPos, ShowWindow, TrackPopupMenu, HWND_TOPMOST, MF_STRING,
        MSG, PM_REMOVE, SWP_NOACTIVATE, SW_RESTORE, SW_SHOWNA, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WNDCLASSEXW,
        WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    static UP_RATE: AtomicU64 = AtomicU64::new(0);
    static DOWN_RATE: AtomicU64 = AtomicU64::new(0);
    static BAND_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    static INIT_ONCE: Once = Once::new();

    const BAND_WIDTH: i32 = 80;
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
                Some(HINSTANCE(hinstance.0)),
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

    fn count_taskbar_buttons() -> i32 {
        unsafe {
            struct EnumState {
                count: i32,
            }
            let mut state = EnumState { count: 0 };
            unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindow, GetWindowLongW, GetWindowTextLengthW, IsWindowVisible, GWL_EXSTYLE,
                    GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
                };
                if !IsWindowVisible(hwnd).as_bool() {
                    return BOOL(1);
                }
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                if (ex & WS_EX_TOOLWINDOW.0) != 0 {
                    return BOOL(1);
                }
                let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
                if !owner.0.is_null() && (ex & WS_EX_APPWINDOW.0) == 0 {
                    return BOOL(1);
                }
                if GetWindowTextLengthW(hwnd) > 0 {
                    let s = &mut *(lparam.0 as *mut EnumState);
                    s.count += 1;
                }
                BOOL(1)
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::EnumWindows(
                Some(enum_proc),
                LPARAM(&mut state as *mut _ as isize),
            );
            state.count.max(1)
        }
    }

    fn calculate_taskbar_position() -> (i32, i32, i32) {
        unsafe {
            let h_tray = match FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => return (100, 1000, BAND_HEIGHT),
            };

            let mut tray_rect = RECT::default();
            let _ = GetWindowRect(h_tray, &mut tray_rect);
            let tray_h = (tray_rect.bottom - tray_rect.top).max(30);

            // Look for TaskList (MSTaskListWClass) inside ReBarWindow32
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
                        (tray_rect.right - tray_rect.left).max(100),
                        tray_h,
                    )
                }
            } else {
                (
                    h_tray,
                    tray_rect.left,
                    (tray_rect.right - tray_rect.left).max(100),
                    tray_h,
                )
            };

            // Look for TrayNotifyWnd (notification area / system tray on right)
            let h_notify =
                FindWindowExW(Some(h_tray), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok();
            let mut notify_rect = RECT::default();
            let notify_found = if let Some(notify) = h_notify {
                GetWindowRect(notify, &mut notify_rect).is_ok()
            } else {
                false
            };

            let mut target_x = 0;
            const SLOT_W: i32 = 46;

            // Dynamically scan toolbar for the Nebulae application icon
            let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(None);
            if !hdc_screen.is_invalid() {
                let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
                let hbm = CreateCompatibleBitmap(hdc_screen, capture_w, capture_h);
                let old_bm = SelectObject(hdc_mem, hbm.into());

                // Use PrintWindow on MSTaskListWClass (PW_RENDERFULLCONTENT = 2)
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

                // Search for the Nebulae application icon across the toolbar
                // Nebulae icon has:
                // 1. Dark background
                // 2. White "N" characters (r > 200, g > 200, b > 200)
                // 3. Orange dot in top-right (r > 185, 70 <= g <= 180, b < 65)
                let max_scan_x = (capture_w - 10).max(50);
                let mut best_icon_x = -1;
                let mut best_score = 0;

                for x in 10..max_scan_x {
                    let mut orange_count = 0;
                    let mut white_count = 0;
                    for y in (capture_h / 5)..(4 * capture_h / 5) {
                        let pixel = windows::Win32::Graphics::Gdi::GetPixel(hdc_mem, x, y);
                        let r = (pixel.0 & 0xFF) as i32;
                        let g = ((pixel.0 >> 8) & 0xFF) as i32;
                        let b = ((pixel.0 >> 16) & 0xFF) as i32;

                        if r > 185 && g >= 70 && g <= 180 && b < 65 {
                            orange_count += 1;
                        }
                        if r > 200 && g > 200 && b > 200 {
                            white_count += 1;
                        }
                    }

                    if orange_count >= 1 && white_count >= 2 {
                        let score = orange_count * 4 + white_count;
                        if score > best_score {
                            best_score = score;
                            best_icon_x = x;
                        }
                    }
                }

                if best_icon_x > 0 {
                    // Found Nebulae icon! Determine the button slot right edge
                    let slot_idx = best_icon_x / SLOT_W;
                    let slot_right = (slot_idx + 1) * SLOT_W;
                    target_x = base_x + slot_right + 2;
                }

                let _ = SelectObject(hdc_mem, old_bm);
                let _ = DeleteObject(hbm.into());
                let _ = DeleteDC(hdc_mem);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc_screen);
            }

            if target_x == 0 {
                // Heuristic fallback if visual scan was occluded:
                // Count taskbar buttons to estimate button slot
                let btn_count = count_taskbar_buttons();
                target_x = base_x + (btn_count * SLOT_W) + 2;
            }

            if target_x == 0 {
                target_x = 998; // Default baseline right next to toolbar Nebulae icon
            }

            // Ensure band does not spill into TrafficMonitor (starts at 1224) or beyond taskbar bounds
            if target_x + BAND_WIDTH > 1220 {
                target_x = 1220 - BAND_WIDTH;
            }
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
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                BAND_WIDTH,
                h,
                SWP_NOACTIVATE,
            );
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

        let mem_dc = CreateCompatibleDC(Some(hdc));
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp.into());

        // Fill background with taskbar-matching dark color
        let bg_brush = CreateSolidBrush(COLORREF(0x00101010));
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush.into());

        // Draw Stacked Transfer Rates immediately adjacent to existing taskbar icon (no miniature icon)
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
        let text_x = 4;
        let up_y = (height / 2) - 15;
        let down_y = (height / 2) + 1;

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

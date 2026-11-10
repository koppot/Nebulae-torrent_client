//! Windows Taskbar Speed Integration for Nebulae 0.2.7.
//!
//! Presents Nebulae's live transfer rates natively anchored to the application's
//! taskbar button without unanchored floating windows or screen pixel scanning.
//!
//! 1. Dynamic Window Title Synchronization (SetWindowTextW):
//!    In "Never combine" / "Show labels" taskbar mode (supported natively on
//!    Windows 10 and Windows 11 23H2+), Explorer renders the formatted rate string
//!    directly beside the icon within the native taskbar button:
//!    [ ICON ]  Nebulae  [ ↑  12.34 MB/s   ↓  56.78 MB/s ]
//!    - 100% unified with the taskbar button.
//!    - Zero overlap with neighboring buttons (Explorer allocates native button width).
//!    - Fully tracks taskbar rearrangement, multi-monitors, and auto-hide.
//!    - Fixed-width padding so changes in rate and unit do not shift layout.
//!
//! 2. Native Taskbar Progress State (ITaskbarList3):
//!    In combined mode (where Explorer hides labels and shows square icons),
//!    drives the native Windows 7+ ITaskbarList3 progress bar directly on
//!    the taskbar button:
//!    - Active downloading: TBPF_NORMAL (green progress fill).
//!    - Active seeding: TBPF_INDETERMINATE.
//!    - Idle: TBPF_NOPROGRESS.

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
    use tauri::{AppHandle, Manager};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        ITaskbarList3, TaskbarList, TBPF_INDETERMINATE, TBPF_NOPROGRESS, TBPF_NORMAL,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    static LAST_UP: AtomicU64 = AtomicU64::new(u64::MAX);
    static LAST_DOWN: AtomicU64 = AtomicU64::new(u64::MAX);

    pub fn init_taskbar_speed(_app: &AppHandle) {}

    pub fn update_taskbar_speed(app: &AppHandle, up: u64, down: u64) {
        let prev_up = LAST_UP.swap(up, Ordering::Relaxed);
        let prev_down = LAST_DOWN.swap(down, Ordering::Relaxed);

        // Throttle updates if rates have not meaningfully changed
        if prev_up == up && prev_down == down {
            return;
        }

        if let Some(win) = app.get_webview_window("main") {
            let title = crate::speed_format::format_taskbar_title(up, down);
            let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

            if let Ok(parent) = win.hwnd() {
                let hwnd = HWND(parent.0);
                unsafe {
                    let _ = SetWindowTextW(hwnd, PCWSTR::from_raw(wide_title.as_ptr()));

                    // Drive native Windows Taskbar Progress indicator for combined taskbar mode
                    if let Ok(taskbar) = CoCreateInstance::<_, ITaskbarList3>(
                        &TaskbarList,
                        None,
                        CLSCTX_INPROC_SERVER,
                    ) {
                        let _ = taskbar.HrInit();
                        if down > 0 {
                            let _ = taskbar.SetProgressState(hwnd, TBPF_NORMAL);
                        } else if up > 0 {
                            let _ = taskbar.SetProgressState(hwnd, TBPF_INDETERMINATE);
                        } else {
                            let _ = taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS);
                        }
                    }
                }
            }
        }
    }

    pub fn clear_taskbar_speed(app: &AppHandle) {
        LAST_UP.store(u64::MAX, Ordering::Relaxed);
        LAST_DOWN.store(u64::MAX, Ordering::Relaxed);
        if let Some(win) = app.get_webview_window("main") {
            let title = "Nebulae";
            let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            if let Ok(parent) = win.hwnd() {
                let hwnd = HWND(parent.0);
                unsafe {
                    let _ = SetWindowTextW(hwnd, PCWSTR::from_raw(wide_title.as_ptr()));
                    if let Ok(taskbar) = CoCreateInstance::<_, ITaskbarList3>(
                        &TaskbarList,
                        None,
                        CLSCTX_INPROC_SERVER,
                    ) {
                        let _ = taskbar.HrInit();
                        let _ = taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::speed_format::format_taskbar_title;

    #[test]
    fn test_taskbar_title_stable_width() {
        let t_idle = format_taskbar_title(0, 0);
        assert_eq!(t_idle, "Nebulae");

        let t_active = format_taskbar_title(1024 * 1024 * 5, 1024 * 1024 * 25);
        assert!(t_active.contains("Nebulae  [ ↑"));
        assert!(t_active.contains("MB/s"));
        assert_eq!(t_active.chars().count(), 38);
    }
}

use crate::engine::SharedEngine;
use std::time::Instant;
use tauri::AppHandle;

/// Global state for status bar management
#[allow(dead_code)]
pub(crate) struct StatusBarState {
    last_update: Instant,
    tray_id: Option<String>,
}

impl StatusBarState {
    fn new() -> Self {
        Self {
            last_update: Instant::now(),
            tray_id: None,
        }
    }
}

/// Thread-safe wrapper for status bar state
pub(crate) static STATUS_BAR_STATE: std::sync::OnceLock<
    std::sync::Arc<std::sync::Mutex<StatusBarState>>,
> = std::sync::OnceLock::new();

/// Initialize the status bar tracking state
pub fn init_status_bar(_app_handle: &AppHandle) {
    STATUS_BAR_STATE
        .get_or_init(|| std::sync::Arc::new(std::sync::Mutex::new(StatusBarState::new())));
}

/// Set the tray ID from the actual tray icon
pub fn set_tray_id(tray_id: String) {
    if let Some(s) = STATUS_BAR_STATE.get() {
        let mut s = s.lock().unwrap();
        s.tray_id = Some(tray_id);
    }
}

/// Update tray icon with current rates (bytes per second)
/// Uses two-line stacked composite NSImage on macOS: upload on top, download on bottom.
/// Dispatches UI mutations to the main thread via run_on_main_thread to prevent any
/// interference with window initialization or the main runloop.
pub fn update_status_bar(app_handle: &AppHandle, upload_rate: u64, download_rate: u64) {
    #[cfg(target_os = "macos")]
    {
        let tray_id_opt = {
            let s = STATUS_BAR_STATE.get();
            s.and_then(|s| s.lock().ok())
                .and_then(|s| s.tray_id.clone())
        };

        let tray_id = match tray_id_opt {
            Some(id) => id,
            None => return,
        };

        // Throttle updates: no more than once per second (>= 950 ms)
        let state = STATUS_BAR_STATE.get();
        let now = Instant::now();
        let should_update = if let Some(s) = state {
            match s.lock() {
                Ok(mut s) => {
                    if now.duration_since(s.last_update) >= std::time::Duration::from_millis(950) {
                        s.last_update = now;
                        true
                    } else {
                        false
                    }
                }
                Err(_) => false,
            }
        } else {
            true
        };

        if !should_update {
            return;
        }

        let up_line = crate::speed_format::format_stacked_line('▲', upload_rate);
        let down_line = crate::speed_format::format_stacked_line('▼', download_rate);
        let tooltip = format!(
            "Nebulae — ▲ {}   ▼ {}",
            up_line.trim_start_matches('▲').trim(),
            down_line.trim_start_matches('▼').trim()
        );

        let handle = app_handle.clone();
        let tray_handle = app_handle.clone();
        let _ = handle.run_on_main_thread(move || {
            apply_status_bar_composite_image(&up_line, &down_line);
            if let Some(tray) = tray_handle.tray_by_id(&tray_id) {
                let _ = tray.set_tooltip(Some(&tooltip));
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
        let _upload_rate = upload_rate;
        let _download_rate = download_rate;
    }
}

/// Clear the status bar - restores default brand icon on idle or engine stop
pub fn clear_status_bar(app_handle: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let handle = app_handle.clone();
        let _ = handle.run_on_main_thread(move || {
            reset_status_bar_brand_icon();
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
    }
}

#[cfg(target_os = "macos")]
unsafe fn find_status_bar_button(
    view: *mut objc2::runtime::AnyObject,
    status_btn_class: &objc2::runtime::AnyClass,
) -> Option<*mut objc2::runtime::AnyObject> {
    if view.is_null() {
        return None;
    }
    let is_btn: bool = objc2::msg_send![view, isKindOfClass: status_btn_class];
    if is_btn {
        return Some(view);
    }
    let subviews: *mut objc2::runtime::AnyObject = objc2::msg_send![view, subviews];
    if subviews.is_null() {
        return None;
    }
    let count: usize = objc2::msg_send![subviews, count];
    for i in 0..count {
        let sub: *mut objc2::runtime::AnyObject = objc2::msg_send![subviews, objectAtIndex: i];
        if let Some(btn) = find_status_bar_button(sub, status_btn_class) {
            return Some(btn);
        }
    }
    None
}

#[cfg(target_os = "macos")]
unsafe fn get_status_bar_button() -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::runtime::{AnyClass, AnyObject};
    let app_class = AnyClass::get(c"NSApplication")?;
    let ns_app: *mut AnyObject = objc2::msg_send![app_class, sharedApplication];
    if ns_app.is_null() {
        return None;
    }
    let windows: *mut AnyObject = objc2::msg_send![ns_app, windows];
    if windows.is_null() {
        return None;
    }
    let count: usize = objc2::msg_send![windows, count];
    let status_win_class = AnyClass::get(c"NSStatusBarWindow")?;
    let status_btn_class = AnyClass::get(c"NSStatusBarButton")?;

    for i in 0..count {
        let win: *mut AnyObject = objc2::msg_send![windows, objectAtIndex: i];
        if win.is_null() {
            continue;
        }
        let is_status_window: bool = objc2::msg_send![win, isKindOfClass: status_win_class];
        if is_status_window {
            let view: *mut AnyObject = objc2::msg_send![win, contentView];
            if !view.is_null() {
                if let Some(btn) = find_status_bar_button(view, status_btn_class) {
                    return Some(btn);
                }
            }
        }
    }
    None
}

/// Render a composite NSImage containing the brand icon and two stacked rate lines,
/// and apply it to the NSStatusBarButton with zero clipping and native San Francisco typography.
#[cfg(target_os = "macos")]
fn apply_status_bar_composite_image(up_line: &str, down_line: &str) {
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::CString;

    unsafe {
        let button = match get_status_bar_button() {
            Some(btn) => btn,
            None => return,
        };

        let img_class = match AnyClass::get(c"NSImage") {
            Some(cls) => cls,
            None => return,
        };
        let data_class = match AnyClass::get(c"NSData") {
            Some(cls) => cls,
            None => return,
        };
        let str_class = match AnyClass::get(c"NSString") {
            Some(cls) => cls,
            None => return,
        };
        let font_class = match AnyClass::get(c"NSFont") {
            Some(cls) => cls,
            None => return,
        };
        let color_class = match AnyClass::get(c"NSColor") {
            Some(cls) => cls,
            None => return,
        };
        let style_class = match AnyClass::get(c"NSMutableParagraphStyle") {
            Some(cls) => cls,
            None => return,
        };
        let attr_str_class = match AnyClass::get(c"NSMutableAttributedString") {
            Some(cls) => cls,
            None => return,
        };

        static BRAND_ICON: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let brand_icon_ptr = *BRAND_ICON.get_or_init(|| {
            let bytes = include_bytes!("../icons/32x32.png");
            let ns_data: *mut AnyObject =
                objc2::msg_send![data_class, dataWithBytes: bytes.as_ptr(), length: bytes.len()];
            let icon: *mut AnyObject = objc2::msg_send![img_class, alloc];
            let icon: *mut AnyObject = objc2::msg_send![icon, initWithData: ns_data];
            let icon_size = objc2_foundation::NSSize::new(16.0, 16.0);
            let _: () = objc2::msg_send![icon, setSize: icon_size];
            icon as usize
        }) as *mut AnyObject;

        let font: *mut AnyObject = objc2::msg_send![
            font_class,
            monospacedDigitSystemFontOfSize: 8.5f64,
            weight: 0.23f64
        ];
        let text_color: *mut AnyObject = objc2::msg_send![color_class, labelColor];

        let style: *mut AnyObject = objc2::msg_send![style_class, new];
        let _: () = objc2::msg_send![style, setMaximumLineHeight: 9.0f64];
        let _: () = objc2::msg_send![style, setMinimumLineHeight: 9.0f64];
        let _: () = objc2::msg_send![style, setLineSpacing: 0.0f64];

        let key_font: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSFont".as_ptr()];
        let key_style: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSParagraphStyle".as_ptr()];
        let key_color: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSColor".as_ptr()];

        let make_attr_str = |text: &str| -> Option<*mut AnyObject> {
            let c_text = CString::new(text).ok()?;
            let ns_str: *mut AnyObject =
                objc2::msg_send![str_class, stringWithUTF8String: c_text.as_ptr()];
            let attr: *mut AnyObject = objc2::msg_send![attr_str_class, alloc];
            let attr: *mut AnyObject = objc2::msg_send![attr, initWithString: ns_str];
            let len: usize = objc2::msg_send![ns_str, length];
            let full_range = objc2_foundation::NSRange::new(0, len);
            let _: () =
                objc2::msg_send![attr, addAttribute: key_font, value: font, range: full_range];
            let _: () =
                objc2::msg_send![attr, addAttribute: key_style, value: style, range: full_range];
            let _: () = objc2::msg_send![attr, addAttribute: key_color, value: text_color, range: full_range];
            Some(attr)
        };

        let up_attr = match make_attr_str(up_line) {
            Some(a) => a,
            None => return,
        };
        let down_attr = match make_attr_str(down_line) {
            Some(a) => a,
            None => return,
        };

        let total_width = 89.0f64;
        let total_height = 22.0f64;

        let img: *mut AnyObject = objc2::msg_send![img_class, alloc];
        let img_size = objc2_foundation::NSSize::new(total_width, total_height);
        let img: *mut AnyObject = objc2::msg_send![img, initWithSize: img_size];
        if img.is_null() {
            return;
        }

        let _: () = objc2::msg_send![img, lockFocus];

        let icon_rect = objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(0.0, 3.0),
            objc2_foundation::NSSize::new(16.0, 16.0),
        );
        let _: () = objc2::msg_send![brand_icon_ptr, drawInRect: icon_rect];

        let up_pt = objc2_foundation::NSPoint::new(21.0, 11.8);
        let _: () = objc2::msg_send![up_attr, drawAtPoint: up_pt];

        let down_pt = objc2_foundation::NSPoint::new(21.0, 2.0);
        let _: () = objc2::msg_send![down_attr, drawAtPoint: down_pt];

        let _: () = objc2::msg_send![img, unlockFocus];

        let _: () = objc2::msg_send![button, setImage: img];
        let _: () = objc2::msg_send![button, setImagePosition: 1usize]; // NSImageOnly
        let empty_str: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"".as_ptr()];
        let _: () = objc2::msg_send![button, setTitle: empty_str];
    }
}

/// Reset status bar button back to default brand icon
#[cfg(target_os = "macos")]
fn reset_status_bar_brand_icon() {
    use objc2::runtime::{AnyClass, AnyObject};

    unsafe {
        let button = match get_status_bar_button() {
            Some(btn) => btn,
            None => return,
        };
        let img_class = match AnyClass::get(c"NSImage") {
            Some(cls) => cls,
            None => return,
        };
        let data_class = match AnyClass::get(c"NSData") {
            Some(cls) => cls,
            None => return,
        };
        let str_class = match AnyClass::get(c"NSString") {
            Some(cls) => cls,
            None => return,
        };

        static BRAND_ICON: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let brand_icon_ptr = *BRAND_ICON.get_or_init(|| {
            let bytes = include_bytes!("../icons/32x32.png");
            let ns_data: *mut AnyObject =
                objc2::msg_send![data_class, dataWithBytes: bytes.as_ptr(), length: bytes.len()];
            let icon: *mut AnyObject = objc2::msg_send![img_class, alloc];
            let icon: *mut AnyObject = objc2::msg_send![icon, initWithData: ns_data];
            let icon_size = objc2_foundation::NSSize::new(16.0, 16.0);
            let _: () = objc2::msg_send![icon, setSize: icon_size];
            icon as usize
        }) as *mut AnyObject;

        let _: () = objc2::msg_send![button, setImage: brand_icon_ptr];
        let _: () = objc2::msg_send![button, setImagePosition: 1usize];
        let empty_str: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"".as_ptr()];
        let _: () = objc2::msg_send![button, setTitle: empty_str];
    }
}

/// Start the status bar telemetry loop on macOS (kept for compatibility but unused)
pub fn spawn_status_bar_loop(_app_handle: AppHandle, _engine: SharedEngine) {
    #[cfg(target_os = "macos")]
    {
        // Status bar updates are driven by the telemetry loop
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = _app_handle;
        let _ = _engine;
    }
}

use crate::engine::SharedEngine;
use std::time::Instant;
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tracing::info;

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
    #[cfg(target_os = "macos")]
    apply_status_bar_font();
}

/// Set the tray ID from the actual tray icon
pub fn set_tray_id(tray_id: String) {
    if let Some(s) = STATUS_BAR_STATE.get() {
        let mut s = s.lock().unwrap();
        s.tray_id = Some(tray_id);
    }
    #[cfg(target_os = "macos")]
    apply_status_bar_font();
}

/// Update tray icon title with current rates (bytes per second converted to KB/s)
/// Uses two-line format: upload on top, download on bottom
pub fn update_status_bar(app_handle: &AppHandle, upload_rate: u64, download_rate: u64) {
    #[cfg(target_os = "macos")]
    {
        // Get the tray ID - use the stored one or default to "nebulae-tray"
        let tray_id_opt = {
            let s = STATUS_BAR_STATE.get();
            s.and_then(|s| s.lock().ok())
                .and_then(|s| s.tray_id.clone())
        };

        // Early return if no tray is configured yet
        let tray_id = match tray_id_opt {
            Some(id) => id,
            None => {
                info!("[status_bar] Tray not configured yet, skipping update");
                return;
            }
        };

        info!(
            "[status_bar] Attempting to update tray '{}' with rates: upload={:?} download={:?}",
            tray_id, upload_rate, download_rate
        );

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

        // Two-line stacked presentation:
        // Line 1: ▲ <upload>
        // Line 2: ▼ <download>
        // Uses 12-char column alignment, figure-space padding, and compact adaptive units.
        let text = crate::speed_format::format_stacked_status_bar(upload_rate, download_rate);

        // Update tray icon title - this displays text beside the icon on macOS
        if let Some(tray) = app_handle.tray_by_id(&tray_id) {
            if let Err(e) = tray.set_title(Some(&text)) {
                info!("[status_bar] set_title failed: {}", e);
            }
            apply_status_bar_attributed_title(&text);
        } else {
            info!("[status_bar] Tray not found");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
        let _upload_rate = upload_rate;
        let _download_rate = download_rate;
    }
}

/// Clear the status bar text - called on idle or engine stop
pub fn clear_status_bar(app_handle: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        // Clear title, preserve icon
        let tray_id = {
            let s = STATUS_BAR_STATE.get().and_then(|s| s.lock().ok());
            s.and_then(|s| s.tray_id.clone())
                .unwrap_or_else(|| "nebulae-tray".to_string())
        };
        info!("[status_bar] clearing status");
        if let Some(tray) = app_handle.tray_by_id(&tray_id) {
            if let Err(e) = tray.set_title(Some("")) {
                info!("[status_bar] clear_status_bar set_title failed: {}", e);
            }
        } else {
            info!("[status_bar] Tray not found for clear");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
    }
}

/// Configure NSStatusBarButton to use a compact monospaced digit font (8.5pt medium)
/// with precise paragraph style line height (10.0pt) and baseline offset (+3.5pt)
/// ensuring both lines of the stacked rate fit within the menu bar height without clipping.
#[cfg(target_os = "macos")]
pub fn apply_status_bar_attributed_title(title: &str) {
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::CString;

    let c_text = match CString::new(title) {
        Ok(s) => s,
        Err(_) => return,
    };

    unsafe {
        let app_class = match AnyClass::get(c"NSApplication") {
            Some(cls) => cls,
            None => return,
        };
        let ns_app: *mut AnyObject = objc2::msg_send![app_class, sharedApplication];
        if ns_app.is_null() {
            return;
        }
        let windows: *mut AnyObject = objc2::msg_send![ns_app, windows];
        if windows.is_null() {
            return;
        }
        let count: usize = objc2::msg_send![windows, count];

        let font_class = match AnyClass::get(c"NSFont") {
            Some(cls) => cls,
            None => return,
        };
        // monospacedDigitSystemFontOfSize: 8.5, weight: 0.23 (NSFontWeightMedium)
        let font: *mut AnyObject = objc2::msg_send![
            font_class,
            monospacedDigitSystemFontOfSize: 8.5f64,
            weight: 0.23f64
        ];
        if font.is_null() {
            return;
        }

        let str_class = match AnyClass::get(c"NSString") {
            Some(cls) => cls,
            None => return,
        };
        let ns_str: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c_text.as_ptr()];
        if ns_str.is_null() {
            return;
        }

        let style_class = match AnyClass::get(c"NSMutableParagraphStyle") {
            Some(cls) => cls,
            None => return,
        };
        let style: *mut AnyObject = objc2::msg_send![style_class, new];
        if style.is_null() {
            return;
        }
        let _: () = objc2::msg_send![style, setMaximumLineHeight: 10.0f64];
        let _: () = objc2::msg_send![style, setMinimumLineHeight: 10.0f64];
        let _: () = objc2::msg_send![style, setLineSpacing: 0.0f64];
        let _: () = objc2::msg_send![style, setAlignment: 0usize]; // left

        let num_class = match AnyClass::get(c"NSNumber") {
            Some(cls) => cls,
            None => return,
        };
        let baseline_offset: *mut AnyObject = objc2::msg_send![num_class, numberWithDouble: 3.5f64];

        let color_class = match AnyClass::get(c"NSColor") {
            Some(cls) => cls,
            None => return,
        };
        let color: *mut AnyObject = objc2::msg_send![color_class, labelColor];

        let key_font: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSFont".as_ptr()];
        let key_style: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSParagraphStyle".as_ptr()];
        let key_offset: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSBaselineOffset".as_ptr()];
        let key_color: *mut AnyObject =
            objc2::msg_send![str_class, stringWithUTF8String: c"NSColor".as_ptr()];

        let attr_class = match AnyClass::get(c"NSMutableAttributedString") {
            Some(cls) => cls,
            None => return,
        };
        let attr_str: *mut AnyObject = objc2::msg_send![attr_class, alloc];
        let attr_str: *mut AnyObject = objc2::msg_send![attr_str, initWithString: ns_str];
        if attr_str.is_null() {
            return;
        }

        let len: usize = objc2::msg_send![ns_str, length];
        let full_range = objc2_foundation::NSRange::new(0, len);

        let _: () =
            objc2::msg_send![attr_str, addAttribute: key_font, value: font, range: full_range];
        let _: () =
            objc2::msg_send![attr_str, addAttribute: key_style, value: style, range: full_range];
        let _: () = objc2::msg_send![attr_str, addAttribute: key_offset, value: baseline_offset, range: full_range];
        let _: () =
            objc2::msg_send![attr_str, addAttribute: key_color, value: color, range: full_range];

        let status_win_class = match AnyClass::get(c"NSStatusBarWindow") {
            Some(cls) => cls,
            None => return,
        };
        let status_btn_class = match AnyClass::get(c"NSStatusBarButton") {
            Some(cls) => cls,
            None => return,
        };

        for i in 0..count {
            let win: *mut AnyObject = objc2::msg_send![windows, objectAtIndex: i];
            if win.is_null() {
                continue;
            }
            let is_status_window: bool = objc2::msg_send![win, isKindOfClass: status_win_class];
            if is_status_window {
                let view: *mut AnyObject = objc2::msg_send![win, contentView];
                if !view.is_null() {
                    let is_status_btn: bool =
                        objc2::msg_send![view, isKindOfClass: status_btn_class];
                    if is_status_btn {
                        let _: () = objc2::msg_send![view, setAttributedTitle: attr_str];
                    }
                }
            }
        }
    }
}

/// Fallback font config when title is not yet populated
#[cfg(target_os = "macos")]
pub fn apply_status_bar_font() {
    use objc2::runtime::{AnyClass, AnyObject};
    unsafe {
        let app_class = match AnyClass::get(c"NSApplication") {
            Some(cls) => cls,
            None => return,
        };
        let ns_app: *mut AnyObject = objc2::msg_send![app_class, sharedApplication];
        if ns_app.is_null() {
            return;
        }
        let windows: *mut AnyObject = objc2::msg_send![ns_app, windows];
        if windows.is_null() {
            return;
        }
        let count: usize = objc2::msg_send![windows, count];
        let font_class = match AnyClass::get(c"NSFont") {
            Some(cls) => cls,
            None => return,
        };
        let font: *mut AnyObject = objc2::msg_send![
            font_class,
            monospacedDigitSystemFontOfSize: 8.5f64,
            weight: 0.23f64
        ];
        if font.is_null() {
            return;
        }

        let status_win_class = match AnyClass::get(c"NSStatusBarWindow") {
            Some(cls) => cls,
            None => return,
        };
        let status_btn_class = match AnyClass::get(c"NSStatusBarButton") {
            Some(cls) => cls,
            None => return,
        };

        for i in 0..count {
            let win: *mut AnyObject = objc2::msg_send![windows, objectAtIndex: i];
            if win.is_null() {
                continue;
            }
            let is_status_window: bool = objc2::msg_send![win, isKindOfClass: status_win_class];
            if is_status_window {
                let view: *mut AnyObject = objc2::msg_send![win, contentView];
                if !view.is_null() {
                    let is_status_btn: bool =
                        objc2::msg_send![view, isKindOfClass: status_btn_class];
                    if is_status_btn {
                        let _: () = objc2::msg_send![view, setFont: font];
                    }
                }
            }
        }
    }
}

/// Start the status bar telemetry loop on macOS (kept for compatibility but unused)
pub fn spawn_status_bar_loop(_app_handle: AppHandle, _engine: SharedEngine) {
    #[cfg(target_os = "macos")]
    {
        // Status bar updates are now driven by the telemetry loop (500ms, throttled to 1s)
        // This function is kept as a placeholder but does nothing
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = _app_handle;
        let _ = _engine;
    }
}

//! Embedded playback built on VLC's engine library (libVLC).
//!
//! ONE media player for everything the user plays: video files, DVD and
//! Blu-ray disc images. libVLC is the only engine in the codebase with real
//! DVD menu support (mpv removed menus upstream years ago), and on macOS the
//! video renders into a native view layered above the webview so playback
//! lives inside Nebulae's own window with Nebulae's own controls - the VLC
//! application interface is never shown.
//!
//! libVLC itself is NOT bundled: it is loaded at runtime from the user's
//! installed VLC (VLC.app on macOS, the standard Windows/Linux installs).
//! Bundling VLC would relicense the whole app under GPL and ship ~200 MB of
//! plugins nobody needs duplicated.

pub mod engine;

#[cfg(target_os = "macos")]
pub use embed_macos::{
    apply_window_corner_radius, begin_window_drag, ensure_overlay, fit_player_window_to_aspect,
    miniaturize, set_overlay_hidden, set_window_corner_radius, sync_frame, toggle_fullscreen,
    window_close_native, window_is_maximized_native, window_maximize_toggle_native,
    window_minimize_native,
};
pub mod bluray_meta;
pub mod dvd_meta;
pub mod letterbox;

#[cfg(target_os = "windows")]
pub use embed_windows::{
    apply_window_corner_radius, begin_window_drag, ensure_overlay, fit_player_window_to_aspect,
    miniaturize, set_overlay_hidden, set_window_corner_radius, sync_frame, toggle_fullscreen,
    window_close_native, window_is_maximized_native, window_maximize_toggle_native,
    window_minimize_native,
};

#[cfg(target_os = "linux")]
pub use embed_linux::{
    apply_window_corner_radius, begin_window_drag, ensure_overlay, fit_player_window_to_aspect,
    miniaturize, set_overlay_hidden, set_window_corner_radius, sync_frame, toggle_fullscreen,
    window_close_native, window_is_maximized_native, window_maximize_toggle_native,
    window_minimize_native,
};

#[cfg(target_os = "macos")]
pub(crate) mod embed_macos;

#[cfg(target_os = "windows")]
pub(crate) mod embed_windows;

#[cfg(target_os = "linux")]
mod embed_linux;

use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

/// Global playback state shared by the Tauri commands.
///
/// Raw pointers are stored as `usize` because the NSView must only ever be
/// touched on the main thread (we re-cast inside `run_on_main_thread`), and
/// `Retained<NSView>` is `!Send`.
#[derive(Default)]
pub struct PlayerState {
    /// macOS: raw `NSView*` of the video overlay layering above the webview.
    pub overlay_view: AtomicUsize,
    /// macOS: raw `NebulaeEDRScreenObserver*` for screen-change notifications.
    /// Stored as AtomicUsize because Retained<EDRScreenObserver> is !Send.
    /// Lives for the process lifetime, like the overlay view itself.
    pub edr_observer: AtomicUsize,
    /// macOS: latches true once the EDR dynamic-range flag has actually been
    /// applied to the VLC video layer for the current session. Reset on each
    /// new session; the player poll retries until the layer exists.
    pub edr_applied: std::sync::atomic::AtomicBool,
    /// Indicates whether the active session is playing HDR / UHD content.
    /// Used by embed_macos to configure ITU-R BT.2100 PQ color space on the
    /// video layer instead of standard sRGB.
    pub is_hdr: std::sync::atomic::AtomicBool,
    /// Indicates whether the active session is playing in direct main-feature mode
    /// with menus bypassed (via :no-bluray-menu and :no-dvdnav-menu).
    pub bypass_menu: std::sync::atomic::AtomicBool,
    /// The loaded libVLC, created on first use.
    pub lib: Mutex<Option<std::sync::Arc<engine::VlcLib>>>,
    /// libvlc_instance_t*
    pub instance: Mutex<usize>,
    /// libvlc_media_t*
    pub media: Mutex<usize>,
    /// libvlc_media_player_t*
    pub media_player: Mutex<usize>,
    /// Set once playback starts; lets `player_poll` report "ended" exactly once.
    pub started: Mutex<bool>,
    /// ONE libvlc instance shared by every session for the app's lifetime:
    /// releasing an instance (old behavior) runs libxml2's global
    /// xmlCleanupParser, which races the next session's metadata parsing
    /// and aborts (malloc "pointer being freed was not allocated" on Play).
    pub shared_instance: AtomicUsize,
    /// What the owner asked to play (may be a folder) — kept so "return to
    /// disc menu" can restart playback fresh when dvdnav ignores set_title.
    pub current_path: Mutex<Option<String>>,
    /// The resolved playback target (ISO/VIDEO_TS root) of the session.
    pub current_resolved: Mutex<Option<String>>,
    /// Guard around every VLC call that uses the media-player pointer and
    /// around the session teardown that releases it: background helpers
    /// (menu watcher) must never call into a released player. Held for the
    /// duration of each call, never across waits.
    pub media_player_cell: std::sync::Mutex<()>,
    /// Monotonic session generation: bumped by stop_session and by every
    /// successful open. Background helpers (liveness probe) capture the
    /// generation at spawn and must bail out when it changes, so a stale
    /// helper can never act on a session that replaced the one it was
    /// started for.
    pub session_gen: AtomicUsize,
    /// Serializes session open and teardown calls so concurrent open requests
    /// cannot race and create duplicate concurrent playback sessions.
    pub open_lock: tokio::sync::Mutex<()>,
    /// Encoded black-bar cropping state.
    pub framing: Mutex<FramingState>,
    /// Last poll transition state logged: (gen, title, at_menu, playing) for bounded diagnostics.
    pub last_logged_poll_state: Mutex<Option<(u64, i32, bool, bool)>>,
}

/// Black-bar measurement and crop tracking state for the active session.
pub struct FramingState {
    pub tracker: letterbox::CropTracker,
    pub title: i32,
    pub applied: Option<letterbox::Bars>,
    pub sampling: bool,
    pub pending: Option<letterbox::BarObservation>,
    pub last_sample_at: Option<std::time::Instant>,
    pub source: (u32, u32),
    pub sar: f64,
    pub window_fitted_aspect: Option<f64>,
    pub settled: bool,
    pub stable_samples: u32,
    pub polls_since_reassert: u32,
}

impl FramingState {
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.title = -1;
        self.applied = None;
        self.sampling = false;
        self.pending = None;
        self.last_sample_at = None;
        self.source = (0, 0);
        self.sar = 1.0;
        self.window_fitted_aspect = None;
        self.settled = false;
        self.stable_samples = 0;
        self.polls_since_reassert = 0;
    }

    pub fn forget_measurements(&mut self) {
        self.tracker.reset();
        self.applied = None;
        self.pending = None;
        self.sampling = false;
        self.settled = false;
        self.stable_samples = 0;
        self.polls_since_reassert = 0;
    }

    pub fn source_display_aspect(&self) -> Option<f64> {
        let (w, h) = self.source;
        if w == 0 || h == 0 {
            None
        } else {
            Some((w as f64 / h as f64) * self.sar)
        }
    }
}

impl Default for FramingState {
    fn default() -> Self {
        Self {
            tracker: letterbox::CropTracker::new(letterbox::DEFAULT_CONFIRMATIONS),
            title: -1,
            applied: None,
            sampling: false,
            pending: None,
            last_sample_at: None,
            source: (0, 0),
            sar: 1.0,
            window_fitted_aspect: None,
            settled: false,
            stable_samples: 0,
            polls_since_reassert: 0,
        }
    }
}

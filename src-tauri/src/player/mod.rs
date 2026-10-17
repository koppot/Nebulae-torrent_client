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
    begin_window_drag, ensure_overlay, set_overlay_hidden, set_window_corner_radius, sync_frame,
    toggle_fullscreen,
};
pub mod bluray_meta;

#[cfg(target_os = "windows")]
pub use embed_windows::{ensure_overlay, set_overlay_hidden, sync_frame};

#[cfg(target_os = "linux")]
pub use embed_linux::{ensure_overlay, set_overlay_hidden, sync_frame};

#[cfg(target_os = "macos")]
mod embed_macos;

#[cfg(target_os = "windows")]
mod embed_windows;

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
    /// What the owner asked to play (may be a folder) — kept so "return to
    /// disc menu" can restart playback fresh when dvdnav ignores set_title.
    pub current_path: Mutex<Option<String>>,
    /// The resolved playback target (ISO/VIDEO_TS root) of the session.
    pub current_resolved: Mutex<Option<String>>,
    /// While true, a post-open watcher keeps forcing the disc's MENU title:
    /// VLC's Blu-ray demuxer starts the disc's FIRST-PLAY flow (warning →
    /// program chain) and would otherwise run straight into playback.
    /// Cleared when the frontend wants playback (resume / menu Play).
    pub menu_watch: std::sync::atomic::AtomicBool,
}

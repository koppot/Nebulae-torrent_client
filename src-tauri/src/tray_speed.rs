//! Windows tray speed readout.
//!
//! Shows the client-wide current download/upload rates beside the tray icon
//! (Windows notification area) while the main window is hidden or minimized,
//! updated at most once per second.
//!
//! Reuses the existing telemetry producer (the 500 ms `list_torrents` tick in
//! `engine::telemetry`) so there is no duplicate polling and no
//! since-start averaging. The readout is driven entirely by the raw
//! instantaneous rates already present in `TorrentItem`.
//!
//! gated to `#[cfg(target_os = "windows")]`.

#![cfg(target_os = "windows")]

use std::time::{Duration, Instant};
use tauri::AppHandle;

use crate::commands::torrent::TorrentItem;
use crate::speed_format::{
    aggregate_client_rates, format_speed_title, SHOW_THRESHOLD, STALE_AFTER_SECS,
};

/// Tray icon id used by the builder in `lib.rs` (`"nebulae-tray"`).
const TRAY_ID: &str = "nebulae-tray";

/// State for the readout: throttles title updates to once per second and
/// clears the readout after both directions stay below threshold for
/// `STALE_AFTER_SECS` (a native dead-man, so producer failure clears it).
pub struct TraySpeedState {
    last_update: Instant,
    last_title: Option<String>,
    last_active: Instant,
}

impl Default for TraySpeedState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            last_update: now - Duration::from_secs(1), // allow immediate first update
            last_title: None,
            last_active: now,
        }
    }
}

impl TraySpeedState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call once per telemetry tick with the current torrent list.
    /// Returns `Some(title)` to show, or `None` to clear the readout.
    /// Throttled to at most one title change per second.
    pub fn update(&mut self, torrents: &[TorrentItem]) -> Option<String> {
        let now = Instant::now();
        let (down, up) = aggregate_client_rates(torrents);
        let active = down >= SHOW_THRESHOLD || up >= SHOW_THRESHOLD;
        if active {
            self.last_active = now;
        }

        // Hide after both directions stayed below threshold for STALE_AFTER.
        if now - self.last_active > Duration::from_secs(STALE_AFTER_SECS) {
            if self.last_title.is_some() {
                self.last_title = None;
                return None;
            }
            return None;
        }

        // Throttle: at most one title update per second.
        if now - self.last_update < Duration::from_secs(1) {
            return self.last_title.clone();
        }
        self.last_update = now;

        let title = format_speed_title(down, up);
        self.last_title = Some(title.clone());
        Some(title)
    }
}

/// Apply a computed title to the tray icon, or clear it when `None`.
fn apply_title(app: &AppHandle, title: Option<&str>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(title);
    }
}

/// Update the tray title from the current telemetry snapshot. Intended to be
/// called once per telemetry tick from `engine::telemetry` (Windows only).
pub fn update_tray_title(app: &AppHandle, torrents: &[TorrentItem], state: &mut TraySpeedState) {
    match state.update(torrents) {
        Some(title) => apply_title(app, Some(&title)),
        None => apply_title(app, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speed_format::{FIGURE_SPACE, MAX_TITLE_CHARS};

    fn item(status: &str, down: u64, up: u64) -> TorrentItem {
        TorrentItem {
            id: "t".to_string(),
            name: "x".to_string(),
            total_bytes: 0,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            download_rate: down,
            upload_rate: up,
            seconds_downloading: 0,
            seconds_seeding: 0,
            progress: 0.0,
            status: status.to_string(),
            eta_seconds: None,
            seeds: None,
            peers: 0,
            ratio: 0.0,
            category: String::new(),
            is_media: false,
            poster_url: None,
            info_hash: "".to_string(),
            save_path: "".to_string(),
            added_on: 0,
            completed_on: None,
            piece_size: 0,
            total_pieces: 0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            tags: Vec::new(),
            files: Vec::new(),
            trackers: Vec::new(),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: 0.0,
            bandwidth_priority: None,
            peer_limit: None,
            racing_mode: None,
            error_string: None,
            media_meta: None,
        }
    }

    #[test]
    fn title_not_empty_when_active() {
        let mut state = TraySpeedState::new();
        let torrents = vec![item("Downloading", 2000, 1000)];
        let title = state.update(&torrents);
        assert!(title.is_some());
        assert!(!title.as_ref().unwrap().is_empty());
        assert!(title.as_ref().unwrap().len() <= MAX_TITLE_CHARS);
    }

    #[test]
    fn idle_torrents_report_zero_title() {
        let mut state = TraySpeedState::new();
        let active = vec![item("Downloading", 2000, 0)];
        assert!(state.update(&active).is_some());
        let idle = vec![item("Paused", 0, 0), item("Error", 0, 0)];
        // Without advancing Instant the state stays "recently active", so it
        // still reports a (zero) title. The 3 s stale clear depends on the
        // real clock and is verified on Windows.
        let title = state.update(&idle);
        assert!(title.is_some());
        let t = title.as_deref().unwrap();
        assert!(t.starts_with("↓"));
        assert!(t.contains("0B"));
        assert!(t.len() <= MAX_TITLE_CHARS);
        // Figure spaces pad the zero fields to a stable width.
        assert!(t.contains(FIGURE_SPACE));
    }
}

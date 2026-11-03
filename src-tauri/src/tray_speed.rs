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
    aggregate_client_rates, create_composite_tray_icon, format_rate, SHOW_THRESHOLD,
    STALE_AFTER_SECS,
};

/// Tray icon id used by the builder in `lib.rs` (`"nebulae-tray"`).
const TRAY_ID: &str = "nebulae-tray";

/// State for the readout: throttles icon updates to once per second and
/// clears the readout (restoring the default brand icon) after both directions
/// stay below threshold for `STALE_AFTER_SECS`.
pub struct TraySpeedState {
    last_update: Instant,
    last_rates: Option<(u64, u64)>,
    last_active: Instant,
    is_custom_icon: bool,
}

impl Default for TraySpeedState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            last_update: now - Duration::from_secs(1), // allow immediate first update
            last_rates: None,
            last_active: now,
            is_custom_icon: false,
        }
    }
}

impl TraySpeedState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call once per telemetry tick with the current torrent list.
    /// Returns `Some((down, up))` to show, or `None` to clear/restore idle icon.
    /// Throttled to at most one update per second.
    pub fn update(&mut self, torrents: &[TorrentItem]) -> Option<(u64, u64)> {
        let now = Instant::now();
        let (down, up) = aggregate_client_rates(torrents);
        let active = down >= SHOW_THRESHOLD || up >= SHOW_THRESHOLD;
        if active {
            self.last_active = now;
        }

        // Restore idle icon after both directions stayed below threshold for STALE_AFTER_SECS.
        if now - self.last_active > Duration::from_secs(STALE_AFTER_SECS) {
            if self.last_rates.is_some() {
                self.last_rates = None;
                return None;
            }
            return None;
        }

        // Throttle: at most one icon update per second.
        if now - self.last_update < Duration::from_secs(1) {
            return self.last_rates;
        }
        self.last_update = now;

        self.last_rates = Some((down, up));
        Some((down, up))
    }
}

/// Update the tray icon and tooltip from the current telemetry snapshot. Intended to be
/// called once per telemetry tick from `engine::telemetry` (Windows only).
pub fn update_tray_title(app: &AppHandle, torrents: &[TorrentItem], state: &mut TraySpeedState) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match state.update(torrents) {
            Some((down, up)) => {
                let rgba = create_composite_tray_icon(up, down);
                let img = tauri::image::Image::new_owned(rgba, 32, 32);
                let _ = tray.set_icon(Some(img));
                state.is_custom_icon = true;

                // Also update tooltip with exact rates
                let tooltip = format!("Nebulae — ↑ {}  ↓ {}", format_rate(up), format_rate(down));
                let _ = tray.set_tooltip(Some(&tooltip));
            }
            None => {
                if state.is_custom_icon {
                    if let Some(default_icon) = app.default_window_icon() {
                        let _ = tray.set_icon(Some(default_icon.clone()));
                    }
                    let _ = tray.set_tooltip(Some("Nebulae — Client by Asian DVD Club"));
                    state.is_custom_icon = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            comment: String::new(),
            created_by: String::new(),
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
    fn rates_reported_when_active() {
        let mut state = TraySpeedState::new();
        let torrents = vec![item("Downloading", 2000, 1000)];
        let rates = state.update(&torrents);
        assert!(rates.is_some());
        assert_eq!(rates.unwrap(), (2000, 1000));
    }

    #[test]
    fn idle_torrents_report_none_initially() {
        let mut state = TraySpeedState::new();
        let idle = vec![item("Paused", 0, 0), item("Error", 0, 0)];
        let rates = state.update(&idle);
        assert!(rates.is_none());
    }
}

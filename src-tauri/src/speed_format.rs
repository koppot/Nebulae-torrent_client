//! Pure formatting helpers for the Windows tray speed readout.
//!
//! These functions have no platform or UI dependencies so they can be
//! unit-tested on any host. The tray integration itself lives in
//! `tray_speed.rs` (`#[cfg(target_os = "windows")]`).
//!
//! Rates are the raw instantaneous engine rates (`TorrentItem.download_rate` /
//! `upload_rate`, bytes/s) already present in the telemetry producer. There is
//! no since-start averaging here and no duplicate polling.

use crate::commands::torrent::TorrentItem;

/// Threshold at or above which the readout is shown (1 KiB/s).
pub const SHOW_THRESHOLD: u64 = 1_024;
/// How long both directions may stay below threshold before the readout hides.
pub const STALE_AFTER_SECS: u64 = 3;
/// Maximum width of the formatted rate title, in characters.
pub const MAX_TITLE_CHARS: usize = 14;
/// Unicode figure space — width of a digit, used for stable padding.
pub const FIGURE_SPACE: char = '\u{2007}';

const UNITS: &[&str] = &["B", "K", "M", "G", "T"];

/// Format a single direction's bytes-per-second as `0B`, `999B`, `1.0K`,
/// `1.2M`, `10.5M`, … using the largest unit that keeps the value ≥ 1.
pub fn format_rate(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "0B".to_string();
    }
    let mut val = bytes_per_sec as f64;
    let mut unit_idx = 0usize;
    while val >= 1024.0 && unit_idx < UNITS.len() - 1 {
        val /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{}{}", val as u64, UNITS[0])
    } else {
        let val = (val * 10.0).floor() / 10.0;
        format!("{:.1}{}", val, UNITS[unit_idx])
    }
}

/// Right-pad a rate string with figure spaces to `width`, or return it
/// unchanged when it already meets or exceeds `width`.
fn pad_rate(s: &str, width: usize) -> String {
    let char_count = s.chars().count();
    if char_count >= width {
        s.to_string()
    } else {
        format!(
            "{}{}",
            FIGURE_SPACE.to_string().repeat(width - char_count),
            s
        )
    }
}

/// Format the combined tray title `↓<down>  ↑<up>`, each rate right-padded
/// with figure spaces to a stable 5-char field so the title does not jitter
/// as the numbers change.
pub fn format_speed_title(down: u64, up: u64) -> String {
    let down_str = pad_rate(&format_rate(down), 5);
    let up_str = pad_rate(&format_rate(up), 5);
    format!("↓{}  ↑{}", down_str, up_str)
}

/// Sum the current download and upload rates across all active torrents,
/// returning the client-wide bytes-per-second for each direction.
///
/// Only `Downloading` and `Seeding` torrents contribute; a paused or errored
/// torrent reports zero from the engine anyway, but the status check makes
/// the intent explicit and matches the Manager's active logic.
pub fn aggregate_client_rates(torrents: &[TorrentItem]) -> (u64, u64) {
    let mut down = 0u64;
    let mut up = 0u64;
    for t in torrents {
        if t.status == "Downloading" || t.status == "Seeding" {
            down = down.saturating_add(t.download_rate);
            up = up.saturating_add(t.upload_rate);
        }
    }
    (down, up)
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
    fn format_rate_zero() {
        assert_eq!(format_rate(0), "0B");
    }

    #[test]
    fn format_rate_bytes() {
        assert_eq!(format_rate(500), "500B");
        assert_eq!(format_rate(999), "999B");
    }

    #[test]
    fn format_rate_kibibytes() {
        assert_eq!(format_rate(1024), "1.0K");
        assert_eq!(format_rate(1536), "1.5K");
        assert_eq!(format_rate(1024 * 1024 - 1), "1023.9K");
    }

    #[test]
    fn format_rate_mebibytes() {
        assert_eq!(format_rate(1024 * 1024), "1.0M");
        assert_eq!(format_rate(1024 * 1024 * 12), "12.0M");
    }

    #[test]
    fn format_rate_gibibytes() {
        assert_eq!(format_rate(1024u64.pow(3)), "1.0G");
    }

    #[test]
    fn format_rate_tebibytes() {
        assert_eq!(format_rate(1024u64.pow(4)), "1.0T");
    }

    #[test]
    fn title_width_budget() {
        let title = format_speed_title(1_234_567, 89_012);
        assert!(
            title.chars().count() <= MAX_TITLE_CHARS,
            "title too long: {title}"
        );
    }

    #[test]
    fn title_uses_figurespace_padding() {
        let title = format_speed_title(500, 7);
        // FIGURE SPACE pads the short "7" field so both rate fields align.
        assert!(title.contains(FIGURE_SPACE));
    }

    #[test]
    fn aggregate_sums_active_only() {
        let torrents = vec![
            item("Downloading", 1000, 2000),
            item("Seeding", 0, 3000),
            item("Paused", 5000, 6000),
            item("Error", 7000, 8000),
        ];
        let (down, up) = aggregate_client_rates(&torrents);
        assert_eq!(down, 1000);
        assert_eq!(up, 5000);
    }

    #[test]
    fn aggregate_empty() {
        assert_eq!(aggregate_client_rates(&[]), (0, 0));
    }
}

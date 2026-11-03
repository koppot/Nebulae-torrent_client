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

/// Format a concise rate string for the dynamic tray icon (e.g. `0B`, `999B`, `9.5K`, `82K`, `500K`, `1.2M`, `99M`, `1.2G`).
pub fn format_tray_rate(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "0B".to_string();
    }
    let mut val = bytes_per_sec as f64;
    let units = ["B", "K", "M", "G", "T"];
    let mut unit_idx = 0usize;
    while val >= 1024.0 && unit_idx < units.len() - 1 {
        val /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{}{}", val as u64, units[0])
    } else if val < 10.0 {
        let val = (val * 10.0).floor() / 10.0;
        format!("{:.1}{}", val, units[unit_idx])
    } else {
        format!("{:.0}{}", val.floor(), units[unit_idx])
    }
}

const BRAND_ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");

/// 3x5 bitmap glyph definitions for digits, units, and arrows.
fn glyph_3x5(c: char) -> Option<[u8; 5]> {
    match c {
        '0' => Some([0b111, 0b101, 0b101, 0b101, 0b111]),
        '1' => Some([0b010, 0b110, 0b010, 0b010, 0b111]),
        '2' => Some([0b111, 0b001, 0b111, 0b100, 0b111]),
        '3' => Some([0b111, 0b001, 0b111, 0b001, 0b111]),
        '4' => Some([0b101, 0b101, 0b111, 0b001, 0b001]),
        '5' => Some([0b111, 0b100, 0b111, 0b001, 0b111]),
        '6' => Some([0b111, 0b100, 0b111, 0b101, 0b111]),
        '7' => Some([0b111, 0b001, 0b010, 0b010, 0b010]),
        '8' => Some([0b111, 0b101, 0b111, 0b101, 0b111]),
        '9' => Some([0b111, 0b101, 0b111, 0b001, 0b111]),
        'B' => Some([0b110, 0b101, 0b110, 0b101, 0b110]),
        'K' => Some([0b101, 0b110, 0b100, 0b110, 0b101]),
        'M' => Some([0b101, 0b111, 0b101, 0b101, 0b101]),
        'G' => Some([0b111, 0b100, 0b101, 0b101, 0b111]),
        'T' => Some([0b111, 0b010, 0b010, 0b010, 0b010]),
        '↑' | '^' => Some([0b010, 0b111, 0b010, 0b010, 0b010]),
        '↓' | 'v' => Some([0b010, 0b010, 0b010, 0b111, 0b010]),
        _ => None,
    }
}

fn draw_glyph(canvas: &mut [u8], c: char, start_x: usize, start_y: usize, color: [u8; 4]) -> usize {
    if c == '.' {
        let px = start_x;
        let py = start_y + 4;
        if px < 32 && py < 32 {
            let idx = (py * 32 + px) * 4;
            canvas[idx..idx + 4].copy_from_slice(&color);
        }
        return 2;
    }
    if let Some(rows) = glyph_3x5(c) {
        for (dy, &row) in rows.iter().enumerate() {
            let py = start_y + dy;
            if py >= 32 {
                continue;
            }
            for dx in 0..3 {
                let px = start_x + dx;
                if px >= 32 {
                    continue;
                }
                if (row & (1 << (2 - dx))) != 0 {
                    let idx = (py * 32 + px) * 4;
                    canvas[idx..idx + 4].copy_from_slice(&color);
                }
            }
        }
        4
    } else {
        2
    }
}

fn draw_text_with_shadow(
    canvas: &mut [u8],
    text: &str,
    x: usize,
    y: usize,
    color: [u8; 4],
    shadow: [u8; 4],
) {
    // 1. Draw 1px drop shadow offset (+1, +1)
    let mut cur_x = x;
    for ch in text.chars() {
        if cur_x >= 32 {
            break;
        }
        cur_x += draw_glyph(canvas, ch, cur_x + 1, y + 1, shadow);
    }
    // 2. Draw foreground glyphs
    let mut cur_x = x;
    for ch in text.chars() {
        if cur_x >= 32 {
            break;
        }
        cur_x += draw_glyph(canvas, ch, cur_x, y, color);
    }
}

/// Creates a 32x32 composite RGBA tray icon containing:
/// - The approved Nebulae brand icon on the left (12x12, vertically centered at Y=10)
/// - Upload rate on TOP (`↑ <up>`)
/// - Download rate on BOTTOM (`↓ <down>`)
pub fn create_composite_tray_icon(up: u64, down: u64) -> Vec<u8> {
    let mut canvas = vec![0u8; 32 * 32 * 4];

    // 1. Approved brand icon on the left (12x12, X=0..11, Y=10..21)
    if let Ok(brand_img) = image::load_from_memory(BRAND_ICON_PNG) {
        let rgba = brand_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0;
        let mut max_y = 0;
        for y in 0..h {
            for x in 0..w {
                if rgba.get_pixel(x, y)[3] > 32 {
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
        let cropped = if min_x <= max_x && min_y <= max_y {
            image::imageops::crop_imm(&rgba, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
                .to_image()
        } else {
            rgba
        };
        let resized =
            image::imageops::resize(&cropped, 12, 12, image::imageops::FilterType::Lanczos3);
        for y in 0..12 {
            for x in 0..12 {
                let p = resized.get_pixel(x, y);
                let target_y = 10 + y as usize;
                let target_x = x as usize;
                let idx = (target_y * 32 + target_x) * 4;
                canvas[idx..idx + 4].copy_from_slice(&p.0);
            }
        }
    }

    // 2. Format rates
    let up_text = format!("↑{}", format_tray_rate(up));
    let down_text = format!("↓{}", format_tray_rate(down));

    let shadow_color = [0, 0, 0, 220];
    let up_color = [56, 189, 248, 255]; // high-contrast light blue/cyan
    let down_color = [74, 222, 128, 255]; // high-contrast bright green

    // Upload on TOP (Y=4)
    draw_text_with_shadow(&mut canvas, &up_text, 13, 4, up_color, shadow_color);

    // Download on BOTTOM (Y=19)
    draw_text_with_shadow(&mut canvas, &down_text, 13, 19, down_color, shadow_color);

    canvas
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

/// Format the combined tray title `↑<up>  ↓<down>`, each rate right-padded
/// with figure spaces to a stable 5-char field so the title does not jitter
/// as the numbers change.
pub fn format_speed_title(down: u64, up: u64) -> String {
    let down_str = pad_rate(&format_rate(down), 5);
    let up_str = pad_rate(&format_rate(up), 5);
    format!("↑{}  ↓{}", up_str, down_str)
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

    #[test]
    fn tray_rate_formatting() {
        assert_eq!(format_tray_rate(0), "0B");
        assert_eq!(format_tray_rate(500), "500B");
        assert_eq!(format_tray_rate(1536), "1.5K");
        assert_eq!(format_tray_rate(50 * 1024), "50K");
        assert_eq!(format_tray_rate(500 * 1024), "500K");
        assert_eq!(format_tray_rate(1024 * 1024 * 12), "12M");
        assert_eq!(format_tray_rate(1024 * 1024 * 3 / 2), "1.5M");
        assert_eq!(format_tray_rate(1024u64.pow(3) * 2), "2.0G");
    }

    #[test]
    fn composite_icon_dimensions_and_content() {
        let icon_bytes = create_composite_tray_icon(1_500_000, 4_500_000);
        assert_eq!(icon_bytes.len(), 32 * 32 * 4);
        // Ensure non-zero alpha in left brand icon area (X=5, Y=15)
        let brand_alpha = icon_bytes[(15 * 32 + 5) * 4 + 3];
        assert!(brand_alpha > 0);
        // Ensure non-zero pixels in text regions
        let has_text_pixels = icon_bytes.as_chunks::<4>().0.iter().any(|p| p[3] > 0);
        assert!(has_text_pixels);
    }
}

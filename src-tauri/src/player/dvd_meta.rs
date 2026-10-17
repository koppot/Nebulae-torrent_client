//! DVD-Video stream languages, read directly from the disc's IFO files.
//!
//! libbluray only covers Blu-ray; for DVDs the track labels fall back to
//! VLC's raw names (which are frequently wrong or unspecific). The IFO
//! tables carry the real per-stream language codes:
//!
//! - `VIDEO_TS.IFO` (VMG): the TT_SRPT table maps VLC's title index (1..n)
//!   to a title-set number, so no duration matching is needed.
//! - `VTS_XX_0.IFO`: audio attributes at 0x204 (8-byte records, language
//!   code at bytes 2-3) and subpicture attributes at 0x254 (6-byte records,
//!   language code at bytes 2-3). Both offsets verified against real discs.

/// Language names for ISO 639-1 (2-char) DVD codes. Unmapped codes are
/// passed through unchanged so the label still shows something sensible.
fn lang_name_2(code: &str) -> String {
    match code {
        "en" => "English",
        "ja" => "Japanese",
        "fr" | "fr " | "fr\0" => "French",
        "es" => "Spanish",
        "de" => "German",
        "it" => "Italian",
        "ko" => "Korean",
        "zh" => "Chinese",
        "ru" => "Russian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "no" => "Norwegian",
        "da" => "Danish",
        "fi" => "Finnish",
        "th" => "Thai",
        "pl" => "Polish",
        "hu" => "Hungarian",
        "cs" => "Czech",
        "tr" => "Turkish",
        "he" => "Hebrew",
        "hi" => "Hindi",
        "ar" => "Arabic",
        "el" => "Greek",
        _ => return code.to_string(),
    }
    .to_string()
}

fn clean_code(bytes: &[u8]) -> Option<String> {
    // Language codes are two ASCII letters; 0x00 padding or the placeholder
    // 0x60 0x60 ("``") means the disc leaves the language unspecified.
    if bytes.len() < 2 {
        return None;
    }
    let a = bytes[0];
    let b = bytes[1];
    let is_letter = |c: u8| c.is_ascii_lowercase() || c.is_ascii_uppercase();
    if is_letter(a) && is_letter(b) {
        let s = String::from_utf8_lossy(&[a.to_ascii_lowercase(), b.to_ascii_lowercase()]).into_owned();
        Some(s)
    } else {
        None
    }
}

/// Locate the record table inside TT_SRPT: `nr_titles` 12-byte records whose
/// bytes 4-5 are zero and whose bytes 6/7 carry the title-set number and the
/// per-VTS title number. The record base varies between authors, so scan for
/// the first position where the first record validates.
fn find_tt_records(data: &[u8], tt_srpt: usize, nr_titles: usize) -> Option<usize> {
    let max_scan = tt_srpt + 512;
    let mut pos = tt_srpt + 4;
    while pos + 8 <= max_scan.min(data.len()) {
        let rec = pos;
        let valid = |r: usize| -> bool {
            r + 8 <= data.len()
                && data[r + 4] == 0
                && data[r + 5] == 0
                && (1..=99).contains(&data[r + 6])
                && (1..=99).contains(&data[r + 7])
        };
        if valid(rec) && (nr_titles == 1 || valid(rec + 12)) {
            return Some(rec);
        }
        pos += 4;
    }
    None
}

/// Audio and subtitle language names for `title` (1-based, the current
/// libvlc title number). Returns empty vectors when the path is not a
/// DVD-Video folder or the tables cannot be parsed.
pub fn disc_stream_languages_dvd(
    disc_path: &str,
    title: i64,
) -> Result<(Vec<String>, Vec<String>), String> {
    let root = std::path::Path::new(disc_path);
    // Accept either the VIDEO_TS folder itself or its parent (the value the
    // frontend stores is the resolved playback target, which is the folder
    // containing VIDEO_TS for plain folders).
    let video_ts = if root.join("VIDEO_TS.IFO").is_file() {
        root.to_path_buf()
    } else {
        root.join("VIDEO_TS")
    };
    let vmg_path = video_ts.join("VIDEO_TS.IFO");
    if !vmg_path.is_file() {
        return Err("not a DVD-Video folder".into());
    }
    let vmg = std::fs::read(&vmg_path).map_err(|e| e.to_string())?;
    if vmg.len() < 0xE0 || &vmg[0..12] != b"DVDVIDEO-VMG" {
        return Err("not a DVD-Video IFO".into());
    }
    // TT_SRPT start sector sits at header offset 0xC4.
    let tt_sector = u32::from_be_bytes([vmg[0xC4], vmg[0xC5], vmg[0xC6], vmg[0xC7]]) as usize;
    let tt_off = tt_sector * 2048;
    if tt_off + 8 > vmg.len() {
        return Err("TT_SRPT out of range".into());
    }
    let nr_titles = u16::from_be_bytes([vmg[tt_off], vmg[tt_off + 1]]) as usize;
    if nr_titles == 0 {
        return Err("no titles in TT_SRPT".into());
    }
    let rec_base = find_tt_records(&vmg, tt_off, nr_titles)
        .ok_or_else(|| String::from("TT_SRPT records not found"))?;
    let idx = if title >= 1 { title as usize - 1 } else { 0 };
    let rec = rec_base + idx.min(nr_titles - 1) * 12;
    let vts_nr = vmg[rec + 6] as usize;
    if !(1..=99).contains(&vts_nr) {
        return Err("invalid title-set number".into());
    }

    let vts_path = video_ts.join(format!("VTS_{:02}_0.IFO", vts_nr));
    if !vts_path.is_file() {
        return Err(format!("missing {}", vts_path.display()));
    }
    let vts = std::fs::read(&vts_path).map_err(|e| e.to_string())?;
    if vts.len() < 0x310 || &vts[0..12] != b"DVDVIDEO-VTS" {
        return Err("not a VTS IFO".into());
    }

    // Audio attributes: count byte at 0x203, up to 8 records of 8 bytes
    // starting at 0x204. A slot is a real stream only when its first bytes
    // are non-zero (menu-only VTSes list all-zero placeholders).
    let mut audio = Vec::new();
    let nr_audio = vts[0x203] as usize;
    for i in 0..nr_audio.min(8) {
        let a = &vts[0x204 + i * 8..0x204 + i * 8 + 8];
        if a[0] == 0 && a[1] == 0 {
            continue;
        }
        match clean_code(&a[2..4]) {
            Some(code) => audio.push(lang_name_2(&code)),
            None => audio.push("Unknown".into()),
        }
    }

    // Subpicture attributes: 2-byte count at 0x250, up to 32 records of
    // 6 bytes starting at 0x254.
    let mut subs = Vec::new();
    let nr_sub = u16::from_be_bytes([vts[0x250], vts[0x251]]) as usize;
    for i in 0..nr_sub.min(32) {
        let s = &vts[0x254 + i * 6..0x254 + i * 6 + 6];
        if s[0] == 0 && s[1] == 0 {
            continue;
        }
        match clean_code(&s[2..4]) {
            Some(code) => subs.push(lang_name_2(&code)),
            None => subs.push("Unknown".into()),
        }
    }

    Ok((audio, subs))
}

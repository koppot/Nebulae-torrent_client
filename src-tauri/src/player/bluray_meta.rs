//! Disc stream metadata via libbluray (bundled dylib or Homebrew install).
//!
//! VLC's Blu-ray demuxer labels audio/subtitle tracks from the CLIPINFO
//! files, which some discs author incorrectly (every stream "English"). The
//! playlist (MPLS) language entries are correct, and libbluray exposes them
//! — so the player bar uses libbluray to name tracks and to auto-pick the
//! owner's preferred languages.

use libloading::{Library, Symbol};
use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ulonglong, c_void};
use std::path::PathBuf;

// Raw libbluray structures (ABI of libbluray 1.x / dylib version 4). Fields
// mirror bluray.h; see the ctypes probe that validated this layout.
#[repr(C)]
struct BdStreamInfo {
    coding_type: c_uchar,
    format: c_uchar,
    rate: c_uchar,
    char_code: c_uchar,
    lang: [c_char; 4],
    pid: u16,
    aspect: c_uchar,
    subpath_id: c_uchar,
    dynamic_range_type: c_uchar,
    color_space: c_uchar,
    cr_flag: c_uchar,
    hdr_plus_flag: c_uchar,
}

#[repr(C)]
struct BdClipInfo {
    pkt_count: c_uint,
    still_mode: c_uchar,
    still_time: u16,
    video_stream_count: c_uchar,
    audio_stream_count: c_uchar,
    pg_stream_count: c_uchar,
    ig_stream_count: c_uchar,
    sec_audio_stream_count: c_uchar,
    sec_video_stream_count: c_uchar,
    dv_stream_count: c_uchar,
    video_streams: *mut BdStreamInfo,
    audio_streams: *mut BdStreamInfo,
    pg_streams: *mut BdStreamInfo,
    ig_streams: *mut BdStreamInfo,
    sec_audio_streams: *mut BdStreamInfo,
    sec_video_streams: *mut BdStreamInfo,
    dv_streams: *mut BdStreamInfo,
    start_time: c_ulonglong,
    in_time: c_ulonglong,
    out_time: c_ulonglong,
    clip_id: [c_char; 6],
}

#[repr(C)]
struct BdTitleInfo {
    idx: c_uint,
    playlist: c_uint,
    duration: c_ulonglong,
    clip_count: c_uint,
    angle_count: c_uchar,
    chapter_count: c_uint,
    mark_count: c_uint,
    clips: *mut BdClipInfo,
    // chapters/marks not needed; keep the pointers for ABI layout.
    chapters: *mut c_void,
    marks: *mut c_void,
    mvc_base_view_r_flag: c_uchar,
    sdr_conversion_notification_flag: c_uchar,
}

/// libbluray's BLURAY_TITLE_CHAP: one chapter of a playlist. `start` is
/// 90 kHz ticks from the title start.
#[repr(C)]
struct BdTitleChap {
    clip_ref: c_uint,
    start: u64,
    duration: u64,
}

/// Locate and open the libbluray dylib (bundled first, then Homebrew).
unsafe fn open_bluray_lib() -> Result<Library, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("../Frameworks/libbluray.4.dylib"));
            candidates.push(dir.join("libbluray.4.dylib"));
        }
    }
    candidates.push(PathBuf::from("/opt/homebrew/lib/libbluray.4.dylib"));
    candidates.push(PathBuf::from("/usr/local/lib/libbluray.4.dylib"));
    for path in &candidates {
        if path.exists() {
            if let Ok(lib) = Library::new(path) {
                return Ok(lib);
            }
        }
    }
    Err("libbluray dylib not found".into())
}

fn lang_name(code: &[c_char]) -> String {
    let s: String = code
        .iter()
        .filter(|c| **c != 0)
        .map(|c| *c as u8 as char)
        .collect();
    match s.as_str() {
        "eng" => "English",
        "jpn" => "Japanese",
        "und" => "Unknown",
        "fre" | "fra" => "French",
        "spa" => "Spanish",
        "deu" | "ger" => "German",
        "ita" => "Italian",
        "kor" => "Korean",
        "chi" | "zho" => "Chinese",
        "rus" => "Russian",
        "por" => "Portuguese",
        "dut" | "nld" => "Dutch",
        "swe" => "Swedish",
        "nor" => "Norwegian",
        "dan" => "Danish",
        "fin" => "Finnish",
        _ => return s,
    }
    .to_string()
}

/// Audio and subtitle language names for the title whose duration matches
/// `duration_ms` (the current libvlc title). Returns empty vectors when the
/// library is unavailable or no title matches within tolerance.
pub fn disc_stream_languages(
    disc_path: &str,
    duration_ms: i64,
) -> Result<(Vec<String>, Vec<String>), String> {
    unsafe {
        let lib = open_bluray_lib()?;
        let bd_open: Symbol<
            unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void,
        > = lib.get(b"bd_open").map_err(|e| e.to_string())?;
        let bd_get_titles: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int,
        > = lib.get(b"bd_get_titles").map_err(|e| e.to_string())?;
        let bd_get_title_info: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo,
        > = lib.get(b"bd_get_title_info").map_err(|e| e.to_string())?;
        let bd_free_title_info: Symbol<unsafe extern "C" fn(*mut BdTitleInfo)> =
            lib.get(b"bd_free_title_info").map_err(|e| e.to_string())?;
        let bd_close: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"bd_close").map_err(|e| e.to_string())?;

        let cpath = std::ffi::CString::new(disc_path).map_err(|e| e.to_string())?;
        let bd = bd_open(cpath.as_ptr(), std::ptr::null());
        if bd.is_null() {
            return Err("bd_open failed".into());
        }
        let count = bd_get_titles(bd, 0, 999);
        let mut audio: Vec<String> = Vec::new();
        let mut subs: Vec<String> = Vec::new();
        // Candidates: the title whose duration matches the playback
        // (duration_ms); for menu titles — which report a bogus/zero
        // duration — the title with the MOST audio streams (the feature;
        // menu and duplicate-audio playlists carry fewer or wrong ones).
        let mut best_match: Option<(i64, *mut BdTitleInfo)> = None;
        let mut most_audio: Option<(u8, *mut BdTitleInfo)> = None;
        for i in 0..count {
            let info = bd_get_title_info(bd, i as c_uint, 0);
            if info.is_null() {
                continue;
            }
            let ti = &*info;
            // Playlist duration is 90 kHz ticks → ms.
            let dur_ms = (ti.duration / 90) as i64;
            let diff = (dur_ms - duration_ms).abs();
            if best_match.map(|(b, _)| diff < b).unwrap_or(true) {
                best_match = Some((diff, info));
            }
            if !ti.clips.is_null() && ti.clip_count > 0 {
                // Score = distinct audio languages on the best clip: the
                // feature carries en+ja, while duplicate-audio and menu
                // playlists repeat one language.
                let mut best_clip = (0u8, 0usize); // (distinct, streams)
                for k in 0..ti.clip_count {
                    let c = &*ti.clips.add(k as usize);
                    let mut codes: Vec<String> = Vec::new();
                    for j in 0..c.audio_stream_count as usize {
                        let st = &*c.audio_streams.add(j);
                        codes.push(lang_name(&st.lang));
                    }
                    codes.sort();
                    codes.dedup();
                    let distinct = codes.len() as u32;
                    if distinct as u64 > best_clip.0 as u64
                        || (distinct as u64 == best_clip.0 as u64
                            && c.audio_stream_count as usize > best_clip.1)
                    {
                        best_clip = (distinct as u8, c.audio_stream_count as usize);
                    }
                }
                if most_audio
                    .map(|(m, _)| best_clip.0 > m || (best_clip.0 == m && best_clip.1 > 0))
                    .unwrap_or(true)
                {
                    most_audio = Some((best_clip.0, info));
                }
            }
        }
        let chosen: Option<*mut BdTitleInfo> = if duration_ms > 0 {
            best_match
                .filter(|(diff, _)| *diff <= 5000)
                .map(|(_, i)| i)
                .or_else(|| most_audio.map(|(_, i)| i))
        } else {
            most_audio.map(|(_, i)| i)
        };
        if let Some(info) = chosen {
                let ti = &*info;
                if !ti.clips.is_null() && ti.clip_count > 0 {
                    // Use the clip with the MOST audio streams (menus carry
                    // one; features carry all tracks).
                    let mut pick = &*ti.clips;
                    for k in 1..ti.clip_count {
                        let c = &*ti.clips.add(k as usize);
                        if c.audio_stream_count > pick.audio_stream_count {
                            pick = c;
                        }
                    }
                    for k in 0..pick.audio_stream_count as usize {
                        let s = &*pick.audio_streams.add(k);
                        audio.push(lang_name(&s.lang));
                    }
                    for k in 0..pick.pg_stream_count as usize {
                        let s = &*pick.pg_streams.add(k);
                        subs.push(lang_name(&s.lang));
                    }
                }
        bd_free_title_info(info);
        }
        bd_close(bd);
        Ok((audio, subs))
    }
}


/// Chapter start offsets (ms from title start) for the title whose
/// duration matches `duration_ms` — same title selection as
/// `disc_stream_languages`. VLC's own chapter API reports nothing for
/// Blu-ray titles, so the seek bar's chapter ticks come from here.
pub fn disc_chapter_offsets(disc_path: &str, duration_ms: i64) -> Result<Vec<i64>, String> {
    unsafe {
        let lib = open_bluray_lib()?;
        let bd_open: Symbol<
            unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void,
        > = lib.get(b"bd_open").map_err(|e| e.to_string())?;
        let bd_get_titles: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int,
        > = lib.get(b"bd_get_titles").map_err(|e| e.to_string())?;
        let bd_get_title_info: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo,
        > = lib.get(b"bd_get_title_info").map_err(|e| e.to_string())?;
        let bd_free_title_info: Symbol<unsafe extern "C" fn(*mut BdTitleInfo)> =
            lib.get(b"bd_free_title_info").map_err(|e| e.to_string())?;
        let bd_close: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"bd_close").map_err(|e| e.to_string())?;

        let cpath = std::ffi::CString::new(disc_path).map_err(|e| e.to_string())?;
        let bd = bd_open(cpath.as_ptr(), std::ptr::null());
        if bd.is_null() {
            return Err("bd_open failed".into());
        }
        let count = bd_get_titles(bd, 0, 999);
        let mut best_match: Option<(i64, *mut BdTitleInfo)> = None;
        let mut most_audio: Option<(u8, *mut BdTitleInfo)> = None;
        for i in 0..count {
            let info = bd_get_title_info(bd, i as c_uint, 0);
            if info.is_null() {
                continue;
            }
            let ti = &*info;
            // Playlist duration is 90 kHz ticks → ms.
            let dur_ms = (ti.duration / 90) as i64;
            let diff = (dur_ms - duration_ms).abs();
            if best_match.map(|(b, _)| diff < b).unwrap_or(true) {
                best_match = Some((diff, info));
            }
            if !ti.clips.is_null() && ti.clip_count > 0 {
                // Same feature-identification scoring as the language
                // probe: distinct audio languages on the best clip.
                let mut best_clip = (0u8, 0usize);
                for k in 0..ti.clip_count {
                    let c = &*ti.clips.add(k as usize);
                    let mut codes: Vec<String> = Vec::new();
                    for j in 0..c.audio_stream_count as usize {
                        let st = &*c.audio_streams.add(j);
                        codes.push(lang_name(&st.lang));
                    }
                    codes.sort();
                    codes.dedup();
                    let distinct = codes.len() as u32;
                    if distinct as u64 > best_clip.0 as u64
                        || (distinct as u64 == best_clip.0 as u64
                            && c.audio_stream_count as usize > best_clip.1)
                    {
                        best_clip = (distinct as u8, c.audio_stream_count as usize);
                    }
                }
                if most_audio
                    .map(|(m, _)| best_clip.0 > m || (best_clip.0 == m && best_clip.1 > 0))
                    .unwrap_or(true)
                {
                    most_audio = Some((best_clip.0, info));
                }
            } else {
                bd_free_title_info(info);
            }
        }
        let chosen: Option<*mut BdTitleInfo> = if duration_ms > 0 {
            best_match
                .filter(|(diff, _)| *diff <= 5000)
                .map(|(_, i)| i)
                .or_else(|| most_audio.map(|(_, i)| i))
        } else {
            most_audio.map(|(_, i)| i)
        };
        let mut offsets: Vec<i64> = Vec::new();
        if let Some(info) = chosen {
            let ti = &*info;
            if !ti.chapters.is_null() && ti.chapter_count > 0 {
                for k in 0..ti.chapter_count as usize {
                    let ch = &*(ti.chapters as *const BdTitleChap).add(k);
                    // 90 kHz ticks → ms.
                    offsets.push((ch.start / 90) as i64);
                }
                offsets.sort_unstable();
                offsets.dedup();
            }
            bd_free_title_info(info);
        }
        bd_close(bd);
        Ok(offsets)
    }
}

/// Debug variant used by the chap_probe example: prints the matched
/// title's playlist duration vs the chapter spread.
pub fn disc_chapter_offsets_debug(
    disc_path: &str,
    duration_ms: i64,
) -> Result<String, String> {
    let offsets = disc_chapter_offsets(disc_path, duration_ms)?;
    let last = offsets.last().copied().unwrap_or(0);
    Ok(format!(
        "matched dur={duration_ms} chapters={} first={} last={} spread={:.1}%",
        offsets.len(),
        offsets.first().copied().unwrap_or(0),
        last,
        if duration_ms > 0 { (last as f64 / duration_ms as f64) * 100.0 } else { 0.0 }
    ))
}

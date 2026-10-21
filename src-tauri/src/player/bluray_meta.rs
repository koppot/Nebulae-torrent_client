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

/// libbluray's BLURAY_TITLE_CHAPTER: one chapter of a playlist.
/// In libbluray C header (bluray.h lines 289-298):
///   uint32_t idx (offset 0)
///   [4 bytes padding for uint64_t alignment]
///   uint64_t start (offset 8, 90 kHz ticks)
///   uint64_t duration (offset 16)
///   uint64_t offset (offset 24, bytes from title start)
///   unsigned clip_ref (offset 32)
///   [4 bytes padding for pointer alignment]
///   const char *chapter_name (offset 40)
/// Total size = 48 bytes on 64-bit systems.
#[repr(C)]
struct BdTitleChap {
    idx: u32,
    _pad: u32,
    start: u64,
    duration: u64,
    offset: u64,
    clip_ref: c_uint,
    _pad2: u32,
    chapter_name: *const c_char,
}

static BLURAY_LIB: std::sync::OnceLock<Result<Library, String>> = std::sync::OnceLock::new();
static BD_PROBE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Locate and open the libbluray dylib (bundled first, then system fallback).
/// Candidates are checked in priority order: bundled resources ship inside
/// the app on all platforms; system paths are the last resort only.
/// Cached in a OnceLock so the dylib and its dependencies (fontconfig, freetype)
/// are NEVER dlclosed while worker threads or VLC playback are active.
unsafe fn open_bluray_lib() -> Result<&'static Library, String> {
    BLURAY_LIB
        .get_or_init(|| {
            let mut candidates: Vec<PathBuf> = Vec::new();

            // Env override — allows integration tests to point at a specific copy.
            if let Ok(p) = std::env::var("NEBULAE_LIBBLURAY_PATH") {
                candidates.push(PathBuf::from(p));
            }

            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    #[cfg(target_os = "macos")]
                    {
                        candidates.push(dir.join("../Frameworks/libbluray.4.dylib"));
                        candidates.push(dir.join("libbluray.4.dylib"));
                        // Bundled inside the app's VLC resource directory.
                        candidates.push(dir.join("vlc/lib/libbluray.4.dylib"));
                        if let Some(res) = dir.parent().map(|p| p.join("Resources")) {
                            candidates.push(res.join("resources/vlc/lib/libbluray.4.dylib"));
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        candidates.push(dir.join("resources/vlc/libbluray.dll"));
                        candidates.push(dir.join("vlc/libbluray.dll"));
                        candidates.push(dir.join("libbluray.dll"));
                    }
                    #[cfg(target_os = "linux")]
                    {
                        candidates.push(dir.join("resources/vlc/lib/libbluray.so.2"));
                        candidates.push(dir.join("vlc/lib/libbluray.so.2"));
                    }
                }
            }

            // System fallback paths (last resort — a missing bundled copy is
            // reported clearly by the empty bundled candidates failing first).
            #[cfg(target_os = "macos")]
            {
                candidates.push(PathBuf::from("/opt/homebrew/lib/libbluray.4.dylib"));
                candidates.push(PathBuf::from("/usr/local/lib/libbluray.4.dylib"));
            }
            #[cfg(target_os = "linux")]
            {
                candidates.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/libbluray.so.2"));
                candidates.push(PathBuf::from("/usr/lib/aarch64-linux-gnu/libbluray.so.2"));
                candidates.push(PathBuf::from("/usr/lib/libbluray.so.2"));
                candidates.push(PathBuf::from("/usr/lib/libbluray.so"));
            }

            for path in &candidates {
                if path.exists() {
                    if let Ok(lib) = Library::new(path) {
                        return Ok(lib);
                    }
                }
            }
            Err(format!(
                "libbluray not found. Checked: {}",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
        .as_ref()
        .map_err(|e| e.clone())
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
    let _probe_guard = BD_PROBE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let lib = open_bluray_lib()?;
        let bd_open: Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void> =
            lib.get(b"bd_open").map_err(|e| e.to_string())?;
        let bd_get_titles: Symbol<unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int> =
            lib.get(b"bd_get_titles").map_err(|e| e.to_string())?;
        let bd_get_title_info: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo,
        > = lib.get(b"bd_get_title_info").map_err(|e| e.to_string())?;
        let bd_free_title_info: Symbol<unsafe extern "C" fn(*mut BdTitleInfo)> =
            lib.get(b"bd_free_title_info").map_err(|e| e.to_string())?;
        let bd_close: Symbol<unsafe extern "C" fn(*mut c_void)> =
            lib.get(b"bd_close").map_err(|e| e.to_string())?;

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
        //
        // OWNERSHIP: every non-null pointer from bd_get_title_info must be
        // freed exactly once. We collect all allocated pointers and free
        // non-chosen ones after selection.
        let mut all_infos: Vec<*mut BdTitleInfo> = Vec::new();
        let mut best_match_idx: Option<(i64, usize)> = None; // (diff, index in all_infos)
        let mut most_audio_idx: Option<(u8, usize)> = None; // (distinct, index in all_infos)
        for i in 0..count {
            let info = bd_get_title_info(bd, i as c_uint, 0);
            if info.is_null() {
                continue;
            }
            let idx = all_infos.len();
            all_infos.push(info);
            let ti = &*info;
            // Playlist duration is 90 kHz ticks → ms.
            let dur_ms = (ti.duration / 90) as i64;
            let diff = (dur_ms - duration_ms).abs();
            if best_match_idx.map(|(b, _)| diff < b).unwrap_or(true) {
                best_match_idx = Some((diff, idx));
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
                if most_audio_idx
                    .map(|(m, _)| best_clip.0 > m || (best_clip.0 == m && best_clip.1 > 0))
                    .unwrap_or(true)
                {
                    most_audio_idx = Some((best_clip.0, idx));
                }
            }
        }
        let chosen_idx: Option<usize> = if duration_ms > 0 {
            best_match_idx
                .filter(|(diff, _)| *diff <= 30_000 || *diff <= (duration_ms / 50).max(5000))
                .map(|(_, i)| i)
                .or_else(|| most_audio_idx.map(|(_, i)| i))
        } else {
            most_audio_idx.map(|(_, i)| i)
        };
        if let Some(ci) = chosen_idx {
            let info = all_infos[ci];
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
        }
        // Free all allocated title infos. chosen_idx was used for reading
        // only; free it along with every other pointer.
        for info in all_infos {
            bd_free_title_info(info);
        }
        bd_close(bd);
        Ok((audio, subs))
    }
}

/// libbluray BLURAY_TITLE_CHAPTER element size in bytes.
/// libbluray >= 1.5.0 has `const char *chapter_name` at offset 40 (48 bytes total).
/// Older libbluray 1.x (< 1.5.0) omitted chapter_name (40 bytes total).
unsafe fn bluray_chapter_stride(lib: &Library, chapters_ptr: *const c_void, count: usize) -> usize {
    let mut stride = std::mem::size_of::<BdTitleChap>(); // 48 bytes
    if let Ok(sym) = lib
        .get::<Symbol<unsafe extern "C" fn(*mut c_int, *mut c_int, *mut c_int)>>(b"bd_get_version")
    {
        let mut major: c_int = 0;
        let mut minor: c_int = 0;
        let mut micro: c_int = 0;
        sym(&mut major, &mut minor, &mut micro);
        if major == 1 && minor < 5 {
            stride = 40;
        }
    }
    // Verify against chapter index of chapter 1 if at least 2 chapters exist
    if count >= 2 && !chapters_ptr.is_null() {
        let ptr_u8 = chapters_ptr as *const u8;
        let idx_at_48 = std::ptr::read_unaligned(ptr_u8.add(48) as *const u32);
        let idx_at_40 = std::ptr::read_unaligned(ptr_u8.add(40) as *const u32);
        if idx_at_48 == 1 && idx_at_40 != 1 {
            stride = 48;
        } else if idx_at_40 == 1 && idx_at_48 != 1 {
            stride = 40;
        }
    }
    stride
}

/// Chapter start offsets (ms from title start) and the matched playlist's
/// duration (ms) for the title whose duration matches `duration_ms` — same title
/// selection as `disc_stream_languages`. VLC's own chapter API reports nothing for
/// Blu-ray titles, so the seek bar's chapter ticks come from here.
pub fn disc_chapter_offsets(disc_path: &str, duration_ms: i64) -> Result<(Vec<i64>, i64), String> {
    let _probe_guard = BD_PROBE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let lib = open_bluray_lib()?;
        let bd_open: Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void> =
            lib.get(b"bd_open").map_err(|e| e.to_string())?;
        let bd_get_titles: Symbol<unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int> =
            lib.get(b"bd_get_titles").map_err(|e| e.to_string())?;
        let bd_get_title_info: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo,
        > = lib.get(b"bd_get_title_info").map_err(|e| e.to_string())?;
        let bd_free_title_info: Symbol<unsafe extern "C" fn(*mut BdTitleInfo)> =
            lib.get(b"bd_free_title_info").map_err(|e| e.to_string())?;
        let bd_close: Symbol<unsafe extern "C" fn(*mut c_void)> =
            lib.get(b"bd_close").map_err(|e| e.to_string())?;

        let cpath = std::ffi::CString::new(disc_path).map_err(|e| e.to_string())?;
        let bd = bd_open(cpath.as_ptr(), std::ptr::null());
        if bd.is_null() {
            return Err("bd_open failed".into());
        }
        let count = bd_get_titles(bd, 0, 999);
        // OWNERSHIP: every non-null pointer from bd_get_title_info must be
        // freed exactly once. Use index-based tracking; free all pointers
        // at the end, after reading from the chosen one.
        let mut all_infos: Vec<*mut BdTitleInfo> = Vec::new();
        let mut best_match_idx: Option<(i64, usize)> = None;
        let mut most_audio_idx: Option<(u8, usize)> = None;
        let mut longest_idx: Option<(i64, usize, usize)> = None; // (dur_ms, chapters, index)
        for i in 0..count {
            let info = bd_get_title_info(bd, i as c_uint, 0);
            if info.is_null() {
                continue;
            }
            let idx = all_infos.len();
            all_infos.push(info);
            let ti = &*info;
            // Playlist duration is 90 kHz ticks → ms.
            let dur_ms = (ti.duration / 90) as i64;
            let diff = (dur_ms - duration_ms).abs();
            if best_match_idx.map(|(b, _)| diff < b).unwrap_or(true) {
                best_match_idx = Some((diff, idx));
            }
            {
                let chapters = if ti.chapters.is_null() {
                    0usize
                } else {
                    ti.chapter_count as usize
                };
                if longest_idx
                    .map(|(d, c, _)| dur_ms > d || (dur_ms == d && chapters > c))
                    .unwrap_or(true)
                {
                    longest_idx = Some((dur_ms, chapters, idx));
                }
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
                if most_audio_idx
                    .map(|(m, _)| best_clip.0 > m || (best_clip.0 == m && best_clip.1 > 0))
                    .unwrap_or(true)
                {
                    most_audio_idx = Some((best_clip.0, idx));
                }
            }
        }
        let chosen_idx: Option<usize> = if duration_ms > 0 {
            best_match_idx
                .filter(|(diff, _)| *diff <= 30_000 || *diff <= (duration_ms / 50).max(5000))
                .map(|(_, i)| i)
        } else {
            most_audio_idx
                .map(|(_, i)| i)
                .or_else(|| longest_idx.map(|(_, _, i)| i))
        };
        let mut offsets: Vec<i64> = Vec::new();
        let mut matched_dur_ms: i64 = 0;
        if let Some(ci) = chosen_idx {
            let info = all_infos[ci];
            let ti = &*info;
            matched_dur_ms = (ti.duration / 90) as i64;
            if !ti.chapters.is_null() && ti.chapter_count > 0 {
                let stride = bluray_chapter_stride(lib, ti.chapters, ti.chapter_count as usize);
                let base_ptr = ti.chapters as *const u8;
                for k in 0..ti.chapter_count as usize {
                    let chap_ptr = base_ptr.add(k * stride);
                    let start_ticks = std::ptr::read_unaligned(chap_ptr.add(8) as *const u64);
                    // 90 kHz ticks → ms.
                    offsets.push((start_ticks / 90) as i64);
                }
                offsets.sort_unstable();
                offsets.dedup();
            }
        }
        // Free all allocated title infos (read phase complete).
        for info in all_infos {
            bd_free_title_info(info);
        }
        bd_close(bd);
        Ok((offsets, matched_dur_ms))
    }
}

/// Debug variant used by the chap_probe example: prints the matched
/// title's playlist duration vs the chapter spread.
pub fn disc_chapter_offsets_debug(disc_path: &str, duration_ms: i64) -> Result<String, String> {
    let (offsets, matched_dur) = disc_chapter_offsets(disc_path, duration_ms)?;
    let last = offsets.last().copied().unwrap_or(0);
    Ok(format!(
        "matched dur={duration_ms} (playlist dur={matched_dur}) chapters={} first={} last={} spread={:.1}%",
        offsets.len(),
        offsets.first().copied().unwrap_or(0),
        last,
        if matched_dur > 0 {
            (last as f64 / matched_dur as f64) * 100.0
        } else if duration_ms > 0 {
            (last as f64 / duration_ms as f64) * 100.0
        } else {
            0.0
        }
    ))
}

/// Debug dump: every playlist with duration (ms), clip and chapter counts.
pub fn disc_title_dump(disc_path: &str) -> Result<String, String> {
    let _probe_guard = BD_PROBE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let lib = open_bluray_lib()?;
        let bd_open: Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void> =
            lib.get(b"bd_open").map_err(|e| e.to_string())?;
        let bd_get_titles: Symbol<unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int> =
            lib.get(b"bd_get_titles").map_err(|e| e.to_string())?;
        let bd_get_title_info: Symbol<
            unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo,
        > = lib.get(b"bd_get_title_info").map_err(|e| e.to_string())?;
        let bd_free_title_info: Symbol<unsafe extern "C" fn(*mut BdTitleInfo)> =
            lib.get(b"bd_free_title_info").map_err(|e| e.to_string())?;
        let bd_close: Symbol<unsafe extern "C" fn(*mut c_void)> =
            lib.get(b"bd_close").map_err(|e| e.to_string())?;
        let cpath = std::ffi::CString::new(disc_path).map_err(|e| e.to_string())?;
        let bd = bd_open(cpath.as_ptr(), std::ptr::null());
        if bd.is_null() {
            return Err("bd_open failed".into());
        }
        let count = bd_get_titles(bd, 0, 999);
        let mut out = String::new();
        for i in 0..count {
            let info = bd_get_title_info(bd, i as c_uint, 0);
            if info.is_null() {
                continue;
            }
            let ti = &*info;
            let dur_ms = (ti.duration / 90) as i64;
            out.push_str(&format!(
                "title {} dur_ms={} clips={} chapters={}\n",
                ti.idx, dur_ms, ti.clip_count, ti.chapter_count
            ));
            bd_free_title_info(info);
        }
        bd_close(bd);
        Ok(out)
    }
}

/// Inspect the disc's titles and clips via libbluray to determine whether
/// the disc is an Ultra HD Blu-ray or contains HDR10 / BT.2020 streams.
/// Returns true if any clip has format == 7 (2160p UHD), color_space == 2 (BT.2020),
/// or dynamic_range_type > 0 (HDR10/Dolby Vision/Philips HDR).
pub fn is_disc_uhd_or_hdr(disc_path: &str) -> bool {
    let _probe_guard = BD_PROBE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let Ok(lib) = open_bluray_lib() else {
            return false;
        };
        let Ok(bd_open): Result<Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void>, _> =
            lib.get(b"bd_open") else { return false; };
        let Ok(bd_get_titles): Result<Symbol<unsafe extern "C" fn(*mut c_void, c_uchar, c_uint) -> c_int>, _> =
            lib.get(b"bd_get_titles") else { return false; };
        let Ok(bd_get_title_info): Result<Symbol<unsafe extern "C" fn(*mut c_void, c_uint, c_uint) -> *mut BdTitleInfo>, _> =
            lib.get(b"bd_get_title_info") else { return false; };
        let Ok(bd_free_title_info): Result<Symbol<unsafe extern "C" fn(*mut BdTitleInfo)>, _> =
            lib.get(b"bd_free_title_info") else { return false; };
        let Ok(bd_close): Result<Symbol<unsafe extern "C" fn(*mut c_void)>, _> =
            lib.get(b"bd_close") else { return false; };

        let Ok(cpath) = std::ffi::CString::new(disc_path) else { return false; };
        let bd = bd_open(cpath.as_ptr(), std::ptr::null());
        if bd.is_null() {
            return false;
        }
        let count = bd_get_titles(bd, 0, 999);
        let mut is_uhd = false;
        for i in 0..count.min(25) as c_uint {
            let info = bd_get_title_info(bd, i, 0);
            if info.is_null() {
                continue;
            }
            let ti = &*info;
            if !ti.clips.is_null() && ti.clip_count > 0 {
                for k in 0..ti.clip_count as usize {
                    let c = &*ti.clips.add(k);
                    for v in 0..c.video_stream_count as usize {
                        let vs = &*c.video_streams.add(v);
                        if vs.format == 7 || vs.color_space == 2 || vs.dynamic_range_type > 0 {
                            is_uhd = true;
                            break;
                        }
                    }
                    if is_uhd {
                        break;
                    }
                }
            }
            bd_free_title_info(info);
            if is_uhd {
                break;
            }
        }
        bd_close(bd);
        is_uhd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluray_91_days_metadata_probe() {
        let disc = "/Volumes/300TB/Blu-ray/91 DAYS US/91 DAYS D1";
        if !std::path::Path::new(disc).exists() {
            println!("Disc not mounted, skipping");
            return;
        }

        for i in 0..5 {
            let (audio, subs) =
                disc_stream_languages(disc, 1454000).expect("stream languages probe");
            assert!(!audio.is_empty(), "Audio tracks must be discovered");

            let (chapters, dur) =
                disc_chapter_offsets(disc, 1454000).expect("chapter offsets probe");
            assert!(!chapters.is_empty(), "Chapters must be discovered");
            assert!(
                chapters.len() >= 5,
                "Episode must have at least 5 chapters, found {}",
                chapters.len()
            );
            let last = chapters.last().copied().unwrap_or(0);
            assert!(
                last > 1_400_000,
                "Last chapter must start near episode end, found {last} ms for {dur} ms"
            );
            println!(
                "Iteration {i}: audio={audio:?} subs={subs:?} chapters={} dur={dur} last_chap={last}",
                chapters.len()
            );
        }
    }

    #[test]
    fn test_is_disc_uhd_or_hdr() {
        let patlabor = "/Volumes/300TB/UHD/Patlabor 2 the Movie (2002) UHD/PATLABOR2_THE_MOVIE_4K_UHD";
        if std::path::Path::new(patlabor).exists() {
            assert!(is_disc_uhd_or_hdr(patlabor), "Patlabor 2 must be detected as UHD/HDR");
        }
        let fuuka = "/Volumes/300TB/Blu-ray/Fuuka (2017) Funimation/FUUKA_CS_D1";
        if std::path::Path::new(fuuka).exists() {
            assert!(!is_disc_uhd_or_hdr(fuuka), "Fuuka D1 must NOT be detected as UHD/HDR");
        }
    }
}

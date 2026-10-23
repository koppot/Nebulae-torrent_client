//! Runtime loader for the system libVLC and a minimal VLC 3.x FFI.
//!
//! Only the handful of symbols playback needs are bound. Everything is
//! dynamically resolved from the user's own VLC installation, so the exact
//! dylib path differs per platform.

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::raw::c_uint;
use std::path::{Path, PathBuf};

/// The loaded libVLC and the symbols we use.
pub struct VlcLib {
    // The Library must outlive the symbols resolved from it; it is never
    // read directly, only held open.
    #[allow(dead_code)]
    lib: Library,
    // libvlc.dylib/.so/.dll references libvlccore via @rpath (macOS) or a
    // bare soname; loading it out of VLC.app fails unless libvlccore is
    // preloaded by absolute path FIRST. These handles must also outlive
    // `lib`.
    #[allow(dead_code)]
    deps: Vec<Library>,
    pub libvlc_new: unsafe extern "C" fn(c_int, *const *const c_char) -> *mut c_void,
    pub libvlc_release: unsafe extern "C" fn(*mut c_void),
    pub libvlc_errmsg: unsafe extern "C" fn() -> *const c_char,
    pub libvlc_get_version: unsafe extern "C" fn() -> *const c_char,
    pub libvlc_media_new_path: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    // MRL-based media (bluray://, dvd://) — folder-based disc images.
    pub libvlc_media_new_location: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub libvlc_media_release: unsafe extern "C" fn(*mut c_void),
    // libvlccore internal, exported by the bundled dylib: sets a variable on
    // a VLC object directly. Needed because libvlc_media_player_set_nsobject()
    // resets "vout" (and "avcodec-hw", "window") to empty strings on the
    // media player object (upstream 3.0.x lib/media_player.c), which shadows
    // any --vout passed to libvlc_new for every vout under that player.
    pub var_set_checked: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, usize) -> c_int,
    pub libvlc_media_player_new_from_media: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub libvlc_media_player_release: unsafe extern "C" fn(*mut c_void),
    pub libvlc_media_player_set_nsobject: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub libvlc_media_player_set_hwnd: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub libvlc_media_player_set_xwindow: unsafe extern "C" fn(*mut c_void, u32),
    pub libvlc_media_player_play: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_stop: unsafe extern "C" fn(*mut c_void),
    pub libvlc_media_player_pause: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_media_player_is_playing: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_get_time: unsafe extern "C" fn(*mut c_void) -> i64,
    pub libvlc_media_player_set_time: unsafe extern "C" fn(*mut c_void, i64),
    pub libvlc_media_player_set_position: unsafe extern "C" fn(*mut c_void, f32),
    pub libvlc_media_player_get_length: unsafe extern "C" fn(*mut c_void) -> i64,
    // DVD/Blu-ray navigation: titles map to disc features, title 0 is the
    // menu domain on DVD (dvdnav); chapters are per-title.
    pub libvlc_media_player_get_title_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_get_title: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_set_title: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_media_player_get_chapter_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_get_chapter: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_media_player_set_chapter: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_audio_set_volume: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    pub libvlc_video_set_deinterlace: unsafe extern "C" fn(*mut c_void, *const c_char),
    // DVD/BD menu interaction. Mouse events do NOT reach dvdnav when the
    // vout is embedded into an external NSView (verified empirically: 10
    // synthesized clicks over 35 s never activated a menu button, while
    // libvlc_media_player_navigate(activate) switched title 0 -> 1 on the
    // first call). This API is therefore the ONLY reliable menu input path.
    pub libvlc_media_player_navigate: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_media_player_navigate_menu: Option<unsafe extern "C" fn(*mut c_void)>,
    // Full title/chapter descriptions: real chapter time offsets for the
    // seek bar marks, and duration-aware title navigation (features only).
    pub libvlc_media_player_get_full_title_descriptions:
        unsafe extern "C" fn(*mut c_void, *mut *mut *mut VlcTitleDescription) -> c_int,
    pub libvlc_title_descriptions_release:
        unsafe extern "C" fn(*mut *mut VlcTitleDescription, c_int),
    pub libvlc_media_player_get_full_chapter_descriptions:
        unsafe extern "C" fn(*mut c_void, c_int, *mut *mut *mut VlcChapterDescription) -> c_int,
    pub libvlc_chapter_descriptions_release:
        unsafe extern "C" fn(*mut *mut VlcChapterDescription, c_int),
    // Subtitle / audio track enumeration and selection (track popovers).
    pub libvlc_video_get_spu_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_video_get_spu: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_video_set_spu: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_video_get_spu_description:
        unsafe extern "C" fn(*mut c_void) -> *mut VlcTrackDescription,
    pub libvlc_audio_get_track_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_audio_get_track: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub libvlc_audio_set_track: unsafe extern "C" fn(*mut c_void, c_int),
    pub libvlc_audio_get_track_description:
        unsafe extern "C" fn(*mut c_void) -> *mut VlcTrackDescription,
    pub libvlc_track_description_list_release: unsafe extern "C" fn(*mut VlcTrackDescription),
    pub libvlc_video_get_size: Option<
        unsafe extern "C" fn(
            *mut c_void,
            std::ffi::c_uint,
            *mut std::ffi::c_uint,
            *mut std::ffi::c_uint,
        ) -> c_int,
    >,
    /// Take a snapshot of the current video frame and save it to a file.
    /// If i_width/i_height are 0, uses the video's native size. Returns 0 on success.
    pub libvlc_video_take_snapshot: Option<
        unsafe extern "C" fn(
            *mut c_void,
            std::ffi::c_uint,
            std::ffi::c_uint,
            *const c_char,
        ) -> c_uint,
    >,
}

/// Mirror of libvlc_title_description_t (C layout, header-verified).
#[repr(C)]
pub struct VlcTitleDescription {
    pub i_duration: i64,
    pub psz_name: *mut c_char,
    pub i_flags: u32,
}

/// Title flags per libvlc: INPUT_TITLE_MENU=0x01, INPUT_TITLE_INTERACTIVE=0x02
/// From upstream include/vlc_input.h (3.0.x)
pub const INPUT_TITLE_MENU: u32 = 0x01;
pub const INPUT_TITLE_INTERACTIVE: u32 = 0x02;

/// Return true if the current title is the disc's authored TOP MENU.
///
/// Only INPUT_TITLE_MENU (0x01) qualifies. INPUT_TITLE_INTERACTIVE (0x02)
/// must NOT be treated as "at menu": VLC's bluray demux (3.0.23
/// modules/access/bluray.c, blurayInitTitles) flags EVERY HDMV title that
/// has a movie object as INTERACTIVE — including First Play, the intro
/// chain, and feature titles — so a 0x02-based test reports "at menu"
/// from the first poll on a pure-HDMV disc (observed on Patlabor 2 UHD:
/// at_menu was true during the studio logos). dvdnav (modules/access/
/// dvdnav.c) likewise marks only its menu title with INPUT_TITLE_MENU.
///
/// Title indices are validated defensively: libvlc_media_player_get_title
/// returns -1 while the demuxer is still starting.
///
/// # Safety
/// Caller must ensure `mp` is a valid pointer to a live libVLC media player.
pub unsafe fn is_menu_title(lib: &VlcLib, mp: usize) -> bool {
    let m = mp as *mut c_void;
    let count = (lib.libvlc_media_player_get_title_count)(m);
    if count <= 0 {
        return false;
    }
    let current = (lib.libvlc_media_player_get_title)(m);
    // On optical discs (DVD, Blu-ray, UHD), title 0 is by definition the authored
    // menu domain (VMGM on DVD, Top Menu on Blu-ray/UHD). If current == 0, we are at the menu.
    if current == 0 {
        return true;
    }
    let mut titles: *mut *mut VlcTitleDescription = std::ptr::null_mut();
    // libvlc_media_player_get_full_title_descriptions fills titles pointer
    // and returns the entry count (NOT NULL-terminated; <= 0 on none/error).
    let n = (lib.libvlc_media_player_get_full_title_descriptions)(m, &mut titles);
    if n <= 0 || titles.is_null() {
        return false;
    }
    if current < 0 || current >= n {
        (lib.libvlc_title_descriptions_release)(titles, n);
        return false;
    }
    let t = &**titles.add(current as usize);
    let is_named_menu = if !t.psz_name.is_null() {
        let cstr = std::ffi::CStr::from_ptr(t.psz_name);
        cstr.to_string_lossy().to_ascii_lowercase().contains("menu")
    } else {
        false
    };
    let is_menu = (t.i_flags & INPUT_TITLE_MENU) != 0 || is_named_menu;
    (lib.libvlc_title_descriptions_release)(titles, n);
    is_menu
}

/// Return the 0-based index of the first title marked with INPUT_TITLE_MENU, if any.
///
/// # Safety
/// Caller must ensure `mp` is a valid pointer to a live libVLC media player.
pub unsafe fn find_menu_title(lib: &VlcLib, mp: usize) -> Option<i32> {
    let m = mp as *mut c_void;
    let mut titles: *mut *mut VlcTitleDescription = std::ptr::null_mut();
    let n = (lib.libvlc_media_player_get_full_title_descriptions)(m, &mut titles);
    if n <= 0 || titles.is_null() {
        return None;
    }
    let mut found = None;
    for i in 0..n {
        let t = &**titles.add(i as usize);
        if (t.i_flags & INPUT_TITLE_MENU) != 0 {
            found = Some(i);
            break;
        }
    }
    (lib.libvlc_title_descriptions_release)(titles, n);
    found
}

/// Return true if the disc exposes at least one authored menu title.
///
/// # Safety
/// Caller must ensure `mp` is a valid pointer to a live libVLC media player.
pub unsafe fn has_menu_title(lib: &VlcLib, mp: usize) -> bool {
    find_menu_title(lib, mp).is_some()
}

/// Return true if the currently active title is First Play or a short intro clip (<= 300s).
///
/// # Safety
/// Caller must ensure `mp` is a valid pointer to a live libVLC media player.
pub unsafe fn is_first_play_or_intro(lib: &VlcLib, mp: usize) -> bool {
    let m = mp as *mut c_void;
    let mut titles: *mut *mut VlcTitleDescription = std::ptr::null_mut();
    let n = (lib.libvlc_media_player_get_full_title_descriptions)(m, &mut titles);
    if n <= 0 || titles.is_null() {
        return false;
    }
    let current = (lib.libvlc_media_player_get_title)(m);
    if current < 0 || current >= n {
        (lib.libvlc_title_descriptions_release)(titles, n);
        return false;
    }
    let t = &**titles.add(current as usize);
    let dur_s = t.i_duration / 1000;
    let is_short = dur_s <= 300;
    let is_named_fp = if !t.psz_name.is_null() {
        let cstr = std::ffi::CStr::from_ptr(t.psz_name);
        cstr.to_string_lossy()
            .to_ascii_lowercase()
            .contains("first play")
    } else {
        false
    };
    (lib.libvlc_title_descriptions_release)(titles, n);
    is_named_fp || is_short
}

/// Mirror of libvlc_chapter_description_t (C layout, header-verified).
#[repr(C)]
pub struct VlcChapterDescription {
    pub i_time_offset: i64,
    pub i_duration: i64,
    pub psz_name: *mut c_char,
}

/// Mirror of libvlc_track_description_t (singly linked list).
#[repr(C)]
pub struct VlcTrackDescription {
    pub i_id: c_int,
    pub psz_name: *mut c_char,
    pub p_next: *mut VlcTrackDescription,
}

// The library handle is process-global and the symbols are plain functions;
// sharing across threads is exactly what libVLC is designed for.
unsafe impl Send for VlcLib {}
unsafe impl Sync for VlcLib {}

/// Percent-encode a filesystem path for use in an MRL: RFC 3986 unreserved
/// characters and path separators stay literal, everything else is escaped
/// (spaces as %20, parentheses, commas, etc. as %XX).
pub fn encode_mrl_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            other => {
                let hex = format!("%{other:02X}");
                out.push_str(&hex);
            }
        }
    }
    out
}

/// Normalizes Windows filesystem paths into a libVLC disc MRL (e.g. bluray:///V:/... or dvd:///C:/...).
/// On Windows, libVLC's disc demuxers (libbluray, dvdread) require:
/// - Three leading slashes after scheme (`bluray:///`)
/// - Forward slashes instead of backslashes
/// - Literal drive letter (e.g. `V:`, not percent-encoded `%3A`)
/// - Percent-encoded spaces and special characters (`%20`, etc.), preserving `/` and `:`.
pub fn format_disc_mrl_windows(scheme: &str, path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = String::with_capacity(normalized.len() + 16);
    let mut i = 0;
    // Preserve literal drive letter if present: e.g. "V:/" or "C:/"
    if chars.len() >= 2 && chars[0].is_ascii_alphabetic() && chars[1] == ':' {
        out.push(chars[0]);
        out.push(':');
        i = 2;
        if i < chars.len() && chars[i] == '/' {
            out.push('/');
            i += 1;
        }
    }
    for &c in &chars[i..] {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '/' | '-' | '.' | '_' | '~' => {
                out.push(c);
            }
            other => {
                let mut buf = [0u8; 4];
                let encoded = other.encode_utf8(&mut buf);
                for b in encoded.as_bytes() {
                    use std::fmt::Write;
                    let _ = write!(out, "%{:02X}", b);
                }
            }
        }
    }
    format!("{scheme}:///{out}")
}

/// Format a disc filesystem path (folder or image) into a platform-correct libVLC disc MRL.
pub fn format_disc_mrl(scheme: &str, path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format_disc_mrl_windows(scheme, path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{scheme}://{}", encode_mrl_path(path))
    }
}

type VlcLogEmit = extern "C" fn(c_int, *const c_char);

/// Load the diagnostic log shim and attach it to `instance`.
unsafe fn install_vlc_log_shim(shim_path: &std::path::Path, instance: *mut c_void) {
    match Library::new(shim_path) {
        Ok(lib) => {
            let install: Symbol<unsafe extern "C" fn(*mut c_void, VlcLogEmit)> =
                lib.get(b"install_vlc_logger").expect("shim symbol");
            install(instance, vlc_log_emit);
            // The shim must outlive every VLC instance.
            std::mem::forget(lib);
        }
        Err(e) => eprintln!("vlc log shim load failed: {e}"),
    }
}

extern "C" fn vlc_log_emit(level: c_int, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let msg = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/nebulae-vlc-cb.log")
    {
        use std::io::Write;
        let _ = writeln!(f, "[{level}] {msg}");
    }
}

impl VlcLib {
    /// Locate and load the bundled libVLC.
    pub fn load() -> Result<VlcLib, String> {
        let candidates = Self::candidate_paths();
        let mut failures: Vec<String> = Vec::new();
        for path in &candidates {
            if !path.exists() {
                continue;
            }
            // Add library directory to PATH so dynamic loaders (especially Windows
            // LoadLibrary for libvlccore.dll and plugin dependencies) resolve siblings.
            if let Some(dir) = path.parent() {
                if let Ok(curr_path) = std::env::var("PATH") {
                    let mut paths = std::env::split_paths(&curr_path).collect::<Vec<_>>();
                    if !paths.iter().any(|p| p == dir) {
                        paths.insert(0, dir.to_path_buf());
                        if let Ok(new_path) = std::env::join_paths(paths) {
                            std::env::set_var("PATH", new_path);
                        }
                    }
                }
            }
            // Preload libvlccore by absolute path: libvlc references it via
            // @rpath/soname or PE import table, which does not resolve from a
            // foreign process without preloading or directory registration.
            let deps = Self::preload_core_deps(path, &mut failures);
            unsafe {
                match Library::new(path) {
                    Ok(lib) => {
                        // Point VLC to its module plugins directory relative to itself.
                        // Setting VLC_PLUGIN_PATH must ONLY happen for the successfully
                        // loaded library so that failed candidates do not corrupt the path.
                        if let Some(plugins) = Self::plugin_dir_for(path) {
                            std::env::set_var("VLC_PLUGIN_PATH", &plugins);
                        }
                        if let Ok(mut p) = LOADED_PATH.lock() {
                            *p = Some(path.clone());
                        }
                        return Ok(Self::bind(lib, path, deps));
                    }
                    Err(e) => failures.push(format!("{}: {e}", path.display())),
                }
            }
        }
        Err(format!(
            "Nebulae's bundled playback engine (libvlc) was not found or failed to load. Attempts: {}",
            failures.join(" | ")
        ))
    }

    /// Preload VLC's support dylibs by absolute path. Disc plugins (bluray,
    /// dvdnav) link against @rpath dylibs like libbluray.2.dylib which cannot
    /// resolve from a foreign process; preloading registers each image under
    /// its install name so plugin dlopen calls match the already-loaded
    /// libraries. Failures are reported but the main load is still attempted.
    fn preload_core_deps(lib_path: &Path, failures: &mut Vec<String>) -> Vec<Library> {
        let Some(dir) = lib_path.parent() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // All shared libraries in the lib dir (skip libvlc itself, loaded separately).
        // Matches .dylib (macOS), .so (Linux), and .dll (Windows).
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let dep = entry.path();
                let is_dep = matches!(
                    dep.extension().and_then(|e| e.to_str()),
                    Some("dylib") | Some("so") | Some("dll")
                );
                let is_libvlc = dep
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("libvlc."))
                    .unwrap_or(false);
                if is_dep && !is_libvlc {
                    unsafe {
                        match Library::new(&dep) {
                            Ok(l) => out.push(l),
                            Err(e) => failures.push(format!("preload {}: {e}", dep.display())),
                        }
                    }
                }
            }
        }
        // Frameworks dir may hold additional dependencies (e.g. libbluray).
        if let Some(fw) = dir.parent().map(|p| p.join("Frameworks")) {
            if let Ok(entries) = std::fs::read_dir(&fw) {
                for entry in entries.flatten() {
                    let dep = entry.path();
                    if dep.extension().and_then(|e| e.to_str()) == Some("dylib") {
                        unsafe {
                            match Library::new(&dep) {
                                Ok(l) => out.push(l),
                                Err(e) => failures.push(format!("preload {}: {e}", dep.display())),
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn candidate_paths() -> Vec<PathBuf> {
        let mut c = Vec::new();
        if let Ok(p) = std::env::var("NEBULAE_LIBVLC_PATH") {
            c.push(PathBuf::from(p));
        }
        #[cfg(target_os = "macos")]
        {
            // Bundled libVLC ships INSIDE the app (strictly self-contained;
            // never depend on an external VLC install):
            // <bundle>/Contents/Resources/resources/vlc/lib/libvlc.dylib
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    c.push(dir.join("vlc").join("lib").join("libvlc.dylib"));
                    c.push(
                        dir.join("resources")
                            .join("vlc")
                            .join("lib")
                            .join("libvlc.dylib"),
                    );
                    if let Some(res) = dir.parent().map(|p| p.join("Resources")) {
                        c.push(
                            res.join("resources")
                                .join("vlc")
                                .join("lib")
                                .join("libvlc.dylib"),
                        );
                    }
                }
            }
            // Development / local fallback paths relative to working directory:
            c.push(PathBuf::from("src-tauri/resources/vlc/lib/libvlc.dylib"));
            c.push(PathBuf::from("resources/vlc/lib/libvlc.dylib"));
        }
        #[cfg(target_os = "windows")]
        {
            // Bundled libVLC ships INSIDE the app: never depend on a system VLC install.
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    c.push(dir.join("resources").join("vlc").join("libvlc.dll"));
                    c.push(
                        dir.join("resources")
                            .join("vlc")
                            .join("lib")
                            .join("libvlc.dll"),
                    );
                    c.push(dir.join("vlc").join("libvlc.dll"));
                }
                if let Some(parent) = exe.parent().and_then(|d| d.parent()) {
                    c.push(parent.join("resources").join("vlc").join("libvlc.dll"));
                    c.push(
                        parent
                            .join("resources")
                            .join("vlc")
                            .join("lib")
                            .join("libvlc.dll"),
                    );
                }
            }
            c.push(PathBuf::from("src-tauri/resources/vlc/libvlc.dll"));
            c.push(PathBuf::from("resources/vlc/libvlc.dll"));
            c.push(PathBuf::from("src-tauri/resources/vlc/lib/libvlc.dll"));
            c.push(PathBuf::from("resources/vlc/lib/libvlc.dll"));
            // Fallbacks if bundled library is damaged or missing
            c.push(PathBuf::from(
                "C:\\Program Files\\VideoLAN\\VLC\\libvlc.dll",
            ));
            c.push(PathBuf::from(
                "C:\\Program Files (x86)\\VideoLAN\\VLC\\libvlc.dll",
            ));
        }
        #[cfg(target_os = "linux")]
        {
            // Bundled libVLC ships INSIDE the app (never depend on a system
            // VLC install).
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    c.push(
                        dir.join("resources")
                            .join("vlc")
                            .join("lib")
                            .join("libvlc.so.5"),
                    );
                    c.push(dir.join("vlc").join("lib").join("libvlc.so.5"));
                }
                if let Some(prefix) = exe.parent() {
                    c.push(prefix.join("lib/Nebulae/resources/vlc/lib/libvlc.so.5"));
                    c.push(prefix.join("lib/Nebulae/resources/vlc/lib/libvlc.so"));
                    c.push(prefix.join("lib/nebulae/resources/vlc/lib/libvlc.so.5"));
                    c.push(prefix.join("share/nebulae/resources/vlc/lib/libvlc.so.5"));
                }
            }
            if let Ok(appdir) = std::env::var("APPDIR") {
                c.push(
                    PathBuf::from(&appdir).join("usr/lib/Nebulae/resources/vlc/lib/libvlc.so.5"),
                );
                c.push(
                    PathBuf::from(&appdir).join("usr/share/nebulae/resources/vlc/lib/libvlc.so.5"),
                );
            }
            c.push(PathBuf::from(
                "/usr/lib/Nebulae/resources/vlc/lib/libvlc.so.5",
            ));
            c.push(PathBuf::from(
                "/usr/lib/Nebulae/resources/vlc/lib/libvlc.so",
            ));
            c.push(PathBuf::from("src-tauri/resources/vlc/lib/libvlc.so.5"));
            c.push(PathBuf::from("resources/vlc/lib/libvlc.so.5"));
        }
        c
    }

    fn plugin_dir_for(lib_path: &Path) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            // .../resources/vlc/lib/libvlc.dylib -> .../resources/vlc/plugins
            if let Some(dir) = lib_path
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins"))
            {
                if dir.exists() {
                    return Some(dir);
                }
            }
            if let Some(dir) = lib_path.parent().map(|p| p.join("plugins")) {
                if dir.exists() {
                    return Some(dir);
                }
            }
            None
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Windows: resources/vlc/libvlc.dll -> resources/vlc/plugins (parent)
            // Linux: resources/vlc/lib/libvlc.so.5 -> resources/vlc/plugins (parent.parent)
            lib_path
                .parent()
                .map(|p| p.join("plugins"))
                .filter(|p| p.exists())
                .or_else(|| {
                    lib_path
                        .parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.join("plugins"))
                        .filter(|p| p.exists())
                })
        }
    }

    unsafe fn bind(lib: Library, _path: &Path, deps: Vec<Library>) -> VlcLib {
        macro_rules! sym {
            ($name:literal : $ty:ty) => {{
                let s: Symbol<$ty> = lib
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .unwrap_or_else(|e| panic!("libVLC symbol {} missing: {}", $name, e));
                *s
            }};
        }
        macro_rules! sym_opt {
            ($name:literal : $ty:ty) => {{
                let s: Option<Symbol<$ty>> = lib.get::<$ty>(concat!($name, "\0").as_bytes()).ok();
                s.map(|s| *s)
            }};
        }
        VlcLib {
            libvlc_new: sym!("libvlc_new": unsafe extern "C" fn(c_int, *const *const c_char) -> *mut c_void),
            libvlc_release: sym!("libvlc_release": unsafe extern "C" fn(*mut c_void)),
            libvlc_errmsg: sym!("libvlc_errmsg": unsafe extern "C" fn() -> *const c_char),
            libvlc_get_version: sym!("libvlc_get_version": unsafe extern "C" fn() -> *const c_char),
            libvlc_media_new_path: sym!("libvlc_media_new_path": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_new_location: sym!("libvlc_media_new_location": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_release: sym!("libvlc_media_release": unsafe extern "C" fn(*mut c_void)),
            // var_SetChecked lives in libvlccore, not libvlc; look it up in
            // the preloaded dependency libraries. libvlc.5 keeps libvlccore
            // mapped, so the pointer stays valid for the process lifetime.
            var_set_checked: {
                type VscFn =
                    unsafe extern "C" fn(*mut c_void, *const c_char, c_int, usize) -> c_int;
                let mut found: Option<Symbol<VscFn>> = None;
                for dep in &deps {
                    if let Ok(s) = dep.get::<VscFn>(b"var_SetChecked\0") {
                        found = Some(s);
                        break;
                    }
                }
                *found.unwrap_or_else(|| panic!("libvlccore symbol var_SetChecked missing"))
            },
            libvlc_media_player_new_from_media: sym!("libvlc_media_player_new_from_media": unsafe extern "C" fn(*mut c_void) -> *mut c_void),
            libvlc_media_player_release: sym!("libvlc_media_player_release": unsafe extern "C" fn(*mut c_void)),
            libvlc_media_player_set_nsobject: sym!("libvlc_media_player_set_nsobject": unsafe extern "C" fn(*mut c_void, *mut c_void)),
            libvlc_media_player_set_hwnd: sym!("libvlc_media_player_set_hwnd": unsafe extern "C" fn(*mut c_void, *mut c_void)),
            libvlc_media_player_set_xwindow: sym!("libvlc_media_player_set_xwindow": unsafe extern "C" fn(*mut c_void, u32)),
            libvlc_media_player_play: sym!("libvlc_media_player_play": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_stop: sym!("libvlc_media_player_stop": unsafe extern "C" fn(*mut c_void)),
            libvlc_media_player_pause: sym!("libvlc_media_player_pause": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_media_player_is_playing: sym!("libvlc_media_player_is_playing": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_get_time: sym!("libvlc_media_player_get_time": unsafe extern "C" fn(*mut c_void) -> i64),
            libvlc_media_player_set_time: sym!("libvlc_media_player_set_time": unsafe extern "C" fn(*mut c_void, i64)),
            libvlc_media_player_set_position: sym!("libvlc_media_player_set_position": unsafe extern "C" fn(*mut c_void, f32)),
            libvlc_media_player_get_length: sym!("libvlc_media_player_get_length": unsafe extern "C" fn(*mut c_void) -> i64),
            libvlc_media_player_get_title_count: sym!("libvlc_media_player_get_title_count": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_get_title: sym!("libvlc_media_player_get_title": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_set_title: sym!("libvlc_media_player_set_title": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_media_player_get_chapter_count: sym!("libvlc_media_player_get_chapter_count": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_get_chapter: sym!("libvlc_media_player_get_chapter": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_media_player_set_chapter: sym!("libvlc_media_player_set_chapter": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_audio_set_volume: sym!("libvlc_audio_set_volume": unsafe extern "C" fn(*mut c_void, c_int) -> c_int),
            libvlc_video_set_deinterlace: sym!("libvlc_video_set_deinterlace": unsafe extern "C" fn(*mut c_void, *const c_char)),
            libvlc_media_player_navigate: sym!("libvlc_media_player_navigate": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_media_player_navigate_menu: sym_opt!("libvlc_media_player_navigate_menu": unsafe extern "C" fn(*mut c_void)),
            libvlc_media_player_get_full_title_descriptions: sym!("libvlc_media_player_get_full_title_descriptions": unsafe extern "C" fn(*mut c_void, *mut *mut *mut VlcTitleDescription) -> c_int),
            libvlc_title_descriptions_release: sym!("libvlc_title_descriptions_release": unsafe extern "C" fn(*mut *mut VlcTitleDescription, c_int)),
            libvlc_media_player_get_full_chapter_descriptions: sym!("libvlc_media_player_get_full_chapter_descriptions": unsafe extern "C" fn(*mut c_void, c_int, *mut *mut *mut VlcChapterDescription) -> c_int),
            libvlc_chapter_descriptions_release: sym!("libvlc_chapter_descriptions_release": unsafe extern "C" fn(*mut *mut VlcChapterDescription, c_int)),
            libvlc_video_get_spu_count: sym!("libvlc_video_get_spu_count": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_video_get_spu: sym!("libvlc_video_get_spu": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_video_set_spu: sym!("libvlc_video_set_spu": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_video_get_spu_description: sym!("libvlc_video_get_spu_description": unsafe extern "C" fn(*mut c_void) -> *mut VlcTrackDescription),
            libvlc_audio_get_track_count: sym!("libvlc_audio_get_track_count": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_audio_get_track: sym!("libvlc_audio_get_track": unsafe extern "C" fn(*mut c_void) -> c_int),
            libvlc_audio_set_track: sym!("libvlc_audio_set_track": unsafe extern "C" fn(*mut c_void, c_int)),
            libvlc_audio_get_track_description: sym!("libvlc_audio_get_track_description": unsafe extern "C" fn(*mut c_void) -> *mut VlcTrackDescription),
            libvlc_track_description_list_release: sym!("libvlc_track_description_list_release": unsafe extern "C" fn(*mut VlcTrackDescription)),
            libvlc_video_get_size: sym_opt!("libvlc_video_get_size": unsafe extern "C" fn(*mut c_void, std::ffi::c_uint, *mut std::ffi::c_uint, *mut std::ffi::c_uint) -> c_int),
            libvlc_video_take_snapshot: sym_opt!("libvlc_video_take_snapshot": unsafe extern "C" fn(*mut c_void, std::ffi::c_uint, std::ffi::c_uint, *const c_char) -> c_uint),
            lib,
            deps,
        }
    }

    pub fn version(&self) -> String {
        unsafe {
            let s = (self.libvlc_get_version)();
            if s.is_null() {
                return String::new();
            }
            CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    /// Path of the libvlc that successfully loaded (for the status pane).
    pub fn last_loaded_path() -> Option<PathBuf> {
        LOADED_PATH.lock().ok()?.clone()
    }
}

static LOADED_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// One playback session: an instance, a media, and its player.
#[derive(Clone, Copy)]
pub struct VlcSession {
    pub instance: usize,
    pub media: usize,
    pub media_player: usize,
}

/// Owned playback session. Dropping releases everything in order.
impl VlcSession {
    /// Start playing `path`, rendering video into `video_target` (a raw
    /// NSView pointer on macOS, 0 for none). The player is created attached
    /// to the target BEFORE `play()` so the vout lands in our view instead
    /// of spawning VLC's own window.
    pub fn open(lib: &VlcLib, path: &str, video_target: usize) -> Result<VlcSession, String> {
        // Standalone constructor: instance created here is kept for the
        // process lifetime (never released — see close()).
        let shared = std::sync::atomic::AtomicUsize::new(0);
        Self::open_shared(lib, &shared, path, video_target)
    }

    /// Shared-instance variant: the instance is created once and kept for
    /// the app's lifetime (never released) — see PlayerState::shared_instance.
    pub fn open_shared(
        lib: &VlcLib,
        shared: &std::sync::atomic::AtomicUsize,
        path: &str,
        video_target: usize,
    ) -> Result<VlcSession, String> {
        let c_path = CString::new(path).map_err(|_| "bad path".to_string())?;
        let mut args = vec![
            CString::new("--deinterlace=-1").unwrap(), // automatic: only when the stream is interlaced
            CString::new("--deinterlace-mode=yadif2x").unwrap(), // high-quality double-rate
            CString::new("--no-osd").unwrap(),
            CString::new("--bluray-menu").unwrap(),
            // Nebulae's UI overlay handles all mouse clicks and keyboard shortcuts:
            // libVLC must never intercept mouse or keyboard events on the video window.
            CString::new("--no-mouse-events").unwrap(),
            CString::new("--no-keyboard-events").unwrap(),
            // VLC internal log is routed via the NEBULAE_VLC_LOG shim dylib
            // (libvlc_log_set C callback). The --file-logging / --logfile args
            // are VLC application flags and are silently ignored by libvlc_new.
            CString::new("--verbose=2").unwrap(),
            // Network share & high-bitrate UHD Blu-ray buffer tuning:
            // Default libvlc caching (300ms) starves on SMB shares. 3000ms prevents stutter and freezes.
            CString::new("--file-caching=3000").unwrap(),
            CString::new("--network-caching=3000").unwrap(),
            CString::new("--disc-caching=3000").unwrap(),
            CString::new("--live-caching=3000").unwrap(),
            // HDR tone-mapping for UHD Blu-ray (BT.2020 PQ / ST2084 ->
            // SDR): handled by the vout's OpenGL tone-mapping shader
            // pipeline (VLC 3.0.x has no libplacebo; it lives in the
            // OpenGL vout helper). These variables are inherited by the
            // vout's GL
            // helper; they are NOT among the player-object variables that
            // libvlc_media_player_set_nsobject() resets, so they survive
            // embedding. A --vout pin is deliberately absent: set_nsobject
            // resets the player object's "vout" variable anyway (upstream
            // 3.0.x lib/media_player.c), and the default module probe
            // already selects caopengllayer for an embedded NSView
            // (priority 300 > vout_macosx 250, verified in
            // modules/video_output/caopengllayer.m and macosx.m).
            CString::new("--tone-mapping=3").unwrap(), // Hable filmic
            CString::new("--tone-mapping-param=1.0").unwrap(),
            CString::new("--tone-mapping-desat=0.5").unwrap(),
            // Windows Direct3D11 HDR tone mapping:
            // Direct3D11 default is d3d11-hdr-mode=auto, which creates a PQ swapchain
            // without SDR tone mapping, resulting in washed-out lifted blacks on standard SDR screens.
            // Setting d3d11-hdr-mode=never forces Direct3D11's shader to apply filmic Hable
            // tone-mapping down to BT.709 SDR, producing rich blacks and saturated colors matching macOS.
            CString::new("--d3d11-hdr-mode=never").unwrap(),
        ];
        if let Some(plugin_dir) = std::env::var_os("VLC_PLUGIN_PATH") {
            if let Some(plugin_str) = plugin_dir.to_str() {
                if let Ok(arg) = CString::new(format!("--plugin-path={}", plugin_str)) {
                    args.push(arg);
                }
            }
        }
        let argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr() as *const c_char).collect();
        unsafe {
            let mut instance = shared.load(std::sync::atomic::Ordering::SeqCst) as *mut c_void;
            if instance.is_null() {
                instance = (lib.libvlc_new)(argv.len() as c_int, argv.as_ptr());
                if instance.is_null() {
                    let detail = {
                        let msg_ptr = (lib.libvlc_errmsg)();
                        if !msg_ptr.is_null() {
                            CStr::from_ptr(msg_ptr).to_string_lossy().into_owned()
                        } else {
                            "unknown error".to_string()
                        }
                    };
                    return Err(format!("libvlc_new failed: {detail}"));
                }
                shared.store(instance as usize, std::sync::atomic::Ordering::SeqCst);
            }
            // Folder-based disc images must be opened through a disc MRL:
            // libvlc_media_new_path on a directory plays it as a plain
            // filesystem listing (single stream, no titles/chapters, no
            // menu), which is why folder Blu-ray showed no Menu button and
            // no chapter controls. VLC 3 exposes both via bluray:// /
            // dvd://.
            // Diagnostic: route VLC's internal log into a file via a C shim
            // (libvlc_log_set needs a variadic C callback, which stable Rust
            // cannot express). NEBULAE_VLC_LOG=<path to shim dylib>.
            if let Some(shim) = std::env::var_os("NEBULAE_VLC_LOG") {
                install_vlc_log_shim(std::path::Path::new(&shim), instance);
            }
            let is_dir = std::path::Path::new(path).is_dir();
            let media = if is_dir {
                // Scheme follows the folder CONTENTS: a BDMV child means
                // Blu-ray, a VIDEO_TS child means DVD. The path string alone
                // is misleading — "Blu-ray" in a folder name does not imply
                // a BDMV under the disc root.
                let root = std::path::Path::new(path);
                let scheme = if root.join("BDMV").exists() || root.join("bdmv").exists() {
                    "bluray"
                } else {
                    "dvd"
                };
                let mrl = format_disc_mrl(scheme, path);
                let c_mrl = CString::new(mrl).map_err(|_| "bad path".to_string())?;
                (lib.libvlc_media_new_location)(instance, c_mrl.as_ptr())
            } else {
                (lib.libvlc_media_new_path)(instance, c_path.as_ptr())
            };
            if media.is_null() {
                // Do NOT release `instance` here: it is the shared app-lifetime
                // libvlc instance stored in PlayerState::shared_instance. Releasing
                // it leaves a dangling pointer in the AtomicUsize and causes a
                // use-after-free on the next playback attempt.
                return Err(format!("libvlc could not open {path}"));
            }
            let mp = (lib.libvlc_media_player_new_from_media)(media);
            if mp.is_null() {
                (lib.libvlc_media_release)(media);
                return Err("media player creation failed".into());
            }
            if video_target != 0 {
                // Use the platform-correct vout attachment API. Calling
                // set_nsobject on Windows/Linux is a no-op at best; calling
                // the wrong variant on macOS would crash.
                #[cfg(target_os = "macos")]
                {
                    (lib.libvlc_media_player_set_nsobject)(mp, video_target as *mut c_void);
                    // NOTE: set_nsobject clears "vout" (and "avcodec-hw",
                    // "window") on the player (upstream sets them to ""),
                    // which would shadow a --vout libvlc_new arg for every
                    // vout created under this player. This is why no --vout
                    // is passed at all: the default probe selects
                    // caopengllayer for the embedded NSView, and that module
                    // carries the OpenGL tone-mapping pipeline (VLC 3.0.x
                    // has no libplacebo). If a future change ever needs a
                    // forced display module, it
                    // must re-set "vout" here AFTER the embedding call via
                    // var_SetChecked (bound above; VLC_VAR_STRING = 0x0040,
                    // the vlc_value_t union is 8 bytes so the C string
                    // pointer travels as usize).
                }
                #[cfg(target_os = "windows")]
                {
                    (lib.libvlc_media_player_set_hwnd)(mp, video_target as *mut c_void);
                }
                #[cfg(target_os = "linux")]
                {
                    // Linux: video_target is an X11 Window id (u32/u64, cast to
                    // u32 for set_xwindow). Under native Wayland there is no X11
                    // drawable and ensure_overlay already returns an error, so
                    // this branch is never reached on a pure Wayland session.
                    (lib.libvlc_media_player_set_xwindow)(mp, video_target as u32);
                }
            }
            let rc = (lib.libvlc_media_player_play)(mp);
            if rc != 0 {
                (lib.libvlc_media_player_release)(mp);
                (lib.libvlc_media_release)(media);
                return Err(format!("libvlc refused to play {path} (rc={rc})"));
            }
            Ok(VlcSession {
                instance: instance as usize,
                media: media as usize,
                media_player: mp as usize,
            })
        }
    }

    /// Tear down. Runs on the main thread on macOS (the vout view lives
    /// there); on other platforms any thread is fine.
    pub fn close(&self, lib: &VlcLib) {
        unsafe {
            if self.media_player != 0 {
                (lib.libvlc_media_player_stop)(self.media_player as *mut c_void);
                for _ in 0..50 {
                    if (lib.libvlc_media_player_is_playing)(self.media_player as *mut c_void) == 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                #[cfg(target_os = "windows")]
                {
                    (lib.libvlc_media_player_set_hwnd)(
                        self.media_player as *mut c_void,
                        std::ptr::null_mut(),
                    );
                }
                #[cfg(target_os = "macos")]
                {
                    (lib.libvlc_media_player_set_nsobject)(
                        self.media_player as *mut c_void,
                        std::ptr::null_mut(),
                    );
                }
                (lib.libvlc_media_player_release)(self.media_player as *mut c_void);
            }
            if self.media != 0 {
                (lib.libvlc_media_release)(self.media as *mut c_void);
            }
            // The libvlc instance is SHARED (PlayerState::shared_instance):
            // releasing it runs libxml2's global xmlCleanupParser, which
            // races the next session's metadata parsing and aborts the
            // process. Keep it for the app's lifetime.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_disc_mrl_windows_drive_and_spaces() {
        let mrl =
            format_disc_mrl_windows("bluray", r"V:\Blu-ray\Fuuka (2017) Funimation\FUUKA_CS_D1");
        assert_eq!(
            mrl,
            "bluray:///V:/Blu-ray/Fuuka%20%282017%29%20Funimation/FUUKA_CS_D1"
        );
    }

    #[test]
    fn test_format_disc_mrl_windows_dvd() {
        let mrl = format_disc_mrl_windows(
            "dvd",
            r"C:\Users\administrator\Downloads\Banner of the Stars III OVA (2005) R2J\DISC1",
        );
        assert_eq!(
            mrl,
            "dvd:///C:/Users/administrator/Downloads/Banner%20of%20the%20Stars%20III%20OVA%20%282005%29%20R2J/DISC1"
        );
    }

    #[test]
    fn test_encode_mrl_path_unix() {
        let path = "/Volumes/Media/Fuuka (2017)/FUUKA_CS_D1";
        let encoded = encode_mrl_path(path);
        assert_eq!(encoded, "/Volumes/Media/Fuuka%20%282017%29/FUUKA_CS_D1");
    }
}

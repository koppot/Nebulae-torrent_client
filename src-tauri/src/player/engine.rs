//! Runtime loader for the system libVLC and a minimal VLC 3.x FFI.
//!
//! Only the handful of symbols playback needs are bound. Everything is
//! dynamically resolved from the user's own VLC installation, so the exact
//! dylib path differs per platform.

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
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
}

/// Mirror of libvlc_title_description_t (C layout, header-verified).
#[repr(C)]
pub struct VlcTitleDescription {
    pub i_duration: i64,
    pub psz_name: *mut c_char,
    pub i_flags: u32,
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
fn encode_mrl_path(path: &str) -> String {
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
            // Preload libvlccore by absolute path: libvlc references it via
            // @rpath/soname, which does not resolve from a foreign process.
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
        // All dylibs in the lib dir (skip libvlc itself, loaded separately).
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let dep = entry.path();
                let is_dylib = dep.extension().and_then(|e| e.to_str()) == Some("dylib");
                let is_libvlc = dep
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("libvlc."))
                    .unwrap_or(false);
                if is_dylib && !is_libvlc {
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
                    c.push(dir.join("vlc").join("libvlc.dll"));
                }
                if let Some(parent) = exe.parent().and_then(|d| d.parent()) {
                    c.push(parent.join("resources").join("vlc").join("libvlc.dll"));
                }
            }
            c.push(PathBuf::from("src-tauri/resources/vlc/libvlc.dll"));
            c.push(PathBuf::from("resources/vlc/libvlc.dll"));
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
        VlcLib {
            libvlc_new: sym!("libvlc_new": unsafe extern "C" fn(c_int, *const *const c_char) -> *mut c_void),
            libvlc_release: sym!("libvlc_release": unsafe extern "C" fn(*mut c_void)),
            libvlc_errmsg: sym!("libvlc_errmsg": unsafe extern "C" fn() -> *const c_char),
            libvlc_get_version: sym!("libvlc_get_version": unsafe extern "C" fn() -> *const c_char),
            libvlc_media_new_path: sym!("libvlc_media_new_path": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_new_location: sym!("libvlc_media_new_location": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_release: sym!("libvlc_media_release": unsafe extern "C" fn(*mut c_void)),
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
            // Route VLC's internal log to a file — the embedded player's
            // stderr is invisible when launched from Finder, and demux
            // failures (e.g. bluray://) otherwise fail silently.
            CString::new("--verbose=2").unwrap(),
            CString::new("--file-logging").unwrap(),
            CString::new(format!(
                "--logfile={}",
                std::env::temp_dir().join("nebulae-vlc.log").display()
            ))
            .unwrap(),
            // Network share & high-bitrate UHD Blu-ray buffer tuning:
            // Default libvlc caching (300ms) starves on SMB shares. 3000ms prevents stutter and freezes.
            CString::new("--file-caching=3000").unwrap(),
            CString::new("--network-caching=3000").unwrap(),
            CString::new("--disc-caching=3000").unwrap(),
            CString::new("--live-caching=3000").unwrap(),
            // HDR Tone-Mapping:
            // When playing BT.2020 PQ (ST2084) HDR content, apply Hable filmic tone-mapping
            // so contrast, saturation, and highlight roll-off render accurately without washed-out colors.
            // On standard SDR content (BT.709), this setting remains inactive.
            CString::new("--tone-mapping=3").unwrap(),
            CString::new("--tone-mapping-param=1.0").unwrap(),
            CString::new("--tone-mapping-desat=0.5").unwrap(),
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
                let mrl = format!("{scheme}://{}", encode_mrl_path(path));
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
                for _ in 0..20 {
                    if (lib.libvlc_media_player_is_playing)(self.media_player as *mut c_void) == 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
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

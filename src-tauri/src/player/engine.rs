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
    pub libvlc_get_version: unsafe extern "C" fn() -> *const c_char,
    pub libvlc_media_new_path: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    // MRL-based media (bluray://, dvd://) — folder-based disc images.
    pub libvlc_media_new_location: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub libvlc_media_release: unsafe extern "C" fn(*mut c_void),
    pub libvlc_media_player_new_from_media: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub libvlc_media_player_release: unsafe extern "C" fn(*mut c_void),
    pub libvlc_media_player_set_nsobject: unsafe extern "C" fn(*mut c_void, *mut c_void),
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

impl VlcLib {
    /// Locate and load the user's libVLC.
    pub fn load() -> Result<VlcLib, String> {
        let candidates = Self::candidate_paths();
        let mut failures: Vec<String> = Vec::new();
        for path in &candidates {
            // libVLC discovers its module plugins relative to itself, but a
            // dylib loaded out of VLC.app needs the plugin dir pointed at
            // explicitly. Setting it before the first `libvlc_new` is the
            // documented approach.
            if let Some(plugins) = Self::plugin_dir_for(path) {
                if std::env::var_os("VLC_PLUGIN_PATH").is_none() {
                    std::env::set_var("VLC_PLUGIN_PATH", &plugins);
                }
            }
            // Preload libvlccore by absolute path: libvlc references it via
            // @rpath/soname, which does not resolve from a foreign process.
            let deps = Self::preload_core_deps(path, &mut failures);
            unsafe {
                match Library::new(path) {
                    Ok(lib) => {
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
            "VLC's playback engine (libvlc) was not found. Install VLC (https://www.videolan.org) and try again. Attempts: {}",
            failures.join(" | ")
        ))
    }

    /// Preload the libvlccore dependency (same directory as libvlc) so the
    /// @rpath / soname reference resolves. Best effort: failures are
    /// reported in `failures` but the main load is still attempted.
    fn preload_core_deps(lib_path: &Path, failures: &mut Vec<String>) -> Vec<Library> {
        let Some(dir) = lib_path.parent() else {
            return Vec::new();
        };
        #[cfg(target_os = "macos")]
        let names = ["libvlccore.9.dylib", "libvlccore.dylib"];
        #[cfg(target_os = "windows")]
        let names = ["libvlccore.dll"];
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let names = ["libvlccore.so.9", "libvlccore.so"];
        let mut out = Vec::new();
        for name in names {
            let dep = dir.join(name);
            if dep.exists() {
                unsafe {
                    match Library::new(&dep) {
                        Ok(l) => out.push(l),
                        Err(e) => failures.push(format!("preload {}: {e}", dep.display())),
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
            if let Ok(home) = std::env::var("HOME") {
                c.push(
                    PathBuf::from(&home)
                        .join("Applications/VLC.app/Contents/MacOS/lib/libvlc.dylib"),
                );
            }
            c.push(PathBuf::from(
                "/Applications/VLC.app/Contents/MacOS/lib/libvlc.dylib",
            ));
            c.push(PathBuf::from("/usr/local/lib/libvlc.dylib"));
            c.push(PathBuf::from("/opt/homebrew/lib/libvlc.dylib"));
        }
        #[cfg(target_os = "windows")]
        {
            c.push(PathBuf::from(r"C:\Program Files\VideoLAN\VLC\libvlc.dll"));
            c.push(PathBuf::from(
                r"C:\Program Files (x86)\VideoLAN\VLC\libvlc.dll",
            ));
        }
        #[cfg(target_os = "linux")]
        {
            c.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/libvlc.so.5"));
            c.push(PathBuf::from("/usr/lib/libvlc.so.5"));
            c.push(PathBuf::from("/usr/lib/libvlc.so"));
        }
        c
    }

    fn plugin_dir_for(lib_path: &Path) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            // .../VLC.app/Contents/MacOS/lib/libvlc.dylib -> .../MacOS/plugins
            let dir = lib_path
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins"));
            dir.filter(|p| p.exists())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = lib_path;
            None
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
            libvlc_get_version: sym!("libvlc_get_version": unsafe extern "C" fn() -> *const c_char),
            libvlc_media_new_path: sym!("libvlc_media_new_path": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_new_location: sym!("libvlc_media_new_location": unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void),
            libvlc_media_release: sym!("libvlc_media_release": unsafe extern "C" fn(*mut c_void)),
            libvlc_media_player_new_from_media: sym!("libvlc_media_player_new_from_media": unsafe extern "C" fn(*mut c_void) -> *mut c_void),
            libvlc_media_player_release: sym!("libvlc_media_player_release": unsafe extern "C" fn(*mut c_void)),
            libvlc_media_player_set_nsobject: sym!("libvlc_media_player_set_nsobject": unsafe extern "C" fn(*mut c_void, *mut c_void)),
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
        let c_path = CString::new(path).map_err(|_| "bad path".to_string())?;
        let args = [
            CString::new("--deinterlace=-1").unwrap(), // automatic: only when the stream is interlaced
            CString::new("--deinterlace-mode=yadif2x").unwrap(), // high-quality double-rate
            CString::new("--no-osd").unwrap(),
        ];
        let argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr() as *const c_char).collect();
        unsafe {
            let instance = (lib.libvlc_new)(argv.len() as c_int, argv.as_ptr());
            if instance.is_null() {
                return Err("libvlc_new failed".into());
            }
            // Folder-based disc images must be opened through a disc MRL:
            // libvlc_media_new_path on a directory plays it as a plain
            // filesystem listing (single stream, no titles/chapters, no
            // menu), which is why folder Blu-ray showed no Menu button and
            // no chapter controls. VLC 3 exposes both via bluray:// /
            // dvd://.
            let is_dir = std::path::Path::new(path).is_dir();
            let media = if is_dir {
                let scheme = if path.to_ascii_lowercase().contains("bdmv") {
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
                (lib.libvlc_release)(instance);
                return Err(format!("libvlc could not open {path}"));
            }
            let mp = (lib.libvlc_media_player_new_from_media)(media);
            if mp.is_null() {
                (lib.libvlc_media_release)(media);
                (lib.libvlc_release)(instance);
                return Err("media player creation failed".into());
            }
            if video_target != 0 {
                (lib.libvlc_media_player_set_nsobject)(mp, video_target as *mut c_void);
            }
            let rc = (lib.libvlc_media_player_play)(mp);
            if rc != 0 {
                (lib.libvlc_media_player_release)(mp);
                (lib.libvlc_media_release)(media);
                (lib.libvlc_release)(instance);
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
            (lib.libvlc_media_player_stop)(self.media_player as *mut c_void);
            (lib.libvlc_media_player_release)(self.media_player as *mut c_void);
            (lib.libvlc_media_release)(self.media as *mut c_void);
            (lib.libvlc_release)(self.instance as *mut c_void);
        }
    }
}

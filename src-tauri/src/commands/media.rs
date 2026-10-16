use crate::player::engine::VlcSession;
use crate::player::{self, PlayerState};
use crate::utils::disc_detector::{DiscDetector, DiscStructureInfo};
use crate::utils::media_parser::{MediaTitleParser, ParsedMediaMetadata};
use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[tauri::command]
pub fn parse_media_title(raw_title: String) -> Result<ParsedMediaMetadata, String> {
    Ok(MediaTitleParser::parse(&raw_title))
}

#[tauri::command]
pub fn detect_disc_format(file_path: String) -> Result<Option<DiscStructureInfo>, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }
    Ok(DiscDetector::inspect(path))
}

/// Every disc inside the release (multi-ISO torrents return one entry per
/// disc, each classified by its own size) — powers the aggregated badge.
#[tauri::command]
pub fn detect_disc_set(file_path: String) -> Result<Vec<DiscStructureInfo>, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        tracing::warn!("[disc-set] path missing: {file_path}");
        return Err(format!("Media path does not exist: {file_path}"));
    }
    let set = DiscDetector::inspect_all(path);
    tracing::info!(
        "[disc-set] {file_path} -> {} disc(s): {}",
        set.len(),
        set.iter()
            .map(|d| format!("{:?}/{:?}", d.disc_type, d.layer_spec))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(set)
}

#[tauri::command]
pub fn scan_disc_artwork(file_path: String) -> Result<Option<String>, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }
    Ok(DiscDetector::find_cover_image(path))
}

use serde::{Deserialize, Serialize};

/// What the Settings → Media pane shows about the playback engine.
/// The engine is the user's own VLC (loaded as a library and embedded in
/// the app window); there is no bundled player anymore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscPlayerStatus {
    pub player_type: String,
    pub player_name: String,
    pub player_path: Option<String>,
    pub supports_dvd_menu: bool,
    pub supports_bluray_menu: bool,
    pub java_home: Option<String>,
    pub has_jvm: bool,
    pub bd_j_supported: bool,
    pub note: Option<String>,
}

#[tauri::command]
pub fn get_disc_player_status() -> Result<DiscPlayerStatus, String> {
    match crate::player::engine::VlcLib::load() {
        Ok(lib) => {
            let version = lib.version();
            let name = if version.is_empty() {
                "VLC (embedded engine)".to_string()
            } else {
                format!("VLC {version} (embedded engine)")
            };
            Ok(DiscPlayerStatus {
                player_type: "vlc".to_string(),
                player_name: name,
                player_path: player::engine::VlcLib::last_loaded_path()
                    .map(|p| p.display().to_string()),
                supports_dvd_menu: true,
                supports_bluray_menu: true,
                java_home: None,
                has_jvm: false,
                // VLC 3 renders Blu-ray standard menus; BD-J interactive
                // menus need a JVM VLC doesn't ship on this platform.
                bd_j_supported: false,
                note: Some(
                    "Playback runs inside Nebulae on VLC's engine: full DVD menus, Blu-ray standard menus, and automatic high-quality deinterlacing. BD-J interactive Blu-ray menus are not available in VLC 3."
                        .to_string(),
                ),
            })
        }
        Err(e) => Ok(DiscPlayerStatus {
            player_type: "none".to_string(),
            player_name: "VLC not found".to_string(),
            player_path: None,
            supports_dvd_menu: false,
            supports_bluray_menu: false,
            java_home: None,
            has_jvm: false,
            bd_j_supported: false,
            note: Some(e),
        }),
    }
}

/// What `player_open` resolved and whether playback runs embedded.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedPlayback {
    pub resolved_path: String,
    pub title: String,
    pub is_disc: bool,
    pub external: bool,
}

/// Resolve what to play for a save path / file / folder. A disc image or
/// disc folder ALWAYS wins over loose episode files; a disc-less folder
/// plays its largest video file (the main feature).
fn resolve_playback(file_path: &str) -> Result<ResolvedPlayback, String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }
    if let Some(disc) = DiscDetector::inspect(path) {
        let title = disc.title.clone();
        return Ok(ResolvedPlayback {
            resolved_path: disc.root_path.clone(),
            title,
            is_disc: true,
            external: false,
        });
    }
    if path.is_dir() {
        if let Some(target) = resolve_disc_target(path) {
            if let Some(disc) = DiscDetector::inspect(&target) {
                let title = disc.title.clone();
                return Ok(ResolvedPlayback {
                    resolved_path: disc.root_path.clone(),
                    title,
                    is_disc: true,
                    external: false,
                });
            }
        }
        return match find_main_video(path) {
            Some(video) => {
                let title = video
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Video")
                    .to_string();
                Ok(ResolvedPlayback {
                    resolved_path: video.display().to_string(),
                    title,
                    is_disc: false,
                    external: false,
                })
            }
            None => Err(format!("No playable media found in {}", path.display())),
        };
    }
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Video")
        .to_string();
    Ok(ResolvedPlayback {
        resolved_path: file_path.to_string(),
        title,
        is_disc: false,
        external: false,
    })
}

fn loaded_lib(state: &PlayerState) -> Result<std::sync::Arc<player::engine::VlcLib>, String> {
    let mut guard = state.lib.lock().map_err(|_| "player state poisoned")?;
    if guard.is_none() {
        *guard = Some(std::sync::Arc::new(player::engine::VlcLib::load()?));
    }
    Ok(guard.as_ref().unwrap().clone())
}

fn poison<T>(g: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    g.lock().unwrap_or_else(|e| e.into_inner())
}

fn stop_session(app: &tauri::AppHandle, state: &PlayerState) {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return;
    }
    if let Ok(lib_guard) = state.lib.lock() {
        if let Some(lib) = lib_guard.as_ref() {
            let session = VlcSession {
                instance: *poison(&state.instance),
                media: *poison(&state.media),
                media_player: mp,
            };
            // The vout renders into our overlay view; hide it before the
            // player goes away so no stale frame lingers.
            let _ = player::set_overlay_hidden(app, true);
            session.close(lib);
        }
    }
    *poison(&state.instance) = 0;
    *poison(&state.media) = 0;
    *poison(&state.media_player) = 0;
    *poison(&state.started) = false;
}

/// Open playback inside Nebulae. Everything goes through ONE engine: the
/// user's VLC, loaded as a library and rendered into the app's own window.
/// Discs start at their root menu; plain files and disc-less folders play
/// the main feature. There is no external-player fallback: if the overlay
/// cannot be created, playback fails with an error.
fn start_playback(
    app: &tauri::AppHandle,
    state: &PlayerState,
    file_path: &str,
) -> Result<ResolvedPlayback, String> {
    let resolved = match resolve_playback(file_path) {
        Ok(r) => {
            tracing::info!(
                "player_open: resolved '{}' (disc: {}, external: {})",
                r.resolved_path,
                r.is_disc,
                r.external
            );
            r
        }
        Err(e) => {
            tracing::error!("player_open: resolve failed: {e}");
            return Err(e);
        }
    };

    match player::ensure_overlay(app) {
        Ok(view) => {
            tracing::info!("player_open: overlay ready (view ptr {view:#x})");
            let lib = match loaded_lib(state) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("player_open: libvlc load failed: {e}");
                    return Err(e);
                }
            };
            // The overlay is created hidden; make it visible BEFORE the vout
            // attaches, or the video renders into a hidden view (a dead
            // black rectangle that receives no clicks).
            if let Err(e) = player::set_overlay_hidden(app, false) {
                tracing::error!("player_open: overlay unhide failed: {e}");
                return Err(e);
            }
            match VlcSession::open(&lib, &resolved.resolved_path, view) {
                Ok(session) => {
                    *poison(&state.instance) = session.instance;
                    *poison(&state.media) = session.media;
                    *poison(&state.media_player) = session.media_player;
                    *poison(&state.started) = true;
                    tracing::info!(
                        "player_open: PLAYING '{}' (disc: {})",
                        resolved.resolved_path,
                        resolved.is_disc
                    );
                    Ok(resolved)
                }
                Err(e) => {
                    tracing::error!("player_open: session open failed: {e}");
                    Err(e)
                }
            }
        }
        Err(embed_err) => {
            tracing::error!("player_open: overlay unavailable: {embed_err}");
            Err(format!(
                "Embedded playback is unavailable: {embed_err}. Playback requires the in-app video overlay - there is no external-player fallback."
            ))
        }
    }
}

#[tauri::command]
pub fn player_open(
    app: tauri::AppHandle,
    state: tauri::State<PlayerState>,
    file_path: String,
) -> Result<ResolvedPlayback, String> {
    tracing::info!("player_open: entry, path='{file_path}'");
    stop_session(&app, &state);
    *poison(&state.current_path) = Some(file_path.clone());
    let resolved = start_playback(&app, &state, &file_path)?;
    *poison(&state.current_resolved) = Some(resolved.resolved_path.clone());
    Ok(resolved)
}

/// Return DVD/BD playback to its interactive menu.
///
/// `set_title(0)` is the textbook way to jump to the dvdnav menu domain, but
/// some dvdnav builds ignore it mid-title. When the title number does not
/// change after a grace period we simply restart the disc fresh — a freshly
/// opened DVD always presents its menu.
#[tauri::command]
pub fn player_return_to_menu(
    app: tauri::AppHandle,
    state: tauri::State<PlayerState>,
) -> Result<(), String> {
    tracing::info!("[player] return_to_menu: requested");
    let mp = *poison(&state.media_player);
    let lib = loaded_lib(&state)?;
    if mp != 0 {
        unsafe {
            (lib.libvlc_media_player_set_title)(mp as *mut std::ffi::c_void, 0);
        }
        tracing::info!("[player] return_to_menu: set_title(0) sent");
    }
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let at_menu = mp != 0
        && unsafe { (lib.libvlc_media_player_get_title)(mp as *mut std::ffi::c_void) } == 0;
    if at_menu {
        tracing::info!("[player] return_to_menu: now at menu title");
        return Ok(());
    }
    // dvdnav ignored us — restart the disc; a fresh open always shows the menu.
    let path = poison(&state.current_path)
        .clone()
        .unwrap_or_default();
    if path.is_empty() {
        tracing::warn!("[player] return_to_menu: no current path to restart");
        return Err("no current media to restart".into());
    }
    tracing::info!("[player] return_to_menu: set_title ineffective — restarting '{path}'");
    stop_session(&app, &state);
    start_playback(&app, &state, &path).map(|_| ())
}

#[tauri::command]
pub fn player_close(app: tauri::AppHandle, state: tauri::State<PlayerState>) -> Result<(), String> {
    stop_session(&app, &state);
    Ok(())
}

/// Position snapshot for the control bar; the frontend polls this.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerPoll {
    pub time_ms: i64,
    pub length_ms: i64,
    pub playing: bool,
    // Disc navigation state (0 / -1 for plain files): the overlay's chapter
    // bar and the Disc Menu button read these from the same poll.
    pub title: i32,
    pub title_count: i32,
    pub chapter: i32,
    pub chapter_count: i32,
}

#[tauri::command]
pub fn player_poll(state: tauri::State<PlayerState>) -> Result<PlayerPoll, String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(PlayerPoll {
            time_ms: 0,
            length_ms: 0,
            playing: false,
            title: 0,
            title_count: 0,
            chapter: 0,
            chapter_count: 0,
        });
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        Ok(PlayerPoll {
            time_ms: (lib.libvlc_media_player_get_time)(m),
            length_ms: (lib.libvlc_media_player_get_length)(m),
            playing: (lib.libvlc_media_player_is_playing)(m) != 0,
            title: (lib.libvlc_media_player_get_title)(m),
            title_count: (lib.libvlc_media_player_get_title_count)(m),
            chapter: (lib.libvlc_media_player_get_chapter)(m),
            chapter_count: (lib.libvlc_media_player_get_chapter_count)(m),
        })
    }
}

/// Jump to a disc TITLE. Title 0 is the DVD menu domain (dvdnav) — the
/// control bar's "Disc Menu" button uses this to leave the feature and
/// return to the interactive menu.
#[tauri::command]
pub fn player_set_title(state: tauri::State<PlayerState>, title: i32) -> Result<(), String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    tracing::info!("[player] set_title({title})");
    unsafe { (lib.libvlc_media_player_set_title)(mp as *mut c_void, title.max(0)) };
    Ok(())
}

/// Jump to a CHAPTER of the current title (0-based; the overlay renders
/// 1-based labels). The chapters popover's grid wires each cell to this.
#[tauri::command]
pub fn player_set_chapter(state: tauri::State<PlayerState>, chapter: i32) -> Result<(), String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    tracing::info!("[player] set_chapter({chapter})");
    unsafe { (lib.libvlc_media_player_set_chapter)(mp as *mut c_void, chapter.max(0)) };
    Ok(())
}

#[tauri::command]
pub fn player_toggle_pause(state: tauri::State<PlayerState>) -> Result<(), String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        let playing = (lib.libvlc_media_player_is_playing)(m) != 0;
        (lib.libvlc_media_player_pause)(m, if playing { 1 } else { 0 });
    }
    Ok(())
}

#[tauri::command]
pub fn player_seek(state: tauri::State<PlayerState>, position_ms: i64) -> Result<(), String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe { (lib.libvlc_media_player_set_time)(mp as *mut c_void, position_ms.max(0)) };
    Ok(())
}

#[tauri::command]
pub fn player_set_volume(state: tauri::State<PlayerState>, volume: i32) -> Result<(), String> {
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        (lib.libvlc_audio_set_volume)(mp as *mut c_void, volume.clamp(0, 100) as c_int);
    }
    Ok(())
}

/// Keep the native video overlay aligned with the frontend placeholder.
/// Coordinates are CSS pixels relative to the top-left of the window.
#[tauri::command]
pub fn player_sync_viewport(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    player::sync_frame(&app, x, y, width, height)
}

/// Depth-limited recursive search for a playable disc image or disc folder
/// structure. A disc image ALWAYS outranks loose episode files - the owner
/// wants the menu-driven disc, never extracted episodes. Returns the disc
/// root: the ISO/IMG file, or the folder containing VIDEO_TS / BDMV.
fn collect_disc_candidates(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if name == "video_ts" || name == "bdmv" {
                // Hand players the disc ROOT (parent of VIDEO_TS/BDMV);
                // both VLC and libdvdnav/libbluray expect that layout.
                if let Some(root) = p.parent() {
                    out.push(root.to_path_buf());
                } else {
                    out.push(p);
                }
            } else {
                subdirs.push(p);
            }
        } else if p.is_file()
            && matches!(
                p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()),
                Some(ref ext) if ext == "iso" || ext == "img"
            )
        {
            out.push(p);
        }
    }
    for d in subdirs {
        collect_disc_candidates(&d, depth + 1, out);
    }
}

/// Pick the disc root to play from a directory: disc-1 images first, then
/// other disc-N images, then VIDEO_TS/BDMV roots, then any other image.
/// Name tie-break keeps multi-disc torrents deterministic.
fn resolve_disc_target(dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_disc_candidates(dir, 0, &mut candidates);
    if candidates.is_empty() {
        return None;
    }
    let rank = |p: &Path| -> u8 {
        if p.is_dir() {
            return 2;
        }
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.contains("disc") && (name.contains('1') || name.contains("one")) {
            0
        } else if name.contains("disc") {
            1
        } else {
            3
        }
    };
    candidates.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    Some(candidates.remove(0))
}

/// Largest video file under a directory (recursive) - the main feature
/// when no disc image exists.
fn find_main_video(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![dir.to_path_buf()];
    let mut depth = 0;
    while let Some(d) = stack.pop() {
        depth += 1;
        if depth > 64 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file()
                && matches!(
                    p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()),
                    Some(ref ext) if matches!(
                        ext.as_str(),
                        "mkv" | "mp4" | "avi" | "mov" | "m2ts" | "ts" | "wmv" | "flv" | "webm"
                            | "mpg" | "mpeg" | "m4v"
                    )
                )
            {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if best.as_ref().map(|(s, _)| size > *s).unwrap_or(true) {
                    best = Some((size, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

#[tauri::command]
pub fn get_volume_disk_space(
    target_path: Option<String>,
) -> Result<crate::utils::library_cleaner::VolumeSpaceInfo, String> {
    let p = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")));
    crate::utils::library_cleaner::LibraryCleaner::get_disk_space(&p)
}

#[tauri::command]
pub fn clean_cache_files(
    older_than_days: Option<u32>,
    dry_run: Option<bool>,
) -> Result<crate::utils::library_cleaner::CacheCleanupReport, String> {
    let days = older_than_days.unwrap_or(30);
    let dry = dry_run.unwrap_or(false);

    let mut candidate_dirs = Vec::new();
    if let Ok(paths) = crate::utils::paths::AppPaths::init() {
        candidate_dirs.push(paths.cache_dir.clone());
        candidate_dirs.push(paths.cache_dir.join("thumbnails"));
        candidate_dirs.push(paths.logs_dir);
    }
    crate::utils::library_cleaner::LibraryCleaner::clean_cache_files(&candidate_dirs, days, dry)
}

#[tauri::command]
pub fn organize_completed_media(
    source_path: String,
    destination_root: String,
) -> Result<crate::utils::library_cleaner::OrganizeMediaReport, String> {
    let src = Path::new(&source_path);
    let dst = Path::new(&destination_root);
    crate::utils::library_cleaner::LibraryCleaner::organize_completed_media(src, dst)
}

#[tauri::command]
pub fn get_media_thumbnail(
    media_path: String,
) -> Result<crate::utils::library_cleaner::ThumbnailResult, String> {
    let path = Path::new(&media_path);
    if !path.exists() {
        tracing::warn!("[thumbnail] media path does not exist: {media_path}");
        return Err(format!("Media path does not exist: {media_path}"));
    }

    let cache_dir = crate::utils::paths::AppPaths::init()
        .map(|p| p.cache_dir)
        .unwrap_or_else(|_| PathBuf::from("."));

    let result =
        crate::utils::library_cleaner::LibraryCleaner::get_or_create_thumbnail(path, &cache_dir);
    if let Some(p) = &result.thumbnail_path {
        tracing::info!("[thumbnail] {media_path} -> {p}");
    } else {
        tracing::warn!("[thumbnail] {media_path} -> no cover found (TMDB fallback should fire)");
    }
    Ok(result)
}

/// TMDB poster + metadata for a release (used when the local folder ships no
/// cover scan). Credentials come from the OS keychain; results are cached in
/// the DB per title+year. Returns None silently when no match / no key.
#[tauri::command]
pub fn fetch_tmdb_release(
    title: String,
    year: Option<i32>,
) -> Result<Option<crate::utils::tmdb::TmdbMatch>, String> {
    // The DB lives in the app's own "Nebulae" data dir (AppPaths), NOT under
    // the bundle identifier dir — resolving it from app.path() produced
    // org.asiandvdclub.nebulae/internal/db/... which never existed, so every
    // TMDB fetch died on "unable to open database file" before any lookup.
    let db_path = crate::utils::paths::AppPaths::init()?.db_file;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Data dir: {e}"))?;
    }
    let db = crate::db::Database::new(&db_path).map_err(|e| format!("Database: {e}"))?;
    let result = crate::utils::tmdb::fetch_poster(&db, &title, &title, year);
    match &result {
        Ok(Some(m)) => tracing::info!(
            "[tmdb] match for {title:?}: poster={} year={:?}",
            m.poster_webp.display(),
            m.year
        ),
        // Silent-skip (no keychain entry) or genuinely no match: both mean the
        // grid keeps its placeholder — log it so the owner sees WHY.
        Ok(None) => tracing::warn!("[tmdb] no match or no credentials for {title:?}"),
        Err(e) => tracing::warn!("[tmdb] fetch failed for {title:?}: {e}"),
    }
    result
}

/// Whether TMDB integration is configured (Settings pane indicator).
#[tauri::command]
pub fn tmdb_configured() -> bool {
    crate::utils::tmdb::has_api_key()
}

/// Fetch the release's ADC webpage for the Details popup (owner: "on details
/// it should get information from ADC website if available"). ADC torrents
/// usually carry the details.php URL in their comment. The page is members-
/// only: any stored session cookie is attached, and a login-redirect answer
/// is reported as unavailable so the frontend falls back to TMDB/Trakt.
#[tauri::command]
pub fn fetch_adc_page(url: String) -> Result<String, String> {
    let parsed = url::Url::parse(&url).map_err(|e| format!("url: {e}"))?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    // Only the ADC site is ever fetched — never an arbitrary URL.
    if !matches!(host.as_str(), "asiandvdclub.org" | "www.asiandvdclub.org") {
        return Err(format!("not an ADC url: {host}"));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let mut req = client.get(&url);
    // A website session (if the owner ever linked one) rides along; the
    // announce passkey is NOT a website credential and is not sent.
    if let Ok(cookie) = crate::utils::keyring::get_secret("adc:session_cookie") {
        if !cookie.is_empty() {
            req = req.header("Cookie", cookie);
        }
    }
    let html = req
        .send()
        .map_err(|e| format!("fetch: {e}"))?
        .text()
        .map_err(|e| format!("body: {e}"))?;

    // Login gate or empty page → treat as "ADC info unavailable".
    let lower = html.to_lowercase();
    if lower.contains("name=\"form_login\"") || lower.contains("login.php") && html.len() < 4096 {
        return Err("ADC page requires a website login".into());
    }

    // Crude HTML → text: drop script/style blocks and tags, collapse
    // whitespace. `split('<')` alternates text and tag segments.
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut skipping: Option<String> = None; // current <script>/<style> kind
    for seg in html.split('<') {
        if !in_tag {
            if skipping.is_none() {
                text.push_str(seg);
            }
            in_tag = true;
        } else if let Some(gt) = seg.find('>') {
            let tag = seg[..gt].to_lowercase();
            let rest = &seg[gt + 1..];
            match &skipping {
                Some(kind) => {
                    if tag.starts_with(&format!("/{kind}")) {
                        skipping = None;
                    }
                }
                None => {
                    if tag.starts_with("script") {
                        skipping = Some("script".into());
                    } else if tag.starts_with("style") {
                        skipping = Some("style".into());
                    } else {
                        text.push_str(rest);
                    }
                }
            }
            in_tag = false;
        }
        // A tag segment without '>' is malformed; drop it.
    }
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim().to_string();
    if text.len() < 80 {
        return Err("ADC page carried no usable description".into());
    }
    let mut excerpt = text.chars().take(1500).collect::<String>();
    if excerpt.len() < text.len() {
        excerpt.push('…');
    }
    tracing::info!("[adc] details page fetched: {} chars", excerpt.len());
    Ok(excerpt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_nonexistent_media() {
        let res = resolve_playback("/nonexistent/movie/file.mkv");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_resolve_disc_target_deep_iso_beats_episodes() {
        let base = std::env::temp_dir().join(format!("neb-disc-{}", std::process::id()));
        let nested = base.join("Season 1").join("Discs");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(base.join("episode01.mkv"), b"x").unwrap();
        std::fs::write(nested.join("DISC_1.ISO"), b"x").unwrap();
        let got = resolve_disc_target(&base).unwrap();
        assert!(got.ends_with("DISC_1.ISO"), "got {got:?}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_resolve_disc_target_prefers_disc_one() {
        let base = std::env::temp_dir().join(format!("neb-disc2-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("DISC_2.ISO"), b"x").unwrap();
        std::fs::write(base.join("DISC_1.ISO"), b"x").unwrap();
        let got = resolve_disc_target(&base).unwrap();
        assert!(got.ends_with("DISC_1.ISO"), "got {got:?}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_resolve_disc_target_video_ts_root() {
        let base = std::env::temp_dir().join(format!("neb-disc3-{}", std::process::id()));
        let dvd = base.join("My Movie").join("VIDEO_TS");
        std::fs::create_dir_all(&dvd).unwrap();
        std::fs::write(dvd.join("VIDEO_TS.IFO"), b"x").unwrap();
        let got = resolve_disc_target(&base).unwrap();
        assert!(got.ends_with("My Movie"), "got {got:?}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_find_main_video_picks_largest() {
        let base = std::env::temp_dir().join(format!("neb-vid-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("small.mp4"), vec![0u8; 10]).unwrap();
        std::fs::write(base.join("feature.mp4"), vec![0u8; 999]).unwrap();
        let got = find_main_video(&base).unwrap();
        assert!(got.ends_with("feature.mp4"), "got {got:?}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_parse_media_title_command() {
        let meta =
            parse_media_title("Ikiru.1952.Remastered.1080p.BluRay.x264.FLAC-ADC".to_string())
                .expect("parsing failed");
        assert_eq!(meta.clean_title, "Ikiru");
        assert_eq!(meta.year, Some(1952));
        assert_eq!(meta.resolution, Some("1080p".to_string()));
        assert_eq!(meta.source, Some("BluRay".to_string()));
        assert_eq!(meta.codec, Some("x264".to_string()));
        assert_eq!(meta.audio, Some("FLAC".to_string()));
        assert_eq!(meta.release_group, Some("ADC".to_string()));
    }

    #[test]
    fn test_get_volume_disk_space_command() {
        let space = get_volume_disk_space(None).expect("Failed to query root volume space");
        assert!(space.total_bytes > 0);
        assert!(space.available_bytes > 0);
        assert!(space.used_percent <= 100);
    }

    #[test]
    fn test_clean_cache_files_command() {
        let report = clean_cache_files(Some(30), Some(true)).expect("Failed clean cache dry run");
        assert!(report.dry_run);
    }

    #[test]
    fn test_organize_completed_media_command() {
        let temp_dir =
            std::env::temp_dir().join(format!("nebulae_cmd_org_test_{}", std::process::id()));
        let src_dir = temp_dir.join("Tokyo.Story.1953.1080p.BluRay-ADC");
        let dest_root = temp_dir.join("Library");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("Tokyo.Story.1953.mkv"), b"video data").unwrap();

        let res = organize_completed_media(
            src_dir.to_string_lossy().to_string(),
            dest_root.to_string_lossy().to_string(),
        )
        .expect("Organize failed");

        assert!(res.success);
        assert_eq!(res.files_moved, 1);
        assert!(dest_root
            .join("Tokyo Story (1953)")
            .join("Tokyo.Story.1953.mkv")
            .exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_disc_player_status_command() {
        let status = get_disc_player_status().expect("get_disc_player_status failed");
        assert!(!status.player_name.is_empty());
        assert!(!status.player_type.is_empty());
    }
}

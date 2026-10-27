use crate::player;
#[cfg(target_os = "macos")]
use crate::player::embed_macos::{force_apply_edr, maybe_apply_edr, overlay_app_handle};
#[cfg(target_os = "windows")]
use crate::player::embed_windows::{force_apply_edr, maybe_apply_edr, overlay_app_handle};
use crate::player::engine;
use crate::player::engine::{is_menu_title, VlcSession};
use crate::player::PlayerState;
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
pub async fn detect_disc_set(file_path: String) -> Result<Vec<DiscStructureInfo>, String> {
    tokio::task::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("detect_disc_set join error: {e}"))?
}

#[tauri::command]
pub async fn scan_disc_artwork(file_path: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let path = Path::new(&file_path);
        if !path.exists() {
            return Err(format!("Media path does not exist: {file_path}"));
        }
        Ok(DiscDetector::find_cover_image(path))
    })
    .await
    .map_err(|e| format!("scan_disc_artwork join error: {e}"))?
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
    let java_home = std::env::var("JAVA_HOME").ok();
    let has_jvm = java_home
        .as_ref()
        .map(|p| {
            let base = std::path::Path::new(p);
            base.join("bin/java").exists() || base.join("bin/java.exe").exists()
        })
        .unwrap_or(false);

    match crate::player::engine::VlcLib::load() {
        Ok(lib) => {
            let version = lib.version();
            let name = if version.is_empty() {
                "Bundled VLC (embedded engine)".to_string()
            } else {
                format!("Bundled VLC {version} (embedded engine)")
            };
            Ok(DiscPlayerStatus {
                player_type: "vlc".to_string(),
                player_name: name,
                player_path: player::engine::VlcLib::last_loaded_path()
                    .map(|p| p.display().to_string()),
                supports_dvd_menu: true,
                supports_bluray_menu: true,
                java_home,
                has_jvm,
                bd_j_supported: has_jvm,
                note: Some(
                    "Playback runs inside Nebulae on the bundled VLC engine: full DVD menus, Blu-ray menus, and automatic high-quality deinterlacing."
                        .to_string(),
                ),
            })
        }
        Err(e) => Ok(DiscPlayerStatus {
            player_type: "none".to_string(),
            player_name: "Bundled player not found".to_string(),
            player_path: None,
            supports_dvd_menu: false,
            supports_bluray_menu: false,
            java_home,
            has_jvm,
            bd_j_supported: has_jvm,
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

/// `player_open` return value and `player-opened` event payload: the
/// resolved playback (flattened, unchanged shape) plus the session
/// generation it belongs to. The event re-fires at 0/1.5/4/8s to survive
/// webview-load races; the generation lets the frontend discard repeats
/// that outlive a user close instead of reopening the player over a dead
/// session.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerOpenResult {
    #[serde(flatten)]
    pub info: ResolvedPlayback,
    pub gen: u64,
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
        // The BDMV/VIDEO_TS directory itself was passed (e.g. saved in the
        // resume store): the disc root is its parent.
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if dir_name == "bdmv" || dir_name == "video_ts" {
            if let Some(disc_root) = path.parent() {
                let title = disc_root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Video")
                    .to_string();
                return Ok(ResolvedPlayback {
                    resolved_path: disc_root.display().to_string(),
                    title,
                    is_disc: true,
                    external: false,
                });
            }
        }
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

    // A file INSIDE a disc structure (BDMV/STREAM/*.m2ts, VIDEO_TS/*.VOB)
    // must never play as a bare clip: such entries are menu loops seconds
    // long, and playback ends into a black screen (owner-reported blank
    // player for the Fuuka Blu-ray). Older builds saved such paths into the
    // resume store; redirect to the disc itself (played through the
    // bluray:// MRL so titles, chapters and the menu are available) instead
    // of playing the clip as a bare file.
    let disc_ancestor = path.ancestors().skip(1).find(|a| {
        let name = a
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        name == "bdmv" || name == "video_ts"
    });
    if let Some(disc_dir) = disc_ancestor {
        // The disc root is the folder CONTAINING BDMV/VIDEO_TS.
        if let Some(disc_root) = disc_dir.parent() {
            let title = disc_root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(title.as_str())
                .to_string();
            tracing::info!(
                "resolve_playback: in-disc file {} redirected to disc root {}",
                path.display(),
                disc_root.display()
            );
            return Ok(ResolvedPlayback {
                resolved_path: disc_root.display().to_string(),
                title,
                is_disc: true,
                external: false,
            });
        }
    }

    // A BDMV folder picked directly is the inside of one disc: its
    // PARENT is the disc root the engine wants (bluray://<root>).
    if path.is_dir() {
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_ascii_lowercase())
            .unwrap_or_default();
        if dir_name == "bdmv" || dir_name == "video_ts" {
            if let Some(root) = path.parent() {
                let title = root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(title.as_str())
                    .to_string();
                tracing::info!(
                    "resolve_playback: {} folder {} redirected to disc root {}",
                    dir_name,
                    path.display(),
                    root.display()
                );
                return Ok(ResolvedPlayback {
                    resolved_path: root.display().to_string(),
                    title,
                    is_disc: true,
                    external: false,
                });
            }
        }
    }

    // A folder holding one or more DISC FOLDERS (a multi-disc release
    // imported from disk): resolve to the first disc folder; the player's
    // disc switcher discovers the siblings by walking up from there.
    if path.is_dir() {
        let mut disc_dirs: Vec<PathBuf> = std::fs::read_dir(path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|d| d.is_dir())
                    .filter(|d| {
                        d.join("BDMV").is_dir()
                            || d.join("bdmv").is_dir()
                            || d.join("VIDEO_TS").is_dir()
                            || d.join("video_ts").is_dir()
                    })
                    .collect()
            })
            .unwrap_or_default();
        disc_dirs.sort();
        if let Some(disc) = disc_dirs.first() {
            let title = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(title.as_str())
                .to_string();
            tracing::info!(
                "resolve_playback: multi-disc folder {} -> first disc {}",
                path.display(),
                disc.display()
            );
            return Ok(ResolvedPlayback {
                resolved_path: disc.display().to_string(),
                title,
                is_disc: true,
                external: false,
            });
        }
    }

    // ISO disc image: a file that CONTAINS a disc. VLC's demuxers detect
    // the image themselves (media_new_path is enough); marking it as a disc
    // gives it menu handling and the demux liveness check. A folder that
    // holds a single ISO resolves to that ISO (the folder picker only
    // chooses folders).
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if path.is_file() && ext == "iso" {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(title.as_str())
            .to_string();
        tracing::info!(
            "resolve_playback: ISO image {} treated as disc",
            path.display()
        );
        return Ok(ResolvedPlayback {
            resolved_path: file_path.to_string(),
            title,
            is_disc: true,
            external: false,
        });
    }
    if path.is_dir() {
        let isos: Vec<PathBuf> = std::fs::read_dir(path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("iso"))
                                .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if isos.len() == 1 {
            let iso = &isos[0];
            let title = iso
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(title.as_str())
                .to_string();
            tracing::info!(
                "resolve_playback: folder {} holds one ISO, using {}",
                path.display(),
                iso.display()
            );
            return Ok(ResolvedPlayback {
                resolved_path: iso.display().to_string(),
                title,
                is_disc: true,
                external: false,
            });
        }
    }

    Ok(ResolvedPlayback {
        resolved_path: file_path.to_string(),
        title,
        is_disc: false,
        external: false,
    })
}

/// Diagnostic helper (env-gated): NEBULAE_AUTOPLAY=<path> starts playback
/// automatically once the app launches, letting the player be exercised
/// from a terminal where VLC's stderr is visible.
pub fn maybe_autoplay(app: &tauri::AppHandle) {
    if let Ok(path) = std::env::var("NEBULAE_AUTOPLAY") {
        let app = app.clone();
        std::thread::spawn(move || {
            use tauri::Manager;
            std::thread::sleep(std::time::Duration::from_secs(3));
            tracing::info!("autoplay diag: opening {path}");
            let Some(state) = app.try_state::<player::PlayerState>() else {
                tracing::error!("autoplay diag: no player state");
                return;
            };
            match start_playback(&app, &state, &path, false) {
                Ok(resolved) => {
                    // Record the same session bookkeeping player_open does,
                    // so the frontend's engine poll can adopt the session.
                    *poison(&state.current_path) = Some(path.clone());
                    *poison(&state.current_resolved) = Some(resolved.resolved_path.clone());
                }
                Err(e) => tracing::error!("autoplay diag failed: {e}"),
            }
        });
    }
}

fn loaded_lib(state: &PlayerState) -> Result<std::sync::Arc<player::engine::VlcLib>, String> {
    let mut guard = state.lib.lock().map_err(|_| "player state poisoned")?;
    if guard.is_none() {
        *guard = Some(std::sync::Arc::new(player::engine::VlcLib::load()?));
    }
    Ok(guard.as_ref().unwrap().clone())
}

use std::ffi::CStr;

fn poison<T>(g: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    g.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stop the live session (if any) and return the NEW session generation.
///
/// The generation is strictly monotonic (every open stops first, every close
/// stops), so callers can use it as a fence: any engine callback carrying a
/// generation at or below the returned value belongs to a session that has
/// already been torn down.
pub fn stop_session(app: &tauri::AppHandle, state: &PlayerState) -> u64 {
    // Restore presentation options, geometry, and blackout displays if fullscreen was active
    player::leave_fullscreen_if_active(app);

    // Invalidate every background helper belonging to the outgoing session
    // BEFORE touching the player pointer.
    let gen = state
        .session_gen
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        .wrapping_add(1) as u64;
    tracing::info!("[lifecycle-diag] stop_session: session_gen={gen} (session teardown)");
    // Reset poll diagnostic tracker for fresh session
    *poison(&state.last_logged_poll_state) = None;
    // Stop advertising the session IMMEDIATELY: the frontend's readiness
    // poll must not be able to capture (and later adopt) the session while
    // the slow teardown below (VLC stop can take ~1s, the overlay hide
    // waits on the main thread) is still running.
    *poison(&state.started) = false;
    *poison(&state.current_resolved) = None;
    let mp_guard = match state.media_player_cell.lock() {
        Ok(g) => g,
        Err(_) => return gen,
    };
    let mp = *poison(&state.media_player);
    if mp == 0 {
        // No live player; the liveness flags were already cleared above.
        return gen;
    }
    // mp_guard IS the player-pointer guard for the whole teardown: background
    // helpers (menu watcher) can never call into the player while (or after)
    // it is released.
    if let Ok(lib_guard) = state.lib.lock() {
        if let Some(lib) = lib_guard.as_ref() {
            let session = VlcSession {
                instance: *poison(&state.instance),
                media: *poison(&state.media),
                media_player: mp,
            };
            // Close the player first (detaching Direct3D/HWND on Windows) before hiding the overlay
            session.close(lib);
            let _ = player::set_overlay_hidden(app, true);
        }
    }
    *poison(&state.instance) = 0;
    *poison(&state.media) = 0;
    *poison(&state.media_player) = 0;
    poison(&state.framing).reset();
    drop(mp_guard);
    // The EDR screen-change observer is intentionally NOT removed here: the
    // overlay view and the window outlive the session, and re-registration
    // never happens (ensure_overlay reuses the view), so removing it disabled
    // screen-change EDR updates for every later session.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    state
        .edr_applied
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state
        .bypass_menu
        .store(false, std::sync::atomic::Ordering::SeqCst);

    use tauri::Emitter;
    let _ = app.emit("player://closed", ());
    let _ = app.emit("player-closed", ());
    gen
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
    resume: bool,
) -> Result<ResolvedPlayback, String> {
    start_playback_inner(app, state, file_path, resume, None, false)
}

/// Inner implementation; `explicit_feature` bypasses the disc-file redirect
/// in `resolve_playback` so an explicit resume target is never sent back to
/// the disc root.
fn start_playback_inner(
    app: &tauri::AppHandle,
    state: &PlayerState,
    file_path: &str,
    resume: bool,
    explicit_feature: Option<&str>,
    bypass_menu: bool,
) -> Result<ResolvedPlayback, String> {
    state
        .bypass_menu
        .store(bypass_menu, std::sync::atomic::Ordering::SeqCst);
    let resolved = if let Some(feature) = explicit_feature {
        // Explicit resume: the caller already resolved the feature file.
        // Treat it as a plain file path so resolve_playback cannot redirect
        // it back to the disc root, BUT preserve is_disc=true so the
        // frontend's seekWhenFeatureStarts branch runs (openInPlayer checks
        // info.is_disc && resume && seekTo to invoke the seek poller).
        let path = Path::new(feature);
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Video")
            .to_string();
        tracing::info!(
            "player_open: resume using explicit feature '{}' (bypassing disc redirect)",
            feature
        );
        ResolvedPlayback {
            resolved_path: feature.to_string(),
            title,
            is_disc: true,
            external: false,
        }
    } else {
        match resolve_playback(file_path) {
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
            match VlcSession::open_shared(
                &lib,
                &state.shared_instance,
                &resolved.resolved_path,
                view,
                bypass_menu,
            ) {
                Ok(session) => {
                    // Record session pointers immediately so stop_session and
                    // pollers are immediately aware of the live media player.
                    *poison(&state.instance) = session.instance;
                    *poison(&state.media) = session.media;
                    *poison(&state.media_player) = session.media_player;
                    *poison(&state.started) = true;
                    poison(&state.framing).reset();

                    // HDR: detect UHD / HDR content from disc metadata or file path,
                    // and apply extended-dynamic-range headroom and BT.2100 PQ colorspace
                    // to the VLC-owned video layer (CAOpenGLLayer).
                    let is_uhd_disc = if resolved.is_disc {
                        crate::player::bluray_meta::is_disc_uhd_or_hdr(&resolved.resolved_path)
                    } else {
                        false
                    };
                    let is_hdr_file = {
                        let p_lower = resolved.resolved_path.to_ascii_lowercase();
                        p_lower.contains("2160p")
                            || p_lower.contains("uhd")
                            || p_lower.contains("hdr")
                    };
                    state.is_hdr.store(
                        is_uhd_disc || is_hdr_file,
                        std::sync::atomic::Ordering::SeqCst,
                    );

                    #[cfg(any(target_os = "macos", target_os = "windows"))]
                    {
                        state
                            .edr_applied
                            .store(false, std::sync::atomic::Ordering::SeqCst);
                        maybe_apply_edr(app);

                        // EDR retry worker: the video layer is created asynchronously
                        // after play() (CAOpenGLLayer on macOS, Direct3D HWND vout on
                        // Windows). Poll at a tight 30ms cadence for up to 3s so EDR and
                        // tone-mapping are latched immediately upon layer creation,
                        // preventing washed-out presentation.
                        let app_edr = app.clone();
                        let edr_gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                        std::thread::spawn(move || {
                            use tauri::Manager;
                            for _ in 0..100 {
                                std::thread::sleep(std::time::Duration::from_millis(30));
                                let state = app_edr.state::<PlayerState>();
                                let gen_now =
                                    state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                if gen_now != edr_gen || !*poison(&state.started) {
                                    return;
                                }
                                if state.edr_applied.load(std::sync::atomic::Ordering::SeqCst) {
                                    return;
                                }
                                maybe_apply_edr(&app_edr);
                            }
                        });
                    }

                    // Disc-mode liveness check: bluray:// (and dvd://) MRLs
                    // can fail inside the demuxer while libvlc_media_player_play
                    // still reports success (reproduced: black screen, no
                    tracing::info!(
                        "player_open: PLAYING '{}' (disc: {})",
                        resolved.resolved_path,
                        resolved.is_disc
                    );
                    if resolved.is_disc && !resume {
                        if !bypass_menu {
                            // AUTHORED MENU WATCHER:
                            // On normal disc Open, commercial Blu-rays/UHDs often execute First Play
                            // which chains directly into Title 1 (Play All: e.g. Fuuka D1 13,546s)
                            // unless an interactive menu call is dispatched.
                            // We poll at a tight 20ms cadence for demuxer readiness, dispatch
                            // navigate_menu immediately, and if First Play masks menu calls
                            // (menu_call_mask=1, e.g. Patlabor 2 UHD 76s logo chain), seek to the
                            // end of First Play (0.99) so it triggers its authored JumpTitle 0 into Top Menu.
                            let app_menu = app.clone();
                            let menu_gen =
                                state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                            let menu_mp = session.media_player;
                            std::thread::spawn(move || {
                                use tauri::Manager;
                                let max_wait_attempts = 150; // 150 * 20ms = 3s
                                let mut ready = false;
                                for _ in 0..max_wait_attempts {
                                    std::thread::sleep(std::time::Duration::from_millis(20));
                                    let state = app_menu.state::<PlayerState>();
                                    let gen_now =
                                        state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                    if gen_now != menu_gen || !*poison(&state.started) {
                                        return;
                                    }
                                    let mp = *poison(&state.media_player);
                                    if mp == 0 || mp != menu_mp {
                                        return;
                                    }
                                    let lib = match loaded_lib(&state) {
                                        Ok(l) => l,
                                        Err(_) => return,
                                    };
                                    let count = unsafe {
                                        (lib.libvlc_media_player_get_title_count)(mp as *mut c_void)
                                    };
                                    if count > 0 {
                                        ready = true;
                                        break;
                                    }
                                }
                                if !ready {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): titles not ready within 3s; exiting"
                                );
                                    return;
                                }

                                let state = app_menu.state::<PlayerState>();
                                let lib = match loaded_lib(&state) {
                                    Ok(l) => l,
                                    Err(_) => return,
                                };
                                let mp = *poison(&state.media_player);
                                if mp == 0 || mp != menu_mp {
                                    return;
                                }

                                // Title/menu evidence: log once when titles first become ready
                                unsafe {
                                    let title_count = (lib.libvlc_media_player_get_title_count)(
                                        mp as *mut c_void,
                                    );
                                    let cur_title =
                                        (lib.libvlc_media_player_get_title)(mp as *mut c_void);
                                    let has_menu = engine::has_menu_title(&lib, mp);
                                    let is_menu = engine::is_menu_title(&lib, mp);
                                    tracing::info!(
                                    "[title-diag] session_gen={menu_gen} titles ready: count={title_count} cur_title={cur_title} has_menu_title={has_menu} is_menu_title={is_menu}"
                                );
                                    let mut titles_ptr: *mut *mut engine::VlcTitleDescription =
                                        std::ptr::null_mut();
                                    let n = (lib.libvlc_media_player_get_full_title_descriptions)(
                                        mp as *mut c_void,
                                        &mut titles_ptr,
                                    );
                                    if n > 0 && !titles_ptr.is_null() {
                                        for idx in 0..n {
                                            let t = &**titles_ptr.add(idx as usize);
                                            let name = if !t.psz_name.is_null() {
                                                std::ffi::CStr::from_ptr(t.psz_name)
                                                    .to_string_lossy()
                                                    .into_owned()
                                            } else {
                                                "none".to_string()
                                            };
                                            let flags = t.i_flags;
                                            let is_menu_flag =
                                                (flags & engine::INPUT_TITLE_MENU) != 0;
                                            let is_interactive =
                                                (flags & engine::INPUT_TITLE_INTERACTIVE) != 0;
                                            tracing::info!(
                                            "[title-diag] session_gen={menu_gen} title[{idx}]: name='{name}' duration_ms={} flags=0x{flags:02x} (MENU={is_menu_flag}, INTERACTIVE={is_interactive})",
                                            t.i_duration
                                        );
                                        }
                                        (lib.libvlc_title_descriptions_release)(titles_ptr, n);
                                    }
                                }

                                // Outcome A: If already at menu (e.g. DVD VMGM or discs starting directly at menu), done.
                                if unsafe { engine::is_menu_title(&lib, mp) } {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): disc already at authored menu [Outcome A]"
                                );
                                    return;
                                }

                                // Outcome B: If already at confirmed active feature playback, preserve it and done.
                                if unsafe { engine::is_active_feature_playback(&lib, mp) } {
                                    let cur = unsafe {
                                        (lib.libvlc_media_player_get_title)(mp as *mut c_void)
                                    };
                                    let len = unsafe {
                                        (lib.libvlc_media_player_get_length)(mp as *mut c_void)
                                    };
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): disc already at active feature playback on title {cur} (length_ms={len}) [Outcome B]; preserving playback and exiting"
                                );
                                    return;
                                }

                                let nav_menu = {
                                    let lib_clone = lib.clone();
                                    move || unsafe {
                                        if let Some(navigate_menu) =
                                            lib_clone.libvlc_media_player_navigate_menu
                                        {
                                            navigate_menu(mp as *mut c_void);
                                        } else {
                                            tracing::warn!(
                                            "[menu-diag] navigate_menu symbol unavailable on this libvlc build"
                                        );
                                        }
                                    }
                                };

                                tracing::info!(
                                "[menu-diag] menu watcher (session_gen={menu_gen}): dispatching initial INPUT_NAV_MENU"
                            );
                                nav_menu();

                                // Check if navigate_menu or natural flow reached menu or feature within 200ms (10 * 20ms)
                                for _ in 0..10 {
                                    std::thread::sleep(std::time::Duration::from_millis(20));
                                    let gen_now =
                                        state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                    if gen_now != menu_gen || !*poison(&state.started) {
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): aborted during initial 200ms check"
                                    );
                                        return;
                                    }
                                    let mp = *poison(&state.media_player);
                                    if mp == 0 || mp != menu_mp {
                                        return;
                                    }
                                    if unsafe { engine::is_menu_title(&lib, mp) } {
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): disc reached authored menu via navigate_menu within 200ms [Outcome A]"
                                    );
                                        return;
                                    }
                                    if unsafe { engine::is_active_feature_playback(&lib, mp) } {
                                        let cur = unsafe {
                                            (lib.libvlc_media_player_get_title)(mp as *mut c_void)
                                        };
                                        let len = unsafe {
                                            (lib.libvlc_media_player_get_length)(mp as *mut c_void)
                                        };
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): feature playback active on title {cur} (length_ms={len}) within 200ms [Outcome B]; preserving playback"
                                    );
                                        return;
                                    }
                                }

                                // On discs where First Play masks menu calls (menu_call_mask=1, e.g. Patlabor 2 UHD),
                                // navigate_menu is rejected by libbluray. If the disc has an authored menu title
                                // and we are currently in First Play / short intro (<= 300s), seek to the end of
                                // First Play (0.99) so it finishes and triggers its authored JumpTitle 0 into Top Menu.
                                let has_menu = unsafe { engine::has_menu_title(&lib, mp) };
                                let is_fp = unsafe { engine::is_first_play_or_intro(&lib, mp) };
                                let is_feat =
                                    unsafe { engine::is_active_feature_playback(&lib, mp) };
                                if has_menu && is_fp && !is_feat {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): First Play seek 0.99 APPLIED (has_menu=true, is_fp=true)"
                                );
                                    unsafe {
                                        (lib.libvlc_media_player_set_position)(
                                            mp as *mut c_void,
                                            0.99,
                                        );
                                    }
                                } else {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): First Play seek 0.99 SKIPPED (has_menu={has_menu}, is_fp={is_fp}, is_feat={is_feat})"
                                );
                                }

                                // Poll up to 15s (75 * 200ms).
                                for i in 0..75 {
                                    std::thread::sleep(std::time::Duration::from_millis(200));
                                    let gen_now =
                                        state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                    if gen_now != menu_gen || !*poison(&state.started) {
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): cancelled at iteration {i} (session ended/changed)"
                                    );
                                        return;
                                    }
                                    let mp = *poison(&state.media_player);
                                    if mp == 0 || mp != menu_mp {
                                        return;
                                    }

                                    // Outcome A: Authored menu reached safely
                                    if unsafe { engine::is_menu_title(&lib, mp) } {
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): disc reached authored menu at iteration {i} (T+{}ms) [Outcome A]",
                                        i * 200
                                    );
                                        return;
                                    }

                                    // Outcome B: Confirmed active feature playback engaged
                                    if unsafe { engine::is_active_feature_playback(&lib, mp) } {
                                        let cur = unsafe {
                                            (lib.libvlc_media_player_get_title)(mp as *mut c_void)
                                        };
                                        let len = unsafe {
                                            (lib.libvlc_media_player_get_length)(mp as *mut c_void)
                                        };
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): feature playback confirmed active on title {cur} (length_ms={len}) at iteration {i} (T+{}ms) [Outcome B]; stopping startup watcher",
                                        i * 200
                                    );
                                        return;
                                    }

                                    // If introductory logos mask menu calls, retry INPUT_NAV_MENU every 2s.
                                    // NOTE: Do NOT repeatedly force set_title(menu_title) over active content.
                                    if i > 0 && i % 10 == 0 {
                                        tracing::info!(
                                        "[menu-diag] menu watcher (session_gen={menu_gen}): retrying INPUT_NAV_MENU at iteration {i}"
                                    );
                                        nav_menu();
                                    }
                                }

                                // Bounded startup window (15s) elapsed.
                                let gen_now =
                                    state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                if gen_now != menu_gen || !*poison(&state.started) {
                                    return;
                                }
                                let mp = *poison(&state.media_player);
                                if mp == 0 || mp != menu_mp {
                                    return;
                                }

                                let is_playing = unsafe {
                                    (lib.libvlc_media_player_is_playing)(mp as *mut c_void) != 0
                                };
                                let cur_title = unsafe {
                                    (lib.libvlc_media_player_get_title)(mp as *mut c_void)
                                };
                                let cur_len = unsafe {
                                    (lib.libvlc_media_player_get_length)(mp as *mut c_void)
                                };

                                if unsafe { engine::is_menu_title(&lib, mp) } {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): disc at authored menu at 15s exit [Outcome A]"
                                );
                                    return;
                                }

                                if is_playing
                                    && (cur_len >= 120_000
                                        || unsafe { engine::is_active_feature_playback(&lib, mp) })
                                {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): feature playback active on title {cur_title} (length_ms={cur_len}) at 15s exit [Outcome B]; preserving playback"
                                );
                                    return;
                                }

                                // Outcome C: Menu startup did not complete and no usable playback is active.
                                // Use safe bounded main-feature fallback: reopen the disc with bypass_menu=true.
                                let disc_path = poison(&state.current_resolved)
                                    .clone()
                                    .or_else(|| poison(&state.current_path).clone());
                                if let Some(path) = disc_path {
                                    tracing::info!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): menu startup incomplete on static/intro title {cur_title}; falling back to direct main-feature playback [Outcome C]"
                                );
                                    let app_fallback = app_menu.clone();
                                    tauri::async_runtime::spawn(async move {
                                        use tauri::Manager;
                                        let s = app_fallback.state::<PlayerState>();
                                        let _ = player_open_inner(
                                            &app_fallback,
                                            &s,
                                            path,
                                            false,
                                            None,
                                            true,
                                        )
                                        .await;
                                    });
                                } else {
                                    tracing::warn!(
                                    "[menu-diag] menu watcher (session_gen={menu_gen}): exited after 15s without authored menu and no disc path found [Outcome None]; leaving recoverable player state"
                                );
                                }
                            });

                            // DISC WARNING SKIP WATCHER:
                            // For discs that play a mandatory warning clip after the user selects
                            // Play from the authored top menu (e.g. Summer Wars FBI warning:
                            // PLAYLIST/00100.mpls, ~9 s), once the warning has genuinely played
                            // through once, persist a flag so subsequent sessions skip past it.
                            //
                            // Actual sequence handled:
                            //   Disc opens → is_menu_title true (top menu) →
                            //   user selects Play → playlist_id changes to warning MPLS →
                            //   warning plays to near-end → playlist_id changes to feature MPLS →
                            //   flag written.
                            //
                            // Identity: (disc_sha1, warn_mpls_id, warn_mpls_size).
                            //   disc_sha1     = SHA-1 of BDMV/MovieObject.bdmv
                            //   warn_mpls_id  = MPLS number of the warning clip
                            //   warn_mpls_size = byte size of that MPLS file at record time
                            // All three must match on a subsequent session for the skip to fire.
                            //
                            // Detection: libvlc_media_player_get_bluray_playlist_id() — custom
                            // export from patches/vlc-3.0.23/bluray-playlist-id.patch.
                            // Absent export → watcher exits immediately; no effect.
                            //
                            // Recording state machine (RECORD MODE):
                            //   1. Wait up to 15 s for is_menu_title == true (top menu active).
                            //      Not a menu-first disc → exit without recording.
                            //   2. Wait for is_menu_title == false (user selects Play).
                            //      First playlist_id while is_menu_title is false = warning candidate.
                            //   3. Track position on candidate. On playlist_id change:
                            //      - is_menu_title true → user backed to menu; do NOT write flag.
                            //      - Different non-menu playlist_id AND saw_near_end → write flag.
                            //      - Different non-menu playlist_id AND NOT saw_near_end → exit, no flag.
                            //
                            // Skip (SKIP MODE):
                            //   When playlist_id == recorded warn_mpls_id (and MPLS size matches),
                            //   call set_position(0.99) once. Disc HDMV VM handles transition.
                            if resolved.is_disc {
                                let app_warn = app.clone();
                                let warn_gen =
                                    state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                                let warn_mp = session.media_player;
                                let warn_disc_path = resolved.resolved_path.clone();
                                std::thread::spawn(move || {
                                    use tauri::Manager;

                                    // Require the custom bluray-playlist-id export.
                                    let state = app_warn.state::<PlayerState>();
                                    let lib = match loaded_lib(&state) {
                                        Ok(l) => l,
                                        Err(_) => return,
                                    };
                                    let get_playlist_id = match lib
                                        .libvlc_media_player_get_bluray_playlist_id
                                    {
                                        Some(f) => f,
                                        None => {
                                            tracing::debug!(
                                            "[warn-watcher] libvlc_media_player_get_bluray_playlist_id \
                                             not present — warning-skip feature inactive"
                                        );
                                            return;
                                        }
                                    };

                                    // Resolve disc root from resolved_path.
                                    let disc_root = {
                                        let p = Path::new(&warn_disc_path);
                                        if p.join("BDMV").is_dir()
                                            || p.file_name().and_then(|n| n.to_str())
                                                == Some("BDMV")
                                        {
                                            p.to_path_buf()
                                        } else if p
                                            .parent()
                                            .map(|pp| pp.join("BDMV").is_dir())
                                            .unwrap_or(false)
                                        {
                                            p.parent().unwrap().to_path_buf()
                                        } else {
                                            return;
                                        }
                                    };

                                    let bdmv_dir = disc_root.join("BDMV");
                                    let movieobject_path = bdmv_dir.join("MovieObject.bdmv");
                                    if !movieobject_path.exists() {
                                        return;
                                    }
                                    let disc_sha1 = match sha1_file(&movieobject_path) {
                                        Some(s) => s,
                                        None => return,
                                    };

                                    // Session/generation fence macro.
                                    // Returns current mp pointer, or returns from thread if stale.
                                    macro_rules! mp_or_return {
                                        () => {{
                                            let s = app_warn.state::<PlayerState>();
                                            let g = s
                                                .session_gen
                                                .load(std::sync::atomic::Ordering::SeqCst);
                                            if g != warn_gen || !*poison(&s.started) {
                                                return;
                                            }
                                            let mp = *poison(&s.media_player);
                                            if mp == 0 || mp != warn_mp {
                                                return;
                                            }
                                            mp
                                        }};
                                    }

                                    // Find any existing flag for this disc.
                                    // Key: disc_sha1 only (at most one warning entry per disc).
                                    let warns = read_disc_warns(&app_warn);
                                    let existing_entry: Option<DiscWarnEntry> = warns
                                        .iter()
                                        .find(|e| e.disc_sha1 == disc_sha1 && e.warn_seen)
                                        .cloned();

                                    if let Some(entry) = existing_entry {
                                        // ---- SKIP MODE ----
                                        // Verify MPLS file still has the recorded size.
                                        let playlist_path = bdmv_dir
                                            .join("PLAYLIST")
                                            .join(format!("{:05}.mpls", entry.warn_mpls_id));
                                        let actual_size = std::fs::metadata(&playlist_path)
                                            .map(|m| m.len())
                                            .unwrap_or(0);
                                        if actual_size == 0 || actual_size != entry.warn_mpls_size {
                                            tracing::warn!(
                                                "[warn-watcher] skip: MPLS {:05} size mismatch \
                                             (recorded {}, on-disc {}) — not skipping",
                                                entry.warn_mpls_id,
                                                entry.warn_mpls_size,
                                                actual_size
                                            );
                                            return;
                                        }
                                        tracing::info!(
                                            "[warn-watcher] skip mode: disc_sha1={} \
                                         warning={:05} ({}B)",
                                            disc_sha1,
                                            entry.warn_mpls_id,
                                            entry.warn_mpls_size
                                        );
                                        // Poll up to 30 s (600 × 50 ms) for the warning playlist.
                                        let mut skipped = false;
                                        for _ in 0..600 {
                                            std::thread::sleep(std::time::Duration::from_millis(
                                                50,
                                            ));
                                            let mp = mp_or_return!();
                                            let playlist_id = unsafe {
                                                get_playlist_id(mp as *mut c_void) as i32
                                            };
                                            if playlist_id == entry.warn_mpls_id as i32 {
                                                tracing::info!(
                                                    "[warn-watcher] skip: warning playlist {:05} \
                                                 active — seeking to 0.99",
                                                    entry.warn_mpls_id
                                                );
                                                unsafe {
                                                    (lib.libvlc_media_player_set_position)(
                                                        mp as *mut c_void,
                                                        0.99,
                                                    );
                                                }
                                                skipped = true;
                                                break;
                                            }
                                        }
                                        if !skipped {
                                            tracing::info!(
                                            "[warn-watcher] skip: warning playlist {:05} never \
                                             became active within 30 s",
                                            entry.warn_mpls_id
                                        );
                                        }
                                    } else {
                                        // ---- RECORD MODE ----

                                        // Step 1: wait up to 15 s for the top menu (is_menu_title).
                                        let mut saw_menu = false;
                                        for _ in 0..300 {
                                            std::thread::sleep(std::time::Duration::from_millis(
                                                50,
                                            ));
                                            let mp = mp_or_return!();
                                            if unsafe { engine::is_menu_title(&lib, mp) } {
                                                saw_menu = true;
                                                break;
                                            }
                                        }
                                        if !saw_menu {
                                            tracing::info!(
                                                "[warn-watcher] record: top menu not seen in 15 s \
                                             — not a menu-first disc; exiting"
                                            );
                                            return;
                                        }
                                        tracing::info!(
                                            "[warn-watcher] record: top menu confirmed \
                                         (disc_sha1={}); waiting for Play selection",
                                            disc_sha1
                                        );

                                        // Step 2+3: track first post-menu playlist as warning candidate.
                                        let mut warn_candidate: Option<u32> = None;
                                        let mut saw_near_end = false;

                                        for _ in 0..2400 {
                                            // up to 120 s
                                            std::thread::sleep(std::time::Duration::from_millis(
                                                50,
                                            ));
                                            let mp = mp_or_return!();

                                            let playlist_id = unsafe {
                                                get_playlist_id(mp as *mut c_void) as i32
                                            };
                                            let at_menu =
                                                unsafe { engine::is_menu_title(&lib, mp) };

                                            if playlist_id < 0 {
                                                continue;
                                            }

                                            if warn_candidate.is_none() {
                                                // Waiting for menu exit.
                                                if at_menu {
                                                    continue; // still at menu
                                                }
                                                // Menu just exited; this playlist is the candidate.
                                                warn_candidate = Some(playlist_id as u32);
                                                tracing::info!(
                                                    "[warn-watcher] record: menu exited; \
                                                 tracking warning candidate {:05}",
                                                    playlist_id
                                                );
                                            } else if let Some(wid) = warn_candidate {
                                                if playlist_id == wid as i32 {
                                                    // Still on warning candidate.
                                                    if !at_menu {
                                                        let total = unsafe {
                                                            (lib.libvlc_media_player_get_length)(
                                                                mp as *mut c_void,
                                                            )
                                                        };
                                                        let cur = unsafe {
                                                            (lib.libvlc_media_player_get_time)(
                                                                mp as *mut c_void,
                                                            )
                                                        };
                                                        if total > 0 && cur > 0 {
                                                            let frac = cur as f64 / total as f64;
                                                            if frac > 0.80 {
                                                                saw_near_end = true;
                                                            }
                                                        }
                                                    }
                                                } else if at_menu {
                                                    // User backed to menu — no flag.
                                                    tracing::info!(
                                                        "[warn-watcher] record: returned to menu \
                                                     from candidate {:05} — \
                                                     not completed; flag not written",
                                                        wid
                                                    );
                                                    return;
                                                } else {
                                                    // Playlist changed to a different non-menu value.
                                                    if saw_near_end {
                                                        tracing::info!(
                                                        "[warn-watcher] record: candidate {:05} \
                                                         transitioned to {:05} after near-end \
                                                         — writing flag",
                                                        wid,
                                                        playlist_id
                                                    );
                                                        let playlist_path = bdmv_dir
                                                            .join("PLAYLIST")
                                                            .join(format!("{:05}.mpls", wid));
                                                        let mpls_size =
                                                            std::fs::metadata(&playlist_path)
                                                                .map(|m| m.len())
                                                                .unwrap_or(0);
                                                        if mpls_size > 0 {
                                                            write_disc_warn(
                                                                &app_warn, &disc_sha1, wid,
                                                                mpls_size,
                                                            );
                                                        } else {
                                                            tracing::warn!(
                                                            "[warn-watcher] record: MPLS {:05} \
                                                             not found — flag not written",
                                                            wid
                                                        );
                                                        }
                                                    } else {
                                                        tracing::info!(
                                                        "[warn-watcher] record: candidate {:05} \
                                                         transitioned before near-end \
                                                         — flag not written",
                                                        wid
                                                    );
                                                    }
                                                    return;
                                                }
                                            }
                                        }
                                        tracing::info!(
                                            "[warn-watcher] record: 120 s elapsed without \
                                         completion — no flag written"
                                        );
                                    }
                                });
                            }
                        } else {
                            // Direct main-feature playback: select normal audio track in background
                            let audio_gen =
                                state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                            let audio_mp = session.media_player;
                            let lib_audio = lib.clone();
                            tokio::task::spawn_blocking(move || {
                                select_normal_audio_track(&lib_audio, audio_mp, audio_gen);
                            });
                        }

                        // Deferred liveness check: detect silent decryption failure.
                        // When AACS keys (libaacs + KEYDB.cfg) are absent, the Blu-ray
                        // demuxer opens the BDMV but cannot decrypt stream data.
                        // libvlc_media_player_play() returns 0 (success) and the session
                        // struct is valid, but get_time stays at -1 because no frames are
                        // decoded. This produces the black-screen / 0:00:00 symptom.
                        // Emit a user-facing error event after 6 seconds if still frozen.
                        // Session-scoped: the probe captures the session generation and
                        // the player pointer at spawn and acts only if both are unchanged.
                        let app3 = app.clone();
                        let spawn_gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                        let spawn_mp = session.media_player;
                        let spawn_time = std::time::Instant::now();
                        std::thread::spawn(move || {
                            use tauri::{Emitter, Manager};
                            std::thread::sleep(std::time::Duration::from_secs(6));
                            let elapsed = spawn_time.elapsed().as_millis();
                            let state = app3.state::<PlayerState>();
                            let gen_now =
                                state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
                            if gen_now != spawn_gen || !*poison(&state.started) {
                                tracing::info!(
                                    "[liveness-diag] session_gen={spawn_gen} check cancelled (session ended/changed, gen_now={gen_now}, elapsed={elapsed}ms)"
                                );
                                return; // session replaced or closed
                            }
                            let mp = *poison(&state.media_player);
                            if mp == 0 || mp != spawn_mp {
                                tracing::info!(
                                    "[liveness-diag] session_gen={spawn_gen} check cancelled (media_player changed, elapsed={elapsed}ms)"
                                );
                                return;
                            }
                            let lib = match loaded_lib(&state) {
                                Ok(l) => l,
                                Err(_) => return,
                            };
                            let (ms, length, is_playing, cur_title, title_count) = unsafe {
                                let m = mp as *mut c_void;
                                (
                                    (lib.libvlc_media_player_get_time)(m),
                                    (lib.libvlc_media_player_get_length)(m),
                                    (lib.libvlc_media_player_is_playing)(m) != 0,
                                    (lib.libvlc_media_player_get_title)(m),
                                    (lib.libvlc_media_player_get_title_count)(m),
                                )
                            };
                            let at_menu = unsafe { engine::is_menu_title(&lib, mp) };
                            let has_menu = unsafe { engine::has_menu_title(&lib, mp) };
                            let backend_err = unsafe {
                                let ptr = (lib.libvlc_errmsg)();
                                if !ptr.is_null() {
                                    std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
                                } else {
                                    "none".to_string()
                                }
                            };

                            let (cur_title_name, cur_title_flags, flag_menu, flag_interactive) = unsafe {
                                let mut titles_ptr: *mut *mut engine::VlcTitleDescription =
                                    std::ptr::null_mut();
                                let n = (lib.libvlc_media_player_get_full_title_descriptions)(
                                    mp as *mut c_void,
                                    &mut titles_ptr,
                                );
                                let info = if n > 0
                                    && !titles_ptr.is_null()
                                    && cur_title >= 0
                                    && cur_title < n
                                {
                                    let t = &**titles_ptr.add(cur_title as usize);
                                    let name = if !t.psz_name.is_null() {
                                        std::ffi::CStr::from_ptr(t.psz_name)
                                            .to_string_lossy()
                                            .into_owned()
                                    } else {
                                        "none".to_string()
                                    };
                                    let f = t.i_flags;
                                    (
                                        name,
                                        f,
                                        (f & engine::INPUT_TITLE_MENU) != 0,
                                        (f & engine::INPUT_TITLE_INTERACTIVE) != 0,
                                    )
                                } else {
                                    ("unknown".to_string(), 0, false, false)
                                };
                                if n > 0 && !titles_ptr.is_null() {
                                    (lib.libvlc_title_descriptions_release)(titles_ptr, n);
                                }
                                info
                            };

                            tracing::info!(
                                "[liveness-diag] session_gen={spawn_gen} decision at elapsed={elapsed}ms: get_time={ms} length={length} is_playing={is_playing} title={cur_title}/{title_count} ('{cur_title_name}', flags=0x{cur_title_flags:02x}, MENU={flag_menu}, INTERACTIVE={flag_interactive}) at_menu={at_menu} has_menu_title={has_menu} backend_err='{backend_err}'"
                            );

                            if ms == -1 {
                                if at_menu {
                                    tracing::info!(
                                        "[liveness-diag] session_gen={spawn_gen}: at disc menu, get_time=-1 is expected (player-error NOT emitted)"
                                    );
                                } else {
                                    tracing::error!(
                                        "[liveness-diag] session_gen={spawn_gen} LIVENESS FAILED: get_time=-1, at_menu=false after {elapsed}ms. Emitting nebulae://player-error (reason: get_time=-1 after 6s on non-menu title). Backend error: '{backend_err}'"
                                    );
                                    let msg = "Playback did not start. \
                                        This disc may be encrypted (AACS copy protection). \
                                        Install libaacs and a valid KEYDB.cfg, then try again. \
                                        If the disc is not encrypted, check that the BDMV folder is readable.";
                                    let _ = app3.emit("nebulae://player-error", msg);
                                }
                            } else {
                                tracing::info!(
                                    "[liveness-diag] session_gen={spawn_gen} LIVENESS OK: get_time={ms} ms after {elapsed}ms"
                                );
                            }
                        });
                    }
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
pub async fn player_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, PlayerState>,
    file_path: String,
    resume: Option<bool>,
) -> Result<PlayerOpenResult, String> {
    let resume = resume.unwrap_or(false);
    player_open_inner(&app, &state, file_path, resume, None, false).await
}

pub async fn player_open_inner(
    app: &tauri::AppHandle,
    state: &PlayerState,
    file_path: String,
    resume: bool,
    explicit_feature: Option<String>,
    bypass_menu: bool,
) -> Result<PlayerOpenResult, String> {
    let _open_guard = state.open_lock.lock().await;
    tracing::info!(
        "[lifecycle-diag] player_open entry: path='{file_path}' (resume={resume}, bypass_menu={bypass_menu})"
    );
    stop_session(app, state);
    *poison(&state.current_path) = Some(file_path.clone());
    // An explicit resume must not sit through the disc's first-play flow
    // (FBI warning → menu chain): resolve the disc's main feature file and
    // open it as a plain file instead. The Menu button can still return to
    // the interactive menu, which restarts from `current_path` (the disc).
    //
    // IMPORTANT: start_playback calls resolve_playback internally, which
    // redirects any path inside BDMV/ or VIDEO_TS/ back to the disc root.
    // When resuming to a feature file, bypass that redirect by passing the
    // file as explicit_feature to start_playback_inner.
    let (open_target, explicit_feature): (String, Option<String>) = if explicit_feature.is_some() {
        (file_path.clone(), explicit_feature)
    } else if resume {
        match resolve_playback(&file_path) {
            Ok(r) if r.is_disc && !r.external => {
                match find_main_video(Path::new(&r.resolved_path)) {
                    Some(feature) => {
                        let feature_str = feature.display().to_string();
                        tracing::info!(
                            "[resume-diag] player_open: resume targets feature file '{}'",
                            feature_str
                        );
                        (file_path.clone(), Some(feature_str))
                    }
                    None => (file_path.clone(), None),
                }
            }
            _ => (file_path.clone(), None),
        }
    } else {
        (file_path.clone(), None)
    };
    let resolved = match start_playback_inner(
        app,
        state,
        &open_target,
        resume,
        explicit_feature.as_deref(),
        bypass_menu,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                "[lifecycle-diag] player_open failed: path='{open_target}' error='{e}'"
            );
            // Failed open: clear the session bookkeeping so the frontend's
            // engine poll cannot adopt a dead/stale session and present the
            // LAST played disc on a blank overlay.
            *poison(&state.current_path) = None;
            *poison(&state.current_resolved) = None;
            *poison(&state.started) = false;
            return Err(e);
        }
    };
    *poison(&state.current_resolved) = Some(resolved.resolved_path.clone());
    // The generation identifies THIS session: stop_session bumped it at the
    // top of this open, and nothing else bumps it until the next stop.
    let gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst) as u64;
    tracing::info!(
        "[lifecycle-diag] player_open success: session_gen={gen} path='{}' is_disc={} bypass_menu={}",
        resolved.resolved_path,
        resolved.is_disc,
        bypass_menu
    );
    let result = PlayerOpenResult {
        info: resolved,
        gen,
    };
    // Programmatic opens (diagnostics, autoplay) must reach the frontend so
    // the overlay mounts and the shell clears away from the video. The
    // webview may still be loading when the open happens, so the event is
    // repeated a few times; the frontend's playerStore.open is idempotent,
    // and the generation fence drops repeats that outlive a later close.
    use tauri::Emitter;
    let emitter = app.clone();
    let payload = result.clone();
    std::thread::spawn(move || {
        for delay in [0, 1500, 4000, 8000] {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            tracing::info!(
                "[lifecycle-diag] player-opened emitted: session_gen={} delay={}ms path='{}'",
                payload.gen,
                delay,
                payload.info.resolved_path
            );
            let _ = emitter.emit("player-opened", &payload);
        }
    });
    Ok(result)
}
/// What the engine is currently playing (for the frontend's readiness
/// poll): Some when a session is live, None otherwise. This replaces
/// event-based programmatic-open notification, which raced the webview's
/// own loading.
#[derive(Debug, Clone, Serialize)]
pub struct EnginePlayback {
    pub resolved_path: String,
    pub title: String,
    pub is_disc: bool,
    /// Session generation this snapshot belongs to: lets the frontend
    /// discard a response that was captured before/during a user close.
    pub gen: u64,
}

#[tauri::command]
pub fn player_engine_playback(state: tauri::State<PlayerState>) -> Option<EnginePlayback> {
    // Read the generation BEFORE the liveness flag: stop_session bumps the
    // generation first and clears `started` right after, so a snapshot that
    // still reports a live session always carries a generation at or below
    // the one the close returns - the frontend fence drops it.
    let gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst) as u64;
    let started = *poison(&state.started);
    if !started {
        return None;
    }
    let resolved = poison(&state.current_resolved).clone()?;
    let title = poison(&state.current_path)
        .clone()
        .and_then(|p| {
            Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| resolved.clone());
    Some(EnginePlayback {
        resolved_path: resolved,
        title,
        is_disc: true, // only disc sessions are auto-opened engine-side
        gen,
    })
}

/// Begin a native window drag (title-bar mousedown handler).
#[tauri::command]
pub fn window_begin_drag(app: tauri::AppHandle) -> Result<(), String> {
    tracing::info!("[drag] window_begin_drag invoked");
    let r = crate::player::begin_window_drag(&app);
    tracing::info!("[drag] begin_window_drag -> {:?}", r);
    r
}

/// Return DVD/BD playback to its disc root / main menu.
///
/// Dispatches INPUT_NAV_MENU through the new libvlc_media_player_navigate_menu()
/// runtime extension, which reaches:
///   - Blu-ray: bd_menu_call() (Top Menu / Root Menu)
///   - DVD:     dvdnav_menu_call() (Title/Root menu)
///
/// This is a deliberate, direct main-menu operation — not popup navigation
/// (INPUT_NAV_POPUP sends a popup keystroke, a different operation), and not
/// a title-index shortcut (set_title(0) is demuxer-specific and must not be
/// generalized).
///
/// Verification is intentionally NOT done via is_menu_title() immediately
/// after dispatch: a menu flag may coexist with a feature-classified title,
/// no menu flag immediately after dispatch does not prove failure, and
/// navigation may complete asynchronously. The command reports dispatch
/// acceptance, not menu visibility.
#[tauri::command]
pub async fn player_return_to_menu(
    app: tauri::AppHandle,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    if state.bypass_menu.load(std::sync::atomic::Ordering::SeqCst) {
        let path = {
            let _mp_guard = state
                .media_player_cell
                .lock()
                .map_err(|_| "player state poisoned")?;
            let mp = *poison(&state.media_player);
            if mp == 0 || !*poison(&state.started) {
                return Err("no active player session".into());
            }
            poison(&state.current_path)
                .clone()
                .or_else(|| poison(&state.current_resolved).clone())
                .ok_or_else(|| "no active disc path".to_string())?
        };
        tracing::info!(
            "[player] return_to_menu: currently in bypass_menu mode; reopening '{path}' with menus enabled"
        );
        player_open_inner(&app, &state, path, false, None, false).await?;
        Ok(())
    } else {
        dispatch_main_menu(&state)
    }
}

/// Send a disc root / main menu request through the libvlc_media_player_navigate_menu()
/// runtime extension. This is the deterministic main-menu path, distinct from
/// popup navigation (INPUT_NAV_POPUP).
///
/// Unlike player_menu_navigate (modes 0-5, public libvlc_navigate_mode_t), this
/// command reaches INPUT_NAV_MENU directly:
///   - Blu-ray: bd_menu_call() (Top Menu / Root Menu)
///   - DVD:     dvdnav_menu_call() (Title/Root menu)
///
/// Returns Err if the runtime does not export libvlc_media_player_navigate_menu.
#[tauri::command]
pub fn player_menu_navigate_menu(state: tauri::State<'_, PlayerState>) -> Result<(), String> {
    dispatch_main_menu(&state)
}

/// Shared implementation for main-menu dispatch. Both commands delegate here.
fn dispatch_main_menu(state: &tauri::State<'_, PlayerState>) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 || !*poison(&state.started) {
        return Err("no active player session".into());
    }
    let lib = loaded_lib(state)?;

    if let Some(navigate_menu) = lib.libvlc_media_player_navigate_menu {
        tracing::info!("[player] return_to_menu: sending INPUT_NAV_MENU");
        unsafe {
            navigate_menu(mp as *mut std::ffi::c_void);
        }
    } else {
        tracing::info!("[player] return_to_menu: fallback to set_title(menu)");
        let menu_title = unsafe { engine::find_menu_title(&lib, mp) }.unwrap_or(0);
        unsafe {
            (lib.libvlc_media_player_set_title)(mp as *mut c_void, menu_title);
        }
    }

    Ok(())
}

/// Ensure a normal feature audio track is selected (avoiding audio-description,
/// commentary, and descriptive narration tracks).
fn select_normal_audio_track(lib: &player::engine::VlcLib, mp: usize, session_gen: usize) {
    if mp == 0 {
        return;
    }
    let m = mp as *mut c_void;
    // Wait briefly for demuxer to populate audio track descriptions
    for attempt in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let list = unsafe { (lib.libvlc_audio_get_track_description)(m) };
        if list.is_null() {
            continue;
        }
        let tracks = unsafe { tracks_from_list(lib, list, -1) };
        if tracks.is_empty() {
            continue;
        }
        // Filter out disable (id <= 0) and find first normal audio track
        let normal_track = tracks.iter().find(|t| {
            if t.id <= 0 {
                return false;
            }
            let name_lower = t.name.to_lowercase();
            !name_lower.contains("description")
                && !name_lower.contains("desc")
                && !name_lower.contains("音声ガイド")
                && !name_lower.contains("解説")
                && !name_lower.contains("commentary")
                && !name_lower.contains("コメンタリー")
        });

        if let Some(target) = normal_track {
            unsafe {
                (lib.libvlc_audio_set_track)(m, target.id);
            }
            tracing::info!(
                "[audio-diag] session_gen={session_gen} selected normal feature audio: id={}, name='{}' (attempt {attempt})",
                target.id,
                target.name
            );
            return;
        } else if let Some(first_valid) = tracks.iter().find(|t| t.id > 0) {
            unsafe {
                (lib.libvlc_audio_set_track)(m, first_valid.id);
            }
            tracing::info!(
                "[audio-diag] session_gen={session_gen} fallback audio track selected: id={}, name='{}' (attempt {attempt})",
                first_valid.id,
                first_valid.name
            );
            return;
        }
    }
    tracing::warn!(
        "[audio-diag] session_gen={session_gen} could not enumerate audio tracks after 1000ms"
    );
}

/// Explicit application-level escape route to play the disc's main feature directly.
/// Bypasses the authored menu / notice screen without invoking failing menu commands,
/// reopening the disc in direct playback mode (:no-bluray-menu and :no-dvdnav-menu).
#[tauri::command]
pub async fn player_play_main_feature(
    app: tauri::AppHandle,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    if state.bypass_menu.load(std::sync::atomic::Ordering::SeqCst) {
        tracing::info!(
            "[feature-play] player_play_main_feature: already in bypass_menu mode; ignoring redundant request"
        );
        return Ok(());
    }

    let path = {
        let _mp_guard = state
            .media_player_cell
            .lock()
            .map_err(|_| "player state poisoned")?;
        let mp = *poison(&state.media_player);
        if mp == 0 || !*poison(&state.started) {
            return Err("no active player session".into());
        }
        poison(&state.current_path)
            .clone()
            .or_else(|| poison(&state.current_resolved).clone())
            .ok_or_else(|| "no active disc path".to_string())?
    };

    tracing::info!(
        "[feature-play] player_play_main_feature: reopening '{path}' with bypass_menu=true"
    );
    player_open_inner(&app, &state, path, false, None, true).await?;
    Ok(())
}

/// Authored disc-library display title (BDMV/META/DL/bdmt_*.xml, the BDA
/// DiVIMeta format discs ship for menus/jackets). Feeds the player's
/// upper-left label with the meaningful movie/disc name; None when the
/// disc carries no usable authored name — generic entries (Fuuka's discs
/// literally author their folder label "FUUKA_CS_D1") and missing
/// metadata fall back to the caller's existing library/filename metadata.
#[derive(Debug, Clone, Serialize)]
pub struct DiscDisplayTitle {
    pub name: String,
    pub lang: String,
}

/// Extract `<di:title><di:name>…</di:name></di:title>` text from a BDA
/// disc-library (bdmt) XML document. Namespace-agnostic on purpose: discs
/// vary in namespace prefixes but the local structure is fixed.
fn disc_library_title(xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    for node in doc.descendants() {
        if node.tag_name().name() == "name"
            && node
                .parent()
                .map(|p| p.tag_name().name() == "title")
                .unwrap_or(false)
        {
            let text = node.text()?.trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

#[tauri::command]
pub async fn player_disc_display_title(
    state: tauri::State<'_, PlayerState>,
    // Optional: the overlay invokes this without arguments (Tauri rejects
    // the call as a missing required key otherwise, which silently lost the
    // authored title). The current session's resolved disc wins anyway.
    path: Option<String>,
) -> Result<Option<DiscDisplayTitle>, String> {
    // Current session's resolved disc wins; the caller's path is the
    // fallback. Matches the probe_root pattern of the other disc probes.
    let path = poison(&state.current_resolved)
        .clone()
        .or_else(|| poison(&state.current_path).clone())
        .or(path)
        .unwrap_or_default();
    if path.is_empty() {
        return Ok(None);
    }
    tokio::task::spawn_blocking(move || {
        let probe_path = probe_root(&path);
        let dl = probe_path.join("BDMV").join("META").join("DL");
        if !dl.is_dir() {
            return Ok(None);
        }
        // Language preference: eng, then jpn, then any other authored file.
        // Files are tiny (<1 KiB); reading all of them is negligible.
        let mut lang_files: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&dl)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().into_string().ok()?;
                        let stem = name
                            .strip_prefix("bdmt_")?
                            .strip_suffix(".xml")?
                            .to_string();
                        Some((stem, e.path()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if lang_files.is_empty() {
            return Ok(None);
        }
        lang_files.sort_by_key(|(lang, _)| match lang.as_str() {
            "eng" => 0,
            "jpn" => 1,
            _ => 2,
        });
        for (lang, file) in lang_files {
            let Ok(xml) = std::fs::read_to_string(&file) else {
                continue;
            };
            let Some(name) = disc_library_title(&xml) else {
                continue;
            };
            // Reject generic authored names: the release's own folder label
            // ("FUUKA_CS_D1") is a placeholder, not a display title. Keep
            // the caller's trusted library metadata in that case.
            let folder = probe_path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let lower = name.to_lowercase();
            if lower.is_empty()
                || lower == folder
                || matches!(
                    lower.as_str(),
                    "movie" | "bluray" | "blu-ray" | "uhd" | "dvd" | "disc" | "title" | "bdmv"
                )
            {
                continue;
            }
            return Ok(Some(DiscDisplayTitle { name, lang }));
        }
        Ok(None)
    })
    .await
    .map_err(|e| format!("display title join error: {e}"))?
}

// Audio/subtitle language names for the CURRENT title. Blu-ray: read from
// the disc playlists via libbluray (VLC mislabels streams from CLIPINFO
// metadata on some discs), matched by `duration_ms`. DVD: read from the
// IFO tables, where `title` (the VLC title index) maps directly to a
// title set - libbluray does not cover DVDs at all.

/// Probe path for the language/chapter probes: prefer the RESOLVED disc
/// root (a multi-disc release resolves to a disc subfolder, so walking up
/// from current_path finds nothing); fall back to walking up from the raw
/// path, then to a disc folder inside it.
fn probe_root(path: &str) -> std::path::PathBuf {
    let mut probe = std::path::PathBuf::from(path);
    for _ in 0..4 {
        if probe.join("BDMV").is_dir() || probe.join("VIDEO_TS").is_dir() {
            return probe;
        }
        if !probe.pop() {
            break;
        }
    }
    // Multi-disc release root: the disc folders live BELOW it.
    let mut probe = std::path::PathBuf::from(path);
    for _ in 0..4 {
        if probe.join("BDMV").is_dir() || probe.join("VIDEO_TS").is_dir() {
            return probe;
        }
        if probe.is_dir() {
            let hit = std::fs::read_dir(&probe).ok().and_then(|rd| {
                rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|d| {
                    d.is_dir() && (d.join("BDMV").is_dir() || d.join("VIDEO_TS").is_dir())
                })
            });
            if let Some(d) = hit {
                return d;
            }
        }
        if !probe.pop() {
            break;
        }
    }
    probe
}

#[tauri::command]
pub async fn player_track_languages(
    state: tauri::State<'_, PlayerState>,
    duration_ms: i64,
    title: Option<i64>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let path = poison(&state.current_resolved)
        .clone()
        .or_else(|| poison(&state.current_path).clone())
        .unwrap_or_default();
    if path.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    tokio::task::spawn_blocking(move || {
        let probe_path = probe_root(&path);
        let video_ts_ifo = probe_path.join("VIDEO_TS").join("VIDEO_TS.IFO");
        let is_dvd = video_ts_ifo.is_file() || probe_path.join("VIDEO_TS.IFO").is_file();
        if is_dvd {
            let t = title.unwrap_or(1);
            tracing::info!("[player] track languages (dvd, title {t})");
            player::dvd_meta::disc_stream_languages_dvd(&probe_path.to_string_lossy(), t)
        } else {
            // duration_ms <= 0 (disc menu) must NOT short-circuit: the probe
            // falls back to the feature playlist (most distinct audio
            // languages) and returns the real track names.
            let r = player::bluray_meta::disc_stream_languages(
                &probe_path.to_string_lossy(),
                duration_ms,
            );
            tracing::info!("[player] track languages for dur {duration_ms}: {:?}", r);
            r
        }
    })
    .await
    .map_err(|e| format!("track languages join error: {e}"))?
}

/// Close the player. Returns the post-close session generation: the
/// frontend fences every engine callback (player-opened repeats fire up to
/// 8s after open; a readiness-poll response can be in flight mid-close) at
/// or below this generation, so none of them can reopen the player UI over
/// the session that was just torn down.
#[tauri::command]
pub async fn player_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, PlayerState>,
) -> Result<u64, String> {
    Ok(stop_session(&app, &state))
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
    // Whether the current title is an interactive menu (INPUT_TITLE_MENU flag).
    // Prefer this over checking `title == 0` for menu detection.
    pub at_menu: bool,
}

fn framing_sample_path(gen: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nebulae-crop-sample-{}-{}.png",
        std::process::id(),
        gen
    ))
}

fn apply_crop_spec(lib: &engine::VlcLib, mp: usize, spec: &str) {
    if let Some(set_crop) = lib.libvlc_video_set_crop_geometry {
        if let Ok(c) = std::ffi::CString::new(spec) {
            unsafe {
                set_crop(mp as *mut c_void, c.as_ptr());
            }
        }
    }
}

fn spawn_framing_sample(app: tauri::AppHandle, gen_spawn: usize, mp_spawn: usize) {
    std::thread::spawn(move || {
        use tauri::Manager;
        let state = app.state::<PlayerState>();
        let path = framing_sample_path(gen_spawn);
        let mut observation = None;
        let mut source = (0u32, 0u32);

        let lib = match loaded_lib(&state) {
            Ok(l) => l,
            Err(_) => {
                poison(&state.framing).sampling = false;
                return;
            }
        };

        let (take_snap_res, sar) = {
            let _mp_guard = match state.media_player_cell.lock() {
                Ok(g) => g,
                Err(_) => {
                    poison(&state.framing).sampling = false;
                    return;
                }
            };

            let gen_now = state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
            if gen_now != gen_spawn || !*poison(&state.started) {
                poison(&state.framing).sampling = false;
                return;
            }

            let mp = *poison(&state.media_player);
            if mp == 0 || mp != mp_spawn {
                poison(&state.framing).sampling = false;
                return;
            }

            if let Some(get_size) = lib.libvlc_video_get_size {
                let mut w: std::ffi::c_uint = 0;
                let mut h: std::ffi::c_uint = 0;
                if unsafe { get_size(mp as *mut c_void, 0, &mut w, &mut h) } == 0 {
                    source = (w, h);
                }
            }

            if source.0 == 0 || source.1 == 0 {
                poison(&state.framing).sampling = false;
                return;
            }

            let sar = unsafe { engine::video_track_sar(&lib, mp) };

            let _ = std::fs::remove_file(&path);

            let snap_opt = lib.libvlc_video_take_snapshot;
            let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok();

            // Downscale proportionally from native resolution to avoid distortion (Assumption 1)
            let snap_w = if source.0 > 960 { 960 } else { source.0 };
            let snap_h = ((snap_w as f64 / source.0 as f64) * source.1 as f64).round() as u32;

            let res = match (snap_opt, c_path) {
                (Some(take_snap), Some(c)) => unsafe {
                    take_snap(mp as *mut c_void, 0, c.as_ptr(), snap_w, snap_h)
                },
                _ => -1,
            };
            (res, sar)
        };

        if take_snap_res == 0 {
            for _ in 0..12 {
                if path.exists() {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if meta.len() > 0 {
                            observation = player::letterbox::observe_png_file(
                                &path,
                                source.0,
                                source.1,
                                Some(sar),
                            );
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }

        let _ = std::fs::remove_file(&path);

        let mut fs = poison(&state.framing);
        if let Some(obs) = observation {
            fs.pending = Some(obs);
        }
        fs.source = source;
        fs.sar = sar;
        fs.sampling = false;
    });
}

fn framing_tick(
    app: tauri::AppHandle,
    state: &PlayerState,
    lib: &engine::VlcLib,
    mp: usize,
    title: i32,
    at_menu: bool,
    playing: bool,
) {
    if !playing || mp == 0 {
        return;
    }

    let mut fs = poison(&state.framing);

    // 1. Disc menus must NEVER have black-bar crops applied.
    if at_menu {
        if fs.applied.is_some() || fs.settled {
            tracing::info!("[framing] menu active: clearing crop");
            apply_crop_spec(lib, mp, "");
        }
        if fs.window_fitted_aspect.is_some() {
            let restore_aspect = fs.source_display_aspect().unwrap_or(16.0 / 9.0);
            let _ = player::fit_player_window_to_aspect(&app, restore_aspect);
            fs.window_fitted_aspect = None;
        }
        fs.forget_measurements();
        return;
    }

    // 2. Title change: clear crop from previous title and reset tracker.
    if fs.title != title {
        tracing::info!(
            "[framing] title changed from {} to {}: clearing crop",
            fs.title,
            title
        );
        if fs.applied.is_some() {
            apply_crop_spec(lib, mp, "");
        }
        if fs.window_fitted_aspect.is_some() {
            let restore_aspect = fs.source_display_aspect().unwrap_or(16.0 / 9.0);
            let _ = player::fit_player_window_to_aspect(&app, restore_aspect);
            fs.window_fitted_aspect = None;
        }
        fs.title = title;
        fs.forget_measurements();
    }

    // 3. Process pending observation from helper thread
    if let Some(obs) = fs.pending.take() {
        match fs.tracker.observe(obs) {
            player::letterbox::CropAction::Set(bars) => {
                tracing::info!(
                    "[framing] crop confirmed: {} (active {}x{} in {}x{})",
                    bars.crop_spec(),
                    bars.active_width(),
                    bars.active_height(),
                    bars.frame_width,
                    bars.frame_height
                );
                fs.applied = Some(bars);
                fs.stable_samples = 0;
                fs.settled = false;
                apply_crop_spec(lib, mp, &bars.crop_spec());

                // Fit window to confirmed cropped display aspect ratio (4:3 exception preserved)
                let active_aspect = bars.active_aspect();
                if active_aspect > 1.45 {
                    let should_fit = match fs.window_fitted_aspect {
                        Some(last) => (last - active_aspect).abs() > 0.02,
                        None => true,
                    };
                    if should_fit {
                        fs.window_fitted_aspect = Some(active_aspect);
                        if let Err(e) = player::fit_player_window_to_aspect(&app, active_aspect) {
                            tracing::warn!("[framing] window fit failed: {e}");
                        }
                    }
                }
            }
            player::letterbox::CropAction::Clear => {
                tracing::info!("[framing] crop cleared (full frame picture detected)");
                fs.applied = None;
                fs.stable_samples = 0;
                fs.settled = false;
                apply_crop_spec(lib, mp, "");

                if fs.window_fitted_aspect.is_some() {
                    let restore_aspect = fs.source_display_aspect().unwrap_or(16.0 / 9.0);
                    let _ = player::fit_player_window_to_aspect(&app, restore_aspect);
                    fs.window_fitted_aspect = None;
                }
            }
            player::letterbox::CropAction::Stable => {
                fs.stable_samples += 1;
                if fs.stable_samples >= 2 {
                    fs.settled = true;
                }
            }
            player::letterbox::CropAction::None => {}
        }
    }

    // 4. Periodic re-assert of applied crop (every ~15s / 30 polls) to survive vout recreation
    if let Some(bars) = fs.applied {
        fs.polls_since_reassert += 1;
        if fs.polls_since_reassert >= 30 {
            fs.polls_since_reassert = 0;
            apply_crop_spec(lib, mp, &bars.crop_spec());
        }
    }

    // 5. Sampling cadence (every 2s until settled)
    if fs.settled || fs.sampling {
        return;
    }

    let sample_due = fs
        .last_sample_at
        .map(|t| t.elapsed() >= std::time::Duration::from_millis(2000))
        .unwrap_or(true);

    if !sample_due {
        return;
    }

    fs.sampling = true;
    fs.last_sample_at = Some(std::time::Instant::now());

    let gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst);
    spawn_framing_sample(app, gen, mp);
}

#[tauri::command]
pub fn player_poll(
    app: tauri::AppHandle,
    state: tauri::State<PlayerState>,
) -> Result<PlayerPoll, String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
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
            at_menu: false,
        });
    }
    let lib = loaded_lib(&state)?;
    // macOS/Windows: the EDR/HDR flag must reach the video layer, which is
    // created asynchronously after play(). Retry from this existing 1 Hz
    // poll until applied (latched per session); no timers or sleeps.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if let Some(get_size) = lib.libvlc_video_get_size {
            let mut width: std::ffi::c_uint = 0;
            let mut height: std::ffi::c_uint = 0;
            let res = unsafe { get_size(mp as *mut c_void, 0, &mut width, &mut height) };
            if res == 0
                && (width >= 3840 || height >= 2160)
                && !state.is_hdr.load(std::sync::atomic::Ordering::SeqCst)
            {
                tracing::info!(
                    "[player] UHD resolution detected: {width}x{height}, latching is_hdr=true"
                );
                state
                    .is_hdr
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                if let Some(app) = overlay_app_handle() {
                    force_apply_edr(app);
                }
            }
        }
        if let Some(app) = overlay_app_handle() {
            maybe_apply_edr(app);
        }
    }
    let (time_ms, length_ms, playing, title, title_count, chapter, chapter_count, at_menu) = unsafe {
        let m = mp as *mut c_void;
        (
            (lib.libvlc_media_player_get_time)(m),
            (lib.libvlc_media_player_get_length)(m),
            (lib.libvlc_media_player_is_playing)(m) != 0,
            (lib.libvlc_media_player_get_title)(m),
            (lib.libvlc_media_player_get_title_count)(m),
            (lib.libvlc_media_player_get_chapter)(m),
            (lib.libvlc_media_player_get_chapter_count)(m),
            is_menu_title(&lib, mp),
        )
    };

    framing_tick(app, &state, &lib, mp, title, at_menu, playing);

    let cur_gen = state.session_gen.load(std::sync::atomic::Ordering::SeqCst) as u64;
    let should_log_poll = {
        let mut guard = match state.last_logged_poll_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some((prev_gen, prev_title, prev_menu, prev_playing)) = *guard {
            if prev_gen != cur_gen
                || prev_title != title
                || prev_menu != at_menu
                || prev_playing != playing
            {
                *guard = Some((cur_gen, title, at_menu, playing));
                true
            } else {
                false
            }
        } else {
            *guard = Some((cur_gen, title, at_menu, playing));
            true
        }
    };
    if should_log_poll {
        let has_menu = unsafe { engine::has_menu_title(&lib, mp) };
        tracing::info!(
            "[poll-diag] session_gen={cur_gen} transition: title={title}/{title_count} at_menu={at_menu} has_menu_title={has_menu} playing={playing} time_ms={time_ms} length_ms={length_ms}"
        );
    }

    Ok(PlayerPoll {
        time_ms,
        length_ms,
        playing,
        title,
        title_count,
        chapter,
        chapter_count,
        at_menu,
    })
}

/// Jump to a disc TITLE. Title 0 is the DVD menu domain (dvdnav) — the
/// control bar's "Disc Menu" button uses this to leave the feature and
/// return to the interactive menu.
#[tauri::command]
pub fn player_set_title(state: tauri::State<PlayerState>, title: i32) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    tracing::info!("[player] set_title({title})");
    unsafe { (lib.libvlc_media_player_set_title)(mp as *mut c_void, title.max(0)) };
    Ok(())
}

/// Send a DVD/Blu-ray menu navigation action through libVLC's navigate API.
///
/// libvlc_navigate_mode_t: 0 = activate (Enter on the highlighted item),
/// 1 = up, 2 = down, 3 = left, 4 = right.
///
/// This is the deterministic menu input path. Mouse clicks on menu buttons
/// are NOT delivered to dvdnav when the vout is embedded into an external
/// NSView (reproduced and measured: synthesized clicks never activate;
/// navigate(activate) works on the first call), so the overlay wires the
/// keyboard and the on-screen D-pad to this command instead.
#[tauri::command]
pub fn player_menu_navigate(state: tauri::State<PlayerState>, mode: i32) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    let mode = mode.clamp(0, 5);
    tracing::info!("[player] menu_navigate({mode})");

    unsafe { (lib.libvlc_media_player_navigate)(mp as *mut c_void, mode) };
    Ok(())
}

/// Jump to a CHAPTER of the current title (0-based; the overlay renders
/// 1-based labels). The chapters popover's grid wires each cell to this.
#[tauri::command]
pub fn player_set_chapter(state: tauri::State<PlayerState>, chapter: i32) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    tracing::info!("[player] set_chapter({chapter})");
    unsafe { (lib.libvlc_media_player_set_chapter)(mp as *mut c_void, chapter.max(0)) };
    Ok(())
}

/// Chapter marks for the seek bar: real per-chapter time offsets of the
/// CURRENT title (libvlc_chapter_description_t). Empty when the demuxer
/// exposes none (e.g. plain files).
#[derive(serde::Serialize)]
pub struct ChapterMark {
    pub offset_ms: i64,
    pub duration_ms: i64,
    pub name: String,
}

/// Entry count from libvlc_media_player_get_full_title_descriptions /
/// libvlc_media_player_get_full_chapter_descriptions. The return value IS the
/// count: > 0 means that many entries (the array is NOT NULL-terminated —
/// verified against lib/media_player.c in the bundled VLC 3.0.23 source),
/// 0 means "no entries", and -1 means the call failed (no input thread yet).
/// The previous code treated any non-zero return as an error and then scanned
/// for a NULL terminator that never comes, which is why DVD titles (dvdnav
/// exposes chapters) never produced seek-bar marks. Capped defensively; the
/// release call still receives the original count.
fn vlc_description_count(rc: i32) -> usize {
    if rc <= 0 {
        0
    } else {
        (rc as usize).min(512)
    }
}

/// Build seek-bar marks from raw chapter entries (offset, duration, optional
/// name). Extracted as a pure function so the mapping is unit-testable
/// without a live VLC session.
fn chapter_marks_from_entries(entries: Vec<(i64, i64, Option<String>)>) -> Vec<ChapterMark> {
    entries
        .into_iter()
        .enumerate()
        .map(|(i, (offset_ms, duration_ms, name))| ChapterMark {
            offset_ms,
            duration_ms,
            name: name.unwrap_or_else(|| format!("Chapter {}", i + 1)),
        })
        .collect()
}

#[tauri::command]
pub fn player_chapter_marks(state: tauri::State<PlayerState>) -> Result<Vec<ChapterMark>, String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(Vec::new());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        let title = (lib.libvlc_media_player_get_title)(m);
        if title < 0 {
            // Demuxer still starting: no title selected yet.
            return Ok(Vec::new());
        }
        let mut chapters: *mut *mut crate::player::engine::VlcChapterDescription =
            std::ptr::null_mut();
        let rc = (lib.libvlc_media_player_get_full_chapter_descriptions)(m, title, &mut chapters);
        let count = vlc_description_count(rc);
        if count == 0 {
            if rc < 0 {
                tracing::debug!("[player] chapter marks: description call failed (rc={rc})");
            }
            if !chapters.is_null() {
                (lib.libvlc_chapter_descriptions_release)(chapters, 0);
            }
            return Ok(Vec::new());
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let ch = *chapters.add(i);
            if ch.is_null() {
                continue;
            }
            let name = if (*ch).psz_name.is_null() {
                None
            } else {
                Some(
                    CStr::from_ptr((*ch).psz_name)
                        .to_string_lossy()
                        .into_owned(),
                )
            };
            entries.push(((*ch).i_time_offset, (*ch).i_duration, name));
        }
        (lib.libvlc_chapter_descriptions_release)(chapters, rc);
        let marks = chapter_marks_from_entries(entries);
        tracing::info!(
            "[player] chapter marks: title {title} -> {} marks (rc={rc})",
            marks.len()
        );
        Ok(marks)
    }
}

/// A selectable playback track (subtitle or audio).
#[derive(serde::Serialize)]
pub struct TrackInfo {
    pub id: i32,
    pub name: String,
    pub active: bool,
}

unsafe fn tracks_from_list(
    lib: &player::engine::VlcLib,
    list: *mut crate::player::engine::VlcTrackDescription,
    active_id: i32,
) -> Vec<TrackInfo> {
    let mut out = Vec::new();
    let mut node = list;
    let mut guard = 0;
    while !node.is_null() && guard < 64 {
        let name = if (*node).psz_name.is_null() {
            format!("Track {}", (*node).i_id)
        } else {
            CStr::from_ptr((*node).psz_name)
                .to_string_lossy()
                .into_owned()
        };
        out.push(TrackInfo {
            id: (*node).i_id,
            name,
            active: (*node).i_id == active_id,
        });
        node = (*node).p_next;
        guard += 1;
    }
    (lib.libvlc_track_description_list_release)(list);
    out
}

#[tauri::command]
pub async fn player_disc_chapters(
    state: tauri::State<'_, PlayerState>,
    duration_ms: i64,
) -> Result<Vec<ChapterMark>, String> {
    // VLC's chapter API reports nothing for Blu-ray titles; read the
    // chapter starts straight from the disc's playlists via libbluray.
    let path = poison(&state.current_resolved)
        .clone()
        .or_else(|| poison(&state.current_path).clone())
        .unwrap_or_default();
    if path.is_empty() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || {
        let mut probe_path = probe_root(&path);
        // libbluray is Blu-ray only: DVDs (VIDEO_TS) have no BDMV and cannot
        // be probed by libbluray. Return empty so the frontend falls through
        // to VLC's chapter API (dvdnav exposes chapters for DVD titles)
        // without any delay from bd_open on a non-Blu-ray path.
        if probe_path.join("VIDEO_TS").is_dir() && !probe_path.join("BDMV").is_dir() {
            return Ok(Vec::new());
        }
        // ISO disc image: libbluray cannot read through the image file —
        // mount it read-only in the BACKGROUND (no dialogs, invisible to the
        // owner) and probe the BDMV inside, then detach.
        let mut iso_mount: Option<std::path::PathBuf> = None;
        if !probe_path.join("BDMV").is_dir() {
            let iso: Option<std::path::PathBuf> = if probe_path.is_file()
                && probe_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("iso"))
                    .unwrap_or(false)
            {
                Some(probe_path.clone())
            } else if probe_path.is_dir() {
                probe_path.read_dir().ok().and_then(|rd| {
                    rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                        p.is_file()
                            && p.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.eq_ignore_ascii_case("iso"))
                                .unwrap_or(false)
                    })
                })
            } else {
                None
            };
            if let Some(iso) = iso {
                let mp = std::env::temp_dir().join(format!("nebulae-iso-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&mp);
                let _ = std::fs::create_dir_all(&mp);
                let out = std::process::Command::new("hdiutil")
                    .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
                    .arg(&mp)
                    .arg(&iso)
                    .output();
                if out.map(|o| o.status.success()).unwrap_or(false) {
                    // BDMV may sit at the mount root or one level down.
                    let bdmv = if mp.join("BDMV").is_dir() {
                        mp.clone()
                    } else {
                        std::fs::read_dir(&mp)
                            .ok()
                            .and_then(|rd| {
                                rd.filter_map(|e| e.ok())
                                    .map(|e| e.path())
                                    .find(|d| d.is_dir() && d.join("BDMV").is_dir())
                            })
                            .unwrap_or_else(|| mp.clone())
                    };
                    probe_path = bdmv;
                    iso_mount = Some(mp);
                } else {
                    let _ = std::fs::remove_dir_all(&mp);
                }
            }
        }
        // No BDMV at the (possibly freshly mounted) probe root: this is not
        // a Blu-ray tree — e.g. a DVD ISO, where the mount holds VIDEO_TS.
        // libbluray can only fail here; skip the bd_open attempt so the
        // frontend falls through to VLC's chapter API (dvdnav exposes DVD
        // chapters) without the wasted mount/probe delay.
        if !probe_path.join("BDMV").is_dir() {
            if let Some(mp) = iso_mount {
                let _ = std::process::Command::new("hdiutil")
                    .args(["detach", "-force"])
                    .arg(&mp)
                    .output();
                let _ = std::fs::remove_dir_all(&mp);
            }
            return Ok(Vec::new());
        }
        let res =
            player::bluray_meta::disc_chapter_offsets(&probe_path.to_string_lossy(), duration_ms);
        if let Some(mp) = iso_mount {
            let _ = std::process::Command::new("hdiutil")
                .args(["detach", "-force"])
                .arg(&mp)
                .output();
            let _ = std::fs::remove_dir_all(&mp);
        }
        let (offsets, matched_dur) = res?;
        tracing::info!(
            "[player] disc chapters for dur {duration_ms}: {} marks (playlist dur {matched_dur} ms)",
            offsets.len()
        );
        Ok(offsets
            .into_iter()
            .enumerate()
            .map(|(i, off)| ChapterMark {
                offset_ms: off,
                duration_ms: matched_dur,
                name: format!("Chapter {}", i + 1),
            })
            .collect())
    })
    .await
    .map_err(|e| format!("disc chapters join error: {e}"))?
}

#[tauri::command]
pub fn player_list_tracks(
    state: tauri::State<PlayerState>,
    kind: String,
) -> Result<Vec<TrackInfo>, String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(Vec::new());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        match kind.as_str() {
            "subtitle" => {
                let active = (lib.libvlc_video_get_spu)(m);
                let list = (lib.libvlc_video_get_spu_description)(m);
                Ok(tracks_from_list(&lib, list, active))
            }
            "audio" => {
                let active = (lib.libvlc_audio_get_track)(m);
                let list = (lib.libvlc_audio_get_track_description)(m);
                Ok(tracks_from_list(&lib, list, active))
            }
            _ => Err(format!("unknown track kind: {kind}")),
        }
    }
}

#[tauri::command]
pub fn player_set_track(
    state: tauri::State<PlayerState>,
    kind: String,
    id: i32,
) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        match kind.as_str() {
            "subtitle" => (lib.libvlc_video_set_spu)(m, id),
            "audio" => (lib.libvlc_audio_set_track)(m, id),
            _ => return Err(format!("unknown track kind: {kind}")),
        }
    }
    Ok(())
}

/// Move to the previous/next FEATURE title, skipping menu and short
/// entries. Raw set_title(title+1) sent the player to arbitrary titles
/// (menus, extras) and read as "restarting from the beginning" (owner).
/// Titles with a duration under 2 minutes are skipped; if the engine
/// cannot enumerate titles, falls back to plain set_title.
#[tauri::command]
pub fn player_step_title(state: tauri::State<PlayerState>, dir: i32) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        let current = (lib.libvlc_media_player_get_title)(m);
        if current < 0 {
            tracing::info!("[player] step_title: no current title yet, ignoring");
            return Ok(());
        }
        let mut titles: *mut *mut crate::player::engine::VlcTitleDescription = std::ptr::null_mut();
        let rc = (lib.libvlc_media_player_get_full_title_descriptions)(m, &mut titles);
        let count = vlc_description_count(rc);
        if count == 0 || titles.is_null() {
            if !titles.is_null() {
                (lib.libvlc_title_descriptions_release)(titles, rc.max(0));
            }
            tracing::info!(
                "[player] step_title: no title list, set_title({})",
                current + dir
            );
            (lib.libvlc_media_player_set_title)(m, (current + dir).max(0));
            return Ok(());
        }
        // Feature titles: duration >= 2 min. Index 0 (menu domain) excluded.
        let mut features: Vec<usize> = Vec::new();
        for i in 1..count {
            let t = *titles.add(i);
            if !t.is_null() && (*t).i_duration >= 120_000 {
                features.push(i);
            }
        }
        if features.len() < 2 {
            (lib.libvlc_title_descriptions_release)(titles, rc);
            tracing::info!(
                "[player] step_title: single feature, set_title({})",
                current + dir
            );
            (lib.libvlc_media_player_set_title)(m, (current + dir).max(0));
            return Ok(());
        }
        // Find the feature index nearest the current position.
        let pos = features
            .iter()
            .enumerate()
            .min_by_key(|(_, idx)| ((**idx as i32) - current).abs())
            .map(|(k, _)| k)
            .unwrap_or(0);
        let next = if dir >= 0 {
            features[(pos + 1).min(features.len() - 1)]
        } else {
            features[pos.saturating_sub(1)]
        };
        tracing::info!(
            "[player] step_title: current={current} -> {next} ({} feature titles)",
            features.len()
        );
        (lib.libvlc_media_player_set_title)(m, next as i32);
        (lib.libvlc_title_descriptions_release)(titles, rc);
    }
    Ok(())
}

/// Toggle macOS fullscreen on the player window natively. The window-
/// manager route (Tauri set_fullscreen) and the CSS-side call both failed
/// silently on this borderless window; NSWindow.toggleFullScreen is the
/// reliable path. Corner radius follows the fullscreen state (owner: 3 px
/// in the player, 0 in fullscreen).
#[tauri::command]
pub fn player_window_minimize(app: tauri::AppHandle) -> Result<(), String> {
    crate::player::miniaturize(&app)
}

/// Title-bar window controls (owner: the JS window API is unreliable on
/// the undecorated Windows window - use native calls on every platform).
#[tauri::command]
pub fn window_minimize_cmd(app: tauri::AppHandle) -> Result<(), String> {
    crate::player::window_minimize_native(&app)
}

#[tauri::command]
pub fn window_maximize_toggle_cmd(app: tauri::AppHandle) -> Result<(), String> {
    crate::player::window_maximize_toggle_native(&app)
}

#[tauri::command]
pub fn window_close_cmd(app: tauri::AppHandle) -> Result<(), String> {
    crate::player::window_close_native(&app)
}

#[tauri::command]
pub fn window_is_maximized_cmd(app: tauri::AppHandle) -> Result<bool, String> {
    crate::player::window_is_maximized_native(&app)
}

#[tauri::command]
pub fn player_toggle_fullscreen(app: tauri::AppHandle) -> Result<(), String> {
    // Manual frame/style-mask toggle in embed_macos: the native
    // toggleFullScreen: animation ignores borderless windows entirely.
    crate::player::toggle_fullscreen(&app)
}

#[tauri::command]
pub fn player_toggle_pause(state: tauri::State<PlayerState>) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
    let mp = *poison(&state.media_player);
    if mp == 0 {
        return Ok(());
    }
    let lib = loaded_lib(&state)?;
    unsafe {
        let m = mp as *mut c_void;
        (lib.libvlc_media_player_pause)(m);
    }
    Ok(())
}

#[tauri::command]
pub fn player_seek(state: tauri::State<PlayerState>, position_ms: i64) -> Result<(), String> {
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
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
    let _mp_guard = state
        .media_player_cell
        .lock()
        .map_err(|_| "player state poisoned")?;
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
    bar_height: Option<f64>,
) -> Result<(), String> {
    player::sync_frame(&app, x, y, width, height, bar_height.unwrap_or(0.0))
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
pub async fn get_media_thumbnail(
    media_path: String,
) -> Result<crate::utils::library_cleaner::ThumbnailResult, String> {
    // Heavy work (disc-artwork scan, image decode, WebP encode, ffmpeg frame
    // extraction) MUST stay off the main thread — a sync command here froze
    // the whole app whenever the media grid requested several thumbnails at
    // once (the reported hang leading to a forced quit).
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&media_path);
        if !path.exists() {
            tracing::warn!("[thumbnail] media path does not exist: {media_path}");
            return Err(format!("Media path does not exist: {media_path}"));
        }

        let cache_dir = crate::utils::paths::AppPaths::init()
            .map(|p| p.cache_dir)
            .unwrap_or_else(|_| PathBuf::from("."));

        let result = crate::utils::library_cleaner::LibraryCleaner::get_or_create_thumbnail(
            path, &cache_dir,
        );
        if let Some(p) = &result.thumbnail_path {
            tracing::info!("[thumbnail] {media_path} -> {p}");
        } else {
            tracing::warn!(
                "[thumbnail] {media_path} -> no cover found (TMDB fallback should fire)"
            );
        }
        Ok(result)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// TMDB poster + metadata for a release (used when the local folder ships no
/// cover scan). Credentials come from the OS keychain; results are cached in
/// the DB per title+year. Returns None silently when no match / no key.
///
/// This command is `async` because `fetch_poster` uses `reqwest::blocking`
/// internally. Calling blocking I/O directly inside a Tokio async context
/// panics with "Cannot start a runtime from within a runtime". The entire
/// body runs inside `spawn_blocking`, which safely offloads it to the
/// blocking thread pool.
#[tauri::command]
pub async fn fetch_tmdb_release(
    title: String,
    year: Option<i32>,
) -> Result<Option<crate::utils::tmdb::TmdbMatch>, String> {
    // Clone before the closure so we can still log after spawn_blocking consumes `title`.
    let title_log = title.clone();
    let result = tokio::task::spawn_blocking(
        move || -> Result<Option<crate::utils::tmdb::TmdbMatch>, String> {
            // The DB lives in the app's own "Nebulae" data dir (AppPaths), NOT under
            // the bundle identifier dir — resolving it from app.path() produced
            // org.asiandvdclub.nebulae/internal/db/... which never existed, so every
            // TMDB fetch died on "unable to open database file" before any lookup.
            let db_path = crate::utils::paths::AppPaths::init()?.db_file;
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Data dir: {e}"))?;
            }
            let db = crate::db::Database::new(&db_path).map_err(|e| format!("Database: {e}"))?;
            crate::utils::tmdb::fetch_poster(&db, &title, &title, year)
        },
    )
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))??;

    match &result {
        Some(m) => tracing::info!(
            "[tmdb] match for {title_log:?}: poster={} year={:?}",
            m.poster_webp.display(),
            m.year
        ),
        // Silent-skip (no keychain entry) or genuinely no match: both mean the
        // grid keeps its placeholder — log it so the owner sees WHY.
        None => tracing::warn!("[tmdb] no match or no credentials for {title_log:?}"),
    }
    Ok(result)
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

    /// The frontend fences stale callbacks by `gen`; the field must ride
    /// the exact payload shape the webview already parses (flattened
    /// ResolvedPlayback + gen), for both the invoke result and the
    /// repeated `player-opened` event.
    #[test]
    fn player_open_result_serializes_flat_with_gen() {
        let r = PlayerOpenResult {
            info: ResolvedPlayback {
                resolved_path: "/Volumes/Media/Disc".to_string(),
                title: "Disc".to_string(),
                is_disc: true,
                external: false,
            },
            gen: 7,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["resolved_path"], "/Volumes/Media/Disc");
        assert_eq!(v["title"], "Disc");
        assert_eq!(v["is_disc"], true);
        assert_eq!(v["external"], false);
        assert_eq!(v["gen"], 7);
    }

    /// The readiness poll's generation is what fences in-flight responses
    /// captured before/during a close; it must always serialize.
    #[test]
    fn engine_playback_serializes_gen() {
        let e = EnginePlayback {
            resolved_path: "/Volumes/Media/Disc".to_string(),
            title: "Disc".to_string(),
            is_disc: true,
            gen: 3,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["resolved_path"], "/Volumes/Media/Disc");
        assert_eq!(v["gen"], 3);
    }

    #[test]
    fn test_vlc_description_count_semantics() {
        // libvlc returns the entry count (> 0), 0 for "no entries", -1 on
        // failure. Regression test: the old code treated rc != 0 as failure,
        // which discarded every DVD chapter list.
        assert_eq!(vlc_description_count(-1), 0);
        assert_eq!(vlc_description_count(0), 0);
        assert_eq!(vlc_description_count(1), 1);
        assert_eq!(vlc_description_count(23), 23);
        // Defensive cap: absurd counts never drive unbounded reads.
        assert_eq!(vlc_description_count(i32::MAX), 512);
    }

    #[test]
    fn test_chapter_marks_from_entries() {
        let marks = chapter_marks_from_entries(vec![
            (0, 600_000, Some("Opening".to_string())),
            (600_000, 540_000, None),
            (1_140_000, 0, None),
        ]);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].offset_ms, 0);
        assert_eq!(marks[0].name, "Opening");
        assert_eq!(marks[1].offset_ms, 600_000);
        assert_eq!(marks[1].name, "Chapter 2"); // generated 1-based fallback
        assert_eq!(marks[2].name, "Chapter 3");
        assert_eq!(marks[2].duration_ms, 0);
        assert!(chapter_marks_from_entries(Vec::new()).is_empty());
    }

    #[test]
    fn test_launch_nonexistent_media() {
        let res = resolve_playback("/nonexistent/movie/file.mkv");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_release_root_of_single_and_multi_disc() {
        assert_eq!(
            release_root_of(r"V:\UHD\Suzume (2022) UHD"),
            r"V:\UHD\Suzume (2022) UHD"
        );
        assert_eq!(
            release_root_of(r"V:\UHD\Suzume (2022) UHD\BDMV"),
            r"V:\UHD\Suzume (2022) UHD"
        );
        assert_eq!(
            release_root_of(r"V:\Blu-ray\Fuuka (2017) Funimation\FUUKA_CS_D1"),
            r"V:\Blu-ray\Fuuka (2017) Funimation"
        );
        assert_eq!(
            release_root_of(
                r"C:\Users\administrator\Downloads\Banner of the Stars III OVA (2005) R2J\DISC1"
            ),
            r"C:\Users\administrator\Downloads\Banner of the Stars III OVA (2005) R2J"
        );
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

/// One externally-opened media item in the persistent collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMedia {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub year: Option<i32>,
    /// "Blu-ray UHD" | "Blu-ray" | "DVD" | "ISO" | "File"
    pub kind: String,
    pub added_ms: i64,
}

fn external_media_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("app data dir: {e}"))?;
    Ok(dir.join("external_media.json"))
}

fn classify_external(path: &str) -> String {
    let p = std::path::Path::new(path);
    let lower_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
        .unwrap_or_default();
    if p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("iso"))
        .unwrap_or(false)
    {
        return "ISO".into();
    }
    if p.join("BDMV").is_dir() || p.join("bdmv").is_dir() {
        // UHD Blu-ray is also a BDMV tree; the release name is the only
        // reliable hint at this layer.
        if lower_name.contains("uhd") || lower_name.contains("2160") {
            return "Blu-ray UHD".into();
        }
        return "Blu-ray".into();
    }
    if p.join("VIDEO_TS").is_dir() || p.join("video_ts").is_dir() {
        return "DVD".into();
    }
    "File".into()
}

fn read_external_store(app: &tauri::AppHandle) -> Vec<ExternalMedia> {
    let Ok(store) = external_media_path(app) else {
        return Vec::new();
    };
    std::fs::read(&store)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_external_store(app: &tauri::AppHandle, items: &[ExternalMedia]) -> Result<(), String> {
    // Collapse legacy duplicates on every write: one entry per title+year
    // (owner: "we do not need 2 separate entries"). Keeps the FIRST.
    let mut seen: Vec<(String, Option<i32>)> = Vec::new();
    let items: Vec<ExternalMedia> = items
        .iter()
        .filter(|m| {
            let key = (m.title.to_ascii_lowercase(), m.year);
            if seen.contains(&key) {
                return false;
            }
            seen.push(key);
            true
        })
        .cloned()
        .collect();
    let store = external_media_path(app)?;
    let data = serde_json::to_vec_pretty(&items).map_err(|e| e.to_string())?;
    std::fs::write(store, data).map_err(|e| e.to_string())
}

/// Record an externally-opened item (file / disc folder / ISO) in the
/// persistent media collection. Idempotent on `path`.
#[tauri::command]

/// Walk up from a recorded path to the RELEASE ROOT: a disc folder
/// (direct BDMV/VIDEO_TS child, or an ISO file) is replaced by its
/// parent until the directory no longer IS a disc.
fn release_root_of(path: &str) -> String {
    let is_windows_style = path.contains('\\');
    let sep = if is_windows_style { '\\' } else { '/' };
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let has_drive = parts.first().map(|p| p.ends_with(':')).unwrap_or(false);
    let is_absolute_unix = path.starts_with('/');

    fn is_library_dir_name(name: &str) -> bool {
        let lower = name.trim().to_lowercase();
        matches!(
            lower.as_str(),
            "blu-ray"
                | "bluray"
                | "dvd"
                | "movies"
                | "films"
                | "media"
                | "cinema"
                | "downloads"
                | "torrents"
                | "uhd"
                | "4k"
                | "anime"
                | "video"
                | "videos"
        )
    }

    fn is_disc_subfolder_name(name: &str) -> bool {
        let lower = name.trim().to_lowercase();
        if lower == "bdmv" || lower == "video_ts" || lower == "certificate" || lower == "audio_ts" {
            return true;
        }
        let (disc_num, _) = DiscDetector::parse_disc_numbers(&lower);
        disc_num.is_some()
    }

    while parts.len() > 1 {
        let last = *parts.last().unwrap();
        // If last segment is a file (e.g. .mkv, .iso, .bdmv), pop it
        if last.contains('.') && !last.ends_with(".disc") {
            parts.pop();
            continue;
        }
        if is_disc_subfolder_name(last) {
            if parts.len() == 2 && has_drive {
                break;
            }
            if parts.len() >= 2 && is_library_dir_name(parts[parts.len() - 2]) {
                break;
            }
            parts.pop();
        } else {
            break;
        }
    }

    if has_drive {
        let drive = parts[0];
        if parts.len() == 1 {
            format!("{drive}{sep}")
        } else {
            format!("{drive}{sep}{}", parts[1..].join(&sep.to_string()))
        }
    } else if is_absolute_unix {
        format!("/{}", parts.join("/"))
    } else {
        parts.join(&sep.to_string())
    }
}

#[tauri::command]
pub fn add_external_media(
    app: tauri::AppHandle,
    path: String,
    title: String,
) -> Result<(), String> {
    let release_root = release_root_of(&path);
    let path = release_root;
    let mut items = read_external_store(&app);
    if items.iter().any(|m| m.path == path) {
        return Ok(());
    }
    // Use MediaTitleParser so TMDB metadata (library banner, poster, overview, runtime)
    // resolves properly for external discs and folders with various naming patterns.
    let parsed = crate::utils::media_parser::MediaTitleParser::parse(&title);
    let title_clean = if !parsed.clean_title.is_empty() {
        parsed.clean_title
    } else {
        title.trim().to_string()
    };
    let year = parsed.year.map(|y| y as i32);
    // One poster per movie (owner): the same title can arrive twice -
    // auto-adopted on play AND added manually - at different folder
    // levels. Keep the FIRST entry; later duplicates are dropped.
    if items
        .iter()
        .any(|m| m.title.eq_ignore_ascii_case(&title_clean) && m.year == year)
    {
        return Ok(());
    }
    items.push(ExternalMedia {
        path: path.clone(),
        title: title_clean,
        year,
        kind: classify_external(&path),
        added_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    });
    write_external_store(&app, &items)
}

/// List the collection; entries whose source path no longer exists are
/// pruned on read ("saved as long as it's present in the original
/// directory").
#[tauri::command]
pub fn list_external_media(app: tauri::AppHandle) -> Result<Vec<ExternalMedia>, String> {
    let items = read_external_store(&app);
    // Heal legacy disc-level paths to the release root on read (same
    // normalization add_external_media applies).
    let live: Vec<ExternalMedia> = items
        .into_iter()
        .map(|mut m| {
            m.path = release_root_of(&m.path);
            m
        })
        .filter(|m| std::path::Path::new(&m.path).exists())
        .collect();
    let _ = write_external_store(&app, &live);
    Ok(live)
}

/// Remove an entry explicitly (owner-initiated delete in the collection).
#[tauri::command]
pub fn remove_external_media(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut items = read_external_store(&app);
    items.retain(|m| m.path != path);
    write_external_store(&app, &items)
}

/// Toggle the player window's always-on-top pin (owner: a little mark in
/// Toggle the player window's always-on-top pin (owner: a little mark in
/// the player's top-right corner; click to engage, click again to
/// disengage). Reports the NEW state.
static PINNED_ON_TOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub async fn player_set_always_on_top(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<bool, String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        win.set_always_on_top(enabled)
            .map_err(|e| format!("always on top: {e}"))?;
        PINNED_ON_TOP.store(enabled, std::sync::atomic::Ordering::SeqCst);
        Ok(enabled)
    } else {
        Err("main window not found".to_string())
    }
}

#[tauri::command]
pub async fn player_always_on_top(app: tauri::AppHandle) -> Result<bool, String> {
    let pinned = PINNED_ON_TOP.load(std::sync::atomic::Ordering::SeqCst);
    player_set_always_on_top(app, !pinned).await
}

// ---------------------------------------------------------------------------
// Disc-warning persistence helpers
// ---------------------------------------------------------------------------
// Stores a per-disc flag recording that the post-menu warning clip has been
// shown at least once. On subsequent sessions the watcher uses this flag to
// seek past the warning automatically.
//
// Storage: <app_data_dir>/disc_warns.json  — a JSON array of DiscWarnEntry.
// Keyed by disc_sha1 only (one entry per disc). warn_mpls_id and warn_mpls_size
// are verified at skip-time to guard against disc replacement.

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct DiscWarnEntry {
    /// SHA-1 hex string of BDMV/MovieObject.bdmv (disc authoring identity).
    disc_sha1: String,
    /// MPLS number (0–99999) of the warning playlist.
    warn_mpls_id: u32,
    /// Byte size of BDMV/PLAYLIST/<warn_mpls_id:05>.mpls at the time the flag
    /// was written. Used to detect disc replacement or authoring differences.
    warn_mpls_size: u64,
    /// True once the warning has been seen and the flag is active.
    warn_seen: bool,
}

fn disc_warns_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("disc_warns.json"))
}

fn read_disc_warns(app: &tauri::AppHandle) -> Vec<DiscWarnEntry> {
    let path = match disc_warns_path(app) {
        Some(p) => p,
        None => return Vec::new(),
    };
    std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<DiscWarnEntry>>(&b).ok())
        .unwrap_or_default()
}

fn write_disc_warn(
    app: &tauri::AppHandle,
    disc_sha1: &str,
    warn_mpls_id: u32,
    warn_mpls_size: u64,
) {
    let path = match disc_warns_path(app) {
        Some(p) => p,
        None => {
            tracing::warn!("[warn-watcher] cannot resolve app data dir — flag not persisted");
            return;
        }
    };
    let mut entries = read_disc_warns(app);
    // Replace any existing entry for this disc (keyed by disc_sha1 only;
    // each disc has exactly one warning entry — a new recording overwrites any prior one).
    let existing = entries.iter_mut().find(|e| e.disc_sha1 == disc_sha1);
    if let Some(e) = existing {
        e.warn_seen = true;
        e.warn_mpls_id = warn_mpls_id;
        e.warn_mpls_size = warn_mpls_size;
    } else {
        entries.push(DiscWarnEntry {
            disc_sha1: disc_sha1.to_string(),
            warn_mpls_id,
            warn_mpls_size,
            warn_seen: true,
        });
    }
    match serde_json::to_vec_pretty(&entries) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, &bytes) {
                tracing::warn!("[warn-watcher] failed to write disc_warns.json: {e}");
            } else {
                tracing::info!(
                    "[warn-watcher] flag written: disc_sha1={disc_sha1} mpls={warn_mpls_id:05} size={warn_mpls_size}"
                );
            }
        }
        Err(e) => {
            tracing::warn!("[warn-watcher] failed to serialise disc_warns: {e}");
        }
    }
}

/// Compute the SHA-1 hex digest of a file. Returns None on I/O error.
/// Uses a 64 KiB read buffer to avoid loading large BDMV files into memory.
fn sha1_file(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut state = [0u32; 5];
    // SHA-1 initialisation constants (FIPS 180-4 § 5.3.1)
    state[0] = 0x67452301;
    state[1] = 0xEFCDAB89;
    state[2] = 0x98BADCFE;
    state[3] = 0x10325476;
    state[4] = 0xC3D2E1F0;

    let mut buf = vec![0u8; 65536];
    let mut total_bytes: u64 = 0;
    let mut pending: Vec<u8> = Vec::with_capacity(128);

    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        total_bytes += n as u64;
        pending.extend_from_slice(&buf[..n]);
        while pending.len() >= 64 {
            let chunk: [u8; 64] = pending[..64].try_into().unwrap();
            sha1_compress(&mut state, &chunk);
            pending.drain(..64);
        }
    }

    // Padding
    pending.push(0x80);
    while pending.len() % 64 != 56 {
        pending.push(0x00);
    }
    let bit_len = total_bytes * 8;
    pending.extend_from_slice(&bit_len.to_be_bytes());
    while pending.len() >= 64 {
        let chunk: [u8; 64] = pending[..64].try_into().unwrap();
        sha1_compress(&mut state, &chunk);
        pending.drain(..64);
    }

    Some(format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}",
        state[0], state[1], state[2], state[3], state[4]
    ))
}

fn sha1_compress(h: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for i in 0..16 {
        w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = [h[0], h[1], h[2], h[3], h[4]];
    #[allow(clippy::needless_range_loop)]
    for i in 0..80 {
        let (f, k) = match i {
            0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
            20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
            _ => (b ^ c ^ d, 0xCA62C1D6u32),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(w[i]);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(c);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
}

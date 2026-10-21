use crate::player;
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

fn stop_session(app: &tauri::AppHandle, state: &PlayerState) {
    let mp_guard = match state.media_player_cell.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let mp = *poison(&state.media_player);
    if mp == 0 {
        // No live player, but clear the liveness flag anyway: a stale
        // `started` here would let the engine poll adopt a dead session.
        *poison(&state.started) = false;
        *poison(&state.current_resolved) = None;
        return;
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
    drop(mp_guard);
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
    start_playback_inner(app, state, file_path, resume, None)
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
) -> Result<ResolvedPlayback, String> {
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
            ) {
                Ok(session) => {
                    // Record session pointers immediately so stop_session and
                    // pollers are immediately aware of the live media player.
                    *poison(&state.instance) = session.instance;
                    *poison(&state.media) = session.media;
                    *poison(&state.media_player) = session.media_player;
                    *poison(&state.started) = true;

                    // Disc-mode liveness check: bluray:// (and dvd://) MRLs
                    // can fail inside the demuxer while libvlc_media_player_play
                    // still reports success (reproduced: black screen, no
                    tracing::info!(
                        "player_open: PLAYING '{}' (disc: {})",
                        resolved.resolved_path,
                        resolved.is_disc
                    );
                    if resolved.is_disc && !resume {
                        // MENU-FIRST: a normal disc Open plays the disc's own
                        // first-play flow, which ends at the authored top
                        // menu (verified against stock VLC 3.0.23 on the
                        // affected HDMV discs). The engine must NOT force
                        // titles underneath the disc's navigation state
                        // machine: the previous 15 s watcher called
                        // set_title(0) in a loop while the disc was still
                        // running first play, which fought the disc's own
                        // navigation and could leave it cycling logo
                        // titles instead of settling on the menu. Explicit
                        // resume (resume=true) still skips this branch and
                        // opens the feature directly for the deferred seek.

                        // Deferred liveness check: detect silent decryption failure.
                        // When AACS keys (libaacs + KEYDB.cfg) are absent, the Blu-ray
                        // demuxer opens the BDMV but cannot decrypt stream data.
                        // libvlc_media_player_play() returns 0 (success) and the session
                        // struct is valid, but get_time stays at -1 because no frames are
                        // decoded. This produces the black-screen / 0:00:00 symptom.
                        // Emit a user-facing error event after 6 seconds if still frozen.
                        let app3 = app.clone();
                        std::thread::spawn(move || {
                            use tauri::{Emitter, Manager};
                            std::thread::sleep(std::time::Duration::from_secs(6));
                            let state = app3.state::<PlayerState>();
                            if !*poison(&state.started) {
                                return; // session already closed
                            }
                            let mp = *poison(&state.media_player);
                            if mp == 0 {
                                return;
                            }
                            let lib = match loaded_lib(&state) {
                                Ok(l) => l,
                                Err(_) => return,
                            };
                            let ms =
                                unsafe { (lib.libvlc_media_player_get_time)(mp as *mut c_void) };
                            if ms == -1 {
                                // No media progress after 6 s — almost certainly a
                                // decryption failure (AACS) or unsupported disc format.
                                tracing::error!(
                                    "[player] liveness check failed: get_time=-1 after 6 s \
                                     — disc may require AACS decryption keys (libaacs + KEYDB.cfg)"
                                );
                                let msg = "Playback did not start. \
                                    This disc may be encrypted (AACS copy protection). \
                                    Install libaacs and a valid KEYDB.cfg, then try again. \
                                    If the disc is not encrypted, check that the BDMV folder is readable.";
                                let _ = app3.emit("nebulae://player-error", msg);
                            } else {
                                tracing::info!(
                                    "[player] liveness check OK: get_time={ms} ms after 6 s"
                                );
                            }
                        });

                        // Menu state tracking: when the disc's first-play flow reaches
                        // the interactive menu, report via player_poll.at_menu. The
                        // frontend can then enable menu-specific controls.
                        // Automatic pausing is NOT applied — the menu remains live and
                        // navigable. The frontend's Play button (when at_menu) handles
                        // activation.
                        let app4 = app.clone();
                        std::thread::spawn(move || {
                            use tauri::{Emitter, Manager};
                            std::thread::sleep(std::time::Duration::from_secs(4));
                            let state = app4.state::<PlayerState>();
                            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                            while std::time::Instant::now() < deadline {
                                if !*poison(&state.started) {
                                    return; // session closed
                                }
                                let mp = *poison(&state.media_player);
                                if mp == 0 {
                                    return;
                                }
                                let lib = match loaded_lib(&state) {
                                    Ok(l) => l,
                                    Err(_) => return,
                                };
                                // Use title flags to detect if current title is a menu.
                                let is_menu = unsafe { engine::is_menu_title(&lib, mp) };
                                if is_menu {
                                    tracing::info!("[player] menu state: at interactive menu (title flags)");
                                    return;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(500));
                            }
                            tracing::info!("[player] menu state: timeout, menu did not confirmed within 30 s");
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
) -> Result<ResolvedPlayback, String> {
    let _open_guard = state.open_lock.lock().await;
    let resume = resume.unwrap_or(false);
    tracing::info!("player_open: entry, path='{file_path}' (resume={resume})");
    stop_session(&app, &state);
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
    let (open_target, explicit_feature): (String, Option<String>) = if resume {
        match resolve_playback(&file_path) {
            Ok(r) if r.is_disc && !r.external => {
                match find_main_video(Path::new(&r.resolved_path)) {
                    Some(feature) => {
                        let feature_str = feature.display().to_string();
                        tracing::info!(
                            "player_open: resume targets feature file '{}'",
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
        &app,
        &state,
        &open_target,
        resume,
        explicit_feature.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
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
    // Programmatic opens (diagnostics, autoplay) must reach the frontend so
    // the overlay mounts and the shell clears away from the video. The
    // webview may still be loading when the open happens, so the event is
    // repeated a few times; the frontend's playerStore.open is idempotent.
    use tauri::Emitter;
    let emitter = app.clone();
    let payload = resolved.clone();
    std::thread::spawn(move || {
        for delay in [0, 1500, 4000, 8000] {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            let _ = emitter.emit("player-opened", &payload);
        }
    });
    Ok(resolved)
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
}

#[tauri::command]
pub fn player_engine_playback(state: tauri::State<PlayerState>) -> Option<EnginePlayback> {
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

/// Return DVD/BD playback to its interactive menu.
///
/// Uses `libvlc_media_player_navigate()` with `libvlc_navigate_popup` — the
/// only disc-menu navigation exposed by the libVLC 3 public API. VLC 3 does
/// not expose `INPUT_NAV_MENU` (root/top menu) through this function; that
/// internal value (6) exists in `include/vlc_input.h` but is not mapped in
/// `lib/media_player.c:1798`. For Blu-ray, `navigate_popup` requests the
/// popup menu; for DVD, dvdnav handles menu navigation internally.
///
/// Verification uses `is_menu_title()` (flag-based) rather than
/// `title_index == 0`, since some discs carry menu titles at non-zero indices.
#[tauri::command]
pub async fn player_return_to_menu(
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    tracing::info!("[player] return_to_menu: requested");
    let (mp, lib) = {
        let _mp_guard = state
            .media_player_cell
            .lock()
            .map_err(|_| "player state poisoned")?;
        let mp = *poison(&state.media_player);
        let lib = loaded_lib(&state)?;
        (mp, lib)
    };
    if mp == 0 {
        return Err("no active player session".into());
    }

    // Already at a menu title: nothing to navigate.
    if unsafe { engine::is_menu_title(&lib, mp) } {
        tracing::info!("[player] return_to_menu: already at interactive menu");
        return Ok(());
    }

    // Request disc menu via the only public navigation available in libVLC 3.
    // Mode 5 = libvlc_navigate_popup = INPUT_NAV_POPUP.
    tracing::info!("[player] return_to_menu: sending navigate_popup");
    unsafe {
        (lib.libvlc_media_player_navigate)(mp as *mut std::ffi::c_void, 5);
    }

    // Verify the navigation reached a menu title using flag-based classification.
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    if unsafe { engine::is_menu_title(&lib, mp) } {
        tracing::info!("[player] return_to_menu: now at interactive menu");
        return Ok(());
    }

    // Navigation was dispatched but state did not confirm a menu. Report the
    // actual state instead of silently restarting the disc.
    let title = unsafe { (lib.libvlc_media_player_get_title)(mp as *mut std::ffi::c_void) };
    tracing::warn!(
        "[player] return_to_menu: navigate_popup dispatched; current title={}, at_menu=false",
        title
    );
    Err("menu navigation dispatched but player did not reach a menu title".into())
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
                        let stem = name.strip_prefix("bdmt_")?.strip_suffix(".xml")?.to_string();
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

#[tauri::command]
pub async fn player_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
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
    // Whether the current title is an interactive menu (INPUT_TITLE_MENU flag).
    // Prefer this over checking `title == 0` for menu detection.
    pub at_menu: bool,
}

#[tauri::command]
pub fn player_poll(state: tauri::State<PlayerState>) -> Result<PlayerPoll, String> {
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
            at_menu: is_menu_title(&lib, mp),
        })
    }
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
    // UHD fix: flush decoder buffers before the format-changing jump to
    // the menu domain (title 0), same as return_to_menu.
    if title == 0 {
        unsafe {
            (lib.libvlc_media_player_pause)(mp as *mut c_void, 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            (lib.libvlc_media_player_pause)(mp as *mut c_void, 0);
        }
    }
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
    
    // UHD playback fix: flush VideoToolbox decoder buffers before activate.
    // When transitioning from 1080p menu to 4K HEVC main title, the decoder
    // holds outstanding picture buffers from the menu stream. A brief pause
    // forces buffer release, allowing clean reinit for the feature stream.
    if mode == 0 {
        unsafe {
            (lib.libvlc_media_player_pause)(mp as *mut c_void, 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            (lib.libvlc_media_player_pause)(mp as *mut c_void, 0);
        }
    }
    
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
        let mut chapters: *mut *mut crate::player::engine::VlcChapterDescription =
            std::ptr::null_mut();
        let rc = (lib.libvlc_media_player_get_full_chapter_descriptions)(m, title, &mut chapters);
        if rc != 0 || chapters.is_null() {
            return Ok(Vec::new());
        }
        let mut marks = Vec::new();
        let mut i = 0;
        // The array is NULL-name-terminated per the libvlc docs (count via
        // entries until NULL); iterate defensively with a cap.
        while i < 512 && !(*chapters.add(i)).is_null() {
            let ch = *chapters.add(i);
            let name = if (*ch).psz_name.is_null() {
                format!("Chapter {}", i + 1)
            } else {
                CStr::from_ptr((*ch).psz_name)
                    .to_string_lossy()
                    .into_owned()
            };
            marks.push(ChapterMark {
                offset_ms: (*ch).i_time_offset,
                duration_ms: (*ch).i_duration,
                name,
            });
            i += 1;
        }
        (lib.libvlc_chapter_descriptions_release)(chapters, i as i32);
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
        let mut titles: *mut *mut crate::player::engine::VlcTitleDescription = std::ptr::null_mut();
        let rc = (lib.libvlc_media_player_get_full_title_descriptions)(m, &mut titles);
        if rc != 0 || titles.is_null() {
            tracing::info!(
                "[player] step_title: no title list, set_title({})",
                current + dir
            );
            (lib.libvlc_media_player_set_title)(m, (current + dir).max(0));
            return Ok(());
        }
        let mut count = 0usize;
        while count < 512 && !(*titles.add(count)).is_null() {
            count += 1;
        }
        // Feature titles: duration >= 2 min. Index 0 (menu domain) excluded.
        let mut features: Vec<usize> = Vec::new();
        for i in 1..count {
            let t = *titles.add(i);
            if (*t).i_duration >= 120_000 {
                features.push(i);
            }
        }
        if features.len() < 2 {
            (lib.libvlc_title_descriptions_release)(titles, count as i32);
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
        (lib.libvlc_title_descriptions_release)(titles, count as i32);
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
        let playing = (lib.libvlc_media_player_is_playing)(m) != 0;
        (lib.libvlc_media_player_pause)(m, if playing { 1 } else { 0 });
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
    fn is_disc_dir(dir: &std::path::Path) -> bool {
        if dir.file_name().map(|n| {
            let n = n.to_string_lossy().to_lowercase();
            n.ends_with(".iso") || n.ends_with(".img")
        }) == Some(true)
        {
            return true;
        }
        dir.join("BDMV").exists() || dir.join("VIDEO_TS").exists()
    }
    let mut cur = std::path::PathBuf::from(path);
    if cur.is_file() {
        cur = cur.parent().map(|p| p.to_path_buf()).unwrap_or(cur);
    }
    while is_disc_dir(&cur) {
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => break,
        }
    }
    cur.to_string_lossy().to_string()
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

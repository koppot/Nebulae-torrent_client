use crate::utils::disc_detector::{DiscDetector, DiscStructureInfo, DiscType};
use crate::utils::media_parser::{MediaTitleParser, ParsedMediaMetadata};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

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

#[tauri::command]
pub fn scan_disc_artwork(file_path: String) -> Result<Option<String>, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }
    Ok(DiscDetector::find_cover_image(path))
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerEngine {
    BundledMpv,
}

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
pub fn get_disc_player_status(app: tauri::AppHandle) -> Result<DiscPlayerStatus, String> {
    let resource_dir = app.path().resource_dir().ok();
    get_disc_player_status_impl(resource_dir.as_deref())
}

fn get_disc_player_status_impl(resource_dir: Option<&Path>) -> Result<DiscPlayerStatus, String> {
    let player_info = find_optical_disc_player(resource_dir);
    let java_home = detect_java_home(resource_dir).map(|p| p.to_string_lossy().to_string());
    let has_jvm = java_home.is_some();

    match player_info {
        Some((path, engine)) => {
            let path_str = path.to_string_lossy().to_string();
            let mut version_str = String::new();
            if let Ok(output) = Command::new(&path).arg("--version").output() {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    if let Some(first_line) = s.lines().next() {
                        version_str = first_line.trim().to_string();
                    }
                }
            }

            let (p_type, p_name) = match engine {
                PlayerEngine::BundledMpv => (
                    "bundled_mpv".to_string(),
                    if !version_str.is_empty() {
                        format!("Bundled mpv ({version_str})")
                    } else {
                        "Bundled mpv (Isolated)".to_string()
                    },
                ),
            };

            let note = if !has_jvm {
                Some("Bundled Java runtime (JRE) not found in this installation. BD-J interactive menus may have limited navigation; standard Blu-ray and DVD menus are fully supported.".to_string())
            } else {
                Some("Full DVD, Blu-ray, and BD-J interactive menu navigation supported.".to_string())
            };

            Ok(DiscPlayerStatus {
                player_type: p_type,
                player_name: p_name,
                player_path: Some(path_str),
                supports_dvd_menu: true,
                supports_bluray_menu: true,
                java_home,
                has_jvm,
                bd_j_supported: has_jvm,
                note,
            })
        }
        None => Ok(DiscPlayerStatus {
            player_type: "none".to_string(),
            player_name: "Bundled Nebulae player not found in this installation".to_string(),
            player_path: None,
            supports_dvd_menu: false,
            supports_bluray_menu: false,
            java_home,
            has_jvm,
            bd_j_supported: false,
            note: Some(
                "The bundled disc player is missing from this installation. Reinstall the official Nebulae package - no system media player is used.".to_string(),
            ),
        }),
    }
}

#[tauri::command]
pub fn launch_disc_player(
    app: tauri::AppHandle,
    file_path: String,
    prefer_menu: Option<bool>,
) -> Result<(), String> {
    let resource_dir = app.path().resource_dir().ok();
    launch_disc_player_impl(resource_dir.as_deref(), &file_path, prefer_menu)
}

/// Core launcher, separated from the Tauri command so it stays testable
/// without an AppHandle.
pub fn launch_disc_player_impl(
    resource_dir: Option<&Path>,
    file_path: &str,
    prefer_menu: Option<bool>,
) -> Result<(), String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }

    let disc_info = DiscDetector::inspect(path);
    let use_menu = prefer_menu.unwrap_or(true);

    // A torrent's save_path is frequently a plain folder (multi-disc torrents
    // store loose disc images, e.g. DISC_1.ISO / DISC_2.ISO) or a download
    // root that contains a titled subfolder with the actual images. Handing
    // a bare directory to the player makes it exit silently. Resolve the
    // actual disc image instead: search the folder and one level of
    // subfolders, preferring "disc 1" style names so an unrelated image in
    // the download root (e.g. an OS installer ISO) never wins.
    let disc_info = disc_info.or_else(|| {
        if !path.is_dir() {
            return None;
        }
        let mut images: Vec<PathBuf> = Vec::new();
        let mut scan = |dir: &Path| {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file()
                        && matches!(
                            p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()),
                            Some(ext) if ext == "iso" || ext == "img"
                        )
                    {
                        images.push(p);
                    }
                }
            }
        };
        scan(path);
        if let Ok(subdirs) = std::fs::read_dir(path) {
            for entry in subdirs.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    scan(&p);
                }
            }
        }
        if images.is_empty() {
            return None;
        }
        // Rank: DISC 1 first, then other disc-N images, then anything else.
        let disc_rank = |p: &Path| {
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
                2
            }
        };
        images.sort_by(|a, b| disc_rank(a).cmp(&disc_rank(b)).then(a.cmp(b)));
        let image = images.first()?;
        tracing::info!(
            "Save destination is a folder; resolved disc image '{}' for playback",
            image.display()
        );
        DiscDetector::inspect(image)
    });

    // Locate preferred disc player (bundled mpv -> system mpv -> VLC -> IINA -> system default)
    if let Some((player_bin, engine)) = find_optical_disc_player(resource_dir) {
        let mut cmd = Command::new(&player_bin);

        // DVDs and many Blu-rays are interlaced. Auto deinterlacing (via
        // the libplacebo-based gpu-next renderer) restores smooth picture
        // without combing and never touches progressive content: only
        // frames actually flagged (or detected) as interlaced are
        // processed.
        cmd.arg("--vo=gpu-next");
        cmd.arg("--deinterlace=auto");

        // Ensure a Java runtime is accessible to libbluray for BD-J and 4K UHD
        // menus. The bundled JRE ships inside the package and is preferred over
        // any system JVM; its bin directory is also prepended to PATH so the
        // JVM can locate its own companion executables.
        let java_home = detect_java_home(resource_dir);
        if let Some(ref jh) = java_home {
            cmd.env("JAVA_HOME", jh);
            if let Some(java_bin) = jh.join("bin").to_str() {
                if let Ok(path_value) =
                    std::env::join_paths(std::iter::once(PathBuf::from(java_bin)).chain(
                        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
                    ))
                {
                    cmd.env("PATH", path_value);
                }
            }
        }

        if let Some(disc) = &disc_info {
            let target = &disc.play_target;
            let title = format!("Nebulae Cinema — {}", disc.title);

            // Log if launching Blu-ray/UHD without JVM
            if (disc.disc_type == DiscType::BluRay || disc.disc_type == DiscType::UhdBluRay)
                && use_menu
                && java_home.is_none()
            {
                tracing::info!(
                    "Launching Blu-ray/UHD menu without detected JVM. Standard menus active; BD-J interactive features require JRE."
                );
            }

            match engine {
                PlayerEngine::BundledMpv => {
                    cmd.arg(format!("--title={title}"));
                    match disc.disc_type {
                        DiscType::DvdVideo | DiscType::DvdAudio => {
                            if use_menu {
                                cmd.arg(format!("--dvd-device={target}"));
                                cmd.arg("dvdnav://");
                            } else {
                                cmd.arg(target);
                            }
                        }
                        DiscType::BluRay | DiscType::UhdBluRay => {
                            if use_menu {
                                cmd.arg(format!("--bluray-device={target}"));
                                cmd.arg("bd://");
                                cmd.arg("--demuxer-lavf-o=bluray_menu=1");
                            } else {
                                cmd.arg(target);
                            }
                        }
                        DiscType::IsoImage => {
                            let lower = target.to_lowercase();
                            let is_bd = lower.contains("bd")
                                || lower.contains("bluray")
                                || lower.contains("uhd")
                                || disc.total_bytes > 9_500_000_000;
                            if is_bd {
                                if use_menu {
                                    cmd.arg(format!("--bluray-device={target}"));
                                    cmd.arg("bd://");
                                    cmd.arg("--demuxer-lavf-o=bluray_menu=1");
                                } else {
                                    cmd.arg(target);
                                }
                            } else if use_menu {
                                cmd.arg(format!("--dvd-device={target}"));
                                cmd.arg("dvdnav://");
                            } else {
                                cmd.arg(target);
                            }
                        }
                        DiscType::Unknown => {
                            cmd.arg(target);
                        }
                    }
                }
            }
        } else {
            cmd.arg(file_path);
        }

        #[cfg(target_os = "windows")]
        {
            // Detach on Windows without popup terminal window
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        // Capture the player's stderr so a fast failure carries its actual
        // diagnostic (dyld errors, bad options, disc open failures) instead
        // of a bare exit status. GUI launches send stderr to /dev/null.
        let mpv_log_path = std::env::temp_dir().join("nebulae-mpv.log");
        if let Ok(log_file) = std::fs::File::create(&mpv_log_path) {
            cmd.stderr(std::process::Stdio::from(log_file));
        }

        match cmd.spawn() {
            Ok(mut child) => {
                // A player that dies immediately (missing libraries, dyld
                // abort, damaged disc image) used to look like a dead
                // button: the spawn succeeded and the process vanished.
                // Give it a moment and surface an immediate non-zero exit
                // as an error so the UI banner explains the failure.
                std::thread::sleep(std::time::Duration::from_millis(1500));
                if let Some(status) = child.try_wait().ok().flatten() {
                    if !status.success() {
                        let said = std::fs::read_to_string(&mpv_log_path).unwrap_or_default();
                        let tail: String = said
                            .chars()
                            .rev()
                            .take(300)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        return Err(format!(
                            "The bundled player exited immediately ({status}). Player output: {}",
                            tail.trim()
                        ));
                    }
                }
                tracing::info!(
                    "Player launch healthy; diagnostics at {}",
                    mpv_log_path.display()
                );
                return Ok(());
            }
            Err(e) => {
                return Err(format!(
                    "Failed to launch the bundled Nebulae player ({engine:?}): {e}. The bundled player requires no external software; if it is missing the installation may be incomplete - reinstall Nebulae or check Settings > Media."
                ));
            }
        }
    }

    // Bundled player is the only supported engine; without it the
    // installation is incomplete.
    Err(
        "The bundled Nebulae disc player is missing from this installation. Reinstall the official Nebulae package - no system media player is used. Check Settings > Media for details."
            .to_string(),
    )
}

#[tauri::command]
pub fn launch_media_player(app: tauri::AppHandle, file_path: String) -> Result<(), String> {
    launch_disc_player(app, file_path, Some(true))
}

/// Candidate directories for the bundled player across every package layout.
/// `resource_dir` is Tauri's canonical resource directory and is probed
/// first; the remaining entries cover installer layouts (NSIS, .app bundle,
/// deb/rpm filesystem paths), the AppImage runtime environment, and
/// development checkouts. The bundled tree is fully self-contained: it
/// carries its own shared libraries and JRE, so no OS-installed player or
/// third-party library is ever required.
fn bundled_player_dir_candidates(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(rd) = resource_dir {
        // Tauri v2 preserves the path relative to src-tauri/ inside the
        // resource directory (e.g. Contents/Resources/resources/player on
        // macOS, $INSTDIR/resources/player on Windows).
        cands.push(rd.join("resources").join("player"));
        cands.push(rd.join("player"));
        cands.push(rd.to_path_buf());
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            cands.push(exe_dir.join("resources").join("player"));
            cands.push(exe_dir.join("resources"));
            cands.push(exe_dir.join("player"));
            cands.push(exe_dir.to_path_buf());
            // deb/rpm layout: /usr/bin/nebulae -> /usr/lib/nebulae/resources/player
            if let Some(prefix) = exe_dir.parent().and_then(|p| p.parent()) {
                cands.push(
                    prefix
                        .join("lib")
                        .join("nebulae")
                        .join("resources")
                        .join("player"),
                );
                cands.push(
                    prefix
                        .join("share")
                        .join("nebulae")
                        .join("resources")
                        .join("player"),
                );
            }
        }
    }
    if let Ok(appdir) = std::env::var("APPDIR") {
        let appdir = PathBuf::from(appdir);
        cands.push(appdir.join("resources").join("player"));
        cands.push(
            appdir
                .join("usr")
                .join("lib")
                .join("nebulae")
                .join("resources")
                .join("player"),
        );
        cands.push(appdir.join("usr").join("bin"));
    }
    #[cfg(target_os = "linux")]
    {
        cands.push(PathBuf::from("/usr/lib/nebulae/resources/player"));
        cands.push(PathBuf::from("/usr/share/nebulae/resources/player"));
    }
    cands
}

/// Discovers the player used for DVD/Blu-ray/UHD menu navigation.
/// Locates the bundled Nebulae disc player. This is the only supported
/// engine: the package ships a self-contained mpv together with its complete
/// shared-library tree and JRE, so a clean machine needs nothing installed
/// and no system player is ever probed or used.
pub fn find_optical_disc_player(resource_dir: Option<&Path>) -> Option<(PathBuf, PlayerEngine)> {
    let mpv_binary = if cfg!(windows) { "mpv.exe" } else { "mpv" };
    for dir in bundled_player_dir_candidates(resource_dir) {
        let cand = dir.join(mpv_binary);
        if cand.is_file() {
            return Some((cand, PlayerEngine::BundledMpv));
        }
    }

    // Development checkout fallbacks (cargo tauri dev runs from src-tauri/)
    let dev_candidates = [
        PathBuf::from("resources/player/mpv.exe"),
        PathBuf::from("src-tauri/resources/player/mpv.exe"),
        PathBuf::from("resources/player/mpv"),
        PathBuf::from("src-tauri/resources/player/mpv"),
    ];
    for cand in &dev_candidates {
        if cand.is_file() {
            return Some((cand.clone(), PlayerEngine::BundledMpv));
        }
    }

    None
}

/// Locate the Java runtime used by libbluray for BD-J and UHD menus.
/// Only the JRE bundled inside the package is used; no system JVM is
/// detected or consulted.
fn detect_java_home(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let java_binary = if cfg!(windows) { "java.exe" } else { "java" };
    for player_dir in bundled_player_dir_candidates(resource_dir) {
        let jre_home = player_dir.join("jre");
        if jre_home.join("bin").join(java_binary).is_file() {
            return Some(jre_home);
        }
    }

    None
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
        return Err(format!("Media path does not exist: {media_path}"));
    }

    let cache_dir = crate::utils::paths::AppPaths::init()
        .map(|p| p.cache_dir)
        .unwrap_or_else(|_| PathBuf::from("."));

    Ok(crate::utils::library_cleaner::LibraryCleaner::get_or_create_thumbnail(path, &cache_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_nonexistent_media() {
        let res = launch_disc_player_impl(None, "/nonexistent/movie/file.mkv", Some(true));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist"));
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
        let status = get_disc_player_status_impl(None).expect("get_disc_player_status failed");
        assert!(!status.player_name.is_empty());
        assert!(!status.player_type.is_empty());
    }

    #[test]
    fn test_disc_player_discovery() {
        // Just verify discovery returns without panic
        let _ = find_optical_disc_player(None);
    }
}

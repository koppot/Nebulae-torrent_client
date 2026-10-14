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
    SystemMpv,
    Vlc,
    Iina,
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
                PlayerEngine::SystemMpv => (
                    "system_mpv".to_string(),
                    if !version_str.is_empty() {
                        format!("System mpv ({version_str})")
                    } else {
                        "System mpv".to_string()
                    },
                ),
                PlayerEngine::Vlc => ("vlc".to_string(), "VLC Media Player".to_string()),
                PlayerEngine::Iina => ("iina".to_string(), "IINA Media Player".to_string()),
            };

            let note = if !has_jvm {
                Some("Java runtime (JRE) not detected. BD-J interactive menus may have limited navigation; standard Blu-ray and DVD menus are fully supported.".to_string())
            } else {
                Some("Full DVD, Blu-ray, and BD-J interactive menu navigation supported.".to_string())
            };

            Ok(DiscPlayerStatus {
                player_type: p_type,
                player_name: p_name,
                player_path: Some(path_str),
                supports_dvd_menu: true,
                supports_bluray_menu: engine != PlayerEngine::Iina,
                java_home,
                has_jvm,
                bd_j_supported: has_jvm && engine != PlayerEngine::Iina,
                note,
            })
        }
        None => Ok(DiscPlayerStatus {
            player_type: "none".to_string(),
            player_name: "None found (system default launcher will be used)".to_string(),
            player_path: None,
            supports_dvd_menu: false,
            supports_bluray_menu: false,
            java_home,
            has_jvm,
            bd_j_supported: false,
            note: Some("No media player detected. For interactive DVD and Blu-ray menus, bundled mpv or system mpv/VLC is recommended.".to_string()),
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

    // Locate preferred disc player (bundled mpv -> system mpv -> VLC -> IINA -> system default)
    if let Some((player_bin, engine)) = find_optical_disc_player(resource_dir) {
        let mut cmd = Command::new(&player_bin);

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
                PlayerEngine::BundledMpv | PlayerEngine::SystemMpv => {
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
                PlayerEngine::Vlc => match disc.disc_type {
                    DiscType::DvdVideo | DiscType::DvdAudio => {
                        if use_menu {
                            cmd.arg(format!("dvd://{target}"));
                        } else {
                            cmd.arg(target);
                        }
                    }
                    DiscType::BluRay | DiscType::UhdBluRay => {
                        if use_menu {
                            cmd.arg("--bluray-menu");
                            cmd.arg(format!("bluray://{target}"));
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
                                cmd.arg("--bluray-menu");
                                cmd.arg(format!("bluray://{target}"));
                            } else {
                                cmd.arg(target);
                            }
                        } else if use_menu {
                            cmd.arg(format!("dvd://{target}"));
                        } else {
                            cmd.arg(target);
                        }
                    }
                    DiscType::Unknown => {
                        cmd.arg(target);
                    }
                },
                PlayerEngine::Iina => {
                    cmd.arg(format!("--mpv-force-media-title={title}"));
                    match disc.disc_type {
                        DiscType::DvdVideo | DiscType::DvdAudio => {
                            if use_menu {
                                cmd.arg(format!("--mpv-dvd-device={target}"));
                                cmd.arg("dvdnav://");
                            } else {
                                cmd.arg(target);
                            }
                        }
                        DiscType::BluRay | DiscType::UhdBluRay => {
                            if use_menu {
                                cmd.arg(format!("--mpv-bluray-device={target}"));
                                cmd.arg("bd://");
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
                                    cmd.arg(format!("--mpv-bluray-device={target}"));
                                    cmd.arg("bd://");
                                } else {
                                    cmd.arg(target);
                                }
                            } else if use_menu {
                                cmd.arg(format!("--mpv-dvd-device={target}"));
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

        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::warn!(
                    "Failed to spawn player {:?} ({:?}): {}. Falling back to default launcher.",
                    player_bin,
                    engine,
                    e
                );
                if launch_system_default(file_path).is_err() {
                    return Err(format!(
                        "Failed to launch optical disc player '{:?}' ({engine:?}): {e}. System fallback launcher also failed. The bundled player should never require external software; if it is missing the installation may be incomplete - reinstall Nebulae or check Settings > Media.",
                        player_bin
                    ));
                }
                return Ok(());
            }
        }
    }

    // Fallback: system default file / directory launcher
    if let Err(fallback_err) = launch_system_default(file_path) {
        return Err(format!(
            "No optical disc player (bundled mpv, system mpv, VLC, or IINA) was found, and system default launcher failed: {fallback_err}. Nebulae normally ships a self-contained player; if it is missing the installation may be incomplete - reinstall, or check Settings > Media."
        ));
    }
    Ok(())
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
/// Probes the bundled self-contained player first, followed by system mpv,
/// VLC, and IINA. The bundled player ships with the package together with
/// its complete shared-library tree and JRE, so a clean machine needs
/// nothing installed.
pub fn find_optical_disc_player(resource_dir: Option<&Path>) -> Option<(PathBuf, PlayerEngine)> {
    // 0. Probe bundled mpv FIRST (self-contained package delivery)
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

    // 1. Probe system mpv candidates
    let mpv_candidates = [
        "/opt/homebrew/bin/mpv",
        "/usr/local/bin/mpv",
        "/usr/bin/mpv",
        "C:\\Program Files\\mpv\\mpv.exe",
        "C:\\mpv\\mpv.exe",
    ];
    for cand in &mpv_candidates {
        let p = PathBuf::from(cand);
        if p.is_file() {
            return Some((p, PlayerEngine::SystemMpv));
        }
    }
    if Command::new("mpv").arg("--version").output().is_ok() {
        return Some((PathBuf::from("mpv"), PlayerEngine::SystemMpv));
    }

    // 2. Probe VLC candidates
    let vlc_candidates = [
        "/Applications/VLC.app/Contents/MacOS/VLC",
        "/usr/bin/vlc",
        "/usr/local/bin/vlc",
        "C:\\Program Files\\VideoLAN\\VLC\\vlc.exe",
        "C:\\Program Files (x86)\\VideoLAN\\VLC\\vlc.exe",
    ];
    for cand in &vlc_candidates {
        let p = PathBuf::from(cand);
        if p.is_file() {
            return Some((p, PlayerEngine::Vlc));
        }
    }
    if Command::new("vlc").arg("--version").output().is_ok() {
        return Some((PathBuf::from("vlc"), PlayerEngine::Vlc));
    }

    // 3. Probe macOS IINA candidates
    #[cfg(target_os = "macos")]
    {
        let iina_candidates = [
            "/Applications/IINA.app/Contents/MacOS/iina-cli",
            "/Applications/IINA.app/Contents/MacOS/IINA",
            "/usr/local/bin/iina",
        ];
        for cand in &iina_candidates {
            let p = PathBuf::from(cand);
            if p.is_file() {
                return Some((p, PlayerEngine::Iina));
            }
        }
    }

    None
}

/// Locate the Java runtime used by libbluray for BD-J and UHD menus.
/// The JRE bundled inside the package is preferred; a system JVM
/// (JAVA_HOME or OS locations) is only a fallback.
fn detect_java_home(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let java_binary = if cfg!(windows) { "java.exe" } else { "java" };
    for player_dir in bundled_player_dir_candidates(resource_dir) {
        let jre_home = player_dir.join("jre");
        if jre_home.join("bin").join(java_binary).is_file() {
            return Some(jre_home);
        }
    }

    if let Ok(env_java) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(env_java);
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("/usr/libexec/java_home").output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    let p = PathBuf::from(s);
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }

        let jvm_dir = Path::new("/Library/Java/JavaVirtualMachines");
        if let Ok(entries) = std::fs::read_dir(jvm_dir) {
            for entry in entries.flatten() {
                let home = entry.path().join("Contents/Home");
                if home.exists() {
                    return Some(home);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let java_dir = Path::new("C:\\Program Files\\Java");
        if let Ok(entries) = std::fs::read_dir(java_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    return Some(entry.path());
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let jvm_dir = Path::new("/usr/lib/jvm");
        if let Ok(entries) = std::fs::read_dir(jvm_dir) {
            let default_java = jvm_dir.join("default-java");
            if default_java.exists() {
                return Some(default_java);
            }
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    return Some(entry.path());
                }
            }
        }
        let jvm64_dir = Path::new("/usr/lib64/jvm");
        if let Ok(entries) = std::fs::read_dir(jvm64_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    return Some(entry.path());
                }
            }
        }
    }

    None
}

fn launch_system_default(file_path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg(file_path)
            .status()
            .map_err(|e| format!("Failed to launch media player on macOS: {e}"))?;

        if !status.success() {
            return Err("Failed to launch default media player".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = crate::utils::process::create_hidden_command("cmd.exe");
        cmd.args(["/C", "start", "", file_path]);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to launch media player on Windows: {e}"))?;

        if !status.success() {
            return Err("Failed to launch default media player on Windows".to_string());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let status = Command::new("xdg-open")
            .arg(file_path)
            .status()
            .map_err(|e| format!("Failed to launch media player on Linux: {e}"))?;

        if !status.success() {
            return Err("Failed to launch media player via xdg-open".to_string());
        }
    }

    Ok(())
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

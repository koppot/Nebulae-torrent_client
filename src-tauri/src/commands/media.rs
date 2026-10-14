use crate::utils::disc_detector::{DiscDetector, DiscStructureInfo, DiscType};
use crate::utils::media_parser::{MediaTitleParser, ParsedMediaMetadata};
use std::path::{Path, PathBuf};
use std::process::Command;

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

#[tauri::command]
pub fn launch_disc_player(file_path: String, prefer_menu: Option<bool>) -> Result<(), String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Media path does not exist: {file_path}"));
    }

    let disc_info = DiscDetector::inspect(path);
    let use_menu = prefer_menu.unwrap_or(true);

    // Locate preferred disc player (bundled mpv -> system mpv -> system VLC -> system default)
    if let Some((player_bin, is_mpv)) = find_optical_disc_player() {
        let mut cmd = Command::new(&player_bin);

        // Ensure Java runtime is accessible to libbluray for BD-J and 4K UHD menus
        if let Some(java_home) = detect_java_home() {
            cmd.env("JAVA_HOME", java_home);
        }

        if let Some(disc) = &disc_info {
            let target = &disc.play_target;
            let title = format!("Nebulae Cinema — {}", disc.title);

            if is_mpv {
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
                    DiscType::IsoImage | DiscType::Unknown => {
                        cmd.arg(target);
                    }
                }
            } else {
                // VLC command line arguments
                match disc.disc_type {
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
                    DiscType::IsoImage | DiscType::Unknown => {
                        cmd.arg(target);
                    }
                }
            }
        } else {
            cmd.arg(&file_path);
        }

        #[cfg(target_os = "windows")]
        {
            // Detach on Windows without popup terminal window
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let spawned = cmd.spawn();
        if spawned.is_ok() {
            return Ok(());
        }
    }

    // Fallback: system default file / directory launcher
    launch_system_default(&file_path)
}

#[tauri::command]
pub fn launch_media_player(file_path: String) -> Result<(), String> {
    launch_disc_player(file_path, Some(true))
}

/// Discovers an installed player capable of DVD/Blu-ray/UHD menu navigation.
fn find_optical_disc_player() -> Option<(PathBuf, bool)> {
    // 1. Check mpv candidates
    let mpv_candidates = [
        "/opt/homebrew/bin/mpv",
        "/usr/local/bin/mpv",
        "/usr/bin/mpv",
        "C:\\Program Files\\mpv\\mpv.exe",
        "C:\\mpv\\mpv.exe",
    ];
    for cand in &mpv_candidates {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Some((p, true));
        }
    }
    // Check if mpv is in system PATH
    if Command::new("mpv").arg("--version").output().is_ok() {
        return Some((PathBuf::from("mpv"), true));
    }

    // 2. Check VLC candidates
    let vlc_candidates = [
        "/Applications/VLC.app/Contents/MacOS/VLC",
        "/usr/bin/vlc",
        "/usr/local/bin/vlc",
        "C:\\Program Files\\VideoLAN\\VLC\\vlc.exe",
        "C:\\Program Files (x86)\\VideoLAN\\VLC\\vlc.exe",
    ];
    for cand in &vlc_candidates {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Some((p, false));
        }
    }
    if Command::new("vlc").arg("--version").output().is_ok() {
        return Some((PathBuf::from("vlc"), false));
    }

    None
}

/// Locate Java runtime directory for BD-J and UHD menu execution in libbluray.
fn detect_java_home() -> Option<PathBuf> {
    if let Ok(env_java) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(env_java);
        if p.exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "macos")]
    {
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
        let res = launch_media_player("/nonexistent/movie/file.mkv".to_string());
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
}

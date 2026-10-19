use crate::utils::disc_detector::DiscDetector;
use crate::utils::media_parser::MediaTitleParser;
use crate::utils::process::create_hidden_command;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeSpaceInfo {
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheCleanupReport {
    pub scanned_directories: Vec<String>,
    pub files_scanned: usize,
    pub files_removed: usize,
    pub bytes_reclaimed: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizeMediaReport {
    pub source_path: String,
    pub destination_path: String,
    pub files_moved: usize,
    pub bytes_moved: u64,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThumbnailResult {
    pub thumbnail_path: Option<String>,
    pub is_disc_artwork: bool,
    pub source: String,
}

pub struct LibraryCleaner;

impl LibraryCleaner {
    /// Measure volume disk space (total, available, used) for a target directory or mount point.
    pub fn get_disk_space(target_path: &Path) -> Result<VolumeSpaceInfo, String> {
        let path = if target_path.exists() {
            target_path.to_path_buf()
        } else {
            dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
        };

        #[cfg(unix)]
        {
            Self::get_unix_disk_space(&path)
        }

        #[cfg(windows)]
        {
            Self::get_windows_disk_space(&path)
        }
    }

    #[cfg(unix)]
    fn get_unix_disk_space(path: &Path) -> Result<VolumeSpaceInfo, String> {
        let output = std::process::Command::new("df")
            .arg("-P")
            .arg("-k")
            .arg(path)
            .output()
            .map_err(|e| format!("Failed to execute df command: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("df command failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_df_output(&stdout)
    }

    pub fn parse_df_output(stdout: &str) -> Result<VolumeSpaceInfo, String> {
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 2 {
            return Err("Unexpected df output format: insufficient lines".to_string());
        }

        let data_line = lines[1];
        let tokens: Vec<&str> = data_line.split_whitespace().collect();
        if tokens.len() < 6 {
            return Err(format!(
                "Unexpected df output column count: {}",
                tokens.len()
            ));
        }

        let total_kib = tokens[1]
            .parse::<u64>()
            .map_err(|e| format!("Invalid total KiB: {e}"))?;
        let used_kib = tokens[2]
            .parse::<u64>()
            .map_err(|e| format!("Invalid used KiB: {e}"))?;
        let avail_kib = tokens[3]
            .parse::<u64>()
            .map_err(|e| format!("Invalid available KiB: {e}"))?;

        let percent_str = tokens[4].trim_end_matches('%');
        let used_percent = percent_str.parse::<u8>().unwrap_or(0);
        let mount_point = tokens[5].to_string();

        Ok(VolumeSpaceInfo {
            mount_point,
            total_bytes: total_kib * 1024,
            available_bytes: avail_kib * 1024,
            used_bytes: used_kib * 1024,
            used_percent,
        })
    }

    #[cfg(windows)]
    fn get_windows_disk_space(path: &Path) -> Result<VolumeSpaceInfo, String> {
        let escaped_path = path.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$p = '{}'; $drive = (Get-Item -LiteralPath $p).PSDrive; if ($drive) {{ [PSCustomObject]@{{ Mount = $drive.Root; Total = ($drive.Used + $drive.Free); Used = $drive.Used; Free = $drive.Free }} | ConvertTo-Json -Compress }} else {{ [PSCustomObject]@{{ Mount = 'C:\\'; Total = 1000000000000; Used = 500000000000; Free = 500000000000 }} | ConvertTo-Json -Compress }}",
            escaped_path
        );

        let output = crate::utils::process::run_hidden_powershell(&script)
            .map_err(|e| format!("Failed to query disk space via PowerShell: {e}"))?;

        if !output.status.success() {
            return Err("PowerShell disk space query failed".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_powershell_disk_json(&stdout)
    }

    pub fn parse_powershell_disk_json(json_str: &str) -> Result<VolumeSpaceInfo, String> {
        #[derive(Deserialize)]
        struct PsDisk {
            #[serde(rename = "Mount")]
            mount: Option<String>,
            #[serde(rename = "Total")]
            total: Option<u64>,
            #[serde(rename = "Used")]
            used: Option<u64>,
            #[serde(rename = "Free")]
            free: Option<u64>,
        }

        let parsed: PsDisk = serde_json::from_str(json_str.trim())
            .map_err(|e| format!("Failed to parse PowerShell disk JSON: {e}"))?;

        let total_bytes = parsed.total.unwrap_or(0);
        let used_bytes = parsed.used.unwrap_or(0);
        let available_bytes = parsed.free.unwrap_or(0);
        let used_percent = if total_bytes > 0 {
            ((used_bytes as f64 / total_bytes as f64) * 100.0).round() as u8
        } else {
            0
        };

        Ok(VolumeSpaceInfo {
            mount_point: parsed.mount.unwrap_or_else(|| "C:\\".to_string()),
            total_bytes,
            available_bytes,
            used_bytes,
            used_percent,
        })
    }

    /// Prunes orphaned cache files and old watched torrent markers.
    pub fn clean_cache_files(
        cache_dirs: &[PathBuf],
        older_than_days: u32,
        dry_run: bool,
    ) -> Result<CacheCleanupReport, String> {
        let max_age = Duration::from_secs(older_than_days as u64 * 86400);
        let now = SystemTime::now();

        let mut scanned_directories = Vec::new();
        let mut files_scanned = 0;
        let mut files_removed = 0;
        let mut bytes_reclaimed = 0;

        for dir in cache_dirs {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }
            scanned_directories.push(dir.to_string_lossy().to_string());

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        files_scanned += 1;
                        let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(now);

                        let should_remove = match now.duration_since(modified) {
                            Ok(age) => age >= max_age,
                            Err(_) => false,
                        };

                        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        // Target temporary chunks, old thumbnail images, or .torrent.loaded markers
                        let is_cache_candidate = name.ends_with(".loaded")
                            || name.ends_with(".part")
                            || name.ends_with(".tmp")
                            || name.ends_with(".neb")
                            || dir.to_string_lossy().contains("thumbnails");

                        if should_remove && is_cache_candidate {
                            bytes_reclaimed += file_size;
                            files_removed += 1;
                            if !dry_run {
                                let _ = fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }

        Ok(CacheCleanupReport {
            scanned_directories,
            files_scanned,
            files_removed,
            bytes_reclaimed,
            dry_run,
        })
    }

    /// Formats target directory path according to archival media organization conventions.
    pub fn format_organize_destination(dest_root: &Path, raw_name: &str) -> PathBuf {
        let meta = MediaTitleParser::parse(raw_name);
        let disc_info = DiscDetector::inspect(Path::new(raw_name));

        let clean_title = if !meta.clean_title.is_empty() {
            meta.clean_title
        } else {
            raw_name.to_string()
        };

        let folder_name = if let Some(year) = meta.year {
            format!("{clean_title} ({year})")
        } else {
            clean_title.clone()
        };

        let base_dest = dest_root.join(folder_name);

        // Check for multi-disc boxset organization
        if let Some(disc) = disc_info {
            if let Some(disc_num) = disc.disc_number {
                return base_dest.join(format!("Disc {disc_num}"));
            }
        }

        // Check for Season / Episode series organization
        if let (Some(s), Some(_e)) = (meta.season, meta.episode) {
            return dest_root
                .join("Series")
                .join(&clean_title)
                .join(format!("Season {s:02}"));
        }

        base_dest
    }

    /// Moves a completed torrent's files into an organized media library directory.
    pub fn organize_completed_media(
        source_path: &Path,
        destination_root: &Path,
    ) -> Result<OrganizeMediaReport, String> {
        if !source_path.exists() {
            return Err(format!(
                "Source path does not exist: {}",
                source_path.display()
            ));
        }

        if !destination_root.exists() {
            fs::create_dir_all(destination_root)
                .map_err(|e| format!("Failed to create destination root directory: {e}"))?;
        }

        let raw_name = source_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown Media".to_string());

        let target_dir = Self::format_organize_destination(destination_root, &raw_name);

        fs::create_dir_all(&target_dir).map_err(|e| {
            format!(
                "Failed to create destination directory {}: {e}",
                target_dir.display()
            )
        })?;

        let mut files_moved = 0;
        let mut bytes_moved = 0;

        if source_path.is_file() {
            let file_name = source_path.file_name().unwrap();
            let dest_file = target_dir.join(file_name);
            let size = source_path.metadata().map(|m| m.len()).unwrap_or(0);

            // Attempt atomic rename first; fallback to copy + delete across mount points
            if fs::rename(source_path, &dest_file).is_err() {
                fs::copy(source_path, &dest_file)
                    .map_err(|e| format!("Failed to copy file to destination: {e}"))?;
                let _ = fs::remove_file(source_path);
            }
            files_moved += 1;
            bytes_moved += size;
        } else if source_path.is_dir() {
            // Move directory contents
            Self::move_dir_contents(source_path, &target_dir, &mut files_moved, &mut bytes_moved)?;
            let _ = fs::remove_dir(source_path);
        }

        Ok(OrganizeMediaReport {
            source_path: source_path.to_string_lossy().to_string(),
            destination_path: target_dir.to_string_lossy().to_string(),
            files_moved,
            bytes_moved,
            success: true,
            message: format!("Organized {files_moved} files ({bytes_moved} bytes) successfully"),
        })
    }

    fn move_dir_contents(
        src: &Path,
        dst: &Path,
        files_moved: &mut usize,
        bytes_moved: &mut u64,
    ) -> Result<(), String> {
        for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
            let entry_path = entry.path();
            let dest_entry = dst.join(entry.file_name());

            if entry_path.is_dir() {
                fs::create_dir_all(&dest_entry).map_err(|e| e.to_string())?;
                Self::move_dir_contents(&entry_path, &dest_entry, files_moved, bytes_moved)?;
                let _ = fs::remove_dir(&entry_path);
            } else if entry_path.is_file() {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if fs::rename(&entry_path, &dest_entry).is_err() {
                    fs::copy(&entry_path, &dest_entry).map_err(|e| e.to_string())?;
                    let _ = fs::remove_file(&entry_path);
                }
                *files_moved += 1;
                *bytes_moved += size;
            }
        }
        Ok(())
    }

    /// Generates or fetches an authentic thumbnail for a media item.
    /// Checks optical disc artwork scans first, then cached thumbnails, then ffmpeg if installed.
    pub fn get_or_create_thumbnail(media_path: &Path, cache_dir: &Path) -> ThumbnailResult {
        // 1. Check for optical disc cover artwork scans. The raw scan is
        // downscaled + re-encoded as WebP into the cache so the media grid
        // never loads multi-MB originals (owner directive: WebP, small).
        if let Some(cover) = DiscDetector::find_cover_image(media_path) {
            let cover_path = Path::new(&cover);
            match crate::thumbs::ensure_thumbnail(cache_dir, cover_path) {
                Ok(webp_path) => {
                    return ThumbnailResult {
                        thumbnail_path: Some(webp_path.display().to_string()),
                        is_disc_artwork: true,
                        source: "disc_artwork".to_string(),
                    };
                }
                Err(e) => {
                    // Non-fatal: fall back to the original scan rather than
                    // showing no art at all.
                    eprintln!("thumbnail webp encode failed: {e}");
                    return ThumbnailResult {
                        thumbnail_path: Some(cover),
                        is_disc_artwork: true,
                        source: "disc_artwork".to_string(),
                    };
                }
            }
        }

        // 2. Compute stable cache file path
        let file_stem = media_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("media");
        let hash = format!("{:x}", md5_hash(media_path.to_string_lossy().as_bytes()));
        let thumb_dir = cache_dir.join("thumbnails");
        let cached_thumb = thumb_dir.join(format!("{file_stem}_{hash}.jpg"));

        if cached_thumb.exists() {
            return ThumbnailResult {
                thumbnail_path: Some(cached_thumb.to_string_lossy().to_string()),
                is_disc_artwork: false,
                source: "cached".to_string(),
            };
        }

        // 3. Attempt video frame extraction via ffmpeg if installed.
        // The source is the file itself (single-file torrent) or the largest
        // video inside the release folder (a folder torrent whose video list
        // is the ISO set). ISO handling is unified below.
        let target_video = if media_path.is_file() {
            Some(media_path.to_path_buf())
        } else if media_path.is_dir() {
            // Find largest video file in directory
            find_largest_video_file(media_path)
        } else {
            None
        };

        if let Some(video) = target_video {
            let ext = video
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            #[cfg(target_os = "macos")]
            if ext == "iso" || ext == "img" {
                // The generic demuxer cannot seek a raw ISO — every -ss
                // re-decodes frame 0, which is the black DVD-menu loop
                // (three identical 2.2KB black frames at 5/12 min proved
                // it). Mount the image read-only and extract from the
                // largest VOB/video inside instead.
                if let Some((mount_point, inner_video)) =
                    Self::mount_disc_image_for_extraction(&video)
                {
                    let result = Self::extract_frame(&inner_video, &thumb_dir, &cached_thumb);
                    let _ = create_hidden_command("hdiutil")
                        .args(["detach", "-quiet", &mount_point.to_string_lossy()])
                        .output();
                    if let Some(r) = result {
                        return r;
                    }
                    return ThumbnailResult {
                        thumbnail_path: None,
                        is_disc_artwork: false,
                        source: "none".to_string(),
                    };
                }
                // Mount failed: try the raw image below.
            }

            if let Some(result) = Self::extract_frame(&video, &thumb_dir, &cached_thumb) {
                return result;
            }
        }

        ThumbnailResult {
            thumbnail_path: None,
            is_disc_artwork: false,
            source: "none".to_string(),
        }
    }

    /// Shared frame extraction: seeks deep past the DVD menu and retries
    /// later positions while the frame is near-black (a black 720x480 JPEG
    /// compresses to ~2KB; real content ~10x that).
    fn extract_frame(
        video: &Path,
        thumb_dir: &Path,
        cached_thumb: &Path,
    ) -> Option<ThumbnailResult> {
        if fs::create_dir_all(thumb_dir).is_err() {
            return None;
        }
        let seeks = ["00:05:00", "00:12:00", "00:20:00", "00:00:30"];
        for (idx, ss) in seeks.iter().enumerate() {
            let mut cmd = create_hidden_command("ffmpeg");
            cmd.args([
                "-ss",
                ss,
                "-i",
                &video.to_string_lossy(),
                // DVDs store anamorphic frames (720x480 stretched to 16:9 or
                // 4:3 on display). Without this correction the extracted
                // poster looks horizontally squished ("width and height do
                // not correspond" — owner). Scale to square pixels; SAR=1
                // sources pass through unchanged.
                "-vf",
                "scale=trunc(iw*sar/2)*2:ih",
                "-vframes",
                "1",
                "-q:v",
                "2",
                "-y",
                &cached_thumb.to_string_lossy(),
            ]);

            if let Ok(output) = cmd.output() {
                if output.status.success() && cached_thumb.exists() {
                    let size = fs::metadata(cached_thumb).map(|m| m.len()).unwrap_or(0);
                    if size > 15_000 || idx == seeks.len() - 1 {
                        return Some(ThumbnailResult {
                            thumbnail_path: Some(cached_thumb.to_string_lossy().to_string()),
                            is_disc_artwork: false,
                            source: "video_frame".to_string(),
                        });
                    }
                }
            }
        }
        None
    }

    /// Mount a disc image read-only (macOS hdiutil) and return the largest
    /// video file inside together with the mount point (the caller MUST
    /// detach the mount point after extracting).
    #[cfg(target_os = "macos")]
    fn mount_disc_image_for_extraction(image: &Path) -> Option<(PathBuf, PathBuf)> {
        let mount_point =
            std::env::temp_dir().join(format!("neb-iso-mount-{}", std::process::id()));
        let _ = fs::remove_dir_all(&mount_point);
        let ok = create_hidden_command("hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-mountpoint",
                &mount_point.to_string_lossy(),
                &image.to_string_lossy(),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return None;
        }
        Some((mount_point.clone(), find_largest_video_file(&mount_point)?))
    }
}

fn md5_hash(data: &[u8]) -> u128 {
    // Simple fast 128-bit hash for filename caching
    let mut hash: u128 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u128;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn find_largest_video_file(dir: &Path) -> Option<PathBuf> {
    let mut largest: Option<(PathBuf, u64)> = None;
    let video_exts = [".mkv", ".mp4", ".ts", ".m2ts", ".vob", ".avi", ".iso"];

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = p.to_string_lossy().to_lowercase();
                if video_exts.iter().any(|ext| name.ends_with(ext)) {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    if largest.as_ref().is_none_or(|(_, s)| size > *s) {
                        largest = Some((p, size));
                    }
                }
            } else if p.is_dir() {
                if let Some((sub_p, sub_size)) = find_largest_video_file_internal(&p, 0, 3) {
                    if largest.as_ref().is_none_or(|(_, s)| sub_size > *s) {
                        largest = Some((sub_p, sub_size));
                    }
                }
            }
        }
    }

    largest.map(|(p, _)| p)
}

fn find_largest_video_file_internal(
    dir: &Path,
    depth: usize,
    max_depth: usize,
) -> Option<(PathBuf, u64)> {
    if depth > max_depth {
        return None;
    }
    let mut largest: Option<(PathBuf, u64)> = None;
    let video_exts = [".mkv", ".mp4", ".ts", ".m2ts", ".vob", ".avi", ".iso"];

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = p.to_string_lossy().to_lowercase();
                if video_exts.iter().any(|ext| name.ends_with(ext)) {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    if largest.as_ref().is_none_or(|(_, s)| size > *s) {
                        largest = Some((p, size));
                    }
                }
            } else if p.is_dir() {
                if let Some((sub_p, sub_size)) =
                    find_largest_video_file_internal(&p, depth + 1, max_depth)
                {
                    if largest.as_ref().is_none_or(|(_, s)| sub_size > *s) {
                        largest = Some((sub_p, sub_size));
                    }
                }
            }
        }
    }
    largest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_df_output() {
        let df_sample = "Filesystem     1024-blocks       Used Available Capacity  Mounted on\n/dev/disk3s1s1  1876557824   12338140  71804348    15%    /\n";
        let info = LibraryCleaner::parse_df_output(df_sample).unwrap();
        assert_eq!(info.mount_point, "/");
        assert_eq!(info.total_bytes, 1876557824 * 1024);
        assert_eq!(info.used_bytes, 12338140 * 1024);
        assert_eq!(info.available_bytes, 71804348 * 1024);
        assert_eq!(info.used_percent, 15);
    }

    #[test]
    fn test_parse_powershell_disk_json() {
        let json_sample =
            r#"{"Mount":"C:\\","Total":1000000000000,"Used":400000000000,"Free":600000000000}"#;
        let info = LibraryCleaner::parse_powershell_disk_json(json_sample).unwrap();
        assert_eq!(info.mount_point, "C:\\");
        assert_eq!(info.total_bytes, 1000000000000);
        assert_eq!(info.used_bytes, 400000000000);
        assert_eq!(info.available_bytes, 600000000000);
        assert_eq!(info.used_percent, 40);
    }

    #[test]
    fn test_format_organize_destination_movie() {
        let root = Path::new("/Media/Archive");
        let dest = LibraryCleaner::format_organize_destination(
            root,
            "Rashomon.1950.1080p.BluRay.x264-ADC",
        );
        assert_eq!(dest, PathBuf::from("/Media/Archive/Rashomon (1950)"));
    }

    #[test]
    fn test_format_organize_destination_series() {
        let root = Path::new("/Media/Archive");
        let dest =
            LibraryCleaner::format_organize_destination(root, "Tokyo.Vice.S01E03.1080p.WEB-DL-ADC");
        assert_eq!(
            dest,
            PathBuf::from("/Media/Archive/Series/Tokyo Vice/Season 01")
        );
    }

    #[test]
    fn test_clean_cache_files_dry_run() {
        let temp_dir =
            std::env::temp_dir().join(format!("nebulae_cleaner_test_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let test_file = temp_dir.join("test.torrent.loaded");
        fs::write(&test_file, b"test loaded marker").unwrap();

        let report =
            LibraryCleaner::clean_cache_files(std::slice::from_ref(&temp_dir), 0, true).unwrap();
        assert_eq!(report.files_scanned, 1);
        assert_eq!(report.files_removed, 1);
        assert!(test_file.exists()); // Dry run should not delete

        let active_report =
            LibraryCleaner::clean_cache_files(std::slice::from_ref(&temp_dir), 0, false).unwrap();
        assert_eq!(active_report.files_removed, 1);
        assert!(!test_file.exists()); // Active run deletes candidate

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

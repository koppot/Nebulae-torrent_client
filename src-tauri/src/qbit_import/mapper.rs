use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentLayout {
    Original,
    Subfolder,
    NoSubfolder,
}

impl ContentLayout {
    pub fn parse(s: Option<&str>) -> Self {
        match s.map(|v| v.trim().to_lowercase()).as_deref() {
            Some("subfolder") | Some("createsubfolder") => ContentLayout::Subfolder,
            Some("nosubfolder") | Some("donotcreatesubfolder") => ContentLayout::NoSubfolder,
            _ => ContentLayout::Original,
        }
    }
}

pub struct QbitMapper;

impl QbitMapper {
    /// Detects if a save path was created on an incompatible operating system
    pub fn check_cross_os(save_path: &str) -> Option<String> {
        let trimmed = save_path.trim();
        if trimmed.is_empty() {
            return None;
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Running on macOS / Linux, but path has a Windows drive letter (e.g. C:\) or UNC (\\)
            if (trimmed.len() >= 2
                && trimmed.as_bytes()[1] == b':'
                && trimmed.as_bytes()[0].is_ascii_alphabetic())
                || trimmed.starts_with(r"\\")
            {
                return Some(format!(
                    "Cross-OS path detected: Windows path '{}' cannot be resolved on Unix/macOS without manual relocation.",
                    trimmed
                ));
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Running on Windows, but path is Unix style root (e.g. /Users/... or /Volumes/...)
            if trimmed.starts_with('/') {
                return Some(format!(
                    "Cross-OS path detected: Unix path '{}' cannot be resolved on Windows without manual relocation.",
                    trimmed
                ));
            }
        }

        None
    }

    /// Computes the effective download directory to pass to the engine
    /// taking into account content layout (Original, Subfolder, NoSubfolder)
    /// and whether the torrent is multi-file.
    pub fn compute_effective_download_dir(
        save_path: &str,
        torrent_name: &str,
        is_multi_file: bool,
        layout: &ContentLayout,
    ) -> PathBuf {
        let base = PathBuf::from(save_path.trim());

        match layout {
            ContentLayout::Original => {
                // Default qBittorrent behavior:
                // Single-file => in base
                // Multi-file => in base/<torrent_name>
                base
            }
            ContentLayout::Subfolder => {
                // Forces subfolder creation
                if !is_multi_file {
                    base.join(torrent_name)
                } else {
                    base
                }
            }
            ContentLayout::NoSubfolder => {
                // Multi-file contents put directly into base folder
                base
            }
        }
    }

    /// Computes the expected on-disk location of a file in the torrent
    pub fn compute_file_disk_path(
        effective_download_dir: &Path,
        torrent_name: &str,
        file_rel_path: &str,
        is_multi_file: bool,
        layout: &ContentLayout,
    ) -> PathBuf {
        let normalized_file = file_rel_path.replace('\\', "/");

        match layout {
            ContentLayout::Original => {
                if is_multi_file {
                    // For multi-file, file_rel_path typically includes torrent_name/ or starts with rel path
                    if normalized_file.starts_with(torrent_name) {
                        effective_download_dir.join(&normalized_file)
                    } else {
                        effective_download_dir
                            .join(torrent_name)
                            .join(&normalized_file)
                    }
                } else {
                    effective_download_dir.join(&normalized_file)
                }
            }
            ContentLayout::Subfolder => {
                if is_multi_file {
                    if normalized_file.starts_with(torrent_name) {
                        effective_download_dir.join(&normalized_file)
                    } else {
                        effective_download_dir
                            .join(torrent_name)
                            .join(&normalized_file)
                    }
                } else {
                    effective_download_dir.join(&normalized_file)
                }
            }
            ContentLayout::NoSubfolder => {
                // Strip leading torrent_name/ if present in multi-file relative path
                let stripped = if is_multi_file && normalized_file.starts_with(torrent_name) {
                    normalized_file
                        .strip_prefix(torrent_name)
                        .unwrap_or(&normalized_file)
                        .trim_start_matches('/')
                } else {
                    &normalized_file
                };
                effective_download_dir.join(stripped)
            }
        }
    }

    /// Maps qBittorrent file priority (0 = skip, 1 = normal, 7 = max)
    /// to Nebula/Transmission UI priority (0 = do not download, 1 = normal, 2 = high)
    pub fn map_file_priority(qbt_priority: u8) -> u8 {
        match qbt_priority {
            0 => 0,
            1 => 1,
            _ => 2, // 2..=7 mapped to high priority
        }
    }

    /// Validates if an existing payload file or incomplete file (.!qB) exists on disk
    pub fn find_existing_payload_file(expected_path: &Path) -> Option<PathBuf> {
        if expected_path.is_file() {
            return Some(expected_path.to_path_buf());
        }

        // Check for incomplete qBittorrent extension: file.ext.!qB
        let mut qb_ext_path = expected_path.as_os_str().to_os_string();
        qb_ext_path.push(".!qB");
        let qb_path = PathBuf::from(qb_ext_path);
        if qb_path.is_file() {
            return Some(qb_path);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_layout_parsing() {
        assert_eq!(
            ContentLayout::parse(Some("Original")),
            ContentLayout::Original
        );
        assert_eq!(
            ContentLayout::parse(Some("Subfolder")),
            ContentLayout::Subfolder
        );
        assert_eq!(
            ContentLayout::parse(Some("NoSubfolder")),
            ContentLayout::NoSubfolder
        );
        assert_eq!(ContentLayout::parse(None), ContentLayout::Original);
    }

    #[test]
    fn test_content_layout_original() {
        let base = "/downloads";
        let t_name = "MyTorrent";
        let eff = QbitMapper::compute_effective_download_dir(
            base,
            t_name,
            true,
            &ContentLayout::Original,
        );
        assert_eq!(eff, PathBuf::from("/downloads"));

        let f_path = QbitMapper::compute_file_disk_path(
            &eff,
            t_name,
            "sub/file.mkv",
            true,
            &ContentLayout::Original,
        );
        assert_eq!(f_path, PathBuf::from("/downloads/MyTorrent/sub/file.mkv"));
    }

    #[test]
    fn test_content_layout_subfolder() {
        let base = "/downloads";
        let t_name = "SingleVideo";
        let eff = QbitMapper::compute_effective_download_dir(
            base,
            t_name,
            false,
            &ContentLayout::Subfolder,
        );
        assert_eq!(eff, PathBuf::from("/downloads/SingleVideo"));

        let f_path = QbitMapper::compute_file_disk_path(
            &eff,
            t_name,
            "movie.mkv",
            false,
            &ContentLayout::Subfolder,
        );
        assert_eq!(f_path, PathBuf::from("/downloads/SingleVideo/movie.mkv"));
    }

    #[test]
    fn test_content_layout_nosubfolder() {
        let base = "/downloads";
        let t_name = "MultiTorrent";
        let eff = QbitMapper::compute_effective_download_dir(
            base,
            t_name,
            true,
            &ContentLayout::NoSubfolder,
        );
        assert_eq!(eff, PathBuf::from("/downloads"));

        let f_path = QbitMapper::compute_file_disk_path(
            &eff,
            t_name,
            "MultiTorrent/episode1.mkv",
            true,
            &ContentLayout::NoSubfolder,
        );
        assert_eq!(f_path, PathBuf::from("/downloads/episode1.mkv"));
    }

    #[test]
    fn test_cross_os_detection() {
        #[cfg(not(target_os = "windows"))]
        {
            assert!(QbitMapper::check_cross_os("D:\\Anime\\Show").is_some());
            assert!(QbitMapper::check_cross_os("C:/Users/dev/Downloads").is_some());
            assert!(QbitMapper::check_cross_os(r"\\NAS\share\data").is_some());
            assert!(QbitMapper::check_cross_os("/Users/dev/Downloads").is_none());
        }

        #[cfg(target_os = "windows")]
        {
            assert!(QbitMapper::check_cross_os("/Volumes/2TB/Movies").is_some());
            assert!(QbitMapper::check_cross_os("D:\\Anime\\Show").is_none());
        }
    }

    #[test]
    fn test_priority_mapping() {
        assert_eq!(QbitMapper::map_file_priority(0), 0);
        assert_eq!(QbitMapper::map_file_priority(1), 1);
        assert_eq!(QbitMapper::map_file_priority(7), 2);
    }
}

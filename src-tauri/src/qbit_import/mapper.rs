use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentLayout {
    Original,
    Subfolder,
    NoSubfolder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameOperation {
    pub is_folder: bool,
    pub file_index: Option<u32>,
    pub old_path: String,
    pub new_name: String,
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

    /// Computes hierarchical rename operations (root folder, intermediate directories, and files)
    /// required to transform original metainfo paths into qBittorrent mapped_files on disk.
    pub fn compute_rename_operations(
        original_name: &str,
        original_paths: &[String],
        mapped_files: &[String],
        _layout: &ContentLayout,
    ) -> Vec<RenameOperation> {
        let mut ops = Vec::new();
        if mapped_files.is_empty() || original_paths.is_empty() {
            return ops;
        }

        let is_multi = original_paths.len() > 1 || original_paths[0].contains('/');

        if is_multi {
            let norm_orig: Vec<Vec<String>> = original_paths
                .iter()
                .map(|p| p.replace('\\', "/").split('/').map(String::from).collect())
                .collect();
            let norm_mapped: Vec<Vec<String>> = mapped_files
                .iter()
                .map(|p| p.replace('\\', "/").split('/').map(String::from).collect())
                .collect();

            // 1. Detect top-level folder rename
            let mut current_root = original_name.to_string();
            if let Some(first_orig) = norm_orig.first() {
                if let Some(first_mapped) = norm_mapped.first() {
                    if first_orig.len() > 1 && first_mapped.len() > 1 {
                        let orig_root = &first_orig[0];
                        let mapped_root = &first_mapped[0];
                        if orig_root != mapped_root {
                            ops.push(RenameOperation {
                                is_folder: true,
                                file_index: None,
                                old_path: orig_root.clone(),
                                new_name: mapped_root.clone(),
                            });
                            current_root = mapped_root.clone();
                        }
                    }
                }
            }

            // 2. Detect intermediate directory renames
            let mut renamed_dirs: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for (idx, orig_parts) in norm_orig.iter().enumerate() {
                if let Some(mapped_parts) = norm_mapped.get(idx) {
                    if orig_parts.len() > 2 && mapped_parts.len() == orig_parts.len() {
                        for d in 1..(orig_parts.len() - 1) {
                            let orig_dir = &orig_parts[d];
                            let mapped_dir = &mapped_parts[d];
                            if orig_dir != mapped_dir {
                                let mut parent_path = current_root.clone();
                                for mapped_part in mapped_parts.iter().take(d).skip(1) {
                                    parent_path.push('/');
                                    parent_path.push_str(mapped_part);
                                }
                                let old_dir_path = format!("{}/{}", parent_path, orig_dir);
                                if !renamed_dirs.contains(&old_dir_path) {
                                    renamed_dirs.insert(old_dir_path.clone());
                                    ops.push(RenameOperation {
                                        is_folder: true,
                                        file_index: None,
                                        old_path: old_dir_path,
                                        new_name: mapped_dir.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 3. Detect individual file renames
            for (idx, orig_parts) in norm_orig.iter().enumerate() {
                if let Some(mapped_parts) = norm_mapped.get(idx) {
                    let orig_file_name = orig_parts.last().cloned().unwrap_or_default();
                    let mapped_file_name = mapped_parts.last().cloned().unwrap_or_default();
                    if !orig_file_name.is_empty() && orig_file_name != mapped_file_name {
                        ops.push(RenameOperation {
                            is_folder: false,
                            file_index: Some(idx as u32),
                            old_path: orig_parts.join("/"),
                            new_name: mapped_file_name,
                        });
                    }
                }
            }
        } else {
            // Single-file torrent
            let orig_file = original_paths[0].replace('\\', "/");
            let orig_file_name = orig_file.split('/').next_back().unwrap_or(&orig_file);

            if let Some(mapped) = mapped_files.first() {
                let mapped_norm = mapped.replace('\\', "/");
                let mapped_file_name = mapped_norm.split('/').next_back().unwrap_or(&mapped_norm);
                if orig_file_name != mapped_file_name {
                    ops.push(RenameOperation {
                        is_folder: false,
                        file_index: Some(0),
                        old_path: orig_file.to_string(),
                        new_name: mapped_file_name.to_string(),
                    });
                }
            }
        }

        ops
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

    #[test]
    fn test_compute_rename_operations_top_level_folder() {
        let orig_name = "MyMovie";
        let orig_paths = vec!["MyMovie/disc.iso".to_string()];
        let mapped = vec!["MyMovie_Remux/disc.iso".to_string()];
        let ops = QbitMapper::compute_rename_operations(
            orig_name,
            &orig_paths,
            &mapped,
            &ContentLayout::Original,
        );
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            RenameOperation {
                is_folder: true,
                file_index: None,
                old_path: "MyMovie".to_string(),
                new_name: "MyMovie_Remux".to_string(),
            }
        );
    }

    #[test]
    fn test_compute_rename_operations_nested_folder_and_file() {
        let orig_name = "Series";
        let orig_paths = vec![
            "Series/season1/ep1.mkv".to_string(),
            "Series/season1/ep2.mkv".to_string(),
        ];
        let mapped = vec![
            "Series_HD/s01/episode01.mkv".to_string(),
            "Series_HD/s01/episode02.mkv".to_string(),
        ];
        let ops = QbitMapper::compute_rename_operations(
            orig_name,
            &orig_paths,
            &mapped,
            &ContentLayout::Original,
        );
        // 1 root rename + 1 nested folder rename + 2 file renames = 4 ops
        assert_eq!(ops.len(), 4);
        assert_eq!(
            ops[0],
            RenameOperation {
                is_folder: true,
                file_index: None,
                old_path: "Series".to_string(),
                new_name: "Series_HD".to_string(),
            }
        );
        assert_eq!(
            ops[1],
            RenameOperation {
                is_folder: true,
                file_index: None,
                old_path: "Series_HD/season1".to_string(),
                new_name: "s01".to_string(),
            }
        );
        assert_eq!(
            ops[2],
            RenameOperation {
                is_folder: false,
                file_index: Some(0),
                old_path: "Series/season1/ep1.mkv".to_string(),
                new_name: "episode01.mkv".to_string(),
            }
        );
        assert_eq!(
            ops[3],
            RenameOperation {
                is_folder: false,
                file_index: Some(1),
                old_path: "Series/season1/ep2.mkv".to_string(),
                new_name: "episode02.mkv".to_string(),
            }
        );
    }

    #[test]
    fn test_compute_rename_operations_single_file() {
        let orig_name = "single.iso";
        let orig_paths = vec!["single.iso".to_string()];
        let mapped = vec!["single_renamed.iso".to_string()];
        let ops = QbitMapper::compute_rename_operations(
            orig_name,
            &orig_paths,
            &mapped,
            &ContentLayout::Original,
        );
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            RenameOperation {
                is_folder: false,
                file_index: Some(0),
                old_path: "single.iso".to_string(),
                new_name: "single_renamed.iso".to_string(),
            }
        );
    }
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscType {
    DvdVideo,
    DvdAudio,
    BluRay,
    UhdBluRay,
    IsoImage,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscLayerSpec {
    Dvd5,     // Single Layer DVD (<= 4.7 GB)
    Dvd9,     // Dual Layer DVD (<= 8.5 GB)
    Bd25,     // Single Layer Blu-ray (<= 25 GB)
    Bd50,     // Dual Layer Blu-ray (<= 50 GB)
    UhdBd66,  // Dual Layer 4K UHD Blu-ray (<= 66 GB)
    UhdBd100, // Triple Layer 4K UHD Blu-ray (<= 100 GB)
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscStructureInfo {
    pub disc_type: DiscType,
    pub layer_spec: DiscLayerSpec,
    pub has_menu: bool,
    pub root_path: String,
    pub play_target: String,
    pub cover_image: Option<String>,
    pub total_bytes: u64,
    pub title: String,
    pub disc_number: Option<u32>,
    pub total_discs: Option<u32>,
}

pub struct DiscDetector;

impl DiscDetector {
    /// Inspect a torrent save directory or file to identify optical disc structures.
    pub fn inspect(path: &Path) -> Option<DiscStructureInfo> {
        if !path.exists() {
            return None;
        }

        let total_bytes = Self::calculate_total_bytes(path);
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown Disc".to_string());

        let (disc_number, total_discs) = Self::parse_disc_numbers(&title);
        let cover_image = Self::find_cover_image(path);

        // 1. Single file: Check for ISO / IMG disc image
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "iso" || ext == "img" {
                let lower_title = title.to_lowercase();
                let is_uhd = lower_title.contains("uhd")
                    || lower_title.contains("2160p")
                    || lower_title.contains("4k")
                    || total_bytes > 55 * 1024 * 1024 * 1024;

                let (disc_type, layer_spec) = Self::classify_disc_bytes(total_bytes, is_uhd);

                return Some(DiscStructureInfo {
                    disc_type,
                    layer_spec,
                    has_menu: true,
                    root_path: path.to_string_lossy().to_string(),
                    play_target: path.to_string_lossy().to_string(),
                    cover_image,
                    total_bytes,
                    title,
                    disc_number,
                    total_discs,
                });
            }
        }

        // 2. Directory inspection
        if path.is_dir() {
            // Check for Blu-ray BDMV structure
            if let Some(bdmv_path) = Self::find_subpath(path, "BDMV") {
                let lower_title = title.to_lowercase();
                let is_uhd = lower_title.contains("uhd")
                    || lower_title.contains("2160p")
                    || lower_title.contains("4k")
                    || total_bytes > 55 * 1024 * 1024 * 1024;

                let (disc_type, layer_spec) = if is_uhd {
                    let spec = if total_bytes <= 66 * 1024 * 1024 * 1024 {
                        DiscLayerSpec::UhdBd66
                    } else {
                        DiscLayerSpec::UhdBd100
                    };
                    (DiscType::UhdBluRay, spec)
                } else {
                    let spec = if total_bytes <= 25 * 1024 * 1024 * 1024 {
                        DiscLayerSpec::Bd25
                    } else {
                        DiscLayerSpec::Bd50
                    };
                    (DiscType::BluRay, spec)
                };

                return Some(DiscStructureInfo {
                    disc_type,
                    layer_spec,
                    has_menu: true,
                    root_path: path.to_string_lossy().to_string(),
                    play_target: bdmv_path.to_string_lossy().to_string(),
                    cover_image,
                    total_bytes,
                    title,
                    disc_number,
                    total_discs,
                });
            }

            // Check for DVD VIDEO_TS structure
            if let Some(video_ts_path) = Self::find_subpath(path, "VIDEO_TS") {
                let layer_spec = if total_bytes <= 4_700_000_000 {
                    DiscLayerSpec::Dvd5
                } else {
                    DiscLayerSpec::Dvd9
                };

                return Some(DiscStructureInfo {
                    disc_type: DiscType::DvdVideo,
                    layer_spec,
                    has_menu: true,
                    root_path: path.to_string_lossy().to_string(),
                    play_target: video_ts_path.to_string_lossy().to_string(),
                    cover_image,
                    total_bytes,
                    title,
                    disc_number,
                    total_discs,
                });
            }

            // Check for DVD AUDIO_TS structure
            if let Some(audio_ts_path) = Self::find_subpath(path, "AUDIO_TS") {
                return Some(DiscStructureInfo {
                    disc_type: DiscType::DvdAudio,
                    layer_spec: DiscLayerSpec::Dvd9,
                    has_menu: true,
                    root_path: path.to_string_lossy().to_string(),
                    play_target: audio_ts_path.to_string_lossy().to_string(),
                    cover_image,
                    total_bytes,
                    title,
                    disc_number,
                    total_discs,
                });
            }

            // Check for ISO image inside directory. read_dir order is
            // arbitrary, so a download root holding several images (e.g. an
            // OS installer next to a multi-disc movie) must be resolved
            // DETERMINISTICALLY: search the folder and one level of
            // subfolders, preferring "disc 1" style names, then other
            // disc-N names, then anything else, with the name as the
            // final tie-break.
            fn collect_iso_images(dir: &Path, out: &mut Vec<PathBuf>) {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file()
                            && matches!(
                                p.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|s| s.to_ascii_lowercase()),
                                Some(ext) if ext == "iso" || ext == "img"
                            )
                        {
                            out.push(p);
                        }
                    }
                }
            }
            let mut candidates: Vec<PathBuf> = Vec::new();
            collect_iso_images(path, &mut candidates);
            if let Ok(subdirs) = fs::read_dir(path) {
                for entry in subdirs.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        collect_iso_images(&p, &mut candidates);
                    }
                }
            }
            let image_rank = |p: &Path| {
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
            candidates.sort_by(|a, b| image_rank(a).cmp(&image_rank(b)).then(a.cmp(b)));

            if let Some(p) = candidates.first() {
                let iso_bytes = p.metadata().map(|m| m.len()).unwrap_or(0);
                let lower = p.to_string_lossy().to_lowercase();
                let is_uhd = lower.contains("uhd")
                    || lower.contains("2160p")
                    || iso_bytes > 55 * 1024 * 1024 * 1024;
                let (disc_type, layer_spec) = Self::classify_disc_bytes(iso_bytes, is_uhd);

                return Some(DiscStructureInfo {
                    disc_type,
                    layer_spec,
                    has_menu: true,
                    root_path: path.to_string_lossy().to_string(),
                    play_target: p.to_string_lossy().to_string(),
                    cover_image,
                    total_bytes: iso_bytes,
                    title,
                    disc_number,
                    total_discs,
                });
            }
        }

        None
    }

    /// Size-class ladder shared by every ISO classification path (the file
    /// branch and the directory-branch image pick must agree, or a single-ISO
    /// folder and its multi-ISO sibling badge differently). Nominal
    /// capacities: DVD-5 ≈ 4.7 GB, DVD-9 ≈ 8.5 GB, BD-25 ≈ 25 GB, BD-50 ≈ 50 GB,
    /// UHD-66/UHD-100 beyond. A 2.2 GB ISO is a DVD-5 — labelling it DVD-9 is
    /// exactly the misrepresentation the owner reported.
    fn classify_disc_bytes(bytes: u64, is_uhd: bool) -> (DiscType, DiscLayerSpec) {
        if is_uhd {
            if bytes <= 70 * 1024 * 1024 * 1024 {
                (DiscType::UhdBluRay, DiscLayerSpec::UhdBd66)
            } else {
                (DiscType::UhdBluRay, DiscLayerSpec::UhdBd100)
            }
        } else if bytes <= 4_700_000_000 {
            (DiscType::DvdVideo, DiscLayerSpec::Dvd5)
        } else if bytes <= 9_400_000_000 {
            (DiscType::DvdVideo, DiscLayerSpec::Dvd9)
        } else if bytes <= 26_000_000_000 {
            (DiscType::BluRay, DiscLayerSpec::Bd25)
        } else if bytes <= 51_000_000_000 {
            (DiscType::BluRay, DiscLayerSpec::Bd50)
        } else {
            (DiscType::UhdBluRay, DiscLayerSpec::UhdBd100)
        }
    }

    /// Detect EVERY disc inside a torrent path: each ISO/IMG image plus each
    /// subfolder carrying its own VIDEO_TS/BDMV structure. A multi-disc
    /// release in one torrent (DISC_1.ISO + DISC_2.ISO) therefore yields a
    /// per-disc spec (owner: the single "DVD-9" badge from the first image
    /// misrepresents the set). Each disc is classified by ITS OWN size.
    pub fn inspect_all(path: &Path) -> Vec<DiscStructureInfo> {
        if !path.exists() {
            return Vec::new();
        }
        if path.is_file() {
            return Self::inspect(path).into_iter().collect();
        }
        let mut discs: Vec<DiscStructureInfo> = Vec::new();
        if let Ok(entries) = fs::read_dir(path) {
            let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            children.sort();
            // If path contains many subdirectories (> 12) or its name is a known library root
            // (e.g. "Blu-ray", "DVD", "Movies"), do not scan it as a single disc set to prevent
            // crawling an entire 300TB library and freezing the system.
            let subdirs_count = children.iter().filter(|c| c.is_dir()).count();
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            let is_library_root = matches!(
                dir_name.as_str(),
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
            );
            if subdirs_count > 12 || is_library_root {
                tracing::warn!(
                    "[disc-detector] Skipping library-level crawl on '{}' (subdirs: {subdirs_count})",
                    path.display()
                );
                return Vec::new();
            }
            for child in children {
                if child.is_file() {
                    let ext = child
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_ascii_lowercase());
                    if ext.as_deref() == Some("iso") || ext.as_deref() == Some("img") {
                        if let Some(info) = Self::inspect(&child) {
                            discs.push(info);
                        }
                    }
                } else if child.is_dir()
                    && (Self::find_subpath(&child, "VIDEO_TS").is_some()
                        || Self::find_subpath(&child, "BDMV").is_some())
                {
                    if let Some(info) = Self::inspect(&child) {
                        discs.push(info);
                    }
                }
            }
        }
        if discs.is_empty() {
            if let Some(info) = Self::inspect(path) {
                discs.push(info);
            }
        }
        // Number the set 1..n when the detector could not infer numbers.
        let total = discs.len() as u32;
        for (idx, d) in discs.iter_mut().enumerate() {
            d.disc_number = d.disc_number.or(Some(idx as u32 + 1));
            d.total_discs = d.total_discs.or(Some(total));
        }
        discs
    }

    /// Calculate total size in bytes of file or directory with network-share optimizations.
    fn calculate_total_bytes(path: &Path) -> u64 {
        if path.is_file() {
            return path.metadata().map(|m| m.len()).unwrap_or(0);
        }
        // Direct stream directory optimizations for fast calculation on network shares
        for stream_cand in &["BDMV/STREAM", "bdmv/stream", "BDMV/stream", "bdmv/STREAM"] {
            let stream_dir = path.join(stream_cand);
            if stream_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&stream_dir) {
                    let total: u64 = entries
                        .flatten()
                        .filter_map(|e| e.metadata().ok())
                        .map(|m| m.len())
                        .sum();
                    if total > 0 {
                        return total;
                    }
                }
            }
        }
        for vts_cand in &["VIDEO_TS", "video_ts"] {
            let vts_dir = path.join(vts_cand);
            if vts_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&vts_dir) {
                    let total: u64 = entries
                        .flatten()
                        .filter_map(|e| e.metadata().ok())
                        .map(|m| m.len())
                        .sum();
                    if total > 0 {
                        return total;
                    }
                }
            }
        }
        Self::calculate_dir_bytes_bounded(path, 0, 3)
    }

    fn calculate_dir_bytes_bounded(path: &Path, depth: usize, max_depth: usize) -> u64 {
        if depth > max_depth {
            return 0;
        }
        let mut total = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    total += p.metadata().map(|m| m.len()).unwrap_or(0);
                } else if p.is_dir() {
                    total += Self::calculate_dir_bytes_bounded(&p, depth + 1, max_depth);
                }
            }
        }
        total
    }

    /// Case-insensitive search for a child directory or file (e.g. "VIDEO_TS" or "BDMV").
    fn find_subpath(parent: &Path, target_name: &str) -> Option<PathBuf> {
        let direct = parent.join(target_name);
        if direct.exists() {
            return Some(direct);
        }
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.eq_ignore_ascii_case(target_name) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// Search for local cover art, disc scans, or poster images in directory.
    pub fn find_cover_image(path: &Path) -> Option<String> {
        let dir = if path.is_file() { path.parent()? } else { path };

        let common_names = [
            "folder.jpg",
            "folder.png",
            "cover.jpg",
            "cover.png",
            "front.jpg",
            "front.png",
            "poster.jpg",
            "poster.png",
            "disc.jpg",
            "disc1.jpg",
        ];

        // 1. Direct check in directory
        for name in &common_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }

        // 2. Case-insensitive scan
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        let lower = fname.to_lowercase();
                        for cn in &common_names {
                            if lower == *cn {
                                return Some(p.to_string_lossy().to_string());
                            }
                        }
                    }
                } else if p.is_dir() {
                    // Check sub-folder like "Scans" or "Artwork"
                    let dname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if dname.eq_ignore_ascii_case("scans") || dname.eq_ignore_ascii_case("artwork")
                    {
                        if let Ok(sub_entries) = fs::read_dir(&p) {
                            for sub in sub_entries.flatten() {
                                let sp = sub.path();
                                if sp.is_file() {
                                    let ext = sp
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "jpg" || ext == "jpeg" || ext == "png" {
                                        return Some(sp.to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract disc number and total discs from title (e.g. "Boxset Disc 2", "Vol. 1", "BD3", "DVD 4").
    pub fn parse_disc_numbers(title: &str) -> (Option<u32>, Option<u32>) {
        let lower = title.to_lowercase();

        // Pattern: "disc X" or "cd X" or "bd X" or "dvd X"
        let markers = ["disc", "disk", "bd", "dvd", "vol", "volume", "part"];
        for marker in &markers {
            if let Some(idx) = lower.find(marker) {
                let remainder = &lower[idx + marker.len()..].trim_start();
                // Strip separators like '.', '-', '_'
                let clean = remainder.trim_start_matches(['.', '-', '_']);
                let digits: String = clean.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(num) = digits.parse::<u32>() {
                    // Ignore optical media layer spec markers (DVD-5, DVD-9, BD-25, BD-50, etc.)
                    if (*marker == "dvd" && (num == 5 || num == 9))
                        || (*marker == "bd" && (num == 25 || num == 50 || num == 66 || num == 100))
                    {
                        continue;
                    }
                    if num > 0 && num <= 99 {
                        return (Some(num), None);
                    }
                }
            }
        }

        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_dvd_video_ts_structure() {
        let tmp = std::env::temp_dir().join("nebulae_disc_test_dvd");
        let video_ts = tmp.join("VIDEO_TS");
        let _ = fs::create_dir_all(&video_ts);
        fs::write(video_ts.join("VIDEO_TS.IFO"), b"fake ifo header").unwrap();
        fs::write(video_ts.join("VTS_01_1.VOB"), vec![0u8; 100_000]).unwrap();

        let detected = DiscDetector::inspect(&tmp).expect("Failed to detect DVD");
        assert_eq!(detected.disc_type, DiscType::DvdVideo);
        assert_eq!(detected.layer_spec, DiscLayerSpec::Dvd5);
        assert!(detected.has_menu);
        assert!(detected.play_target.contains("VIDEO_TS"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_bluray_bdmv_structure() {
        let tmp = std::env::temp_dir().join("nebulae_disc_test_bdmv");
        let bdmv = tmp.join("BDMV");
        let _ = fs::create_dir_all(&bdmv);
        fs::write(bdmv.join("index.bdmv"), b"fake bdmv index").unwrap();

        let detected = DiscDetector::inspect(&tmp).expect("Failed to detect Blu-ray");
        assert_eq!(detected.disc_type, DiscType::BluRay);
        assert!(detected.has_menu);
        assert!(detected.play_target.contains("BDMV"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_4k_uhd_iso() {
        let tmp = std::env::temp_dir().join("nebulae_disc_test_uhd.iso");
        fs::write(&tmp, b"fake uhd iso content").unwrap();

        let detected = DiscDetector::inspect(&tmp).expect("Failed to detect UHD ISO");
        assert_eq!(detected.disc_type, DiscType::UhdBluRay);
        assert!(detected.has_menu);

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn test_inspect_all_multi_iso_counts_every_disc() {
        // Owner's boxset layout: ONE torrent folder, TWO ISO images. The set
        // must classify BOTH discs by their own sizes — 2.2 GB / 3.4 GB
        // images are DVD-5 each, never a lone "DVD-9" from the first image.
        let base = std::env::temp_dir().join(format!("neb-set-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        for (name, size) in [
            ("DISC_1.ISO", 2_159_046_656u64),
            ("DISC_2.ISO", 3_354_021_888u64),
        ] {
            let f = fs::File::create(base.join(name)).unwrap();
            f.set_len(size).unwrap(); // sparse: reserves size without writing
        }
        let set = DiscDetector::inspect_all(&base);
        assert_eq!(set.len(), 2, "both ISOs must be classified");
        assert!(
            set.iter().all(|d| d.layer_spec == DiscLayerSpec::Dvd5),
            "both images are DVD-5-class, got {:?}",
            set.iter().map(|d| d.layer_spec).collect::<Vec<_>>()
        );
        assert!(set[0].play_target.ends_with("DISC_1.ISO"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parse_disc_number_patterns() {
        assert_eq!(
            DiscDetector::parse_disc_numbers("Tokyo_Story_1953_Criterion_BD1"),
            (Some(1), None)
        );
        assert_eq!(
            DiscDetector::parse_disc_numbers("Seven_Samurai_1954_Disc 2_Special_Features"),
            (Some(2), None)
        );
        assert_eq!(
            DiscDetector::parse_disc_numbers("A_City_of_Sadness_DVD-9"),
            (None, None) // DVD-9 is layer spec, not disc index
        );
    }
}

use super::metainfo::{announce_http_tracker, ensure_torrent_storage, ParsedTorrent};
use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn decode_base64_data(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in input.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = TABLE
            .iter()
            .position(|&c| c == b)
            .ok_or_else(|| "Invalid base64 character".to_string())? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn normalize_source_path(source: &str) -> std::path::PathBuf {
    let mut s = source.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s = &s[1..s.len() - 1];
    }
    if let Some(stripped) = s.strip_prefix("file://") {
        let decoded = percent_decode_path(stripped);
        return std::path::PathBuf::from(decoded);
    }
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    std::path::PathBuf::from(s)
}

fn percent_decode_path(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let Ok(val) =
                    u8::from_str_radix(std::str::from_utf8(&[h1, h2]).unwrap_or(""), 16)
                {
                    bytes.push(val);
                    continue;
                }
                bytes.push(b'%');
                bytes.push(h1);
                bytes.push(h2);
                continue;
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

pub fn scan_directory_files(dir: &std::path::Path) -> Result<Vec<TorrentFileItem>, String> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
        list: &mut Vec<TorrentFileItem>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(current)
            .map_err(|e| format!("Failed to read directory '{}': {e}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Directory read error: {e}"))?;
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, list)?;
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| format!("Path prefix error: {e}"))?;
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let size = entry
                    .metadata()
                    .map_err(|e| format!("Metadata error: {e}"))?
                    .len();
                list.push(TorrentFileItem {
                    index: list.len() as u32,
                    name: rel_str,
                    size_bytes: size,
                    progress: 1.0,
                    priority: 1,
                });
            }
        }
        Ok(())
    }
    let mut list = Vec::new();
    walk(dir, dir, &mut list)?;
    list.sort_by(|a, b| a.name.cmp(&b.name));
    for (idx, item) in list.iter_mut().enumerate() {
        item.index = idx as u32;
    }
    Ok(list)
}

pub struct MockTorrentEngine {
    torrents: Arc<Mutex<Vec<TorrentItem>>>,
    kill_switch_active: Arc<AtomicBool>,
    bound_interface: Arc<Mutex<Option<String>>>,
}

impl Default for MockTorrentEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTorrentEngine {
    pub fn new() -> Self {
        let torrents = Arc::new(Mutex::new(Vec::new()));
        let kill_switch_active = Arc::new(AtomicBool::new(false));
        start_engine_transfer_loop(Arc::clone(&torrents), Arc::clone(&kill_switch_active));

        Self {
            torrents,
            kill_switch_active,
            bound_interface: Arc::new(Mutex::new(Some("utun3".to_string()))),
        }
    }

    pub fn with_sample_data() -> Self {
        let initial_items = vec![
            TorrentItem {
                id: "swarm-test-1".to_string(),
                name: "Tokyo Story (1953) [4K Restoration]".to_string(),
                total_bytes: 34_500_000_000,
                downloaded_bytes: 34_500_000_000,
                uploaded_bytes: 52_000_000_000,
                download_rate: 0,
                upload_rate: 1_250_000,
                progress: 1.0,
                status: "Seeding".to_string(),
                eta_seconds: None,
                seeds: 42,
                peers: 8,
                ratio: 1.51,
                category: "Movies".to_string(),
                is_media: true,
                poster_url: None,
                info_hash: "a4f89d31c4e723901bca782910fedcba32014567".to_string(),
                save_path: "/Downloads/Movies/Tokyo Story (1953)".to_string(),
                added_on: 1728800000,
                completed_on: Some(1728850000),
                comment: "Asian DVD Club 4K Archive Edition".to_string(),
                created_by: "Nebulae Metainfo Tool 1.0".to_string(),
                piece_size: 4_194_304,
                total_pieces: 8225,
                sequential_download: false,
                first_last_piece_prio: true,
                super_seeding: false,
                tags: vec![
                    "Japanese".to_string(),
                    "Classic".to_string(),
                    "Cinema".to_string(),
                ],
                files: vec![
                    TorrentFileItem {
                        index: 0,
                        name: "Tokyo.Story.1953.2160p.UHD.Remux.mkv".to_string(),
                        size_bytes: 34_480_000_000,
                        progress: 1.0,
                        priority: 1,
                    },
                    TorrentFileItem {
                        index: 1,
                        name: "Tokyo.Story.1953.en.srt".to_string(),
                        size_bytes: 20_000_000,
                        progress: 1.0,
                        priority: 1,
                    },
                ],
                trackers: vec![
                    TorrentTrackerItem {
                        tier: 0,
                        url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                        status: "Working".to_string(),
                        seeds: 42,
                        peers: 8,
                        downloaded: 139,
                        message: "Announce OK".to_string(),
                    },
                    TorrentTrackerItem {
                        tier: 1,
                        url: "udp://tracker.opentrackr.org:1337/announce".to_string(),
                        status: "Working".to_string(),
                        seeds: 35,
                        peers: 6,
                        downloaded: 240,
                        message: "Announce OK".to_string(),
                    },
                ],
                download_limit_kib: 0,
                upload_limit_kib: 0,
            },
            TorrentItem {
                id: "swarm-test-2".to_string(),
                name: "Seven Samurai (1954) [Criterion Remaster]".to_string(),
                total_bytes: 48_200_000_000,
                downloaded_bytes: 26_510_000_000,
                uploaded_bytes: 4_300_000_000,
                download_rate: 14_800_000,
                upload_rate: 850_000,
                progress: 0.55,
                status: "Downloading".to_string(),
                eta_seconds: Some(1460),
                seeds: 88,
                peers: 24,
                ratio: 0.16,
                category: "Movies".to_string(),
                is_media: true,
                poster_url: None,
                info_hash: "7bc549e830114f29a00b84d9f1e4093821045bda".to_string(),
                save_path: "/Downloads/Movies/Seven Samurai (1954)".to_string(),
                added_on: 1728860000,
                completed_on: None,
                comment: "Criterion Remaster Dual-Audio".to_string(),
                created_by: "Nebulae Metainfo Tool 1.0".to_string(),
                piece_size: 4_194_304,
                total_pieces: 11492,
                sequential_download: true,
                first_last_piece_prio: true,
                super_seeding: false,
                tags: vec!["Criterion".to_string(), "Kurosawa".to_string()],
                files: vec![
                    TorrentFileItem {
                        index: 0,
                        name: "Seven.Samurai.1954.1080p.BluRay.mkv".to_string(),
                        size_bytes: 48_180_000_000,
                        progress: 0.55,
                        priority: 1,
                    },
                    TorrentFileItem {
                        index: 1,
                        name: "Seven.Samurai.1954.Commentary.srt".to_string(),
                        size_bytes: 20_000_000,
                        progress: 1.0,
                        priority: 1,
                    },
                ],
                trackers: vec![TorrentTrackerItem {
                    tier: 0,
                    url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                    status: "Working".to_string(),
                    seeds: 88,
                    peers: 24,
                    downloaded: 412,
                    message: "Announce OK".to_string(),
                }],
                download_limit_kib: 0,
                upload_limit_kib: 0,
            },
        ];

        let torrents = Arc::new(Mutex::new(initial_items));
        let kill_switch_active = Arc::new(AtomicBool::new(false));
        start_engine_transfer_loop(Arc::clone(&torrents), Arc::clone(&kill_switch_active));

        Self {
            torrents,
            kill_switch_active,
            bound_interface: Arc::new(Mutex::new(Some("utun3".to_string()))),
        }
    }

    pub fn set_bound_interface(&self, interface: Option<String>) {
        let mut guard = self.bound_interface.lock().unwrap();
        *guard = interface;
    }

    pub fn add_parsed_torrent(
        &self,
        parsed: ParsedTorrent,
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let mut list = self.torrents.lock().unwrap();
        let new_id = format!("swarm-test-{}", list.len() + 1);
        let save_base = download_dir
            .map(|d| {
                if let Some(stripped) = d.strip_prefix("~/") {
                    if let Some(home) = dirs::home_dir() {
                        home.join(stripped)
                    } else {
                        std::path::PathBuf::from(d)
                    }
                } else if d == "~" {
                    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(d))
                } else {
                    std::path::PathBuf::from(d)
                }
            })
            .unwrap_or_else(|| {
                dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
            });

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let content_dir = ensure_torrent_storage(&save_base, &parsed)
            .unwrap_or_else(|_| save_base.join(&parsed.name));
        let save_path = content_dir.to_string_lossy().to_string();

        let is_media = parsed.name.ends_with(".mkv")
            || parsed.name.ends_with(".mp4")
            || parsed.name.ends_with(".iso")
            || parsed.files.iter().any(|f| {
                let lower = f.name.to_lowercase();
                lower.ends_with(".iso")
                    || lower.ends_with(".mkv")
                    || lower.ends_with(".mp4")
                    || lower.ends_with(".vob")
            });

        let content_path = std::path::Path::new(&save_path);
        let already_complete = if content_path.is_file() {
            std::fs::metadata(content_path)
                .map(|m| m.len() >= parsed.total_bytes && parsed.total_bytes > 0)
                .unwrap_or(false)
        } else if content_path.is_dir() {
            let existing_size: u64 = parsed
                .files
                .iter()
                .filter_map(|f| {
                    let p = content_path.join(&f.name);
                    std::fs::metadata(p).ok().map(|m| m.len())
                })
                .sum();
            existing_size >= parsed.total_bytes && parsed.total_bytes > 0
        } else {
            false
        };

        let initial_status = if already_complete {
            "Seeding".to_string()
        } else {
            "Downloading".to_string()
        };
        let initial_progress = if already_complete { 1.0f32 } else { 0.0f32 };
        let initial_downloaded = if already_complete {
            parsed.total_bytes
        } else {
            0
        };
        let completed_time = if already_complete { Some(now) } else { None };

        let mut final_files = parsed.files.clone();
        if already_complete {
            for f in &mut final_files {
                f.progress = 1.0;
            }
        }

        // Background tracker scrape for actual swarm counts
        let torrents_ref = Arc::clone(&self.torrents);
        let target_id = new_id.clone();
        let raw_hash = parsed.raw_info_hash;
        let left = parsed.total_bytes;
        if let Some(first_tracker) = parsed.trackers.first() {
            let tr_url = first_tracker.url.clone();
            tauri::async_runtime::spawn(async move {
                let peer_id = b"-NE0100-849204820194";
                if let Ok((seeds, peers)) =
                    announce_http_tracker(&tr_url, &raw_hash, peer_id, 51413, left).await
                {
                    if let Ok(mut items) = torrents_ref.lock() {
                        if let Some(item) = items.iter_mut().find(|t| t.id == target_id) {
                            item.seeds = seeds;
                            item.peers = peers;
                            if let Some(tr) = item.trackers.first_mut() {
                                tr.seeds = seeds;
                                tr.peers = peers;
                                tr.status = "Working".to_string();
                                tr.message = "Announce OK".to_string();
                            }
                        }
                    }
                }
            });
        }

        list.push(TorrentItem {
            id: new_id.clone(),
            name: parsed.name,
            total_bytes: parsed.total_bytes,
            downloaded_bytes: initial_downloaded,
            uploaded_bytes: 0,
            download_rate: 0,
            upload_rate: 0,
            progress: initial_progress,
            status: initial_status,
            eta_seconds: None,
            seeds: 2,
            peers: 0,
            ratio: 0.0,
            category: "General".to_string(),
            is_media,
            poster_url: None,
            info_hash: parsed.info_hash,
            save_path,
            added_on: now,
            completed_on: completed_time,
            comment: parsed.comment,
            created_by: parsed.created_by,
            piece_size: parsed.piece_size,
            total_pieces: parsed.total_pieces,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            tags: vec!["Active".to_string()],
            files: final_files,
            trackers: parsed.trackers,
            download_limit_kib: 0,
            upload_limit_kib: 0,
        });

        Ok(new_id)
    }

    pub fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        let parsed = ParsedTorrent::parse_bytes(bytes)?;
        self.add_parsed_torrent(parsed, download_dir)
    }
}

impl TorrentEngine for MockTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let mut list = self.torrents.lock().unwrap();
        for item in list.iter_mut() {
            let p = std::path::Path::new(&item.save_path);
            if p.exists() && p.is_dir() && item.files.len() <= 1 {
                if let Ok(scanned) = scan_directory_files(p) {
                    if scanned.len() > item.files.len() {
                        let total: u64 = scanned.iter().map(|f| f.size_bytes).sum();
                        item.files = scanned;
                        if item.total_bytes == 1_250_000_000 || item.total_bytes == 0 {
                            item.total_bytes = total;
                            if item.progress >= 1.0 {
                                item.downloaded_bytes = total;
                            }
                        }
                    }
                }
            }
        }
        Ok(list.clone())
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let trimmed = source.trim();

        // 1. Check if source is base64 encoded
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = decode_base64_data(b64) {
                if let Ok(parsed) = ParsedTorrent::parse_bytes(&decoded) {
                    return self.add_parsed_torrent(parsed, download_dir);
                }
            }
        }

        // 2. Check if source is a path on disk
        let norm_path = normalize_source_path(trimmed);
        if norm_path.exists() {
            if norm_path.is_file() {
                if let Ok(parsed) = ParsedTorrent::parse_file(&norm_path) {
                    return self.add_parsed_torrent(parsed, download_dir);
                }
            } else if norm_path.is_dir() {
                if let Ok(scanned_files) = scan_directory_files(&norm_path) {
                    let total: u64 = scanned_files.iter().map(|f| f.size_bytes).sum();
                    let dir_name = norm_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Local Directory".to_string());
                    let parsed = ParsedTorrent {
                        name: dir_name,
                        total_bytes: total,
                        piece_size: 1_048_576,
                        total_pieces: total.div_ceil(1_048_576) as u32,
                        info_hash: format!("local_{:x}", total),
                        raw_info_hash: [0u8; 20],
                        is_private: false,
                        is_multi_file: true,
                        files: scanned_files,
                        trackers: vec![TorrentTrackerItem {
                            tier: 0,
                            url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                            status: "Working".to_string(),
                            seeds: 25,
                            peers: 5,
                            downloaded: 0,
                            message: "Announce OK".to_string(),
                        }],
                        comment: "Local directory content".to_string(),
                        created_by: "Nebulae Metainfo Tool".to_string(),
                    };
                    return self.add_parsed_torrent(parsed, download_dir);
                }
            }
        }

        // 3. Fallback to magnet or placeholder
        let save_base = download_dir
            .map(|d| {
                if let Some(stripped) = d.strip_prefix("~/") {
                    if let Some(home) = dirs::home_dir() {
                        home.join(stripped)
                    } else {
                        std::path::PathBuf::from(d)
                    }
                } else if d == "~" {
                    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(d))
                } else {
                    std::path::PathBuf::from(d)
                }
            })
            .unwrap_or_else(|| {
                dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
            });

        let mut list = self.torrents.lock().unwrap();
        let new_id = format!("swarm-test-{}", list.len() + 1);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let name = if trimmed.starts_with("magnet:") {
            let parsed_name = trimmed
                .split("&dn=")
                .nth(1)
                .and_then(|dn| dn.split('&').next())
                .unwrap_or("Magnet Download");
            parsed_name.replace('+', " ").replace("%20", " ")
        } else {
            norm_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Queued Torrent".to_string())
        };

        let save_str = save_base.to_string_lossy().to_string();
        let files = vec![TorrentFileItem {
            index: 0,
            name: name.clone(),
            size_bytes: 1_250_000_000,
            progress: 0.0,
            priority: 1,
        }];

        list.push(TorrentItem {
            id: new_id.clone(),
            name,
            total_bytes: 1_250_000_000,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            download_rate: 0,
            upload_rate: 0,
            progress: 0.0,
            status: "Downloading".to_string(),
            eta_seconds: None,
            seeds: 12,
            peers: 4,
            ratio: 0.0,
            category: "General".to_string(),
            is_media: false,
            poster_url: None,
            info_hash: format!("hash_{:x}", now),
            save_path: save_str,
            added_on: now,
            completed_on: None,
            comment: "Added via Nebulae".to_string(),
            created_by: "Nebulae Client".to_string(),
            piece_size: 1_048_576,
            total_pieces: 1192,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            tags: vec!["Active".to_string()],
            files,
            trackers: vec![TorrentTrackerItem {
                tier: 0,
                url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                status: "Working".to_string(),
                seeds: 12,
                peers: 4,
                downloaded: 0,
                message: "Announce OK".to_string(),
            }],
            download_limit_kib: 0,
            upload_limit_kib: 0,
        });
        Ok(new_id)
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        let parsed = ParsedTorrent::parse_bytes(bytes)?;
        self.add_parsed_torrent(parsed, download_dir)
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.status = "Paused".to_string();
            item.download_rate = 0;
            item.upload_rate = 0;
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        let pos = list.iter().position(|t| t.id == id);
        let Some(idx) = pos else {
            return Ok(());
        };

        if delete_files {
            let item = &list[idx];
            let save_lower = item.save_path.to_lowercase();
            if save_lower.contains("qbittorrent") || save_lower.contains("bt_backup") {
                return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be deleted by Nebulae.".to_string());
            }

            let content_path = std::path::Path::new(&item.save_path);
            if content_path.exists() {
                // Determine system paths that must never be removed as directories
                let protected_roots: Vec<std::path::PathBuf> = vec![
                    dirs::download_dir(),
                    dirs::home_dir(),
                    dirs::desktop_dir(),
                    dirs::document_dir(),
                    Some(std::path::PathBuf::from("/")),
                    Some(std::path::PathBuf::from("C:\\")),
                ]
                .into_iter()
                .flatten()
                .collect();

                let is_protected_root = protected_roots.iter().any(|r| r == content_path);

                if content_path.is_file() {
                    let _ = std::fs::remove_file(content_path);
                } else if content_path.is_dir() {
                    // Delete all individual files tracked in this torrent
                    for f in &item.files {
                        let file_p = content_path.join(&f.name);
                        if file_p.exists() && file_p.is_file() {
                            let _ = std::fs::remove_file(&file_p);
                        }
                        // Clean up empty intermediate directories
                        let mut parent = file_p.parent();
                        while let Some(par) = parent {
                            if par == content_path {
                                break;
                            }
                            if let Ok(mut entries) = std::fs::read_dir(par) {
                                if entries.next().is_none() {
                                    let _ = std::fs::remove_dir(par);
                                }
                            }
                            parent = par.parent();
                        }
                    }

                    // If content_path is NOT a protected root directory, check if it's the torrent's dedicated folder
                    if !is_protected_root {
                        let is_dedicated_folder = content_path
                            .file_name()
                            .map(|name| name.to_string_lossy() == item.name)
                            .unwrap_or(false);

                        if is_dedicated_folder {
                            let _ = std::fs::remove_dir_all(content_path);
                        } else if let Ok(mut entries) = std::fs::read_dir(content_path) {
                            if entries.next().is_none() {
                                let _ = std::fs::remove_dir(content_path);
                            }
                        }
                    }
                }
            }
        }

        list.remove(idx);
        Ok(())
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err(
                "Cannot resume torrent: VPN Kill Switch is active (fail-closed)".to_string(),
            );
        }

        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.status = if item.progress >= 1.0 {
                "Seeding".to_string()
            } else {
                "Downloading".to_string()
            };
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn pause_all(&self) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        for item in list.iter_mut() {
            item.status = "Paused".to_string();
            item.download_rate = 0;
            item.upload_rate = 0;
        }
        Ok(())
    }

    fn resume_all(&self) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err(
                "Cannot resume transfers: VPN Kill Switch is active (fail-closed)".to_string(),
            );
        }

        let mut list = self.torrents.lock().unwrap();
        for item in list.iter_mut() {
            item.status = if item.progress >= 1.0 {
                "Seeding".to_string()
            } else {
                "Downloading".to_string()
            };
        }
        Ok(())
    }

    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(true, Ordering::SeqCst);
        self.pause_all()
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch_active.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let list = self.torrents.lock().unwrap();
        let bound = self.bound_interface.lock().unwrap().clone();
        let active = list.iter().filter(|t| t.status != "Paused").count();
        let paused = list.iter().filter(|t| t.status == "Paused").count();

        EngineStatus {
            active_torrents: active,
            paused_torrents: paused,
            kill_switch_engaged: self.is_kill_switch_active(),
            bound_interface: bound,
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        match hook {
            EngineHook::PrivateTorrentRestrictions => true,
            EngineHook::PauseResumeKillSwitch => true,
            EngineHook::TrackerDhtPexHandling => true,
            EngineHook::PeerTelemetryCollection => true,
            EngineHook::SocketInterfaceBinding => true,
            EngineHook::PieceRequestScheduling => false,
            EngineHook::ConnectionAdmission => false,
            EngineHook::ConnectionEviction => false,
            EngineHook::PerPeerPrioritization => false,
        }
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.status = "Checking".to_string();
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn reannounce(&self, id: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            for tracker in item.trackers.iter_mut() {
                tracker.status = "Updating".to_string();
                tracker.message = "Reannounced manually".to_string();
            }
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.save_path = new_path.to_string();
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.category = category.to_string();
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        _ratio_limit: f32,
    ) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.download_limit_kib = dl_kib;
            item.upload_limit_kib = ul_kib;
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.sequential_download = !item.sequential_download;
            Ok(item.sequential_download)
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.first_last_piece_prio = !item.first_last_piece_prio;
            Ok(item.first_last_piece_prio)
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            item.super_seeding = !item.super_seeding;
            Ok(item.super_seeding)
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            if let Some(file) = item.files.iter_mut().find(|f| f.index == file_index) {
                file.priority = priority;
                Ok(())
            } else {
                Err(format!(
                    "File index '{file_index}' not found in torrent '{id}'"
                ))
            }
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Torrent name cannot be empty".to_string());
        }

        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            let save_lower = item.save_path.to_lowercase();
            if save_lower.contains("qbittorrent") || save_lower.contains("bt_backup") {
                return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be amended by Nebulae.".to_string());
            }

            let old_dir = std::path::Path::new(&item.save_path);
            if old_dir.exists() {
                if let Some(parent) = old_dir.parent() {
                    let new_dir = parent.join(trimmed);
                    if old_dir != new_dir && old_dir.is_dir() {
                        let _ = std::fs::rename(old_dir, &new_dir);
                        item.save_path = new_dir.to_string_lossy().to_string();
                    }
                }
            }

            item.name = trimmed.to_string();
            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn rename_torrent_file(
        &self,
        id: &str,
        file_index: u32,
        new_file_name: &str,
    ) -> Result<(), String> {
        let trimmed = new_file_name.trim();
        if trimmed.is_empty() {
            return Err("File name cannot be empty".to_string());
        }

        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            let save_lower = item.save_path.to_lowercase();
            if save_lower.contains("qbittorrent") || save_lower.contains("bt_backup") {
                return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be amended by Nebulae.".to_string());
            }

            if let Some(file) = item.files.iter_mut().find(|f| f.index == file_index) {
                let base_dir = std::path::Path::new(&item.save_path);
                let current_rel = file.name.replace('\\', "/");
                let current_path = std::path::Path::new(&current_rel);

                // If user entered just a filename without directories, preserve existing folder path
                let new_rel = if !trimmed.contains('/') && !trimmed.contains('\\') {
                    if let Some(parent) = current_path.parent() {
                        let parent_str = parent.to_string_lossy();
                        if !parent_str.is_empty() {
                            format!("{}/{}", parent_str, trimmed)
                        } else {
                            trimmed.to_string()
                        }
                    } else {
                        trimmed.to_string()
                    }
                } else {
                    trimmed.replace('\\', "/")
                };

                let old_file_path = base_dir.join(&current_rel);
                let new_file_path = base_dir.join(&new_rel);

                if old_file_path.exists() && old_file_path != new_file_path {
                    if let Some(parent) = new_file_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::rename(&old_file_path, &new_file_path);
                }

                file.name = new_rel;
                Ok(())
            } else {
                Err(format!(
                    "File index '{file_index}' not found in torrent '{id}'"
                ))
            }
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }

    fn rename_torrent_folder(
        &self,
        id: &str,
        old_folder_path: &str,
        new_folder_name: &str,
    ) -> Result<(), String> {
        let trimmed_new_name = new_folder_name.trim();
        if trimmed_new_name.is_empty() {
            return Err("Folder name cannot be empty".to_string());
        }
        if trimmed_new_name.contains('/')
            || trimmed_new_name.contains('\\')
            || trimmed_new_name == ".."
            || trimmed_new_name == "."
        {
            return Err(
                "Folder name cannot contain path separators or relative dot tokens".to_string(),
            );
        }

        let mut list = self.torrents.lock().unwrap();
        if let Some(item) = list.iter_mut().find(|t| t.id == id) {
            let save_lower = item.save_path.to_lowercase();
            if save_lower.contains("qbittorrent") || save_lower.contains("bt_backup") {
                return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be amended by Nebulae.".to_string());
            }

            let old_norm = old_folder_path
                .trim()
                .replace('\\', "/")
                .trim_matches('/')
                .to_string();

            // Check if renaming root torrent folder
            if old_norm.is_empty() || old_norm == item.name {
                let old_dir = std::path::Path::new(&item.save_path);
                if old_dir.exists() && old_dir.is_dir() {
                    if let Some(parent) = old_dir.parent() {
                        let new_dir = parent.join(trimmed_new_name);
                        if old_dir != new_dir {
                            let _ = std::fs::rename(old_dir, &new_dir);
                            item.save_path = new_dir.to_string_lossy().to_string();
                        }
                    }
                }
                item.name = trimmed_new_name.to_string();
                return Ok(());
            }

            // Subfolder renaming
            let old_path_obj = std::path::Path::new(&old_norm);
            let parent_prefix = old_path_obj.parent().and_then(|p| {
                let s = p.to_string_lossy();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });

            let new_norm = match &parent_prefix {
                Some(p) => format!("{}/{}", p, trimmed_new_name),
                None => trimmed_new_name.to_string(),
            };

            let base_dir = std::path::Path::new(&item.save_path);
            let old_dir_full = base_dir.join(&old_norm);
            let new_dir_full = base_dir.join(&new_norm);

            if old_dir_full.exists() && old_dir_full.is_dir() && old_dir_full != new_dir_full {
                if let Some(parent) = new_dir_full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(&old_dir_full, &new_dir_full);
            }

            // Update in-memory file paths
            let old_prefix = format!("{}/", old_norm);
            let new_prefix = format!("{}/", new_norm);
            for file in &mut item.files {
                let norm_file = file.name.replace('\\', "/");
                if norm_file.starts_with(&old_prefix) {
                    let suffix = &norm_file[old_prefix.len()..];
                    file.name = format!("{}{}", new_prefix, suffix);
                } else if norm_file == old_norm {
                    file.name = new_norm.clone();
                }
            }

            Ok(())
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }
}

fn start_engine_transfer_loop(
    torrents: Arc<Mutex<Vec<TorrentItem>>>,
    kill_switch_active: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("nebulae-engine-transfer-loop".to_string())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));

            if kill_switch_active.load(Ordering::SeqCst) {
                continue;
            }

            let mut list = match torrents.lock() {
                Ok(guard) => guard,
                Err(_) => continue,
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            for item in list.iter_mut() {
                if item.status == "Downloading" {
                    if item.downloaded_bytes >= item.total_bytes && item.total_bytes > 0 {
                        item.downloaded_bytes = item.total_bytes;
                        item.progress = 1.0;
                        item.status = "Seeding".to_string();
                        item.download_rate = 0;
                        item.eta_seconds = None;
                        if item.completed_on.is_none() {
                            item.completed_on = Some(now);
                        }
                        continue;
                    }

                    // Active download throughput
                    // Base throughput around 4.5 MB/s to 9.2 MB/s if seeds exist
                    let seed_count = item.seeds.max(1) as u64;
                    let jitter = (now % 7) * 180_000;
                    let mut rate = (seed_count.min(10) * 950_000) + 2_800_000 + jitter;

                    if item.download_limit_kib > 0 {
                        let max_bytes = (item.download_limit_kib as u64) * 1024;
                        rate = rate.min(max_bytes);
                    }

                    item.download_rate = rate;

                    // Real BitTorrent upload logic: A client can only upload if there are active
                    // leechers (incomplete peers) in the swarm who request missing blocks.
                    // If peers == 0 (no one downloading) or the client hasn't downloaded enough pieces,
                    // upload rate MUST be 0 B/s.
                    let leecher_count = item.peers as u64;
                    if leecher_count > 0 && item.progress > 0.05 {
                        let mut ul_rate = (leecher_count * 120_000) + ((now % 5) * 25_000);
                        if item.upload_limit_kib > 0 {
                            let max_ul = (item.upload_limit_kib as u64) * 1024;
                            ul_rate = ul_rate.min(max_ul);
                        }
                        item.upload_rate = ul_rate;
                        item.uploaded_bytes += ul_rate;
                    } else {
                        item.upload_rate = 0;
                    }

                    item.downloaded_bytes = (item.downloaded_bytes + rate).min(item.total_bytes);

                    if item.total_bytes > 0 {
                        item.progress = ((item.downloaded_bytes as f64 / item.total_bytes as f64)
                            .clamp(0.0, 1.0)) as f32;
                        let remaining = item.total_bytes.saturating_sub(item.downloaded_bytes);
                        item.eta_seconds = remaining.checked_div(rate);
                    }

                    if item.downloaded_bytes >= item.total_bytes {
                        item.downloaded_bytes = item.total_bytes;
                        item.progress = 1.0;
                        item.status = "Seeding".to_string();
                        item.download_rate = 0;
                        item.upload_rate = 0;
                        item.eta_seconds = None;
                        if item.completed_on.is_none() {
                            item.completed_on = Some(now);
                        }
                    }

                    if item.downloaded_bytes > 0 {
                        item.ratio =
                            (item.uploaded_bytes as f64 / item.downloaded_bytes as f64) as f32;
                    }

                    // Keep file progress synced with overall progress
                    for f in item.files.iter_mut() {
                        f.progress = item.progress;
                    }
                } else if item.status == "Seeding" {
                    item.download_rate = 0;
                    item.eta_seconds = None;
                    let leecher_count = item.peers as u64;
                    if leecher_count > 0 {
                        let mut ul_rate = (leecher_count * 250_000) + ((now % 5) * 45_000);
                        if item.upload_limit_kib > 0 {
                            let max_ul = (item.upload_limit_kib as u64) * 1024;
                            ul_rate = ul_rate.min(max_ul);
                        }
                        item.upload_rate = ul_rate;
                        item.uploaded_bytes += ul_rate;
                    } else {
                        item.upload_rate = 0;
                    }
                    if item.downloaded_bytes > 0 {
                        item.ratio =
                            (item.uploaded_bytes as f64 / item.downloaded_bytes as f64) as f32;
                    }
                } else if item.status == "Paused" {
                    item.download_rate = 0;
                    item.upload_rate = 0;
                    item.eta_seconds = None;
                }
            }
        })
        .expect("Failed to spawn engine transfer loop");
}

use super::metainfo::{announce_http_tracker, ensure_torrent_storage, ParsedTorrent};
use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
}

impl TorrentEngine for MockTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let list = self.torrents.lock().unwrap();
        Ok(list.clone())
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
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

        let source_path = std::path::Path::new(source);
        let parsed_opt =
            if source_path.exists() && (source.ends_with(".torrent") || source_path.is_file()) {
                ParsedTorrent::parse_file(source_path).ok()
            } else {
                None
            };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let (
            name,
            total_bytes,
            files,
            piece_size,
            total_pieces,
            info_hash,
            comment,
            created_by,
            trackers,
            save_path,
            initial_seeds,
            initial_peers,
        ) = if let Some(parsed) = parsed_opt {
            let content_dir = ensure_torrent_storage(&save_base, &parsed)
                .unwrap_or_else(|_| save_base.join(&parsed.name));
            let save_str = content_dir.to_string_lossy().to_string();

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

            (
                parsed.name,
                parsed.total_bytes,
                parsed.files,
                parsed.piece_size,
                parsed.total_pieces,
                parsed.info_hash,
                parsed.comment,
                parsed.created_by,
                parsed.trackers,
                save_str,
                2, // Default to 2 seeds (matching actual ADC swarm) while async announce resolves
                0,
            )
        } else {
            let name = if source.starts_with("magnet:") {
                let parsed_name = source
                    .split("&dn=")
                    .nth(1)
                    .and_then(|dn| dn.split('&').next())
                    .unwrap_or("Magnet Download");
                parsed_name.replace('+', " ").replace("%20", " ")
            } else {
                source_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Queued Torrent".to_string())
            };

            let save_str = save_base.to_string_lossy().to_string();
            (
                name.clone(),
                1_250_000_000,
                vec![TorrentFileItem {
                    index: 0,
                    name: name.clone(),
                    size_bytes: 1_250_000_000,
                    progress: 0.0,
                    priority: 1,
                }],
                1_048_576,
                1192,
                format!("hash_{:x}", now),
                "Added via Nebulae".to_string(),
                "Nebulae Client".to_string(),
                vec![TorrentTrackerItem {
                    tier: 0,
                    url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                    status: "Working".to_string(),
                    seeds: 12,
                    peers: 4,
                    downloaded: 0,
                    message: "Announce OK".to_string(),
                }],
                save_str,
                12,
                4,
            )
        };

        let is_media = name.ends_with(".mkv")
            || name.ends_with(".mp4")
            || name.ends_with(".iso")
            || files.iter().any(|f| {
                let lower = f.name.to_lowercase();
                lower.ends_with(".iso")
                    || lower.ends_with(".mkv")
                    || lower.ends_with(".mp4")
                    || lower.ends_with(".vob")
            });

        let content_path = std::path::Path::new(&save_path);
        let already_complete = if content_path.is_file() {
            std::fs::metadata(content_path)
                .map(|m| m.len() >= total_bytes && total_bytes > 0)
                .unwrap_or(false)
        } else if content_path.is_dir() {
            let existing_size: u64 = files
                .iter()
                .filter_map(|f| {
                    let p = content_path.join(&f.name);
                    std::fs::metadata(p).ok().map(|m| m.len())
                })
                .sum();
            existing_size >= total_bytes && total_bytes > 0
        } else {
            false
        };

        let initial_status = if already_complete {
            "Seeding".to_string()
        } else {
            "Downloading".to_string()
        };
        let initial_progress = if already_complete { 1.0f32 } else { 0.0f32 };
        let initial_downloaded = if already_complete { total_bytes } else { 0 };
        let completed_time = if already_complete { Some(now) } else { None };

        let mut final_files = files;
        if already_complete {
            for f in &mut final_files {
                f.progress = 1.0;
            }
        }

        list.push(TorrentItem {
            id: new_id.clone(),
            name,
            total_bytes,
            downloaded_bytes: initial_downloaded,
            uploaded_bytes: 0,
            download_rate: 0,
            upload_rate: 0,
            progress: initial_progress,
            status: initial_status,
            eta_seconds: None,
            seeds: initial_seeds,
            peers: initial_peers,
            ratio: 0.0,
            category: "General".to_string(),
            is_media,
            poster_url: None,
            info_hash,
            save_path,
            added_on: now,
            completed_on: completed_time,
            comment,
            created_by,
            piece_size,
            total_pieces,
            sequential_download: false,
            first_last_piece_prio: true,
            super_seeding: false,
            tags: vec!["Active".to_string()],
            files: final_files,
            trackers,
            download_limit_kib: 0,
            upload_limit_kib: 0,
        });
        Ok(new_id)
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

    fn remove_torrent(&self, id: &str) -> Result<(), String> {
        let mut list = self.torrents.lock().unwrap();
        list.retain(|t| t.id != id);
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
                let old_file_path = base_dir.join(&file.name);
                let new_file_path = base_dir.join(trimmed);

                if old_file_path.exists() && old_file_path != new_file_path {
                    let _ = std::fs::rename(old_file_path, new_file_path);
                }

                file.name = trimmed.to_string();
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

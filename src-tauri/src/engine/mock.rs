use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct MockTorrentEngine {
    torrents: Mutex<Vec<TorrentItem>>,
    kill_switch_active: AtomicBool,
    bound_interface: Mutex<Option<String>>,
}

impl Default for MockTorrentEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTorrentEngine {
    pub fn new() -> Self {
        Self {
            torrents: Mutex::new(Vec::new()),
            kill_switch_active: AtomicBool::new(false),
            bound_interface: Mutex::new(Some("utun3".to_string())),
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
                tags: vec!["Japanese".to_string(), "Classic".to_string(), "Cinema".to_string()],
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
                trackers: vec![
                    TorrentTrackerItem {
                        tier: 0,
                        url: "https://tracker.asiandvdclub.org:2710/announce".to_string(),
                        status: "Working".to_string(),
                        seeds: 88,
                        peers: 24,
                        downloaded: 412,
                        message: "Announce OK".to_string(),
                    },
                ],
                download_limit_kib: 0,
                upload_limit_kib: 0,
            },
        ];

        Self {
            torrents: Mutex::new(initial_items),
            kill_switch_active: AtomicBool::new(false),
            bound_interface: Mutex::new(Some("utun3".to_string())),
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
        let save = download_dir
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                dirs::download_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .to_string_lossy()
                    .to_string()
            });

        let name = if source.starts_with("magnet:") {
            let parsed_name = source
                .split("&dn=")
                .nth(1)
                .and_then(|dn| dn.split('&').next())
                .unwrap_or("Magnet Download");
            parsed_name.replace('+', " ").replace("%20", " ")
        } else {
            std::path::Path::new(source)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Queued Torrent".to_string())
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        list.push(TorrentItem {
            id: new_id.clone(),
            name: name.clone(),
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
            is_media: name.ends_with(".mkv") || name.ends_with(".mp4") || name.ends_with(".iso"),
            poster_url: None,
            info_hash: format!("hash_{:x}", now),
            save_path: save,
            added_on: now,
            completed_on: None,
            comment: "Added via Nebulae".to_string(),
            created_by: "Nebulae Client".to_string(),
            piece_size: 1_048_576,
            total_pieces: 1192,
            sequential_download: false,
            first_last_piece_prio: true,
            super_seeding: false,
            tags: vec!["New".to_string()],
            files: vec![TorrentFileItem {
                index: 0,
                name: name.clone(),
                size_bytes: 1_250_000_000,
                progress: 0.0,
                priority: 1,
            }],
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
                Err(format!("File index '{file_index}' not found in torrent '{id}'"))
            }
        } else {
            Err(format!("Torrent with id '{id}' not found"))
        }
    }
}

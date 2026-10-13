use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::commands::torrent::TorrentItem;
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

    fn add_torrent(&self, source: &str, _download_dir: Option<&str>) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let mut list = self.torrents.lock().unwrap();
        let new_id = format!("swarm-test-{}", list.len() + 1);
        list.push(TorrentItem {
            id: new_id.clone(),
            name: if source.starts_with("magnet:") {
                "Magnet Download".to_string()
            } else {
                "Queued Torrent".to_string()
            },
            total_bytes: 1_000_000_000,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            download_rate: 0,
            upload_rate: 0,
            progress: 0.0,
            status: "Downloading".to_string(),
            eta_seconds: None,
            seeds: 1,
            peers: 1,
            ratio: 0.0,
            category: "General".to_string(),
            is_media: false,
            poster_url: None,
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
            // Capabilities exposed safely or natively supported by the underlying engine architecture
            EngineHook::PrivateTorrentRestrictions => true,
            EngineHook::PauseResumeKillSwitch => true,
            EngineHook::TrackerDhtPexHandling => true,
            EngineHook::PeerTelemetryCollection => true,
            EngineHook::SocketInterfaceBinding => true,

            // Observational boundary: librqbit does not expose external scheduling / admission hooks
            EngineHook::PieceRequestScheduling => false,
            EngineHook::ConnectionAdmission => false,
            EngineHook::ConnectionEviction => false,
            EngineHook::PerPeerPrioritization => false,
        }
    }
}

use crate::commands::torrent::TorrentItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineHook {
    PeerTelemetryCollection,
    ConnectionAdmission,
    ConnectionEviction,
    PieceRequestScheduling,
    PerPeerPrioritization,
    TrackerDhtPexHandling,
    PrivateTorrentRestrictions,
    SocketInterfaceBinding,
    PauseResumeKillSwitch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub active_torrents: usize,
    pub paused_torrents: usize,
    pub kill_switch_engaged: bool,
    pub bound_interface: Option<String>,
}

pub trait TorrentEngine: Send + Sync {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String>;
    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String>;
    fn remove_torrent(&self, id: &str) -> Result<(), String>;
    fn pause_torrent(&self, id: &str) -> Result<(), String>;
    fn resume_torrent(&self, id: &str) -> Result<(), String>;
    fn pause_all(&self) -> Result<(), String>;
    fn resume_all(&self) -> Result<(), String>;
    fn trigger_kill_switch(&self) -> Result<(), String>;
    fn reset_kill_switch(&self) -> Result<(), String>;
    fn is_kill_switch_active(&self) -> bool;
    fn engine_status(&self) -> EngineStatus;
    fn supports_hook(&self, hook: EngineHook) -> bool;

    // qBittorrent transfer & swarm management parity
    fn force_recheck(&self, id: &str) -> Result<(), String>;
    fn reannounce(&self, id: &str) -> Result<(), String>;
    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String>;
    fn set_category(&self, id: &str, category: &str) -> Result<(), String>;
    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String>;
    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String>;
    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String>;
    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String>;
    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String>;
    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String>;
    fn rename_torrent_file(
        &self,
        id: &str,
        file_index: u32,
        new_file_name: &str,
    ) -> Result<(), String>;
    fn rename_torrent_folder(
        &self,
        id: &str,
        old_folder_path: &str,
        new_folder_name: &str,
    ) -> Result<(), String>;
}


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
    fn pause_torrent(&self, id: &str) -> Result<(), String>;
    fn resume_torrent(&self, id: &str) -> Result<(), String>;
    fn pause_all(&self) -> Result<(), String>;
    fn resume_all(&self) -> Result<(), String>;
    fn trigger_kill_switch(&self) -> Result<(), String>;
    fn reset_kill_switch(&self) -> Result<(), String>;
    fn is_kill_switch_active(&self) -> bool;
    fn engine_status(&self) -> EngineStatus;
    fn supports_hook(&self, hook: EngineHook) -> bool;
}

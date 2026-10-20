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
    /// Add a torrent from raw .torrent bencode bytes (in-memory upload, e.g.
    /// drag-and-drop without a file path).
    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String>;
    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String>;
    fn pause_torrent(&self, id: &str) -> Result<(), String>;
    fn resume_torrent(&self, id: &str) -> Result<(), String>;
    fn pause_all(&self) -> Result<(), String>;
    fn resume_all(&self) -> Result<(), String>;
    fn trigger_kill_switch(&self) -> Result<(), String>;
    fn reset_kill_switch(&self) -> Result<(), String>;
    fn is_kill_switch_active(&self) -> bool;
    fn engine_status(&self) -> EngineStatus;
    fn supports_hook(&self, hook: EngineHook) -> bool;

    /// Graceful shutdown: stop the engine's owned subprocesses and prevent
    /// the supervisor/watchdog from respawning them. Called on app exit so
    /// a tray Quit never leaves an orphaned transmission-daemon behind
    /// (owner defect 2026-10-20). Default: nothing owned to stop.
    fn shutdown(&self) {}

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
    fn export_torrent_bytes(&self, id: &str) -> Result<Vec<u8>, String>;
    fn get_peer_telemetry(
        &self,
        id: &str,
    ) -> Result<Vec<crate::models::peering::PeerTelemetry>, String>;
    fn check_port_open(&self) -> Option<bool> {
        None
    }

    /// Push peer limit settings to the engine session.
    ///
    /// Called whenever `PeeringSettings` are saved. Engines that do not
    /// support session-level peer limits may leave this as the default no-op.
    fn apply_peering_settings(
        &self,
        _settings: &crate::models::peering::PeeringSettings,
    ) -> Result<(), String> {
        Ok(()) // default: no-op
    }

    /// Push all active session settings (speed, ratio, queue, cache, trackers,
    /// incomplete-dir, alt-speed, script, watch-dir, Racing Mode overrides) to
    /// the live engine whenever the user saves Settings. Engines that do not
    /// support live session-set may leave this as the default no-op.
    fn apply_global_settings(
        &self,
        _settings: &crate::commands::settings::UserSettings,
    ) -> Result<(), String> {
        Ok(()) // default: no-op
    }

    /// Toggle Racing Mode for a single torrent (per-torrent peer limit,
    /// bandwidth priority, and session-limit bypass). Default: unsupported.
    fn set_torrent_racing_mode(&self, _id: &str, _enabled: bool) -> Result<(), String> {
        Err("Racing Mode is not supported by this engine".to_string())
    }

    /// Returns the live or detected engine daemon version, if available.
    fn engine_version(&self) -> Option<String> {
        None
    }
}

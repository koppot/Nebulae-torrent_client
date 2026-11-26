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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveCompletionSemantics {
    /// Asynchronous daemon relocation: the engine moves data in the background and signals completion/failure via callback.
    AsyncCallback,
    /// Synchronous relocation: the engine moves data and confirms completion on return.
    Synchronous,
    /// Unconfirmed relocation: the engine changes location metadata or forwards request, but does not establish or confirm physical file relocation.
    Unconfirmed,
}

pub trait TorrentEngine: Send + Sync {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String>;
    fn get_torrent_details(&self, id: &str) -> Result<TorrentItem, String> {
        self.list_torrents()?
            .into_iter()
            .find(|t| t.id == id || t.info_hash.eq_ignore_ascii_case(id))
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))
    }
    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String>;
    /// Add a torrent from raw .torrent bencode bytes (in-memory upload, e.g.
    /// drag-and-drop without a file path).
    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String>;
    /// Add a torrent from raw .torrent bencode bytes guaranteed to be added in the paused state.
    fn add_torrent_from_bytes_paused(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        let id = self.add_torrent_from_bytes(bytes, download_dir)?;
        let _ = self.pause_torrent(&id);
        Ok(id)
    }
    /// Add a torrent from a source string (file path, magnet or URL)
    /// guaranteed to be added in the paused state (F-2 staged add: the user
    /// names and filters the content BEFORE the torrent starts). Default:
    /// add, then pause — mirrors add_torrent_from_bytes_paused. Engines
    /// whose add RPC has a "paused" field (Transmission) override this so
    /// the torrent never starts at all.
    fn add_torrent_paused(
        &self,
        source: &str,
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        let id = self.add_torrent(source, download_dir)?;
        let _ = self.pause_torrent(&id);
        Ok(id)
    }
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

    /// True when this engine drives a separate Nebulae Server instead of a local daemon.
    fn is_remote(&self) -> bool {
        false
    }

    /// Graceful shutdown: stop the engine's owned subprocesses and prevent
    /// the supervisor/watchdog from respawning them. Called on app exit so
    /// a tray Quit never leaves an orphaned transmission-daemon behind
    /// (owner defect 2026-10-20). Default: nothing owned to stop.
    fn shutdown(&self) {}

    // qBittorrent transfer & swarm management parity
    fn force_recheck(&self, id: &str) -> Result<(), String>;
    fn reannounce(&self, id: &str) -> Result<(), String>;
    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String>;
    fn relink_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        self.set_location(id, new_path)
    }
    /// Indicates how this engine establishes completion for `set_location`.
    fn move_completion_semantics(&self) -> MoveCompletionSemantics {
        MoveCompletionSemantics::Unconfirmed
    }
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
    /// Applies one priority to many files (folder-level selection). Engines
    /// with a batch API override this; the default loops over the files.
    fn set_files_priority(
        &self,
        id: &str,
        file_indices: &[u32],
        priority: u8,
    ) -> Result<(), String> {
        for index in file_indices {
            self.set_file_priority(id, *index, priority)?;
        }
        Ok(())
    }
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
    /// Manual peer blocks are global IP blocks. Engines that cannot enforce
    /// them must leave the capability unsupported rather than silently
    /// persisting a UI-only preference.
    fn peer_block_capability(&self) -> crate::models::peering::PeerBlockCapability {
        crate::models::peering::PeerBlockCapability::Unsupported
    }
    /// Disconnect the selected peer before a global IP block is persisted.
    fn disconnect_peer_for_block(
        &self,
        _torrent_id: &str,
        _ip: &str,
        _port: u16,
    ) -> Result<(), String> {
        Err("Manual peer blocking is not supported by this engine".to_string())
    }
    /// Whether the engine is bound to IPv4 only (bind-address-ipv6 == ::1).
    /// When so, the port test must pass ip_protocol=ipv4 or Transmission 4.1.3
    /// probes IPv6 first and returns "No Response" instead of a verdict.
    fn port_protocol(&self) -> Option<&'static str> {
        None
    }

    /// Test whether the engine's peer port is externally reachable.
    /// `ip_protocol`, when set, forces Transmission to test that address family.
    fn check_port_open(&self, _ip_protocol: Option<&'static str>) -> Option<bool> {
        None
    }

    /// Tests exactly one address family. Unlike `check_port_open`, a failed or
    /// inconclusive family test never falls back to an unqualified probe, which
    /// on a dual-stack engine would report whichever family answered first.
    fn check_port_family(&self, _ip_protocol: &'static str) -> Option<bool> {
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

    /// Read-only engine diagnostic reporting exact daemon path/PID,
    /// effective IPv4/IPv6 bind settings, VPN policy, IPv6 listeners,
    /// and established IPv6 peer count.
    fn engine_ipv6_diagnostic(
        &self,
    ) -> Result<crate::models::connectivity::EngineIpv6Diagnostic, String> {
        Err("Not supported by this engine".to_string())
    }
}

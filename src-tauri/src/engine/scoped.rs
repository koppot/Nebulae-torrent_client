//! Routes UI torrent commands to either the local engine (Manager tab) or the
//! Nebulae Server engine (Server tab) while background work (seeding rules,
//! kill switch, watch folders, ADC delivery, RSS, telemetry rates) keeps using
//! the local engine directly through [`LocalEngine`].

use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use super::{RemoteServerEngine, SharedEngine};
use crate::commands::torrent::TorrentItem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// The always-running local engine, for code that must never be redirected to the server.
#[derive(Clone)]
pub struct LocalEngine(pub SharedEngine);

impl std::ops::Deref for LocalEngine {
    type Target = SharedEngine;
    fn deref(&self) -> &SharedEngine {
        &self.0
    }
}

struct RemoteSlot {
    url: String,
    token: String,
    engine: SharedEngine,
}

pub struct ScopedEngine {
    local: SharedEngine,
    remote: RwLock<Option<RemoteSlot>>,
    remote_active: AtomicBool,
}

impl ScopedEngine {
    pub fn new(local: SharedEngine) -> Self {
        Self {
            local,
            remote: RwLock::new(None),
            remote_active: AtomicBool::new(false),
        }
    }

    /// Points the server scope at `url`; an unchanged url/token keeps the existing client.
    pub fn configure_remote(&self, url: &str, token: &str) -> Result<(), String> {
        let url = url.trim();
        if url.is_empty() {
            return Err(
                "No Nebulae Server address is configured (Settings -> Server).".to_string(),
            );
        }
        let mut slot = self
            .remote
            .write()
            .map_err(|_| "engine lock poisoned".to_string())?;
        if let Some(existing) = slot.as_ref() {
            if existing.url == url && existing.token == token {
                return Ok(());
            }
        }
        *slot = Some(RemoteSlot {
            url: url.to_string(),
            token: token.to_string(),
            engine: Arc::new(RemoteServerEngine::new(url.to_string(), token.to_string())),
        });
        Ok(())
    }

    pub fn set_remote_active(&self, remote: bool) -> Result<(), String> {
        if remote && self.remote_engine().is_none() {
            return Err("No Nebulae Server is configured.".to_string());
        }
        self.remote_active.store(remote, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_remote_active(&self) -> bool {
        self.remote_active.load(Ordering::SeqCst)
    }

    pub fn remote_engine(&self) -> Option<SharedEngine> {
        self.remote
            .read()
            .ok()
            .and_then(|s| s.as_ref().map(|r| Arc::clone(&r.engine)))
    }

    fn active(&self) -> SharedEngine {
        if self.is_remote_active() {
            if let Some(remote) = self.remote_engine() {
                return remote;
            }
        }
        Arc::clone(&self.local)
    }
}

impl TorrentEngine for ScopedEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        self.active().list_torrents()
    }
    fn get_torrent_details(&self, id: &str) -> Result<TorrentItem, String> {
        self.active().get_torrent_details(id)
    }
    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        self.active().add_torrent(source, download_dir)
    }
    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        self.active().add_torrent_from_bytes(bytes, download_dir)
    }
    fn add_torrent_from_bytes_paused(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        self.active()
            .add_torrent_from_bytes_paused(bytes, download_dir)
    }
    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        self.active().remove_torrent(id, delete_files)
    }
    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        self.active().pause_torrent(id)
    }
    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        self.active().resume_torrent(id)
    }
    fn pause_all(&self) -> Result<(), String> {
        self.active().pause_all()
    }
    fn resume_all(&self) -> Result<(), String> {
        self.active().resume_all()
    }
    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.active().trigger_kill_switch()
    }
    fn reset_kill_switch(&self) -> Result<(), String> {
        self.active().reset_kill_switch()
    }
    fn is_kill_switch_active(&self) -> bool {
        self.active().is_kill_switch_active()
    }
    fn engine_status(&self) -> EngineStatus {
        self.active().engine_status()
    }
    fn supports_hook(&self, hook: EngineHook) -> bool {
        self.active().supports_hook(hook)
    }
    fn is_remote(&self) -> bool {
        self.is_remote_active()
    }
    fn shutdown(&self) {
        self.local.shutdown();
    }
    fn force_recheck(&self, id: &str) -> Result<(), String> {
        self.active().force_recheck(id)
    }
    fn reannounce(&self, id: &str) -> Result<(), String> {
        self.active().reannounce(id)
    }
    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        self.active().set_location(id, new_path)
    }
    fn move_completion_semantics(&self) -> crate::engine::traits::MoveCompletionSemantics {
        self.active().move_completion_semantics()
    }
    fn relink_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        self.active().relink_location(id, new_path)
    }
    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        self.active().set_category(id, category)
    }
    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        self.active()
            .set_torrent_limits(id, dl_kib, ul_kib, ratio_limit)
    }
    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String> {
        self.active().toggle_sequential_download(id)
    }
    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String> {
        self.active().toggle_first_last_piece_priority(id)
    }
    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String> {
        self.active().toggle_super_seeding(id)
    }
    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        self.active().set_file_priority(id, file_index, priority)
    }
    fn set_files_priority(
        &self,
        id: &str,
        file_indices: &[u32],
        priority: u8,
    ) -> Result<(), String> {
        self.active().set_files_priority(id, file_indices, priority)
    }
    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String> {
        self.active().rename_torrent(id, new_name)
    }
    fn rename_torrent_file(
        &self,
        id: &str,
        file_index: u32,
        new_file_name: &str,
    ) -> Result<(), String> {
        self.active()
            .rename_torrent_file(id, file_index, new_file_name)
    }
    fn rename_torrent_folder(
        &self,
        id: &str,
        old_folder_path: &str,
        new_folder_name: &str,
    ) -> Result<(), String> {
        self.active()
            .rename_torrent_folder(id, old_folder_path, new_folder_name)
    }
    fn export_torrent_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        self.active().export_torrent_bytes(id)
    }
    fn get_peer_telemetry(
        &self,
        id: &str,
    ) -> Result<Vec<crate::models::peering::PeerTelemetry>, String> {
        self.active().get_peer_telemetry(id)
    }
    fn peer_block_capability(&self) -> crate::models::peering::PeerBlockCapability {
        self.active().peer_block_capability()
    }
    fn disconnect_peer_for_block(
        &self,
        torrent_id: &str,
        ip: &str,
        port: u16,
    ) -> Result<(), String> {
        self.active()
            .disconnect_peer_for_block(torrent_id, ip, port)
    }
    fn set_torrent_racing_mode(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.active().set_torrent_racing_mode(id, enabled)
    }

    // Session configuration and network diagnostics describe this computer's engine.
    fn check_port_open(&self, ip_protocol: Option<&'static str>) -> Option<bool> {
        self.local.check_port_open(ip_protocol)
    }
    fn check_port_family(&self, ip_protocol: &'static str) -> Option<bool> {
        self.local.check_port_family(ip_protocol)
    }
    fn port_protocol(&self) -> Option<&'static str> {
        self.local.port_protocol()
    }
    fn apply_peering_settings(
        &self,
        settings: &crate::models::peering::PeeringSettings,
    ) -> Result<(), String> {
        self.local.apply_peering_settings(settings)
    }
    fn apply_global_settings(
        &self,
        settings: &crate::commands::settings::UserSettings,
    ) -> Result<(), String> {
        self.local.apply_global_settings(settings)
    }
    fn engine_version(&self) -> Option<String> {
        self.local.engine_version()
    }
    fn engine_ipv6_diagnostic(
        &self,
    ) -> Result<crate::models::connectivity::EngineIpv6Diagnostic, String> {
        self.local.engine_ipv6_diagnostic()
    }
}

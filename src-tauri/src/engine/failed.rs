use super::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::commands::torrent::TorrentItem;

pub struct FailedTorrentEngine {
    pub error_message: String,
}

impl FailedTorrentEngine {
    pub fn new(error_message: String) -> Self {
        Self { error_message }
    }
}

impl TorrentEngine for FailedTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        Ok(vec![])
    }

    fn add_torrent(&self, _source: &str, _download_dir: Option<&str>) -> Result<String, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn add_torrent_from_bytes(
        &self,
        _bytes: &[u8],
        _download_dir: Option<&str>,
    ) -> Result<String, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn remove_torrent(&self, _id: &str, _delete_files: bool) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn pause_torrent(&self, _id: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn resume_torrent(&self, _id: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn pause_all(&self) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn resume_all(&self) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn trigger_kill_switch(&self) -> Result<(), String> {
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        false
    }

    fn engine_status(&self) -> EngineStatus {
        EngineStatus {
            active_torrents: 0,
            paused_torrents: 0,
            kill_switch_engaged: false,
            bound_interface: None,
        }
    }

    fn supports_hook(&self, _hook: EngineHook) -> bool {
        false
    }

    fn force_recheck(&self, _id: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn reannounce(&self, _id: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn set_location(&self, _id: &str, _new_path: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn set_category(&self, _id: &str, _category: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn set_torrent_limits(
        &self,
        _id: &str,
        _dl_kib: u32,
        _ul_kib: u32,
        _ratio_limit: f32,
    ) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn toggle_sequential_download(&self, _id: &str) -> Result<bool, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn toggle_first_last_piece_priority(&self, _id: &str) -> Result<bool, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn toggle_super_seeding(&self, _id: &str) -> Result<bool, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn set_file_priority(&self, _id: &str, _file_index: u32, _priority: u8) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn rename_torrent(&self, _id: &str, _new_name: &str) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn rename_torrent_file(
        &self,
        _id: &str,
        _file_index: u32,
        _new_file_name: &str,
    ) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn rename_torrent_folder(
        &self,
        _id: &str,
        _old_folder_path: &str,
        _new_folder_name: &str,
    ) -> Result<(), String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }

    fn export_torrent_bytes(&self, _id: &str) -> Result<Vec<u8>, String> {
        Err(format!(
            "Torrent engine unavailable: {}",
            self.error_message
        ))
    }
}

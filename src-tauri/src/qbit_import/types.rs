use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QbitSourceType {
    LegacyFile {
        torrent_path: PathBuf,
        fastresume_path: PathBuf,
    },
    SqliteRow {
        info_hash: String,
        db_path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitTorrent {
    pub info_hash: String,
    pub name: String,
    pub save_path: String,
    pub download_path: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub content_layout: Option<String>, // Original, Subfolder, NoSubfolder
    pub paused: bool,
    pub auto_managed: bool,
    pub sequential_download: bool,
    pub first_last_piece_prio: bool,
    pub ratio_limit: f32,
    pub seeding_time_limit: Option<i64>,
    pub upload_limit_kib: u32,
    pub download_limit_kib: u32,
    pub added_time: u64,
    pub completed_time: Option<u64>,
    pub total_size: u64,
    pub progress: f32,
    pub state_string: String,
    pub trackers: Vec<String>,
    pub mapped_files: Vec<String>,
    pub file_priorities: Vec<u8>,
    pub raw_torrent_bytes: Vec<u8>,
    pub source: QbitSourceType,
    pub already_in_nebula: bool,
    pub missing_path: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitTorrentSummary {
    pub info_hash: String,
    pub name: String,
    pub size_bytes: u64,
    pub save_path: String,
    pub effective_path: String,
    pub progress: f32,
    pub state: String,
    pub category: String,
    pub status_label: String,
    pub already_in_nebula: bool,
    pub missing_path: bool,
    pub source_format: String, // "Legacy BT_backup" or "SQLite torrents.db"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub qbit_dir: String,
    pub storage_format: String,
    pub torrents: Vec<QbitTorrentSummary>,
    pub warnings: Vec<String>,
    pub is_qbit_running: bool,
    pub is_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    pub info_hashes: Vec<String>,
    pub skip_recheck: bool,
    pub remove_from_qbit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSkipInfo {
    pub info_hash: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFailInfo {
    pub info_hash: String,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentNotRemovedInfo {
    pub info_hash: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummaryReport {
    pub total_selected: usize,
    pub imported_count: usize,
    pub skipped: Vec<TorrentSkipInfo>,
    pub failed: Vec<TorrentFailInfo>,
    pub removed_count: usize,
    pub not_removed: Vec<TorrentNotRemovedInfo>,
    pub backup_dir: Option<String>,
    pub log_file: String,
    pub log_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitProcessCheck {
    pub is_running: bool,
    pub process_name: Option<String>,
    pub message: String,
}

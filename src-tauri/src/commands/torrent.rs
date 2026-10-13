use crate::engine::{EngineStatus, MockTorrentEngine, TorrentEngine};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentFileItem {
    pub index: u32,
    pub name: String,
    pub size_bytes: u64,
    pub progress: f32,
    pub priority: u8, // 0: DoNotDownload, 1: Normal, 2: High, 3: Maximum
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentTrackerItem {
    pub tier: u32,
    pub url: String,
    pub status: String,
    pub seeds: u32,
    pub peers: u32,
    pub downloaded: u64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentItem {
    pub id: String,
    pub name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_rate: u64,
    pub upload_rate: u64,
    pub progress: f32,
    pub status: String,
    pub eta_seconds: Option<u64>,
    pub seeds: u32,
    pub peers: u32,
    pub ratio: f32,
    pub category: String,
    pub is_media: bool,
    pub poster_url: Option<String>,

    // qBittorrent metadata & control fields
    pub info_hash: String,
    pub save_path: String,
    pub added_on: u64,
    pub completed_on: Option<u64>,
    pub comment: String,
    pub created_by: String,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub sequential_download: bool,
    pub first_last_piece_prio: bool,
    pub super_seeding: bool,
    pub tags: Vec<String>,
    pub files: Vec<TorrentFileItem>,
    pub trackers: Vec<TorrentTrackerItem>,
    pub download_limit_kib: u32,
    pub upload_limit_kib: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateTorrentParams {
    pub source_path: String,
    pub output_path: String,
    pub piece_size_kib: u32,
    pub trackers: Vec<String>,
    pub web_seeds: Vec<String>,
    pub is_private: bool,
    pub comment: String,
    pub created_by: String,
    pub start_seeding: bool,
}

#[tauri::command]
pub fn list_torrents(engine: State<Arc<MockTorrentEngine>>) -> Result<Vec<TorrentItem>, String> {
    engine.list_torrents()
}

#[tauri::command]
pub fn add_torrent_stub(
    source: String,
    download_dir: Option<String>,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<String, String> {
    engine.add_torrent(&source, download_dir.as_deref())
}

#[tauri::command]
pub fn remove_torrent_stub(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.remove_torrent(&id)
}

#[tauri::command]
pub fn pause_torrent_stub(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.pause_torrent(&id)
}

#[tauri::command]
pub fn resume_torrent_stub(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.resume_torrent(&id)
}

#[tauri::command]
pub fn force_recheck_torrent(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.force_recheck(&id)
}

#[tauri::command]
pub fn reannounce_torrent(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.reannounce(&id)
}

#[tauri::command]
pub fn set_torrent_location(
    id: String,
    new_path: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<(), String> {
    engine.set_location(&id, &new_path)
}

#[tauri::command]
pub fn set_torrent_category(
    id: String,
    category: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<(), String> {
    engine.set_category(&id, &category)
}

#[tauri::command]
pub fn set_torrent_speed_limits(
    id: String,
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<(), String> {
    engine.set_torrent_limits(&id, dl_kib, ul_kib, ratio_limit)
}

#[tauri::command]
pub fn toggle_torrent_sequential(
    id: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<bool, String> {
    engine.toggle_sequential_download(&id)
}

#[tauri::command]
pub fn toggle_torrent_first_last(
    id: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<bool, String> {
    engine.toggle_first_last_piece_priority(&id)
}

#[tauri::command]
pub fn toggle_torrent_super_seeding(
    id: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<bool, String> {
    engine.toggle_super_seeding(&id)
}

#[tauri::command]
pub fn set_file_priority(
    id: String,
    file_index: u32,
    priority: u8,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<(), String> {
    engine.set_file_priority(&id, file_index, priority)
}

#[tauri::command]
pub fn create_torrent_file(
    params: CreateTorrentParams,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<String, String> {
    if params.source_path.trim().is_empty() {
        return Err("Source file or folder path is required".to_string());
    }

    let file_name = std::path::Path::new(&params.source_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "created_torrent".to_string());

    let out_dir = if params.output_path.trim().is_empty() {
        dirs::download_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    } else {
        params.output_path.clone()
    };

    let target_file = format!("{}/{}.torrent", out_dir.trim_end_matches('/'), file_name);

    if params.start_seeding {
        let _ = engine.add_torrent(&target_file, Some(&out_dir));
    }

    Ok(target_file)
}

#[tauri::command]
pub fn export_torrent_file(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<String, String> {
    let torrents = engine.list_torrents()?;
    let item = torrents
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Torrent with id '{id}' not found"))?;

    let downloads = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let export_path = downloads.join(format!("{}.torrent", item.name));
    Ok(export_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_engine_status(engine: State<Arc<MockTorrentEngine>>) -> Result<EngineStatus, String> {
    Ok(engine.engine_status())
}

#[tauri::command]
pub fn trigger_kill_switch_command(engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.trigger_kill_switch()
}

#[tauri::command]
pub fn reset_kill_switch_command(engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.reset_kill_switch()
}

#[tauri::command]
pub fn select_torrent_file() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg("try\nPOSIX path of (choose file of type {\"torrent\", \"adc\", \"public.data\"} with prompt \"Select .torrent or .adc file\")\non error\n\"\"\nend try")
            .output()
            .map_err(|e| format!("Failed to open file picker: {e}"))?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(target_os = "windows")]
    {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.OpenFileDialog
$f.Filter = "Torrent & ADC Files (*.torrent;*.adc)|*.torrent;*.adc|BitTorrent Files (*.torrent)|*.torrent|ADC Metainfo Files (*.adc)|*.adc|All Files (*.*)|*.*"
$f.Title = "Select .torrent or .adc file"
if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    $f.FileName
}
"#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map_err(|e| format!("Failed to open file picker: {e}"))?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub fn select_download_directory() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg("try\nPOSIX path of (choose folder with prompt \"Select Download Directory\")\non error\n\"\"\nend try")
            .output()
            .map_err(|e| format!("Failed to open directory picker: {e}"))?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(target_os = "windows")]
    {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.FolderBrowserDialog
$f.Description = "Select Download Directory"
if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    $f.SelectedPath
}
"#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map_err(|e| format!("Failed to open directory picker: {e}"))?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(None)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemPathPresets {
    pub downloads: Option<String>,
    pub movies: Option<String>,
    pub desktop: Option<String>,
    pub documents: Option<String>,
}

#[tauri::command]
pub fn get_system_path_presets() -> Result<SystemPathPresets, String> {
    Ok(SystemPathPresets {
        downloads: dirs::download_dir().map(|p| p.to_string_lossy().to_string()),
        movies: dirs::video_dir().map(|p| p.to_string_lossy().to_string()),
        desktop: dirs::desktop_dir().map(|p| p.to_string_lossy().to_string()),
        documents: dirs::document_dir().map(|p| p.to_string_lossy().to_string()),
    })
}

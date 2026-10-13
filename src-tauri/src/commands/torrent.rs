use crate::engine::{EngineStatus, MockTorrentEngine, TorrentEngine};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

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
pub fn pause_torrent_stub(id: String, engine: State<Arc<MockTorrentEngine>>) -> Result<(), String> {
    engine.pause_torrent(&id)
}

#[tauri::command]
pub fn resume_torrent_stub(
    id: String,
    engine: State<Arc<MockTorrentEngine>>,
) -> Result<(), String> {
    engine.resume_torrent(&id)
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
            .arg("try\nPOSIX path of (choose file of type {\"torrent\", \"public.data\"} with prompt \"Select .torrent file\")\non error\n\"\"\nend try")
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
$f.Filter = "Torrent Files (*.torrent)|*.torrent|All Files (*.*)|*.*"
$f.Title = "Select .torrent file"
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

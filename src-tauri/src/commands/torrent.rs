use crate::engine::{EngineStatus, SharedEngine};
use serde::{Deserialize, Serialize};
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
    #[serde(default)]
    pub super_seeding: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub files: Vec<TorrentFileItem>,
    #[serde(default)]
    pub trackers: Vec<TorrentTrackerItem>,
    #[serde(default)]
    pub download_limit_kib: u32,
    #[serde(default)]
    pub upload_limit_kib: u32,
    #[serde(default)]
    pub ratio_limit: f32,
    #[serde(default)]
    pub media_meta: Option<crate::utils::media_parser::ParsedMediaMetadata>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InspectTorrentResult {
    pub name: String,
    pub total_bytes: u64,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub info_hash: String,
    pub is_private: bool,
    pub comment: String,
    pub created_by: String,
    pub files: Vec<TorrentFileItem>,
    pub trackers: Vec<TorrentTrackerItem>,
}

#[tauri::command]
pub fn inspect_torrent_file(
    source: Option<String>,
    bytes: Option<Vec<u8>>,
) -> Result<InspectTorrentResult, String> {
    // 1. If raw bytes are passed
    if let Some(raw) = bytes {
        if !raw.is_empty() {
            let parsed = crate::engine::metainfo::ParsedTorrent::parse_bytes(&raw)?;
            return Ok(InspectTorrentResult {
                name: parsed.name,
                total_bytes: parsed.total_bytes,
                piece_size: parsed.piece_size,
                total_pieces: parsed.total_pieces,
                info_hash: parsed.info_hash,
                is_private: parsed.is_private,
                comment: parsed.comment,
                created_by: parsed.created_by,
                files: parsed.files,
                trackers: parsed.trackers,
            });
        }
    }

    // 2. If source string is passed
    if let Some(src) = source {
        let trimmed = src.trim();

        // Base64 encoded torrent?
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = crate::engine::mock::decode_base64_data(b64) {
                if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_bytes(&decoded) {
                    return Ok(InspectTorrentResult {
                        name: parsed.name,
                        total_bytes: parsed.total_bytes,
                        piece_size: parsed.piece_size,
                        total_pieces: parsed.total_pieces,
                        info_hash: parsed.info_hash,
                        is_private: parsed.is_private,
                        comment: parsed.comment,
                        created_by: parsed.created_by,
                        files: parsed.files,
                        trackers: parsed.trackers,
                    });
                }
            }
        }

        // Magnet link?
        if trimmed.starts_with("magnet:") {
            let parsed_name = trimmed
                .split("&dn=")
                .nth(1)
                .and_then(|dn| dn.split('&').next())
                .unwrap_or("Magnet Download");
            let name = parsed_name.replace('+', " ").replace("%20", " ");
            let xt_hash = trimmed
                .split("xt=urn:btih:")
                .nth(1)
                .and_then(|h| h.split('&').next())
                .unwrap_or("unknown_hash");
            return Ok(InspectTorrentResult {
                name,
                total_bytes: 0,
                piece_size: 1_048_576,
                total_pieces: 0,
                info_hash: xt_hash.to_lowercase(),
                is_private: false,
                comment: "Magnet Link".to_string(),
                created_by: "Nebulae Metainfo Tool".to_string(),
                files: vec![],
                trackers: vec![],
            });
        }

        // Disk path?
        let norm_path = crate::engine::mock::normalize_source_path(trimmed);
        if norm_path.exists() {
            if norm_path.is_file() {
                if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_file(&norm_path) {
                    return Ok(InspectTorrentResult {
                        name: parsed.name,
                        total_bytes: parsed.total_bytes,
                        piece_size: parsed.piece_size,
                        total_pieces: parsed.total_pieces,
                        info_hash: parsed.info_hash,
                        is_private: parsed.is_private,
                        comment: parsed.comment,
                        created_by: parsed.created_by,
                        files: parsed.files,
                        trackers: parsed.trackers,
                    });
                }
            } else if norm_path.is_dir() {
                let files = crate::engine::mock::scan_directory_files(&norm_path)?;
                let total_bytes: u64 = files.iter().map(|f| f.size_bytes).sum();
                let dir_name = norm_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Local Directory".to_string());
                return Ok(InspectTorrentResult {
                    name: dir_name,
                    total_bytes,
                    piece_size: 1_048_576,
                    total_pieces: total_bytes.div_ceil(1_048_576) as u32,
                    info_hash: format!("local_{:x}", total_bytes),
                    is_private: false,
                    comment: "Local Storage Directory".to_string(),
                    created_by: "Nebulae Metainfo Tool".to_string(),
                    files,
                    trackers: vec![],
                });
            }
        }
    }

    Err("Could not inspect torrent: invalid source or unsupported format".to_string())
}

#[tauri::command]
pub fn list_torrents(engine: State<SharedEngine>) -> Result<Vec<TorrentItem>, String> {
    engine.list_torrents()
}

#[tauri::command]
pub fn add_torrent(
    source: String,
    download_dir: Option<String>,
    bytes: Option<Vec<u8>>,
    engine: State<SharedEngine>,
) -> Result<String, String> {
    if let Some(raw) = bytes {
        if !raw.is_empty() {
            return engine.add_torrent_from_bytes(&raw, download_dir.as_deref());
        }
    }
    engine.add_torrent(&source, download_dir.as_deref())
}

#[tauri::command]
pub fn add_torrent_bytes(
    bytes: Vec<u8>,
    download_dir: Option<String>,
    engine: State<SharedEngine>,
) -> Result<String, String> {
    engine.add_torrent_from_bytes(&bytes, download_dir.as_deref())
}

#[tauri::command]
pub fn remove_torrent(
    id: String,
    delete_files: Option<bool>,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.remove_torrent(&id, delete_files.unwrap_or(false))
}

#[tauri::command]
pub fn pause_torrent(id: String, engine: State<SharedEngine>) -> Result<(), String> {
    engine.pause_torrent(&id)
}

#[tauri::command]
pub fn resume_torrent(id: String, engine: State<SharedEngine>) -> Result<(), String> {
    engine.resume_torrent(&id)
}

#[tauri::command]
pub fn pause_all_torrents(engine: State<SharedEngine>) -> Result<(), String> {
    engine.pause_all()
}

#[tauri::command]
pub fn resume_all_torrents(engine: State<SharedEngine>) -> Result<(), String> {
    engine.resume_all()
}

#[tauri::command]
pub fn force_recheck_torrent(id: String, engine: State<SharedEngine>) -> Result<(), String> {
    engine.force_recheck(&id)
}

#[tauri::command]
pub fn reannounce_torrent(id: String, engine: State<SharedEngine>) -> Result<(), String> {
    engine.reannounce(&id)
}

#[tauri::command]
pub fn set_torrent_location(
    id: String,
    new_path: String,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.set_location(&id, &new_path)
}

#[tauri::command]
pub fn set_torrent_category(
    id: String,
    category: String,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.set_category(&id, &category)
}

#[tauri::command]
pub fn set_torrent_speed_limits(
    id: String,
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.set_torrent_limits(&id, dl_kib, ul_kib, ratio_limit)
}

#[tauri::command]
pub fn toggle_torrent_sequential(id: String, engine: State<SharedEngine>) -> Result<bool, String> {
    engine.toggle_sequential_download(&id)
}

#[tauri::command]
pub fn toggle_torrent_first_last(id: String, engine: State<SharedEngine>) -> Result<bool, String> {
    engine.toggle_first_last_piece_priority(&id)
}

#[tauri::command]
pub fn toggle_torrent_super_seeding(
    id: String,
    engine: State<SharedEngine>,
) -> Result<bool, String> {
    engine.toggle_super_seeding(&id)
}

#[tauri::command]
pub fn set_file_priority(
    id: String,
    file_index: u32,
    priority: u8,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.set_file_priority(&id, file_index, priority)
}

#[tauri::command]
pub fn rename_torrent(
    id: String,
    new_name: String,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent(&id, &new_name)
}

#[tauri::command]
pub fn rename_torrent_file(
    id: String,
    file_index: u32,
    new_file_name: String,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent_file(&id, file_index, &new_file_name)
}

#[tauri::command]
pub fn rename_torrent_folder(
    id: String,
    old_folder_path: String,
    new_folder_name: String,
    engine: State<SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent_folder(&id, &old_folder_path, &new_folder_name)
}

#[tauri::command]
pub async fn create_torrent_file(
    params: CreateTorrentParams,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let source_path_str = params.source_path.trim().to_string();
    if source_path_str.is_empty() {
        return Err("Source file or folder path is required".to_string());
    }

    let source = std::path::PathBuf::from(&source_path_str);
    if !source.exists() {
        return Err(format!(
            "Source path '{}' does not exist on disk",
            source.display()
        ));
    }

    // A torrent without at least one tracker URL is rejected: no .torrent
    // file is generated until the user enters one (owner requirement).
    let trackers_clean: Vec<String> = params
        .trackers
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if trackers_clean.is_empty() {
        return Err("Please enter torrent tracker URL".to_string());
    }

    let out_dir = if params.output_path.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(params.output_path.trim())
    };

    let params_clone = params.clone();
    let source_clone = source.clone();
    let out_dir_clone = out_dir.clone();
    let trackers_for_build = trackers_clean.clone();

    let target_file = tokio::task::spawn_blocking(move || {
        crate::engine::metainfo::build_torrent_file(
            &source_clone,
            &out_dir_clone,
            params_clone.piece_size_kib,
            &trackers_for_build,
            params_clone.is_private,
            &params_clone.comment,
            &params_clone.created_by,
        )
    })
    .await
    .map_err(|e| format!("Task execution error: {e}"))??;

    let target_file_str = target_file.to_string_lossy().to_string();

    if params.start_seeding {
        let parent = source.parent().map(|p| p.to_string_lossy().to_string());
        let _ = engine.add_torrent(&target_file_str, parent.as_deref());
    }

    Ok(target_file_str)
}

#[tauri::command]
pub fn export_torrent_file(
    id: String,
    destination_path: Option<String>,
    engine: State<SharedEngine>,
) -> Result<String, String> {
    let bytes = engine.export_torrent_bytes(&id)?;
    let torrents = engine.list_torrents()?;
    let item = torrents.iter().find(|t| t.id == id);
    let raw_name = item.map(|t| t.name.clone()).unwrap_or_else(|| id.clone());
    let clean_name = raw_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let filename = if clean_name.to_lowercase().ends_with(".torrent") {
        clean_name
    } else {
        format!("{clean_name}.torrent")
    };

    let target_path = if let Some(dest) = destination_path {
        let p = std::path::PathBuf::from(dest.trim());
        if p.is_dir() {
            p.join(&filename)
        } else {
            p
        }
    } else {
        let downloads = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        downloads.join(&filename)
    };

    if let Some(parent) = target_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(&target_path, &bytes).map_err(|e| {
        format!(
            "Failed to write exported .torrent file to '{}': {e}",
            target_path.display()
        )
    })?;

    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_engine_status(engine: State<SharedEngine>) -> Result<EngineStatus, String> {
    Ok(engine.engine_status())
}

#[tauri::command]
pub fn trigger_kill_switch_command(engine: State<SharedEngine>) -> Result<(), String> {
    engine.trigger_kill_switch()
}

#[tauri::command]
pub fn reset_kill_switch_command(engine: State<SharedEngine>) -> Result<(), String> {
    engine.reset_kill_switch()
}

#[tauri::command]
pub async fn select_source_folder() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose folder with prompt "Select Folder for Torrent Creation")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open folder picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(target_os = "windows")]
        {
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.FolderBrowserDialog
            $f.Description = "Select Folder for Torrent Creation"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.SelectedPath
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open folder picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_dir("Select Folder for Torrent Creation")
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

#[tauri::command]
pub async fn select_source_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose file with prompt "Select File for Torrent Creation")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(target_os = "windows")]
        {
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.OpenFileDialog
            $f.Title = "Select File for Torrent Creation"
            $f.Filter = "All Files (*.*)|*.*"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.FileName
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_file("Select File for Torrent Creation", None)
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

#[tauri::command]
pub async fn select_torrent_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose file of type {"torrent", "adc", "public.data"} with prompt "Select .torrent or .adc file")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
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
            let output = crate::utils::process::run_hidden_powershell(script)
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
            linux_pick_file(
                "Select .torrent or .adc file",
                Some(("Torrent & ADC Files (*.torrent, *.adc)", "*.torrent *.adc")),
            )
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

pub fn parse_dialog_output(
    status_success: bool,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Option<String>, String> {
    if status_success {
        let path = String::from_utf8_lossy(stdout).trim().to_string();
        if path.is_empty() {
            return Ok(None);
        }
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err(format!(
                "Selected path does not exist or is unreadable: {path}"
            ));
        }
        Ok(Some(path))
    } else if exit_code == Some(1) {
        // User cancelled the file dialog
        Ok(None)
    } else {
        let err_msg = String::from_utf8_lossy(stderr).trim().to_string();
        Err(format!(
            "Dialog failed with exit code {:?}: {}",
            exit_code,
            if err_msg.is_empty() {
                "Operation cancelled or dialog closed"
            } else {
                &err_msg
            }
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn linux_pick_dir(title: &str) -> Result<Option<String>, String> {
    match std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--directory",
            &format!("--title={title}"),
        ])
        .output()
    {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute zenity: {e}"));
        }
        Err(_) => {}
    }

    match std::process::Command::new("kdialog")
        .args(["--title", title, "--getexistingdirectory", "."])
        .output()
    {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute kdialog: {e}"));
        }
        Err(_) => {}
    }

    Err("No supported graphical file dialog found. Please install 'zenity' (GNOME/GTK) or 'kdialog' (KDE/Qt) to use directory selection.".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn linux_pick_file(title: &str, filter: Option<(&str, &str)>) -> Result<Option<String>, String> {
    let mut zenity_cmd = std::process::Command::new("zenity");
    zenity_cmd.args(["--file-selection", &format!("--title={title}")]);
    if let Some((name, patterns)) = filter {
        zenity_cmd.arg(format!("--file-filter={name} | {patterns}"));
        zenity_cmd.arg("--file-filter=All Files | *");
    }
    match zenity_cmd.output() {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute zenity: {e}"));
        }
        Err(_) => {}
    }

    let mut kdialog_cmd = std::process::Command::new("kdialog");
    kdialog_cmd.args(["--title", title, "--getopenfilename", "."]);
    if let Some((name, patterns)) = filter {
        kdialog_cmd.arg(format!("{patterns}|{name}"));
    }
    match kdialog_cmd.output() {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute kdialog: {e}"));
        }
        Err(_) => {}
    }

    Err("No supported graphical file dialog found. Please install 'zenity' (GNOME/GTK) or 'kdialog' (KDE/Qt) to use file selection.".to_string())
}

#[tauri::command]
pub async fn select_download_directory() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose folder with prompt "Select Download Directory")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
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
            let output = crate::utils::process::run_hidden_powershell(script)
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
            linux_pick_dir("Select Download Directory")
        }
    })
    .await
    .map_err(|e| format!("Directory picker task error: {e}"))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_dialog_output_user_cancel() {
        let res = parse_dialog_output(false, Some(1), b"", b"");
        assert_eq!(res, Ok(None));
    }

    #[test]
    fn test_parse_dialog_output_empty_selection() {
        let res = parse_dialog_output(true, Some(0), b"\n", b"");
        assert_eq!(res, Ok(None));
    }

    #[test]
    fn test_parse_dialog_output_nonexistent_path() {
        let res = parse_dialog_output(
            true,
            Some(0),
            b"/tmp/non_existent_nebulae_path_12345\n",
            b"",
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist or is unreadable"));
    }

    #[test]
    fn test_parse_dialog_output_path_with_spaces() {
        let temp_dir = std::env::temp_dir().join("nebulae test with spaces");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_dialog_output_path_with_unicode() {
        let temp_dir = std::env::temp_dir().join("nebulae_テスト_日本語_файл");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_dialog_output_path_with_quotes_and_metachars() {
        let temp_dir = std::env::temp_dir().join("nebulae_test's & $VAR");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_dialog_output_dialog_error() {
        let res = parse_dialog_output(false, Some(127), b"", b"zenity: command not found\n");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("zenity: command not found"));
    }
}

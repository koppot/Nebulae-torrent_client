use crate::db::Database;
use crate::utils::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticSummary {
    pub app_name: String,
    pub executable_name: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub database_status: String,
    pub engine_status: String,
    pub internal_storage_status: String,
    pub total_logs_size_bytes: u64,
}

#[tauri::command]
pub fn open_logs_folder(paths: State<Arc<AppPaths>>) -> Result<(), String> {
    let logs_dir = &paths.logs_dir;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder on macOS: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder on Windows: {e}"))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(logs_dir)
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_external_link(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on macOS: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on Windows: {e}"))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on Linux: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_diagnostic_summary(paths: State<Arc<AppPaths>>) -> Result<DiagnosticSummary, String> {
    let logs_size = fs::metadata(paths.logs_dir.join("nebulae_session.log"))
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(DiagnosticSummary {
        app_name: "Nebulae".to_string(),
        executable_name: "nebulae".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        database_status: if paths.db_file.exists() {
            "Connected (Internal)"
        } else {
            "Uninitialized"
        }
        .to_string(),
        engine_status: "Initialized (rqbit local-first)".to_string(),
        internal_storage_status: "Isolated in Nebulae/internal/".to_string(),
        total_logs_size_bytes: logs_size,
    })
}

#[tauri::command]
pub fn clear_cache(paths: State<Arc<AppPaths>>) -> Result<String, String> {
    if paths.cache_dir.exists() {
        for entry in fs::read_dir(&paths.cache_dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            } else if path.is_dir() {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
    Ok("Internal cache cleared successfully.".to_string())
}

#[tauri::command]
pub fn reset_settings(db: State<Arc<Database>>) -> Result<String, String> {
    db.reset_all_settings()
        .map_err(|e| format!("Failed to reset settings: {e}"))?;
    Ok("Settings have been reset to factory defaults.".to_string())
}

#[tauri::command]
pub fn export_logs(paths: State<Arc<AppPaths>>) -> Result<String, String> {
    let log_path = paths.logs_dir.join("nebulae_session.log");
    if log_path.exists() {
        let content = fs::read_to_string(&log_path).map_err(|e| e.to_string())?;
        // Return latest 100 lines for diagnostic review
        let lines: Vec<&str> = content.lines().rev().take(100).collect();
        let preview = lines.into_iter().rev().collect::<Vec<&str>>().join("\n");
        Ok(preview)
    } else {
        Ok("No logs recorded yet.".to_string())
    }
}

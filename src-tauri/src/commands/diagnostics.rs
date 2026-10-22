use crate::db::Database;
use crate::utils::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tauri::State;

#[derive(Clone, Default)]
pub struct EngineInitState(pub Arc<std::sync::Mutex<Option<String>>>);

#[tauri::command]
pub fn get_engine_init_error(state: State<EngineInitState>) -> Option<String> {
    state.0.lock().unwrap().clone()
}

/// Frontend breadcrumb: writes a webview-side message into the session log
/// so clicks that never reach a command still leave a trace.
#[tauri::command]
pub fn log_frontend(message: String) {
    tracing::info!("[frontend] {message}");
}

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
        crate::utils::process::create_hidden_command("explorer")
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

pub fn validate_external_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!(
            "Disallowed URL scheme: only http/https URLs are permitted: {url}"
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn open_external_link(url: String) -> Result<(), String> {
    validate_external_url(&url)?;

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on macOS: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        crate::utils::process::create_hidden_command("explorer")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on Windows: {e}"))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open link on Linux via xdg-open: {e}. Please ensure xdg-utils is installed."))?;
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
        engine_status: "Initialized (Transmission sidecar)".to_string(),
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

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    use tauri::Manager;
    // Safety watchdog: ensure process terminates within 3s even if an external socket or child wedges
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        std::process::exit(0);
    });

    if let Some(player_state) = app.try_state::<crate::player::PlayerState>() {
        crate::commands::media::stop_session(&app, &player_state);
    }
    if let Some(engine) = app.try_state::<crate::SharedEngine>() {
        engine.shutdown();
    }
    crate::tailscale::shutdown();
    crate::updater::apply_staged_update_if_present(&app);
    app.exit(0);
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_external_url_valid() {
        assert!(validate_external_url("https://asiandvdclub.org").is_ok());
        assert!(validate_external_url("https://github.com/asiandvdclub/Nebulae").is_ok());
        assert!(validate_external_url("http://localhost:8080/test").is_ok());
    }

    #[test]
    fn test_validate_external_url_invalid() {
        assert!(validate_external_url("file:///etc/passwd").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("sh -c rm -rf /").is_err());
    }
}

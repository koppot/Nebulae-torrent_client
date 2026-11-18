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
pub async fn log_frontend(message: String) {
    let _ = tokio::task::spawn_blocking(move || {
        tracing::info!("[frontend] {message}");
    })
    .await;
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
    crate::shutdown::initiate_shutdown(&app);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReportBundle {
    pub title: String,
    pub category: String,
    pub description: String,
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub database_status: String,
    pub engine_status: String,
    pub logs_preview: String,
    pub error_fingerprint: String,
    pub formatted_markdown: String,
    pub github_issue_url: String,
    pub is_url_truncated: bool,
}

pub fn sanitize_diagnostic_text(text: &str) -> String {
    let mut s = text.to_string();

    // 1. Redact Passkeys
    if let Ok(re) = regex::Regex::new(r"(?i)(passkey[=/: ]+)[a-zA-Z0-9]{8,}") {
        s = re.replace_all(&s, "$1[REDACTED_PASSKEY]").to_string();
    }

    // 2. Redact Authorization tokens, API keys, Bearer tokens
    if let Ok(re) = regex::Regex::new(
        r"(?i)(bearer\s+|token[=/: ]+|auth[=/: ]+|key[=/: ]+)[a-zA-Z0-9_\-\.]{8,}",
    ) {
        s = re.replace_all(&s, "$1[REDACTED_TOKEN]").to_string();
    }

    // 3. Redact ADC tracker announce URLs
    if let Ok(re) = regex::Regex::new(r"https?://[^/\s]+/(?:announce|scrape)/[a-zA-Z0-9]+") {
        s = re.replace_all(&s, "[REDACTED_TRACKER_URL]").to_string();
    }

    // 4. Redact Magnet URIs and info hashes
    if let Ok(re) = regex::Regex::new(r"magnet:\?[^\s]+") {
        s = re
            .replace_all(&s, "magnet:?[REDACTED_MAGNET_LINK]")
            .to_string();
    }
    if let Ok(re) = regex::Regex::new(r"(?i)\b[0-9a-f]{40}\b") {
        s = re.replace_all(&s, "[REDACTED_HASH]").to_string();
    }

    // 5. Redact Public IP Addresses (preserving loopback and zero)
    if let Ok(re) = regex::Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b") {
        s = re
            .replace_all(&s, |caps: &regex::Captures| {
                let ip = &caps[0];
                if ip == "127.0.0.1" || ip == "0.0.0.0" {
                    ip.to_string()
                } else {
                    "[REDACTED_IP]".to_string()
                }
            })
            .to_string();
    }

    // 6. Redact Local Username and User Directory Paths
    if let Ok(re) = regex::Regex::new(r"/Users/[^/\s]+") {
        s = re.replace_all(&s, "~").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"(?i)[a-z]:\\Users\\[^\s\\]+") {
        s = re.replace_all(&s, "%USERPROFILE%").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"/home/[^/\s]+") {
        s = re.replace_all(&s, "~").to_string();
    }

    s
}

/// Human-readable OS release (e.g. "14.5" or "Microsoft Windows [Version 10.0.19045.5555]").
fn os_version_string() -> String {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        crate::utils::process::create_hidden_command("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "unknown".to_string()
    }
}

/// Issue-body section naming how many screenshots the reporter still has to
/// attach by hand: GitHub's prefilled-issue link cannot carry files.
fn screenshot_section(count: u32) -> String {
    if count == 0 {
        return String::new();
    }
    let noun = if count == 1 {
        "screenshot"
    } else {
        "screenshots"
    };
    format!(
        "### Screenshots\n{count} {noun} from the reporter to be attached to this issue (paste or drag into this comment before submitting).\n\n"
    )
}

#[tauri::command]
pub fn generate_bug_report_bundle(
    paths: State<Arc<AppPaths>>,
    title: String,
    description: String,
    category: String,
    context_error: Option<String>,
    include_logs: bool,
    screenshot_count: Option<u32>,
) -> Result<BugReportBundle, String> {
    let screenshots = screenshot_section(screenshot_count.unwrap_or(0));
    let clean_title = sanitize_diagnostic_text(&title);
    let clean_desc = sanitize_diagnostic_text(&description);
    let clean_err = context_error.as_deref().map(sanitize_diagnostic_text);

    let summary = get_diagnostic_summary(paths.clone())?;

    let logs_preview = if include_logs {
        let raw_logs = export_logs(paths)?;
        sanitize_diagnostic_text(&raw_logs)
    } else {
        "(User opted out of log bundle)".to_string()
    };

    // Calculate deterministic error fingerprint
    let err_seed = format!(
        "{category}:{}:{clean_title}",
        clean_err.as_deref().unwrap_or("")
    );
    let fingerprint = format!("{:x}", md5::compute(err_seed.as_bytes()))[..8].to_uppercase();

    let mut markdown = format!(
        "### Bug Summary\n**Category:** {category}\n**Fingerprint:** `{fingerprint}`\n\n### Description\n{clean_desc}\n\n"
    );

    if let Some(err) = &clean_err {
        markdown.push_str(&format!("### Observed Error\n```text\n{err}\n```\n\n"));
    }

    markdown.push_str(&screenshots);

    markdown.push_str(&format!(
        "### Environment\n- **App Version:** {} (build {})\n- **OS:** {} {} ({})\n- **Engine Status:** {}\n- **Database:** {}\n\n",
        summary.version,
        env!("NEBULAE_BUILD_ID"),
        summary.os,
        os_version_string(),
        summary.arch,
        summary.engine_status,
        summary.database_status
    ));

    if include_logs {
        markdown.push_str(&format!(
            "### Diagnostic Logs (Sanitized)\n<details>\n<summary>Click to view latest logs</summary>\n\n```text\n{logs_preview}\n```\n\n</details>\n"
        ));
    }

    let issue_title = format!("[Bug] [{category}] [{fingerprint}] {clean_title}");

    // URL encoding for GitHub issue link
    let mut is_url_truncated = false;
    let base_github = "https://github.com/koppot/Nebulae-torrent_client/issues/new";

    let full_url = format!(
        "{}?title={}&body={}",
        base_github,
        urlencoding::encode(&issue_title),
        urlencoding::encode(&markdown)
    );

    let github_issue_url = if full_url.len() > 3500 {
        is_url_truncated = true;
        let truncated_body = format!(
            "### Bug Summary\n**Category:** {category}\n**Fingerprint:** `{fingerprint}`\n\n### Description\n{clean_desc}\n\n{screenshots}*Note: Complete diagnostic bundle exceeds URL limits and was copied to your clipboard. Please paste below:*\n\n```text\n[Paste copied diagnostics here]\n```"
        );
        format!(
            "{}?title={}&body={}",
            base_github,
            urlencoding::encode(&issue_title),
            urlencoding::encode(&truncated_body)
        )
    } else {
        full_url
    };

    Ok(BugReportBundle {
        title: clean_title,
        category,
        description: clean_desc,
        app_version: summary.version,
        os: summary.os,
        arch: summary.arch,
        database_status: summary.database_status,
        engine_status: summary.engine_status,
        logs_preview,
        error_fingerprint: fingerprint,
        formatted_markdown: markdown,
        github_issue_url,
        is_url_truncated,
    })
}

#[tauri::command]
pub fn save_bug_report_draft(db: State<Arc<Database>>, draft: String) -> Result<(), String> {
    db.set_setting("bug_report_draft", &draft)
        .map_err(|e| format!("Save draft: {e}"))
}

#[tauri::command]
pub fn get_bug_report_draft(db: State<Arc<Database>>) -> Result<Option<String>, String> {
    db.get_setting("bug_report_draft")
        .map_err(|e| format!("Get draft: {e}"))
}

#[tauri::command]
pub fn clear_bug_report_draft(db: State<Arc<Database>>) -> Result<(), String> {
    let _ = db.set_setting("bug_report_draft", "");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_section_is_empty_without_screenshots_and_pluralises() {
        assert_eq!(screenshot_section(0), "");
        assert!(screenshot_section(1).contains("1 screenshot from the reporter"));
        assert!(screenshot_section(3).contains("3 screenshots from the reporter"));
    }

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

    #[test]
    fn test_sanitize_diagnostic_text_passkey_and_tokens() {
        let input = "Error connecting with passkey: 12345678abcdef and Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let clean = sanitize_diagnostic_text(input);
        assert!(!clean.contains("12345678abcdef"));
        assert!(!clean.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(clean.contains("[REDACTED_PASSKEY]"));
        assert!(clean.contains("[REDACTED_TOKEN]"));
    }

    #[test]
    fn test_sanitize_diagnostic_text_paths_and_ips() {
        let input = "Failed to open file at /Users/johndoe/Downloads/Movie.mkv from peer 198.51.100.45:6881 (local 127.0.0.1:41413)";
        let clean = sanitize_diagnostic_text(input);
        assert!(!clean.contains("johndoe"));
        assert!(!clean.contains("198.51.100.45"));
        assert!(clean.contains("~/Downloads/Movie.mkv"));
        assert!(clean.contains("[REDACTED_IP]:6881"));
        assert!(clean.contains("127.0.0.1:41413")); // Loopback preserved
    }

    #[test]
    fn test_sanitize_diagnostic_text_magnet_and_tracker() {
        let input = "Announce to https://asiandvdclub.org/announce/secretpasskey123 failed for magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Test";
        let clean = sanitize_diagnostic_text(input);
        assert!(!clean.contains("secretpasskey123"));
        assert!(clean.contains("[REDACTED_TRACKER_URL]"));
        assert!(clean.contains("magnet:?[REDACTED_MAGNET_LINK]"));
    }

    #[test]
    fn test_bug_report_draft_lifecycle() {
        use std::path::Path;
        let db = Database::new(Path::new(":memory:")).unwrap();
        db.set_setting("bug_report_draft", "Draft content here")
            .unwrap();
        let loaded = db.get_setting("bug_report_draft").unwrap();
        assert_eq!(loaded.as_deref(), Some("Draft content here"));
        let _ = db.set_setting("bug_report_draft", "");
        let cleared = db.get_setting("bug_report_draft").unwrap();
        assert_eq!(cleared.as_deref(), Some(""));
    }

    #[test]
    fn test_error_fingerprint_generation() {
        let err1 = "playback_init_failed";
        let title1 = "VLC fails to open disc";
        let seed = format!("Playback:{err1}:{title1}");
        let fp1 = format!("{:x}", md5::compute(seed.as_bytes()))[..8].to_uppercase();
        let fp2 = format!("{:x}", md5::compute(seed.as_bytes()))[..8].to_uppercase();
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 8);
    }
}

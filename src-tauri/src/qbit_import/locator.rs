use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QbitLocation {
    pub data_dir: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub source_description: String,
    pub has_sqlite_db: bool,
    pub has_legacy_backup: bool,
}

pub trait LocatorTrait: Send + Sync {
    fn locate(&self) -> Option<QbitLocation>;
    fn validate_dir(&self, dir: &Path) -> Result<QbitLocation, String>;
    fn is_process_running(&self) -> (bool, Option<String>);
    fn check_dir_writable(&self, dir: &Path) -> bool;
}

pub struct DefaultLocator;

impl DefaultLocator {
    pub fn new() -> Self {
        Self
    }

    /// Checks if a candidate directory contains a non-empty BT_backup or a torrents.db
    pub fn inspect_dir(dir: &Path) -> (bool, bool) {
        if !dir.is_dir() {
            return (false, false);
        }

        let sqlite_file = dir.join("torrents.db");
        let has_sqlite = sqlite_file.is_file();

        let bt_backup = dir.join("BT_backup");
        let mut has_legacy = false;
        if bt_backup.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&bt_backup) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                            if ext.eq_ignore_ascii_case("fastresume")
                                || ext.eq_ignore_ascii_case("torrent")
                            {
                                has_legacy = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        (has_sqlite, has_legacy)
    }

    pub fn find_config_dir(data_dir: &Path) -> Option<PathBuf> {
        // 1. Next to data_dir or inside data_dir
        if data_dir.join("qBittorrent.ini").is_file() || data_dir.join("categories.json").is_file()
        {
            return Some(data_dir.to_path_buf());
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(roaming) = dirs::config_dir() {
                let candidate = roaming.join("qBittorrent");
                if candidate.join("qBittorrent.ini").is_file()
                    || candidate.join("categories.json").is_file()
                {
                    return Some(candidate);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(home) = dirs::home_dir() {
                let prefs = home.join("Library/Preferences/qBittorrent");
                if prefs.join("qBittorrent.ini").is_file()
                    || prefs.join("categories.json").is_file()
                {
                    return Some(prefs);
                }
                let config = home.join(".config/qBittorrent");
                if config.join("qBittorrent.ini").is_file()
                    || config.join("categories.json").is_file()
                {
                    return Some(config);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(config) = dirs::config_dir() {
                let candidate = config.join("qBittorrent");
                if candidate.join("qBittorrent.ini").is_file()
                    || candidate.join("categories.json").is_file()
                {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

impl Default for DefaultLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl LocatorTrait for DefaultLocator {
    fn locate(&self) -> Option<QbitLocation> {
        let candidates = get_platform_candidates();
        for (candidate, desc) in candidates {
            debug!("Probing qBittorrent directory candidate: {:?}", candidate);
            let (has_sqlite, has_legacy) = Self::inspect_dir(&candidate);
            if has_sqlite || has_legacy {
                info!(
                    "Discovered qBittorrent directory at {:?} ({desc}). Sqlite: {has_sqlite}, Legacy: {has_legacy}",
                    candidate
                );
                let config_dir = Self::find_config_dir(&candidate);
                return Some(QbitLocation {
                    data_dir: candidate,
                    config_dir,
                    source_description: desc,
                    has_sqlite_db: has_sqlite,
                    has_legacy_backup: has_legacy,
                });
            }
        }
        None
    }

    fn validate_dir(&self, dir: &Path) -> Result<QbitLocation, String> {
        if !dir.exists() {
            return Err(format!("Directory does not exist: {}", dir.display()));
        }
        if !dir.is_dir() {
            return Err(format!("Path is not a directory: {}", dir.display()));
        }

        // Check if user selected BT_backup directly instead of the parent data directory
        let effective_dir = if dir.file_name().and_then(|f| f.to_str()) == Some("BT_backup") {
            dir.parent().unwrap_or(dir).to_path_buf()
        } else {
            dir.to_path_buf()
        };

        let (has_sqlite, has_legacy) = Self::inspect_dir(&effective_dir);
        if !has_sqlite && !has_legacy {
            return Err(format!(
                "The selected directory '{}' does not contain qBittorrent torrents.db or a non-empty BT_backup folder.",
                effective_dir.display()
            ));
        }

        let config_dir = Self::find_config_dir(&effective_dir);
        Ok(QbitLocation {
            data_dir: effective_dir,
            config_dir,
            source_description: "User specified folder".to_string(),
            has_sqlite_db: has_sqlite,
            has_legacy_backup: has_legacy,
        })
    }

    fn is_process_running(&self) -> (bool, Option<String>) {
        check_qbittorrent_running_system()
    }

    fn check_dir_writable(&self, dir: &Path) -> bool {
        check_path_writable(dir)
    }
}

fn get_platform_candidates() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // 1. Process probe for --profile argument
        if let Some(prof) = probe_windows_profile_cmdline() {
            out.push((
                prof.join("qBittorrent").join("data"),
                "Running portable process profile argument".to_string(),
            ));
        }

        // 2. Default LocalAppData (%LOCALAPPDATA%\qBittorrent)
        // Note: dirs::data_local_dir() evaluates to FOLDERID_LocalAppData
        if let Some(local_app_data) = dirs::data_local_dir() {
            out.push((
                local_app_data.join("qBittorrent"),
                "%LOCALAPPDATA%\\qBittorrent (Default Windows data dir)".to_string(),
            ));
        }

        // 3. Roaming AppData (%APPDATA%\qBittorrent)
        if let Some(roaming) = dirs::config_dir() {
            out.push((
                roaming.join("qBittorrent").join("data"),
                "%APPDATA%\\qBittorrent\\data".to_string(),
            ));
            out.push((
                roaming.join("qBittorrent"),
                "%APPDATA%\\qBittorrent".to_string(),
            ));
        }

        // 4. Registry hints for portable mode next to binary
        if let Some(reg_dir) = probe_windows_registry_install_dir() {
            out.push((
                reg_dir.join("profile").join("qBittorrent").join("data"),
                "qBittorrent registry install path (portable profile)".to_string(),
            ));
        }

        // 5. MSIX / Store variants
        if let Some(local_app_data) = dirs::data_local_dir() {
            let packages = local_app_data.join("Packages");
            if packages.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&packages) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.to_lowercase().contains("qbittorrent") {
                            let candidate = entry
                                .path()
                                .join("LocalCache")
                                .join("Local")
                                .join("qBittorrent");
                            out.push((
                                candidate,
                                "Windows Store / MSIX packaged qBittorrent".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            // 1. Default data dir: ~/Library/Application Support/qBittorrent
            out.push((
                home.join("Library/Application Support/qBittorrent"),
                "~/Library/Application Support/qBittorrent (Default macOS data dir)".to_string(),
            ));

            // 2. Probing ~/.config/qBittorrent
            out.push((
                home.join(".config/qBittorrent"),
                "~/.config/qBittorrent".to_string(),
            ));

            // 3. Probing ~/.local/share/qBittorrent and ~/.local/share/data/qBittorrent
            out.push((
                home.join(".local/share/qBittorrent"),
                "~/.local/share/qBittorrent".to_string(),
            ));
            out.push((
                home.join(".local/share/data/qBittorrent"),
                "~/.local/share/data/qBittorrent".to_string(),
            ));
        }

        // 4. Probe running process command line on macOS
        if let Some(prof) = probe_macos_profile_cmdline() {
            out.push((
                prof.join("qBittorrent").join("data"),
                "Running process --profile argument".to_string(),
            ));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(data) = dirs::data_dir() {
            out.push((
                data.join("qBittorrent"),
                "~/.local/share/qBittorrent (Default Linux data dir)".to_string(),
            ));
            out.push((
                data.join("data/qBittorrent"),
                "~/.local/share/data/qBittorrent".to_string(),
            ));
        }
    }

    out
}

#[cfg(target_os = "windows")]
fn probe_windows_profile_cmdline() -> Option<PathBuf> {
    // Best effort via PowerShell CIM instance query; failure is ignored
    use std::process::Command;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_Process -Filter \"Name = 'qbittorrent.exe'\").CommandLine",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let cmd = String::from_utf8_lossy(&output.stdout);
    parse_profile_flag_from_cmd(&cmd)
}

#[cfg(target_os = "windows")]
fn probe_windows_registry_install_dir() -> Option<PathBuf> {
    use std::process::Command;
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\SOFTWARE\qBittorrent",
            "/v",
            "InstallLocation",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("InstallLocation") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let p = parts[2..].join(" ");
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn probe_macos_profile_cmdline() -> Option<PathBuf> {
    use std::process::Command;
    let output = Command::new("ps").args(["-eo", "command"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("qbittorrent") && lower.contains("--profile") {
            return parse_profile_flag_from_cmd(line);
        }
    }
    None
}

fn parse_profile_flag_from_cmd(cmd: &str) -> Option<PathBuf> {
    let flag = "--profile=";
    if let Some(idx) = cmd.find(flag) {
        let rest = &cmd[idx + flag.len()..];
        let token = if rest.starts_with('"') {
            rest.strip_prefix('"')?.split('"').next()?
        } else {
            rest.split_whitespace().next()?
        };
        return Some(PathBuf::from(token));
    }
    None
}

fn check_qbittorrent_running_system() -> (bool, Option<String>) {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq qbittorrent.exe", "/FO", "CSV", "/NH"])
            .output();
        match output {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout);
                if s.to_lowercase().contains("qbittorrent.exe") {
                    return (true, Some("qbittorrent.exe".to_string()));
                }
            }
            Err(e) => {
                tracing::warn!("tasklist command failed: {e}. Treating process state conservatively as running.");
                return (true, Some("Indeterminate (tasklist failed)".to_string()));
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;
        let output = Command::new("pgrep").args(["-i", "qbittorrent"]).output();
        match output {
            Ok(out) if out.status.success() => {
                return (true, Some("qbittorrent".to_string()));
            }
            Ok(_) => {
                // Secondary check via ps in case of custom executable names
                if let Ok(ps_out) = Command::new("ps").args(["-ax", "-c"]).output() {
                    let s = String::from_utf8_lossy(&ps_out.stdout);
                    for line in s.lines() {
                        let lower = line.to_lowercase();
                        if lower.contains("qbittorrent") {
                            return (true, Some("qbittorrent".to_string()));
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("pgrep failed: {e}. Falling back to ps probe.");
                if let Ok(ps_out) = Command::new("ps").args(["-ax", "-c"]).output() {
                    let s = String::from_utf8_lossy(&ps_out.stdout);
                    for line in s.lines() {
                        let lower = line.to_lowercase();
                        if lower.contains("qbittorrent") {
                            return (true, Some("qbittorrent".to_string()));
                        }
                    }
                } else {
                    tracing::warn!(
                        "ps command also failed. Treating process state conservatively as running."
                    );
                    return (
                        true,
                        Some("Indeterminate (process probe failed)".to_string()),
                    );
                }
            }
        }
    }

    (false, None)
}

fn check_path_writable(path: &Path) -> bool {
    let probe_dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };

    let test_file = probe_dir.join(format!(".nebulae_write_test_{}", std::process::id()));
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            true
        }
        Err(e) => {
            debug!("Path {:?} is not writable: {}", probe_dir, e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_profile_flag_works() {
        assert_eq!(
            parse_profile_flag_from_cmd("qbittorrent.exe --profile=D:\\custom_prof --port=8080"),
            Some(PathBuf::from("D:\\custom_prof"))
        );
        assert_eq!(
            parse_profile_flag_from_cmd("qbittorrent --profile=\"/Users/dev/qbit prof\""),
            Some(PathBuf::from("/Users/dev/qbit prof"))
        );
        assert_eq!(parse_profile_flag_from_cmd("qbittorrent --port=8080"), None);
    }
}

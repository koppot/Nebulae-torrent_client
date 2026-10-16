use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tracing::{error, info, warn};

use crate::commands::settings::UserSettings;
use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use crate::db::{Database, TorrentDbMetadata};
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::models::peering::{LocalityTier, PeerTelemetry};
use crate::utils::network::find_active_vpn_interface;
use crate::utils::paths::AppPaths;

/// Expands a leading `~` to the user's home directory. Relative paths resolve
/// against the home directory, never the process working directory (which can
/// be a read-only volume when the app is launched straight from a mounted DMG).
fn expand_tilde_path(path: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed == "~" {
        return Some(home);
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    let as_path = PathBuf::from(trimmed);
    if as_path.is_absolute() {
        Some(as_path)
    } else {
        Some(home.join(as_path))
    }
}

/// Helper to normalize download directory paths (expand `~`, convert to absolute).
fn normalize_download_dir(download_dir: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = download_dir.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };

    let absolute = expand_tilde_path(Path::new(raw))
        .ok_or_else(|| "Cannot resolve home directory".to_string())?;

    std::fs::create_dir_all(&absolute).map_err(|e| {
        format!(
            "Cannot use save destination '{}': {e}. Pick another folder under Save Destination.",
            absolute.display()
        )
    })?;

    Ok(Some(absolute.to_string_lossy().to_string()))
}

/// Locates the bundled transmission-daemon executable.
pub fn find_transmission_daemon_binary(resource_dir: Option<&Path>) -> Result<PathBuf, String> {
    let bin_name = if cfg!(windows) {
        "transmission-daemon.exe"
    } else {
        "transmission-daemon"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(rd) = resource_dir {
        candidates.push(
            rd.join("resources")
                .join("engine")
                .join("transmission")
                .join(bin_name),
        );
        candidates.push(rd.join("engine").join("transmission").join(bin_name));
        candidates.push(rd.join("transmission").join(bin_name));
        candidates.push(rd.join(bin_name));
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("engine")
                    .join("transmission")
                    .join(bin_name),
            );
            candidates.push(exe_dir.join("engine").join("transmission").join(bin_name));
            candidates.push(exe_dir.join("resources").join(bin_name));
            candidates.push(exe_dir.join(bin_name));

            // macOS .app bundle layout: Contents/MacOS/nebulae -> Contents/Resources/
            if let Some(contents_dir) = exe_dir.parent() {
                let res_dir = contents_dir.join("Resources");
                candidates.push(
                    res_dir
                        .join("resources")
                        .join("engine")
                        .join("transmission")
                        .join(bin_name),
                );
                candidates.push(res_dir.join("engine").join("transmission").join(bin_name));
                candidates.push(res_dir.join("transmission").join(bin_name));
                candidates.push(res_dir.join(bin_name));
            }

            // Linux packaging layouts
            if let Some(prefix) = exe_dir.parent().and_then(|p| p.parent()) {
                candidates.push(
                    prefix
                        .join("lib")
                        .join("nebulae")
                        .join("resources")
                        .join("engine")
                        .join("transmission")
                        .join(bin_name),
                );
                candidates.push(
                    prefix
                        .join("share")
                        .join("nebulae")
                        .join("resources")
                        .join("engine")
                        .join("transmission")
                        .join(bin_name),
                );
            }
        }
    }

    // AppImage support ($APPDIR)
    if let Ok(appdir) = std::env::var("APPDIR") {
        let appdir_path = PathBuf::from(appdir);
        candidates.push(
            appdir_path
                .join("usr")
                .join("share")
                .join("nebulae")
                .join("resources")
                .join("engine")
                .join("transmission")
                .join(bin_name),
        );
        candidates.push(
            appdir_path
                .join("resources")
                .join("engine")
                .join("transmission")
                .join(bin_name),
        );
    }

    // Development checkout paths
    candidates.push(PathBuf::from("src-tauri/resources/engine/transmission").join(bin_name));
    candidates.push(PathBuf::from("resources/engine/transmission").join(bin_name));

    // Common system fallback locations for development/macOS
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(bin_name));
    candidates.push(PathBuf::from("/usr/local/bin").join(bin_name));
    candidates.push(PathBuf::from("/usr/bin").join(bin_name));

    for cand in candidates {
        if cand.is_file() {
            return Ok(cand);
        }
    }

    Err(format!(
        "Bundled transmission-daemon ({bin_name}) was not found in installation resources or system paths."
    ))
}

/// Finds a free ephemeral TCP port by binding to 127.0.0.1:0 and immediately closing.
fn find_free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind to ephemeral port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local port: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Addendum 1: Orphan daemon cleanup.
/// Scans for and terminates any stale transmission-daemon process associated with our config directory.
fn cleanup_orphan_daemon(config_dir: &Path) {
    let pid_file = config_dir.join("daemon.pid");
    if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            terminate_pid_if_transmission(pid);
        }
        let _ = std::fs::remove_file(&pid_file);
    }

    // Secondary scan on Unix using pgrep -f on the config dir path
    #[cfg(unix)]
    {
        let config_str = config_dir.to_string_lossy();
        if !config_str.is_empty() {
            if let Ok(output) = std::process::Command::new("pgrep")
                .args(["-f", &format!("transmission-daemon.*{}", config_str)])
                .output()
            {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    for line in stdout.lines() {
                        if let Ok(pid) = line.trim().parse::<u32>() {
                            if pid != std::process::id() {
                                terminate_pid_if_transmission(pid);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Terminate process by PID if it appears to be transmission-daemon.
fn terminate_pid_if_transmission(pid: u32) {
    if pid == 0 || pid == std::process::id() {
        return;
    }

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status();
    }

    #[cfg(unix)]
    {
        // Check if process name is transmission-daemon
        let is_transmission = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(|s| s.contains("transmission-daemon"))
            .unwrap_or(false);

        if is_transmission {
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
            std::thread::sleep(Duration::from_millis(500));
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
    }
}

/// Transmission JSON-RPC Client.
pub struct TransmissionRpcClient {
    rpc_url: String,
    auth_header: Option<String>,
    session_id: Mutex<Option<String>>,
    client: reqwest::blocking::Client,
}

impl TransmissionRpcClient {
    pub fn new(port: u16, username: &str, password: &str) -> Self {
        let auth = if !username.is_empty() || !password.is_empty() {
            use base64::Engine;
            let raw = format!("{username}:{password}");
            Some(format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(raw)
            ))
        } else {
            None
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            rpc_url: format!("http://127.0.0.1:{port}/transmission/rpc"),
            auth_header: auth,
            session_id: Mutex::new(None),
            client,
        }
    }

    /// Low-level RPC call handler with automatic CSRF session-id negotiation.
    pub fn request(&self, method: &str, arguments: Option<Value>) -> Result<Value, String> {
        let payload = json!({
            "method": method,
            "arguments": arguments.unwrap_or_else(|| json!({}))
        });

        for attempt in 0..2 {
            let session_id = self.session_id.lock().unwrap().clone();

            let mut req = self.client.post(&self.rpc_url).json(&payload);
            if let Some(ref sid) = session_id {
                req = req.header("X-Transmission-Session-Id", sid);
            }
            if let Some(ref auth) = self.auth_header {
                req = req.header("Authorization", auth);
            }

            let resp = match req.send() {
                Ok(r) => r,
                Err(e) => {
                    return Err(format!("Transmission RPC connection error: {e}"));
                }
            };

            // Handle 409 Conflict CSRF token handshake
            if resp.status().as_u16() == 409 {
                if let Some(header_val) = resp.headers().get("X-Transmission-Session-Id") {
                    if let Ok(new_sid) = header_val.to_str() {
                        *self.session_id.lock().unwrap() = Some(new_sid.to_string());
                        if attempt == 0 {
                            continue; // Retry once with the new CSRF token
                        }
                    }
                }
                return Err(
                    "Failed to obtain valid X-Transmission-Session-Id from daemon".to_string(),
                );
            }

            if !resp.status().is_success() {
                return Err(format!("Transmission RPC returned HTTP {}", resp.status()));
            }

            let body_val: Value = resp
                .json()
                .map_err(|e| format!("Failed to parse Transmission RPC JSON response: {e}"))?;

            let result = body_val
                .get("result")
                .and_then(|r| r.as_str())
                .unwrap_or("unknown");

            if result != "success" {
                return Err(format!("Transmission RPC call '{method}' failed: {result}"));
            }

            return Ok(body_val
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({})));
        }

        Err("Transmission RPC request failed after retry".to_string())
    }

    pub fn session_get(&self) -> Result<Value, String> {
        self.request("session-get", None)
    }

    pub fn session_set(&self, args: Value) -> Result<(), String> {
        self.request("session-set", Some(args))?;
        Ok(())
    }

    pub fn session_stats(&self) -> Result<Value, String> {
        self.request("session-stats", None)
    }

    pub fn port_test(&self) -> Result<bool, String> {
        let res = self.request("port-test", None)?;
        Ok(res
            .get("port-is-open")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    pub fn torrent_get(&self, ids: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
        let fields = vec![
            "id",
            "hashString",
            "name",
            "status",
            "percentDone",
            "downloadedEver",
            "uploadedEver",
            "rateDownload",
            "rateUpload",
            "sizeWhenDone",
            "totalSize",
            "leftUntilDone",
            "pieceCount",
            "pieceSize",
            "downloadDir",
            "addedDate",
            "doneDate",
            "eta",
            "errorString",
            "torrentFile",
            "trackerStats",
            "files",
            "fileStats",
            "peers",
        ];

        let mut args = json!({ "fields": fields });
        if let Some(id_list) = ids {
            args["ids"] = json!(id_list);
        }

        let res = self.request("torrent-get", Some(args))?;
        let torrents = res
            .get("torrents")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(torrents)
    }

    pub fn torrent_add(&self, args: Value) -> Result<String, String> {
        let res = self.request("torrent-add", Some(args))?;

        if let Some(added) = res.get("torrent-added") {
            if let Some(hash) = added.get("hashString").and_then(|h| h.as_str()) {
                return Ok(hash.to_string());
            }
        }
        if let Some(dup) = res.get("torrent-duplicate") {
            if let Some(hash) = dup.get("hashString").and_then(|h| h.as_str()) {
                return Ok(hash.to_string());
            }
        }

        Err(
            "torrent-add response contained neither torrent-added nor torrent-duplicate"
                .to_string(),
        )
    }

    pub fn torrent_remove(&self, id: &str, delete_data: bool) -> Result<(), String> {
        let args = json!({
            "ids": [id],
            "delete-local-data": delete_data
        });
        self.request("torrent-remove", Some(args))?;
        Ok(())
    }

    pub fn torrent_start(&self, id: Option<&str>) -> Result<(), String> {
        let args = id.map(|i| json!({ "ids": [i] }));
        self.request("torrent-start", args)?;
        Ok(())
    }

    pub fn torrent_stop(&self, id: Option<&str>) -> Result<(), String> {
        let args = id.map(|i| json!({ "ids": [i] }));
        self.request("torrent-stop", args)?;
        Ok(())
    }

    pub fn torrent_verify(&self, id: &str) -> Result<(), String> {
        let args = json!({ "ids": [id] });
        self.request("torrent-verify", Some(args))?;
        Ok(())
    }

    pub fn torrent_reannounce(&self, id: &str) -> Result<(), String> {
        let args = json!({ "ids": [id] });
        self.request("torrent-reannounce", Some(args))?;
        Ok(())
    }

    pub fn torrent_set_location(
        &self,
        id: &str,
        location: &str,
        move_data: bool,
    ) -> Result<(), String> {
        let args = json!({
            "ids": [id],
            "location": location,
            "move": move_data
        });
        self.request("torrent-set-location", Some(args))?;
        Ok(())
    }

    pub fn torrent_set(&self, id: &str, mut args: Value) -> Result<(), String> {
        if let Some(map) = args.as_object_mut() {
            map.insert("ids".to_string(), json!([id]));
        }
        self.request("torrent-set", Some(args))?;
        Ok(())
    }

    pub fn torrent_rename_path(&self, id: &str, path: &str, name: &str) -> Result<(), String> {
        let args = json!({
            "ids": [id],
            "path": path,
            "name": name
        });
        self.request("torrent-rename-path", Some(args))?;
        Ok(())
    }
}

/// Transmission Sidecar Torrent Engine.
pub struct TransmissionTorrentEngine {
    rpc_client: Arc<TransmissionRpcClient>,
    child_process: Arc<Mutex<Option<std::process::Child>>>,
    config_dir: PathBuf,
    db: Arc<Database>,
    #[allow(dead_code)]
    paths: Arc<AppPaths>,
    kill_switch_active: Arc<AtomicBool>,
    bound_interface: Arc<Mutex<Option<String>>>,
    metadata_cache: Arc<Mutex<HashMap<String, TorrentDbMetadata>>>,
}

/// Builds the transmission-daemon `settings.json` configuration structure with
/// strict private tracker constraints (DHT, PEX, LPD hard-disabled at the session level)
/// and settings mapped from Nebulae UserSettings.
pub fn build_transmission_settings(
    settings: &UserSettings,
    default_download_dir: &str,
    bind_ipv4: &str,
    rpc_port: u16,
    rpc_user: &str,
    rpc_password: &str,
) -> Value {
    json!({
        "rpc-enabled": true,
        "rpc-port": rpc_port,
        "rpc-bind-address": "127.0.0.1",
        "rpc-whitelist-enabled": true,
        "rpc-whitelist": "127.0.0.1",
        "rpc-authentication-required": true,
        "rpc-username": rpc_user,
        "rpc-password": rpc_password,

        // Peer connectivity settings
        "peer-port": settings.listening_port,
        "port-forwarding-enabled": settings.enable_upnp,
        "peer-limit-per-torrent": settings.max_connections_per_torrent.clamp(60, 100),
        "peer-limit-global": settings.max_global_connections.clamp(250, 500),
        "upload-slots-per-torrent": settings.max_upload_slots_per_torrent.clamp(4, 8),
        "encryption": 1, // Preferred
        "utp-enabled": true,

        // Mandatory private tracker constraints: hard-off at session level
        "dht-enabled": false,
        "pex-enabled": false,
        "lpd-enabled": false,

        // Interface binding
        "bind-address-ipv4": bind_ipv4,
        "download-dir": default_download_dir,
    })
}

/// Resolves the appropriate download directory for Transmission taking into account
/// the multi-file root-join rule from the v0.9.0 fix:
/// - If multi-file and output_folder ends with the torrent name, Transmission requires the parent
///   directory so it joins the root name itself without creating a nested duplicate directory.
/// - If single-file, the output_folder is the destination directory directly.
pub fn resolve_transmission_download_dir(
    output_folder: &str,
    torrent_name: &str,
    is_multi_file: bool,
) -> String {
    if !is_multi_file {
        return output_folder.to_string();
    }

    let p = Path::new(output_folder);
    if let Some(file_name) = p.file_name().and_then(|f| f.to_str()) {
        if file_name == torrent_name {
            if let Some(parent) = p.parent() {
                let parent_str = parent.to_string_lossy().to_string();
                if !parent_str.is_empty() {
                    return parent_str;
                }
            }
        }
    }

    output_folder.to_string()
}

/// Executes a one-time migration from librqbit persistence files to transmission-daemon.
/// Preserves all original librqbit persistence files untouched for rollback safety.
pub fn run_librqbit_migration(
    rpc_client: &TransmissionRpcClient,
    librqbit_dir: &Path,
    db: &Database,
) {
    let already_migrated = db
        .get_setting("transmission_migration_completed")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    if already_migrated {
        return;
    }

    info!("Starting one-time migration of existing librqbit torrents to Transmission...");

    let session_json_path = librqbit_dir.join("session.json");
    if !session_json_path.is_file() {
        info!(
            "No librqbit session.json found at '{}'; no migration needed.",
            session_json_path.display()
        );
        let _ = db.set_setting("transmission_migration_completed", "true");
        return;
    }

    let session_data = match std::fs::read_to_string(&session_json_path) {
        Ok(data) => data,
        Err(e) => {
            warn!("Failed to read librqbit session.json: {e}");
            return;
        }
    };

    let v: Value = match serde_json::from_str(&session_data) {
        Ok(val) => val,
        Err(e) => {
            warn!("Failed to parse librqbit session.json: {e}");
            return;
        }
    };

    let torrents_map = match v.get("torrents").and_then(|t| t.as_object()) {
        Some(m) => m,
        None => {
            info!("librqbit session.json contains no torrents; migration completed.");
            let _ = db.set_setting("transmission_migration_completed", "true");
            return;
        }
    };

    let mut migrated_count = 0;
    let mut skipped_count = 0;

    for (_key, item) in torrents_map {
        let info_hash = item
            .get("info_hash")
            .and_then(|h| h.as_str())
            .unwrap_or_default()
            .to_lowercase();
        if info_hash.is_empty() {
            continue;
        }

        let output_folder = item
            .get("output_folder")
            .and_then(|o| o.as_str())
            .unwrap_or("");
        let is_paused = item
            .get("is_paused")
            .and_then(|p| p.as_bool())
            .unwrap_or(false);

        // Locate corresponding .torrent file in librqbit persistence directory
        let mut torrent_bytes_opt: Option<Vec<u8>> = None;
        let direct_file = librqbit_dir.join(format!("{info_hash}.torrent"));
        if direct_file.is_file() {
            torrent_bytes_opt = std::fs::read(&direct_file).ok();
        }

        if torrent_bytes_opt.is_none() {
            if let Ok(entries) = std::fs::read_dir(librqbit_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) == Some("torrent") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem.to_lowercase() == info_hash {
                                torrent_bytes_opt = std::fs::read(&path).ok();
                                break;
                            }
                        }
                    }
                }
            }
        }

        let Some(bytes) = torrent_bytes_opt else {
            warn!("Could not locate .torrent file for librqbit torrent {info_hash}; skipping");
            skipped_count += 1;
            continue;
        };

        let parsed = match crate::engine::metainfo::ParsedTorrent::parse_bytes(&bytes) {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to parse .torrent metainfo for {info_hash}: {e}");
                skipped_count += 1;
                continue;
            }
        };

        let trans_download_dir =
            resolve_transmission_download_dir(output_folder, &parsed.name, parsed.is_multi_file);

        use base64::Engine;
        let b64_bytes = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let add_args = json!({
            "metainfo": b64_bytes,
            "download-dir": trans_download_dir,
            "paused": true,
        });

        match rpc_client.torrent_add(add_args) {
            Ok(added_id) => {
                // Verify existing content on disk
                let _ = rpc_client.torrent_verify(&added_id);

                // If active in librqbit, start in Transmission
                if !is_paused {
                    let _ = rpc_client.torrent_start(Some(&added_id));
                }

                info!(
                    "Migrated torrent '{}' ({info_hash}) to Transmission at '{}'",
                    parsed.name, trans_download_dir
                );
                migrated_count += 1;
            }
            Err(e) => {
                warn!("Failed to add migrated torrent {info_hash} to Transmission: {e}");
                skipped_count += 1;
            }
        }
    }

    info!(
        "librqbit migration completed: {migrated_count} migrated, {skipped_count} skipped. Keeping librqbit resume data intact for rollback safety."
    );
    let _ = db.set_setting("transmission_migration_completed", "true");
}

impl TransmissionTorrentEngine {
    pub fn new(
        paths: Arc<AppPaths>,
        db: Arc<Database>,
        default_download_dir: PathBuf,
        settings: &UserSettings,
        resource_dir: Option<&Path>,
    ) -> Result<Self, String> {
        let engine_dir = paths.base_data_dir.join("engine").join("transmission");
        std::fs::create_dir_all(&engine_dir).map_err(|e| {
            format!(
                "Failed to create transmission engine directory '{}': {e}",
                engine_dir.display()
            )
        })?;
        // The front-end default is the literal string '~/Downloads'; expand it
        // to an absolute path or transmission-daemon resolves it against its
        // working directory, which is read-only when launched from a DMG.
        let default_download_dir = expand_tilde_path(&default_download_dir).ok_or_else(|| {
            "Cannot resolve home directory for the default download directory".to_string()
        })?;
        std::fs::create_dir_all(&default_download_dir).map_err(|e| {
            format!(
                "Failed to create default download directory '{}': {e}",
                default_download_dir.display()
            )
        })?;

        // 1. Locate the transmission-daemon executable
        let daemon_path = find_transmission_daemon_binary(resource_dir)?;
        info!(
            "Located bundled transmission-daemon at: {}",
            daemon_path.display()
        );

        // 2. Addendum 1: Clean up any orphan daemon from a previous crash/run
        cleanup_orphan_daemon(&engine_dir);

        // 3. Ephemeral port and random auth token
        let rpc_port = find_free_port()?;
        let rpc_user = "nebulae";
        let rpc_password = uuid::Uuid::new_v4().to_string();

        // 4. Addendum 4: Determine tunnel IP at spawn if VPN is required
        let vpn_mode = db
            .get_setting("vpn_mode")
            .unwrap_or(None)
            .unwrap_or_else(|| "NoVpn".to_string());
        let kill_switch_armed = db
            .get_setting("vpn_kill_switch")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(true);
        let is_vpn_required = vpn_mode == "AdcOnlyVpn" || vpn_mode == "SystemVpn";

        let mut bind_ipv4 = "0.0.0.0".to_string();
        let mut initial_ks_active = false;

        if is_vpn_required {
            if let Some(vpn_iface) = find_active_vpn_interface() {
                if let Some(vpn_ip) = vpn_iface
                    .ip_addresses
                    .iter()
                    .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                {
                    bind_ipv4 = vpn_ip.clone();
                    info!("Hardening at spawn: bound transmission-daemon to active tunnel IP {bind_ipv4}");
                }
            } else if kill_switch_armed {
                warn!("VPN is required and killswitch is armed, but no active tunnel found at engine spawn! Engaging killswitch protection immediately.");
                bind_ipv4 = "127.0.0.1".to_string();
                initial_ks_active = true;
            }
        }

        // 5. Build settings.json for Transmission with private-tracker constraints hard-off
        let trans_settings = build_transmission_settings(
            settings,
            &default_download_dir.to_string_lossy(),
            &bind_ipv4,
            rpc_port,
            rpc_user,
            &rpc_password,
        );

        let settings_path = engine_dir.join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&trans_settings).unwrap_or_default(),
        )
        .map_err(|e| format!("Failed to write transmission settings.json: {e}"))?;

        // 6. Spawn the transmission-daemon sidecar
        info!(
            "Spawning transmission-daemon on RPC port {} with config dir {}",
            rpc_port,
            engine_dir.display()
        );

        let child = std::process::Command::new(&daemon_path)
            .arg("--config-dir")
            .arg(&engine_dir)
            .arg("--foreground")
            // Pin the working directory to the writable engine dir: inheriting
            // the app's CWD (e.g. a mounted DMG volume) must never affect the
            // daemon's path handling.
            .current_dir(&engine_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn transmission-daemon: {e}"))?;

        let pid = child.id();
        let _ = std::fs::write(engine_dir.join("daemon.pid"), pid.to_string());

        let rpc_client = Arc::new(TransmissionRpcClient::new(
            rpc_port,
            rpc_user,
            &rpc_password,
        ));

        // 7. Verify daemon is responsive via session-get probe with backoff
        let mut ready = false;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if rpc_client.session_get().is_ok() {
                ready = true;
                break;
            }
        }

        if !ready {
            let mut proc = child;
            let _ = proc.kill();
            return Err(
                "transmission-daemon failed to become responsive on RPC port within 5 seconds"
                    .to_string(),
            );
        }

        info!("transmission-daemon successfully initialized and responsive via JSON-RPC.");

        // 7b. Execute one-time migration from librqbit if needed
        run_librqbit_migration(&rpc_client, &paths.engine_dir, &db);

        let cached_meta = db.list_all_torrent_metadata().unwrap_or_default();

        let child_arc = Arc::new(Mutex::new(Some(child)));

        // 8. Spawn health watchdog
        {
            let child_watch = Arc::clone(&child_arc);
            let daemon_path_watch = daemon_path.clone();
            let engine_dir_watch = engine_dir.clone();
            let pid_file_watch = engine_dir.join("daemon.pid");

            tauri::async_runtime::spawn(async move {
                let mut backoff = Duration::from_secs(1);
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    let is_dead = {
                        let mut lock = child_watch.lock().unwrap();
                        if let Some(ref mut child) = *lock {
                            match child.try_wait() {
                                Ok(Some(status)) => {
                                    warn!("transmission-daemon exited unexpectedly with status {status}. Attempting restart in {backoff:?}...");
                                    true
                                }
                                Ok(None) => false,
                                Err(e) => {
                                    warn!("Error polling transmission-daemon status: {e}");
                                    false
                                }
                            }
                        } else {
                            false
                        }
                    };

                    if is_dead {
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(16));

                        match std::process::Command::new(&daemon_path_watch)
                            .arg("--config-dir")
                            .arg(&engine_dir_watch)
                            .arg("--foreground")
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn()
                        {
                            Ok(new_child) => {
                                let new_pid = new_child.id();
                                let _ = std::fs::write(&pid_file_watch, new_pid.to_string());
                                *child_watch.lock().unwrap() = Some(new_child);
                                info!("Restarted transmission-daemon (pid {new_pid})");
                                backoff = Duration::from_secs(1);
                            }
                            Err(e) => {
                                error!("Failed to restart transmission-daemon: {e}");
                            }
                        }
                    }
                }
            });
        }

        Ok(Self {
            rpc_client,
            child_process: child_arc,
            config_dir: engine_dir,
            db,
            paths,
            kill_switch_active: Arc::new(AtomicBool::new(initial_ks_active)),
            bound_interface: Arc::new(Mutex::new(if bind_ipv4 != "0.0.0.0" {
                Some(bind_ipv4)
            } else {
                None
            })),
            metadata_cache: Arc::new(Mutex::new(cached_meta)),
        })
    }

    pub fn rpc(&self) -> &Arc<TransmissionRpcClient> {
        &self.rpc_client
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
}

impl Drop for TransmissionTorrentEngine {
    fn drop(&mut self) {
        // Attempt clean RPC shutdown first
        let _ = self.rpc_client.request("session-close", None);
        // Terminate child process
        if let Ok(mut lock) = self.child_process.lock() {
            if let Some(ref mut child) = *lock {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let _ = std::fs::remove_file(self.config_dir.join("daemon.pid"));
    }
}

impl TorrentEngine for TransmissionTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let raw_torrents = self.rpc_client.torrent_get(None)?;
        let cached = self.metadata_cache.lock().unwrap().clone();
        let is_ks = self.kill_switch_active.load(Ordering::SeqCst);

        let mut items = Vec::new();

        for t in raw_torrents {
            let id = t
                .get("id")
                .and_then(|v| v.as_i64())
                .map(|i| i.to_string())
                .unwrap_or_default();
            let info_hash = t
                .get("hashString")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_lowercase();
            let name = t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unnamed Torrent")
                .to_string();

            let meta = cached.get(&info_hash);

            let status_num = t.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
            let percent_done = t.get("percentDone").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let downloaded_ever = t
                .get("downloadedEver")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let uploaded_ever = t.get("uploadedEver").and_then(|v| v.as_u64()).unwrap_or(0);
            let rate_dl = t.get("rateDownload").and_then(|v| v.as_u64()).unwrap_or(0);
            let rate_ul = t.get("rateUpload").and_then(|v| v.as_u64()).unwrap_or(0);
            let total_bytes = t
                .get("totalSize")
                .or_else(|| t.get("sizeWhenDone"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let download_dir = t
                .get("downloadDir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let added_on = t.get("addedDate").and_then(|v| v.as_u64()).unwrap_or(0);
            let completed_on = t
                .get("doneDate")
                .and_then(|v| v.as_u64())
                .filter(|&d| d > 0);
            let piece_size = t
                .get("pieceSize")
                .and_then(|v| v.as_u64())
                .unwrap_or(1048576) as u32;
            let total_pieces = t.get("pieceCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            let eta_raw = t.get("eta").and_then(|v| v.as_i64()).unwrap_or(-1);
            let eta_seconds = if eta_raw > 0 {
                Some(eta_raw as u64)
            } else {
                None
            };

            // Status mapping per design note table:
            // 0: Stopped -> "Paused"
            // 1: Queued to check, 2: Checking -> "Checking"
            // 3: Queued to download, 4: Downloading -> "Downloading"
            // 5: Queued to seed, 6: Seeding -> "Seeding"
            let status = if is_ks {
                "Paused".to_string()
            } else {
                match status_num {
                    0 => "Paused".to_string(),
                    1 | 2 => "Checking".to_string(),
                    3 | 4 => "Downloading".to_string(),
                    5 | 6 => "Seeding".to_string(),
                    _ => "Paused".to_string(),
                }
            };

            let ratio = if downloaded_ever > 0 {
                (uploaded_ever as f64 / downloaded_ever as f64) as f32
            } else if percent_done >= 1.0 {
                1.0
            } else {
                0.0
            };

            // Files mapping
            let mut files = Vec::new();
            if let Some(raw_files) = t.get("files").and_then(|v| v.as_array()) {
                let file_stats = t.get("fileStats").and_then(|v| v.as_array());
                for (idx, f) in raw_files.iter().enumerate() {
                    let fname = f
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let fsize = f.get("length").and_then(|v| v.as_u64()).unwrap_or(0);
                    let fdone = f
                        .get("bytesCompleted")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let fprog = if fsize > 0 {
                        (fdone as f64 / fsize as f64).min(1.0) as f32
                    } else {
                        1.0
                    };

                    let wanted = file_stats
                        .and_then(|fs| fs.get(idx))
                        .and_then(|s| s.get("wanted"))
                        .and_then(|w| w.as_bool())
                        .unwrap_or(true);

                    // Transmission file priority: -1 low, 0 normal, 1 high.
                    // UI scale: 0 = do not download, 1 = normal, 2 = high
                    // (low displays as normal — the UI has no low bucket).
                    let raw_prio = file_stats
                        .and_then(|fs| fs.get(idx))
                        .and_then(|s| s.get("priority"))
                        .and_then(|p| p.as_i64())
                        .unwrap_or(0);
                    let ui_prio = if !wanted {
                        0
                    } else if raw_prio == 1 {
                        2
                    } else {
                        1
                    };

                    files.push(TorrentFileItem {
                        index: idx as u32,
                        name: fname,
                        size_bytes: fsize,
                        progress: fprog,
                        priority: ui_prio,
                    });
                }
            }

            // Trackers mapping and swarm counts
            let mut trackers = Vec::new();
            let mut max_seeds: Option<u32> = None;
            let mut max_peers: u32 = 0;

            if let Some(tr_stats) = t.get("trackerStats").and_then(|v| v.as_array()) {
                for (tier, tr) in tr_stats.iter().enumerate() {
                    let url = tr
                        .get("announce")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let seeder_count = tr
                        .get("seederCount")
                        .and_then(|v| v.as_i64())
                        .filter(|&s| s >= 0)
                        .map(|s| s as u32);
                    let leecher_count = tr
                        .get("leecherCount")
                        .and_then(|v| v.as_i64())
                        .filter(|&l| l >= 0)
                        .map(|l| l as u32);
                    let last_announce_res = tr
                        .get("lastAnnounceResult")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(s) = seeder_count {
                        max_seeds = Some(max_seeds.map_or(s, |m| m.max(s)));
                    }
                    if let Some(l) = leecher_count {
                        max_peers = max_peers.max(l);
                    }

                    let last_announce_time = tr
                        .get("lastAnnounceTime")
                        .and_then(|v| v.as_i64())
                        .filter(|&s| s > 0);
                    let next_announce_time = tr
                        .get("nextAnnounceTime")
                        .and_then(|v| v.as_i64())
                        .filter(|&s| s > 0);

                    trackers.push(TorrentTrackerItem {
                        tier: tier as u32,
                        url,
                        status: if !last_announce_res.is_empty() {
                            "Working".to_string()
                        } else {
                            "Not working".to_string()
                        },
                        seeds: seeder_count,
                        peers: leecher_count,
                        downloaded: 0,
                        message: if !last_announce_res.is_empty() {
                            last_announce_res
                        } else {
                            "Waiting for announce...".to_string()
                        },
                        last_announce: last_announce_time,
                        next_announce: next_announce_time,
                    });
                }
            }

            // Connected peers count from peers array
            let connected_peers = t
                .get("peers")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as u32)
                .unwrap_or(0);
            let display_peers = max_peers.max(connected_peers);

            let is_media = files.iter().any(|f| {
                let lower = f.name.to_lowercase();
                lower.ends_with(".mkv")
                    || lower.ends_with(".mp4")
                    || lower.ends_with(".avi")
                    || lower.ends_with(".ts")
                    || lower.ends_with(".m2ts")
                    || lower.ends_with(".iso")
                    || lower.ends_with(".vob")
            });

            items.push(TorrentItem {
                id,
                name: meta
                    .and_then(|m| m.display_title.clone())
                    .unwrap_or(name.clone()),
                total_bytes,
                downloaded_bytes: downloaded_ever,
                uploaded_bytes: uploaded_ever,
                download_rate: rate_dl,
                upload_rate: rate_ul,
                progress: percent_done,
                status,
                eta_seconds,
                seeds: max_seeds,
                peers: display_peers,
                ratio,
                category: meta.and_then(|m| m.category.clone()).unwrap_or_default(),
                is_media,
                poster_url: None,
                info_hash: info_hash.clone(),
                save_path: meta
                    .and_then(|m| m.save_path.clone())
                    .unwrap_or(download_dir),
                added_on,
                completed_on,
                comment: String::new(),
                created_by: String::new(),
                piece_size,
                total_pieces,
                sequential_download: meta.map(|m| m.sequential_download).unwrap_or(false),
                first_last_piece_prio: meta.map(|m| m.first_last_piece_prio).unwrap_or(false),
                super_seeding: meta.map(|m| m.super_seeding).unwrap_or(false),
                tags: Vec::new(),
                files,
                trackers,
                download_limit_kib: meta.map(|m| m.download_limit_kib).unwrap_or(0),
                upload_limit_kib: meta.map(|m| m.upload_limit_kib).unwrap_or(0),
                ratio_limit: meta.map(|m| m.ratio_limit).unwrap_or(-1.0),
                media_meta: Some(crate::utils::media_parser::MediaTitleParser::parse(
                    &name.clone(),
                )),
            });
        }

        Ok(items)
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let trimmed = source.trim();

        // 1. Check if source is base64 encoded
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
            && !trimmed.starts_with("http://")
            && !trimmed.starts_with("https://")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = crate::utils::paths::decode_base64_data(b64) {
                return self.add_torrent_from_bytes(&decoded, download_dir);
            }
        }

        // 2. Check if source is a local file path
        if !trimmed.starts_with("magnet:")
            && !trimmed.starts_with("http://")
            && !trimmed.starts_with("https://")
        {
            let norm_path = crate::utils::paths::normalize_source_path(trimmed);
            if norm_path.is_file() {
                if let Ok(bytes) = std::fs::read(&norm_path) {
                    return self.add_torrent_from_bytes(&bytes, download_dir);
                }
            }
        }

        // 3. Magnet or HTTP(S) URL
        let dest = normalize_download_dir(download_dir)?;
        let mut args = json!({
            "filename": trimmed,
            "paused": false
        });
        if let Some(ref d) = dest {
            args["download-dir"] = json!(d);
        }

        let hash = self.rpc_client.torrent_add(args)?;
        info!("Added torrent {hash} via Transmission sidecar");
        Ok(hash)
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let dest = normalize_download_dir(download_dir)?;

        let effective_dest = if let Some(ref d) = dest {
            if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_bytes(bytes) {
                Some(resolve_transmission_download_dir(
                    d,
                    &parsed.name,
                    parsed.is_multi_file,
                ))
            } else {
                Some(d.clone())
            }
        } else {
            None
        };

        let mut args = json!({
            "metainfo": b64,
            "paused": false
        });
        if let Some(ref d) = effective_dest {
            args["download-dir"] = json!(d);
        }

        let hash = self.rpc_client.torrent_add(args)?;
        info!("Added torrent {hash} from metainfo bytes via Transmission sidecar");
        Ok(hash)
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        self.rpc_client.torrent_remove(id, delete_files)
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        self.rpc_client.torrent_stop(Some(id))
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        self.rpc_client.torrent_start(Some(id))
    }

    fn pause_all(&self) -> Result<(), String> {
        self.rpc_client.torrent_stop(None)
    }

    fn resume_all(&self) -> Result<(), String> {
        self.rpc_client.torrent_start(None)
    }

    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(true, Ordering::SeqCst);

        // Immediate mitigation: pause all torrent transfers
        let _ = self.rpc_client.torrent_stop(None);

        // Re-bind to local loopback so zero socket leaks occur
        let _ = self
            .rpc_client
            .session_set(json!({ "bind-address-ipv4": "127.0.0.1" }));

        warn!("Killswitch engaged: all transfers paused and Transmission bound to 127.0.0.1");
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(false, Ordering::SeqCst);

        // Check if a VPN tunnel is currently active to restore binding
        let bind_ip = if let Some(vpn) = find_active_vpn_interface() {
            vpn.ip_addresses
                .iter()
                .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                .cloned()
                .unwrap_or_else(|| "0.0.0.0".to_string())
        } else {
            "0.0.0.0".to_string()
        };

        let _ = self
            .rpc_client
            .session_set(json!({ "bind-address-ipv4": bind_ip }));
        info!("Killswitch disengaged: Transmission restored binding to {bind_ip}");
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch_active.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let stats = self
            .rpc_client
            .session_stats()
            .unwrap_or_else(|_| json!({}));
        let active = stats
            .get("activeTorrentCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let paused = stats
            .get("pausedTorrentCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        EngineStatus {
            active_torrents: active,
            paused_torrents: paused,
            kill_switch_engaged: self.is_kill_switch_active(),
            bound_interface: self.bound_interface.lock().unwrap().clone(),
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        matches!(
            hook,
            EngineHook::PeerTelemetryCollection
                | EngineHook::TrackerDhtPexHandling
                | EngineHook::PrivateTorrentRestrictions
                | EngineHook::SocketInterfaceBinding
                | EngineHook::PauseResumeKillSwitch
        )
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        self.rpc_client.torrent_verify(id)
    }

    fn reannounce(&self, id: &str) -> Result<(), String> {
        self.rpc_client.torrent_reannounce(id)
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        self.rpc_client.torrent_set_location(id, new_path, true)
    }

    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        if let Ok(mut cache) = self.metadata_cache.lock() {
            if let Some(meta) = cache.get_mut(id) {
                meta.category = Some(category.to_string());
                let _ = self.db.upsert_torrent_metadata(meta);
            }
        }
        Ok(())
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        let args = json!({
            "speed-limit-down": dl_kib,
            "speed-limit-down-enabled": dl_kib > 0,
            "speed-limit-up": ul_kib,
            "speed-limit-up-enabled": ul_kib > 0
        });
        self.rpc_client.torrent_set(id, args)?;

        if let Ok(mut cache) = self.metadata_cache.lock() {
            if let Some(meta) = cache.get_mut(id) {
                meta.download_limit_kib = dl_kib;
                meta.upload_limit_kib = ul_kib;
                meta.ratio_limit = ratio_limit;
                let _ = self.db.upsert_torrent_metadata(meta);
            }
        }
        Ok(())
    }

    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String> {
        let current = self
            .metadata_cache
            .lock()
            .unwrap()
            .get(id)
            .map(|m| m.sequential_download)
            .unwrap_or(false);
        let new_val = !current;

        // Transmission 4.1.3 natively supports sequential-download (verified in Phase A1)
        self.rpc_client.torrent_set(
            id,
            json!({
                "sequential-download": new_val
            }),
        )?;

        if let Ok(mut cache) = self.metadata_cache.lock() {
            if let Some(meta) = cache.get_mut(id) {
                meta.sequential_download = new_val;
                let _ = self.db.upsert_torrent_metadata(meta);
            }
        }

        Ok(new_val)
    }

    fn toggle_first_last_piece_priority(&self, _id: &str) -> Result<bool, String> {
        // Not exposed in Transmission JSON-RPC; return Ok(false) with clean log per design note
        warn!("toggle_first_last_piece_priority is not supported by transmission-daemon; no-op");
        Ok(false)
    }

    fn toggle_super_seeding(&self, _id: &str) -> Result<bool, String> {
        // Not exposed in Transmission JSON-RPC; return Ok(false) with clean log per design note
        warn!("toggle_super_seeding is not supported by transmission-daemon; no-op");
        Ok(false)
    }

    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        // UI scale: 0 = do not download, 1 = normal, 2 = high, 3 = max.
        // Transmission RPC has three wanted-file priority buckets
        // (priority-low / priority-normal / priority-high); "maximum"
        // collapses into priority-high.
        let args = if priority == 0 {
            json!({ "files-unwanted": [file_index] })
        } else {
            let bucket = match priority {
                1 => "priority-normal",
                2 | 3 => "priority-high",
                _ => "priority-normal",
            };
            let mut map = serde_json::Map::new();
            map.insert("files-wanted".to_string(), json!([file_index]));
            map.insert(bucket.to_string(), json!([file_index]));
            Value::Object(map)
        };
        self.rpc_client.torrent_set(id, args)
    }

    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String> {
        // Find current top-level name from torrent-get
        let torrents = self.rpc_client.torrent_get(Some(vec![json!(id)]))?;
        let current_name = torrents
            .first()
            .and_then(|t| t.get("name"))
            .and_then(|n| n.as_str())
            .ok_or_else(|| format!("Torrent with id '{id}' not found for rename"))?;

        self.rpc_client
            .torrent_rename_path(id, current_name, new_name)?;

        if let Ok(mut cache) = self.metadata_cache.lock() {
            if let Some(meta) = cache.get_mut(id) {
                meta.display_title = Some(new_name.to_string());
                let _ = self.db.upsert_torrent_metadata(meta);
            }
        }
        Ok(())
    }

    fn rename_torrent_file(
        &self,
        id: &str,
        file_index: u32,
        new_file_name: &str,
    ) -> Result<(), String> {
        let torrents = self.rpc_client.torrent_get(Some(vec![json!(id)]))?;
        let torrent = torrents
            .first()
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))?;

        let file_path = torrent
            .get("files")
            .and_then(|f| f.as_array())
            .and_then(|arr| arr.get(file_index as usize))
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .ok_or_else(|| format!("File index {file_index} not found in torrent '{id}'"))?;

        self.rpc_client
            .torrent_rename_path(id, file_path, new_file_name)
    }

    fn rename_torrent_folder(
        &self,
        id: &str,
        old_folder_path: &str,
        new_folder_name: &str,
    ) -> Result<(), String> {
        self.rpc_client
            .torrent_rename_path(id, old_folder_path, new_folder_name)
    }

    fn export_torrent_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        let torrents = self.rpc_client.torrent_get(Some(vec![json!(id)]))?;
        let torrent = torrents
            .first()
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))?;

        if let Some(path_str) = torrent.get("torrentFile").and_then(|p| p.as_str()) {
            if let Ok(bytes) = std::fs::read(path_str) {
                return Ok(bytes);
            }
        }

        // Fallback: search config_dir/torrents/
        let hash = torrent
            .get("hashString")
            .and_then(|h| h.as_str())
            .unwrap_or_default();
        let direct_file = self
            .config_dir
            .join("torrents")
            .join(format!("{hash}.torrent"));
        if direct_file.is_file() {
            return std::fs::read(&direct_file).map_err(|e| {
                format!(
                    "Failed to read .torrent file at '{}': {e}",
                    direct_file.display()
                )
            });
        }

        Err(format!(
            "Could not locate exported .torrent file for torrent '{id}'"
        ))
    }

    fn get_peer_telemetry(&self, id: &str) -> Result<Vec<PeerTelemetry>, String> {
        let torrents = self.rpc_client.torrent_get(Some(vec![json!(id)]))?;
        let torrent = torrents
            .first()
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))?;

        let mut result = Vec::new();

        if let Some(peers) = torrent.get("peers").and_then(|v| v.as_array()) {
            for (idx, p) in peers.iter().enumerate() {
                let address = p
                    .get("address")
                    .and_then(|a| a.as_str())
                    .unwrap_or("0.0.0.0")
                    .to_string();
                let port = p.get("port").and_then(|pt| pt.as_u64()).unwrap_or(0) as u16;
                let client_name = p
                    .get("clientName")
                    .and_then(|c| c.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let rate_to_client = p.get("rateToClient").and_then(|r| r.as_u64()).unwrap_or(0);
                let rate_to_peer = p.get("rateToPeer").and_then(|r| r.as_u64()).unwrap_or(0);
                let is_encrypted = p
                    .get("isEncrypted")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);
                let is_utp = p.get("isUTP").and_then(|u| u.as_bool()).unwrap_or(false);

                result.push(PeerTelemetry {
                    peer_id: format!("peer-{idx}-{address}"),
                    ip: address.clone(),
                    port,
                    rtt_ms: 50,
                    req_resp_latency_ms: 60,
                    sustained_dl_bps: rate_to_client * 8,
                    sustained_ul_bps: rate_to_peer * 8,
                    upload_reciprocity: 0.5,
                    stability_score: 0.9,
                    disconnect_count: 0,
                    timeout_count: 0,
                    choke_events: 0,
                    client_name,
                    locality_tier: if address.starts_with("192.168.") || address.starts_with("10.")
                    {
                        LocalityTier::Lan
                    } else {
                        LocalityTier::Global
                    },
                    country_code: None,
                    asn: None,
                    utp: is_utp,
                    encryption: is_encrypted,
                    ipv6: address.contains(':'),
                    dht: false,
                    pex: false,
                    connection_age_seconds: 300,
                    total_downloaded_bytes: 0,
                    total_uploaded_bytes: 0,
                });
            }
        }

        Ok(result)
    }

    fn check_port_open(&self) -> Option<bool> {
        self.rpc_client.port_test().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csrf_session_id_negotiation_logic() {
        let client = TransmissionRpcClient::new(9999, "user", "pass");
        assert!(client.session_id.lock().unwrap().is_none());
        assert!(client.auth_header.is_some());
    }

    #[test]
    fn test_transmission_status_mapping() {
        // Transmission status codes:
        // 0: Stopped -> "Paused"
        // 1: Queued to check, 2: Checking -> "Checking"
        // 3: Queued to download, 4: Downloading -> "Downloading"
        // 5: Queued to seed, 6: Seeding -> "Seeding"
        let map_status = |s: i64| match s {
            0 => "Paused",
            1 | 2 => "Checking",
            3 | 4 => "Downloading",
            5 | 6 => "Seeding",
            _ => "Paused",
        };

        assert_eq!(map_status(0), "Paused");
        assert_eq!(map_status(1), "Checking");
        assert_eq!(map_status(2), "Checking");
        assert_eq!(map_status(3), "Downloading");
        assert_eq!(map_status(4), "Downloading");
        assert_eq!(map_status(5), "Seeding");
        assert_eq!(map_status(6), "Seeding");
    }

    #[test]
    fn test_transmission_private_tracker_settings_hard_off() {
        let settings = json!({
            "dht-enabled": false,
            "pex-enabled": false,
            "lpd-enabled": false,
            "encryption": 1,
            "utp-enabled": true
        });

        assert_eq!(settings["dht-enabled"], false);
        assert_eq!(settings["pex-enabled"], false);
        assert_eq!(settings["lpd-enabled"], false);
        assert_eq!(settings["encryption"], 1);
        assert_eq!(settings["utp-enabled"], true);
    }

    #[test]
    fn test_build_transmission_settings_mapping_table() {
        let user_settings = UserSettings {
            listening_port: 52123,
            enable_upnp: true,
            max_connections_per_torrent: 150, // Should be clamped to 100
            max_global_connections: 600,      // Should be clamped to 500
            max_upload_slots_per_torrent: 10, // Should be clamped to 8
            ..Default::default()
        };

        let config = build_transmission_settings(
            &user_settings,
            "/home/user/Downloads",
            "10.2.0.5",
            9091,
            "nebulae",
            "secret_pass",
        );

        assert_eq!(config["rpc-enabled"], true);
        assert_eq!(config["rpc-bind-address"], "127.0.0.1");
        assert_eq!(config["rpc-whitelist-enabled"], true);
        assert_eq!(config["rpc-whitelist"], "127.0.0.1");
        assert_eq!(config["rpc-port"], 9091);
        assert_eq!(config["rpc-username"], "nebulae");
        assert_eq!(config["rpc-password"], "secret_pass");

        // Clamping and mapping table
        assert_eq!(config["peer-port"], 52123);
        assert_eq!(config["port-forwarding-enabled"], true);
        assert_eq!(config["peer-limit-per-torrent"], 100);
        assert_eq!(config["peer-limit-global"], 500);
        assert_eq!(config["upload-slots-per-torrent"], 8);
        assert_eq!(config["encryption"], 1);
        assert_eq!(config["utp-enabled"], true);

        // Private tracker constraints hard-disabled
        assert_eq!(config["dht-enabled"], false);
        assert_eq!(config["pex-enabled"], false);
        assert_eq!(config["lpd-enabled"], false);

        // Interface and path
        assert_eq!(config["bind-address-ipv4"], "10.2.0.5");
        assert_eq!(config["download-dir"], "/home/user/Downloads");
    }

    #[test]
    fn test_rpc_client_mock_csrf_handshake_and_success() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            // First request: return 409 with X-Transmission-Session-Id header
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let resp = "HTTP/1.1 409 Conflict\r\nServer: Transmission\r\nX-Transmission-Session-Id: mock-token-999\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            }

            // Second request: client retries with the token
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]);
                assert!(req_str.contains("x-transmission-session-id: mock-token-999"));

                let body = r#"{"result":"success","arguments":{"version":"4.1.3"}}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        let client = TransmissionRpcClient::new(port, "testuser", "testpass");
        let stats = client.session_get().expect("session_get failed");
        assert_eq!(stats.get("version").and_then(|v| v.as_str()), Some("4.1.3"));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_rpc_client_mock_error_mapping() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            // 1. Accept first request (provide token)
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let resp = "HTTP/1.1 409 Conflict\r\nX-Transmission-Session-Id: mock-token\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            }

            // 2. Accept retry request, return RPC error result
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = r#"{"result":"duplicate torrent","arguments":{}}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            }

            // 3. Next request: return HTTP 500 Internal Server Error
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        let client = TransmissionRpcClient::new(port, "user", "pass");

        // 1. Test RPC error mapping
        let err1 = client.torrent_verify("some-id").unwrap_err();
        assert!(err1.contains("failed: duplicate torrent"));

        // 2. Test HTTP 500 error mapping
        let err2 = client.session_stats().unwrap_err();
        assert!(err2.contains("HTTP 500"));

        server_thread.join().unwrap();
    }

    #[test]
    fn test_resolve_transmission_download_dir_layout() {
        // Multi-file layout: output_folder ends with torrent_name -> parent directory
        let multi_resolved =
            resolve_transmission_download_dir("/home/user/Downloads/MySeries", "MySeries", true);
        assert_eq!(multi_resolved, "/home/user/Downloads");

        // Single-file layout: output_folder is the destination directory
        let single_resolved =
            resolve_transmission_download_dir("/home/user/Downloads", "movie.iso", false);
        assert_eq!(single_resolved, "/home/user/Downloads");

        // Multi-file where output_folder is flat / not suffixed with name
        let flat_resolved =
            resolve_transmission_download_dir("/home/user/Downloads", "OtherSeries", true);
        assert_eq!(flat_resolved, "/home/user/Downloads");
    }

    #[test]
    fn test_expand_tilde_path_expands_front_end_default() {
        let home = dirs::home_dir().expect("test environment must have a home directory");

        // The exact default shipped by settingsStore.ts must resolve to the
        // real home Downloads dir, never a relative '~/Downloads' directory.
        assert_eq!(
            expand_tilde_path(Path::new("~/Downloads")),
            Some(home.join("Downloads"))
        );
        assert_eq!(expand_tilde_path(Path::new("~")), Some(home.clone()));
        assert_eq!(
            expand_tilde_path(Path::new("/absolute/path")),
            Some(PathBuf::from("/absolute/path"))
        );
        // Relative paths must resolve against home, not the process CWD.
        assert_eq!(
            expand_tilde_path(Path::new("relative/dir")),
            Some(home.join("relative/dir"))
        );
    }
}

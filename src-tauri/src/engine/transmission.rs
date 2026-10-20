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
use crate::utils::keyring::{get_secret, passkey_secret_target};
use crate::utils::network::{find_active_egress_vpn_interface, find_active_vpn_interface};
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

            // Linux packaging layouts (Tauri packages resources to /usr/lib/Nebulae/resources/)
            if let Some(prefix) = exe_dir.parent() {
                candidates.push(
                    prefix
                        .join("lib")
                        .join("Nebulae")
                        .join("resources")
                        .join("engine")
                        .join("transmission")
                        .join(bin_name),
                );
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

    // Standard Linux packaged path (/usr/lib/Nebulae/resources/)
    candidates.push(PathBuf::from("/usr/lib/Nebulae/resources/engine/transmission").join(bin_name));

    // AppImage support ($APPDIR)
    if let Ok(appdir) = std::env::var("APPDIR") {
        let appdir_path = PathBuf::from(appdir);
        candidates.push(
            appdir_path
                .join("usr")
                .join("lib")
                .join("Nebulae")
                .join("resources")
                .join("engine")
                .join("transmission")
                .join(bin_name),
        );
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
        let _ = crate::utils::process::create_hidden_command("taskkill")
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

/// `ids` in Transmission RPC accepts NUMBERS (session ids) or strings
/// (info-hash). A numeric STRING like "3" is parsed as an info-hash and
/// silently matches NOTHING - the call "succeeds" while doing nothing.
/// Normalize: numeric ids go over the wire as JSON numbers.
fn rpc_id(id: &str) -> serde_json::Value {
    match id.parse::<i64>() {
        Ok(n) => json!(n),
        Err(_) => json!(id),
    }
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
            "peerStats",
            // Racing Mode state fields
            "bandwidthPriority",
            "honorsSessionLimits",
            "peer-limit",
            // Availability / health
            "desiredAvailable",
            "corruptEver",
            "etaIdle",
            // Verification / errors
            "error",
            "recheckProgress",
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
            "ids": [rpc_id(id)],
            "delete-local-data": delete_data
        });
        self.request("torrent-remove", Some(args))?;
        Ok(())
    }

    pub fn torrent_start(&self, id: Option<&str>) -> Result<(), String> {
        let args = id.map(|i| json!({ "ids": [rpc_id(i)] }));
        self.request("torrent-start", args)?;
        Ok(())
    }

    pub fn torrent_stop(&self, id: Option<&str>) -> Result<(), String> {
        let args = id.map(|i| json!({ "ids": [rpc_id(i)] }));
        self.request("torrent-stop", args)?;
        Ok(())
    }

    pub fn torrent_verify(&self, id: &str) -> Result<(), String> {
        let args = json!({ "ids": [rpc_id(id)] });
        info!("[transmission-rpc] sending torrent-verify for id={id}");
        self.request("torrent-verify", Some(args))?;
        info!("[transmission-rpc] torrent-verify accepted for id={id}");
        Ok(())
    }

    pub fn torrent_reannounce(&self, id: &str) -> Result<(), String> {
        let args = json!({ "ids": [rpc_id(id)] });
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
            "ids": [rpc_id(id)],
            "location": location,
            "move": move_data
        });
        // Moving terabyte-scale libraries takes FAR longer than the shared
        // client's 15 s timeout (the move blocks this RPC until done), so
        // the call is fired on a dedicated thread with a 30-minute window
        // and returns immediately - the daemon keeps moving regardless of
        // what the UI does.
        let rpc_url = self.rpc_url.clone();
        let auth = self.auth_header.clone();
        // Clone the current token as a mutable local — the thread will update
        // it in-place whenever Transmission returns 409 with a new token,
        // so subsequent retries use the refreshed value.
        let mut sid = self.session_id.lock().unwrap().clone();
        let id = id.to_string();
        let location = location.to_string();
        std::thread::spawn(move || {
            let client = match reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(1800))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("[set-location] client build failed: {e}");
                    return;
                }
            };
            for _attempt in 0..3 {
                let mut req = client.post(&rpc_url).json(&args);
                if let Some(ref s) = sid {
                    req = req.header("X-Transmission-Session-Id", s);
                }
                if let Some(ref a) = auth {
                    req = req.header("Authorization", a);
                }
                match req.send() {
                    Ok(resp) if resp.status().as_u16() == 409 => {
                        // CSRF handshake: store the new token so the next
                        // retry sends the correct value.
                        if let Some(Ok(new_token)) = resp
                            .headers()
                            .get("X-Transmission-Session-Id")
                            .map(|h| h.to_str())
                        {
                            sid = Some(new_token.to_string());
                        }
                        continue;
                    }
                    Ok(resp) => {
                        tracing::info!(
                            "[set-location] move of {} -> {} finished: {}",
                            id,
                            location,
                            resp.status()
                        );
                        return;
                    }
                    Err(e) => {
                        tracing::error!("[set-location] move failed: {e}");
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    pub fn torrent_set(&self, id: &str, mut args: Value) -> Result<(), String> {
        if let Some(map) = args.as_object_mut() {
            map.insert("ids".to_string(), json!([rpc_id(id)]));
        }
        self.request("torrent-set", Some(args))?;
        Ok(())
    }

    pub fn torrent_rename_path(&self, id: &str, path: &str, name: &str) -> Result<(), String> {
        let args = json!({
            "ids": [rpc_id(id)],
            "path": path,
            "name": name
        });
        self.request("torrent-rename-path", Some(args))?;
        Ok(())
    }

    /// Disconnect a specific peer from a specific torrent.
    ///
    /// Uses Transmission's `peer-remove` RPC method (TR 4.x+).
    /// `torrent_id` is the numeric Transmission torrent id (not the info-hash).
    /// `peer_address` is the peer's `"ip:port"` string as returned by `torrent-get peers[]`.
    ///
    /// If the daemon does not support `peer-remove` it returns an error string
    /// beginning with "Transmission RPC call 'peer-remove' failed:" which the
    /// caller can detect to skip future removal attempts.
    pub fn peer_remove(&self, torrent_id: i64, peer_address: &str) -> Result<(), String> {
        let args = json!({
            "ids": [torrent_id],
            "address": peer_address
        });
        self.request("peer-remove", Some(args))?;
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
    /// Per-peer EMA rate state, keyed by `"<torrent_hash>:<peer_ip>"`.
    /// Cleared when a torrent is removed to prevent unbounded growth.
    peer_ema_cache: Arc<Mutex<HashMap<String, crate::models::peering::PeerEmaState>>>,
    /// Offline IP geolocation resolver. `None` when .mmdb files are absent.
    geo_resolver: Option<Arc<crate::utils::geo::GeoResolver>>,
    /// Background TCP RTT prober. Always present; probing is opt-in per peer.
    rtt_prober: Arc<crate::utils::rtt_prober::RttProber>,
    /// Shared flag: set to true when peer-remove RPC is confirmed unsupported
    /// by the bundled daemon. Shared between enforcement and quality_eviction.
    /// Kept on the struct to ensure the Arc outlives all background tasks.
    #[allow(dead_code)]
    peer_remove_unsupported: Arc<AtomicBool>,
    daemon_version: Arc<Mutex<Option<String>>>,
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
    let cache_mb = if settings.disk_cache_size_mib <= 0 {
        4i32
    } else {
        settings.disk_cache_size_mib
    };
    let prealloc: u8 = if settings.preallocate_disk_space {
        1
    } else {
        0
    };
    let idle_seeding_enabled = settings.max_seeding_time_minutes >= 0;
    let idle_seeding_limit = settings.max_seeding_time_minutes.max(0) as u32;

    // Racing Mode session overrides — aggressive tuning preset.
    // When disabled, user settings are restored as configured without arbitrary min clamps.
    let (peer_per_torrent, peer_global, unchoke_slots, cache_mb_eff, stall_minutes) =
        if settings.racing_mode_enabled {
            (150u32, 1000u32, 14u32, 256i32, 2u32)
        } else {
            (
                settings.max_connections_per_torrent.max(1),
                settings.max_global_connections.max(1),
                settings.max_upload_slots_per_torrent.max(1),
                cache_mb,
                30u32,
            )
        };

    // Completion script: must remain disabled unless explicitly configured with a non-empty command
    let script_enabled = settings.run_external_program_on_finished
        && !settings.external_program_command.trim().is_empty();

    json!({
        // RPC access (loopback-only)
        "rpc-enabled": true,
        "rpc-port": rpc_port,
        "rpc-bind-address": "127.0.0.1",
        "rpc-whitelist-enabled": true,
        "rpc-whitelist": "127.0.0.1",
        "rpc-authentication-required": true,
        "rpc-username": rpc_user,
        "rpc-password": rpc_password,

        // Peer connectivity
        "peer-port": settings.listening_port,
        "port-forwarding-enabled": settings.enable_upnp,
        "peer-limit-per-torrent": peer_per_torrent,
        "peer-limit-global": peer_global,
        "upload-slots-per-torrent": unchoke_slots,
        "encryption": 1, // Preferred
        "utp-enabled": true,

        // Mandatory private tracker constraints: hard-off at session level
        "dht-enabled": false,
        "pex-enabled": false,
        "lpd-enabled": false,

        // Interface binding and storage
        "bind-address-ipv4": bind_ipv4,
        "download-dir": default_download_dir,
        "incomplete-dir": settings.temp_incomplete_directory,
        "incomplete-dir-enabled": settings.use_temp_incomplete_dir,
        "preallocation": prealloc,
        "rename-partial-files": true,
        // Nebulae manages watched folder via app watcher loop; disable daemon watcher to prevent duplicate competing importers
        "watch-dir": settings.watched_folder_directory,
        "watch-dir-enabled": false,

        // Disk cache (major throughput lever — default is only 4 MiB)
        "cache-size-mb": cache_mb_eff,

        // Default trackers for public torrents
        "default-trackers": settings.default_trackers_list,

        // Speed limits
        "speed-limit-down": settings.max_download_speed_kib,
        "speed-limit-down-enabled": settings.max_download_speed_kib > 0,
        "speed-limit-up": settings.max_upload_speed_kib,
        "speed-limit-up-enabled": settings.max_upload_speed_kib > 0,

        // Turtle Mode (alt speed)
        "alt-speed-enabled": settings.alt_rate_limits_enabled,
        "alt-speed-down": settings.alt_download_speed_kib,
        "alt-speed-up": settings.alt_upload_speed_kib,

        // Queue management: queue-stalled-minutes affects active-torrent queue accounting
        "download-queue-size": settings.max_active_downloads,
        "download-queue-enabled": settings.max_active_downloads > 0,
        "queue-stalled-enabled": settings.ignore_slow_torrents_in_queue,
        "queue-stalled-minutes": stall_minutes,

        // Seeding ratio / idle limits
        "seedRatioLimit": settings.max_ratio_limit.max(0.0),
        "seedRatioLimited": settings.max_ratio_limit >= 0.0,
        "idle-seeding-limit": idle_seeding_limit,
        "idle-seeding-limit-enabled": idle_seeding_enabled,

        // External script on completion
        "script-torrent-done-enabled": script_enabled,
        "script-torrent-done-filename": settings.external_program_command,
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

/// Owner directive: try different peer ports until one is found externally
/// reachable. Each candidate is applied live (`session-set` "peer-port", the
/// daemon rebinds) and tested with Transmission's own external port test.
/// The first reachable port wins and is persisted; if none is reachable the
/// configured port is kept and the log records the honest verdict — this
/// router refuses programmatic mappings (NAT-PMP/PCP), so a manual forward
/// in the router UI remains the only path to connectability.
fn negotiate_reachable_port(
    rpc: &TransmissionRpcClient,
    db: &Arc<Database>,
    engine_dir: &Path,
    configured: u16,
) {
    let mut candidates: Vec<u16> = vec![configured];
    for delta in [1u32, 2, 3, 5, 11] {
        if let Ok(p) = u16::try_from(configured as u32 + delta) {
            candidates.push(p);
        }
    }

    for (i, &port) in candidates.iter().enumerate() {
        if i > 0 {
            if let Err(e) = rpc.session_set(json!({ "peer-port": port })) {
                warn!("[port] candidate {port}: session-set failed: {e}");
                continue;
            }
            // Give the daemon a moment to rebind before testing.
            std::thread::sleep(Duration::from_secs(2));
        }
        match rpc.port_test() {
            Ok(true) => {
                info!("[port] negotiated: peer port {port} is externally reachable");
                persist_negotiated_port(db, engine_dir, port);
                return;
            }
            Ok(false) => info!("[port] candidate {port}: externally closed"),
            Err(e) => warn!("[port] candidate {port}: test failed: {e}"),
        }
    }

    // None of the candidates was reachable: put the live session back on the
    // configured port. Without this, the daemon stays on the LAST tried
    // candidate (e.g. 41424) while settings.json keeps {configured} — a
    // silent drift that leaves the router's manual forward pointing at a
    // port nobody listens on. Next launch rebinds correctly anyway, but the
    // running session should not spend its whole lifetime on the wrong port.
    if let Err(e) = rpc.session_set(json!({ "peer-port": configured })) {
        warn!("[port] failed to restore configured port {configured} on the live session: {e}");
    } else {
        info!("[port] live session restored to configured port {configured}");
    }

    warn!(
        "[port] none of the tried ports ({}) is externally reachable; keeping {configured}. \
         The router refuses programmatic mappings (NAT-PMP/PCP) — a manual port forward is required.",
        candidates
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Owner directive: when no candidate is externally reachable, surface a
    // user-facing explanation instead of failing silently. The verdict is
    // persisted (the engine is built before the Tauri app handle exists, so
    // there is no event channel here); the front end reads it through
    // `get_port_negotiation_result` and shows a dialog with router
    // port-forwarding instructions.
    let _ = db.set_setting(
        "port_negotiation_result",
        json!({
            "status": "unreachable",
            "configured_port": configured,
            "tried_ports": candidates,
            "checked_at_epoch_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        })
        .to_string()
        .as_str(),
    );
}

/// Persist the negotiated port everywhere the port is read: the stored user
/// settings (status bar, connectivity doctor, next spawn) and the daemon's
/// own settings.json (so a restart rebinds the same port even if the DB
/// write fails).
fn persist_negotiated_port(db: &Arc<Database>, engine_dir: &Path, port: u16) {
    // Clear any prior "unreachable" verdict so the front end does not show a
    // stale port-forward warning after the port is working.
    let _ = db.set_setting(
        "port_negotiation_result",
        json!({
            "status": "reachable",
            "configured_port": port,
            "checked_at_epoch_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        })
        .to_string()
        .as_str(),
    );
    if let Ok(Some(raw)) = db.get_setting("user_settings_json") {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if v.get("listening_port").and_then(|p| p.as_u64()) != Some(port as u64) {
                v["listening_port"] = json!(port);
                if let Ok(s) = serde_json::to_string(&v) {
                    if let Err(e) = db.set_setting("user_settings_json", &s) {
                        warn!("[port] failed to persist negotiated port: {e}");
                    }
                }
            }
        }
    }
    let sp = engine_dir.join("settings.json");
    if let Ok(raw) = std::fs::read_to_string(&sp) {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
            v["peer-port"] = json!(port);
            let _ = std::fs::write(&sp, serde_json::to_string_pretty(&v).unwrap_or_default());
        }
    }
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
            // Bind ONLY to a tunnel that can actually carry internet traffic.
            // A mesh-VPN interface without egress (Tailscale, no exit node)
            // accepts the bind but every peer connection then fails with no
            // error — torrents stall with zero peers while announces still
            // work. Verified empirically 2026-10-16.
            if let Some(vpn_iface) = find_active_egress_vpn_interface() {
                if let Some(vpn_ip) = vpn_iface
                    .ip_addresses
                    .iter()
                    .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                {
                    bind_ipv4 = vpn_ip.clone();
                    info!("Hardening at spawn: bound transmission-daemon to active tunnel IP {bind_ipv4}");
                }
            } else if let Some(any_vpn) = find_active_vpn_interface() {
                // A VPN-type interface exists but has no internet egress.
                // Binding peer traffic to it would stall every torrent; the
                // default route is used instead and the situation is logged
                // loudly so the connectivity doctor and the user can see it.
                bind_ipv4 = "0.0.0.0".to_string();
                warn!(
                    "[vpn] interface {} is up but has NO internet egress (mesh VPN without exit node?); \
                     binding engine to the default route instead. Peer traffic is NOT tunneled.",
                    any_vpn.name
                );
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

        let daemon_supports_identity =
            crate::engine::identity::check_daemon_identity_support(&daemon_path);
        let app_version = crate::engine::identity::get_app_version();
        let nebulae_ua_res = crate::engine::identity::derive_nebulae_user_agent(app_version);
        let nebulae_prefix_res =
            crate::engine::identity::derive_nebulae_peer_id_prefix(app_version);

        let tr_default_ua = crate::engine::identity::get_bundled_transmission_user_agent();
        let tr_default_prefix = crate::engine::identity::get_bundled_transmission_peer_id_prefix();

        let (spawned_profile, spawned_ua, spawned_prefix) = if !daemon_supports_identity {
            ("transmission".to_string(), tr_default_ua, tr_default_prefix)
        } else {
            match settings.engine_identity_profile.as_str() {
                "nebulae" => match (nebulae_ua_res, nebulae_prefix_res) {
                    (Ok(ua), Ok(prefix)) => ("nebulae".to_string(), ua, prefix),
                    (Err(e), _) | (_, Err(e)) => {
                        warn!("[identity] native Nebulae identity unavailable: {e}; falling back to stock Transmission");
                        ("transmission".to_string(), tr_default_ua, tr_default_prefix)
                    }
                },
                "custom" => {
                    let custom_ua = if settings.custom_user_agent.trim().is_empty() {
                        crate::engine::identity::derive_nebulae_user_agent(app_version)
                            .unwrap_or_else(|_| tr_default_ua.clone())
                    } else {
                        settings.custom_user_agent.trim().to_string()
                    };
                    let custom_prefix = if settings.custom_peer_id_prefix.trim().is_empty() {
                        crate::engine::identity::derive_nebulae_peer_id_prefix(app_version)
                            .unwrap_or_else(|_| tr_default_prefix.clone())
                    } else {
                        settings.custom_peer_id_prefix.trim().to_string()
                    };

                    if custom_prefix.len() == 8 {
                        ("custom".to_string(), custom_ua, custom_prefix)
                    } else {
                        warn!(
                            "[identity] custom peer-id prefix '{}' is not 8 bytes; falling back to stock Transmission",
                            custom_prefix
                        );
                        ("transmission".to_string(), tr_default_ua, tr_default_prefix)
                    }
                }
                _ => ("transmission".to_string(), tr_default_ua, tr_default_prefix),
            }
        };

        let mut cmd = std::process::Command::new(&daemon_path);
        cmd.arg("--config-dir")
            .arg(&engine_dir)
            .arg("--foreground")
            // Pin the working directory to the writable engine dir: inheriting
            // the app's CWD (e.g. a mounted DMG volume) must never affect the
            // daemon's path handling.
            .current_dir(&engine_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if daemon_supports_identity && spawned_profile != "transmission" {
            cmd.env("TRANSMISSION_USER_AGENT", &spawned_ua);
            cmd.env("TRANSMISSION_PEER_ID_PREFIX", &spawned_prefix);
            info!(
                "[identity] launched daemon with custom identity: UA='{}', PeerID='{}'",
                spawned_ua, spawned_prefix
            );
        } else {
            cmd.env_remove("TRANSMISSION_USER_AGENT");
            cmd.env_remove("TRANSMISSION_PEER_ID_PREFIX");
            info!("[identity] launched daemon with stock Transmission identity");
        }
        // transmission-daemon is a CONSOLE program: without CREATE_NO_WINDOW
        // Windows opens a cmd window (showing the engine dir) next to the app.
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd
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
        let daemon_version_arc = Arc::new(Mutex::new(None));
        let mut ready = false;
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if let Ok(resp) = rpc_client.session_get() {
                if let Some(v) = resp
                    .get("arguments")
                    .and_then(|a| a.get("version"))
                    .and_then(|v| v.as_str())
                {
                    *daemon_version_arc.lock().unwrap() = Some(v.to_string());
                }
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

        // Record active/spawned identity in DB only after daemon has successfully started
        // and verified responsive, ensuring a failed start/restart never reports an identity
        // that failed to become active.
        let _ = db.set_setting(
            "active_engine_identity",
            &serde_json::json!({
                "profile": spawned_profile,
                "user_agent": spawned_ua,
                "peer_id_prefix": spawned_prefix,
                "supports_overrides": daemon_supports_identity,
            })
            .to_string(),
        );
        let _ = db.set_setting(
            "spawned_engine_identity",
            &serde_json::json!({
                "profile": spawned_profile,
                "user_agent": spawned_ua,
                "peer_id_prefix": spawned_prefix,
            })
            .to_string(),
        );

        // 7b. Execute one-time migration from librqbit if needed
        run_librqbit_migration(&rpc_client, &paths.engine_dir, &db);

        // 7c. Owner directive: try different peer ports until one is found
        // externally reachable. Runs on a background thread (each external
        // port test takes seconds); the winning port is persisted so the
        // status bar, connectivity doctor and the next spawn all use it.
        {
            let rpc = Arc::clone(&rpc_client);
            let db = Arc::clone(&db);
            let engine_dir_neg = engine_dir.clone();
            let configured = settings.listening_port;
            let _ = std::thread::Builder::new()
                .name("port-negotiator".into())
                .spawn(move || negotiate_reachable_port(&rpc, &db, &engine_dir_neg, configured))
                .map_err(|e| warn!("[port] negotiator thread failed to spawn: {e}"));
        }

        let cached_meta = db.list_all_torrent_metadata().unwrap_or_default();

        let child_arc = Arc::new(Mutex::new(Some(child)));

        // 8. Spawn health watchdog
        {
            let child_watch = Arc::clone(&child_arc);
            let daemon_path_watch = daemon_path.clone();
            let engine_dir_watch = engine_dir.clone();
            let pid_file_watch = engine_dir.join("daemon.pid");
            let spawned_ua_watch = spawned_ua.clone();
            let spawned_prefix_watch = spawned_prefix.clone();
            let spawned_profile_watch = spawned_profile.clone();
            let daemon_supports_identity_watch = daemon_supports_identity;

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

                        let mut cmd = std::process::Command::new(&daemon_path_watch);
                        cmd.arg("--config-dir")
                            .arg(&engine_dir_watch)
                            .arg("--foreground")
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null());

                        if daemon_supports_identity_watch && spawned_profile_watch != "transmission"
                        {
                            cmd.env("TRANSMISSION_USER_AGENT", &spawned_ua_watch);
                            cmd.env("TRANSMISSION_PEER_ID_PREFIX", &spawned_prefix_watch);
                        } else {
                            cmd.env_remove("TRANSMISSION_USER_AGENT");
                            cmd.env_remove("TRANSMISSION_PEER_ID_PREFIX");
                        }
                        #[cfg(target_os = "windows")]
                        {
                            use std::os::windows::process::CommandExt;
                            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                        }
                        match cmd.spawn() {
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

        // 9. Spawn tracker-enforcement poller (peer-remove for banned leechers).
        //    The passkey lives in the OS keychain — the same credential that the
        //    announce URL uses. If no account is linked yet, we skip silently.
        //
        //    The `peer_remove_unsupported` flag is shared between the enforcement
        //    poller and the quality-eviction task so both go idle together if the
        //    bundled daemon doesn't support the RPC.
        let peer_remove_unsupported = Arc::new(AtomicBool::new(false));
        let peer_ema_cache: Arc<Mutex<HashMap<String, crate::models::peering::PeerEmaState>>> =
            Arc::new(Mutex::new(HashMap::new()));

        {
            let passkey_opt = get_secret(&passkey_secret_target())
                .ok()
                .filter(|p| !p.trim().is_empty())
                .or_else(|| {
                    db.get_setting("adc_passkey")
                        .ok()
                        .flatten()
                        .filter(|p| !p.trim().is_empty())
                });

            match passkey_opt {
                Some(passkey) => {
                    crate::engine::enforcement::spawn(
                        "https://asiandvdclub.org".to_string(),
                        passkey,
                        Arc::clone(&rpc_client),
                    );
                    info!("[enforcement] tracker enforcement poller spawned");
                }
                None => {
                    info!("[enforcement] passkey empty — enforcement poller not started (link your ADC account)");
                }
            }
        }

        // 10. Load GeoLite2 resolver first so it can be passed to the eviction
        //     task below (graceful degradation if .mmdb absent).
        let geo_resolver = match resource_dir {
            Some(rdir) => {
                match crate::utils::geo::GeoResolver::from_resource_dir(rdir) {
                    Ok(Some(resolver)) => {
                        // Set host egress IP for relative locality classification.
                        let host_ip = find_active_egress_vpn_interface()
                            .or_else(find_active_vpn_interface)
                            .and_then(|iface| iface.ip_addresses.into_iter().next());
                        let with_host = if let Some(ip) = host_ip {
                            resolver.with_host_ip(&ip)
                        } else {
                            resolver
                        };
                        info!(
                            "[geo] GeoLite2 resolver loaded — full locality classification active"
                        );
                        Some(Arc::new(with_host))
                    }
                    Ok(None) => {
                        info!("[geo] GeoLite2 .mmdb absent — locality classification: LAN/Global only");
                        None
                    }
                    Err(e) => {
                        warn!("[geo] Failed to load GeoLite2 resolver: {e}");
                        None
                    }
                }
            }
            None => None,
        };

        // 11. Spawn quality-based peer eviction task (geo-aware when .mmdb present).
        crate::engine::quality_eviction::spawn(
            Arc::clone(&rpc_client),
            Arc::clone(&peer_ema_cache),
            Arc::clone(&db),
            Arc::clone(&peer_remove_unsupported),
            geo_resolver.clone(),
        );
        info!("[quality_eviction] quality-based peer eviction task spawned");

        // 12. RTT prober (always created; probing is opt-in per settings).
        let rtt_prober = Arc::new(crate::utils::rtt_prober::RttProber::new());

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
            peer_ema_cache,
            geo_resolver,
            rtt_prober,
            peer_remove_unsupported,
            daemon_version: daemon_version_arc,
        })
    }

    pub fn rpc(&self) -> &Arc<TransmissionRpcClient> {
        &self.rpc_client
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Applies per-torrent Racing Mode settings immediately after a torrent is
    /// added. Reads current user settings from DB to determine whether Racing
    /// Mode is active and what speed cap is set.
    fn apply_racing_mode_on_add(&self, hash: &str) {
        let settings_opt = self
            .db
            .get_setting("user_settings_json")
            .ok()
            .flatten()
            .and_then(|raw| {
                serde_json::from_str::<crate::commands::settings::UserSettings>(&raw).ok()
            });

        let Some(settings) = settings_opt else {
            return;
        };

        if !settings.racing_mode_enabled {
            return;
        }

        // Preserve session limits: global upload cap, alternate speed policy,
        // and queue limits must remain honored even when Racing Mode preset is active.
        if let Err(e) = self.rpc_client.torrent_set(
            hash,
            serde_json::json!({
                "peer-limit":          150i64,
                "bandwidthPriority":   1i64,
                "honorsSessionLimits": true,
            }),
        ) {
            warn!("[racing_mode] failed to apply per-torrent settings to {hash}: {e}");
        } else {
            info!("[racing_mode] per-torrent settings applied to {hash}");
        }
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

        let user_settings = self
            .db
            .get_setting("user_settings_json")
            .ok()
            .flatten()
            .and_then(|raw| {
                serde_json::from_str::<crate::commands::settings::UserSettings>(&raw).ok()
            })
            .unwrap_or_default();
        let user_peer_limit = user_settings.max_connections_per_torrent.max(1);
        let global_racing = user_settings.racing_mode_enabled;
        let global_super_seeding = user_settings.super_seeding_enabled;

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
            let recheck_progress = t
                .get("recheckProgress")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let error_num = t.get("error").and_then(|v| v.as_i64()).unwrap_or(0);
            let error_string = t.get("errorString").and_then(|v| v.as_str()).unwrap_or("");
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
            // 1: Queued to check -> "Queued for checking"
            // 2: Checking -> "Checking"
            // 3: Queued to download, 4: Downloading -> "Downloading"
            // 5: Queued to seed, 6: Seeding -> "Seeding"
            // Local errors (e.g. missing files on disk) -> "Error"
            let status = if is_ks {
                "Paused".to_string()
            } else if error_num == 3 || (!error_string.is_empty() && error_num != 0) {
                "Error".to_string()
            } else {
                match status_num {
                    0 => "Paused".to_string(),
                    1 => "Queued for checking".to_string(),
                    2 => "Checking".to_string(),
                    3 | 4 => "Downloading".to_string(),
                    5 | 6 => "Seeding".to_string(),
                    _ => "Paused".to_string(),
                }
            };

            // During checking or check-queueing, progress reflects verification
            // progress (0.0 when queued, recheck_progress when checking), NOT
            // percent_done. When not checking, progress is percent_done.
            let progress = match status_num {
                1 => 0.0f32,
                2 => recheck_progress,
                _ => percent_done,
            };

            // Honest ratio (owner correction): a torrent whose data was
            // already on disk at add-time (verified to 100% with zero bytes
            // downloaded) must NOT display a synthetic ratio of 1. Ratio is
            // 0 until bytes have actually been uploaded; infinite (shown as
            // ∞) only when upload happened with no recorded download.
            let ratio = if uploaded_ever == 0 {
                0.0f32
            } else if downloaded_ever > 0 {
                uploaded_ever as f32 / downloaded_ever as f32
            } else {
                f32::INFINITY
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

            let bandwidth_priority = t
                .get("bandwidthPriority")
                .and_then(|v| v.as_i64())
                .map(|p| p as i8);

            let peer_limit = t
                .get("peer-limit")
                .and_then(|v| v.as_i64())
                .map(|l| l as u32);

            let is_downloading = status_num == 3 || status_num == 4;
            let is_seeding = status_num == 5 || status_num == 6;

            // Policy synchronization:
            // 1. Actively downloading torrents: apply Racing mode if globally enabled, restore if disabled
            if is_downloading {
                if global_racing && (bandwidth_priority != Some(1) || peer_limit.unwrap_or(0) < 150)
                {
                    let _ = self.rpc_client.torrent_set(
                        &id,
                        serde_json::json!({
                            "peer-limit": 150i64,
                            "bandwidthPriority": 1i64,
                            "honorsSessionLimits": true,
                        }),
                    );
                } else if !global_racing
                    && (bandwidth_priority == Some(1) && peer_limit.unwrap_or(0) >= 150)
                {
                    let _ = self.rpc_client.torrent_set(
                        &id,
                        serde_json::json!({
                            "peer-limit": user_peer_limit as i64,
                            "bandwidthPriority": 0i64,
                            "honorsSessionLimits": true,
                        }),
                    );
                }
            } else if is_seeding {
                // 2. Actively seeding torrents: must NEVER retain downloading Racing mode overrides
                if bandwidth_priority == Some(1) && peer_limit.unwrap_or(0) >= 150 {
                    let _ = self.rpc_client.torrent_set(
                        &id,
                        serde_json::json!({
                            "peer-limit": user_peer_limit as i64,
                            "bandwidthPriority": 0i64,
                            "honorsSessionLimits": true,
                        }),
                    );
                }
            }

            // Applied indicators (strictly following state applicability):
            // Racing mode applied: actively downloading AND has racing mode active
            let is_racing_applied = is_downloading
                && (global_racing
                    || (bandwidth_priority == Some(1) && peer_limit.unwrap_or(0) >= 150));
            let racing_mode = Some(is_racing_applied);

            // Super seeding applied: actively seeding AND has super seeding active
            let is_super_seeding_applied = is_seeding
                && (global_super_seeding || meta.map(|m| m.super_seeding).unwrap_or(false));

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
                progress,
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
                super_seeding: is_super_seeding_applied,
                tags: Vec::new(),
                files,
                trackers,
                download_limit_kib: meta.map(|m| m.download_limit_kib).unwrap_or(0),
                upload_limit_kib: meta.map(|m| m.upload_limit_kib).unwrap_or(0),
                ratio_limit: meta.map(|m| m.ratio_limit).unwrap_or(-1.0),
                bandwidth_priority,
                peer_limit,
                racing_mode,
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

            // Check if trimmed is relative to user's Downloads directory
            if let Some(dl_dir) = dirs::download_dir() {
                let candidate = dl_dir.join(trimmed);
                if candidate.is_file() {
                    if let Ok(bytes) = std::fs::read(&candidate) {
                        return self.add_torrent_from_bytes(&bytes, download_dir);
                    }
                }
            }

            // Check if trimmed is relative to the target download_dir
            if let Some(dir) = download_dir {
                let candidate = std::path::Path::new(dir).join(trimmed);
                if candidate.is_file() {
                    if let Ok(bytes) = std::fs::read(&candidate) {
                        return self.add_torrent_from_bytes(&bytes, download_dir);
                    }
                }
            }

            // If it is not a URL and not a found file on disk, return an explicit error
            // instead of sending an unresolvable bare filename to Transmission RPC that
            // triggers "Transmission RPC call 'torrent-add' failed: unrecognized info".
            return Err(format!(
                "Torrent file not found: '{}'. Please select or drag the .torrent file again.",
                trimmed
            ));
        }

        // 3. Magnet or HTTP(S) URL
        let dest = normalize_download_dir(download_dir)?;
        let mut args = json!({
            "filename": trimmed,
            "paused": true
        });
        if let Some(ref d) = dest {
            args["download-dir"] = json!(d);
        }

        let hash = self.rpc_client.torrent_add(args)?;
        // Verify FIRST (finds renamed existing data), then start.
        let _ = self.rpc_client.torrent_verify(&hash);
        let _ = self.rpc_client.torrent_start(Some(&hash));
        self.apply_racing_mode_on_add(&hash);
        info!("Added torrent {hash} via Transmission sidecar (verify-then-start)");
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
            "paused": true
        });
        if let Some(ref d) = effective_dest {
            args["download-dir"] = json!(d);
        }

        let hash = self.rpc_client.torrent_add(args)?;
        // Verify FIRST (finds renamed existing data), then start: without
        // the initial check the torrent re-downloads from scratch.
        let _ = self.rpc_client.torrent_verify(&hash);
        let _ = self.rpc_client.torrent_start(Some(&hash));
        self.apply_racing_mode_on_add(&hash);
        info!(
            "Added torrent {hash} from metainfo bytes via Transmission sidecar (verify-then-start)"
        );
        Ok(hash)
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        self.rpc_client.torrent_remove(id, delete_files)?;
        // Clear EMA state for all peers that belonged to this torrent.
        // Keys have the form "<torrent_hash>:<peer_ip>" so a prefix match suffices.
        let prefix = format!("{id}:");
        if let Ok(mut cache) = self.peer_ema_cache.lock() {
            cache.retain(|k, _| !k.starts_with(&prefix));
        }
        Ok(())
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

        // Check if a VPN tunnel with real egress is currently active to
        // restore binding. A mesh-VPN interface without egress must NOT
        // receive the peer bind (torrents stall with zero peers).
        let bind_ip = if let Some(vpn) = find_active_egress_vpn_interface() {
            vpn.ip_addresses
                .iter()
                .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                .cloned()
                .unwrap_or_else(|| "0.0.0.0".to_string())
        } else {
            if find_active_vpn_interface().is_some() {
                warn!("[vpn] VPN interface present but without internet egress; keeping the default-route binding");
            }
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
        info!("[engine] force_recheck requested for torrent id={id}");
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
        // Negative ratio_limit means "use global default" (no per-torrent override).
        // Transmission interprets seedRatioLimited=false as "follow session default".
        let args = json!({
            "speed-limit-down": dl_kib,
            "speed-limit-down-enabled": dl_kib > 0,
            "speed-limit-up": ul_kib,
            "speed-limit-up-enabled": ul_kib > 0,
            "seedRatioLimit": ratio_limit.max(0.0),
            "seedRatioLimited": ratio_limit >= 0.0
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
        let hash = if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            id.to_lowercase()
        } else if let Ok(torrents) = self.rpc_client.torrent_get(Some(vec![rpc_id(id)])) {
            torrents
                .first()
                .and_then(|t| t.get("hashString"))
                .and_then(|h| h.as_str())
                .unwrap_or(id)
                .to_lowercase()
        } else {
            id.to_lowercase()
        };

        let current = if let Ok(cache) = self.metadata_cache.lock() {
            cache
                .get(&hash)
                .map(|m| m.sequential_download)
                .unwrap_or(false)
        } else {
            false
        };
        let new_val = !current;

        // Transmission 4.1.3 RPC spec uses snake_case: "sequential_download".
        // The hyphenated variant ("sequential-download") is silently ignored.
        self.rpc_client.torrent_set(
            id,
            json!({
                "sequential_download": new_val
            }),
        )?;

        if let Ok(mut cache) = self.metadata_cache.lock() {
            let meta = cache
                .entry(hash.clone())
                .or_insert_with(|| TorrentDbMetadata {
                    info_hash: hash.clone(),
                    ..Default::default()
                });
            meta.sequential_download = new_val;
            let _ = self.db.upsert_torrent_metadata(meta);
        }

        info!("[sequential_download] toggled for torrent {id} ({hash}) -> {new_val}");
        Ok(new_val)
    }

    fn toggle_first_last_piece_priority(&self, _id: &str) -> Result<bool, String> {
        // Not exposed in Transmission JSON-RPC; return Ok(false) with clean log per design note
        warn!("toggle_first_last_piece_priority is not supported by transmission-daemon; no-op");
        Ok(false)
    }

    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String> {
        let hash = if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            id.to_string()
        } else if let Ok(torrents) = self.rpc_client.torrent_get(Some(vec![rpc_id(id)])) {
            torrents
                .first()
                .and_then(|t| t.get("hashString"))
                .and_then(|h| h.as_str())
                .unwrap_or(id)
                .to_string()
        } else {
            id.to_string()
        };

        let current = self
            .db
            .get_torrent_metadata(&hash)
            .ok()
            .flatten()
            .map(|m| m.super_seeding)
            .unwrap_or(false);
        let new_state = !current;
        let _ = self.db.set_torrent_super_seeding(&hash, new_state);

        if let Ok(mut cache) = self.metadata_cache.lock() {
            if let Some(meta) = cache.get_mut(&hash) {
                meta.super_seeding = new_state;
            }
        }

        info!("[super_seeding] toggled for torrent {id} ({hash}) -> {new_state}");
        Ok(new_state)
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
        // Find current top-level name from torrent-get.
        // rpc_id() converts numeric session-id strings to JSON numbers;
        // a bare json!(id) would treat the id as an info-hash string and
        // silently match nothing when the id is numeric.
        let torrents = self.rpc_client.torrent_get(Some(vec![rpc_id(id)]))?;
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
        let torrents = self.rpc_client.torrent_get(Some(vec![rpc_id(id)]))?;
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
        let torrents = self.rpc_client.torrent_get(Some(vec![rpc_id(id)]))?;
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
        use crate::models::peering::PeerEmaState;

        const EMA_ALPHA: f64 = 0.3; // smoothing factor — 30% new sample, 70% history
        const MIN_OBSERVATIONS_FOR_STABILITY: u32 = 3;

        let torrents = self.rpc_client.torrent_get(Some(vec![rpc_id(id)]))?;
        let torrent = torrents
            .first()
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))?;

        // torrent hash is used to namespace EMA cache keys so entries don't
        // collide across different torrents that happen to have the same peer IP.
        let torrent_hash = torrent
            .get("hashString")
            .and_then(|h| h.as_str())
            .unwrap_or(id);

        // `peerStats` is an array parallel to `peers`, indexed identically.
        // Each entry has { "toClientBytes": u64, "toPeerBytes": u64, ... }.
        // Fall back gracefully to zero if the daemon doesn't provide it.
        let peer_stats_arr: Vec<&Value> = torrent
            .get("peerStats")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default();

        let mut result = Vec::new();

        if let Some(peers) = torrent.get("peers").and_then(|v| v.as_array()) {
            let mut ema_cache = self
                .peer_ema_cache
                .lock()
                .unwrap_or_else(|p| p.into_inner());

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

                // ── Instantaneous rates (bytes/s) ────────────────────────────
                let raw_dl = p.get("rateToClient").and_then(|r| r.as_u64()).unwrap_or(0) as f64;
                let raw_ul = p.get("rateToPeer").and_then(|r| r.as_u64()).unwrap_or(0) as f64;

                // ── flagStr — Transmission peer connection flags ──────────────
                // Common flags (case-sensitive):
                //   D  = currently downloading (interested+unchoked)
                //   d  = peer interested but we are choking them
                //   E  = encrypted connection (from flagStr; isEncrypted may differ)
                //   H  = peer was discovered via DHT
                //   X  = peer was discovered via PEX
                //   T  = connection is uTP (micro-transport protocol)
                //   K  = peer is not interested in us (not choking)
                //   O  = optimistic-unchoked
                //   U  = uploading to peer
                let flag_str = p.get("flagStr").and_then(|f| f.as_str()).unwrap_or("");
                let flag_dht = flag_str.contains('H');
                let flag_pex = flag_str.contains('X');
                // isEncrypted / isUTP are more reliable than flagStr on older
                // daemons; use them as primary source and fall back to flags.
                let is_encrypted = p
                    .get("isEncrypted")
                    .and_then(|e| e.as_bool())
                    .unwrap_or_else(|| flag_str.contains('E'));
                let is_utp = p
                    .get("isUTP")
                    .and_then(|u| u.as_bool())
                    .unwrap_or_else(|| flag_str.contains('T'));

                // ── Cumulative bytes from peerStats ──────────────────────────
                let stats = peer_stats_arr.get(idx).copied();
                let total_dl = stats
                    .and_then(|s| s.get("toClientBytes"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let total_ul = stats
                    .and_then(|s| s.get("toPeerBytes"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                // ── EMA update ───────────────────────────────────────────────
                let cache_key = format!("{torrent_hash}:{address}");
                let ema = ema_cache.entry(cache_key).or_insert(PeerEmaState {
                    ema_dl_bps: raw_dl,
                    ema_ul_bps: raw_ul,
                    ema_dl_variance: 0.0,
                    observation_count: 0,
                });

                let prev_dl = ema.ema_dl_bps;
                ema.ema_dl_bps = EMA_ALPHA * raw_dl + (1.0 - EMA_ALPHA) * ema.ema_dl_bps;
                ema.ema_ul_bps = EMA_ALPHA * raw_ul + (1.0 - EMA_ALPHA) * ema.ema_ul_bps;
                // Variance proxy: EMA of |new_sample - previous_ema|
                let dl_delta = (raw_dl - prev_dl).abs();
                ema.ema_dl_variance =
                    EMA_ALPHA * dl_delta + (1.0 - EMA_ALPHA) * ema.ema_dl_variance;
                ema.observation_count = ema.observation_count.saturating_add(1);

                let sustained_dl = ema.ema_dl_bps as u64;
                let sustained_ul = ema.ema_ul_bps as u64;

                // ── Stability score ──────────────────────────────────────────
                // Encode as 1.0 - normalized_variance. Variance is normalized
                // against the sustained DL rate so a peer with very low DL is
                // not penalized harshly for small absolute fluctuations.
                // After MIN_OBSERVATIONS_FOR_STABILITY cycles we have enough
                // history; before that we default to neutral 0.5.
                let stability_score = if ema.observation_count >= MIN_OBSERVATIONS_FOR_STABILITY {
                    let norm_variance = if sustained_dl > 0 {
                        (ema.ema_dl_variance / ema.ema_dl_bps.max(1.0)).clamp(0.0, 1.0) as f32
                    } else {
                        0.0 // peer with zero DL has no measurable variance
                    };
                    (1.0 - norm_variance).clamp(0.0, 1.0)
                } else {
                    0.5 // not enough history yet
                };

                // ── Upload reciprocity ───────────────────────────────────────
                // Ratio of peer's upload-to-us relative to our upload-to-peer.
                // 1.0 = purely seeding to us. 0.0 = we upload only, they never do.
                let upload_reciprocity = if sustained_ul > 0 {
                    (sustained_dl as f32 / (sustained_dl + sustained_ul) as f32).clamp(0.0, 1.0)
                } else if sustained_dl > 0 {
                    1.0 // they upload to us and we upload nothing — perfect leech
                } else {
                    0.5 // both zero — neutral
                };

                // ── Geo + locality tier ──────────────────────────────────────
                let (locality_tier, country_code, peer_asn) = if let Some(geo) = &self.geo_resolver
                {
                    let tier = geo.classify_locality(&address);
                    let cc = geo.country_code(&address);
                    let asn = geo.asn(&address);
                    (tier, cc, asn)
                } else {
                    // Graceful degradation: LAN/Global only
                    let is_lan = crate::utils::geo::is_local_ip(&address);
                    (
                        if is_lan {
                            LocalityTier::Lan
                        } else {
                            LocalityTier::Global
                        },
                        None,
                        None,
                    )
                };

                // ── RTT prober ───────────────────────────────────────────────
                // Trigger an async probe (fire-and-forget, rate-limited).
                // The result is available on the *next* call once the probe
                // completes. First call for a new peer returns rtt_ms=0.
                let rtt_ms = self.rtt_prober.get_rtt(&address, port).unwrap_or(0);
                // Read probe_rtt_enabled from DB at call time (cheap: single
                // SQLite read, cached behind the DB mutex).
                let probe_enabled = self
                    .db
                    .get_setting("peering_settings_json")
                    .ok()
                    .flatten()
                    .and_then(|j| {
                        serde_json::from_str::<crate::models::peering::PeeringSettings>(&j).ok()
                    })
                    .map(|s| s.probe_rtt_enabled)
                    .unwrap_or(false);
                if probe_enabled {
                    self.rtt_prober.probe(&address, port);
                }

                result.push(PeerTelemetry {
                    peer_id: format!("peer-{idx}-{address}"),
                    ip: address.clone(),
                    port,
                    rtt_ms,
                    req_resp_latency_ms: 0,
                    sustained_dl_bps: sustained_dl * 8, // bytes → bits per second
                    sustained_ul_bps: sustained_ul * 8,
                    upload_reciprocity,
                    stability_score,
                    disconnect_count: 0,
                    timeout_count: 0,
                    choke_events: 0,
                    client_name,
                    locality_tier,
                    country_code,
                    asn: peer_asn,
                    utp: is_utp,
                    encryption: is_encrypted,
                    ipv6: address.contains(':'),
                    dht: flag_dht,
                    pex: flag_pex,
                    connection_age_seconds: 0, // not exposed by Transmission RPC
                    total_downloaded_bytes: total_dl,
                    total_uploaded_bytes: total_ul,
                });
            }
        }

        Ok(result)
    }

    fn check_port_open(&self) -> Option<bool> {
        self.rpc_client.port_test().ok()
    }

    fn apply_peering_settings(
        &self,
        settings: &crate::models::peering::PeeringSettings,
    ) -> Result<(), String> {
        // Push peer connection limits to the running Transmission session.
        // These are session-level settings — they persist until the daemon
        // restarts and are re-applied on engine startup via saved PeeringSettings.
        let result = self.rpc_client.session_set(json!({
            "peer-limit-per-torrent": settings.max_peers_per_torrent,
            "peer-limit-global":      settings.max_global_peers,
        }));
        match result {
            Ok(()) => {
                info!(
                    "[peering] applied peer limits to Transmission session: \
                     per-torrent={}, global={}",
                    settings.max_peers_per_torrent, settings.max_global_peers
                );
                Ok(())
            }
            Err(e) => {
                // Non-fatal: DB is already updated. Log and return error so
                // the frontend can show a partial-success notification.
                warn!("[peering] session-set peer limits failed: {e}");
                Err(format!(
                    "Peer limits saved but could not be applied to engine: {e}"
                ))
            }
        }
    }

    fn apply_global_settings(
        &self,
        settings: &crate::commands::settings::UserSettings,
    ) -> Result<(), String> {
        use serde_json::json;

        let cache_mb = if settings.disk_cache_size_mib <= 0 {
            4i32
        } else {
            settings.disk_cache_size_mib
        };
        let idle_seeding_enabled = settings.max_seeding_time_minutes >= 0;
        let idle_seeding_limit = settings.max_seeding_time_minutes.max(0) as u32;
        let ratio_enabled = settings.max_ratio_limit >= 0.0;
        let seed_queue = settings
            .max_active_torrents
            .saturating_sub(settings.max_active_downloads);

        // Derive peer + unchoke values from Racing Mode or user settings
        let (peer_per_torrent, peer_global, unchoke_slots, cache_mb_eff, stall_minutes) =
            if settings.racing_mode_enabled {
                (150u32, 1000u32, 14u32, 256i32, 2u32)
            } else {
                (
                    settings.max_connections_per_torrent.max(1),
                    settings.max_global_connections.max(1),
                    settings.max_upload_slots_per_torrent.max(1),
                    cache_mb,
                    30u32,
                )
            };

        let script_enabled = settings.run_external_program_on_finished
            && !settings.external_program_command.trim().is_empty();

        // 1. Update settings.json on disk so disk-only keys (upload-slots-per-torrent,
        // preallocation, watch-dir) and restart values are preserved.
        let sp = self.config_dir.join("settings.json");
        if let Ok(raw) = std::fs::read_to_string(&sp) {
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
                v["peer-limit-per-torrent"] = json!(peer_per_torrent);
                v["peer-limit-global"] = json!(peer_global);
                v["upload-slots-per-torrent"] = json!(unchoke_slots);
                v["cache-size-mb"] = json!(cache_mb_eff);
                v["preallocation"] = json!(if settings.preallocate_disk_space {
                    1u8
                } else {
                    0u8
                });
                v["incomplete-dir"] = json!(&settings.temp_incomplete_directory);
                v["incomplete-dir-enabled"] = json!(settings.use_temp_incomplete_dir);
                v["watch-dir"] = json!(&settings.watched_folder_directory);
                v["watch-dir-enabled"] = json!(false);
                v["default-trackers"] = json!(&settings.default_trackers_list);
                v["speed-limit-down"] = json!(settings.max_download_speed_kib);
                v["speed-limit-down-enabled"] = json!(settings.max_download_speed_kib > 0);
                v["speed-limit-up"] = json!(settings.max_upload_speed_kib);
                v["speed-limit-up-enabled"] = json!(settings.max_upload_speed_kib > 0);
                v["alt-speed-enabled"] = json!(settings.alt_rate_limits_enabled);
                v["alt-speed-down"] = json!(settings.alt_download_speed_kib);
                v["alt-speed-up"] = json!(settings.alt_upload_speed_kib);
                v["download-queue-size"] = json!(settings.max_active_downloads);
                v["download-queue-enabled"] = json!(settings.max_active_downloads > 0);
                v["seed-queue-size"] = json!(seed_queue.max(1));
                v["seed-queue-enabled"] = json!(settings.max_active_torrents > 0);
                v["queue-stalled-enabled"] = json!(settings.ignore_slow_torrents_in_queue);
                v["queue-stalled-minutes"] = json!(stall_minutes);
                v["seedRatioLimit"] = json!(settings.max_ratio_limit.max(0.0));
                v["seedRatioLimited"] = json!(ratio_enabled);
                v["idle-seeding-limit"] = json!(idle_seeding_limit);
                v["idle-seeding-limit-enabled"] = json!(idle_seeding_enabled);
                v["script-torrent-done-enabled"] = json!(script_enabled);
                v["script-torrent-done-filename"] = json!(&settings.external_program_command);
                let _ = std::fs::write(&sp, serde_json::to_string_pretty(&v).unwrap_or_default());
            }
        }

        // 2. Push supported session-set keys to the live running engine.
        self.rpc_client.session_set(json!({
            // Peer connectivity
            "peer-limit-per-torrent":   peer_per_torrent,
            "peer-limit-global":        peer_global,

            // Disk cache (throughput-critical)
            "cache-size-mb":            cache_mb_eff,

            // Storage
            "incomplete-dir":           settings.temp_incomplete_directory,
            "incomplete-dir-enabled":   settings.use_temp_incomplete_dir,

            // Default trackers
            "default-trackers":         settings.default_trackers_list,

            // Speed limits
            "speed-limit-down":         settings.max_download_speed_kib,
            "speed-limit-down-enabled": settings.max_download_speed_kib > 0,
            "speed-limit-up":           settings.max_upload_speed_kib,
            "speed-limit-up-enabled":   settings.max_upload_speed_kib > 0,

            // Turtle Mode
            "alt-speed-enabled":        settings.alt_rate_limits_enabled,
            "alt-speed-down":           settings.alt_download_speed_kib,
            "alt-speed-up":             settings.alt_upload_speed_kib,

            // Queue management
            "download-queue-size":      settings.max_active_downloads,
            "download-queue-enabled":   settings.max_active_downloads > 0,
            "seed-queue-size":          seed_queue.max(1),
            "seed-queue-enabled":       settings.max_active_torrents > 0,
            "queue-stalled-enabled":    settings.ignore_slow_torrents_in_queue,
            "queue-stalled-minutes":    stall_minutes,

            // Seeding ratio
            "seedRatioLimit":           settings.max_ratio_limit.max(0.0),
            "seedRatioLimited":         ratio_enabled,

            // Idle seeding cutoff
            "idle-seeding-limit":           idle_seeding_limit,
            "idle-seeding-limit-enabled":   idle_seeding_enabled,

            // External completion script
            "script-torrent-done-enabled":   script_enabled,
            "script-torrent-done-filename":  settings.external_program_command,
        }))?;

        info!(
            "[settings] session applied — racing={} peers_per_torrent={} global_peers={} \
             unchoke_slots={} cache_mib={} script_enabled={}",
            settings.racing_mode_enabled,
            peer_per_torrent,
            peer_global,
            unchoke_slots,
            cache_mb_eff,
            script_enabled,
        );

        // 3. Synchronize existing torrents according to active policies:
        // - Downloading torrents: apply/remove Racing mode based on settings.racing_mode_enabled.
        // - Seeding torrents: apply/remove Super seeding based on settings.super_seeding_enabled,
        //   and ensure racing-specific bandwidthPriority / peer-limit overrides are removed.
        let user_peer_limit = settings.max_connections_per_torrent.max(1);
        if let Ok(arr) = self.rpc_client.torrent_get(None) {
            for t in arr {
                let id = t.get("id").and_then(|v| v.as_i64());
                let hash = t.get("hashString").and_then(|v| v.as_str());
                let status_num = t.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
                let prio = t
                    .get("bandwidthPriority")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let peer_limit = t.get("peer-limit").and_then(|v| v.as_i64()).unwrap_or(0);

                let Some(id_val) = id else { continue };
                let id_str = id_val.to_string();

                // Status 3 (Download Wait) or 4 (Downloading)
                if status_num == 3 || status_num == 4 {
                    if settings.racing_mode_enabled {
                        if prio != 1 || peer_limit < 150 {
                            let _ = self.rpc_client.torrent_set(
                                &id_str,
                                serde_json::json!({
                                    "peer-limit": 150i64,
                                    "bandwidthPriority": 1i64,
                                    "honorsSessionLimits": true,
                                }),
                            );
                        }
                    } else if prio == 1 && peer_limit >= 150 {
                        let _ = self.rpc_client.torrent_set(
                            &id_str,
                            serde_json::json!({
                                "peer-limit": user_peer_limit as i64,
                                "bandwidthPriority": 0i64,
                                "honorsSessionLimits": true,
                            }),
                        );
                    }
                } else if status_num == 5 || status_num == 6 {
                    // Seeding torrents: remove any downloading racing overrides
                    if prio == 1 && peer_limit >= 150 {
                        let _ = self.rpc_client.torrent_set(
                            &id_str,
                            serde_json::json!({
                                "peer-limit": user_peer_limit as i64,
                                "bandwidthPriority": 0i64,
                                "honorsSessionLimits": true,
                            }),
                        );
                    }
                    if let Some(h) = hash {
                        let _ = self
                            .db
                            .set_torrent_super_seeding(h, settings.super_seeding_enabled);
                    }
                }
            }
        }

        Ok(())
    }

    fn set_torrent_racing_mode(&self, id: &str, enabled: bool) -> Result<(), String> {
        use serde_json::json;

        // When disabling Racing Mode, restore the user's actual configured peer limit
        // from DB settings rather than an arbitrary default.
        let user_peer_limit = self
            .db
            .get_setting("user_settings_json")
            .ok()
            .flatten()
            .and_then(|raw| {
                serde_json::from_str::<crate::commands::settings::UserSettings>(&raw).ok()
            })
            .map(|s| s.max_connections_per_torrent.max(1))
            .unwrap_or(100);

        if enabled {
            // Preserve session limits: global upload cap, alternate speed policy,
            // and queue limits must remain honored even when Racing Mode preset is active.
            self.rpc_client.torrent_set(
                id,
                json!({
                    "peer-limit":          150i64,
                    "bandwidthPriority":   1i64,
                    "honorsSessionLimits": true,
                }),
            )?;
            info!("[racing_mode] enabled on torrent {id}");
        } else {
            self.rpc_client.torrent_set(
                id,
                json!({
                    "peer-limit":          user_peer_limit as i64,
                    "bandwidthPriority":   0i64,
                    "honorsSessionLimits": true,
                }),
            )?;
            info!("[racing_mode] disabled on torrent {id} (restored peer-limit={user_peer_limit})");
        }
        Ok(())
    }

    fn engine_version(&self) -> Option<String> {
        self.daemon_version.lock().unwrap().clone()
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
        // 1: Queued to check -> "Queued for checking"
        // 2: Checking -> "Checking"
        // 3: Queued to download, 4: Downloading -> "Downloading"
        // 5: Queued to seed, 6: Seeding -> "Seeding"
        let map_status = |s: i64| match s {
            0 => "Paused",
            1 => "Queued for checking",
            2 => "Checking",
            3 | 4 => "Downloading",
            5 | 6 => "Seeding",
            _ => "Paused",
        };

        assert_eq!(map_status(0), "Paused");
        assert_eq!(map_status(1), "Queued for checking");
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
            max_connections_per_torrent: 150, // clamp(60,150) → 150
            max_global_connections: 600,      // clamp(250,1000) → 600
            max_upload_slots_per_torrent: 10, // max(4) → 10
            racing_mode_enabled: false,       // test non-racing path
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

        // Clamping and mapping table (non-Racing Mode path)
        assert_eq!(config["peer-port"], 52123);
        assert_eq!(config["port-forwarding-enabled"], true);
        assert_eq!(config["peer-limit-per-torrent"], 150); // clamped to 150 max
        assert_eq!(config["peer-limit-global"], 600); // within clamp(250,1000)
        assert_eq!(config["upload-slots-per-torrent"], 10); // max(4) → 10
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

    #[test]
    fn test_racing_mode_distinguishes_independent_high_priority() {
        // High priority (1) but standard peer limit (50) is NOT Racing Mode preset
        let normal_high_prio = json!({
            "id": 1,
            "hashString": "abc1",
            "name": "Normal High Priority",
            "bandwidthPriority": 1,
            "peer-limit": 50,
        });
        let bp1 = normal_high_prio
            .get("bandwidthPriority")
            .and_then(|v| v.as_i64())
            .map(|p| p as i8);
        let pl1 = normal_high_prio
            .get("peer-limit")
            .and_then(|v| v.as_i64())
            .map(|p| p as u32);
        let is_racing1 = bp1 == Some(1) && pl1.unwrap_or(0) >= 150;
        assert!(
            !is_racing1,
            "Independently high-priority torrent must not be misidentified as Racing Mode"
        );

        // High priority (1) and aggressive peer limit (>= 150) IS Racing Mode preset
        let racing_torrent = json!({
            "id": 2,
            "hashString": "abc2",
            "name": "Racing Preset Torrent",
            "bandwidthPriority": 1,
            "peer-limit": 150,
        });
        let bp2 = racing_torrent
            .get("bandwidthPriority")
            .and_then(|v| v.as_i64())
            .map(|p| p as i8);
        let pl2 = racing_torrent
            .get("peer-limit")
            .and_then(|v| v.as_i64())
            .map(|p| p as u32);
        let is_racing2 = bp2 == Some(1) && pl2.unwrap_or(0) >= 150;
        assert!(
            is_racing2,
            "Torrent with both high priority and peer-limit >= 150 is recognized as Racing Mode"
        );

        // Normal priority (0) with high peer limit (150) is NOT Racing Mode preset
        let normal_prio_high_peers = json!({
            "id": 3,
            "hashString": "abc3",
            "name": "Normal Prio High Peers",
            "bandwidthPriority": 0,
            "peer-limit": 150,
        });
        let bp3 = normal_prio_high_peers
            .get("bandwidthPriority")
            .and_then(|v| v.as_i64())
            .map(|p| p as i8);
        let pl3 = normal_prio_high_peers
            .get("peer-limit")
            .and_then(|v| v.as_i64())
            .map(|p| p as u32);
        let is_racing3 = bp3 == Some(1) && pl3.unwrap_or(0) >= 150;
        assert!(
            !is_racing3,
            "Normal priority torrent must not be recognized as Racing Mode"
        );
    }

    #[test]
    fn test_racing_mode_preserves_session_limits_and_restores_user_limits() {
        let user_settings = UserSettings {
            max_connections_per_torrent: 45,
            max_global_connections: 180,
            max_upload_slots_per_torrent: 6,
            disk_cache_size_mib: 64,
            max_upload_speed_kib: 512,
            racing_mode_enabled: false,
            run_external_program_on_finished: false,
            external_program_command: String::new(),
            ..Default::default()
        };

        // When Racing Mode is OFF, user configured limits are respected without arbitrary clamps
        let off_config = build_transmission_settings(
            &user_settings,
            "/downloads",
            "0.0.0.0",
            9091,
            "nebulae",
            "pass",
        );
        assert_eq!(off_config["peer-limit-per-torrent"], 45);
        assert_eq!(off_config["peer-limit-global"], 180);
        assert_eq!(off_config["upload-slots-per-torrent"], 6);
        assert_eq!(off_config["cache-size-mb"], 64);
        assert_eq!(off_config["queue-stalled-minutes"], 30);
        assert_eq!(off_config["speed-limit-up"], 512);
        assert_eq!(off_config["speed-limit-up-enabled"], true);
        assert_eq!(off_config["watch-dir-enabled"], false);
        assert_eq!(off_config["script-torrent-done-enabled"], false);

        // When Racing Mode is ON, tuning preset overrides are applied
        let mut racing_settings = user_settings.clone();
        racing_settings.racing_mode_enabled = true;
        let on_config = build_transmission_settings(
            &racing_settings,
            "/downloads",
            "0.0.0.0",
            9091,
            "nebulae",
            "pass",
        );
        assert_eq!(on_config["peer-limit-per-torrent"], 150);
        assert_eq!(on_config["peer-limit-global"], 1000);
        assert_eq!(on_config["upload-slots-per-torrent"], 14);
        assert_eq!(on_config["cache-size-mb"], 256);
        assert_eq!(on_config["queue-stalled-minutes"], 2);
        // Upload cap is preserved even when Racing Mode is active
        assert_eq!(on_config["speed-limit-up"], 512);
        assert_eq!(on_config["speed-limit-up-enabled"], true);
    }
}

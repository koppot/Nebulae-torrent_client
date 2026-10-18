use crate::db::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct UserSettings {
    // 1. Behavior / General
    pub language: String,
    pub theme: String,
    pub start_on_boot: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub notify_on_download_finished: bool,
    pub notify_on_torrent_added: bool,
    pub inhibit_sleep_when_active: bool,
    pub confirm_torrent_deletion: bool,
    pub confirm_exit_when_active: bool,
    pub save_log_to_disk: bool,
    pub max_log_size_mib: u32,
    pub auto_update_enabled: bool,

    // Splash Community Branding
    pub enable_splash: bool,
    pub splash_message: String,
    pub splash_url: String,

    // 2. Downloads
    pub download_directory: String,
    pub recent_download_directories: Vec<String>,
    pub temp_incomplete_directory: String,
    pub use_temp_incomplete_dir: bool,
    pub copy_torrent_files_to: String,
    pub copy_finished_torrents_to: String,
    pub watched_folder_directory: String,
    pub auto_management_mode: bool,
    pub do_not_start_automatically: bool,
    pub show_torrent_content_dialog: bool,
    pub append_neb_extension: bool,
    pub preallocate_disk_space: bool,
    pub run_external_program_on_finished: bool,
    pub external_program_command: String,

    // 3. Connection
    pub peer_connection_protocol: String,
    pub listening_port: u16,
    pub randomize_port_on_startup: bool,
    pub enable_upnp: bool,
    pub max_global_connections: u32,
    pub max_connections_per_torrent: u32,
    pub max_global_upload_slots: u32,
    pub max_upload_slots_per_torrent: u32,
    pub proxy_type: String,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_auth_enabled: bool,
    pub proxy_username: String,
    pub proxy_password: String,
    pub proxy_peer_connections: bool,
    pub proxy_torrents: bool,
    pub proxy_rss: bool,
    pub proxy_disable_unsupported: bool,
    pub ip_filtering_enabled: bool,
    pub ip_filter_path: String,
    pub ip_filter_apply_to_trackers: bool,

    // 4. Speed
    pub max_download_speed_kib: u32,
    pub max_upload_speed_kib: u32,
    pub alt_download_speed_kib: u32,
    pub alt_upload_speed_kib: u32,
    pub alt_rate_limits_enabled: bool,
    pub alt_schedule_enabled: bool,
    pub alt_schedule_from_time: String,
    pub alt_schedule_to_time: String,
    pub alt_schedule_days: String,
    pub rate_limit_utp: bool,
    pub rate_limit_overhead: bool,
    pub rate_limit_lan_peers: bool,

    // 5. BitTorrent
    pub enable_dht: bool,
    pub enable_pex: bool,
    pub enable_lsd: bool,
    pub encryption_mode: String,
    pub anonymous_mode: bool,
    pub max_active_downloads: u32,
    pub max_active_uploads: u32,
    pub max_active_torrents: u32,
    pub ignore_slow_torrents_in_queue: bool,
    pub slow_torrent_dl_threshold_kib: u32,
    pub slow_torrent_ul_threshold_kib: u32,
    pub max_ratio_limit: f32,
    pub max_seeding_time_minutes: i32,
    pub seeding_limit_action: String,
    pub default_trackers_list: String,

    // 6. RSS
    pub rss_auto_refresh_enabled: bool,
    pub rss_refresh_interval_minutes: u32,
    pub rss_max_articles_per_feed: u32,
    pub rss_auto_downloader_enabled: bool,

    // 7. Web UI
    pub webui_enabled: bool,
    pub webui_address: String,
    pub webui_port: u16,
    pub webui_username: String,
    pub webui_password: String,
    pub webui_https_enabled: bool,
    pub webui_cert_path: String,
    pub webui_key_path: String,
    pub webui_bypass_localhost: bool,

    // 8. Advanced
    pub network_interface_binding: String,
    pub optional_ip_address_binding: String,
    pub disk_cache_size_mib: i32,
    pub disk_cache_ttl_seconds: u32,
    pub async_io_threads: u32,
    pub file_pool_size: u32,
    pub stop_tracker_timeout_seconds: u32,
    pub outgoing_ports_min: u16,
    pub outgoing_ports_max: u16,
    pub coalesce_reads_writes: bool,
    pub embedded_tracker_enabled: bool,
    pub embedded_tracker_port: u16,

    // 9. rTorrent Server (Nebulae Server bridge) — applies on app restart
    pub rtorrent_server_enabled: bool,
    pub rtorrent_server_url: String,
    pub rtorrent_server_token: String,

    // 10. Engine Selection ("transmission" | "librqbit")
    pub engine: String,

    // 11. Internal bookkeeping
    /// Version of Nebulae that last persisted these settings. When a new
    /// install takes over, settings are re-persisted with the new version
    /// and any newly applied defaults.
    pub app_version: Option<String>,

    // 12. Racing Mode tuning preset
    /// When enabled, Transmission is configured with aggressive peer limits,
    /// 256 MiB disk cache, 14 unchoke slots, 2-minute queue stall threshold,
    /// and added torrents receive high bandwidth priority.
    #[serde(default)]
    pub racing_mode_enabled: bool,

    // 13. Engine Identification
    /// Selected engine identity profile: "transmission" (default) | "nebulae" | "custom"
    #[serde(default = "default_engine_identity_profile")]
    pub engine_identity_profile: String,
    /// Custom User-Agent string (used when engine_identity_profile == "custom")
    #[serde(default)]
    pub custom_user_agent: String,
    /// Custom Peer ID prefix (8 chars, e.g. "-NE0220-", used when profile == "custom")
    #[serde(default)]
    pub custom_peer_id_prefix: String,
}

fn default_engine_identity_profile() -> String {
    "transmission".to_string()
}

impl Default for UserSettings {
    fn default() -> Self {
        let default_download = dirs::download_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        Self {
            // 1. Behavior
            language: "en-US".to_string(),
            theme: "dark".to_string(),
            start_on_boot: false,
            start_minimized: false,
            minimize_to_tray: true,
            close_to_tray: true,
            notify_on_download_finished: true,
            notify_on_torrent_added: true,
            inhibit_sleep_when_active: true,
            confirm_torrent_deletion: true,
            confirm_exit_when_active: true,
            save_log_to_disk: true,
            max_log_size_mib: 10,
            auto_update_enabled: true,
            enable_splash: true,
            splash_message: "Consider joining our community\nasiandvdclub.org".to_string(),
            splash_url: "https://asiandvdclub.org".to_string(),

            // 2. Downloads
            download_directory: default_download.clone(),
            recent_download_directories: vec![default_download],
            temp_incomplete_directory: "".to_string(),
            use_temp_incomplete_dir: false,
            copy_torrent_files_to: "".to_string(),
            copy_finished_torrents_to: "".to_string(),
            watched_folder_directory: "".to_string(),
            auto_management_mode: true,
            do_not_start_automatically: false,
            show_torrent_content_dialog: true,
            append_neb_extension: true,
            preallocate_disk_space: false,
            run_external_program_on_finished: false,
            external_program_command: "".to_string(),

            // 3. Connection
            peer_connection_protocol: "Both".to_string(),
            // Nebulae-specific default: avoids binding conflicts with a
            // standalone Transmission install (stock default 51413).
            listening_port: 41413,
            randomize_port_on_startup: false,
            enable_upnp: true,
            max_global_connections: 500,
            max_connections_per_torrent: 100,
            max_global_upload_slots: 8,
            max_upload_slots_per_torrent: 14,
            proxy_type: "None".to_string(),
            proxy_host: "".to_string(),
            proxy_port: 8080,
            proxy_auth_enabled: false,
            proxy_username: "".to_string(),
            proxy_password: "".to_string(),
            proxy_peer_connections: true,
            proxy_torrents: true,
            proxy_rss: true,
            proxy_disable_unsupported: false,
            ip_filtering_enabled: false,
            ip_filter_path: "".to_string(),
            ip_filter_apply_to_trackers: false,

            // 4. Speed
            max_download_speed_kib: 0,
            max_upload_speed_kib: 0,
            alt_download_speed_kib: 1024,
            alt_upload_speed_kib: 256,
            alt_rate_limits_enabled: false,
            alt_schedule_enabled: false,
            alt_schedule_from_time: "08:00".to_string(),
            alt_schedule_to_time: "20:00".to_string(),
            alt_schedule_days: "Every day".to_string(),
            rate_limit_utp: true,
            rate_limit_overhead: false,
            rate_limit_lan_peers: false,

            // 5. BitTorrent
            enable_dht: true,
            enable_pex: true,
            enable_lsd: true,
            encryption_mode: "Prefer".to_string(),
            anonymous_mode: false,
            max_active_downloads: 3,
            max_active_uploads: 3,
            max_active_torrents: 5,
            ignore_slow_torrents_in_queue: false,
            slow_torrent_dl_threshold_kib: 2,
            slow_torrent_ul_threshold_kib: 2,
            max_ratio_limit: -1.0,
            max_seeding_time_minutes: -1,
            seeding_limit_action: "Pause".to_string(),
            default_trackers_list: "".to_string(),

            // 6. RSS
            rss_auto_refresh_enabled: true,
            rss_refresh_interval_minutes: 30,
            rss_max_articles_per_feed: 50,
            rss_auto_downloader_enabled: false,

            // 7. Web UI
            webui_enabled: false,
            webui_address: "127.0.0.1".to_string(),
            webui_port: 8080,
            webui_username: "admin".to_string(),
            webui_password: "nebulae".to_string(),
            webui_https_enabled: false,
            webui_cert_path: "".to_string(),
            webui_key_path: "".to_string(),
            webui_bypass_localhost: true,

            // 8. Advanced
            network_interface_binding: "Any interface".to_string(),
            optional_ip_address_binding: "All IPv4 and IPv6 addresses".to_string(),
            disk_cache_size_mib: -1,
            disk_cache_ttl_seconds: 60,
            async_io_threads: 4,
            file_pool_size: 100,
            stop_tracker_timeout_seconds: 5,
            outgoing_ports_min: 0,
            outgoing_ports_max: 0,
            coalesce_reads_writes: true,
            embedded_tracker_enabled: false,
            embedded_tracker_port: 9000,

            // 9. rTorrent Server
            rtorrent_server_enabled: false,
            rtorrent_server_url: String::new(),
            rtorrent_server_token: String::new(),

            // 10. Engine Selection
            engine: "transmission".to_string(),

            // 11. Internal bookkeeping
            app_version: None,

            // 12. Racing Mode tuning preset: false by default to prevent silent
            // activation on existing installs and keep upstream defaults.
            racing_mode_enabled: false,

            // 13. Engine Identification
            engine_identity_profile: "transmission".to_string(),
            custom_user_agent: "".to_string(),
            custom_peer_id_prefix: "".to_string(),
        }
    }
}

#[tauri::command]
pub fn load_user_settings(db: State<Arc<Database>>) -> Result<UserSettings, String> {
    let mut settings = UserSettings::default();

    if let Ok(Some(val)) = db.get_setting("user_settings_json") {
        if let Ok(mut parsed) = serde_json::from_str::<UserSettings>(&val) {
            let mut migrated = false;
            if parsed.engine.trim() == "librqbit" {
                parsed.engine = "transmission".to_string();
                migrated = true;
            }
            // Migrate the stock Transmission default port to the Nebulae-
            // specific default so existing installs move off a port a
            // standalone Transmission client on the same machine may claim.
            if parsed.listening_port == 51413 {
                parsed.listening_port = 41413;
                migrated = true;
            }
            // Persist the migration so every reader of the raw stored
            // settings (connectivity doctor, status bar) sees the same
            // values without each one re-deriving it.
            if migrated {
                if let Ok(json) = serde_json::to_string(&parsed) {
                    let _ = db.set_setting("user_settings_json", &json);
                }
            }
            settings = parsed;
        }
    }

    Ok(settings)
}

#[tauri::command]
pub fn save_user_settings(
    mut settings: UserSettings,
    db: State<Arc<Database>>,
    engine: State<crate::engine::SharedEngine>,
) -> Result<(), String> {
    if settings.engine.trim() == "librqbit" {
        settings.engine = "transmission".to_string();
    }
    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    db.set_setting("user_settings_json", &json)
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    apply_autostart(settings.start_on_boot, settings.start_minimized)?;

    // Apply all session settings to the live engine immediately so the user
    // does not have to restart the daemon for changes to take effect.
    let _ = engine.apply_global_settings(&settings);

    Ok(())
}

/// Make "Start Nebulae on OS startup" actually work (owner report: the
/// toggle did nothing — nothing consumed the flag).
/// - macOS: a LaunchAgent plist with RunAtLoad.
/// - Windows: the HKCU Run registry key (hidden console via `reg`).
/// - Linux: an XDG autostart .desktop entry.
/// `start_minimized` passes a `--minimized` flag the app honours at boot.
pub fn apply_autostart(enabled: bool, minimized: bool) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Cannot resolve the app executable: {e}"))?
        .to_string_lossy()
        .to_string();

    #[cfg(target_os = "macos")]
    {
        let dirs = dirs::home_dir().ok_or("no home dir")?;
        let dir = dirs.join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir).map_err(|e| format!("LaunchAgents dir: {e}"))?;
        let plist = dir.join("com.asiandvdclub.nebulae.plist");
        if !enabled {
            let _ = std::fs::remove_file(&plist);
            return Ok(());
        }
        let minimized_arg = if minimized {
            "  <string>--minimized</string>\n"
        } else {
            ""
        };
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>com.asiandvdclub.nebulae</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{exe}</string>{minimized_arg}  </array>\n  <key>RunAtLoad</key><true/>\n</dict>\n</plist>\n"
        );
        std::fs::write(&plist, xml).map_err(|e| format!("Write plist: {e}"))?;
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", "gui/com.asiandvdclub.nebulae"])
            .output();
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        if !enabled {
            let _ = std::process::Command::new("reg")
                .args(["delete", key, "/v", "Nebulae", "/f"])
                .creation_flags_hidden();
            return Ok(());
        }
        let value = if minimized {
            format!("\"{exe}\" --minimized")
        } else {
            format!("\"{exe}\"")
        };
        let out = std::process::Command::new("reg")
            .args([
                "add", key, "/v", "Nebulae", "/t", "REG_SZ", "/d", &value, "/f",
            ])
            .creation_flags_hidden()
            .output()
            .map_err(|e| format!("reg add: {e}"))?;
        if !out.status.success() {
            return Err("Failed to write the Run registry key".into());
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let dirs = dirs::home_dir().ok_or("no home dir")?;
        let dir = dirs.join(".config/autostart");
        std::fs::create_dir_all(&dir).map_err(|e| format!("autostart dir: {e}"))?;
        let desktop = dir.join("nebulae.desktop");
        if !enabled {
            let _ = std::fs::remove_file(&desktop);
            return Ok(());
        }
        let minimized_env = if minimized { " --minimized" } else { "" };
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=Nebulae\nExec={exe}{minimized_env}\nX-GNOME-Autostart-enabled=true\n"
        );
        std::fs::write(&desktop, content).map_err(|e| format!("Write desktop entry: {e}"))
    }
}

#[cfg(target_os = "windows")]
trait CreationFlagsHidden {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}
#[cfg(target_os = "windows")]
impl CreationFlagsHidden for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000)
    }
}

/// Active engine mode for this session, set once at startup and exposed to
/// the GUI so it can hide options the active engine cannot fulfill.
#[derive(Debug, Clone)]
pub struct EngineMode(pub String);

/// `"local"` (Transmission) or `"rtorrent_server"`.
#[tauri::command]
pub fn get_engine_mode(mode: State<EngineMode>) -> String {
    mode.0.clone()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatusResponse {
    pub name: String,
    pub version: String,
    pub engine: String,
    pub kill_switch_engaged: bool,
}

/// Probe a Nebulae Server instance from the settings screen. Returns the
/// server's status payload so the UI can show what it actually reached.
#[tauri::command]
pub fn test_rtorrent_server_connection(
    url: String,
    token: String,
) -> Result<ServerStatusResponse, String> {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Server address is empty".to_string());
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err("Server address must start with http:// or https://".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let response = client
        .get(format!("{base}/api/v1/status"))
        .bearer_auth(token.trim())
        .send()
        .map_err(|e| format!("Connection failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized: the API token does not match".to_string());
        }
        return Err(format!("Server returned HTTP {status}"));
    }
    response
        .json::<ServerStatusResponse>()
        .map_err(|e| format!("Unexpected server response: {e}"))
}

#[tauri::command]
pub fn get_engine_identity_info(
    db: State<Arc<Database>>,
    app: tauri::AppHandle,
) -> Result<crate::engine::identity::EngineIdentityInfo, String> {
    use tauri::Manager;
    let settings = load_user_settings(db.clone())?;

    let app_version = crate::engine::identity::get_app_version();
    let nebulae_ua = crate::engine::identity::derive_nebulae_user_agent(app_version);
    let nebulae_prefix = crate::engine::identity::derive_nebulae_peer_id_prefix(app_version);

    let tr_version = crate::engine::identity::get_bundled_transmission_version().to_string();
    let tr_ua = crate::engine::identity::get_bundled_transmission_user_agent();
    let tr_prefix = crate::engine::identity::get_bundled_transmission_peer_id_prefix();

    // Check daemon capabilities
    let res_dir = app.path().resource_dir().ok();
    let daemon_path_res =
        crate::engine::transmission::find_transmission_daemon_binary(res_dir.as_deref());
    let (_daemon_found, supports_overrides) = match daemon_path_res {
        Ok(path) => (true, crate::engine::identity::check_daemon_identity_support(&path)),
        Err(_) => (false, false),
    };

    let is_remote = settings.rtorrent_server_enabled;

    let (configured_ua, configured_prefix) = match settings.engine_identity_profile.as_str() {
        "nebulae" => (nebulae_ua.clone(), nebulae_prefix.clone()),
        "custom" => (
            if settings.custom_user_agent.trim().is_empty() {
                nebulae_ua.clone()
            } else {
                settings.custom_user_agent.clone()
            },
            if settings.custom_peer_id_prefix.trim().is_empty() {
                nebulae_prefix.clone()
            } else {
                settings.custom_peer_id_prefix.clone()
            },
        ),
        _ => (tr_ua.clone(), tr_prefix.clone()),
    };

    // Check what identity the running engine was spawned with
    let spawned_info = db
        .get_setting("spawned_engine_identity")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());

    let (active_ua, active_prefix, status_message) = if is_remote {
        (
            "rTorrent/remote".to_string(),
            "-RT----".to_string(),
            "Remote rTorrent server is active. Engine identity is managed server-side and cannot be overridden locally.".to_string(),
        )
    } else if !supports_overrides {
        (
            tr_ua.clone(),
            tr_prefix.clone(),
            if settings.engine_identity_profile == "transmission" {
                "Transmission upstream engine active.".to_string()
            } else {
                "Bundled transmission-daemon is stock upstream and lacks runtime identity override hooks. Upstream Transmission identity is active. Apply Nebulae build patch to enable identity overrides.".to_string()
            },
        )
    } else if let Some(info) = spawned_info {
        let spawned_ua = info
            .get("user_agent")
            .and_then(|v| v.as_str())
            .unwrap_or(&tr_ua)
            .to_string();
        let spawned_prefix = info
            .get("peer_id_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or(&tr_prefix)
            .to_string();
        let spawned_profile = info
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("transmission");

        let msg = match spawned_profile {
            "nebulae" => format!("Native Nebulae v{} engine identity active.", app_version),
            "custom" => "Custom engine identity active.".to_string(),
            _ => "Transmission upstream engine identity active.".to_string(),
        };
        (spawned_ua, spawned_prefix, msg)
    } else {
        match settings.engine_identity_profile.as_str() {
            "nebulae" => (
                nebulae_ua.clone(),
                nebulae_prefix.clone(),
                format!("Native Nebulae v{} engine identity active.", app_version),
            ),
            "custom" => (
                configured_ua.clone(),
                configured_prefix.clone(),
                "Custom engine identity active.".to_string(),
            ),
            _ => (
                tr_ua.clone(),
                tr_prefix.clone(),
                "Transmission upstream engine identity active.".to_string(),
            ),
        }
    };

    let restart_required = !is_remote
        && supports_overrides
        && (configured_ua != active_ua || configured_prefix != active_prefix);

    Ok(crate::engine::identity::EngineIdentityInfo {
        profile: settings.engine_identity_profile,
        configured_user_agent: configured_ua,
        configured_peer_id_prefix: configured_prefix,
        active_user_agent: active_ua,
        active_peer_id_prefix: active_prefix,
        bundled_transmission_version: tr_version,
        bundled_transmission_user_agent: tr_ua,
        bundled_transmission_peer_id_prefix: tr_prefix,
        nebulae_version: app_version.to_string(),
        nebulae_user_agent: nebulae_ua,
        nebulae_peer_id_prefix: nebulae_prefix,
        supports_runtime_overrides: supports_overrides && !is_remote,
        restart_required,
        status_message,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_settings_default_and_roundtrip() {
        let defaults = UserSettings::default();
        let serialized = serde_json::to_string(&defaults).expect("Serialization must succeed");
        let deserialized: UserSettings =
            serde_json::from_str(&serialized).expect("Deserialization must succeed");

        assert_eq!(defaults.language, deserialized.language);
        assert_eq!(defaults.listening_port, deserialized.listening_port);
        assert_eq!(
            defaults.max_global_connections,
            deserialized.max_global_connections
        );
        assert_eq!(defaults.encryption_mode, deserialized.encryption_mode);
        assert_eq!(defaults.webui_port, deserialized.webui_port);
        assert_eq!(defaults.engine, deserialized.engine);
        assert!(deserialized.auto_update_enabled);
    }

    #[test]
    fn test_backward_compatible_partial_json_deserialization() {
        // Minimal json with only 2 fields
        let minimal_json = r#"{"download_directory": "/tmp/custom", "listening_port": 6889}"#;
        let deserialized: UserSettings = serde_json::from_str(minimal_json)
            .expect("Should deserialize with defaults for missing fields");

        assert_eq!(deserialized.download_directory, "/tmp/custom");
        assert_eq!(deserialized.listening_port, 6889);
        assert_eq!(deserialized.encryption_mode, "Prefer");
        assert_eq!(deserialized.max_global_connections, 500);
        assert_eq!(deserialized.engine, "transmission");
        assert!(!deserialized.racing_mode_enabled);
        assert!(deserialized.auto_update_enabled);
    }

    #[test]
    fn test_racing_mode_migration_preserves_false_and_defaults_missing() {
        // Existing user settings without racing_mode_enabled must default to false
        let missing_json = r#"{"download_directory": "/tmp/custom"}"#;
        let deserialized: UserSettings = serde_json::from_str(missing_json).unwrap();
        assert!(
            !deserialized.racing_mode_enabled,
            "Missing racing_mode_enabled must default to false for existing users"
        );

        // Explicit false must be preserved
        let false_json = r#"{"racing_mode_enabled": false}"#;
        let deserialized_false: UserSettings = serde_json::from_str(false_json).unwrap();
        assert!(
            !deserialized_false.racing_mode_enabled,
            "Explicit false must be preserved"
        );

        // Explicit true must be preserved
        let true_json = r#"{"racing_mode_enabled": true}"#;
        let deserialized_true: UserSettings = serde_json::from_str(true_json).unwrap();
        assert!(
            deserialized_true.racing_mode_enabled,
            "Explicit true must be preserved"
        );
    }
}

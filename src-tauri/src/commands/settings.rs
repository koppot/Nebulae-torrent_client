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
            listening_port: 51413,
            randomize_port_on_startup: false,
            enable_upnp: true,
            max_global_connections: 500,
            max_connections_per_torrent: 100,
            max_global_upload_slots: 8,
            max_upload_slots_per_torrent: 4,
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
        }
    }
}

#[tauri::command]
pub fn load_user_settings(db: State<Arc<Database>>) -> Result<UserSettings, String> {
    let mut settings = UserSettings::default();

    if let Ok(Some(val)) = db.get_setting("user_settings_json") {
        if let Ok(parsed) = serde_json::from_str::<UserSettings>(&val) {
            settings = parsed;
        }
    }

    Ok(settings)
}

#[tauri::command]
pub fn save_user_settings(settings: UserSettings, db: State<Arc<Database>>) -> Result<(), String> {
    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    db.set_setting("user_settings_json", &json)
        .map_err(|e| format!("Failed to save settings: {e}"))?;

    Ok(())
}

/// Active engine mode for this session, set once at startup and exposed to
/// the GUI so it can hide options the active engine cannot fulfill.
#[derive(Debug, Clone)]
pub struct EngineMode(pub String);

/// `"local"` (librqbit or mock) or `"rtorrent_server"`.
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
        assert!(deserialized.auto_update_enabled);
    }
}

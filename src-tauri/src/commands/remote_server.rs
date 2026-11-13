//! Remote server proxy commands.
//!
//! All calls to the nebulae-server REST API go through here. The API token is
//! read from the settings database on every call and never sent to the
//! WebView / JavaScript layer. The frontend sends no credentials — it only
//! invokes these commands with torrent IDs and action parameters.
//!
//! Security model:
//!   - Token stays in Rust process memory and the SQLite settings DB.
//!   - WebView receives only the response payload (torrent list, status).
//!   - No CORS relaxation is needed — requests originate from native code.
//!   - Tauri's capability system already restricts which commands the WebView
//!     can call; `remote_server_*` commands are allowlisted in tauri.conf.json.

use crate::commands::settings::{load_user_settings, ServerStatusResponse};
use crate::db::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

// ── Shared HTTP client helper ─────────────────────────────────────────────

fn make_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))
}

/// Retrieve the configured server base URL and bearer token from settings.
/// Returns Err if remote mode is not enabled or the URL is blank.
fn get_remote_credentials(db: &State<Arc<Database>>) -> Result<(String, String), String> {
    let settings = load_user_settings(db.clone())?;
    if !settings.rtorrent_server_enabled {
        return Err("Remote server is not enabled in Settings.".to_string());
    }
    let url = settings
        .rtorrent_server_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    if url.is_empty() {
        return Err("Remote server URL is not configured.".to_string());
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Remote server URL must start with http:// or https://".to_string());
    }
    let token = settings.rtorrent_server_token.trim().to_string();
    Ok((url, token))
}

fn classify_http_error(status: reqwest::StatusCode) -> String {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        "auth_error: API token rejected — check Settings → Server.".to_string()
    } else {
        format!("http_error:{}", status.as_u16())
    }
}

// ── Response types (mirror nebulae-server API) ────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteTorrentFileItem {
    pub index: u32,
    pub name: String,
    pub size_bytes: u64,
    pub progress: f32,
    pub priority: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteTorrentTrackerItem {
    pub tier: u32,
    pub url: String,
    pub status: String,
    pub seeds: u32,
    pub peers: u32,
    pub downloaded: u64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteTorrentItem {
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
    pub super_seeding: bool,
    pub tags: Vec<String>,
    pub files: Vec<RemoteTorrentFileItem>,
    pub trackers: Vec<RemoteTorrentTrackerItem>,
    pub download_limit_kib: u32,
    pub upload_limit_kib: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteKillSwitchStatus {
    pub engaged: bool,
}

// ── Commands ──────────────────────────────────────────────────────────────

/// Fetch the remote server status (name, version, engine, kill_switch_engaged).
/// The token is read from the settings DB and not exposed to the caller.
#[tauri::command]
pub fn remote_server_status(db: State<Arc<Database>>) -> Result<ServerStatusResponse, String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .get(format!("{url}/api/v1/status"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    let status = res.status();
    if !status.is_success() {
        return Err(classify_http_error(status));
    }
    res.json::<ServerStatusResponse>()
        .map_err(|e| format!("parse_error:{e}"))
}

/// Fetch the full torrent list from the remote server.
#[tauri::command]
pub fn remote_list_torrents(db: State<Arc<Database>>) -> Result<Vec<RemoteTorrentItem>, String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .get(format!("{url}/api/v1/torrents"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    let status = res.status();
    if !status.is_success() {
        return Err(classify_http_error(status));
    }
    res.json::<Vec<RemoteTorrentItem>>()
        .map_err(|e| format!("parse_error:{e}"))
}

/// Pause a single remote torrent by its info-hash ID.
#[tauri::command]
pub fn remote_pause_torrent(db: State<Arc<Database>>, id: String) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/{id}/pause"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Resume a single remote torrent by its info-hash ID.
#[tauri::command]
pub fn remote_resume_torrent(db: State<Arc<Database>>, id: String) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/{id}/resume"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Pause all remote torrents.
#[tauri::command]
pub fn remote_pause_all(db: State<Arc<Database>>) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/pause-all"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Resume all remote torrents.
#[tauri::command]
pub fn remote_resume_all(db: State<Arc<Database>>) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/resume-all"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Remove a remote torrent. Set `delete_files` to also delete downloaded data.
#[tauri::command]
pub fn remote_remove_torrent(
    db: State<Arc<Database>>,
    id: String,
    delete_files: bool,
) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .delete(format!(
            "{url}/api/v1/torrents/{id}?delete_files={delete_files}"
        ))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Engage or reset the remote kill switch.
#[tauri::command]
pub fn remote_set_kill_switch(
    db: State<Arc<Database>>,
    engaged: bool,
) -> Result<RemoteKillSwitchStatus, String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let body = serde_json::json!({ "engaged": engaged });
    let res = client
        .post(format!("{url}/api/v1/kill-switch"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    res.json::<RemoteKillSwitchStatus>()
        .map_err(|e| format!("parse_error:{e}"))
}

/// Reannounce a single remote torrent.
#[tauri::command]
pub fn remote_reannounce_torrent(db: State<Arc<Database>>, id: String) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/{id}/reannounce"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Force-recheck a single remote torrent.
#[tauri::command]
pub fn remote_recheck_torrent(db: State<Arc<Database>>, id: String) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let res = client
        .post(format!("{url}/api/v1/torrents/{id}/recheck"))
        .bearer_auth(&token)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

/// Set per-torrent speed limits on the remote server.
#[tauri::command]
pub fn remote_set_torrent_limits(
    db: State<Arc<Database>>,
    id: String,
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
) -> Result<(), String> {
    let (url, token) = get_remote_credentials(&db)?;
    let client = make_client()?;
    let body = serde_json::json!({
        "dl_kib": dl_kib,
        "ul_kib": ul_kib,
        "ratio_limit": ratio_limit,
    });
    let res = client
        .post(format!("{url}/api/v1/torrents/{id}/limits"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .map_err(|e| format!("connection_error:{e}"))?;
    if !res.status().is_success() {
        return Err(classify_http_error(res.status()));
    }
    Ok(())
}

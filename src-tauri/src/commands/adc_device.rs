use crate::commands::adc_download::fetch_adc_torrent_bytes;
use crate::commands::inbox::show_os_notification;
use crate::db::Database;
use crate::engine::traits::TorrentEngine;
use crate::utils::keyring::{get_secret, passkey_secret_target};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, State};

/// Setting key in SQLite settings table storing the persistent device identifier.
pub const SETTING_ADC_DEVICE_ID: &str = "adc_device_id";

/// Base endpoint for AsianDVDClub tracker API.
pub const ADC_BASE_URL: &str = "https://asiandvdclub.org";

/// Device information structure surfaced to Tauri frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdcDeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub client_type: String,
    pub is_linked: bool,
}

/// Delivery item returned by `POST /api/nebulae/delivery/poll`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryItem {
    pub queue_id: u64,
    pub torrent_id: u64,
    pub request_token: String,
    pub name: Option<String>,
    pub info_hash: Option<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeliveryPollResponse {
    pub deliveries: Option<Vec<DeliveryItem>>,
}

/// Returns a clean hostname string suitable for slugifying or display.
pub fn get_clean_hostname() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("hostname").output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(val) = std::env::var("COMPUTERNAME") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    if let Ok(val) = std::env::var("HOSTNAME").or_else(|_| std::env::var("HOST")) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    "nebulae-device".to_string()
}

/// Derive human-readable device name from OS hostname and OS family.
/// E.g. "SUPERMICRO (Windows)" or "MacBook-Pro (macOS)"
pub fn get_device_name() -> String {
    let host = get_clean_hostname();
    let os_name = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    format!("{host} ({os_name})")
}

/// Retrieve existing stable device ID from settings, or generate and persist a new one.
///
/// Format: `<hostname_slug>-<uuid32>` (constrained to <= 64 chars to strictly fit MySQL schema).
pub fn get_or_create_device_id(db: &Database) -> String {
    if let Ok(Some(existing)) = db.get_setting(SETTING_ADC_DEVICE_ID) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let host = get_clean_hostname();
    let host_slug: String = host
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    let short_host = if host_slug.len() > 28 {
        &host_slug[..28]
    } else if host_slug.is_empty() {
        "nebulae"
    } else {
        &host_slug
    };

    let random_uuid = uuid::Uuid::new_v4().simple().to_string(); // exactly 32 hex chars
    let new_id = format!("{short_host}-{random_uuid}"); // max 28 + 1 + 32 = 61 chars <= 64 chars

    let _ = db.set_setting(SETTING_ADC_DEVICE_ID, &new_id);
    new_id
}

/// Synchronously register this device with the ADC tracker API (`POST /api/nebulae/devices/register`).
pub fn register_device_sync(db: &Database) -> Result<String, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Err(
            "ADC account is not linked. Open Tracker Integration to link your account.".to_string(),
        );
    }

    let passkey = match get_secret(&passkey_secret_target()) {
        Ok(pk) if !pk.trim().is_empty() => pk.trim().to_string(),
        _ => {
            return Err(
                "ADC passkey is not available in Keychain. Please re-link your ADC account."
                    .to_string(),
            );
        }
    };

    let device_id = get_or_create_device_id(db);
    let device_name = get_device_name();

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let endpoint = format!("{ADC_BASE_URL}/api/nebulae/devices/register");
    let payload = serde_json::json!({
        "passkey": passkey,
        "device_id": device_id,
        "device_name": device_name,
        "client_type": "local",
        "priority": 10
    });

    let resp = client
        .post(&endpoint)
        .json(&payload)
        .send()
        .map_err(|e| format!("Could not connect to ADC device registry: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("ADC device registration rate limit reached. Retrying later.".to_string());
    }

    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("ADC passkey was rejected by tracker during device registration.".to_string());
    }

    if !status.is_success() {
        return Err(format!(
            "ADC device registration returned unexpected HTTP status {status}."
        ));
    }

    tracing::info!(
        device_id = %device_id,
        device_name = %device_name,
        "[adc-device] Successfully registered device with AsianDVDClub"
    );

    Ok(device_id)
}

/// Polls for incoming one-click download deliveries, downloads metainfo, adds to engine, and acknowledges.
pub async fn poll_and_process_deliveries(
    app: &tauri::AppHandle,
    engine: &Arc<dyn TorrentEngine>,
    db: &Arc<Database>,
) -> Result<usize, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Ok(0);
    }

    let passkey = match get_secret(&passkey_secret_target()) {
        Ok(pk) if !pk.trim().is_empty() => pk.trim().to_string(),
        _ => return Ok(0),
    };

    let device_id = get_or_create_device_id(db);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let poll_endpoint = format!("{ADC_BASE_URL}/api/nebulae/delivery/poll");
    let poll_payload = serde_json::json!({
        "passkey": passkey,
        "device_id": device_id
    });

    let resp = client
        .post(&poll_endpoint)
        .json(&poll_payload)
        .send()
        .await
        .map_err(|e| format!("Delivery poll network error: {e}"))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Delivery poll rate limited.".to_string());
    }

    if !resp.status().is_success() {
        return Err(format!(
            "Delivery poll returned HTTP status: {}",
            resp.status()
        ));
    }

    let poll_res: DeliveryPollResponse = resp
        .json()
        .await
        .map_err(|e| format!("Delivery poll JSON parse error: {e}"))?;

    let deliveries = poll_res.deliveries.unwrap_or_default();
    if deliveries.is_empty() {
        return Ok(0);
    }

    let mut processed_count = 0;

    for delivery in deliveries {
        let torrent_id = delivery.torrent_id;
        let queue_id = delivery.queue_id;
        let request_token = delivery.request_token;
        let title = delivery
            .name
            .clone()
            .unwrap_or_else(|| format!("Torrent #{torrent_id}"));

        tracing::info!(
            queue_id = queue_id,
            torrent_id = torrent_id,
            title = %title,
            "[adc-delivery] Processing incoming delivery from ADC website"
        );

        // 1. Fetch torrent metainfo bytes using existing fetch_adc_torrent_bytes
        match fetch_adc_torrent_bytes(torrent_id, Arc::clone(db)).await {
            Ok(raw_bytes) => {
                // 2. Add torrent to engine using default directory (no dialog)
                let engine_ref = Arc::clone(engine);
                let add_res = tokio::task::spawn_blocking(move || {
                    engine_ref.add_torrent_from_bytes(&raw_bytes, None)
                })
                .await;

                match add_res {
                    Ok(Ok(torrent_info_hash)) => {
                        tracing::info!(
                            queue_id = queue_id,
                            info_hash = %torrent_info_hash,
                            "[adc-delivery] Torrent added to engine successfully"
                        );

                        // 3. Acknowledge with deliveryAck
                        let ack_endpoint = format!("{ADC_BASE_URL}/api/nebulae/delivery/ack");
                        let ack_payload = serde_json::json!({
                            "passkey": passkey,
                            "device_id": device_id,
                            "queue_id": queue_id,
                            "request_token": request_token,
                            "status": "delivered"
                        });

                        if let Err(e) = client.post(&ack_endpoint).json(&ack_payload).send().await {
                            tracing::warn!(
                                queue_id = queue_id,
                                error = %e,
                                "[adc-delivery] Failed to ack delivery to server"
                            );
                        }

                        // 4. Show OS notification and emit Tauri event
                        show_os_notification(
                            "Nebulae — 1-Click Download",
                            &format!("Added: {title}"),
                        );

                        let _ = app.emit(
                            "nebulae://adc-torrent-added",
                            serde_json::json!({
                                "queue_id": queue_id,
                                "torrent_id": torrent_id,
                                "title": title,
                                "info_hash": torrent_info_hash
                            }),
                        );

                        processed_count += 1;
                    }
                    Ok(Err(e)) => {
                        tracing::error!(
                            queue_id = queue_id,
                            error = %e,
                            "[adc-delivery] Failed to add torrent to engine"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            queue_id = queue_id,
                            error = %e,
                            "[adc-delivery] Thread join error while adding torrent"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    queue_id = queue_id,
                    torrent_id = torrent_id,
                    error = %e,
                    "[adc-delivery] Failed to download torrent metainfo from ADC"
                );
            }
        }
    }

    Ok(processed_count)
}

/// Spawns the background poller loop for ADC device registration and delivery queue polling.
pub fn spawn_delivery_poller(
    app: tauri::AppHandle,
    engine: Arc<dyn TorrentEngine>,
    db: Arc<Database>,
) {
    tauri::async_runtime::spawn(async move {
        // Initial delay to avoid startup contention
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;

        let mut registered = false;
        let mut loop_tick: u64 = 0;

        loop {
            loop_tick = loop_tick.wrapping_add(1);

            let is_linked = db
                .get_setting("adc_linked")
                .unwrap_or(None)
                .map(|v| v == "true")
                .unwrap_or(false);

            if is_linked {
                // Register / heartbeat on startup and every ~10 minutes (30 ticks * 20s)
                if !registered || loop_tick % 30 == 1 {
                    let db_reg = Arc::clone(&db);
                    let reg_res =
                        tokio::task::spawn_blocking(move || register_device_sync(&db_reg)).await;

                    match reg_res {
                        Ok(Ok(_)) => {
                            registered = true;
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("[adc-device] device registration attempt: {e}");
                        }
                        Err(e) => {
                            tracing::error!("[adc-device] registration join error: {e}");
                        }
                    }
                }

                // Poll for pending deliveries
                if let Err(e) = poll_and_process_deliveries(&app, &engine, &db).await {
                    tracing::debug!("[adc-delivery] poll: {e}");
                }
            } else {
                registered = false;
            }

            // Sleep 20 seconds between polling cycles
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        }
    });
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn get_adc_device_info(db: State<'_, Arc<Database>>) -> Result<AdcDeviceInfo, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    let device_id = get_or_create_device_id(&db);
    let device_name = get_device_name();

    Ok(AdcDeviceInfo {
        device_id,
        device_name,
        client_type: "local".to_string(),
        is_linked,
    })
}

#[tauri::command]
pub async fn register_adc_device(db: State<'_, Arc<Database>>) -> Result<String, String> {
    let db_clone = Arc::clone(&db);
    tokio::task::spawn_blocking(move || register_device_sync(&db_clone))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_device_id_lifecycle_and_stability() {
        let db = Database::new(Path::new(":memory:")).expect("create test db");

        // Initially no device ID setting
        assert_eq!(db.get_setting(SETTING_ADC_DEVICE_ID).unwrap(), None);

        // First generation produces non-empty ID and persists
        let id1 = get_or_create_device_id(&db);
        assert!(!id1.is_empty());
        assert!(id1.len() <= 64, "Device ID must fit VARCHAR(64)");
        assert_eq!(
            db.get_setting(SETTING_ADC_DEVICE_ID).unwrap(),
            Some(id1.clone())
        );

        // Second retrieval returns the exact same stable ID
        let id2 = get_or_create_device_id(&db);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_device_id_length_and_charset() {
        let db = Database::new(Path::new(":memory:")).expect("create test db");
        let id = get_or_create_device_id(&db);

        assert!(id.len() <= 64, "Generated device ID exceeds 64 characters");
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Device ID contains invalid characters"
        );
        assert!(
            id.contains('-'),
            "Device ID should contain hyphen separator"
        );
    }

    #[test]
    fn test_device_name_format() {
        let name = get_device_name();
        assert!(!name.trim().is_empty());
        assert!(
            name.contains('(') && name.contains(')'),
            "Device name should format OS in parentheses, got: {name}"
        );
    }

    #[test]
    fn test_unlinked_device_registration_error() {
        let db = Database::new(Path::new(":memory:")).expect("create test db");
        let res = register_device_sync(&db);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("not linked"));
    }
}

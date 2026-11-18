use crate::commands::adc_download::{
    classify_adc_uri, fetch_adc_torrent_bytes, is_valid_delivery_token, AdcUriKind,
    DOWNLOAD_RATE_LIMIT_MSG,
};
#[cfg(not(test))]
use crate::commands::inbox::show_os_notification;
use crate::db::Database;
use crate::engine::metainfo::ParsedTorrent;
use crate::engine::traits::TorrentEngine;
#[cfg(not(test))]
use crate::utils::keyring::{get_secret, passkey_secret_target};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tauri::{Emitter, State};

/// Setting key in SQLite settings table storing the persistent device identifier.
pub const SETTING_ADC_DEVICE_ID: &str = "adc_device_id";

/// Setting key storing the durable local website-delivery state ledger.
pub const SETTING_ADC_DELIVERY_LEDGER: &str = "adc_delivery_ledger_json";

/// Max retained completed ledger entries to keep SQLite state bounded.
const MAX_LEDGER_ENTRIES: usize = 500;

/// Setting key storing per-request failure state so one failed request is
/// reported once and retried with backoff instead of on every poll.
pub const SETTING_ADC_DELIVERY_FAILURES: &str = "adc_delivery_failures_json";
const MAX_FAILURE_ENTRIES: usize = 100;
const RATE_LIMIT_WAIT_SECS: u64 = 31;
const RATE_LIMIT_MAX_WAITS: u32 = 3;
const FAILURE_RETRY_BASE_SECS: i64 = 60;
const FAILURE_RETRY_MAX_SECS: i64 = 900;

/// Serializes poll-driven and URI-driven delivery redemption so concurrent paths
/// never race on the same `request_token`.
static DELIVERY_MUTEX: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Base endpoint for AsianDVDClub tracker API.
pub const ADC_BASE_URL: &str = "https://asiandvdclub.org";

/// Durable per-token local state so interrupted acknowledgements or restarts
/// recover without duplicate engine additions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LocalDeliveryStatus {
    Claimed,
    AddedPendingAck { info_hash: String },
    Delivered { info_hash: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalDeliveryEntry {
    pub account_scope: String,
    pub queue_id: u64,
    pub torrent_id: u64,
    pub status: LocalDeliveryStatus,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeliveryLedger {
    pub entries: HashMap<String, LocalDeliveryEntry>,
}

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
    #[serde(default)]
    pub status: Option<String>,
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

/// Retrieve user's ADC passkey from platform Keychain with fallback to persisted setting.
pub fn get_adc_passkey(db: &Database) -> Option<String> {
    #[cfg(not(test))]
    if let Ok(pk) = get_secret(&passkey_secret_target()) {
        let trimmed = pk.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    db.get_setting("adc_passkey")
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

    let passkey = match get_adc_passkey(db) {
        Some(pk) => pk,
        None => {
            return Err(
                "ADC passkey is not available. Please open Tracker Integration and link your ADC account."
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

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Compute an opaque account+device scope key without embedding the raw passkey.
pub fn compute_account_scope(passkey: &str, device_id: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(passkey.trim().as_bytes());
    hasher.update(b"::");
    hasher.update(device_id.trim().as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn load_delivery_ledger(db: &Database) -> DeliveryLedger {
    db.get_setting(SETTING_ADC_DELIVERY_LEDGER)
        .ok()
        .flatten()
        .and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                serde_json::from_str::<DeliveryLedger>(trimmed).ok()
            }
        })
        .unwrap_or_default()
}

pub fn save_delivery_ledger(db: &Database, ledger: &mut DeliveryLedger) -> Result<(), String> {
    if ledger.entries.len() > MAX_LEDGER_ENTRIES {
        let mut delivered_keys: Vec<(i64, String)> = ledger
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.status, LocalDeliveryStatus::Delivered { .. }))
            .map(|(k, e)| (e.updated_at, k.clone()))
            .collect();
        delivered_keys.sort_by_key(|(ts, _)| *ts);
        let over = ledger.entries.len().saturating_sub(MAX_LEDGER_ENTRIES);
        for (_, key) in delivered_keys.into_iter().take(over) {
            ledger.entries.remove(&key);
        }
    }
    let json =
        serde_json::to_string(ledger).map_err(|e| format!("Serialize delivery ledger: {e}"))?;
    db.set_setting(SETTING_ADC_DELIVERY_LEDGER, &json)
        .map_err(|e| format!("Save delivery ledger: {e}"))
}

pub fn clear_delivery_ledger(db: &Database) {
    let _ = db.set_setting(SETTING_ADC_DELIVERY_LEDGER, "");
}

/// Validate that the user's configured default download destination exists or can
/// be created. Returns an explicit error if unconfigured or unavailable (D12 / D17 / D18).
pub fn validate_configured_download_dir(db: &Database) -> Result<String, String> {
    let settings = crate::commands::settings::load_user_settings_from_db(db);
    let dir = settings.download_directory.trim().to_string();
    if dir.is_empty() {
        return Err(
            "Default download directory is not configured. Set a download folder in Settings -> Downloads."
                .to_string(),
        );
    }
    // The same resolved path is validated and handed to the engine, so a saved
    // `~/Downloads` is never created relative to the process working directory.
    crate::utils::paths::prepare_local_download_dir(&dir)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("Configured download destination '{dir}' is unavailable: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryFailureRecord {
    pub reason: String,
    /// Download destination setting in effect when the attempt failed; a change
    /// means the user may have corrected the problem, so retry immediately.
    pub destination: String,
    pub attempts: u32,
    pub last_attempt_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeliveryFailureLedger {
    pub entries: HashMap<String, DeliveryFailureRecord>,
}

fn load_failure_ledger(db: &Database) -> DeliveryFailureLedger {
    db.get_setting(SETTING_ADC_DELIVERY_FAILURES)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<DeliveryFailureLedger>(raw.trim()).ok())
        .unwrap_or_default()
}

fn save_failure_ledger(db: &Database, ledger: &mut DeliveryFailureLedger) {
    if ledger.entries.len() > MAX_FAILURE_ENTRIES {
        let mut by_age: Vec<(i64, String)> = ledger
            .entries
            .iter()
            .map(|(k, v)| (v.last_attempt_at, k.clone()))
            .collect();
        by_age.sort();
        let over = ledger.entries.len() - MAX_FAILURE_ENTRIES;
        for (_, key) in by_age.into_iter().take(over) {
            ledger.entries.remove(&key);
        }
    }
    if let Ok(json) = serde_json::to_string(ledger) {
        let _ = db.set_setting(SETTING_ADC_DELIVERY_FAILURES, &json);
    }
}

fn current_destination_setting(db: &Database) -> String {
    crate::commands::settings::load_user_settings_from_db(db)
        .download_directory
        .trim()
        .to_string()
}

fn failure_backoff_secs(attempts: u32) -> i64 {
    let shift = attempts.saturating_sub(1).min(10);
    (FAILURE_RETRY_BASE_SECS << shift).min(FAILURE_RETRY_MAX_SECS)
}

/// Whether a background poll should try this request again. A deliberate
/// (user-initiated) attempt always proceeds.
fn should_attempt_delivery(db: &Database, token: &str, deliberate: bool) -> bool {
    if deliberate {
        return true;
    }
    match load_failure_ledger(db).entries.get(token) {
        None => true,
        Some(rec) => {
            rec.destination != current_destination_setting(db)
                || now_unix_secs() - rec.last_attempt_at >= failure_backoff_secs(rec.attempts)
        }
    }
}

/// Records a failure for `token`; returns true when the reason is new for this
/// request (no earlier record, or a different reason) and so worth surfacing.
fn record_delivery_failure(db: &Database, token: &str, reason: &str) -> bool {
    let mut ledger = load_failure_ledger(db);
    let destination = current_destination_setting(db);
    let is_new = match ledger.entries.get_mut(token) {
        Some(rec) if rec.reason == reason => {
            rec.attempts = rec.attempts.saturating_add(1);
            rec.last_attempt_at = now_unix_secs();
            rec.destination = destination;
            false
        }
        _ => {
            ledger.entries.insert(
                token.to_string(),
                DeliveryFailureRecord {
                    reason: reason.to_string(),
                    destination,
                    attempts: 1,
                    last_attempt_at: now_unix_secs(),
                },
            );
            true
        }
    };
    save_failure_ledger(db, &mut ledger);
    is_new
}

fn clear_delivery_failure(db: &Database, token: &str) {
    let mut ledger = load_failure_ledger(db);
    if ledger.entries.remove(token).is_some() {
        save_failure_ledger(db, &mut ledger);
    }
}

pub fn clear_delivery_failures(db: &Database) {
    let _ = db.set_setting(SETTING_ADC_DELIVERY_FAILURES, "");
}

#[allow(clippy::too_many_arguments)]
fn report_delivery_failure(
    app: Option<&tauri::AppHandle>,
    db: &Database,
    token: &str,
    queue_id: Option<u64>,
    torrent_id: Option<u64>,
    title: Option<&str>,
    reason: &str,
    deliberate: bool,
) {
    let is_new = record_delivery_failure(db, token, reason);
    if is_new || deliberate {
        emit_delivery_failure(app, Some(token), queue_id, torrent_id, title, reason);
    } else {
        tracing::debug!(
            queue_id = ?queue_id,
            reason = %reason,
            "[adc-delivery] Repeat failure suppressed; already reported for this request"
        );
    }
}

fn emit_delivery_failure(
    app: Option<&tauri::AppHandle>,
    request_token: Option<&str>,
    queue_id: Option<u64>,
    torrent_id: Option<u64>,
    title: Option<&str>,
    reason: &str,
) {
    tracing::warn!(
        queue_id = ?queue_id,
        torrent_id = ?torrent_id,
        reason = %reason,
        "[adc-delivery] Website delivery failed"
    );
    let notif_body = match title {
        Some(t) if !t.is_empty() => format!("{t}: {reason}"),
        _ => reason.to_string(),
    };
    #[cfg(not(test))]
    show_os_notification("Nebulae — ADC Delivery Failed", &notif_body);
    #[cfg(test)]
    let _ = &notif_body;
    if let Some(app_handle) = app {
        let _ = app_handle.emit(
            "nebulae://adc-delivery-failed",
            serde_json::json!({
                "request_token": request_token,
                "queue_id": queue_id,
                "torrent_id": torrent_id,
                "title": title,
                "reason": reason,
            }),
        );
    }
}

fn emit_delivery_added(
    app: Option<&tauri::AppHandle>,
    queue_id: u64,
    torrent_id: u64,
    title: &str,
    info_hash: &str,
    duplicate: bool,
) {
    let notif_title = if duplicate {
        "Nebulae — Already Added"
    } else {
        "Nebulae — Downloading from ADC"
    };
    let notif_body = if duplicate {
        format!("Already in Nebulae: {title}")
    } else {
        format!("Added: {title}")
    };
    #[cfg(not(test))]
    show_os_notification(notif_title, &notif_body);
    #[cfg(test)]
    let _ = (notif_title, &notif_body);
    if let Some(app_handle) = app {
        let _ = app_handle.emit(
            "nebulae://adc-torrent-added",
            serde_json::json!({
                "queue_id": queue_id,
                "torrent_id": torrent_id,
                "title": title,
                "info_hash": info_hash,
                "duplicate": duplicate,
            }),
        );
    }
}

type DeliveryFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

trait DeliveryTransport: Send + Sync {
    fn poll_token_delivery<'a>(
        &'a self,
        passkey: &'a str,
        device_id: &'a str,
        request_token: &'a str,
    ) -> DeliveryFuture<'a, Result<Vec<DeliveryItem>, String>>;

    fn ack_delivery<'a>(
        &'a self,
        passkey: &'a str,
        device_id: &'a str,
        queue_id: u64,
        request_token: &'a str,
        status: &'a str,
    ) -> DeliveryFuture<'a, Result<(), (u16, String)>>;

    fn fetch_metainfo<'a>(
        &'a self,
        torrent_id: u64,
        db: Arc<Database>,
    ) -> DeliveryFuture<'a, Result<Vec<u8>, String>>;
}

struct LiveDeliveryTransport {
    client: reqwest::Client,
}

impl DeliveryTransport for LiveDeliveryTransport {
    fn poll_token_delivery<'a>(
        &'a self,
        passkey: &'a str,
        device_id: &'a str,
        request_token: &'a str,
    ) -> DeliveryFuture<'a, Result<Vec<DeliveryItem>, String>> {
        Box::pin(async move {
            poll_deliveries_from_server(&self.client, passkey, device_id, Some(request_token)).await
        })
    }

    fn ack_delivery<'a>(
        &'a self,
        passkey: &'a str,
        device_id: &'a str,
        queue_id: u64,
        request_token: &'a str,
        status: &'a str,
    ) -> DeliveryFuture<'a, Result<(), (u16, String)>> {
        Box::pin(async move {
            let ack_endpoint = format!("{ADC_BASE_URL}/api/nebulae/delivery/ack");
            let ack_payload = serde_json::json!({
                "passkey": passkey,
                "device_id": device_id,
                "queue_id": queue_id,
                "request_token": request_token,
                "status": status
            });

            let resp = self
                .client
                .post(&ack_endpoint)
                .json(&ack_payload)
                .send()
                .await
                .map_err(|e| (0u16, format!("Network error acknowledging delivery: {e}")))?;

            let code = resp.status();
            if code.is_success() {
                return Ok(());
            }

            let status_u16 = code.as_u16();
            let err_json = resp.json::<serde_json::Value>().await.ok();
            let err_code = err_json
                .as_ref()
                .and_then(|v| v.get("error").and_then(|s| s.as_str()))
                .unwrap_or("");
            let err_msg = err_json
                .as_ref()
                .and_then(|v| v.get("message").and_then(|s| s.as_str()))
                .unwrap_or("");

            let friendly = match (status_u16, err_code) {
                (403, "device_not_owned") => {
                    "This delivery request belongs to a different device or client.".to_string()
                }
                (403, _) => "ADC rejected account authorization for this delivery.".to_string(),
                (404, _) => {
                    "Delivery request was not found for this ADC account and device.".to_string()
                }
                (409, "already_delivered") => {
                    "Delivery request has already been redeemed and cannot be replayed.".to_string()
                }
                (410, _) => "Delivery request has expired on the tracker.".to_string(),
                _ if !err_msg.is_empty() => err_msg.to_string(),
                _ => format!("Tracker delivery ack failed (HTTP {code})."),
            };

            Err((status_u16, friendly))
        })
    }

    fn fetch_metainfo<'a>(
        &'a self,
        torrent_id: u64,
        db: Arc<Database>,
    ) -> DeliveryFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            fetch_waiting_out_rate_limit(
                || fetch_adc_torrent_bytes(torrent_id, Arc::clone(&db)),
                std::time::Duration::from_secs(RATE_LIMIT_WAIT_SECS),
                RATE_LIMIT_MAX_WAITS,
            )
            .await
        })
    }
}

/// Several queued requests (or two devices on one account) land inside the tracker's
/// 30 s download window; wait it out instead of failing the request.
async fn fetch_waiting_out_rate_limit<F, Fut>(
    mut fetch: F,
    wait: std::time::Duration,
    max_waits: u32,
) -> Result<Vec<u8>, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    let mut waits = 0;
    loop {
        match fetch().await {
            Err(e) if e == DOWNLOAD_RATE_LIMIT_MSG && waits < max_waits => {
                waits += 1;
                tokio::time::sleep(wait).await;
            }
            other => return other,
        }
    }
}

async fn engine_has_torrent(
    engine: &Arc<dyn TorrentEngine>,
    info_hash: &str,
) -> Result<bool, String> {
    let eng = Arc::clone(engine);
    let target = info_hash.to_string();
    tokio::task::spawn_blocking(move || {
        let items = eng.list_torrents()?;
        Ok(items.into_iter().any(|t| {
            t.id.eq_ignore_ascii_case(&target) || t.info_hash.eq_ignore_ascii_case(&target)
        }))
    })
    .await
    .map_err(|e| format!("Engine check join error: {e}"))?
}

struct DeliveryRoute {
    engine: Option<Arc<dyn TorrentEngine>>,
    remote: bool,
}

impl DeliveryRoute {
    fn new(engine: Arc<dyn TorrentEngine>, remote: bool) -> Self {
        Self {
            engine: Some(engine),
            remote,
        }
    }

    fn engine(&self) -> &Arc<dyn TorrentEngine> {
        self.engine
            .as_ref()
            .expect("route engine is set until drop")
    }
}

impl Drop for DeliveryRoute {
    /// The remote engine owns a blocking HTTP client, which must not be released
    /// on an async worker thread.
    fn drop(&mut self) {
        if let Some(engine) = self.engine.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn_blocking(move || drop(engine));
            }
        }
    }
}

/// Chooses where a website delivery is added, from the ADC one-click routing
/// priorities (Settings -> BitTorrent). The remote target uses the Nebulae Server
/// connection from Settings -> Server (Transmission behind nebulae-server) and does
/// not require the app itself to run in Server mode. Unreachable or unconfigured
/// targets cascade to the next enabled one.
async fn resolve_delivery_route(
    db: &Arc<Database>,
    live: &Arc<dyn TorrentEngine>,
) -> Result<DeliveryRoute, String> {
    use crate::commands::adc_download::{
        enabled_delivery_targets, load_client_targets, REMOTE_SERVER_TARGET_ID,
    };

    let order = enabled_delivery_targets(&load_client_targets(db));
    if order.is_empty() {
        return Ok(DeliveryRoute::new(Arc::clone(live), live.is_remote()));
    }

    let mut last_error: Option<String> = None;
    for target in order {
        match target.client_type.as_str() {
            "local" => {
                if live.is_remote() {
                    last_error.get_or_insert_with(|| {
                        "The local Manager engine is not running because Nebulae is in Server mode."
                            .to_string()
                    });
                    continue;
                }
                return Ok(DeliveryRoute::new(Arc::clone(live), false));
            }
            t if t == REMOTE_SERVER_TARGET_ID => {
                if live.is_remote() {
                    return Ok(DeliveryRoute::new(Arc::clone(live), true));
                }
                let settings = crate::commands::settings::load_user_settings_from_db(db);
                let url = settings.rtorrent_server_url.trim().to_string();
                if url.is_empty() {
                    last_error = Some(
                        "Remote server delivery is selected but no Nebulae Server URL is configured (Settings -> Server)."
                            .to_string(),
                    );
                    continue;
                }
                let token = settings.rtorrent_server_token.clone();
                // The engine wraps a blocking HTTP client: build, probe and (on
                // failure) release it off the async worker threads.
                let probed = tokio::task::spawn_blocking(move || {
                    let candidate: Arc<dyn TorrentEngine> =
                        Arc::new(crate::engine::remote::RemoteServerEngine::new(url, token));
                    candidate.list_torrents().map(|_| candidate)
                })
                .await
                .map_err(|e| format!("Remote server check join error: {e}"))?;
                match probed {
                    Ok(candidate) => return Ok(DeliveryRoute::new(candidate, true)),
                    Err(e) => {
                        last_error = Some(format!("Remote server is not reachable: {e}"));
                    }
                }
            }
            _ => {}
        }
    }
    Err(last_error.unwrap_or_else(|| "No enabled ADC delivery target is available.".to_string()))
}
async fn process_single_delivery_item(
    app: Option<&tauri::AppHandle>,
    engine: &Arc<dyn TorrentEngine>,
    db: &Arc<Database>,
    transport: &dyn DeliveryTransport,
    passkey: &str,
    device_id: &str,
    delivery: &DeliveryItem,
) -> Result<String, String> {
    process_delivery_item(
        app, engine, db, transport, passkey, device_id, delivery, false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn process_delivery_item(
    app: Option<&tauri::AppHandle>,
    engine: &Arc<dyn TorrentEngine>,
    db: &Arc<Database>,
    transport: &dyn DeliveryTransport,
    passkey: &str,
    device_id: &str,
    delivery: &DeliveryItem,
    deliberate: bool,
) -> Result<String, String> {
    let torrent_id = delivery.torrent_id;
    let queue_id = delivery.queue_id;
    let token = delivery.request_token.trim().to_ascii_lowercase();
    let title = delivery
        .name
        .clone()
        .unwrap_or_else(|| format!("Torrent #{torrent_id}"));
    let fail = |reason: &str| {
        report_delivery_failure(
            app,
            db,
            &token,
            Some(queue_id),
            Some(torrent_id),
            Some(&title),
            reason,
            deliberate,
        )
    };

    if passkey.trim().len() < 8 || device_id.trim().is_empty() {
        let msg = "ADC account or device identity is missing.".to_string();
        fail(&msg);
        return Err(msg);
    }

    if !is_valid_delivery_token(&token) {
        let msg = "Invalid delivery request token format.".to_string();
        fail(&msg);
        return Err(msg);
    }

    let scope = compute_account_scope(passkey, device_id);
    let mut ledger = load_delivery_ledger(db);

    if let Some(existing) = ledger.entries.get(&token).cloned() {
        if existing.account_scope != scope {
            let msg = "Delivery token belongs to a different ADC account or device.".to_string();
            fail(&msg);
            return Err(msg);
        }

        match existing.status {
            LocalDeliveryStatus::Delivered { info_hash } => {
                // Best-effort idempotent ack if server still sees it pending/claimed.
                if delivery.status.as_deref() != Some("delivered") {
                    let _ = transport
                        .ack_delivery(passkey, device_id, queue_id, &token, "delivered")
                        .await;
                }
                return Ok(info_hash);
            }
            LocalDeliveryStatus::AddedPendingAck { info_hash } => {
                if engine_has_torrent(engine, &info_hash)
                    .await
                    .unwrap_or(false)
                {
                    // Interrupted acknowledgement recovery (D16): never re-download or re-add.
                    match transport
                        .ack_delivery(passkey, device_id, queue_id, &token, "delivered")
                        .await
                    {
                        Ok(()) => {
                            ledger.entries.insert(
                                token.clone(),
                                LocalDeliveryEntry {
                                    account_scope: scope,
                                    queue_id,
                                    torrent_id,
                                    status: LocalDeliveryStatus::Delivered {
                                        info_hash: info_hash.clone(),
                                    },
                                    updated_at: now_unix_secs(),
                                },
                            );
                            let _ = save_delivery_ledger(db, &mut ledger);
                            emit_delivery_added(
                                app, queue_id, torrent_id, &title, &info_hash, false,
                            );
                            return Ok(info_hash);
                        }
                        Err((_code, err)) => {
                            tracing::warn!(
                                queue_id = queue_id,
                                error = %err,
                                "[adc-delivery] Interrupted-ack retry failed; will retry on next poll"
                            );
                            return Err(err);
                        }
                    }
                }
            }
            LocalDeliveryStatus::Claimed => {}
        }
    }

    // A request that already failed is retried with backoff (or at once after the
    // destination setting changes) so a bad configuration cannot spin every poll.
    if !should_attempt_delivery(db, &token, deliberate) {
        return Err("Delivery is waiting to retry after an earlier failure.".to_string());
    }

    let route = match resolve_delivery_route(db, engine).await {
        Ok(r) => r,
        Err(reason) => {
            fail(&reason);
            return Err(reason);
        }
    };
    let engine = route.engine();

    // Validate configured default destination directory before claiming on server (D12 / D17 / D18).
    // A remote server owns its own filesystem, so it receives no local path and uses its default.
    let dest_dir = if route.remote {
        None
    } else {
        match validate_configured_download_dir(db) {
            Ok(d) => Some(d),
            Err(reason) => {
                fail(&reason);
                return Err(reason);
            }
        }
    };

    tracing::info!(
        queue_id = queue_id,
        torrent_id = torrent_id,
        title = %title,
        "[adc-delivery] Claiming website delivery request before metainfo fetch"
    );

    // Atomic server-side claim before downloading metainfo (replay/ownership protection).
    if let Err((_code, reason)) = transport
        .ack_delivery(passkey, device_id, queue_id, &token, "claimed")
        .await
    {
        fail(&reason);
        return Err(reason);
    }

    ledger.entries.insert(
        token.clone(),
        LocalDeliveryEntry {
            account_scope: scope.clone(),
            queue_id,
            torrent_id,
            status: LocalDeliveryStatus::Claimed,
            updated_at: now_unix_secs(),
        },
    );
    let _ = save_delivery_ledger(db, &mut ledger);

    // Fetch authenticated member-specific .torrent bytes from ADC.
    let raw_bytes = match transport.fetch_metainfo(torrent_id, Arc::clone(db)).await {
        Ok(b) => b,
        Err(reason) => {
            fail(&reason);
            return Err(reason);
        }
    };

    let parsed = match ParsedTorrent::parse_bytes(&raw_bytes) {
        Ok(p) => p,
        Err(reason) => {
            fail(&reason);
            return Err(reason);
        }
    };
    let expected_hash = parsed.info_hash.clone();

    // Prevent duplicate torrent addition if info_hash already exists in engine (D14).
    if engine_has_torrent(engine, &expected_hash)
        .await
        .unwrap_or(false)
    {
        let _ = transport
            .ack_delivery(passkey, device_id, queue_id, &token, "delivered")
            .await;
        ledger.entries.insert(
            token.clone(),
            LocalDeliveryEntry {
                account_scope: scope,
                queue_id,
                torrent_id,
                status: LocalDeliveryStatus::Delivered {
                    info_hash: expected_hash.clone(),
                },
                updated_at: now_unix_secs(),
            },
        );
        let _ = save_delivery_ledger(db, &mut ledger);
        clear_delivery_failure(db, &token);
        emit_delivery_added(app, queue_id, torrent_id, &title, &expected_hash, true);
        return Ok(expected_hash);
    }

    // Add to engine with configured destination, start download, and verify presence (D11 / D12 / D15).
    let engine_ref = Arc::clone(engine);
    let dest_clone = dest_dir.clone();
    let expected_clone = expected_hash.clone();
    let remote_target = route.remote;
    let add_res = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let added_hash = engine_ref.add_torrent_from_bytes(&raw_bytes, dest_clone.as_deref())?;
        // The Nebulae Server bridge does not return the new id, so it is identified by the
        // metainfo hash and confirmed against the server's own torrent list.
        if !remote_target && added_hash.trim().is_empty() {
            return Err("Torrent engine returned an empty torrent identifier.".to_string());
        }
        if !remote_target {
            let _ = engine_ref.resume_torrent(&added_hash);
        }
        let matches = |t: &crate::commands::torrent::TorrentItem| {
            t.id.eq_ignore_ascii_case(&added_hash)
                || t.info_hash.eq_ignore_ascii_case(&expected_clone)
        };
        let attempts = if remote_target { 20 } else { 1 };
        for attempt in 0..attempts {
            if engine_ref.list_torrents()?.iter().any(matches) {
                return Ok(expected_clone);
            }
            if attempt + 1 < attempts {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        Err(if remote_target {
            "Remote server accepted the torrent but did not list it yet; retry to confirm."
                .to_string()
        } else {
            "Engine did not report the added torrent in active session list.".to_string()
        })
    })
    .await
    .map_err(|e| format!("Thread join error while adding torrent: {e}"))?;

    let added_hash = match add_res {
        Ok(h) => h,
        Err(reason) => {
            fail(&reason);
            return Err(reason);
        }
    };

    // Persist AddedPendingAck BEFORE sending final delivered ack (D16 recovery).
    ledger.entries.insert(
        token.clone(),
        LocalDeliveryEntry {
            account_scope: scope.clone(),
            queue_id,
            torrent_id,
            status: LocalDeliveryStatus::AddedPendingAck {
                info_hash: added_hash.clone(),
            },
            updated_at: now_unix_secs(),
        },
    );
    let _ = save_delivery_ledger(db, &mut ledger);

    match transport
        .ack_delivery(passkey, device_id, queue_id, &token, "delivered")
        .await
    {
        Ok(()) => {
            ledger.entries.insert(
                token.clone(),
                LocalDeliveryEntry {
                    account_scope: scope,
                    queue_id,
                    torrent_id,
                    status: LocalDeliveryStatus::Delivered {
                        info_hash: added_hash.clone(),
                    },
                    updated_at: now_unix_secs(),
                },
            );
            let _ = save_delivery_ledger(db, &mut ledger);
        }
        Err((_code, ack_err)) => {
            tracing::warn!(
                queue_id = queue_id,
                error = %ack_err,
                "[adc-delivery] Final delivered ack failed after engine add; AddedPendingAck saved for retry"
            );
        }
    }

    clear_delivery_failure(db, &token);
    emit_delivery_added(app, queue_id, torrent_id, &title, &added_hash, false);
    Ok(added_hash)
}

async fn poll_deliveries_from_server(
    client: &reqwest::Client,
    passkey: &str,
    device_id: &str,
    request_token: Option<&str>,
) -> Result<Vec<DeliveryItem>, String> {
    let poll_endpoint = format!("{ADC_BASE_URL}/api/nebulae/delivery/poll");
    let mut poll_payload = serde_json::json!({
        "passkey": passkey,
        "device_id": device_id
    });
    if let Some(tok) = request_token {
        poll_payload["request_token"] = serde_json::json!(tok);
    }

    let resp = client
        .post(&poll_endpoint)
        .json(&poll_payload)
        .send()
        .await
        .map_err(|e| format!("Delivery poll network error: {e}"))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Delivery poll rate limited.".to_string());
    }
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::UNAUTHORIZED
    {
        return Err(
            "ADC rejected authentication or device ownership for this delivery request."
                .to_string(),
        );
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

    Ok(poll_res.deliveries.unwrap_or_default())
}

/// A claim older than the tracker's 24h request TTL can no longer be redeemed.
const RECOVERY_WINDOW_SECS: i64 = 23 * 3600;
const RECOVERY_MAX_PER_CYCLE: usize = 3;
const RECOVERY_RETRY_BASE_SECS: i64 = 60;
const RECOVERY_RETRY_MAX_SECS: i64 = 900;

/// Per-token (attempts, last attempt) so a request the tracker no longer lists is not
/// re-polled every cycle.
static RECOVERY_ATTEMPTS: LazyLock<std::sync::Mutex<HashMap<String, (u32, i64)>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn recovery_due(token: &str, now: i64) -> bool {
    let attempts = RECOVERY_ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    match attempts.get(token) {
        None => true,
        Some((count, last)) => {
            let wait = (RECOVERY_RETRY_BASE_SECS << (*count).min(5)).min(RECOVERY_RETRY_MAX_SECS);
            now - last >= wait
        }
    }
}

fn note_recovery_attempt(token: &str, now: i64, succeeded: bool) {
    let mut attempts = RECOVERY_ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
    if succeeded {
        attempts.remove(token);
    } else {
        let count = attempts.get(token).map(|(c, _)| *c).unwrap_or(0);
        attempts.insert(token.to_string(), (count.saturating_add(1), now));
    }
}

/// Tokens this account/device claimed locally but never finished, oldest first.
fn recoverable_tokens(db: &Database, scope: &str, now: i64) -> Vec<String> {
    let ledger = load_delivery_ledger(db);
    let mut open: Vec<(i64, String)> = ledger
        .entries
        .iter()
        .filter(|(_, e)| {
            e.account_scope == scope
                && !matches!(e.status, LocalDeliveryStatus::Delivered { .. })
                && now - e.updated_at <= RECOVERY_WINDOW_SECS
        })
        .map(|(token, e)| (e.updated_at, token.clone()))
        .collect();
    open.sort();
    open.into_iter().map(|(_, token)| token).collect()
}

/// Resumes requests left half-finished by an interrupted run. The ordinary delivery path
/// already refuses to add a torrent the engine holds and only re-sends the final
/// acknowledgement, so resuming cannot add a duplicate.
async fn recover_interrupted_deliveries(
    app: Option<&tauri::AppHandle>,
    engine: &Arc<dyn TorrentEngine>,
    db: &Arc<Database>,
    transport: &dyn DeliveryTransport,
    passkey: &str,
    device_id: &str,
) -> usize {
    let scope = compute_account_scope(passkey, device_id);
    let now = now_unix_secs();
    let mut recovered = 0;
    let mut tried = 0;

    for token in recoverable_tokens(db, &scope, now) {
        if tried >= RECOVERY_MAX_PER_CYCLE {
            break;
        }
        if !recovery_due(&token, now) {
            continue;
        }
        tried += 1;

        let items = match transport
            .poll_token_delivery(passkey, device_id, &token)
            .await
        {
            Ok(items) => items,
            Err(e) => {
                tracing::debug!("[adc-delivery] recovery poll failed: {e}");
                note_recovery_attempt(&token, now, false);
                continue;
            }
        };
        let Some(item) = items.into_iter().next() else {
            // The tracker no longer lists it as open (delivered elsewhere or expired).
            note_recovery_attempt(&token, now, false);
            continue;
        };

        let ok =
            process_single_delivery_item(app, engine, db, transport, passkey, device_id, &item)
                .await
                .is_ok();
        note_recovery_attempt(&token, now, ok);
        if ok {
            recovered += 1;
        }
    }
    recovered
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

    let passkey = match get_adc_passkey(db) {
        Some(pk) => pk,
        None => return Ok(0),
    };

    let device_id = get_or_create_device_id(db);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let _guard = DELIVERY_MUTEX.lock().await;

    let polled = poll_deliveries_from_server(&client, &passkey, &device_id, None).await;
    let transport = LiveDeliveryTransport { client };
    let mut processed_count = 0;

    // A request this client claimed but never finished (restart, crash, lost ack) is not
    // listed by the plain poll, so it is resumed by token. Runs even when the plain poll
    // was rate limited: token polls are not subject to that limit.
    processed_count +=
        recover_interrupted_deliveries(Some(app), engine, db, &transport, &passkey, &device_id)
            .await;

    let deliveries = polled?;

    for delivery in deliveries {
        if process_single_delivery_item(
            Some(app),
            engine,
            db,
            &transport,
            &passkey,
            &device_id,
            &delivery,
        )
        .await
        .is_ok()
        {
            processed_count += 1;
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
        // Short initial delay to let window and runtime initialize
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

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

/// Extract and validate an opaque 64-hex delivery token from either a raw token
/// string or an `adc://delivery/<token>` / `adc://request/<token>` URI.
/// Manual `adc://torrent/<id>` references are explicitly rejected so they never
/// bypass `AddTorrentModal`.
pub fn extract_delivery_token(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.to_ascii_lowercase().starts_with("adc:") {
        match classify_adc_uri(trimmed)? {
            AdcUriKind::DeliveryRequestToken(tok) => Ok(tok),
            AdcUriKind::ManualTorrentId(_) => Err(
                "Manual adc:// torrent references must be reviewed in Add Torrent and cannot bypass confirmation."
                    .to_string(),
            ),
        }
    } else if is_valid_delivery_token(trimmed) {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err("Invalid ADC delivery request token format.".to_string())
    }
}

async fn redeem_delivery_with_transport(
    app: Option<&tauri::AppHandle>,
    engine: &Arc<dyn TorrentEngine>,
    db: &Arc<Database>,
    transport: &dyn DeliveryTransport,
    request_token_or_uri: &str,
) -> Result<String, String> {
    let token = match extract_delivery_token(request_token_or_uri) {
        Ok(tok) => tok,
        Err(msg) => {
            emit_delivery_failure(app, None, None, None, None, &msg);
            return Err(msg);
        }
    };

    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);
    if !is_linked {
        let msg = "ADC account is not linked. Link your account in Tracker Integration first."
            .to_string();
        report_delivery_failure(app, db, &token, None, None, None, &msg, true);
        return Err(msg);
    }

    let passkey = match get_adc_passkey(db) {
        Some(pk) => pk,
        None => {
            let msg = "ADC passkey is unavailable. Relink your account in Tracker Integration."
                .to_string();
            report_delivery_failure(app, db, &token, None, None, None, &msg, true);
            return Err(msg);
        }
    };

    let device_id = get_or_create_device_id(db);
    let scope = compute_account_scope(&passkey, &device_id);

    // If a poll cycle held the lock, it may have just delivered this same request.
    let (_guard, waited) = match DELIVERY_MUTEX.try_lock() {
        Ok(g) => (g, false),
        Err(_) => (DELIVERY_MUTEX.lock().await, true),
    };

    let ledger = load_delivery_ledger(db);
    if let Some(existing) = ledger.entries.get(&token) {
        if existing.account_scope != scope {
            let msg = "Delivery token belongs to a different ADC account or device.".to_string();
            report_delivery_failure(
                app,
                db,
                &token,
                Some(existing.queue_id),
                Some(existing.torrent_id),
                None,
                &msg,
                true,
            );
            return Err(msg);
        }
        if let LocalDeliveryStatus::Delivered { info_hash } = &existing.status {
            if waited {
                return Ok(info_hash.clone());
            }
            let msg =
                "Delivery request has already been redeemed and cannot be replayed.".to_string();
            report_delivery_failure(
                app,
                db,
                &token,
                Some(existing.queue_id),
                Some(existing.torrent_id),
                None,
                &msg,
                true,
            );
            return Err(msg);
        }
    }

    let deliveries = match transport
        .poll_token_delivery(&passkey, &device_id, &token)
        .await
    {
        Ok(items) => items,
        Err(msg) => {
            report_delivery_failure(app, db, &token, None, None, None, &msg, true);
            return Err(msg);
        }
    };

    let delivery = match deliveries
        .into_iter()
        .find(|d| d.request_token.eq_ignore_ascii_case(&token))
    {
        Some(item) => item,
        None => {
            let msg = "Delivery request was not found, has expired, or belongs to a different ADC account or device."
                .to_string();
            report_delivery_failure(app, db, &token, None, None, None, &msg, true);
            return Err(msg);
        }
    };

    process_delivery_item(
        app, engine, db, transport, &passkey, &device_id, &delivery, true,
    )
    .await
}

/// Redeems an opaque website-created delivery request URI/token (`adc://delivery/<token>`)
/// after authenticating the linked member, verifying device ownership, validating the
/// configured download directory, claiming the queue item server-side, and confirming
/// the torrent is present in the engine.
#[tauri::command]
pub async fn redeem_adc_delivery(
    app: tauri::AppHandle,
    request_token: String,
    engine: State<'_, crate::engine::LocalEngine>,
    db: State<'_, Arc<Database>>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let transport = LiveDeliveryTransport { client };
    redeem_delivery_with_transport(
        Some(&app),
        engine.inner(),
        db.inner(),
        &transport,
        &request_token,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::mock::MockTorrentEngine;
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(1);

    /// Tests that go through `redeem_delivery_with_transport` share the global
    /// delivery lock; running them one at a time keeps the "waited for the lock"
    /// branch deterministic.
    static REDEEM_TEST_SERIAL: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(prefix: &str) -> Self {
            let id = NEXT_TEST_DIR_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nebulae_{prefix}_{}_{}",
                std::process::id(),
                id
            ));
            let _ = std::fs::create_dir_all(&path);
            Self { path }
        }

        fn str_path(&self) -> String {
            self.path.to_string_lossy().to_string()
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn make_valid_torrent_bytes(name: &str, length: u64) -> Vec<u8> {
        let announce = b"https://asiandvdclub.org/announce";
        let mut out = Vec::new();
        out.extend_from_slice(format!("d8:announce{}:", announce.len()).as_bytes());
        out.extend_from_slice(announce);
        out.extend_from_slice(
            format!(
                "4:infod6:lengthi{}e4:name{}:{}12:piece lengthi16384e6:pieces20:aaaaaaaaaaaaaaaaaaaa7:privatei1eee",
                length,
                name.len(),
                name
            )
            .as_bytes(),
        );
        out
    }

    fn sample_token(ch: char) -> String {
        ch.to_string().repeat(64)
    }

    fn setup_linked_db(download_dir: Option<&str>) -> Arc<Database> {
        let db = Arc::new(Database::new(Path::new(":memory:")).expect("create test db"));
        db.set_setting("adc_linked", "true").unwrap();
        db.set_setting("adc_passkey", "0123456789abcdef0123456789abcdef")
            .unwrap();
        if let Some(dir) = download_dir {
            let settings_json = serde_json::json!({ "download_directory": dir }).to_string();
            db.set_setting("user_settings_json", &settings_json)
                .unwrap();
        }
        db
    }

    type AckCallRecord = (String, String, u64, String, String);

    struct MockDeliveryTransport {
        poll_calls: Mutex<Vec<String>>,
        ack_calls: Mutex<Vec<AckCallRecord>>,
        metainfo_calls: Mutex<Vec<u64>>,
        poll_result: Mutex<Result<Vec<DeliveryItem>, String>>,
        claim_result: Mutex<Result<(), (u16, String)>>,
        delivered_results: Mutex<VecDeque<Result<(), (u16, String)>>>,
        metainfo_result: Mutex<Result<Vec<u8>, String>>,
    }

    impl MockDeliveryTransport {
        fn success(item: DeliveryItem, metainfo: Vec<u8>) -> Self {
            Self {
                poll_calls: Mutex::new(Vec::new()),
                ack_calls: Mutex::new(Vec::new()),
                metainfo_calls: Mutex::new(Vec::new()),
                poll_result: Mutex::new(Ok(vec![item])),
                claim_result: Mutex::new(Ok(())),
                delivered_results: Mutex::new(VecDeque::new()),
                metainfo_result: Mutex::new(Ok(metainfo)),
            }
        }
    }

    impl DeliveryTransport for MockDeliveryTransport {
        fn poll_token_delivery<'a>(
            &'a self,
            _passkey: &'a str,
            _device_id: &'a str,
            request_token: &'a str,
        ) -> DeliveryFuture<'a, Result<Vec<DeliveryItem>, String>> {
            self.poll_calls
                .lock()
                .unwrap()
                .push(request_token.to_string());
            let res = self.poll_result.lock().unwrap().clone();
            Box::pin(async move { res })
        }

        fn ack_delivery<'a>(
            &'a self,
            passkey: &'a str,
            device_id: &'a str,
            queue_id: u64,
            request_token: &'a str,
            status: &'a str,
        ) -> DeliveryFuture<'a, Result<(), (u16, String)>> {
            self.ack_calls.lock().unwrap().push((
                passkey.to_string(),
                device_id.to_string(),
                queue_id,
                request_token.to_string(),
                status.to_string(),
            ));
            let res = if status == "claimed" {
                self.claim_result.lock().unwrap().clone()
            } else {
                self.delivered_results
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(()))
            };
            Box::pin(async move { res })
        }

        fn fetch_metainfo<'a>(
            &'a self,
            torrent_id: u64,
            _db: Arc<Database>,
        ) -> DeliveryFuture<'a, Result<Vec<u8>, String>> {
            self.metainfo_calls.lock().unwrap().push(torrent_id);
            let res = self.metainfo_result.lock().unwrap().clone();
            Box::pin(async move { res })
        }
    }

    #[tokio::test]
    async fn test_rate_limited_download_waits_and_retries_then_succeeds() {
        let calls = std::sync::atomic::AtomicU32::new(0);
        let out = fetch_waiting_out_rate_limit(
            || {
                let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err(DOWNLOAD_RATE_LIMIT_MSG.to_string())
                    } else {
                        Ok(vec![1, 2, 3])
                    }
                }
            },
            std::time::Duration::from_millis(1),
            3,
        )
        .await;
        assert_eq!(out, Ok(vec![1, 2, 3]));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_rate_limit_gives_up_after_max_waits_and_other_errors_do_not_wait() {
        let calls = std::sync::atomic::AtomicU32::new(0);
        let out = fetch_waiting_out_rate_limit(
            || {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { Err(DOWNLOAD_RATE_LIMIT_MSG.to_string()) }
            },
            std::time::Duration::from_millis(1),
            2,
        )
        .await;
        assert_eq!(out, Err(DOWNLOAD_RATE_LIMIT_MSG.to_string()));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);

        let calls = std::sync::atomic::AtomicU32::new(0);
        let out = fetch_waiting_out_rate_limit(
            || {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { Err("Torrent #1 was not found on AsianDVDClub.".to_string()) }
            },
            std::time::Duration::from_millis(1),
            3,
        )
        .await;
        assert!(out.is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

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

    #[test]
    fn test_extract_delivery_token_rejects_manual_and_invalid_uris() {
        let tok = sample_token('a');
        assert_eq!(
            extract_delivery_token(&format!("adc://delivery/{tok}")).unwrap(),
            tok
        );
        assert_eq!(
            extract_delivery_token(&format!("adc:request/{}", tok.to_ascii_uppercase())).unwrap(),
            tok
        );
        assert_eq!(extract_delivery_token(&tok).unwrap(), tok);

        let manual_err = extract_delivery_token("adc://torrent/12345").unwrap_err();
        assert!(manual_err.contains("Add Torrent"));
        assert!(extract_delivery_token("adc://12345").is_err());
        assert!(extract_delivery_token("adc://delivery/not-64-hex").is_err());
    }

    #[tokio::test]
    async fn test_delivery_valid_claim_add_verify_and_delivered_flow() {
        let _serial = REDEEM_TEST_SERIAL.lock().await;
        let tmp = TempTestDir::new("delivery_valid");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('b');
        let metainfo = make_valid_torrent_bytes("Valid.DVD.ISO", 2048);
        let expected_hash = ParsedTorrent::parse_bytes(&metainfo).unwrap().info_hash;

        let item = DeliveryItem {
            queue_id: 101,
            torrent_id: 42,
            request_token: token.clone(),
            status: Some("pending".to_string()),
            name: Some("Valid.DVD.ISO".to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport = MockDeliveryTransport::success(item, metainfo);

        let result_hash = redeem_delivery_with_transport(
            None,
            &engine,
            &db,
            &transport,
            &format!("adc://delivery/{token}"),
        )
        .await
        .expect("valid website delivery should succeed");

        assert!(result_hash.eq_ignore_ascii_case(&expected_hash));
        assert_eq!(*transport.poll_calls.lock().unwrap(), vec![token.clone()]);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![42]);

        let ack_statuses: Vec<String> = transport
            .ack_calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.4.clone())
            .collect();
        assert_eq!(ack_statuses, vec!["claimed", "delivered"]);

        let torrents = engine.list_torrents().unwrap();
        assert_eq!(torrents.len(), 1);
        assert!(torrents[0].info_hash.eq_ignore_ascii_case(&expected_hash));

        let ledger = load_delivery_ledger(&db);
        assert_eq!(
            ledger.entries.get(&token).map(|e| &e.status),
            Some(&LocalDeliveryStatus::Delivered {
                info_hash: result_hash,
            })
        );

        // Explicit URI replay must be blocked locally without polling or fetching metainfo again.
        let replay_err = redeem_delivery_with_transport(
            None,
            &engine,
            &db,
            &transport,
            &format!("adc://delivery/{token}"),
        )
        .await
        .unwrap_err();
        assert!(replay_err.contains("already been redeemed"));
        assert_eq!(transport.poll_calls.lock().unwrap().len(), 1);
        assert_eq!(transport.metainfo_calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delivery_rejects_ownership_expiry_and_replay_before_metainfo_fetch() {
        let _serial = REDEEM_TEST_SERIAL.lock().await;
        let tmp = TempTestDir::new("delivery_rejects");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());

        for (case_idx, (http_code, err_msg)) in [
            (
                403u16,
                "This delivery request belongs to a different device or client.",
            ),
            (410u16, "Delivery request has expired on the tracker."),
            (
                409u16,
                "Delivery request has already been redeemed and cannot be replayed.",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let tok = sample_token(char::from_digit((case_idx + 1) as u32, 16).unwrap());
            let item = DeliveryItem {
                queue_id: 200 + case_idx as u64,
                torrent_id: 77,
                request_token: tok.clone(),
                status: Some("pending".to_string()),
                name: Some("Restricted.ISO".to_string()),
                info_hash: None,
                created_at: None,
                expires_at: None,
            };
            let transport =
                MockDeliveryTransport::success(item, make_valid_torrent_bytes("x.iso", 1024));
            *transport.claim_result.lock().unwrap() = Err((http_code, err_msg.to_string()));

            let res = redeem_delivery_with_transport(
                None,
                &engine,
                &db,
                &transport,
                &format!("adc://delivery/{tok}"),
            )
            .await;

            assert_eq!(res.unwrap_err(), err_msg);
            assert!(
                transport.metainfo_calls.lock().unwrap().is_empty(),
                "metainfo must never be fetched when claim returns HTTP {http_code}"
            );
            assert!(engine.list_torrents().unwrap().is_empty());
        }

        // Different account scope on same token must be rejected locally before polling.
        let tok_cross = sample_token('d');
        let mut ledger = load_delivery_ledger(&db);
        ledger.entries.insert(
            tok_cross.clone(),
            LocalDeliveryEntry {
                account_scope: "other-account-scope".to_string(),
                queue_id: 299,
                torrent_id: 77,
                status: LocalDeliveryStatus::Delivered {
                    info_hash: "0000".to_string(),
                },
                updated_at: 1,
            },
        );
        save_delivery_ledger(&db, &mut ledger).unwrap();

        let item = DeliveryItem {
            queue_id: 299,
            torrent_id: 77,
            request_token: tok_cross.clone(),
            status: Some("pending".to_string()),
            name: None,
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport =
            MockDeliveryTransport::success(item, make_valid_torrent_bytes("x.iso", 1024));
        let cross_err = redeem_delivery_with_transport(
            None,
            &engine,
            &db,
            &transport,
            &format!("adc://delivery/{tok_cross}"),
        )
        .await
        .unwrap_err();
        assert!(cross_err.contains("different ADC account"));
        assert!(transport.poll_calls.lock().unwrap().is_empty());
        assert!(transport.metainfo_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_delivery_rejects_missing_or_invalid_destination_before_server_claim() {
        let _serial = REDEEM_TEST_SERIAL.lock().await;
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('e');
        let item = DeliveryItem {
            queue_id: 301,
            torrent_id: 88,
            request_token: token.clone(),
            status: Some("pending".to_string()),
            name: Some("DestCheck.ISO".to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        };

        // 1. Unconfigured download directory (empty string).
        let db_unconfigured = setup_linked_db(Some(""));
        let transport_unconf = MockDeliveryTransport::success(
            item.clone(),
            make_valid_torrent_bytes("DestCheck.ISO", 1024),
        );
        let err1 = redeem_delivery_with_transport(
            None,
            &engine,
            &db_unconfigured,
            &transport_unconf,
            &format!("adc://delivery/{token}"),
        )
        .await
        .unwrap_err();
        assert!(err1.contains("not configured"));
        assert!(transport_unconf.ack_calls.lock().unwrap().is_empty());
        assert!(transport_unconf.metainfo_calls.lock().unwrap().is_empty());

        // 2. Destination path points to a regular file, not a directory.
        let tmp = TempTestDir::new("dest_file");
        let file_path = tmp.path.join("not_a_dir.txt");
        std::fs::write(&file_path, b"occupied").unwrap();
        let db_bad_dir = setup_linked_db(Some(&file_path.to_string_lossy()));
        let transport_bad =
            MockDeliveryTransport::success(item, make_valid_torrent_bytes("DestCheck.ISO", 1024));
        let err2 = redeem_delivery_with_transport(
            None,
            &engine,
            &db_bad_dir,
            &transport_bad,
            &format!("adc://delivery/{token}"),
        )
        .await
        .unwrap_err();
        assert!(err2.contains("not a directory"));
        assert!(transport_bad.ack_calls.lock().unwrap().is_empty());
        assert!(transport_bad.metainfo_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_delivery_interrupted_ack_recovery_retries_only_ack_without_duplicate_add() {
        let tmp = TempTestDir::new("delivery_ack_retry");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('f');
        let metainfo = make_valid_torrent_bytes("Recover.ISO", 4096);
        let expected_hash = ParsedTorrent::parse_bytes(&metainfo).unwrap().info_hash;
        let item = DeliveryItem {
            queue_id: 401,
            torrent_id: 99,
            request_token: token.clone(),
            status: Some("pending".to_string()),
            name: Some("Recover.ISO".to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport = MockDeliveryTransport::success(item.clone(), metainfo);
        *transport.delivered_results.lock().unwrap() =
            VecDeque::from([Err((500u16, "transient server error".to_string())), Ok(())]);

        // First attempt adds torrent to engine, fails final delivered ack, and stores AddedPendingAck.
        let added = process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            "0123456789abcdef0123456789abcdef",
            &get_or_create_device_id(&db),
            &item,
        )
        .await
        .expect("added hash should return while preserving AddedPendingAck");
        assert!(added.eq_ignore_ascii_case(&expected_hash));
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![99]);
        assert!(matches!(
            load_delivery_ledger(&db)
                .entries
                .get(&token)
                .map(|e| &e.status),
            Some(LocalDeliveryStatus::AddedPendingAck { .. })
        ));

        // Second attempt (with server status = claimed) must retry ONLY "delivered" ack.
        let mut claimed_item = item;
        claimed_item.status = Some("claimed".to_string());
        let recovered = process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            "0123456789abcdef0123456789abcdef",
            &get_or_create_device_id(&db),
            &claimed_item,
        )
        .await
        .expect("interrupted ack recovery should succeed");
        assert!(recovered.eq_ignore_ascii_case(&expected_hash));
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![99]);
        let ack_statuses: Vec<String> = transport
            .ack_calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.4.clone())
            .collect();
        assert_eq!(ack_statuses, vec!["claimed", "delivered", "delivered"]);
        assert!(matches!(
            load_delivery_ledger(&db)
                .entries
                .get(&token)
                .map(|e| &e.status),
            Some(LocalDeliveryStatus::Delivered { .. })
        ));
    }

    #[tokio::test]
    async fn test_interrupted_claim_is_resumed_without_duplicate_add() {
        let tmp = TempTestDir::new("delivery_recover_claimed");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('7');
        let passkey = "0123456789abcdef0123456789abcdef";
        let device_id = get_or_create_device_id(&db);
        let metainfo = make_valid_torrent_bytes("Interrupted.ISO", 4096);
        let expected_hash = ParsedTorrent::parse_bytes(&metainfo).unwrap().info_hash;

        // The previous run claimed the request and added the torrent, then stopped
        // before the final acknowledgement.
        engine
            .add_torrent_from_bytes(&metainfo, Some(&tmp.str_path()))
            .unwrap();
        let mut ledger = load_delivery_ledger(&db);
        ledger.entries.insert(
            token.clone(),
            LocalDeliveryEntry {
                account_scope: compute_account_scope(passkey, &device_id),
                queue_id: 501,
                torrent_id: 77,
                status: LocalDeliveryStatus::Claimed,
                updated_at: now_unix_secs(),
            },
        );
        save_delivery_ledger(&db, &mut ledger).unwrap();

        let item = DeliveryItem {
            queue_id: 501,
            torrent_id: 77,
            request_token: token.clone(),
            status: Some("claimed".to_string()),
            name: Some("Interrupted.ISO".to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport = MockDeliveryTransport::success(item, metainfo);

        let recovered =
            recover_interrupted_deliveries(None, &engine, &db, &transport, passkey, &device_id)
                .await;

        assert_eq!(recovered, 1);
        assert_eq!(*transport.poll_calls.lock().unwrap(), vec![token.clone()]);
        assert_eq!(engine.list_torrents().unwrap().len(), 1, "no duplicate add");
        assert_eq!(
            transport
                .ack_calls
                .lock()
                .unwrap()
                .last()
                .map(|c| c.4.clone()),
            Some("delivered".to_string())
        );
        match load_delivery_ledger(&db)
            .entries
            .get(&token)
            .map(|e| &e.status)
        {
            Some(LocalDeliveryStatus::Delivered { info_hash }) => {
                assert!(info_hash.eq_ignore_ascii_case(&expected_hash))
            }
            other => panic!("expected Delivered, got {other:?}"),
        }

        // Finished work is not polled again.
        let polls_before = transport.poll_calls.lock().unwrap().len();
        let again =
            recover_interrupted_deliveries(None, &engine, &db, &transport, passkey, &device_id)
                .await;
        assert_eq!(again, 0);
        assert_eq!(transport.poll_calls.lock().unwrap().len(), polls_before);
    }

    #[tokio::test]
    async fn test_recovery_backs_off_when_tracker_no_longer_lists_the_request() {
        let tmp = TempTestDir::new("delivery_recover_gone");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('8');
        let passkey = "0123456789abcdef0123456789abcdef";
        let device_id = get_or_create_device_id(&db);

        let mut ledger = load_delivery_ledger(&db);
        ledger.entries.insert(
            token.clone(),
            LocalDeliveryEntry {
                account_scope: compute_account_scope(passkey, &device_id),
                queue_id: 502,
                torrent_id: 78,
                status: LocalDeliveryStatus::Claimed,
                updated_at: now_unix_secs(),
            },
        );
        save_delivery_ledger(&db, &mut ledger).unwrap();

        let item = DeliveryItem {
            queue_id: 502,
            torrent_id: 78,
            request_token: token.clone(),
            status: None,
            name: None,
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport = MockDeliveryTransport::success(item, Vec::new());
        *transport.poll_result.lock().unwrap() = Ok(Vec::new());

        for _ in 0..4 {
            let n =
                recover_interrupted_deliveries(None, &engine, &db, &transport, passkey, &device_id)
                    .await;
            assert_eq!(n, 0);
        }
        assert_eq!(
            transport.poll_calls.lock().unwrap().len(),
            1,
            "a request the tracker no longer lists must not be polled every cycle"
        );
        assert!(transport.ack_calls.lock().unwrap().is_empty());
        assert!(engine.list_torrents().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_recovery_ignores_other_accounts_and_stale_claims() {
        let db = setup_linked_db(None);
        let passkey = "0123456789abcdef0123456789abcdef";
        let device_id = get_or_create_device_id(&db);
        let scope = compute_account_scope(passkey, &device_id);
        let now = now_unix_secs();
        let entry = |scope: &str, age: i64, status: LocalDeliveryStatus| LocalDeliveryEntry {
            account_scope: scope.to_string(),
            queue_id: 1,
            torrent_id: 1,
            status,
            updated_at: now - age,
        };
        let mut ledger = load_delivery_ledger(&db);
        ledger.entries.insert(
            sample_token('1'),
            entry(&scope, 10, LocalDeliveryStatus::Claimed),
        );
        ledger.entries.insert(
            sample_token('2'),
            entry("someone-else", 10, LocalDeliveryStatus::Claimed),
        );
        ledger.entries.insert(
            sample_token('3'),
            entry(
                &scope,
                RECOVERY_WINDOW_SECS + 5,
                LocalDeliveryStatus::Claimed,
            ),
        );
        ledger.entries.insert(
            sample_token('4'),
            entry(
                &scope,
                10,
                LocalDeliveryStatus::Delivered {
                    info_hash: "h".into(),
                },
            ),
        );
        ledger.entries.insert(
            sample_token('5'),
            entry(
                &scope,
                5,
                LocalDeliveryStatus::AddedPendingAck {
                    info_hash: "h".into(),
                },
            ),
        );
        save_delivery_ledger(&db, &mut ledger).unwrap();

        assert_eq!(
            recoverable_tokens(&db, &scope, now),
            vec![sample_token('1'), sample_token('5')]
        );
    }

    #[tokio::test]
    async fn test_delivery_duplicate_info_hash_and_already_delivered_dedup() {
        let tmp = TempTestDir::new("delivery_dedup");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let metainfo = make_valid_torrent_bytes("AlreadyPresent.ISO", 8192);
        let expected_hash = ParsedTorrent::parse_bytes(&metainfo).unwrap().info_hash;
        engine
            .add_torrent_from_bytes(&metainfo, Some(&tmp.str_path()))
            .expect("pre-add torrent");

        let token = sample_token('9');
        let item = DeliveryItem {
            queue_id: 501,
            torrent_id: 123,
            request_token: token.clone(),
            status: Some("pending".to_string()),
            name: Some("AlreadyPresent.ISO".to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        };
        let transport = MockDeliveryTransport::success(item.clone(), metainfo);
        let passkey = "0123456789abcdef0123456789abcdef";
        let device_id = get_or_create_device_id(&db);

        let hash1 = process_single_delivery_item(
            None, &engine, &db, &transport, passkey, &device_id, &item,
        )
        .await
        .expect("duplicate info_hash should ack cleanly");
        assert!(hash1.eq_ignore_ascii_case(&expected_hash));
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![123]);

        // Subsequent poll sees Delivered in ledger and does not re-fetch metainfo or add again.
        let hash2 = process_single_delivery_item(
            None, &engine, &db, &transport, passkey, &device_id, &item,
        )
        .await
        .expect("already-delivered poll entry should idempotently succeed");
        assert!(hash2.eq_ignore_ascii_case(&expected_hash));
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![123]);
    }

    fn sample_item(token: &str, queue_id: u64, torrent_id: u64, name: &str) -> DeliveryItem {
        DeliveryItem {
            queue_id,
            torrent_id,
            request_token: token.to_string(),
            status: Some("pending".to_string()),
            name: Some(name.to_string()),
            info_hash: None,
            created_at: None,
            expires_at: None,
        }
    }

    const TEST_PASSKEY: &str = "0123456789abcdef0123456789abcdef";

    fn set_download_dir(db: &Database, dir: &str) {
        let settings_json = serde_json::json!({ "download_directory": dir }).to_string();
        db.set_setting("user_settings_json", &settings_json)
            .unwrap();
    }

    #[test]
    fn test_validate_destination_rejects_relative_without_creating_it() {
        let rel = format!("nebulae_rel_probe_{}", std::process::id());
        let db = setup_linked_db(Some(&rel));
        let err = validate_configured_download_dir(&db).unwrap_err();
        assert!(
            err.contains("unavailable") && err.contains("absolute"),
            "{err}"
        );
        assert!(
            !Path::new(&rel).exists(),
            "a relative destination must never be created against the working directory"
        );
    }

    #[test]
    fn test_validate_destination_returns_resolved_absolute_path_with_spaces_and_unicode() {
        let tmp = TempTestDir::new("dest ünï");
        let target = tmp.path.join("My Movies ★");
        let db = setup_linked_db(Some(&target.to_string_lossy()));
        let resolved = validate_configured_download_dir(&db).unwrap();
        assert_eq!(PathBuf::from(&resolved), target);
        assert!(target.is_dir());
    }

    #[test]
    fn test_failure_record_reports_once_and_backs_off() {
        let db = setup_linked_db(Some(""));
        let tok = sample_token('a');
        assert!(record_delivery_failure(&db, &tok, "bad destination"));
        assert!(!record_delivery_failure(&db, &tok, "bad destination"));
        assert!(!record_delivery_failure(&db, &tok, "bad destination"));
        assert!(
            record_delivery_failure(&db, &tok, "different reason"),
            "a different reason is surfaced"
        );
        assert!(!should_attempt_delivery(&db, &tok, false));
        assert!(should_attempt_delivery(&db, &tok, true));

        let mut ledger = load_failure_ledger(&db);
        ledger.entries.get_mut(&tok).unwrap().last_attempt_at -= FAILURE_RETRY_MAX_SECS + 1;
        save_failure_ledger(&db, &mut ledger);
        assert!(should_attempt_delivery(&db, &tok, false), "backoff expires");

        clear_delivery_failure(&db, &tok);
        assert!(should_attempt_delivery(&db, &tok, false));
        assert!(load_failure_ledger(&db).entries.is_empty());
    }

    #[test]
    fn test_failure_backoff_grows_and_is_capped() {
        assert_eq!(failure_backoff_secs(1), 60);
        assert_eq!(failure_backoff_secs(2), 120);
        assert_eq!(failure_backoff_secs(3), 240);
        assert_eq!(failure_backoff_secs(50), FAILURE_RETRY_MAX_SECS);
    }

    #[tokio::test]
    async fn test_poll_cycles_after_one_destination_failure_do_not_retry_or_renotify() {
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let tmp = TempTestDir::new("loop_file");
        let file_path = tmp.path.join("occupied");
        std::fs::write(&file_path, b"x").unwrap();
        let db = setup_linked_db(Some(&file_path.to_string_lossy()));
        let token = sample_token('c');
        let item = sample_item(&token, 601, 5, "Loop.ISO");
        let transport =
            MockDeliveryTransport::success(item.clone(), make_valid_torrent_bytes("Loop.ISO", 512));
        let device_id = get_or_create_device_id(&db);

        for cycle in 0..25 {
            let res = process_single_delivery_item(
                None,
                &engine,
                &db,
                &transport,
                TEST_PASSKEY,
                &device_id,
                &item,
            )
            .await;
            let err = res.unwrap_err();
            if cycle == 0 {
                assert!(err.contains("not a directory"), "{err}");
            } else {
                assert!(err.contains("waiting to retry"), "cycle {cycle}: {err}");
            }
        }

        let record = load_failure_ledger(&db)
            .entries
            .get(&token)
            .cloned()
            .unwrap();
        assert_eq!(
            record.attempts, 1,
            "only the first poll may attempt delivery"
        );
        assert!(transport.ack_calls.lock().unwrap().is_empty());
        assert!(transport.metainfo_calls.lock().unwrap().is_empty());
        assert!(engine.list_torrents().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_correcting_destination_retries_immediately_and_adds_exactly_one_torrent() {
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let tmp = TempTestDir::new("loop_fix");
        let file_path = tmp.path.join("occupied");
        std::fs::write(&file_path, b"x").unwrap();
        let good_dir = tmp.path.join("Downloads");
        let db = setup_linked_db(Some(&file_path.to_string_lossy()));
        let token = sample_token('d');
        let metainfo = make_valid_torrent_bytes("Fixed.ISO", 1024);
        let expected_hash = ParsedTorrent::parse_bytes(&metainfo).unwrap().info_hash;
        let item = sample_item(&token, 602, 6, "Fixed.ISO");
        let transport = MockDeliveryTransport::success(item.clone(), metainfo);
        let device_id = get_or_create_device_id(&db);

        assert!(process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item
        )
        .await
        .is_err());

        set_download_dir(&db, &good_dir.to_string_lossy());
        let hash = process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item,
        )
        .await
        .expect("changed destination must retry without waiting for backoff");
        assert!(hash.eq_ignore_ascii_case(&expected_hash));
        assert!(good_dir.is_dir());
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
        assert_eq!(*transport.metainfo_calls.lock().unwrap(), vec![6]);
        assert!(load_failure_ledger(&db).entries.is_empty());

        // A later poll that still sees the item must not add a second copy.
        process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item,
        )
        .await
        .unwrap();
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_deliberate_retry_bypasses_backoff_and_succeeds_after_fix() {
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let tmp = TempTestDir::new("deliberate");
        let file_path = tmp.path.join("occupied");
        std::fs::write(&file_path, b"x").unwrap();
        let db = setup_linked_db(Some(&file_path.to_string_lossy()));
        let token = sample_token('e');
        let item = sample_item(&token, 603, 7, "Retry.ISO");
        let transport = MockDeliveryTransport::success(
            item.clone(),
            make_valid_torrent_bytes("Retry.ISO", 1024),
        );
        let device_id = get_or_create_device_id(&db);

        let first = process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item,
        )
        .await
        .unwrap_err();
        assert!(first.contains("not a directory"));

        let deliberate = process_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item,
            true,
        )
        .await
        .unwrap_err();
        assert!(
            deliberate.contains("not a directory"),
            "deliberate retry must re-run validation, got: {deliberate}"
        );
        assert_eq!(load_failure_ledger(&db).entries[&token].attempts, 2);

        // The same retry succeeds once the folder is corrected (no setting-change needed
        // for the deliberate path, but the destination fingerprint is refreshed anyway).
        std::fs::remove_file(&file_path).unwrap();
        std::fs::create_dir_all(&file_path).unwrap();
        let ok = process_delivery_item(
            None,
            &engine,
            &db,
            &transport,
            TEST_PASSKEY,
            &device_id,
            &item,
            true,
        )
        .await;
        assert!(ok.is_ok(), "{ok:?}");
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_blocked_request_does_not_block_other_valid_requests() {
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let tmp = TempTestDir::new("blocked_other");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let device_id = get_or_create_device_id(&db);

        let tok_a = sample_token('1');
        let item_a = sample_item(&tok_a, 701, 1, "A.ISO");
        let transport_a =
            MockDeliveryTransport::success(item_a.clone(), make_valid_torrent_bytes("A.ISO", 512));
        *transport_a.claim_result.lock().unwrap() = Err((
            403,
            "This delivery request belongs to a different device or client.".into(),
        ));
        assert!(process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport_a,
            TEST_PASSKEY,
            &device_id,
            &item_a
        )
        .await
        .is_err());
        assert!(process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport_a,
            TEST_PASSKEY,
            &device_id,
            &item_a
        )
        .await
        .unwrap_err()
        .contains("waiting to retry"));

        let tok_b = sample_token('2');
        let item_b = sample_item(&tok_b, 702, 2, "B.ISO");
        let transport_b =
            MockDeliveryTransport::success(item_b.clone(), make_valid_torrent_bytes("B.ISO", 768));
        process_single_delivery_item(
            None,
            &engine,
            &db,
            &transport_b,
            TEST_PASSKEY,
            &device_id,
            &item_b,
        )
        .await
        .expect("unrelated request must still be delivered");
        assert_eq!(engine.list_torrents().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_uri_redeem_waiting_on_poll_returns_success_instead_of_replay_error() {
        let _serial = REDEEM_TEST_SERIAL.lock().await;
        let tmp = TempTestDir::new("race");
        let db = setup_linked_db(Some(&tmp.str_path()));
        let engine: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let token = sample_token('f');
        let device_id = get_or_create_device_id(&db);
        let scope = compute_account_scope(TEST_PASSKEY, &device_id);
        let mut ledger = load_delivery_ledger(&db);
        ledger.entries.insert(
            token.clone(),
            LocalDeliveryEntry {
                account_scope: scope,
                queue_id: 801,
                torrent_id: 9,
                status: LocalDeliveryStatus::Delivered {
                    info_hash: "abc123".to_string(),
                },
                updated_at: now_unix_secs(),
            },
        );
        save_delivery_ledger(&db, &mut ledger).unwrap();

        let transport = Arc::new(MockDeliveryTransport::success(
            sample_item(&token, 801, 9, "Race.ISO"),
            make_valid_torrent_bytes("Race.ISO", 512),
        ));

        // The poll cycle holds the delivery lock while it finishes this request.
        let guard = DELIVERY_MUTEX.lock().await;
        let (db2, eng2, tr2, uri) = (
            Arc::clone(&db),
            Arc::clone(&engine),
            Arc::clone(&transport),
            format!("adc://delivery/{token}"),
        );
        let handle = tokio::spawn(async move {
            redeem_delivery_with_transport(None, &eng2, &db2, tr2.as_ref(), &uri).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(guard);

        let res = handle.await.unwrap();
        assert_eq!(res.unwrap(), "abc123");
        assert!(transport.poll_calls.lock().unwrap().is_empty());
        assert!(transport.metainfo_calls.lock().unwrap().is_empty());
    }

    fn route_db(targets: serde_json::Value, server_url: &str) -> Arc<Database> {
        let db = setup_linked_db(None);
        db.set_setting("adc_client_targets", &targets.to_string())
            .unwrap();
        let settings = serde_json::json!({
            "rtorrent_server_url": server_url,
            "rtorrent_server_token": "test-token",
        });
        db.set_setting("user_settings_json", &settings.to_string())
            .unwrap();
        db
    }

    fn target(id: &str, priority: u32, enabled: bool, kind: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "name": id, "priority": priority, "enabled": enabled, "client_type": kind
        })
    }

    #[tokio::test]
    async fn test_route_defaults_to_local_engine() {
        let db = setup_linked_db(None);
        let live: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let route = resolve_delivery_route(&db, &live).await.unwrap();
        assert!(!route.remote);
    }

    #[tokio::test]
    async fn test_route_skips_remote_without_server_url_and_uses_local() {
        let db = route_db(
            serde_json::json!([
                target("remote_server", 1, true, "remote_server"),
                target("local", 2, true, "local"),
            ]),
            "",
        );
        let live: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let route = resolve_delivery_route(&db, &live).await.unwrap();
        assert!(!route.remote);
    }

    #[tokio::test]
    async fn test_route_remote_only_reports_missing_server_url() {
        let db = route_db(
            serde_json::json!([
                target("local", 1, false, "local"),
                target("remote_server", 2, true, "remote_server"),
            ]),
            "  ",
        );
        let live: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let err = resolve_delivery_route(&db, &live).await.err().unwrap();
        assert!(err.contains("no Nebulae Server URL"), "{err}");
    }

    #[tokio::test]
    async fn test_route_unreachable_remote_cascades_to_local_or_reports() {
        let targets = serde_json::json!([
            target("remote_server", 1, true, "remote_server"),
            target("local", 2, true, "local"),
        ]);
        let db = route_db(targets, "http://127.0.0.1:1");
        let live: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        assert!(!resolve_delivery_route(&db, &live).await.unwrap().remote);

        let remote_only = route_db(
            serde_json::json!([
                target("local", 1, false, "local"),
                target("remote_server", 2, true, "remote_server"),
            ]),
            "http://127.0.0.1:1",
        );
        let err = resolve_delivery_route(&remote_only, &live)
            .await
            .err()
            .unwrap();
        assert!(err.contains("Remote server is not reachable"), "{err}");
    }

    #[tokio::test]
    async fn test_route_legacy_remote_rtorrent_target_is_honoured() {
        let db = route_db(
            serde_json::json!([
                target("local", 1, false, "local"),
                target("remote_rtorrent", 2, true, "remote_rtorrent"),
            ]),
            "",
        );
        let live: Arc<dyn TorrentEngine> = Arc::new(MockTorrentEngine::new());
        let err = resolve_delivery_route(&db, &live).await.err().unwrap();
        assert!(err.contains("no Nebulae Server URL"), "{err}");
    }
}

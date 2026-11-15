use crate::db::Database;
use crate::models::tracker::{AdcTrackerProfile, SeedTimeRecord};
use crate::utils::keyring::{
    delete_secret_with_user_authorization, get_secret, get_secret_with_user_authorization,
    passkey_secret_target, store_secret_with_user_authorization,
};
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

/// ADC `/api/user_stats/<passkey>` response shape.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AdcUserStatsResponse {
    username: Option<String>,
    uploaded_bytes: Option<u64>,
    downloaded_bytes: Option<u64>,
    bonus_points: Option<f64>,
    ratio: Option<f64>,
}

/// Fetch live stats from the ADC user_stats API and persist them in the DB.
///
/// Returns `Ok(true)` on a successful update, `Ok(false)` when the account is
/// not linked or the passkey is missing, and `Err(...)` on a network / API
/// failure that the caller may log but should not surface as a hard error in
/// the UI.
pub fn fetch_and_persist_adc_stats(db: &Database) -> Result<bool, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Ok(false);
    }

    let passkey = match get_secret(&passkey_secret_target()) {
        Ok(pk) => pk,
        Err(error) => {
            tracing::warn!(
                status = error.adc_state(),
                "[tracker-sync] credential unavailable; skipping stats fetch"
            );
            return Ok(false);
        }
    };

    let base_url = "https://asiandvdclub.org";
    let url = format!("{}/api/user_stats/{}", base_url, passkey.trim());

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.5")
        .build()
        .map_err(|e| format!("HTTP client build: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        // The endpoint path contains the passkey. Avoid reqwest errors that
        // could include the complete request URL.
        .map_err(|_| "[tracker-sync] request failed".to_string())?;

    let status = resp.status();

    // Rate-limited: silently skip — stats will be refreshed on the next cycle.
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::info!("[tracker-sync] rate limited — stats unchanged");
        return Ok(false);
    }

    // Authentication failure: passkey is wrong or account is banned.
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        tracing::warn!("[tracker-sync] passkey rejected ({})", status);
        return Err(format!("ADC returned {status} — passkey may be invalid"));
    }

    if !status.is_success() {
        return Err(format!(
            "ADC user_stats returned unexpected status {status}"
        ));
    }

    let stats: AdcUserStatsResponse = resp
        .json()
        .map_err(|_| "[tracker-sync] response parse failed".to_string())?;

    // Persist the live values.
    if let Some(ul) = stats.uploaded_bytes {
        let _ = db.set_setting("adc_uploaded", &ul.to_string());
    }
    if let Some(dl) = stats.downloaded_bytes {
        let _ = db.set_setting("adc_downloaded", &dl.to_string());
    }
    if let Some(bp) = stats.bonus_points {
        let _ = db.set_setting("adc_bonus_points", &bp.to_string());
    }
    // If the server returned a username that differs from what we stored
    // (e.g., the user's display name changed), update it.
    if let Some(uname) = stats.username {
        if !uname.is_empty() {
            let _ = db.set_setting("adc_username", &uname);
        }
    }

    let now = Utc::now().timestamp();
    let _ = db.set_setting("adc_stats_last_fetched", &now.to_string());

    tracing::info!(
        "[tracker-sync] stats updated — uploaded={}, downloaded={}, bp={}",
        stats.uploaded_bytes.unwrap_or(0),
        stats.downloaded_bytes.unwrap_or(0),
        stats.bonus_points.unwrap_or(0.0),
    );

    Ok(true)
}

/// Tauri command: manually trigger a stats refresh from ADC.
///
/// The frontend calls this after linking an account and on the Ratio Coach
/// panel's refresh button. The background task (`spawn_stats_poller`) also
/// calls `fetch_and_persist_adc_stats` on a schedule.
///
/// Returns the updated profile on success so the UI can refresh immediately
/// without a second round-trip.
#[tauri::command]
pub async fn sync_adc_stats(db: State<'_, Arc<Database>>) -> Result<AdcTrackerProfile, String> {
    let db_arc = Arc::clone(&*db);
    tokio::task::spawn_blocking(move || {
        match fetch_and_persist_adc_stats(&db_arc) {
            Ok(_) => {}
            Err(e) => {
                // Non-fatal: log and fall through to return what we have in DB.
                tracing::warn!("[tracker-sync] stats fetch failed: {e}");
            }
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?;

    // Re-read from DB (which now has the fresh values) via the existing command.
    get_adc_tracker_profile(db).await
}

/// Spawn a background Tokio task that polls ADC user stats every 30 minutes.
///
/// The first poll fires 10 seconds after startup to populate stats quickly
/// without delaying application boot. Subsequent polls run every 30 minutes.
/// On failure the cycle is silently skipped; no retry storm.
pub fn spawn_stats_poller(db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        // Short initial delay so startup isn't overloaded.
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        loop {
            let db_ref = Arc::clone(&db);
            let result =
                tokio::task::spawn_blocking(move || fetch_and_persist_adc_stats(&db_ref)).await;

            match result {
                Ok(Ok(true)) => tracing::info!("[tracker-sync] background stats refresh OK"),
                Ok(Ok(false)) => {} // Not linked or no passkey — silent.
                Ok(Err(e)) => tracing::warn!("[tracker-sync] background stats fetch: {e}"),
                Err(e) => tracing::error!("[tracker-sync] spawn_blocking error: {e}"),
            }

            // Wait 30 minutes before the next poll.
            tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
        }
    });
}

#[tauri::command]
pub async fn get_adc_tracker_profile(
    db: State<'_, Arc<Database>>,
) -> Result<AdcTrackerProfile, String> {
    let db = Arc::clone(&db);
    tokio::task::spawn_blocking(move || build_profile_from_db(&db))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

/// Explicitly retry ADC Keychain access after the user chooses Unlock.
#[tauri::command]
pub async fn unlock_adc_credential(
    db: State<'_, Arc<Database>>,
) -> Result<AdcTrackerProfile, String> {
    let db = Arc::clone(&db);
    tokio::task::spawn_blocking(move || {
        let configured = db
            .get_setting("adc_linked")
            .unwrap_or(None)
            .map(|value| value == "true")
            .unwrap_or(false);
        if !configured {
            return Err("No linked ADC account is configured.".to_string());
        }
        get_secret_with_user_authorization(&passkey_secret_target())
            .map_err(|error| format!("Could not unlock the ADC credential: {error}"))?;
        build_profile_from_db(&db)
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

/// Inner implementation shared between `get_adc_tracker_profile` and
/// `import_adc_passkey`.  Accepts any type that derefs to `Database`.
pub fn build_profile_from_db(db: &Database) -> Result<AdcTrackerProfile, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Ok(AdcTrackerProfile {
            username: String::new(),
            masked_passkey: String::new(),
            account_configured: false,
            is_linked: false,
            credential_status: "not_linked".to_string(),
            last_announce_time: None,
            next_announce_countdown_sec: None,
            announce_status: "Link your account to see stats".to_string(),
            ratio: 1.0,
            uploaded_bytes: 0,
            downloaded_bytes: 0,
            freeleech_active: false,
            bonus_points: 0.0,
            required_seed_time_hours: 72,
        });
    }

    let username = db
        .get_setting("adc_username")
        .unwrap_or(None)
        .unwrap_or_else(|| "Member".to_string());
    let masked_passkey = db
        .get_setting("adc_masked_passkey")
        .unwrap_or(None)
        .unwrap_or_else(|| "••••••••".to_string());
    let uploaded: u64 = db
        .get_setting("adc_uploaded")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let downloaded: u64 = db
        .get_setting("adc_downloaded")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let bonus: f32 = db
        .get_setting("adc_bonus_points")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let freeleech = db
        .get_setting("adc_freeleech")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);
    let seed_hours: u32 = db
        .get_setting("adc_required_seed_hours")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(72);

    let last_announce: Option<i64> = db
        .get_setting("adc_last_announce_time")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok());
    let announce_interval_sec: u32 = db
        .get_setting("adc_announce_interval_sec")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800);

    let now = Utc::now().timestamp();
    let (countdown, announce_status) = if let Some(last) = last_announce {
        let elapsed = (now - last).max(0) as u32;
        let remaining = announce_interval_sec.saturating_sub(elapsed);
        (Some(remaining), "Announce OK".to_string())
    } else {
        (None, "Unknown until first announce".to_string())
    };

    let ratio = AdcTrackerProfile::calculate_ratio(uploaded, downloaded);

    let credential_status = match get_secret(&passkey_secret_target()) {
        Ok(_) => "available",
        Err(error) => error.adc_state(),
    };
    let credential_available = credential_status == "available";

    Ok(AdcTrackerProfile {
        username,
        masked_passkey,
        account_configured: true,
        is_linked: credential_available,
        credential_status: credential_status.to_string(),
        last_announce_time: last_announce,
        next_announce_countdown_sec: countdown,
        announce_status,
        ratio,
        uploaded_bytes: uploaded,
        downloaded_bytes: downloaded,
        freeleech_active: freeleech,
        bonus_points: bonus,
        required_seed_time_hours: seed_hours,
    })
}

fn bump_adc_account_revision_if_changed(db: &Database, username: &str) -> Result<(), String> {
    let previous_username = db
        .get_setting("adc_username")
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    if previous_username
        .trim()
        .eq_ignore_ascii_case(username.trim())
    {
        return Ok(());
    }
    let current_revision = db
        .get_setting("adc_account_revision")
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    db.set_setting(
        "adc_account_revision",
        &current_revision.saturating_add(1).to_string(),
    )
    .map_err(|error| error.to_string())?;
    if let Err(error) = db.clear_cached_adc_members(Some(current_revision.saturating_add(1))) {
        tracing::warn!("[swarm-members] could not clear cache after ADC account change: {error}");
    }
    Ok(())
}

#[tauri::command]
pub async fn link_adc_account(
    username: String,
    passkey: String,
    db: State<'_, Arc<Database>>,
) -> Result<AdcTrackerProfile, String> {
    if username.trim().is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if passkey.trim().len() < 8 {
        return Err("Passkey must be at least 8 characters long".to_string());
    }

    let db_for_link = Arc::clone(&db);
    let profile = tokio::task::spawn_blocking(move || {
        // This is a direct response to the user's Link action, so Keychain may
        // request authorization for an item created by a different app build.
        store_secret_with_user_authorization(&passkey_secret_target(), &passkey)
            .map_err(|e| format!("Could not store the ADC credential securely: {e}"))?;

        let masked = AdcTrackerProfile::mask_passkey(&passkey);
        bump_adc_account_revision_if_changed(&db_for_link, &username)?;
        db_for_link
            .set_setting("adc_linked", "true")
            .map_err(|e| e.to_string())?;
        db_for_link
            .set_setting("adc_username", username.trim())
            .map_err(|e| e.to_string())?;
        db_for_link
            .set_setting("adc_masked_passkey", &masked)
            .map_err(|e| e.to_string())?;
        if db_for_link
            .get_setting("adc_uploaded")
            .unwrap_or(None)
            .is_none()
        {
            let _ = db_for_link.set_setting("adc_uploaded", "0");
        }
        if db_for_link
            .get_setting("adc_downloaded")
            .unwrap_or(None)
            .is_none()
        {
            let _ = db_for_link.set_setting("adc_downloaded", "0");
        }
        if db_for_link
            .get_setting("adc_bonus_points")
            .unwrap_or(None)
            .is_none()
        {
            let _ = db_for_link.set_setting("adc_bonus_points", "0.0");
        }
        let _ = db_for_link.set_setting("adc_freeleech", "false");
        let _ = db_for_link.set_setting("adc_required_seed_hours", "72");
        build_profile_from_db(&db_for_link)
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))??;

    let db_for_sync = Arc::clone(&db);
    tauri::async_runtime::spawn(async move {
        match tokio::task::spawn_blocking(move || fetch_and_persist_adc_stats(&db_for_sync)).await {
            Ok(Ok(true)) => tracing::info!("[tracker-sync] immediate post-link stats OK"),
            Ok(Ok(false)) => {}
            Ok(Err(e)) => tracing::warn!("[tracker-sync] immediate post-link stats: {e}"),
            Err(e) => tracing::error!("[tracker-sync] spawn_blocking: {e}"),
        }
    });
    Ok(profile)
}

#[tauri::command]
pub async fn unlink_adc_account(db: State<'_, Arc<Database>>) -> Result<(), String> {
    let db = Arc::clone(&db);
    tokio::task::spawn_blocking(move || {
        delete_secret_with_user_authorization(&passkey_secret_target())
            .map_err(|e| format!("Could not remove the ADC credential securely: {e}"))?;

        db.set_setting("adc_linked", "false")
            .map_err(|e| e.to_string())?;
        db.set_setting("adc_username", "")
            .map_err(|e| e.to_string())?;
        db.set_setting("adc_masked_passkey", "")
            .map_err(|e| e.to_string())?;
        // Do not inspect or erase the legacy adc_passkey setting here. It may
        // contain the only pre-Keychain copy; migration/deletion needs approval.
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[tauri::command]
pub fn get_seed_time_records(db: State<Arc<Database>>) -> Result<Vec<SeedTimeRecord>, String> {
    db.list_seed_time_records().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn calculate_ratio_projection(
    uploaded: u64,
    downloaded: u64,
    additional_download: u64,
) -> Result<f32, String> {
    Ok(AdcTrackerProfile::project_ratio(
        uploaded,
        downloaded,
        additional_download,
    ))
}

/// ADC response shape for POST /api/nebulae/passkey_import.
#[derive(Debug, Deserialize)]
struct AdcPasskeyImportResponse {
    username: Option<String>,
    passkey: Option<String>,
    error: Option<String>,
}

/// Tauri command: import the ADC torrent passkey by authenticating with
/// the user's ADC username and password.
///
/// Calls `POST https://asiandvdclub.org/api/nebulae/passkey_import`, which
/// validates the credentials server-side using the same Argon2id path as
/// the ADC login form, and returns the raw torrent passkey.
///
/// On success the passkey is stored in platform secure storage and the account
/// is linked exactly as if the user had entered the passkey manually.
/// The caller receives the full `AdcTrackerProfile` so the UI can update
/// immediately without a second round-trip.
#[tauri::command]
pub async fn import_adc_passkey(
    username: String,
    password: String,
    db: State<'_, Arc<Database>>,
) -> Result<AdcTrackerProfile, String> {
    if username.trim().is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if password.is_empty() {
        return Err("Password cannot be empty".to_string());
    }

    let db_arc = Arc::clone(&*db);
    let username_clone = username.clone();
    let password_clone = password.clone();

    tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("Nebulae/0.2.5")
            .build()
            .map_err(|e| format!("HTTP client build error: {e}"))?;

        let url = "https://asiandvdclub.org/api/nebulae/passkey_import";

        let resp = client
            .post(url)
            .json(&serde_json::json!({
                "username": username_clone.trim(),
                "password": password_clone,
            }))
            .send()
            .map_err(|e| format!("Request to ADC failed: {e}"))?;

        let status = resp.status();

        let body: AdcPasskeyImportResponse = resp
            .json()
            .map_err(|e| format!("Unexpected ADC response format: {e}"))?;

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(
                "ADC is rate limiting this request. Please wait before trying again.".to_string(),
            );
        }

        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            let kind = body.error.as_deref().unwrap_or("unknown");
            return Err(match kind {
                "account_banned" => "Your ADC account is banned.".to_string(),
                _ => "Invalid ADC username or password.".to_string(),
            });
        }

        if !status.is_success() {
            let msg = body.error.unwrap_or_else(|| status.to_string());
            return Err(format!("ADC returned an error: {msg}"));
        }

        let returned_username = body
            .username
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| username_clone.trim().to_string());

        let passkey = body
            .passkey
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| "ADC did not return a passkey in its response.".to_string())?;

        // This follows the user's explicit Import action, so Keychain may ask
        // macOS to authorize updating an item from a different app signature.
        let masked = AdcTrackerProfile::mask_passkey(&passkey);
        store_secret_with_user_authorization(&passkey_secret_target(), &passkey)
            .map_err(|e| format!("Could not store the ADC credential securely: {e}"))?;
        bump_adc_account_revision_if_changed(&db_arc, &returned_username)?;
        db_arc
            .set_setting("adc_linked", "true")
            .map_err(|e| e.to_string())?;
        db_arc
            .set_setting("adc_username", &returned_username)
            .map_err(|e| e.to_string())?;
        db_arc
            .set_setting("adc_masked_passkey", &masked)
            .map_err(|e| e.to_string())?;
        if db_arc.get_setting("adc_uploaded").unwrap_or(None).is_none() {
            let _ = db_arc.set_setting("adc_uploaded", "0");
        }
        if db_arc
            .get_setting("adc_downloaded")
            .unwrap_or(None)
            .is_none()
        {
            let _ = db_arc.set_setting("adc_downloaded", "0");
        }
        if db_arc
            .get_setting("adc_bonus_points")
            .unwrap_or(None)
            .is_none()
        {
            let _ = db_arc.set_setting("adc_bonus_points", "0.0");
        }
        let _ = db_arc.set_setting("adc_freeleech", "false");
        let _ = db_arc.set_setting("adc_required_seed_hours", "72");

        // Trigger an immediate stats fetch so ratio/upload/download are populated.
        {
            let db_for_fetch = Arc::clone(&db_arc);
            std::thread::spawn(move || {
                if let Err(e) = fetch_and_persist_adc_stats(&db_for_fetch) {
                    tracing::warn!("[import-passkey] post-import stats fetch: {e}");
                }
            });
        }

        // Build and return the profile the UI needs.
        let profile =
            crate::commands::tracker::build_profile_from_db(&db_arc).map_err(|e| e.to_string())?;
        Ok(profile)
    })
    .await
    .map_err(|e| format!("Internal task error: {e}"))?
}

// ── ADC swarm member lookup ──────────────────────────────────────────────────

/// One matched ADC member from the swarm_members endpoint.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SwarmMember {
    pub ip: String,
    pub port: u16,
    pub username: String,
    pub user_id: u64,
    pub profile_url: String,
    pub ratio: f64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub bonus_points: f64,
    #[serde(default)]
    pub matched_at_epoch_ms: Option<u64>,
}

/// Per-peer address sent in the lookup request.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct PeerAddr {
    pub ip: String,
    pub port: u16,
}

/// Result returned to the front-end.
#[derive(Debug, serde::Serialize)]
pub struct SwarmMembersResult {
    /// Machine-readable result state; credential failures are never classified
    /// as endpoint support failures.
    pub status: String,
    /// true when the endpoint is known to exist, including no-match/rate-limit.
    pub supported: bool,
    pub matches: Vec<SwarmMember>,
    /// Human-readable degradation reason when supported=false.
    pub error: Option<String>,
    /// True when all requested peers were served from the persistent cache.
    pub from_cache_only: bool,
}

impl SwarmMembersResult {
    fn state(status: &str, supported: bool) -> Self {
        Self {
            status: status.to_string(),
            supported,
            matches: vec![],
            error: Some(status.to_string()),
            from_cache_only: false,
        }
    }
}

pub(crate) fn is_valid_info_hash(info_hash: &str) -> bool {
    let hash = info_hash.trim();
    hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

const ADC_SWARM_MEMBERS_ENDPOINT: &str = "https://asiandvdclub.org/api/nebulae/swarm_members";

fn request_error_class(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "body"
    } else {
        "other"
    }
}

fn normalized_peer_key(ip: &str, port: u16) -> Option<String> {
    let parsed = ip.parse::<std::net::IpAddr>().ok()?;
    // Canonicalise IPv4-mapped IPv6 (::ffff:x.x.x.x) to plain IPv4 so that
    // DB-cached member addresses always compare equal to Transmission peer addresses.
    let canonical = match parsed {
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(std::net::IpAddr::V4)
            .unwrap_or(parsed),
        v4 => v4,
    };
    Some(format!("{}|{port}", canonical))
}

/// Sends the authenticated HTTP request and classifies the endpoint response.
///
/// The HTTP body deliberately uses the endpoint's `info_hash` field. This is
/// separate from Tauri IPC, where the default camelCase mapping accepts
/// frontend `infoHash` for the Rust `info_hash` command parameter.
fn request_swarm_members(
    info_hash: &str,
    peers: &[PeerAddr],
    passkey: &str,
    endpoint: &str,
) -> SwarmMembersResult {
    let body = serde_json::json!({
        "passkey": passkey.trim(),
        "info_hash": info_hash,
        "peers": peers,
    });

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Nebulae/0.2.7")
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            tracing::warn!("[swarm-members] HTTP client initialization failed");
            return SwarmMembersResult::state("network_error", false);
        }
    };

    let response = match client.post(endpoint).json(&body).send() {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                error_kind = request_error_class(&error),
                "[swarm-members] request failed"
            );
            return SwarmMembersResult::state("network_error", false);
        }
    };

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        // Without a verified response contract, classify every 404 as an
        // unsupported route. A successful JSON response with no matches is
        // handled separately below.
        return SwarmMembersResult::state("endpoint_unsupported", false);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return SwarmMembersResult::state("authentication_failed", true);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return SwarmMembersResult::state("rate_limited", true);
    }
    if !status.is_success() {
        tracing::warn!(
            http_status = status.as_u16(),
            "[swarm-members] unexpected HTTP status"
        );
        return SwarmMembersResult::state("http_error", false);
    }

    #[derive(serde::Deserialize)]
    struct ApiResponse {
        #[serde(default)]
        matches: Vec<SwarmMember>,
    }

    match response.json::<ApiResponse>() {
        Ok(parsed) => SwarmMembersResult {
            status: if parsed.matches.is_empty() {
                "no_matching_peers".to_string()
            } else {
                "matched".to_string()
            },
            supported: true,
            matches: parsed.matches,
            error: None,
            from_cache_only: false,
        },
        Err(_) => {
            // Do not log reqwest's error value: it can contain URL or response
            // details. The response body may contain authenticated data.
            tracing::warn!("[swarm-members] endpoint returned invalid JSON");
            SwarmMembersResult::state("parse_error", true)
        }
    }
}

/// Calls POST /api/nebulae/swarm_members on the ADC tracker.
///
/// The passkey is read only from platform secure storage inside the Rust layer
/// and never crosses the IPC boundary into the WebView.
///
/// Successful matches are cached in SQLite by ADC account revision, torrent,
/// and observed peer endpoint. This lets a restart reuse recent matches without
/// exposing credentials to the WebView.
///
/// IPC contract: with Tauri's default command naming, the frontend sends
/// `infoHash`, which maps to this Rust `info_hash` parameter. The HTTP body
/// below has its own endpoint-defined snake_case contract.
#[tauri::command]
pub async fn adc_swarm_members(
    info_hash: String,
    peers: Vec<PeerAddr>,
    db: tauri::State<'_, std::sync::Arc<crate::db::Database>>,
) -> Result<SwarmMembersResult, String> {
    let db_arc = std::sync::Arc::clone(&db);

    tokio::task::spawn_blocking(move || {
        // 1. Validate the torrent identity before using either cache or network.
        if !is_valid_info_hash(&info_hash) {
            return Ok(SwarmMembersResult::state("invalid_info_hash", false));
        }
        let hash = info_hash.trim().to_lowercase();

        // 2. Feature-gate: only proceed if ADC is linked.
        let is_linked = db_arc
            .get_setting("adc_linked")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false);

        if !is_linked {
            return Ok(SwarmMembersResult::state("not_linked", false));
        }

        let account_revision = db_arc
            .get_setting("adc_account_revision")
            .map_err(|error| error.to_string())?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let cached_matches = match db_arc.get_cached_adc_members(account_revision, &hash, &peers) {
            Ok(matches) => matches,
            Err(error) => {
                tracing::warn!("[swarm-members] persistent cache read failed: {error}");
                Vec::new()
            }
        };
        let cached_keys: std::collections::HashSet<String> = cached_matches
            .iter()
            .filter_map(|member| normalized_peer_key(&member.ip, member.port))
            .collect();
        let unresolved: Vec<PeerAddr> = peers
            .into_iter()
            .filter(|peer| match normalized_peer_key(&peer.ip, peer.port) {
                Some(key) => !cached_keys.contains(&key),
                None => true,
            })
            .collect();

        if unresolved.is_empty() {
            return Ok(SwarmMembersResult {
                status: if cached_matches.is_empty() {
                    "no_matching_peers"
                } else {
                    "matched"
                }
                .to_string(),
                supported: true,
                matches: cached_matches,
                error: None,
                from_cache_only: true,
            });
        }

        // Cached matches remain available even when the secure store is locked.
        // Never fall back to the legacy plaintext SQLite setting.
        let passkey = match get_secret(&passkey_secret_target()) {
            Ok(pk) => pk,
            Err(error) => {
                let mut result = SwarmMembersResult::state(error.adc_state(), false);
                result.matches = cached_matches;
                return Ok(result);
            }
        };

        let mut result =
            request_swarm_members(&hash, &unresolved, &passkey, ADC_SWARM_MEMBERS_ENDPOINT);
        if result.status == "matched" || result.status == "no_matching_peers" {
            let matched_at = chrono::Utc::now().timestamp_millis().max(0) as u64;
            let requested_keys: std::collections::HashSet<String> = unresolved
                .iter()
                .filter_map(|peer| normalized_peer_key(&peer.ip, peer.port))
                .collect();
            result.matches.retain(|member| {
                normalized_peer_key(&member.ip, member.port)
                    .is_some_and(|key| requested_keys.contains(&key))
            });
            for member in &mut result.matches {
                member.matched_at_epoch_ms = Some(matched_at);
            }
            if let Err(error) =
                db_arc.save_cached_adc_members(account_revision, &hash, &result.matches)
            {
                tracing::warn!("[swarm-members] persistent cache write failed: {error}");
            }
            if result.status == "matched" && result.matches.is_empty() {
                result.status = "no_matching_peers".to_string();
            }
        }

        let fresh_matches = result.matches.clone();
        // An error response may still carry cached matches. The UI keeps those
        // visible while displaying the new lookup's degradation status.
        let mut merged = std::collections::HashMap::new();
        for member in cached_matches.into_iter().chain(result.matches) {
            if let Some(key) = normalized_peer_key(&member.ip, member.port) {
                merged.insert(key, member);
            }
        }
        result.matches = merged.into_values().collect();
        if result.status == "matched" || result.status == "no_matching_peers" {
            result.status = if result.matches.is_empty() {
                "no_matching_peers"
            } else {
                "matched"
            }
            .to_string();
            if !fresh_matches.is_empty() {
                if let Err(error) = db_arc.enrich_peer_history_adc(
                    &hash,
                    account_revision,
                    &fresh_matches,
                    fresh_matches
                        .iter()
                        .filter_map(|member| member.matched_at_epoch_ms)
                        .max()
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().max(0) as u64),
                ) {
                    tracing::warn!("[swarm-members] peer-history enrichment failed: {error}");
                }
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::thread;

    const SYNTHETIC_LEGACY_PASSKEY: &str = "synthetic-legacy-adc-passkey";
    const SYNTHETIC_SWARM_PASSKEY: &str = "synthetic-swarm-test-passkey";
    const SYNTHETIC_INFO_HASH: &str = "340313449696d11fbfec8982c5055f90459c3ece";

    /// A local-only HTTP server for mocked endpoint-processing tests.
    /// It captures the synthetic request body and never contacts ADC.
    fn start_mock_swarm_endpoint(
        response_json: String,
    ) -> (String, thread::JoinHandle<serde_json::Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local mock endpoint");
        let address = listener.local_addr().expect("read local endpoint address");
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            let mut reader = BufReader::new(stream.try_clone().expect("clone test socket"));
            let mut request_line = String::new();
            reader
                .read_line(&mut request_line)
                .expect("read request line");
            assert_eq!(
                request_line.trim_end(),
                "POST /api/nebulae/swarm_members HTTP/1.1"
            );

            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request headers");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().expect("valid content length");
                    }
                }
            }

            let mut request_body = vec![0; content_length];
            reader
                .read_exact(&mut request_body)
                .expect("read request body");
            let request_body =
                serde_json::from_slice(&request_body).expect("parse synthetic request JSON");

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_json.len(),
                response_json
            );
            stream
                .write_all(response.as_bytes())
                .expect("write local mock response");
            request_body
        });

        (format!("http://{address}/api/nebulae/swarm_members"), task)
    }

    // Test-only model of an explicitly approved migration. Runtime startup and
    // ordinary account commands must never call this against the user's DB.
    fn migrate_synthetic_legacy_passkey(
        db: &Database,
        store: impl FnOnce(&str) -> Result<(), String>,
        retrieve: impl FnOnce() -> Result<String, String>,
    ) -> Result<bool, String> {
        let Some(legacy) = db
            .get_setting("adc_passkey")
            .map_err(|e| e.to_string())?
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(false);
        };

        store(&legacy)?;
        if retrieve()? != legacy {
            return Err("Stored credential verification failed.".to_string());
        }

        db.set_setting("adc_passkey", "")
            .map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn memory_db() -> Database {
        Database::new(Path::new(":memory:")).expect("in-memory test database")
    }

    #[test]
    fn legacy_field_is_cleared_only_after_secure_store_verification() {
        let db = memory_db();
        db.set_setting("adc_passkey", SYNTHETIC_LEGACY_PASSKEY)
            .unwrap();
        let synthetic_secure_store = Mutex::new(None::<String>);

        let migrated = migrate_synthetic_legacy_passkey(
            &db,
            |value| {
                *synthetic_secure_store.lock().unwrap() = Some(value.to_string());
                Ok(())
            },
            || {
                synthetic_secure_store
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or_else(|| "synthetic secure store is empty".to_string())
            },
        )
        .unwrap();

        assert!(migrated);
        assert_eq!(db.get_setting("adc_passkey").unwrap().as_deref(), Some(""));
        assert_eq!(
            synthetic_secure_store.lock().unwrap().as_deref(),
            Some(SYNTHETIC_LEGACY_PASSKEY)
        );
    }

    #[test]
    fn legacy_field_is_retained_when_secure_store_verification_fails() {
        let db = memory_db();
        db.set_setting("adc_passkey", SYNTHETIC_LEGACY_PASSKEY)
            .unwrap();

        let result = migrate_synthetic_legacy_passkey(
            &db,
            |_| Ok(()),
            || Ok("different-synthetic-value".to_string()),
        );

        assert!(result.is_err());
        assert_eq!(
            db.get_setting("adc_passkey").unwrap().as_deref(),
            Some(SYNTHETIC_LEGACY_PASSKEY)
        );
    }

    #[test]
    fn adc_result_states_distinguish_credentials_empty_matches_and_unsupported_route() {
        let credential = SwarmMembersResult::state("credential_access_denied", false);
        let unsupported = SwarmMembersResult::state("endpoint_unsupported", false);
        let no_matches = SwarmMembersResult {
            status: "no_matching_peers".to_string(),
            supported: true,
            matches: vec![],
            error: None,
            from_cache_only: false,
        };

        assert_eq!(credential.status, "credential_access_denied");
        assert_eq!(unsupported.status, "endpoint_unsupported");
        assert_eq!(no_matches.status, "no_matching_peers");
        assert!(!credential.supported);
        assert!(!unsupported.supported);
        assert!(no_matches.supported);
    }

    #[test]
    fn info_hash_validation_accepts_valid_40_hex_and_rejects_malformed() {
        assert!(is_valid_info_hash(
            "340313449696d11fbfec8982c5055f90459c3ece"
        ));
        assert!(is_valid_info_hash(
            "340313449696D11FBFEC8982C5055F90459C3ECE"
        ));
        assert!(is_valid_info_hash(
            "  340313449696d11fbfec8982c5055f90459c3ece  "
        ));
        // 39 chars
        assert!(!is_valid_info_hash(
            "340313449696d11fbfec8982c5055f90459c3ec"
        ));
        // 41 chars
        assert!(!is_valid_info_hash(
            "340313449696d11fbfec8982c5055f90459c3ece1"
        ));
        // Non-hex characters
        assert!(!is_valid_info_hash(
            "340313449696d11fbfec8982c5055f90459c3ecg"
        ));
        // Empty
        assert!(!is_valid_info_hash(""));
    }

    #[test]
    fn actual_tauri_dispatch_accepts_camel_case_info_hash_and_reaches_handler() {
        // Actual Tauri MockRuntime dispatch, controlled with an in-memory DB.
        // adc_linked is absent, so the handler returns before keychain or HTTP access.
        let app = tauri::test::mock_builder()
            .manage(Arc::new(memory_db()))
            .invoke_handler(tauri::generate_handler![adc_swarm_members])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock Tauri app");
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("build mock Tauri window");

        let request = |body| tauri::webview::InvokeRequest {
            cmd: "adc_swarm_members".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .expect("parse mock Tauri URL"),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        };

        tauri::test::assert_ipc_response(
            &window,
            request(serde_json::json!({
                "infoHash": SYNTHETIC_INFO_HASH,
                "peers": [],
            })),
            Ok(SwarmMembersResult::state("not_linked", false)),
        );

        let legacy_snake_case = tauri::test::get_ipc_response(
            &window,
            request(serde_json::json!({
                "info_hash": SYNTHETIC_INFO_HASH,
                "peers": [],
            })),
        );
        assert!(
            legacy_snake_case.is_err(),
            "default Tauri naming requires the frontend `infoHash` argument"
        );
    }

    #[test]
    fn mocked_endpoint_parses_matches_and_keeps_http_body_snake_case() {
        // Mocked local HTTP integration: a synthetic passkey and peer are sent
        // to loopback, and a synthetic match is parsed by the production helper.
        let response = serde_json::json!({
            "matches": [{
                "ip": "203.0.113.17",
                "port": 51413,
                "username": "synthetic-member",
                "user_id": 7,
                "profile_url": "https://example.invalid/profile/7",
                "ratio": 1.5,
                "uploaded_bytes": 1000,
                "downloaded_bytes": 500,
                "bonus_points": 2.0
            }]
        })
        .to_string();
        let (endpoint, request_task) = start_mock_swarm_endpoint(response);
        let peers = vec![PeerAddr {
            ip: "203.0.113.17".to_string(),
            port: 51413,
        }];

        let result = request_swarm_members(
            SYNTHETIC_INFO_HASH,
            &peers,
            SYNTHETIC_SWARM_PASSKEY,
            &endpoint,
        );

        assert_eq!(result.status, "matched");
        assert!(result.supported);
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].username, "synthetic-member");
        let sent_body = request_task.join().expect("mock server completed");
        assert_eq!(sent_body["passkey"], SYNTHETIC_SWARM_PASSKEY);
        assert_eq!(sent_body["info_hash"], SYNTHETIC_INFO_HASH);
        assert!(sent_body.get("infoHash").is_none());
        assert_eq!(sent_body["peers"][0]["ip"], "203.0.113.17");
    }

    #[test]
    fn mocked_successful_empty_response_is_not_a_transport_failure() {
        // Mocked local HTTP integration: valid JSON with no matches is a
        // successful supported response, distinct from transport failure.
        let (endpoint, request_task) = start_mock_swarm_endpoint(r#"{"matches":[]}"#.to_string());
        let result =
            request_swarm_members(SYNTHETIC_INFO_HASH, &[], SYNTHETIC_SWARM_PASSKEY, &endpoint);

        assert_eq!(result.status, "no_matching_peers");
        assert!(result.supported);
        assert!(result.matches.is_empty());
        assert_eq!(result.error, None);
        let _ = request_task.join().expect("mock server completed");
    }

    #[test]
    fn request_transport_failure_is_classified_without_error_details() {
        let result = request_swarm_members(
            SYNTHETIC_INFO_HASH,
            &[],
            SYNTHETIC_SWARM_PASSKEY,
            "http://127.0.0.1:0/api/nebulae/swarm_members",
        );

        assert_eq!(result.status, "network_error");
        assert!(!result.supported);
        assert_eq!(result.error.as_deref(), Some("network_error"));
    }

    #[test]
    fn adc_cache_revision_changes_only_when_the_account_changes() {
        let db = crate::db::Database::new(Path::new(":memory:")).unwrap();
        db.set_setting("adc_username", "MemberOne").unwrap();
        db.set_setting("adc_account_revision", "7").unwrap();

        bump_adc_account_revision_if_changed(&db, "memberone").unwrap();
        assert_eq!(
            db.get_setting("adc_account_revision").unwrap().as_deref(),
            Some("7")
        );

        bump_adc_account_revision_if_changed(&db, "MemberTwo").unwrap();
        assert_eq!(
            db.get_setting("adc_account_revision").unwrap().as_deref(),
            Some("8")
        );
    }
}

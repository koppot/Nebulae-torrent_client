use crate::db::Database;
use crate::models::tracker::{AdcTrackerProfile, SeedTimeRecord};
use crate::utils::keyring::{delete_secret, passkey_secret_target, store_secret};
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

    // Prefer keychain; fall back to DB setting (stored as plaintext for
    // compatibility with existing installs that pre-date keychain adoption).
    let passkey = crate::utils::keyring::get_secret(&passkey_secret_target())
        .ok()
        .or_else(|| {
            db.get_setting("adc_passkey")
                .unwrap_or(None)
                .filter(|v| !v.is_empty())
        });

    let passkey = match passkey {
        Some(pk) if !pk.trim().is_empty() => pk,
        _ => {
            tracing::warn!("[tracker-sync] passkey not available — skipping stats fetch");
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
        .map_err(|e| format!("[tracker-sync] request failed: {e}"))?;

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
        .map_err(|e| format!("[tracker-sync] parse error: {e}"))?;

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
    get_adc_tracker_profile(db)
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
pub fn get_adc_tracker_profile(db: State<Arc<Database>>) -> Result<AdcTrackerProfile, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Ok(AdcTrackerProfile {
            username: String::new(),
            masked_passkey: String::new(),
            is_linked: false,
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

    Ok(AdcTrackerProfile {
        username,
        masked_passkey,
        is_linked: true,
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

#[tauri::command]
pub fn link_adc_account(
    username: String,
    passkey: String,
    db: State<Arc<Database>>,
) -> Result<AdcTrackerProfile, String> {
    if username.trim().is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if passkey.trim().len() < 8 {
        return Err("Passkey must be at least 8 characters long".to_string());
    }

    let masked = AdcTrackerProfile::mask_passkey(&passkey);

    // Securely route passkey to OS Keychain (macOS Keychain / Windows Credential Manager)
    let secret_target = passkey_secret_target();
    if let Err(e) = store_secret(&secret_target, &passkey) {
        tracing::warn!("Failed to store passkey in OS keychain: {e}");
    }
    let _ = db.set_setting("adc_passkey", &passkey);

    db.set_setting("adc_linked", "true")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_username", &username)
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_masked_passkey", &masked)
        .map_err(|e| e.to_string())?;
    // Initialise stats to 0 — the background poller will fetch real values
    // within 10 seconds. Do NOT overwrite if values already exist (re-link
    // scenario where the user re-enters the same passkey).
    if db.get_setting("adc_uploaded").unwrap_or(None).is_none() {
        let _ = db.set_setting("adc_uploaded", "0");
    }
    if db.get_setting("adc_downloaded").unwrap_or(None).is_none() {
        let _ = db.set_setting("adc_downloaded", "0");
    }
    if db.get_setting("adc_bonus_points").unwrap_or(None).is_none() {
        let _ = db.set_setting("adc_bonus_points", "0.0");
    }
    let _ = db.set_setting("adc_freeleech", "false");
    let _ = db.set_setting("adc_required_seed_hours", "72");

    // Trigger an immediate stats fetch in the background so the UI reflects
    // real values as soon as possible after linking.
    {
        let db_clone = Arc::clone(&*db);
        tauri::async_runtime::spawn(async move {
            let r =
                tokio::task::spawn_blocking(move || fetch_and_persist_adc_stats(&db_clone)).await;
            match r {
                Ok(Ok(true)) => tracing::info!("[tracker-sync] immediate post-link stats OK"),
                Ok(Ok(false)) => {}
                Ok(Err(e)) => tracing::warn!("[tracker-sync] immediate post-link stats: {e}"),
                Err(e) => tracing::error!("[tracker-sync] spawn_blocking: {e}"),
            }
        });
    }

    get_adc_tracker_profile(db)
}

#[tauri::command]
pub fn unlink_adc_account(db: State<Arc<Database>>) -> Result<(), String> {
    let secret_target = passkey_secret_target();
    if let Err(e) = delete_secret(&secret_target) {
        tracing::warn!("Failed to delete passkey from OS keychain: {e}");
    }
    let _ = db.set_setting("adc_passkey", "");

    db.set_setting("adc_linked", "false")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_username", "")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_masked_passkey", "")
        .map_err(|e| e.to_string())?;
    Ok(())
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

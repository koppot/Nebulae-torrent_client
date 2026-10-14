use crate::db::Database;
use crate::models::tracker::{AdcTrackerProfile, SeedTimeRecord};
use chrono::Utc;
use std::sync::Arc;
use tauri::State;

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

    db.set_setting("adc_linked", "true")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_username", &username)
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_masked_passkey", &masked)
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_uploaded", "0")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_downloaded", "0")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_bonus_points", "0.0")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_freeleech", "false")
        .map_err(|e| e.to_string())?;
    db.set_setting("adc_required_seed_hours", "72")
        .map_err(|e| e.to_string())?;

    get_adc_tracker_profile(db)
}

#[tauri::command]
pub fn unlink_adc_account(db: State<Arc<Database>>) -> Result<(), String> {
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

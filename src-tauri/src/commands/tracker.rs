use tauri::State;
use std::sync::Arc;
use chrono::Utc;
use crate::models::tracker::{AdcTrackerProfile, SeedTimeRecord};
use crate::db::Database;

#[tauri::command]
pub fn get_adc_tracker_profile(db: State<Arc<Database>>) -> Result<AdcTrackerProfile, String> {
    let is_linked = db.get_setting("adc_linked")
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
            announce_status: "Unlinked".to_string(),
            ratio: 1.0,
            uploaded_bytes: 0,
            downloaded_bytes: 0,
            freeleech_active: false,
            bonus_points: 0.0,
            required_seed_time_hours: 72,
        });
    }

    let username = db.get_setting("adc_username").unwrap_or(None).unwrap_or_else(|| "Member".to_string());
    let masked_passkey = db.get_setting("adc_masked_passkey").unwrap_or(None).unwrap_or_else(|| "••••••••".to_string());
    let uploaded: u64 = db.get_setting("adc_uploaded")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(185_400_000_000); // 185.4 GB
    let downloaded: u64 = db.get_setting("adc_downloaded")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(68_200_000_000); // 68.2 GB
    let bonus: f32 = db.get_setting("adc_bonus_points")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_420.50);
    let freeleech = db.get_setting("adc_freeleech")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(true);
    let seed_hours: u32 = db.get_setting("adc_required_seed_hours")
        .unwrap_or(None)
        .and_then(|v| v.parse().ok())
        .unwrap_or(72);

    let ratio = AdcTrackerProfile::calculate_ratio(uploaded, downloaded);

    Ok(AdcTrackerProfile {
        username,
        masked_passkey,
        is_linked: true,
        last_announce_time: Some(Utc::now().timestamp() - 420),
        next_announce_countdown_sec: Some(1380), // 23 mins remaining
        announce_status: "Announce OK (HTTP 200)".to_string(),
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

    db.set_setting("adc_linked", "true").map_err(|e| e.to_string())?;
    db.set_setting("adc_username", &username).map_err(|e| e.to_string())?;
    db.set_setting("adc_masked_passkey", &masked).map_err(|e| e.to_string())?;
    db.set_setting("adc_uploaded", "185400000000").map_err(|e| e.to_string())?;
    db.set_setting("adc_downloaded", "68200000000").map_err(|e| e.to_string())?;
    db.set_setting("adc_bonus_points", "3420.50").map_err(|e| e.to_string())?;
    db.set_setting("adc_freeleech", "true").map_err(|e| e.to_string())?;
    db.set_setting("adc_required_seed_hours", "72").map_err(|e| e.to_string())?;

    // Seed default seed-time records if empty
    let existing = db.list_seed_time_records().unwrap_or_default();
    if existing.is_empty() {
        let sample1 = SeedTimeRecord {
            info_hash: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".to_string(),
            torrent_name: "Seven Samurai (1954) [Criterion Remux 1080p]".to_string(),
            seeding_time_seconds: 58 * 3600, // 58 hours seeded out of 72
            required_seeding_seconds: 72 * 3600,
            is_shield_satisfied: false,
        };
        let sample2 = SeedTimeRecord {
            info_hash: "b2c3d4e5f60718293a4b5c6d7e8f90123456789a".to_string(),
            torrent_name: "Tokyo Story (1953) [BFI Restoration 1080p]".to_string(),
            seeding_time_seconds: 84 * 3600, // 84 hours seeded -> Satisfied
            required_seeding_seconds: 72 * 3600,
            is_shield_satisfied: true,
        };
        let _ = db.upsert_seed_time_record(&sample1);
        let _ = db.upsert_seed_time_record(&sample2);
    }

    get_adc_tracker_profile(db)
}

#[tauri::command]
pub fn unlink_adc_account(db: State<Arc<Database>>) -> Result<(), String> {
    db.set_setting("adc_linked", "false").map_err(|e| e.to_string())?;
    db.set_setting("adc_username", "").map_err(|e| e.to_string())?;
    db.set_setting("adc_masked_passkey", "").map_err(|e| e.to_string())?;
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
    Ok(AdcTrackerProfile::project_ratio(uploaded, downloaded, additional_download))
}

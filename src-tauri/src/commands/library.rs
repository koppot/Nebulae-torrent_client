//! Library management commands: user-defined titles, poster ordering,
//! snatched-record persistence, and metadata provider attribution.

use crate::db::Database;
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

// ── Feature 6: Editable listing names ────────────────────────────────────────

/// Set a user-chosen display name for a torrent entry.
///
/// Once set, `user_title_locked = true` prevents automatic metadata syncs
/// (torrent-client polls, tracker updates, rescans) from overwriting the title.
#[tauri::command]
pub fn set_user_display_title(
    info_hash: String,
    title: String,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_user_display_title(&info_hash, &title, true)
        .map_err(|e| format!("set_user_display_title: {e}"))
}

/// Remove the user-defined title lock, restoring automatic metadata management.
///
/// The currently stored `display_title` is left as-is until the next sync
/// overwrites it with provider data.
#[tauri::command]
pub fn reset_user_display_title(
    info_hash: String,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.reset_user_display_title(&info_hash)
        .map_err(|e| format!("reset_user_display_title: {e}"))
}

// ── Feature 3: Poster ordering ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MediaOrderEntry {
    pub info_hash: String,
    pub sort_order: Option<i64>,
}

/// Persist custom poster order for a batch of media entries.
/// Null `sort_order` moves an entry to the "unordered / newly added" tail.
#[tauri::command]
pub fn set_media_order(
    orders: Vec<MediaOrderEntry>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    for entry in &orders {
        db.set_torrent_sort_order(&entry.info_hash, entry.sort_order)
            .map_err(|e| format!("set_torrent_sort_order({}): {e}", entry.info_hash))?;
    }
    let mut sorted_entries = orders;
    sorted_entries.sort_by_key(|e| e.sort_order.unwrap_or(i64::MAX));
    let hashes: Vec<String> = sorted_entries.into_iter().map(|e| e.info_hash).collect();
    if let Ok(json) = serde_json::to_string(&hashes) {
        let _ = db.set_setting("media_downloaded_order", &json);
    }
    Ok(())
}

/// Retrieve the custom ordered list of downloaded torrent info_hashes.
#[tauri::command]
pub fn get_downloaded_media_order(db: State<'_, Arc<Database>>) -> Result<Vec<String>, String> {
    db.get_downloaded_media_order()
        .map_err(|e| format!("get_downloaded_media_order: {e}"))
}

/// Retrieve the custom ordered list of external media paths.
#[tauri::command]
pub fn get_external_media_order(db: State<'_, Arc<Database>>) -> Result<Vec<String>, String> {
    db.get_external_media_order()
        .map_err(|e| format!("get_external_media_order: {e}"))
}

/// Persist the custom ordered list of external media paths.
#[tauri::command]
pub fn set_external_media_order(
    paths: Vec<String>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_external_media_order(&paths)
        .map_err(|e| format!("set_external_media_order: {e}"))
}

// ── Feature 4: Posters-per-row preference ────────────────────────────────────

/// Read the stored posters-per-row preference (default 5).
#[tauri::command]
pub fn get_posters_per_row(db: State<'_, Arc<Database>>) -> Result<u32, String> {
    let val = db
        .get_setting("media_posters_per_row")
        .map_err(|e| format!("get_posters_per_row: {e}"))?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5);
    Ok(val)
}

/// Persist the posters-per-row preference.
/// Clamped to 2–12; values outside this range are clamped silently.
#[tauri::command]
pub fn set_posters_per_row(count: u32, db: State<'_, Arc<Database>>) -> Result<(), String> {
    let clamped = count.clamp(2, 12);
    db.set_setting("media_posters_per_row", &clamped.to_string())
        .map_err(|e| format!("set_posters_per_row: {e}"))
}

// ── Feature 8/9: Metadata provider attribution ────────────────────────────────

/// Store provider attribution (tmdb/imdb/mal) on an existing torrent entry.
/// Does not create a new entry — the torrent must already exist in the DB.
#[tauri::command]
pub fn set_media_provider(
    info_hash: String,
    provider: String,
    provider_id: String,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_media_provider(&info_hash, &provider, &provider_id)
        .map_err(|e| format!("set_media_provider: {e}"))
}

/// Persist a chosen poster path or URL for a torrent.
#[tauri::command]
pub fn set_torrent_poster(
    info_hash: String,
    poster_path: String,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_torrent_poster(&info_hash, &poster_path)
        .map_err(|e| format!("set_torrent_poster: {e}"))
}

/// Retrieve the persisted poster path or URL for a torrent.
#[tauri::command]
pub fn get_torrent_poster(
    info_hash: String,
    db: State<'_, Arc<Database>>,
) -> Result<Option<String>, String> {
    db.get_torrent_poster(&info_hash)
        .map_err(|e| format!("get_torrent_poster: {e}"))
}

/// Unified metadata and poster saving command supporting Downloaded (info-hash)
/// and External media (filepath).
#[tauri::command]
pub fn save_media_metadata_and_poster(
    app: tauri::AppHandle,
    id_or_hash_or_path: String,
    title: String,
    provider: String,
    provider_id: String,
    poster_url: Option<String>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let clean = id_or_hash_or_path.trim();
    let is_hash = clean.len() == 40 && clean.chars().all(|c| c.is_ascii_hexdigit());

    if is_hash {
        let hash = clean.to_lowercase();
        let _ = db.set_user_display_title(&hash, &title, true);
        let _ = db.set_media_provider(&hash, &provider, &provider_id);
        if let Some(ref p) = poster_url {
            let _ = db.set_torrent_poster(&hash, p);
        }
    } else {
        // External media path or title
        let _ = crate::commands::media::set_external_media_metadata_and_poster(
            app,
            clean.to_string(),
            title.clone(),
            poster_url.clone(),
        );

        // Also check if any torrent_metadata row has matching save_path
        if let Ok(all) = db.list_all_torrent_metadata() {
            for (h, m) in all {
                if let Some(ref sp) = m.save_path {
                    if sp == clean || clean.starts_with(sp) {
                        let _ = db.set_user_display_title(&h, &title, true);
                        let _ = db.set_media_provider(&h, &provider, &provider_id);
                        if let Some(ref p) = poster_url {
                            let _ = db.set_torrent_poster(&h, p);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

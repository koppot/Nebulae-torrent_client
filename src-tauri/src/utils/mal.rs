//! MyAnimeList API integration for anime metadata and posters.
//!
//! Uses the official MyAnimeList API with the bundled Client ID.
//! Results are cached in the DB per title.

use std::path::PathBuf;

use reqwest::blocking::Client;
use serde::Deserialize;
use tracing::debug;

use crate::db::Database;

const MAL_BASE: &str = "https://api.myanimelist.net/v2";

/// MyAnimeList Client ID - bundled for out-of-box functionality
const BUNDLED_MAL_CLIENT_ID: &str = "1330c51140c852a04b06e7461d0e2981";

/// Cached path to anime poster (WebP format)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MalAnimeResponse {
    id: i32,
    title: String,
    main_picture: Option<MalPicture>,
    synonyms: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    synopsis: Option<String>,
    mean: Option<f32>,
    rank: Option<i32>,
    popularity: Option<i32>,
    num_list_users: Option<i32>,
    num_scoring_users: Option<i32>,
    rating: Option<String>,
    genres: Option<Vec<MalGenre>>,
    media_type: Option<String>,
    status: Option<String>,
    poster_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MalPicture {
    large: Option<String>,
    medium: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MalGenre {
    id: i32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct MalSearchResponse {
    data: Vec<MalSearchItem>,
}

#[derive(Debug, Deserialize)]
struct MalSearchItem {
    node: MalAnimeResponse,
}

/// Fetch anime poster and metadata from MyAnimeList
/// Returns None if no match or API unavailable
pub fn fetch_anime_poster(
    db: &Database,
    lookup_key: &str,
    title: &str,
) -> Result<Option<PathBuf>, String> {
    let cache_key = format!("mal_poster:{}", lookup_key.to_lowercase());

    // Check cache first
    if let Ok(Some(cached)) = db.get_setting(&cache_key) {
        let path = PathBuf::from(&cached);
        if path.exists() {
            debug!("[mal] Using cached poster for {}", lookup_key);
            return Ok(Some(path));
        }
    }

    let client = Client::new();
    let mal_id = BUNDLED_MAL_CLIENT_ID;

    // Search MyAnimeList for the title
    let search_url = format!(
        "{}/anime?q={}&limit=5&fields=id,title,main_picture,synopsis,start_date",
        MAL_BASE,
        urlencoding::encode(title)
    );

    debug!("[mal] Searching: {}", search_url);

    let resp = client
        .get(&search_url)
        .header("X-MAL-CLIENT-ID", mal_id)
        .send()
        .map_err(|e| format!("MAL search request: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("MAL search returned {}", resp.status()));
    }

    let search_result: MalSearchResponse = resp.json().map_err(|e| format!("MAL parse: {e}"))?;

    // Find the best match
    for item in search_result.data {
        let anime = item.node;

        // Check if title matches (case-insensitive)
        let title_lower = title.to_lowercase();
        let anime_title_lower = anime.title.to_lowercase();

        if anime_title_lower.contains(&title_lower) || title_lower.contains(&anime_title_lower) {
            // Found a match
            if let Some(picture) = anime.main_picture {
                if let Some(large) = picture.large.or(picture.medium) {
                    // Download and cache the poster
                    return download_and_cache_poster(db, &cache_key, &large, &anime.synopsis);
                }
            }
        }
    }

    // No match found
    Ok(None)
}

/// Download poster and cache it
fn download_and_cache_poster(
    db: &Database,
    cache_key: &str,
    poster_url: &str,
    synopsis: &Option<String>,
) -> Result<Option<PathBuf>, String> {
    let client = Client::new();

    let resp = client
        .get(poster_url)
        .send()
        .map_err(|e| format!("MAL poster download: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("MAL poster returned {}", resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| format!("MAL poster body: {e}"))?;

    // Save as JPEG (will be converted to WebP)
    let thumbs_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Nebulae")
        .join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| format!("Cache dir: {e}"))?;

    let hash = hash_of(cache_key);
    let raw_path = thumbs_dir.join(format!("mal-{}{}.jpg", hash, hash_of(poster_url)));
    std::fs::write(&raw_path, &bytes).map_err(|e| format!("Write poster: {e}"))?;

    // Convert to WebP
    let webp = crate::thumbs::ensure_thumbnail(&thumbs_dir, &raw_path).ok();
    let webp_path = webp.as_ref().cloned().unwrap_or(raw_path.clone());

    if webp.is_some() {
        let _ = std::fs::remove_file(&raw_path);
    }

    // Cache the path and synopsis
    let _ = db.set_setting(cache_key, &webp_path.display().to_string());
    if let Some(syn) = synopsis {
        let _ = db.set_setting(&format!("{}_overview", cache_key), syn);
    }

    Ok(Some(webp_path))
}

/// Simple hash function for cache keys
fn hash_of(key: &str) -> u64 {
    let mut hash: u64 = 1469598103934665603;
    for b in key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Check if MAL API is configured (always true - bundled Client ID)
pub fn mal_configured() -> bool {
    true
}

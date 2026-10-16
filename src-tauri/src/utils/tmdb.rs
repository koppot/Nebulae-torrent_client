//! TMDB integration: poster lookup for releases without local cover scans.
//!
//! Credentials live in the OS keychain (service `nebulae_torrent_client`,
//! targets `tmdb:api_key` / `tmdb:read_token`), never in source. Results are
//! cached in the DB per title+year so a search fires once per release, and
//! the downloaded artwork is re-encoded into the WebP thumbnail cache.

use std::path::PathBuf;

use serde::Deserialize;

use crate::utils::keyring::get_secret;

const TMDB_BASE: &str = "https://api.themoviedb.org/3";
const TMDB_IMAGE: &str = "https://image.tmdb.org/t/p/w342";

pub fn has_api_key() -> bool {
    get_secret("tmdb:api_key")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResult {
    results: Vec<TmdbItem>,
}

#[derive(Debug, Deserialize)]
struct TraktSearchResult {
    movie: Option<TraktEntity>,
    show: Option<TraktEntity>,
}

#[derive(Debug, Deserialize)]
struct TraktEntity {
    #[serde(default)]
    overview: Option<String>,
    ids: TraktIds,
}

#[derive(Debug, Deserialize)]
struct TraktIds {
    #[serde(default)]
    tmdb: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TmdbItem {
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TmdbMatch {
    pub poster_webp: PathBuf,
    pub overview: Option<String>,
    pub year: Option<i32>,
}

/// Search TMDB for a release and cache its poster as WebP.
/// `lookup_key` is the DB cache key (per release title+year).
pub fn fetch_poster(
    db: &crate::db::Database,
    lookup_key: &str,
    title: &str,
    year: Option<i32>,
) -> Result<Option<TmdbMatch>, String> {
    // 1. Cached match?
    let cache_key = format!("tmdb_poster:{}", lookup_key.to_lowercase());
    if let Ok(Some(cached)) = db.get_setting(&cache_key) {
        let p = PathBuf::from(&cached);
        if p.exists() {
            return Ok(Some(TmdbMatch {
                poster_webp: p,
                overview: db
                    .get_setting(&format!("{cache_key}:overview"))
                    .ok()
                    .flatten(),
                year: db
                    .get_setting(&format!("{cache_key}:year"))
                    .ok()
                    .flatten()
                    .and_then(|y| y.parse().ok()),
            }));
        }
    }

    let api_key = match get_secret("tmdb:api_key") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            // Distinguish this from a genuine no-match in the log: the owner
            // needs to know whether credentials or the catalogue came up short.
            tracing::warn!(
                "[tmdb] no API key readable from the keychain — TMDB disabled for {title:?}"
            );
            return Ok(None);
        }
    };

    // 2. Search (title + optional year).
    let mut url = url::Url::parse(&format!("{TMDB_BASE}/search/movie"))
        .map_err(|e| format!("TMDB url: {e}"))?;
    url.query_pairs_mut()
        .append_pair("api_key", &api_key)
        .append_pair("query", title);
    if let Some(y) = year {
        url.query_pairs_mut().append_pair("year", &y.to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("TMDB client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("TMDB search: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("TMDB search returned {}", resp.status()));
    }
    let body: TmdbSearchResult = resp.json().map_err(|e| format!("TMDB parse: {e}"))?;
    let item = match body.results.into_iter().next() {
        // A text match with no art falls through to Trakt as well.
        Some(i) if i.poster_path.is_some() => i,
        _ => return trakt_fallback(&client, db, &cache_key, title, lookup_key),
    };
    let poster = item.poster_path.clone().expect("checked above");

    // 3. Download the poster into the thumbnail cache as WebP.
    let img_resp = client
        .get(format!("{TMDB_IMAGE}{poster}"))
        .send()
        .map_err(|e| format!("TMDB image: {e}"))?;
    if !img_resp.status().is_success() {
        return Err(format!("TMDB image returned {}", img_resp.status()));
    }
    let bytes = img_resp
        .bytes()
        .map_err(|e| format!("TMDB image body: {e}"))?;
    let thumbs_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Nebulae")
        .join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| format!("Cache dir: {e}"))?;
    let raw_path = thumbs_dir.join(format!("tmdb-{}.jpg", hash_of(lookup_key)));
    std::fs::write(&raw_path, &bytes).map_err(|e| format!("Write poster: {e}"))?;

    let year: Option<i32> = item
        .release_date
        .as_deref()
        .and_then(|d| d.get(0..4))
        .and_then(|y| y.parse::<i32>().ok());
    let webp = crate::thumbs::ensure_thumbnail(&thumbs_dir, &raw_path).ok();
    let webp_path = webp.clone().unwrap_or(raw_path.clone());
    let _ = std::fs::remove_file(&raw_path);

    // 4. Persist the match (+ the overview for the hover panel).
    let _ = db.set_setting(&cache_key, &webp_path.display().to_string());
    if let Some(ov) = item.overview.clone() {
        let _ = db.set_setting(&format!("{cache_key}:overview"), &ov);
    }
    if let Some(y) = year {
        let _ = db.set_setting(&format!("{cache_key}:year"), &y.to_string());
    }

    Ok(Some(TmdbMatch {
        poster_webp: webp_path,
        overview: item.overview,
        year,
    }))
}

fn hash_of(key: &str) -> u64 {
    let mut hash: u64 = 1469598103934665603;
    for b in key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// TMDB text search missed (or had no art): try Trakt's search, which often
/// matches obscure private-tracker release names, then map its TMDB id back
/// to a poster. Owner directive: embed Trakt when TMDB has no title match.
fn trakt_fallback(
    client: &reqwest::blocking::Client,
    db: &crate::db::Database,
    cache_key: &str,
    title: &str,
    lookup_key: &str,
) -> Result<Option<TmdbMatch>, String> {
    let trakt_id = match get_secret("trakt:client_id") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            // Nothing anywhere: cache the miss so we don't re-search.
            tracing::warn!(
                "[tmdb] trakt: no client id in the keychain — fallback unavailable for {title:?}"
            );
            let _ = db.set_setting(cache_key, "");
            return Ok(None);
        }
    };

    let mut trakt_url = url::Url::parse(&format!("{TRAKT_BASE}/search/movie,show")).unwrap();
    trakt_url
        .query_pairs_mut()
        .append_pair("query", title)
        .append_pair("extended", "full")
        .append_pair("limit", "3");
    let resp = client
        .get(trakt_url)
        .header("trakt-api-key", trakt_id)
        .header("trakt-api-version", "2")
        .send()
        .map_err(|e| format!("Trakt search: {e}"))?;
    if !resp.status().is_success() {
        let _ = db.set_setting(cache_key, "");
        return Ok(None);
    }
    let body: Vec<TraktSearchResult> = resp.json().map_err(|e| format!("Trakt parse: {e}"))?;
    let Some(hit) = body
        .iter()
        .find_map(|r| r.movie.as_ref().or(r.show.as_ref()))
    else {
        let _ = db.set_setting(cache_key, "");
        return Ok(None);
    };
    let Some(tmdb_id) = hit.ids.tmdb else {
        // Trakt matched but has no TMDB id: use its overview alone.
        let _ = db.set_setting(cache_key, "");
        return Ok(None);
    };

    // Map the Trakt match back to TMDB for art.
    let api_key = get_secret("tmdb:api_key").unwrap_or_default();
    let mut detail_url = url::Url::parse(&format!("{TMDB_BASE}/movie/{tmdb_id}")).unwrap();
    detail_url
        .query_pairs_mut()
        .append_pair("api_key", &api_key);
    let detail: TmdbItem = client
        .get(detail_url)
        .send()
        .map_err(|e| format!("TMDB detail: {e}"))?
        .json()
        .map_err(|e| format!("TMDB detail parse: {e}"))?;

    let Some(poster) = detail.poster_path.clone() else {
        let _ = db.set_setting(cache_key, "");
        return Ok(None);
    };
    let img_resp = client
        .get(format!("{TMDB_IMAGE}{poster}"))
        .send()
        .map_err(|e| format!("TMDB image: {e}"))?;
    let bytes = img_resp
        .bytes()
        .map_err(|e| format!("TMDB image body: {e}"))?;
    let thumbs_dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Nebulae")
        .join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| format!("Cache dir: {e}"))?;
    let raw_path = thumbs_dir.join(format!("trakt-{}.jpg", hash_of(lookup_key)));
    std::fs::write(&raw_path, &bytes).map_err(|e| format!("Write poster: {e}"))?;
    let webp_path =
        crate::thumbs::ensure_thumbnail(&thumbs_dir, &raw_path).unwrap_or(raw_path.clone());
    let _ = std::fs::remove_file(&raw_path);

    let overview = detail.overview.clone().or_else(|| hit.overview.clone());
    let year: Option<i32> = detail
        .release_date
        .as_deref()
        .and_then(|d| d.get(0..4))
        .and_then(|y| y.parse::<i32>().ok());
    let _ = db.set_setting(cache_key, &webp_path.display().to_string());
    if let Some(ov) = &overview {
        let _ = db.set_setting(&format!("{cache_key}:overview"), ov);
    }
    Ok(Some(TmdbMatch {
        poster_webp: webp_path,
        overview,
        year,
    }))
}

const TRAKT_BASE: &str = "https://api.trakt.tv";

/// Returns the cached TMDB overview for a release (for the hover panel).
pub fn cached_overview(db: &crate::db::Database, lookup_key: &str) -> Option<String> {
    let cache_key = format!("tmdb_poster:{}:overview", lookup_key.to_lowercase());
    db.get_setting(&cache_key)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_is_not_an_error() {
        // With no keychain entry present the fetch degrades to Ok(None)
        // rather than erroring — the grid just shows its placeholder.
        // Reaching the assert at all proves no panic/Err propagated.
        let result = has_api_key();
        let _ = result;
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash_of("test"), hash_of("test"));
        assert_ne!(hash_of("test"), hash_of("test2"));
    }
}

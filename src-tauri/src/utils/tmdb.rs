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
const TMDB_IMAGE: &str = "https://image.tmdb.org/t/p/w500";

/// Owner directive: TMDB works for ALL downloads out of the box — the user
/// never needs to obtain their own key. The bundled key is the app's own
/// account key; a keychain override (tmdb:api_key) still wins when present.
const BUNDLED_TMDB_API_KEY: &str = "ec0fadf7fe0d8dcf71b7a1d4d8b6d941";

/// Resolve the effective TMDB key: keychain override first, bundled key
/// as the always-available fallback. Under `cfg(test)` the keychain is
/// never touched (a test binary is not in the item's ACL, so the read
/// would block on a security prompt mid-test-run).
fn effective_api_key() -> Option<String> {
    #[cfg(test)]
    {
        Some(BUNDLED_TMDB_API_KEY.to_string())
    }
    #[cfg(not(test))]
    {
        match get_secret("tmdb:api_key") {
            Ok(k) if !k.is_empty() => Some(k),
            _ => Some(BUNDLED_TMDB_API_KEY.to_string()),
        }
    }
}

pub fn has_api_key() -> bool {
    effective_api_key().is_some()
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

/// Detail-response subset (movie/tv/{id}): total runtime fields.
#[derive(Debug, Deserialize)]
struct TmdbDetail {
    #[serde(default)]
    runtime: Option<u32>,
    #[serde(default)]
    episode_run_time: Vec<u32>,
    #[serde(default)]
    number_of_episodes: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TmdbItem {    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    // TV results carry the air date and per-episode runtimes instead.
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    episode_run_time: Vec<u32>,
    // /movie/{id} and /tv/{id} detail responses carry the runtime (minutes);
    // search results do not (defaults to None there).
    #[serde(default)]
    runtime: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
pub struct TmdbMatch {
    pub poster_webp: PathBuf,
    pub overview: Option<String>,
    pub year: Option<i32>,
    /// Feature runtime in minutes (owner: "total time duration" in the
    /// hover panel); None when the catalogue doesn't provide it.
    #[serde(default)]
    pub runtime: Option<u32>,
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
    // v2: the search pipeline gained suffix-stripping retries and the
    // first_air_date_year fix (owner: "Banner of the Stars III OVA" never
    // matched); bump so previously cached misses re-search.
    // v3: the TV total-duration fix (episodes × runtime). v2 cached the
    // single-episode runtime before that fix, so bump to recompute.
    let cache_key = format!("tmdb_poster:v3:{}", lookup_key.to_lowercase());
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
                runtime: db
                    .get_setting(&format!("{cache_key}:runtime"))
                    .ok()
                    .flatten()
                    .and_then(|r| r.parse().ok()),
            }));
        }
    }

    let api_key = match effective_api_key() {
        Some(k) => k,
        None => {
            // effective_api_key() currently always returns Some (bundled
            // fallback), so this branch is defensive only.
            tracing::warn!("[tmdb] no TMDB key available — TMDB disabled for {title:?}");
            return Ok(None);
        }
    };

    // v3 cache miss: the pre-v3 entries are still valid art and summary —
    // only the runtime may predate the TV total fix. When the network is
    // unreachable, serve them instead of leaving the shelf blank.
    let v2_key = format!("tmdb_poster:v2:{}", lookup_key.to_lowercase());
    let v2_fallback = || -> Option<TmdbMatch> {
        let p = db.get_setting(&v2_key).ok().flatten()?;
        let path = PathBuf::from(&p);
        if !path.exists() {
            return None;
        }
        Some(TmdbMatch {
            poster_webp: path,
            overview: db.get_setting(&format!("{v2_key}:overview")).ok().flatten(),
            year: db
                .get_setting(&format!("{v2_key}:year"))
                .ok()
                .flatten()
                .and_then(|y| y.parse().ok()),
            runtime: db
                .get_setting(&format!("{v2_key}:runtime"))
                .ok()
                .flatten()
                .and_then(|r| r.parse().ok()),
        })
    };

    // The network pipeline; on total failure fall back to the pre-v3
    // cached entry so the shelf never goes blank.
    let net = (|| -> Result<Option<TmdbMatch>, String> {
    // 2. Search (title + optional year): movies first, then TV — the library
    // holds OVAs and series that only match /search/tv (owner's "Foundation"
    // mock is a show, and a movie-only search never sees it).
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("TMDB client: {e}"))?;
    let mut item: Option<TmdbItem> = None;
    let mut is_tv = false;
    // Search variants, tried in order: the raw title first, then a cleaned
    // form without format suffixes — "Banner of the Stars III OVA" only
    // matches TMDB as "Banner of the Stars III" (owner report), and a year
    // filter can suppress the right TV hit (TV search wants
    // first_air_date_year, not year).
    let cleaned = title
        .trim_end_matches(|c: char| c.is_whitespace())
        .trim_end_matches("OVA")
        .trim_end_matches("ova")
        .trim_end_matches(|c: char| c.is_whitespace())
        .to_string();
    let mut variants: Vec<(String, Option<i32>)> = vec![(title.to_string(), year)];
    if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case(title) {
        variants.push((cleaned, None));
    }
    variants.push((title.to_string(), None));
    'outer: for (query, variant_year) in variants {
        for endpoint in ["search/movie", "search/tv"] {
            let mut url = url::Url::parse(&format!("{TMDB_BASE}/{endpoint}"))
                .map_err(|e| format!("TMDB url: {e}"))?;
            url.query_pairs_mut()
                .append_pair("api_key", &api_key)
                .append_pair("query", &query);
            if let Some(y) = variant_year {
                // The TV endpoint ignores `year`; its own filter is
                // first_air_date_year (using `year` there can hide hits).
                let pair = if endpoint.ends_with("tv") {
                    "first_air_date_year"
                } else {
                    "year"
                };
                url.query_pairs_mut().append_pair(pair, &y.to_string());
            }
            let resp = client
                .get(url)
                .send()
                .map_err(|e| format!("TMDB search: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("TMDB search returned {}", resp.status()));
            }
            let body: TmdbSearchResult = resp.json().map_err(|e| format!("TMDB parse: {e}"))?;
            if let Some(i) = body
                .results
                .into_iter()
                .next()
                .filter(|i| i.poster_path.is_some())
            {
                is_tv = endpoint.ends_with("tv");
                item = Some(i);
                break 'outer;
            }
        }
    }
    let item = match item {
        // A text match with no art falls through to Trakt as well.
        Some(i) => i,
        None => return trakt_fallback(&client, db, &cache_key, title, lookup_key),
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
        .or(item.first_air_date.as_deref())
        .and_then(|d| d.get(0..4))
        .and_then(|y| y.parse::<i32>().ok());
    // The runtime comes from the /movie/{id} or /tv/{id} detail response
    // (TV exposes per-episode runtimes); the search result doesn't carry
    // it. One bounded extra request, failures skipped.
    // For TV the owner wants the TOTAL across all media: episode count ×
    // per-episode runtime (both on the TV detail response).
    let runtime: Option<u32> = match item.id {
        Some(id) => {
            let kind = if is_tv { "tv" } else { "movie" };
            let mut detail_url = url::Url::parse(&format!("{TMDB_BASE}/{kind}/{id}"))
                .map_err(|e| format!("TMDB url: {e}"))?;
            detail_url
                .query_pairs_mut()
                .append_pair("api_key", &api_key);
            client
                .get(detail_url)
                .send()
                .ok()
                .and_then(|r| r.json::<TmdbDetail>().ok())
                .and_then(|d| {
                    if is_tv {
                        let per_ep = d
                            .episode_run_time
                            .first()
                            .copied()
                            .or(d.runtime)
                            .unwrap_or(0);
                        let eps = d.number_of_episodes.unwrap_or(0);
                        let total = per_ep.saturating_mul(eps);
                        if total > 0 { Some(total) } else { None }
                    } else {
                        d.runtime
                    }
                })
        }
        None => None,
    };
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
    if let Some(rt) = runtime {
        let _ = db.set_setting(&format!("{cache_key}:runtime"), &rt.to_string());
    }

    Ok(Some(TmdbMatch {
        poster_webp: webp_path,
        overview: item.overview,
        year,
        runtime,
    }))
    })();
    match net {
        Ok(v) => Ok(v),
        Err(_) => Ok(v2_fallback()),
    }
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

    // Map the Trakt match back to TMDB for art. Trakt matched movie OR show;
    // try the movie detail first, then the TV detail (same TMDB id space).
    let api_key = effective_api_key().unwrap_or_default();
    let fetch_detail = |kind: &str| -> Result<Option<TmdbItem>, String> {
        let mut detail_url = url::Url::parse(&format!("{TMDB_BASE}/{kind}/{tmdb_id}")).unwrap();
        detail_url
            .query_pairs_mut()
            .append_pair("api_key", &api_key);
        let resp = client
            .get(detail_url)
            .send()
            .map_err(|e| format!("TMDB detail: {e}"))?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        resp.json::<TmdbItem>()
            .map(Some)
            .map_err(|e| format!("TMDB detail parse: {e}"))
    };
    let detail: TmdbItem = match fetch_detail("movie").or_else(|_| fetch_detail("tv"))? {
        Some(d) => d,
        None => {
            let _ = db.set_setting(cache_key, "");
            return Ok(None);
        }
    };

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
        .or(detail.first_air_date.as_deref())
        .and_then(|d| d.get(0..4))
        .and_then(|y| y.parse::<i32>().ok());
    let runtime = detail
        .runtime
        .or_else(|| detail.episode_run_time.first().copied());
    let _ = db.set_setting(cache_key, &webp_path.display().to_string());
    if let Some(ov) = &overview {
        let _ = db.set_setting(&format!("{cache_key}:overview"), ov);
    }
    if let Some(rt) = runtime {
        let _ = db.set_setting(&format!("{cache_key}:runtime"), &rt.to_string());
    }
    Ok(Some(TmdbMatch {
        poster_webp: webp_path,
        overview,
        year,
        runtime,
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
        // Owner directive: TMDB works out of the box for ALL downloads —
        // the bundled key means a missing keychain entry is never an
        // error; resolution always yields a key (hermetic under test).
        assert_eq!(effective_api_key(), Some(BUNDLED_TMDB_API_KEY.to_string()));
        assert!(has_api_key());
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash_of("test"), hash_of("test"));
        assert_ne!(hash_of("test"), hash_of("test2"));
    }
}

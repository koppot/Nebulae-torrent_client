use crate::commands::torrent::InspectTorrentResult;
use crate::db::Database;
use crate::engine::metainfo::ParsedTorrent;
use crate::utils::keyring::{get_secret, passkey_secret_target};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tauri::State;
use tokio::sync::broadcast;

type InFlightSender = broadcast::Sender<Result<Vec<u8>, String>>;
type InFlightFetchMap = Mutex<HashMap<u64, InFlightSender>>;

/// In-flight request deduplication map to prevent redundant concurrent fetches
/// to the ADC tracker API for the same torrent ID.
static IN_FLIGHT_FETCHES: LazyLock<InFlightFetchMap> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Target torrent client for multi-client routing (e.g. Local Nebulae vs Remote Seedbox).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdcClientTarget {
    pub id: String,
    pub name: String,
    pub priority: u32,
    pub enabled: bool,
    pub client_type: String, // "local" | "remote_server" | "custom_agent"
}

/// Setting key for persisting multi-client target priorities.
pub const SETTING_ADC_CLIENT_TARGETS: &str = "adc_client_targets";

/// Setting key for suppressing the default download directory warning.
pub const SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING: &str =
    "adc_suppress_default_download_warning";

/// Strict separation of `adc:` protocol invocations (E11 / G13 / D12 / D13):
/// - `ManualTorrentId`: user-initiated manual references (`adc://torrent/<id>`,
///   `adc://download/<id>`, `adc://<id>`) that MUST go through `AddTorrentModal`
///   for preview and explicit user confirmation.
/// - `DeliveryRequestToken`: opaque 64-hex website-created delivery tokens
///   (`adc://delivery/<token>`, `adc://request/<token>`) redeemed through the
///   authenticated member/device-bound queue before automatic download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdcUriKind {
    ManualTorrentId(u64),
    DeliveryRequestToken(String),
}

/// Validate that a token is an opaque 64-character hexadecimal delivery token.
pub fn is_valid_delivery_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Classify a custom `adc://` or `adc:` URI into either a manual torrent
/// reference or an opaque website delivery request token.
pub fn classify_adc_uri(uri: &str) -> Result<AdcUriKind, String> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();

    if !lower.starts_with("adc:") {
        return Err(format!("Not an ADC protocol URI: '{trimmed}'"));
    }

    // Strip scheme prefix ("adc://" or "adc:")
    let without_scheme = if lower.starts_with("adc://") {
        &trimmed[6..]
    } else {
        &trimmed[4..]
    };

    // Split fragment and query string while keeping query parameters available for token fallback.
    let without_fragment = without_scheme.split('#').next().unwrap_or("");
    let mut path_and_query = without_fragment.splitn(2, '?');
    let clean_path = path_and_query.next().unwrap_or("").trim_matches('/');
    let query_str = path_and_query.next().unwrap_or("");

    let segments: Vec<&str> = clean_path.split('/').filter(|s| !s.is_empty()).collect();

    if let Some(&first) = segments.first() {
        let kind_lower = first.to_ascii_lowercase();
        if kind_lower == "delivery" || kind_lower == "request" {
            let candidate = if segments.len() >= 2 {
                segments[1].trim()
            } else {
                query_str
                    .split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .find(|(k, _)| {
                        k.eq_ignore_ascii_case("token") || k.eq_ignore_ascii_case("request_token")
                    })
                    .map(|(_, v)| v.trim())
                    .unwrap_or("")
            };
            if is_valid_delivery_token(candidate) {
                return Ok(AdcUriKind::DeliveryRequestToken(
                    candidate.to_ascii_lowercase(),
                ));
            }
            return Err(format!(
                "Invalid ADC delivery request URI '{trimmed}': expected a 64-character hex request token."
            ));
        }
    }

    let id_str = match segments.as_slice() {
        ["torrent", id, ..] | ["download", id, ..] => *id,
        [id] => *id,
        _ => {
            return Err(format!(
                "Invalid ADC torrent URI format '{trimmed}'. Expected adc://torrent/<id>, adc://download/<id>, or adc://delivery/<token>"
            ));
        }
    };

    let parsed_id = id_str.parse::<u64>().map_err(|_| {
        format!("Invalid torrent ID '{id_str}' in ADC URI '{trimmed}'. Must be a positive integer.")
    })?;
    if parsed_id == 0 {
        return Err(format!(
            "Invalid torrent ID '{id_str}' in ADC URI '{trimmed}'. Must be greater than zero."
        ));
    }
    Ok(AdcUriKind::ManualTorrentId(parsed_id))
}

/// Message for the tracker's one-download-per-30-seconds-per-account limit (HTTP 429).
pub const DOWNLOAD_RATE_LIMIT_MSG: &str =
    "ADC download rate limit reached. Please wait a moment before retrying.";

/// Parse a manual `adc://` or `adc:` torrent reference to extract the positive torrent ID.
///
/// Delivery request tokens (`adc://delivery/<token>`, `adc://request/<token>`) are
/// explicitly rejected here so they cannot be conflated with manual torrent references.
pub fn parse_adc_uri(uri: &str) -> Result<u64, String> {
    match classify_adc_uri(uri)? {
        AdcUriKind::ManualTorrentId(id) => Ok(id),
        AdcUriKind::DeliveryRequestToken(_) => Err(
            "Website delivery request URIs must be redeemed via the authenticated delivery flow, not resolved as bare torrent IDs.".to_string(),
        ),
    }
}

/// Fetch personalized `.torrent` bencoded metainfo bytes from AsianDVDClub.
///
/// Authenticates via the user's stored account passkey from OS Keychain.
/// Validates response status codes and bencoded metainfo structure.
pub fn fetch_adc_torrent_bytes_sync(torrent_id: u64, db: &Database) -> Result<Vec<u8>, String> {
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

    let passkey = match get_secret(&passkey_secret_target())
        .ok()
        .filter(|pk| !pk.trim().is_empty())
        .or_else(|| {
            db.get_setting("adc_passkey")
                .ok()
                .flatten()
                .filter(|pk| !pk.trim().is_empty())
        }) {
        Some(pk) => pk.trim().to_string(),
        None => {
            return Err(
                "ADC passkey is not available in Keychain. Please unlock or re-link your ADC account."
                    .to_string(),
            );
        }
    };

    let base_url = "https://asiandvdclub.org";
    let endpoint = format!("{base_url}/api/nebulae/torrent_download");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client initialization failed: {e}"))?;

    let payload = serde_json::json!({
        "passkey": passkey,
        "torrent_id": torrent_id
    });

    let resp = client
        .post(&endpoint)
        .json(&payload)
        .send()
        .map_err(|e| format!("Could not connect to AsianDVDClub: {e}"))?;

    let status = resp.status();

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(DOWNLOAD_RATE_LIMIT_MSG.to_string());
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "Torrent #{torrent_id} was not found on AsianDVDClub."
        ));
    }

    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        // Attempt to extract structured error detail if returned as JSON
        if let Ok(err_body) = resp.json::<serde_json::Value>() {
            let reason = err_body
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_lowercase();
            let err_code = err_body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_lowercase();

            if reason.contains("ratio") || err_code.contains("ratio") {
                return Err("ADC download denied: Your ratio is below the minimum required or download quota exceeded.".to_string());
            } else if reason.contains("warn") || err_code.contains("warn") {
                return Err(
                    "ADC download denied: Account is currently in Warned status.".to_string(),
                );
            } else if reason.contains("class")
                || reason.contains("limited")
                || err_code.contains("limited")
            {
                return Err(
                    "ADC download denied: User class is restricted from downloading.".to_string(),
                );
            } else if reason.contains("passkey") || err_code.contains("passkey") {
                return Err(
                    "ADC passkey was rejected by tracker. Please re-link your ADC account."
                        .to_string(),
                );
            }
        }
        return Err(
            "ADC download denied (HTTP 403 Forbidden). Check account standing or ratio."
                .to_string(),
        );
    }

    if status.is_server_error() {
        return Err(format!(
            "AsianDVDClub server error (HTTP {}) while preparing the torrent. The request stays queued; retry shortly.",
            status.as_u16()
        ));
    }

    if !status.is_success() {
        return Err(format!(
            "ADC tracker returned unexpected HTTP status {status}."
        ));
    }

    let bytes = resp
        .bytes()
        .map_err(|e| format!("Failed to read response bytes: {e}"))?
        .to_vec();

    if bytes.is_empty() {
        return Err("ADC returned empty torrent metainfo.".to_string());
    }

    // Validate that the returned bytes form a valid bencoded torrent
    ParsedTorrent::parse_bytes(&bytes)
        .map_err(|e| format!("Downloaded metainfo from ADC is not a valid torrent: {e}"))?;

    Ok(bytes)
}

/// Asynchronous wrapper around ADC torrent fetch with in-memory deduplication.
pub async fn fetch_adc_torrent_bytes(
    torrent_id: u64,
    db: Arc<Database>,
) -> Result<Vec<u8>, String> {
    // 1. Check if another task is already fetching this torrent_id
    let mut rx = {
        let mut in_flight = IN_FLIGHT_FETCHES
            .lock()
            .map_err(|e| format!("Lock poison: {e}"))?;
        if let Some(tx) = in_flight.get(&torrent_id) {
            Some(tx.subscribe())
        } else {
            let (tx, _) = broadcast::channel(2);
            in_flight.insert(torrent_id, tx);
            None
        }
    };

    // If an in-flight fetch exists, wait for its result
    if let Some(ref mut subscriber) = rx {
        match subscriber.recv().await {
            Ok(res) => return res,
            Err(e) => {
                tracing::warn!(
                    "[adc-download] in-flight broadcast subscription failed: {e}; falling back to direct fetch"
                );
            }
        }
    }

    // 2. Perform the fetch in a blocking task
    let db_clone = Arc::clone(&db);
    let result =
        tokio::task::spawn_blocking(move || fetch_adc_torrent_bytes_sync(torrent_id, &db_clone))
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    // 3. Broadcast result to any waiting callers and clean up map
    {
        if let Ok(mut in_flight) = IN_FLIGHT_FETCHES.lock() {
            if let Some(tx) = in_flight.remove(&torrent_id) {
                let _ = tx.send(result.clone());
            }
        }
    }

    result
}

/// Tauri command: Resolve an `adc://torrent/<id>` or `adc://download/<id>` link,
/// fetch the signed personalized `.torrent` file from ADC, and return its inspected metainfo.
#[tauri::command]
pub async fn resolve_adc_torrent(
    uri: String,
    db: State<'_, Arc<Database>>,
) -> Result<InspectTorrentResult, String> {
    let torrent_id = parse_adc_uri(&uri)?;
    let bytes = fetch_adc_torrent_bytes(torrent_id, Arc::clone(&db)).await?;
    let parsed = ParsedTorrent::parse_bytes(&bytes)?;

    let file_paths: Vec<String> = parsed.files.iter().map(|f| f.name.clone()).collect();
    let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(&file_paths));

    Ok(InspectTorrentResult {
        name: parsed.name,
        total_bytes: parsed.total_bytes,
        piece_size: parsed.piece_size,
        total_pieces: parsed.total_pieces,
        info_hash: parsed.info_hash,
        is_private: parsed.is_private,
        comment: parsed.comment,
        created_by: parsed.created_by,
        files: parsed.files,
        trackers: parsed.trackers,
        metainfo_bytes: Some(bytes),
        hygiene,
    })
}

/// Tauri command: Query whether the default download directory warning is suppressed.
#[tauri::command]
pub fn get_adc_download_warning_suppressed(db: State<'_, Arc<Database>>) -> Result<bool, String> {
    let suppressed = db
        .get_setting(SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING)
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(suppressed)
}

/// Tauri command: Set the suppression preference for the default download directory warning.
#[tauri::command]
pub fn set_adc_download_warning_suppressed(
    suppressed: bool,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_setting(
        SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING,
        if suppressed { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())
}

/// Id and type of the remote Nebulae Server (Transmission) delivery target.
pub const REMOTE_SERVER_TARGET_ID: &str = "remote_server";
pub const REMOTE_SERVER_TARGET_NAME: &str = "Remote server / seedbox (Transmission)";
/// Earlier builds saved this target as "remote_rtorrent"; saved settings are migrated on read.
const LEGACY_REMOTE_TARGET_ID: &str = "remote_rtorrent";

fn default_local_target(priority: u32) -> AdcClientTarget {
    AdcClientTarget {
        id: "local".to_string(),
        name: "Local Nebulae Engine".to_string(),
        priority,
        enabled: true,
        client_type: "local".to_string(),
    }
}

fn default_remote_target(priority: u32) -> AdcClientTarget {
    AdcClientTarget {
        id: REMOTE_SERVER_TARGET_ID.to_string(),
        name: REMOTE_SERVER_TARGET_NAME.to_string(),
        priority,
        enabled: false,
        client_type: REMOTE_SERVER_TARGET_ID.to_string(),
    }
}

/// Returns default list of client targets when none have been configured.
pub fn default_client_targets() -> Vec<AdcClientTarget> {
    vec![default_local_target(1), default_remote_target(2)]
}

/// Brings saved targets up to date without losing the user's choices: the legacy
/// "remote_rtorrent" entry becomes the Nebulae remote-server target (priority and
/// enabled state kept), and a missing local or remote entry is added disabled/last.
pub fn normalize_client_targets(targets: Vec<AdcClientTarget>) -> Vec<AdcClientTarget> {
    let mut out: Vec<AdcClientTarget> = targets
        .into_iter()
        .map(|mut t| {
            if t.id == LEGACY_REMOTE_TARGET_ID || t.client_type == LEGACY_REMOTE_TARGET_ID {
                t.id = REMOTE_SERVER_TARGET_ID.to_string();
                t.client_type = REMOTE_SERVER_TARGET_ID.to_string();
            }
            if t.client_type == REMOTE_SERVER_TARGET_ID {
                t.name = REMOTE_SERVER_TARGET_NAME.to_string();
            }
            t
        })
        .collect();
    out.sort_by_key(|t| t.priority);
    let next = out.len() as u32 + 1;
    if !out.iter().any(|t| t.client_type == "local") {
        out.push(default_local_target(next));
    }
    let next = out.len() as u32 + 1;
    if !out.iter().any(|t| t.client_type == REMOTE_SERVER_TARGET_ID) {
        out.push(default_remote_target(next));
    }
    for (i, t) in out.iter_mut().enumerate() {
        t.priority = i as u32 + 1;
    }
    out
}

/// Enabled delivery targets, highest priority first.
pub fn enabled_delivery_targets(targets: &[AdcClientTarget]) -> Vec<AdcClientTarget> {
    let mut v: Vec<AdcClientTarget> = targets.iter().filter(|t| t.enabled).cloned().collect();
    v.sort_by_key(|t| t.priority);
    v
}

/// Loads the saved targets (migrating legacy entries); defaults when nothing is saved.
pub fn load_client_targets(db: &Database) -> Vec<AdcClientTarget> {
    match db.get_setting(SETTING_ADC_CLIENT_TARGETS).unwrap_or(None) {
        Some(json_str) => match serde_json::from_str::<Vec<AdcClientTarget>>(&json_str) {
            Ok(saved) => normalize_client_targets(saved),
            Err(_) => default_client_targets(),
        },
        None => default_client_targets(),
    }
}

/// Tauri command: Get configured client targets and priorities.
#[tauri::command]
pub fn get_adc_client_targets(
    db: State<'_, Arc<Database>>,
) -> Result<Vec<AdcClientTarget>, String> {
    let raw = db.get_setting(SETTING_ADC_CLIENT_TARGETS).unwrap_or(None);
    match raw {
        Some(json_str) => serde_json::from_str::<Vec<AdcClientTarget>>(&json_str)
            .map(normalize_client_targets)
            .map_err(|e| format!("Failed to parse client targets: {e}")),
        None => Ok(default_client_targets()),
    }
}

/// Tauri command: Save configured client targets and priorities.
#[tauri::command]
pub fn set_adc_client_targets(
    targets: Vec<AdcClientTarget>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let json_str = serde_json::to_string(&targets)
        .map_err(|e| format!("Failed to serialize client targets: {e}"))?;
    db.set_setting(SETTING_ADC_CLIENT_TARGETS, &json_str)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_adc_uri_standard_formats() {
        assert_eq!(parse_adc_uri("adc://torrent/12345").unwrap(), 12345);
        assert_eq!(parse_adc_uri("adc://download/12345").unwrap(), 12345);
        assert_eq!(parse_adc_uri("adc:torrent/12345").unwrap(), 12345);
        assert_eq!(parse_adc_uri("adc:download/12345").unwrap(), 12345);
        assert_eq!(parse_adc_uri("adc://12345").unwrap(), 12345);
        assert_eq!(parse_adc_uri("adc:12345").unwrap(), 12345);
    }

    #[test]
    fn test_parse_adc_uri_with_trailing_slash_and_queries() {
        assert_eq!(parse_adc_uri("adc://torrent/67890/").unwrap(), 67890);
        assert_eq!(
            parse_adc_uri("adc://torrent/67890?title=Awesome+Movie").unwrap(),
            67890
        );
        assert_eq!(
            parse_adc_uri("adc://torrent/67890#section-details").unwrap(),
            67890
        );
        assert_eq!(
            parse_adc_uri("adc://download/67890/?passkey=foo#bar").unwrap(),
            67890
        );
    }

    #[test]
    fn test_parse_adc_uri_invalid() {
        assert!(parse_adc_uri("https://asiandvdclub.org/torrent/12345").is_err());
        assert!(parse_adc_uri("magnet:?xt=urn:btih:abc").is_err());
        assert!(parse_adc_uri("adc://torrent/notanumber").is_err());
        assert!(parse_adc_uri("adc://torrent/").is_err());
        assert!(parse_adc_uri("adc:").is_err());
    }

    #[test]
    fn test_default_client_targets() {
        let targets = default_client_targets();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].id, "local");
        assert!(targets[0].enabled);
        assert_eq!(targets[0].priority, 1);
        assert_eq!(targets[1].id, REMOTE_SERVER_TARGET_ID);
        assert!(!targets[1].enabled);
    }

    #[test]
    fn legacy_remote_rtorrent_target_migrates_without_losing_choices() {
        let saved = vec![
            AdcClientTarget {
                id: "remote_rtorrent".into(),
                name: "Remote Seedbox (rTorrent)".into(),
                priority: 1,
                enabled: true,
                client_type: "remote_rtorrent".into(),
            },
            AdcClientTarget {
                id: "local".into(),
                name: "Local Nebulae Engine".into(),
                priority: 2,
                enabled: false,
                client_type: "local".into(),
            },
        ];
        let out = normalize_client_targets(saved);
        assert_eq!(out[0].id, REMOTE_SERVER_TARGET_ID);
        assert_eq!(out[0].client_type, REMOTE_SERVER_TARGET_ID);
        assert_eq!(out[0].name, REMOTE_SERVER_TARGET_NAME);
        assert!(out[0].enabled);
        assert_eq!(out[1].id, "local");
        assert!(!out[1].enabled);
        let order = enabled_delivery_targets(&out);
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].id, REMOTE_SERVER_TARGET_ID);
    }

    #[test]
    fn missing_targets_are_added_and_unrelated_types_kept() {
        let out = normalize_client_targets(vec![AdcClientTarget {
            id: "agent".into(),
            name: "Custom".into(),
            priority: 1,
            enabled: true,
            client_type: "custom_agent".into(),
        }]);
        assert_eq!(out.len(), 3);
        assert!(out.iter().any(|t| t.client_type == "custom_agent"));
        assert!(out.iter().any(|t| t.client_type == "local"));
        assert!(out
            .iter()
            .any(|t| t.client_type == REMOTE_SERVER_TARGET_ID && !t.enabled));
    }
    #[test]
    fn test_warning_suppression_and_client_targets_db() {
        let db = Database::new(Path::new(":memory:")).expect("in-memory test db");

        // Initial default warning suppression should be false
        let suppressed = db
            .get_setting(SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING)
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false);
        assert!(!suppressed);

        // Set suppression to true
        db.set_setting(SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING, "true")
            .unwrap();
        let suppressed_after = db
            .get_setting(SETTING_ADC_SUPPRESS_DEFAULT_DOWNLOAD_WARNING)
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false);
        assert!(suppressed_after);

        // Test targets serialization and deserialization
        let mut custom_targets = default_client_targets();
        custom_targets[1].enabled = true;
        custom_targets[1].priority = 1;
        custom_targets[0].priority = 2;

        let json = serde_json::to_string(&custom_targets).unwrap();
        db.set_setting(SETTING_ADC_CLIENT_TARGETS, &json).unwrap();

        let loaded_json = db.get_setting(SETTING_ADC_CLIENT_TARGETS).unwrap().unwrap();
        let loaded_targets: Vec<AdcClientTarget> = serde_json::from_str(&loaded_json).unwrap();
        assert_eq!(loaded_targets, custom_targets);
    }

    #[test]
    fn test_unlinked_account_error() {
        let db = Database::new(Path::new(":memory:")).expect("in-memory test db");
        // Not linked
        let result = fetch_adc_torrent_bytes_sync(12345, &db);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ADC account is not linked"));
    }

    #[test]
    fn test_classify_adc_uri_separates_manual_and_delivery_requests() {
        let tok = "A1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0";
        assert_eq!(tok.len(), 64);

        assert_eq!(
            classify_adc_uri("adc://torrent/12345").unwrap(),
            AdcUriKind::ManualTorrentId(12345)
        );
        assert_eq!(
            classify_adc_uri("adc://download/98765").unwrap(),
            AdcUriKind::ManualTorrentId(98765)
        );
        assert_eq!(
            classify_adc_uri(&format!("adc://delivery/{tok}")).unwrap(),
            AdcUriKind::DeliveryRequestToken(tok.to_ascii_lowercase())
        );
        assert_eq!(
            classify_adc_uri(&format!("adc://request/{tok}/?foo=bar")).unwrap(),
            AdcUriKind::DeliveryRequestToken(tok.to_ascii_lowercase())
        );
        assert_eq!(
            classify_adc_uri(&format!("adc://delivery?token={tok}")).unwrap(),
            AdcUriKind::DeliveryRequestToken(tok.to_ascii_lowercase())
        );

        // parse_adc_uri must reject delivery request tokens so manual preview never conflates paths
        assert!(parse_adc_uri(&format!("adc://delivery/{tok}")).is_err());
        assert!(classify_adc_uri("adc://delivery/not-64-hex").is_err());
        assert!(classify_adc_uri("adc://request/12345").is_err());
        assert!(classify_adc_uri("adc://torrent/0").is_err());
    }
}

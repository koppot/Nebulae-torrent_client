use crate::engine::{EngineStatus, SharedEngine};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TorrentOperationKind {
    Remove { delete_files: bool },
    Move { destination: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TorrentOperationPhase {
    Pending,
    RemovingTorrent,
    RemovingTorrentAndDeletingFiles,
    Moving,
    Reconciling,
    Complete,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TorrentOperationSnapshot {
    pub operation_id: String,
    pub torrent_id: String,
    pub kind: TorrentOperationKind,
    pub phase: TorrentOperationPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub revision: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReservationResult {
    Reserved(TorrentOperationSnapshot),
    Duplicate(TorrentOperationSnapshot),
}

fn current_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn register_app_handle(app: &tauri::AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

static ACTIVE_OPERATIONS: OnceLock<Arc<Mutex<HashMap<String, TorrentOperationSnapshot>>>> =
    OnceLock::new();

fn get_active_operations_map() -> &'static Arc<Mutex<HashMap<String, TorrentOperationSnapshot>>> {
    ACTIVE_OPERATIONS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

#[allow(dead_code)]
pub(crate) fn get_active_operation(key: &str) -> Option<TorrentOperationSnapshot> {
    let map = get_active_operations_map().lock().ok()?;
    map.get(key)
        .or_else(|| map.get(&key.to_lowercase()))
        .cloned()
}

pub(crate) fn reserve_active_remove_operation(
    id: &str,
    info_hash: Option<&str>,
    del: bool,
    initial_phase: TorrentOperationPhase,
) -> Result<ReservationResult, String> {
    let mut map = get_active_operations_map()
        .lock()
        .map_err(|e| format!("Registry lock poisoned: {e}"))?;

    let active_lookup_key = info_hash.unwrap_or(id);

    let existing = map
        .get(id)
        .or_else(|| map.get(&id.to_lowercase()))
        .or_else(|| info_hash.and_then(|h| map.get(h).or_else(|| map.get(&h.to_lowercase()))))
        .cloned();

    if let Some(existing) = existing {
        // An existing Error record must not return Ok as a successful duplicate
        if existing.phase == TorrentOperationPhase::Error {
            let err_msg = existing
                .safe_error
                .unwrap_or_else(|| "Previous operation failed".to_string());
            return Err(err_msg);
        }

        match existing.kind {
            TorrentOperationKind::Remove {
                delete_files: existing_del,
            } if existing_del == del => {
                // In-progress duplicate with matching parameters: reuse active result
                return Ok(ReservationResult::Duplicate(existing));
            }
            _ => {
                return Err(format!(
                    "An active operation is already in progress for torrent '{id}'"
                ));
            }
        }
    }

    let op_id = uuid::Uuid::new_v4().to_string();
    let snapshot = TorrentOperationSnapshot {
        operation_id: op_id,
        torrent_id: active_lookup_key.to_string(),
        kind: TorrentOperationKind::Remove { delete_files: del },
        phase: initial_phase,
        destination: None,
        safe_error: None,
        outcome: None,
        revision: 1,
        created_at: current_time_millis(),
    };

    map.insert(id.to_string(), snapshot.clone());
    map.insert(id.to_lowercase(), snapshot.clone());
    if let Some(hash) = info_hash {
        map.insert(hash.to_string(), snapshot.clone());
        map.insert(hash.to_lowercase(), snapshot.clone());
    }

    Ok(ReservationResult::Reserved(snapshot))
}

pub(crate) fn reserve_active_move_operation(
    id: &str,
    info_hash: Option<&str>,
    destination: &str,
    initial_phase: TorrentOperationPhase,
) -> Result<ReservationResult, String> {
    let mut map = get_active_operations_map()
        .lock()
        .map_err(|e| format!("Registry lock poisoned: {e}"))?;

    let active_lookup_key = info_hash.unwrap_or(id);

    let existing = map
        .get(id)
        .or_else(|| map.get(&id.to_lowercase()))
        .or_else(|| info_hash.and_then(|h| map.get(h).or_else(|| map.get(&h.to_lowercase()))))
        .cloned();

    if let Some(existing) = existing {
        // An existing Error record must not return Ok as a successful duplicate
        if existing.phase == TorrentOperationPhase::Error {
            let err_msg = existing
                .safe_error
                .unwrap_or_else(|| "Previous operation failed".to_string());
            return Err(err_msg);
        }

        match &existing.kind {
            TorrentOperationKind::Move {
                destination: existing_dest,
            } if existing_dest == destination => {
                // In-progress duplicate with matching destination: reuse active result
                return Ok(ReservationResult::Duplicate(existing));
            }
            _ => {
                return Err(format!(
                    "An active operation is already in progress for torrent '{id}'"
                ));
            }
        }
    }

    let op_id = uuid::Uuid::new_v4().to_string();
    let snapshot = TorrentOperationSnapshot {
        operation_id: op_id,
        torrent_id: active_lookup_key.to_string(),
        kind: TorrentOperationKind::Move {
            destination: destination.to_string(),
        },
        phase: initial_phase,
        destination: Some(destination.to_string()),
        safe_error: None,
        outcome: None,
        revision: 1,
        created_at: current_time_millis(),
    };

    map.insert(id.to_string(), snapshot.clone());
    map.insert(id.to_lowercase(), snapshot.clone());
    if let Some(hash) = info_hash {
        map.insert(hash.to_string(), snapshot.clone());
        map.insert(hash.to_lowercase(), snapshot.clone());
    }

    Ok(ReservationResult::Reserved(snapshot))
}

pub(crate) fn complete_active_move_operation(
    id: &str,
    info_hash: Option<&str>,
    destination: &str,
    outcome: Option<String>,
) -> Option<TorrentOperationSnapshot> {
    let mut map = match get_active_operations_map().lock() {
        Ok(m) => m,
        Err(_) => return None,
    };

    let target = map
        .get(id)
        .or_else(|| map.get(&id.to_lowercase()))
        .or_else(|| info_hash.and_then(|h| map.get(h).or_else(|| map.get(&h.to_lowercase()))))
        .cloned();

    if let Some(mut snapshot) = target {
        snapshot.phase = TorrentOperationPhase::Complete;
        snapshot.destination = Some(destination.to_string());
        snapshot.outcome = Some(outcome.unwrap_or_else(|| format!("Moved to {}", destination)));
        snapshot.revision += 1;

        emit_operation_event(None, &snapshot);

        if let Some(app) = APP_HANDLE.get() {
            use tauri::Emitter;
            let _ = app.emit("nebulae://media-updated", ());
        }

        let op_id = snapshot.operation_id.clone();
        map.retain(|_, op| op.operation_id != op_id);
        Some(snapshot)
    } else {
        None
    }
}

pub(crate) fn fail_active_move_operation(
    id: &str,
    info_hash: Option<&str>,
    error_msg: &str,
) -> Option<TorrentOperationSnapshot> {
    let mut map = match get_active_operations_map().lock() {
        Ok(m) => m,
        Err(_) => return None,
    };

    let target = map
        .get(id)
        .or_else(|| map.get(&id.to_lowercase()))
        .or_else(|| info_hash.and_then(|h| map.get(h).or_else(|| map.get(&h.to_lowercase()))))
        .cloned();

    let snapshot = match target {
        Some(mut s) => {
            s.phase = TorrentOperationPhase::Error;
            s.safe_error = Some(error_msg.to_string());
            s.revision += 1;
            s
        }
        None => {
            let active_lookup_key = info_hash.unwrap_or(id);
            TorrentOperationSnapshot {
                operation_id: uuid::Uuid::new_v4().to_string(),
                torrent_id: active_lookup_key.to_string(),
                kind: TorrentOperationKind::Move {
                    destination: String::new(),
                },
                phase: TorrentOperationPhase::Error,
                destination: None,
                safe_error: Some(error_msg.to_string()),
                outcome: None,
                revision: 1,
                created_at: current_time_millis(),
            }
        }
    };

    let target_op_id = snapshot.operation_id.clone();
    for op in map.values_mut() {
        if op.operation_id == target_op_id {
            *op = snapshot.clone();
        }
    }
    map.insert(id.to_string(), snapshot.clone());
    map.insert(id.to_lowercase(), snapshot.clone());
    if let Some(hash) = info_hash {
        map.insert(hash.to_string(), snapshot.clone());
        map.insert(hash.to_lowercase(), snapshot.clone());
    }

    emit_operation_event(None, &snapshot);
    Some(snapshot)
}

pub(crate) fn register_active_operation(
    id: &str,
    info_hash: Option<&str>,
    snapshot: &TorrentOperationSnapshot,
) {
    if let Ok(mut map) = get_active_operations_map().lock() {
        map.insert(id.to_string(), snapshot.clone());
        map.insert(id.to_lowercase(), snapshot.clone());
        if let Some(hash) = info_hash {
            map.insert(hash.to_string(), snapshot.clone());
            map.insert(hash.to_lowercase(), snapshot.clone());
        }
    }
}

pub(crate) fn dismiss_active_operation(key: &str) -> Result<(), String> {
    let mut map = get_active_operations_map()
        .lock()
        .map_err(|e| format!("Registry lock poisoned: {e}"))?;

    let target = map
        .get(key)
        .or_else(|| map.get(&key.to_lowercase()))
        .or_else(|| {
            map.values().find(|op| {
                op.torrent_id == key
                    || op.torrent_id.eq_ignore_ascii_case(key)
                    || op.operation_id == key
            })
        })
        .cloned();

    let snapshot = match target {
        Some(s) => s,
        None => return Ok(()),
    };

    // Allow dismissal only for terminal errors; do not let dismissal clear an in-flight reservation
    if snapshot.phase != TorrentOperationPhase::Error {
        return Err(format!(
            "Cannot dismiss operation '{}': operation is currently in phase '{:?}'. Only terminal errors can be dismissed.",
            snapshot.operation_id, snapshot.phase
        ));
    }

    // Remove every alias for that operation_id
    let target_op_id = snapshot.operation_id;
    map.retain(|_, op| op.operation_id != target_op_id);

    Ok(())
}

pub(crate) fn remove_active_operation(id: &str, info_hash: Option<&str>) {
    if let Ok(mut map) = get_active_operations_map().lock() {
        let op_id = map
            .get(id)
            .or_else(|| map.get(&id.to_lowercase()))
            .or_else(|| info_hash.and_then(|h| map.get(h).or_else(|| map.get(&h.to_lowercase()))))
            .map(|op| op.operation_id.clone());

        if let Some(ref target_id) = op_id {
            map.retain(|_, op| &op.operation_id != target_id);
        } else {
            map.remove(id);
            map.remove(&id.to_lowercase());
            if let Some(hash) = info_hash {
                map.remove(hash);
                map.remove(&hash.to_lowercase());
            }
        }
    }
}

pub(crate) fn list_active_operations() -> Vec<TorrentOperationSnapshot> {
    if let Ok(map) = get_active_operations_map().lock() {
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for op in map.values() {
            if seen.insert(op.operation_id.clone()) {
                list.push(op.clone());
            }
        }
        list
    } else {
        Vec::new()
    }
}

fn emit_operation_event(app: Option<&tauri::AppHandle>, snapshot: &TorrentOperationSnapshot) {
    let app = app.or_else(|| APP_HANDLE.get());
    if let Some(app) = app {
        use tauri::Emitter;
        let _ = app.emit("nebulae://torrent-operation", snapshot);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TorrentFileItem {
    pub index: u32,
    pub name: String,
    pub size_bytes: u64,
    pub progress: f32,
    pub priority: u8, // 0: DoNotDownload, 1: Normal, 2: High, 3: Maximum
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TorrentTrackerItem {
    pub tier: u32,
    pub url: String,
    pub status: String,
    // None = swarm counts unknown (scrape not available/failed) - the UI
    // shows a dash instead of a lying zero.
    pub seeds: Option<u32>,
    pub peers: Option<u32>,
    pub downloaded: u64,
    pub message: String,
    // Unix timestamps (seconds). None = never announced / not yet scheduled.
    #[serde(default)]
    pub last_announce: Option<i64>,
    #[serde(default)]
    pub next_announce: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TorrentItem {
    pub id: String,
    pub name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_rate: u64,
    pub upload_rate: u64,
    /// Cumulative seconds the torrent spent actively downloading (Transmission
    /// `secondsDownloading`). Denominator of the since-transfer-began average
    /// download rate; 0 when the engine does not report active seconds.
    #[serde(default)]
    pub seconds_downloading: u64,
    /// Cumulative seconds the torrent spent actively seeding (Transmission
    /// `secondsSeeding`). Denominator of the since-transfer-began average
    /// upload rate; 0 when the engine does not report active seconds.
    #[serde(default)]
    pub seconds_seeding: u64,
    pub progress: f32,
    pub status: String,
    pub eta_seconds: Option<u64>,
    // None = swarm seed count unknown (no successful tracker scrape yet).
    // The UI shows a dash rather than a misleading zero.
    pub seeds: Option<u32>,
    pub peers: u32,
    pub ratio: f32,
    pub category: String,
    pub is_media: bool,
    pub poster_url: Option<String>,

    // Canonical filesystem name and content path
    #[serde(default)]
    pub raw_name: Option<String>,
    #[serde(default)]
    pub content_path: Option<String>,

    // qBittorrent metadata & control fields
    pub info_hash: String,
    pub save_path: String,
    #[serde(default)]
    pub historical_save_path: Option<String>,
    pub added_on: u64,
    pub completed_on: Option<u64>,
    pub comment: String,
    pub created_by: String,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub sequential_download: bool,
    pub first_last_piece_prio: bool,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub super_seeding: bool,
    #[serde(default)]
    pub last_checked: Option<u64>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub files: Vec<TorrentFileItem>,
    #[serde(default)]
    pub trackers: Vec<TorrentTrackerItem>,
    #[serde(default)]
    pub download_limit_kib: u32,
    #[serde(default)]
    pub upload_limit_kib: u32,
    #[serde(default)]
    pub ratio_limit: f32,
    /// Transmission torrent bandwidth priority: 1 = high, 0 = normal, -1 = low
    #[serde(default, rename = "bandwidthPriority")]
    pub bandwidth_priority: Option<i8>,
    /// Per-torrent peer limit reported by the engine
    #[serde(default, rename = "peerLimit")]
    pub peer_limit: Option<u32>,
    /// Explicit Racing Mode state: true only when both bandwidthPriority == 1 AND peerLimit >= 150
    #[serde(default, rename = "racingMode")]
    pub racing_mode: Option<bool>,
    /// Engine error string when present (e.g. disk full, permission denied, tracker error)
    #[serde(default, rename = "errorString")]
    pub error_string: Option<String>,
    #[serde(default)]
    pub media_meta: Option<crate::utils::media_parser::ParsedMediaMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateTorrentParams {
    pub source_path: String,
    pub output_path: String,
    pub piece_size_kib: u32,
    pub trackers: Vec<String>,
    pub web_seeds: Vec<String>,
    pub is_private: bool,
    pub comment: String,
    /// Accepted for older frontends but ignored by the creator.
    /// Generated metainfo always uses the authoritative application version.
    #[serde(default)]
    pub created_by: String,
    pub start_seeding: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InspectTorrentResult {
    pub name: String,
    pub total_bytes: u64,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub info_hash: String,
    pub is_private: bool,
    pub comment: String,
    pub created_by: String,
    pub files: Vec<TorrentFileItem>,
    pub trackers: Vec<TorrentTrackerItem>,
    #[serde(default)]
    pub metainfo_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub hygiene: Option<crate::engine::metainfo::TorrentPathHygieneReport>,
}

#[tauri::command]
pub fn inspect_torrent_file(
    source: Option<String>,
    bytes: Option<Vec<u8>>,
) -> Result<InspectTorrentResult, String> {
    // 1. If raw bytes are passed
    if let Some(raw) = bytes {
        if !raw.is_empty() {
            let parsed = crate::engine::metainfo::ParsedTorrent::parse_bytes(&raw)?;
            let file_paths: Vec<String> = parsed.files.iter().map(|f| f.name.clone()).collect();
            let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(&file_paths));
            return Ok(InspectTorrentResult {
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
                metainfo_bytes: Some(raw),
                hygiene,
            });
        }
    }

    // 2. If source string is passed
    if let Some(src) = source {
        let trimmed = src.trim();
        let lower = trimmed.to_ascii_lowercase();

        // Base64 encoded torrent?
        let b64_candidate = if lower.starts_with("data:") {
            lower.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if lower.starts_with("base64:") {
            Some(&trimmed["base64:".len()..])
        } else if !lower.starts_with("magnet:")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = crate::engine::mock::decode_base64_data(b64) {
                if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_bytes(&decoded) {
                    let file_paths: Vec<String> =
                        parsed.files.iter().map(|f| f.name.clone()).collect();
                    let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(&file_paths));
                    return Ok(InspectTorrentResult {
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
                        metainfo_bytes: Some(decoded),
                        hygiene,
                    });
                }
            }
        }

        // Magnet link?
        if lower.starts_with("magnet:") {
            let parsed_name = trimmed
                .split("&dn=")
                .nth(1)
                .and_then(|dn| dn.split('&').next())
                .unwrap_or("Magnet Download");
            let name = parsed_name.replace('+', " ").replace("%20", " ");
            let xt_hash = trimmed
                .split("xt=urn:btih:")
                .nth(1)
                .and_then(|h| h.split('&').next())
                .unwrap_or("unknown_hash");
            return Ok(InspectTorrentResult {
                name,
                total_bytes: 0,
                piece_size: 1_048_576,
                total_pieces: 0,
                info_hash: xt_hash.to_lowercase(),
                is_private: false,
                comment: "Magnet Link".to_string(),
                created_by: "Nebulae Metainfo Tool".to_string(),
                files: vec![],
                trackers: vec![],
                metainfo_bytes: None,
                hygiene: None,
            });
        }

        // ADC protocol link: requires authenticated passkey resolution
        if lower.starts_with("adc:") {
            return Err(
                "ADC protocol links require passkey authentication. Use resolve_adc_torrent to resolve metainfo."
                    .to_string(),
            );
        }

        // Disk path?
        let norm_path = crate::engine::mock::normalize_source_path(trimmed);
        let disk_path = if norm_path.is_file() {
            Some(norm_path.clone())
        } else if let Some(dl_dir) = dirs::download_dir() {
            let candidate = dl_dir.join(trimmed);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(target_path) = disk_path {
            if let Ok(raw_bytes) = std::fs::read(&target_path) {
                if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_bytes(&raw_bytes)
                {
                    let file_paths: Vec<String> =
                        parsed.files.iter().map(|f| f.name.clone()).collect();
                    let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(&file_paths));
                    return Ok(InspectTorrentResult {
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
                        metainfo_bytes: Some(raw_bytes),
                        hygiene,
                    });
                }
            }
            if let Ok(meta) = target_path.metadata() {
                let total_bytes = meta.len();
                let file_name = target_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Local File".to_string());
                let piece_size = recommended_piece_size_bytes(total_bytes);
                let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(
                    std::slice::from_ref(&file_name),
                ));
                return Ok(InspectTorrentResult {
                    name: file_name.clone(),
                    total_bytes,
                    piece_size,
                    total_pieces: total_bytes.div_ceil(piece_size as u64) as u32,
                    info_hash: format!("local_{:x}", total_bytes),
                    is_private: true,
                    comment: "Local Storage File".to_string(),
                    created_by: "Nebulae Metainfo Tool".to_string(),
                    files: vec![crate::commands::torrent::TorrentFileItem {
                        index: 0,
                        name: file_name,
                        size_bytes: total_bytes,
                        progress: 1.0,
                        priority: 1,
                    }],
                    trackers: vec![],
                    metainfo_bytes: None,
                    hygiene,
                });
            }
        } else if norm_path.is_dir() {
            let files = crate::engine::mock::scan_directory_files(&norm_path)?;
            let total_bytes: u64 = files.iter().map(|f| f.size_bytes).sum();
            let dir_name = norm_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Local Directory".to_string());
            let piece_size = recommended_piece_size_bytes(total_bytes);
            let file_paths: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
            let hygiene = Some(crate::engine::metainfo::inspect_paths_hygiene(&file_paths));
            return Ok(InspectTorrentResult {
                name: dir_name,
                total_bytes,
                piece_size,
                total_pieces: total_bytes.div_ceil(piece_size as u64) as u32,
                info_hash: format!("local_{:x}", total_bytes),
                is_private: true,
                comment: "Local Storage Directory".to_string(),
                created_by: "Nebulae Metainfo Tool".to_string(),
                files,
                trackers: vec![],
                metainfo_bytes: None,
                hygiene,
            });
        }
    }

    Err("Could not inspect torrent: invalid source or unsupported format".to_string())
}

/// Computes the recommended piece size in bytes based on total content size:
/// - >= 50 GiB: 16 MiB
/// - >= 15 GiB: 8 MiB
/// - < 15 GiB: 1 MiB default
pub fn recommended_piece_size_bytes(total_bytes: u64) -> u32 {
    const GIB: u64 = 1024 * 1024 * 1024;
    if total_bytes >= 50 * GIB {
        16 * 1024 * 1024
    } else if total_bytes >= 15 * GIB {
        8 * 1024 * 1024
    } else {
        1_048_576
    }
}

/// Switches which engine the Manager UI commands drive: the local engine (Manager tab) or the
/// configured Nebulae Server (Server tab). Background work is unaffected.
#[tauri::command]
pub async fn set_engine_scope(
    remote: bool,
    scoped: State<'_, std::sync::Arc<crate::engine::ScopedEngine>>,
    db: State<'_, std::sync::Arc<crate::db::Database>>,
) -> Result<(), String> {
    if remote {
        let settings = crate::commands::settings::load_user_settings_from_db(db.inner());
        if !settings.rtorrent_server_enabled {
            return Err(
                "The Nebulae Server connection is turned off in Settings -> Server.".to_string(),
            );
        }
        scoped.configure_remote(
            &settings.rtorrent_server_url,
            &settings.rtorrent_server_token,
        )?;
    }
    scoped.set_remote_active(remote)
}

#[tauri::command]
pub async fn list_torrents(engine: State<'_, SharedEngine>) -> Result<Vec<TorrentItem>, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.list_torrents())
        .await
        .map_err(|e| format!("list_torrents task failed: {e}"))?
}

#[tauri::command]
pub async fn get_torrent_details(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<TorrentItem, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.get_torrent_details(&id))
        .await
        .map_err(|e| format!("get_torrent_details task failed: {e}"))?
}

#[tauri::command]
pub async fn add_torrent(
    source: String,
    download_dir: Option<String>,
    bytes: Option<Vec<u8>>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    let download_dir = resolve_engine_download_dir(download_dir)?;
    tokio::task::spawn_blocking(move || {
        if let Some(raw) = bytes {
            if !raw.is_empty() {
                return engine.add_torrent_from_bytes(&raw, download_dir.as_deref());
            }
        }
        engine.add_torrent(&source, download_dir.as_deref())
    })
    .await
    .map_err(|e| format!("add_torrent task failed: {e}"))?
}

#[tauri::command]
pub async fn add_torrent_bytes(
    bytes: Vec<u8>,
    download_dir: Option<String>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    let download_dir = resolve_engine_download_dir(download_dir)?;
    tokio::task::spawn_blocking(move || {
        engine.add_torrent_from_bytes(&bytes, download_dir.as_deref())
    })
    .await
    .map_err(|e| format!("add_torrent_bytes task failed: {e}"))?
}

// ---------------------------------------------------------------------------
// F-2 staged add (task order section 3): add the torrent PAUSED, apply the
// user's name and file-selection choices from the Add Torrent dialog, then
// start. Every engine operation used here already exists (torrent-rename-path
// via rename_torrent/_file/_folder, files-wanted/unwanted via
// set_files_priority, torrent-start via resume_torrent); this command only
// sequences them so the content layout is correct before the first byte
// moves. No new RPC method and no daemon-side change.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StagedTorrentRename {
    /// Full path inside the torrent whose NAME changes, relative to the save
    /// destination and INCLUDING the torrent's root folder for multi-file
    /// torrents (the same format Transmission's files[].name uses). The root
    /// itself is renamed with root_name instead.
    pub path: String,
    /// The new NAME (not path) for that entry.
    pub name: String,
    /// True for folders (rename_torrent_folder), false for files
    /// (rename_torrent_file by index).
    pub is_folder: bool,
    /// File index for file renames (matches the metainfo/RPC file order).
    #[serde(default)]
    pub file_index: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StagedTorrentAdd {
    pub source: String,
    #[serde(default)]
    pub download_dir: Option<String>,
    #[serde(default)]
    pub bytes: Option<Vec<u8>>,
    /// Rename the torrent's root (its top-level name / folder).
    #[serde(default)]
    pub root_name: Option<String>,
    /// File and folder renames, ordered deepest-first (children before their
    /// parent folders) so each rename still finds the original path.
    #[serde(default)]
    pub renames: Vec<StagedTorrentRename>,
    /// File indices the user excluded (everything else stays wanted).
    #[serde(default)]
    pub excluded_files: Vec<u32>,
    /// Start the torrent after every staged edit applied successfully.
    #[serde(default)]
    pub start_after_apply: bool,
}

#[tauri::command]
pub async fn add_torrent_staged(
    staged: StagedTorrentAdd,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    let download_dir = resolve_engine_download_dir(staged.download_dir)?;
    tokio::task::spawn_blocking(move || {
        // 1. Add PAUSED so nothing downloads before the content is correct.
        let id = match staged.bytes {
            Some(raw) if !raw.is_empty() => {
                engine.add_torrent_from_bytes_paused(&raw, download_dir.as_deref())?
            }
            _ => engine.add_torrent_paused(&staged.source, download_dir.as_deref())?,
        };

        // 2. Apply the staged edits. File and folder renames must arrive
        // deepest-first (the frontend orders them): each rename re-resolves
        // the CURRENT path, so a child must be renamed before its parent
        // folder changes. The root rename goes LAST because the folder paths
        // sent by the frontend still carry the old root prefix.
        let mut apply_errors: Vec<String> = Vec::new();
        for r in &staged.renames {
            let result = if r.is_folder {
                engine.rename_torrent_folder(&id, &r.path, &r.name)
            } else {
                engine.rename_torrent_file(&id, r.file_index, &r.name)
            };
            if let Err(e) = result {
                apply_errors.push(format!("rename '{}' to '{}': {}", r.path, r.name, e));
            }
        }
        if !staged.excluded_files.is_empty() {
            if let Err(e) = engine.set_files_priority(&id, &staged.excluded_files, 0) {
                apply_errors.push(format!(
                    "exclude {} file(s): {e}",
                    staged.excluded_files.len()
                ));
            }
        }
        if let Some(root) = staged
            .root_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Err(e) = engine.rename_torrent(&id, root) {
                apply_errors.push(format!("rename root to '{root}': {e}"));
            }
        }

        // 3. Start only if every staged edit applied. On any failure the
        // torrent stays PAUSED (nothing downloads with the wrong layout);
        // the user can fix it in the torrent's Files tab or remove it.
        if apply_errors.is_empty() {
            if staged.start_after_apply {
                if let Err(e) = engine.resume_torrent(&id) {
                    return Err(format!(
                        "torrent '{id}' was added and prepared but could not be started: {e}"
                    ));
                }
            }
            Ok(id)
        } else {
            Err(format!(
                "The torrent was added PAUSED, but {} staged change(s) failed: {}. \
                 Nothing downloads until it is started; fix the names or selection in the \
                 torrent's Files tab, or remove the torrent.",
                apply_errors.len(),
                apply_errors.join("; ")
            ))
        }
    })
    .await
    .map_err(|e| format!("add_torrent_staged task failed: {e}"))?
}

// ---------------------------------------------------------------------------
// Rename-on-add: match existing content to a torrent's expected layout.
//
// File and folder names are part of a torrent's info-hash, so they cannot be
// changed in the metainfo without leaving the original swarm. The supported
// workflow renames the user's existing files on disk to the torrent's
// expected layout before adding, so the engine's initial recheck finds the
// data and the torrent keeps seeding with its original info-hash.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentContentFileMatch {
    pub index: u32,
    /// Path the torrent expects, relative to the save destination (includes
    /// the torrent's root folder for multi-file torrents).
    pub expected_rel_path: String,
    pub size_bytes: u64,
    /// Where the matching file lives today, relative to the save destination.
    /// None when no candidate was found; the user can fill one in manually.
    pub matched_rel_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentContentMatch {
    pub torrent_name: String,
    pub is_multi_file: bool,
    pub files: Vec<TorrentContentFileMatch>,
    pub matched_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentRenameEntry {
    /// Existing file, relative to the save destination.
    pub from_rel: String,
    /// Target path the torrent expects, relative to the save destination.
    pub to_rel: String,
}

/// Greedy size-based matcher: exact relative path first, then same file name,
/// then a single remaining same-size candidate. Ambiguous candidates are left
/// for the user to resolve in the UI.
fn match_content_files(
    expected: &[(u32, String, u64)],
    disk: &[(String, u64)],
) -> Vec<TorrentContentFileMatch> {
    let mut available: Vec<Option<(String, u64)>> =
        disk.iter().map(|(p, s)| Some((p.clone(), *s))).collect();
    let mut out = Vec::with_capacity(expected.len());

    for (index, rel_path, size) in expected {
        // Zero-byte files cannot be identified by size; leave them to the user.
        let candidates: Vec<usize> = if *size == 0 {
            Vec::new()
        } else {
            available
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.as_ref().map(|(_, s)| s == size).unwrap_or(false))
                .map(|(i, _)| i)
                .collect()
        };

        let expected_lower = rel_path.to_lowercase();
        let expected_name_lower = rel_path
            .rsplit('/')
            .next()
            .unwrap_or(rel_path)
            .to_lowercase();

        let chosen = candidates
            .iter()
            .copied()
            .find(|&i| {
                available[i]
                    .as_ref()
                    .map(|(p, _)| p.to_lowercase() == expected_lower)
                    .unwrap_or(false)
            })
            .or_else(|| {
                candidates.iter().copied().find(|&i| {
                    available[i]
                        .as_ref()
                        .map(|(p, _)| {
                            p.rsplit('/').next().unwrap_or(p).to_lowercase() == expected_name_lower
                        })
                        .unwrap_or(false)
                })
            })
            .or_else(|| {
                if candidates.len() == 1 {
                    Some(candidates[0])
                } else {
                    None
                }
            });

        let matched = chosen.and_then(|i| available[i].take());
        out.push(TorrentContentFileMatch {
            index: *index,
            expected_rel_path: rel_path.clone(),
            size_bytes: *size,
            matched_rel_path: matched.map(|(p, _)| p),
        });
    }

    out
}

fn collect_disk_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    out: &mut Vec<(String, u64)>,
) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_disk_files(root, &path, depth + 1, out);
        } else if file_type.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((rel.to_string_lossy().replace('\\', "/"), size));
            }
        }
    }
}

fn expand_home_dir(raw: &str) -> Result<std::path::PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("No save destination set".to_string());
    }
    crate::utils::paths::resolve_local_dir(raw).map_err(|e| format!("Save destination {e}"))
}

/// Resolves a caller-supplied engine download directory so `~` notation is
/// expanded before the engine (which has no home-directory semantics) sees it.
fn resolve_engine_download_dir(dir: Option<String>) -> Result<Option<String>, String> {
    match dir.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) => crate::utils::paths::resolve_local_dir(raw)
            .map(|p| Some(p.to_string_lossy().to_string()))
            .map_err(|e| format!("Download destination {e}")),
    }
}

/// Resolves a relative path against the save destination, rejecting absolute
/// paths and traversal components so renames can never escape the folder.
fn resolve_under(download_dir: &std::path::Path, rel: &str) -> Result<std::path::PathBuf, String> {
    let normalized = rel.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("Empty path".to_string());
    }
    let rel_path = std::path::PathBuf::from(&normalized);
    if rel_path.is_absolute() {
        return Err(format!(
            "Path '{}' must be relative to the save destination",
            normalized
        ));
    }
    for component in rel_path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(format!("Invalid path component in '{}'", normalized));
        }
    }
    Ok(download_dir.join(rel_path))
}

#[tauri::command]
pub fn match_torrent_content(
    source: String,
    download_dir: String,
) -> Result<TorrentContentMatch, String> {
    let source_path = crate::engine::mock::normalize_source_path(&source);
    let parsed = crate::engine::metainfo::ParsedTorrent::parse_file(&source_path)?;

    let expanded = expand_home_dir(&download_dir)?;
    if !expanded.is_dir() {
        return Err(format!(
            "Save destination '{}' does not exist",
            expanded.display()
        ));
    }

    let expected: Vec<(u32, String, u64)> = parsed
        .files
        .iter()
        .map(|f| (f.index, f.name.clone(), f.size_bytes))
        .collect();

    let mut disk = Vec::new();
    collect_disk_files(&expanded, &expanded, 0, &mut disk);

    let files = match_content_files(&expected, &disk);
    let matched_count = files
        .iter()
        .filter(|f| f.matched_rel_path.is_some())
        .count();

    Ok(TorrentContentMatch {
        torrent_name: parsed.name,
        is_multi_file: parsed.is_multi_file,
        files,
        matched_count,
    })
}

#[tauri::command]
pub fn apply_torrent_renames(
    download_dir: String,
    renames: Vec<TorrentRenameEntry>,
) -> Result<usize, String> {
    let expanded = expand_home_dir(&download_dir)?;
    if !expanded.is_dir() {
        return Err(format!(
            "Save destination '{}' does not exist",
            expanded.display()
        ));
    }
    apply_torrent_renames_impl(&expanded, &renames)
}

fn apply_torrent_renames_impl(
    download_dir: &std::path::Path,
    renames: &[TorrentRenameEntry],
) -> Result<usize, String> {
    let mut applied = 0usize;
    for entry in renames {
        let from = resolve_under(download_dir, &entry.from_rel)?;
        let to = resolve_under(download_dir, &entry.to_rel)?;
        if from == to {
            continue;
        }
        if !from.exists() {
            return Err(format!(
                "Cannot rename: '{}' does not exist",
                entry.from_rel
            ));
        }
        if to.exists() {
            return Err(format!("Cannot rename: '{}' already exists", entry.to_rel));
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create '{}': {e}", parent.display()))?;
        }
        std::fs::rename(&from, &to).map_err(|e| {
            format!(
                "Failed to rename '{}' to '{}': {e}",
                entry.from_rel, entry.to_rel
            )
        })?;
        applied += 1;
    }
    Ok(applied)
}

/// Validate whether destructive file deletion can safely proceed.
/// Blocks deletion if files are shared with another active torrent, belong to external/reclassified
/// media, or if the target path is ambiguous/unverified.
pub(crate) fn check_destructive_removal_safety(
    target: Option<&TorrentItem>,
    all_torrents: &[TorrentItem],
    is_target_external: bool,
) -> Result<(), String> {
    if is_target_external {
        return Err(
            "Cannot delete files: Target media is an external or reclassified item. Deletion of external media files is blocked."
                .to_string(),
        );
    }

    let t = match target {
        Some(t) => t,
        None => {
            return Err(
                "Cannot delete files: Target torrent not found or indeterminate ownership. Select 'Remove without deleting files'."
                    .to_string(),
            );
        }
    };

    let save_path = t.save_path.trim();
    let name = t.raw_name.as_deref().unwrap_or(&t.name).trim();
    if save_path.is_empty() || name.is_empty() {
        return Err(
            "Cannot delete files: Storage path is ambiguous or unverified. Select 'Remove without deleting files'."
                .to_string(),
        );
    }

    let target_path = std::path::Path::new(save_path).join(name);
    // Disallow deleting root directories or top-level drives
    if target_path.parent().is_none()
        || target_path == std::path::Path::new("/")
        || target_path == std::path::Path::new("\\")
    {
        return Err(
            "Cannot delete files: Storage path resolves to a root directory. Select 'Remove without deleting files'."
                .to_string(),
        );
    }

    // Check against all other active torrents for path or content sharing
    let is_shared = all_torrents.iter().any(|other| {
        if other.id == t.id {
            return false;
        }
        if other.info_hash.eq_ignore_ascii_case(&t.info_hash) {
            return true;
        }
        if let (Some(ref cp_other), Some(ref cp_t)) = (&other.content_path, &t.content_path) {
            if !cp_other.is_empty() && cp_other == cp_t {
                return true;
            }
        }
        let other_save = other.save_path.trim();
        let other_name = other.raw_name.as_deref().unwrap_or(&other.name).trim();
        if !other_save.is_empty() && !other_name.is_empty() {
            let other_path = std::path::Path::new(other_save).join(other_name);
            if other_path == target_path
                || target_path.starts_with(&other_path)
                || other_path.starts_with(&target_path)
            {
                return true;
            }
        }
        false
    });

    if is_shared {
        return Err(
            "Cannot delete files: One or more files are shared with another active torrent. Remove the other torrent first, or select 'Remove without deleting files'."
                .to_string(),
        );
    }

    Ok(())
}

/// Determine whether retained completed files should be reclassified to External media.
/// Only reclassifies when the torrent is 100% complete and no other active managed torrent
/// references the same files.
pub(crate) fn should_reclassify_as_external(
    target: Option<&TorrentItem>,
    all_torrents: &[TorrentItem],
    db_completed: bool,
) -> bool {
    let t = match target {
        Some(t) => t,
        None => return db_completed,
    };

    let is_completed = t.progress >= 1.0
        || t.completed_on.is_some()
        || t.status.eq_ignore_ascii_case("seeding")
        || (t.status.eq_ignore_ascii_case("paused") && t.progress >= 1.0);

    if !is_completed {
        return false;
    }

    // If another active torrent references the same files, keep managed status
    let has_other_active_reference = all_torrents.iter().any(|other| {
        if other.id == t.id {
            return false;
        }
        if other.info_hash.eq_ignore_ascii_case(&t.info_hash) {
            return true;
        }
        if let (Some(ref cp_other), Some(ref cp_t)) = (&other.content_path, &t.content_path) {
            if !cp_other.is_empty() && cp_other == cp_t {
                return true;
            }
        }
        let other_save = other.save_path.trim();
        let other_name = other.raw_name.as_deref().unwrap_or(&other.name).trim();
        let t_save = t.save_path.trim();
        let t_name = t.raw_name.as_deref().unwrap_or(&t.name).trim();
        if !other_save.is_empty()
            && !other_name.is_empty()
            && !t_save.is_empty()
            && !t_name.is_empty()
        {
            let other_path = std::path::Path::new(other_save).join(other_name);
            let t_path = std::path::Path::new(t_save).join(t_name);
            if other_path == t_path {
                return true;
            }
        }
        false
    });

    !has_other_active_reference
}

#[tauri::command]
pub async fn remove_torrent(
    id: String,
    delete_files: Option<bool>,
    app: tauri::AppHandle,
    engine: State<'_, SharedEngine>,
    db: State<'_, std::sync::Arc<crate::db::Database>>,
) -> Result<(), String> {
    register_app_handle(&app);
    let engine = engine.inner().clone();
    let db = db.inner().clone();
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let del = delete_files.unwrap_or(false);
        let torrents = engine.list_torrents().unwrap_or_default();
        let target = torrents
            .iter()
            .find(|t| t.id == id || t.info_hash.eq_ignore_ascii_case(&id))
            .cloned();

        let (info_hash, display_name): (Option<String>, Option<String>) =
            if let Some(ref t) = target {
                (Some(t.info_hash.to_lowercase()), Some(t.name.clone()))
            } else if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
                (Some(id.to_lowercase()), None)
            } else {
                (None, None)
            };

        let is_target_external = if let Some(ref hash) = info_hash {
            db.get_torrent_metadata(hash)
                .ok()
                .flatten()
                .map(|m| m.is_external)
                .unwrap_or(false)
        } else {
            false
        };

        run_remove_lifecycle_step(
            &id,
            del,
            target.as_ref(),
            &torrents,
            is_target_external,
            |tid, d| engine.remove_torrent(tid, d),
            |_tid, _d| {
                if let Some(ref hash) = info_hash {
                    if del {
                        db.delete_torrent_metadata(hash)
                            .map_err(|e| format!("Failed to delete torrent metadata: {e}"))?;
                    } else {
                        let db_completed = db
                            .get_torrent_metadata(hash)
                            .map_err(|e| format!("Failed to read metadata: {e}"))?
                            .map(|m| m.completed_on.is_some())
                            .unwrap_or(false);

                        if should_reclassify_as_external(target.as_ref(), &torrents, db_completed) {
                            if let Ok(Some(mut meta)) = db.get_torrent_metadata(hash) {
                                if meta.display_title.is_none() {
                                    if let Some(name) = &display_name {
                                        meta.display_title = Some(name.clone());
                                    }
                                }
                                if meta.save_path.is_none() {
                                    if let Some(ref t) = target {
                                        meta.save_path = Some(t.save_path.clone());
                                    }
                                }
                                db.upsert_torrent_metadata(&meta).map_err(|e| {
                                    format!("Failed to update torrent metadata: {e}")
                                })?;
                            } else if let Some(ref t) = target {
                                let meta = crate::db::TorrentDbMetadata {
                                    info_hash: hash.clone(),
                                    display_title: Some(t.name.clone()),
                                    category: Some(t.category.clone()),
                                    labels: None,
                                    is_watched: false,
                                    notes: None,
                                    added_at: t.added_on,
                                    completed_on: t.completed_on,
                                    save_path: Some(t.save_path.clone()),
                                    download_limit_kib: 0,
                                    upload_limit_kib: 0,
                                    ratio_limit: -1.0,
                                    sequential_download: false,
                                    first_last_piece_prio: false,
                                    super_seeding: false,
                                    racing_override: None,
                                    user_display_title: None,
                                    user_title_locked: false,
                                    sort_order: None,
                                    poster_path: t.poster_url.clone(),
                                    is_external: false,
                                    sync_miss_count: 0,
                                    last_checked: None,
                                    hidden_at: None,
                                };
                                db.upsert_torrent_metadata(&meta).map_err(|e| {
                                    format!("Failed to insert torrent metadata: {e}")
                                })?;
                            }
                            db.mark_as_external(hash)
                                .map_err(|e| format!("Failed to mark as external: {e}"))?;
                            db.mark_snatched_record_external(hash).map_err(|e| {
                                format!("Failed to mark snatched record external: {e}")
                            })?;
                        }
                    }
                }
                use tauri::Emitter;
                let _ = app_handle.emit("nebulae://media-updated", ());
                Ok(())
            },
            |snap| {
                emit_operation_event(Some(&app_handle), snap);
            },
        )
    })
    .await
    .map_err(|e| format!("remove_torrent task failed: {e}"))?
}

#[tauri::command]
pub fn get_active_torrent_operations() -> Result<Vec<TorrentOperationSnapshot>, String> {
    Ok(list_active_operations())
}

#[tauri::command]
pub fn dismiss_torrent_operation(id: String) -> Result<(), String> {
    dismiss_active_operation(&id)
}

pub(crate) fn is_ambiguous_transport_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection error")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("response was not received")
        || lower.contains("broken pipe")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_remove_lifecycle_step<E, R>(
    id: &str,
    del: bool,
    target: Option<&TorrentItem>,
    torrents: &[TorrentItem],
    is_target_external: bool,
    engine_remove: E,
    reconcile: R,
    mut on_transition: impl FnMut(&TorrentOperationSnapshot),
) -> Result<(), String>
where
    E: FnOnce(&str, bool) -> Result<(), String>,
    R: FnOnce(&str, bool) -> Result<(), String>,
{
    let info_hash = target.map(|t| t.info_hash.to_lowercase()).or_else(|| {
        if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(id.to_lowercase())
        } else {
            None
        }
    });

    let initial_phase = if del {
        TorrentOperationPhase::RemovingTorrentAndDeletingFiles
    } else {
        TorrentOperationPhase::RemovingTorrent
    };

    // 1. Atomic reservation under registry lock (lock dropped before engine work)
    let reservation =
        reserve_active_remove_operation(id, info_hash.as_deref(), del, initial_phase)?;
    let mut snapshot = match reservation {
        ReservationResult::Duplicate(_) => {
            // Reusing active result, avoid duplicate destructive engine removal
            return Ok(());
        }
        ReservationResult::Reserved(s) => s,
    };

    on_transition(&snapshot);

    // 2. Safety check before destructive removal
    if del {
        if let Err(e) = check_destructive_removal_safety(target, torrents, is_target_external) {
            snapshot.phase = TorrentOperationPhase::Error;
            snapshot.safe_error = Some(e.clone());
            snapshot.revision += 1;
            register_active_operation(id, info_hash.as_deref(), &snapshot);
            on_transition(&snapshot);
            return Err(e);
        }
    }

    // 3. Indivisible engine removal
    if let Err(e) = engine_remove(id, del) {
        let err_msg = if is_ambiguous_transport_error(&e) {
            "The engine response was not received. Removal or file deletion may still be in progress. Check the torrent’s status before trying again.".to_string()
        } else {
            format!("Engine removal failed: {e}")
        };
        snapshot.phase = TorrentOperationPhase::Error;
        snapshot.safe_error = Some(err_msg.clone());
        snapshot.revision += 1;
        register_active_operation(id, info_hash.as_deref(), &snapshot);
        on_transition(&snapshot);
        return Err(err_msg);
    }

    // 4. Reconciling phase
    snapshot.phase = TorrentOperationPhase::Reconciling;
    snapshot.revision += 1;
    register_active_operation(id, info_hash.as_deref(), &snapshot);
    on_transition(&snapshot);

    if let Err(_e) = reconcile(id, del) {
        // If engine removal succeeded but reconciliation failed, record specific failure
        let err_msg = "Torrent removed; Media update failed.".to_string();
        snapshot.phase = TorrentOperationPhase::Error;
        snapshot.safe_error = Some(err_msg.clone());
        snapshot.revision += 1;
        register_active_operation(id, info_hash.as_deref(), &snapshot);
        on_transition(&snapshot);
        return Err(err_msg);
    }

    // 5. Complete phase
    snapshot.phase = TorrentOperationPhase::Complete;
    snapshot.revision += 1;
    if del {
        snapshot.outcome = Some(
            "Torrent removed. File-deletion request processed; deletion of every file could not be verified."
                .to_string(),
        );
    } else {
        snapshot.outcome = Some("Torrent removed. Files kept.".to_string());
    }
    on_transition(&snapshot);
    remove_active_operation(id, info_hash.as_deref());

    Ok(())
}

pub(crate) fn run_move_lifecycle_step<E>(
    id: &str,
    destination: &str,
    info_hash: Option<&str>,
    semantics: crate::engine::traits::MoveCompletionSemantics,
    engine_move: E,
    mut on_transition: impl FnMut(&TorrentOperationSnapshot),
) -> Result<(), String>
where
    E: FnOnce(&str, &str) -> Result<(), String>,
{
    // 1. Atomic reservation under registry lock
    let reservation =
        reserve_active_move_operation(id, info_hash, destination, TorrentOperationPhase::Moving)?;
    let mut snapshot = match reservation {
        ReservationResult::Duplicate(_) => return Ok(()),
        ReservationResult::Reserved(s) => s,
    };
    on_transition(&snapshot);

    // 2. Engine move execution
    if let Err(e) = engine_move(id, destination) {
        let err_msg = if e.starts_with("Move failed:") {
            e
        } else {
            format!("Move failed: {e}")
        };
        snapshot.phase = TorrentOperationPhase::Error;
        snapshot.safe_error = Some(err_msg.clone());
        snapshot.revision += 1;
        register_active_operation(id, info_hash, &snapshot);
        on_transition(&snapshot);
        return Err(err_msg);
    }

    // 3. Completion handling bound to demonstrated engine semantics
    match semantics {
        crate::engine::traits::MoveCompletionSemantics::AsyncCallback => {
            // Engine moves asynchronously; completion or failure will be signaled by callback.
            Ok(())
        }
        crate::engine::traits::MoveCompletionSemantics::Synchronous => {
            snapshot.phase = TorrentOperationPhase::Complete;
            snapshot.destination = Some(destination.to_string());
            snapshot.outcome = Some(format!("Moved to {}", destination));
            snapshot.revision += 1;
            on_transition(&snapshot);
            if let Some(app) = APP_HANDLE.get() {
                use tauri::Emitter;
                let _ = app.emit("nebulae://media-updated", ());
            }
            remove_active_operation(id, info_hash);
            Ok(())
        }
        crate::engine::traits::MoveCompletionSemantics::Unconfirmed => {
            snapshot.phase = TorrentOperationPhase::Complete;
            snapshot.destination = Some(destination.to_string());
            snapshot.outcome =
                Some("Location set; file relocation not confirmed by engine".to_string());
            snapshot.revision += 1;
            on_transition(&snapshot);
            if let Some(app) = APP_HANDLE.get() {
                use tauri::Emitter;
                let _ = app.emit("nebulae://media-updated", ());
            }
            remove_active_operation(id, info_hash);
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn pause_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.pause_torrent(&id))
        .await
        .map_err(|e| format!("pause_torrent task failed: {e}"))?
}

#[tauri::command]
pub async fn resume_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.resume_torrent(&id))
        .await
        .map_err(|e| format!("resume_torrent task failed: {e}"))?
}

#[tauri::command]
pub async fn pause_all_torrents(engine: State<'_, SharedEngine>) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.pause_all())
        .await
        .map_err(|e| format!("pause_all task failed: {e}"))?
}

#[tauri::command]
pub async fn resume_all_torrents(engine: State<'_, SharedEngine>) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.resume_all())
        .await
        .map_err(|e| format!("resume_all task failed: {e}"))?
}

#[tauri::command]
pub async fn force_recheck_torrent(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.force_recheck(&id))
        .await
        .map_err(|e| format!("force_recheck task failed: {e}"))?
}

#[tauri::command]
pub async fn reannounce_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.reannounce(&id))
        .await
        .map_err(|e| format!("reannounce task failed: {e}"))?
}

#[tauri::command]
pub async fn set_torrent_location(
    id: String,
    new_path: String,
    app: tauri::AppHandle,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    register_app_handle(&app);
    let engine = engine.inner().clone();
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let torrents = engine.list_torrents().unwrap_or_default();
        let target = torrents
            .iter()
            .find(|t| t.id == id || t.info_hash.eq_ignore_ascii_case(&id))
            .cloned();

        let info_hash = target
            .as_ref()
            .map(|t| t.info_hash.to_lowercase())
            .or_else(|| {
                if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
                    Some(id.to_lowercase())
                } else {
                    None
                }
            });

        let semantics = engine.move_completion_semantics();

        run_move_lifecycle_step(
            &id,
            &new_path,
            info_hash.as_deref(),
            semantics,
            |tid, path| engine.set_location(tid, path),
            |snap| {
                emit_operation_event(Some(&app_handle), snap);
            },
        )
    })
    .await
    .map_err(|e| format!("set_torrent_location task failed: {e}"))?
}

#[tauri::command]
pub async fn set_torrent_category(
    id: String,
    category: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.set_category(&id, &category))
        .await
        .map_err(|e| format!("set_torrent_category task failed: {e}"))?
}

#[tauri::command]
pub async fn set_torrent_speed_limits(
    id: String,
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.set_torrent_limits(&id, dl_kib, ul_kib, ratio_limit))
        .await
        .map_err(|e| format!("set_torrent_speed_limits task failed: {e}"))?
}

#[tauri::command]
pub async fn toggle_torrent_sequential(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.toggle_sequential_download(&id))
        .await
        .map_err(|e| format!("toggle_torrent_sequential task failed: {e}"))?
}

#[tauri::command]
pub async fn toggle_torrent_first_last(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.toggle_first_last_piece_priority(&id))
        .await
        .map_err(|e| format!("toggle_torrent_first_last task failed: {e}"))?
}

#[tauri::command]
pub async fn toggle_torrent_super_seeding(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.toggle_super_seeding(&id))
        .await
        .map_err(|e| format!("toggle_torrent_super_seeding task failed: {e}"))?
}

#[tauri::command]
pub async fn set_torrent_racing_mode(
    id: String,
    enabled: bool,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.set_torrent_racing_mode(&id, enabled))
        .await
        .map_err(|e| format!("set_torrent_racing_mode task failed: {e}"))?
}

#[tauri::command]
pub async fn set_file_priority(
    id: String,
    file_index: u32,
    priority: u8,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.set_file_priority(&id, file_index, priority))
        .await
        .map_err(|e| format!("set_file_priority task failed: {e}"))?
}

#[tauri::command]
pub async fn set_files_priority(
    id: String,
    file_indices: Vec<u32>,
    priority: u8,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.set_files_priority(&id, &file_indices, priority))
        .await
        .map_err(|e| format!("set_files_priority task failed: {e}"))?
}

#[tauri::command]
pub async fn rename_torrent(
    id: String,
    new_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.rename_torrent(&id, &new_name))
        .await
        .map_err(|e| format!("rename_torrent task failed: {e}"))?
}

#[tauri::command]
pub async fn rename_torrent_file(
    id: String,
    file_index: u32,
    new_file_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || engine.rename_torrent_file(&id, file_index, &new_file_name))
        .await
        .map_err(|e| format!("rename_torrent_file task failed: {e}"))?
}

#[tauri::command]
pub async fn rename_torrent_folder(
    id: String,
    old_folder_path: String,
    new_folder_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    let trimmed_name = new_folder_name.trim().to_string();
    if trimmed_name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if trimmed_name.contains('/')
        || trimmed_name.contains('\\')
        || trimmed_name == ".."
        || trimmed_name == "."
    {
        return Err(
            "Folder name cannot contain path separators or relative dot tokens".to_string(),
        );
    }
    let engine = engine.inner().clone();
    tokio::task::spawn_blocking(move || {
        engine.rename_torrent_folder(&id, &old_folder_path, &trimmed_name)
    })
    .await
    .map_err(|e| format!("rename_torrent_folder task failed: {e}"))?
}

#[tauri::command]
pub async fn create_torrent_file(
    params: CreateTorrentParams,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let source_path_str = params.source_path.trim().to_string();
    if source_path_str.is_empty() {
        return Err("Source file or folder path is required".to_string());
    }

    let source = std::path::PathBuf::from(&source_path_str);
    if !source.exists() {
        return Err(format!(
            "Source path '{}' does not exist on disk",
            source.display()
        ));
    }

    // A torrent without at least one tracker URL is rejected: no .torrent
    // file is generated until the user enters one (owner requirement).
    let trackers_clean: Vec<String> = params
        .trackers
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if trackers_clean.is_empty() {
        return Err("Please enter torrent tracker URL".to_string());
    }

    let out_dir = if params.output_path.trim().is_empty() {
        dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(params.output_path.trim())
    };

    let params_clone = params.clone();
    let source_clone = source.clone();
    let out_dir_clone = out_dir.clone();
    let trackers_for_build = trackers_clean.clone();

    let piece_size_kib = if params.piece_size_kib == 0 {
        let total_bytes: u64 = if source.is_file() {
            std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0)
        } else {
            crate::engine::mock::scan_directory_files(&source)
                .map(|files| files.iter().map(|f| f.size_bytes).sum())
                .unwrap_or(0)
        };
        recommended_piece_size_bytes(total_bytes) / 1024
    } else {
        params.piece_size_kib
    };

    let target_file = tokio::task::spawn_blocking(move || {
        crate::engine::metainfo::build_torrent_file(
            &source_clone,
            &out_dir_clone,
            piece_size_kib,
            &trackers_for_build,
            true, // private: 1 stays mandatory for created torrents per private-tracker security constraint
            &params_clone.comment,
            &torrent_creator_identity(),
        )
    })
    .await
    .map_err(|e| format!("Task execution error: {e}"))??;

    let target_file_str = target_file.to_string_lossy().to_string();

    if params.start_seeding {
        let parent = source.parent().map(|p| p.to_string_lossy().to_string());
        let _ = engine.add_torrent(&target_file_str, parent.as_deref());
    }

    Ok(target_file_str)
}

fn torrent_creator_identity() -> String {
    format!("Nebulae/{}", crate::engine::identity::get_app_version())
}

/// "Save torrent file as...": native save dialog (owner request) then write
/// the metainfo where the user picked.
///
/// The AppKit save panel MUST be shown on the main thread on macOS, so the
/// dialog runs via run_on_main_thread and the (possibly slow) file write
/// happens on a blocking thread - the UI never stalls.
#[tauri::command]
pub async fn save_torrent_file_as(
    app: tauri::AppHandle,
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = engine.export_torrent_bytes(&id)?;
        let torrents = engine.list_torrents()?;
        let item = torrents.iter().find(|t| t.id == id);
        let raw_name = item.map(|t| t.name.clone()).unwrap_or_else(|| id.clone());
        let clean_name = raw_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let filename = if clean_name.to_lowercase().ends_with(".torrent") {
            clean_name
        } else {
            format!("{clean_name}.torrent")
        };

        // Native save dialog on the main thread (rfd requirement on macOS).
        let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
        let dialog_name = filename.clone();
        app.run_on_main_thread(move || {
            let picked = rfd::FileDialog::new()
                .set_file_name(&dialog_name)
                .add_filter("Torrent files", &["torrent"])
                .save_file();
            let _ = tx.send(picked);
        })
        .map_err(|e| format!("Dialog unavailable: {e}"))?;
        let target = rx
            .recv()
            .map_err(|_| "Dialog did not return a result".to_string())?;

        let Some(target_path) = target else {
            return Err("cancelled".into());
        };

        std::fs::write(&target_path, &bytes)
            .map_err(|e| format!("Failed to write .torrent file: {e}"))?;
        Ok(target_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Task execution error: {e}"))?
}

#[tauri::command]
pub fn export_torrent_file(
    id: String,
    destination_path: Option<String>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    let bytes = engine.export_torrent_bytes(&id)?;
    let torrents = engine.list_torrents()?;
    let item = torrents.iter().find(|t| t.id == id);
    let raw_name = item.map(|t| t.name.clone()).unwrap_or_else(|| id.clone());
    let clean_name = raw_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let filename = if clean_name.to_lowercase().ends_with(".torrent") {
        clean_name
    } else {
        format!("{clean_name}.torrent")
    };

    let target_path = if let Some(dest) = destination_path {
        let p = std::path::PathBuf::from(dest.trim());
        if p.is_dir() {
            p.join(&filename)
        } else {
            p
        }
    } else {
        let downloads = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        downloads.join(&filename)
    };

    if let Some(parent) = target_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(&target_path, &bytes).map_err(|e| {
        format!(
            "Failed to write exported .torrent file to '{}': {e}",
            target_path.display()
        )
    })?;

    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_engine_status(engine: State<'_, SharedEngine>) -> Result<EngineStatus, String> {
    Ok(engine.engine_status())
}

#[tauri::command]
pub async fn trigger_kill_switch_command(engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.trigger_kill_switch()
}

#[tauri::command]
pub async fn reset_kill_switch_command(engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.reset_kill_switch()
}

/// Pick a disc image (ISO) for external playback. Any file can be chosen;
/// the caller validates the extension.
#[tauri::command]
pub async fn select_iso_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose file with prompt "Select a disc image (ISO)")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        #[cfg(target_os = "windows")]
        {
            let picked = rfd::FileDialog::new()
                .set_title("Select a disc image (ISO)")
                .add_filter("Disc images (*.iso)", &["iso"])
                .pick_file();
            Ok(picked.map(|p| p.to_string_lossy().to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Ok(None)
        }
    })
    .await
    .map_err(|e| format!("picker join error: {e}"))?
    .map(|v: Option<String>| {
        v.filter(|p| {
            std::path::Path::new(p)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("iso"))
                .unwrap_or(false)
        })
    })
}

#[tauri::command]
pub async fn select_source_folder() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose folder with prompt "Select Folder for Torrent Creation")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open folder picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(target_os = "windows")]
        {
            let picked = rfd::FileDialog::new()
                .set_title("Select Folder for Torrent Creation")
                .pick_folder();
            Ok(picked.map(|p| p.to_string_lossy().to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_dir("Select Folder for Torrent Creation")
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

#[tauri::command]
pub async fn select_source_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose file with prompt "Select File for Torrent Creation")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
        }
        #[cfg(target_os = "windows")]
        {
            let picked = rfd::FileDialog::new()
                .set_title("Select File for Torrent Creation")
                .pick_file();
            Ok(picked.map(|p| p.to_string_lossy().to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_file("Select File for Torrent Creation", None)
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

#[tauri::command]
pub async fn select_torrent_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose file of type {"torrent", "adc", "public.data"} with prompt "Select .torrent or .adc file")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        #[cfg(target_os = "windows")]
        {
            let picked = rfd::FileDialog::new()
                .set_title("Select .torrent or .adc file")
                .add_filter("Torrent & ADC Files (*.torrent, *.adc)", &["torrent", "adc"])
                .add_filter("All Files", &["*"])
                .pick_file();
            Ok(picked.map(|p| p.to_string_lossy().to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_file(
                "Select .torrent or .adc file",
                Some(("Torrent & ADC Files (*.torrent, *.adc)", "*.torrent *.adc")),
            )
        }
    })
    .await
    .map_err(|e| format!("Picker task error: {e}"))?
}

pub fn parse_dialog_output(
    status_success: bool,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Option<String>, String> {
    if status_success {
        let path = String::from_utf8_lossy(stdout).trim().to_string();
        if path.is_empty() {
            return Ok(None);
        }
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err(format!(
                "Selected path does not exist or is unreadable: {path}"
            ));
        }
        Ok(Some(path))
    } else if exit_code == Some(1) {
        // User cancelled the file dialog
        Ok(None)
    } else {
        let err_msg = String::from_utf8_lossy(stderr).trim().to_string();
        Err(format!(
            "Dialog failed with exit code {:?}: {}",
            exit_code,
            if err_msg.is_empty() {
                "Operation cancelled or dialog closed"
            } else {
                &err_msg
            }
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn linux_pick_dir(title: &str) -> Result<Option<String>, String> {
    match std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--directory",
            &format!("--title={title}"),
        ])
        .output()
    {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute zenity: {e}"));
        }
        Err(_) => {}
    }

    match std::process::Command::new("kdialog")
        .args(["--title", title, "--getexistingdirectory", "."])
        .output()
    {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute kdialog: {e}"));
        }
        Err(_) => {}
    }

    Err("No supported graphical file dialog found. Please install 'zenity' (GNOME/GTK) or 'kdialog' (KDE/Qt) to use directory selection.".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn linux_pick_file(title: &str, filter: Option<(&str, &str)>) -> Result<Option<String>, String> {
    let mut zenity_cmd = std::process::Command::new("zenity");
    zenity_cmd.args(["--file-selection", &format!("--title={title}")]);
    if let Some((name, patterns)) = filter {
        zenity_cmd.arg(format!("--file-filter={name} | {patterns}"));
        zenity_cmd.arg("--file-filter=All Files | *");
    }
    match zenity_cmd.output() {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute zenity: {e}"));
        }
        Err(_) => {}
    }

    let mut kdialog_cmd = std::process::Command::new("kdialog");
    kdialog_cmd.args(["--title", title, "--getopenfilename", "."]);
    if let Some((name, patterns)) = filter {
        kdialog_cmd.arg(format!("{patterns}|{name}"));
    }
    match kdialog_cmd.output() {
        Ok(output) => {
            return parse_dialog_output(
                output.status.success(),
                output.status.code(),
                &output.stdout,
                &output.stderr,
            );
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Failed to execute kdialog: {e}"));
        }
        Err(_) => {}
    }

    Err("No supported graphical file dialog found. Please install 'zenity' (GNOME/GTK) or 'kdialog' (KDE/Qt) to use file selection.".to_string())
}

#[tauri::command]
pub async fn select_download_directory() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
            with timeout of 86400 seconds
                try
                    tell application (path to frontmost application as text)
                        activate
                        return POSIX path of (choose folder with prompt "Select Download Directory")
                    end tell
                on error
                    return ""
                end try
            end timeout
            "#;
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output()
                .map_err(|e| format!("Failed to open directory picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        #[cfg(target_os = "windows")]
        {
            let picked = rfd::FileDialog::new()
                .set_title("Select Download Directory")
                .pick_folder();
            Ok(picked.map(|p| p.to_string_lossy().to_string()))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            linux_pick_dir("Select Download Directory")
        }
    })
    .await
    .map_err(|e| format!("Directory picker task error: {e}"))?
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemPathPresets {
    pub downloads: Option<String>,
    pub movies: Option<String>,
    pub desktop: Option<String>,
    pub documents: Option<String>,
}

#[tauri::command]
pub fn get_system_path_presets() -> Result<SystemPathPresets, String> {
    Ok(SystemPathPresets {
        downloads: dirs::download_dir().map(|p| p.to_string_lossy().to_string()),
        movies: dirs::video_dir().map(|p| p.to_string_lossy().to_string()),
        desktop: dirs::desktop_dir().map(|p| p.to_string_lossy().to_string()),
        documents: dirs::document_dir().map(|p| p.to_string_lossy().to_string()),
    })
}

/// Open a torrent's on-disk folder in the OS file manager. The frontend
/// invokes this for the media view's folder button and the manager's
/// "Show in Finder" context action; before this command existed every call
/// rejected with "command not found" and was swallowed by the UI, so the
/// buttons appeared completely dead.
#[tauri::command]
pub async fn open_torrent_folder(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = std::path::PathBuf::from(&path);
        if !p.exists() {
            return Err(format!("Path does not exist on disk: {path}"));
        }
        #[cfg(target_os = "macos")]
        let opened = if p.is_dir() {
            std::process::Command::new("open").arg(&p).status()
        } else {
            std::process::Command::new("open")
                .arg("-R")
                .arg(&p)
                .status()
        };
        #[cfg(target_os = "windows")]
        let opened = if p.is_dir() {
            crate::utils::process::create_hidden_command("explorer")
                .arg(&p)
                .status()
        } else {
            crate::utils::process::create_hidden_command("explorer")
                .arg(format!("/select,{}", p.display()))
                .status()
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let opened = std::process::Command::new("xdg-open")
            .arg(if p.is_dir() {
                &p
            } else {
                p.parent().unwrap_or(&p)
            })
            .status();
        match opened {
            // Windows: explorer.exe exits 1 on success for some invocations.
            // Treat 0 or 1 as success; anything else is a real failure.
            Ok(s) if s.success() || s.code() == Some(1) => Ok(()),
            Ok(s) => Err(format!("File manager exited with {s}")),
            Err(e) => Err(format!("Failed to open folder: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn torrent_creator_identity_uses_authoritative_package_version() {
        assert_eq!(
            torrent_creator_identity(),
            format!("Nebulae/{}", crate::engine::identity::get_app_version())
        );
    }

    fn expected(entries: &[(&str, u64)]) -> Vec<(u32, String, u64)> {
        entries
            .iter()
            .enumerate()
            .map(|(i, (path, size))| (i as u32, path.to_string(), *size))
            .collect()
    }

    #[test]
    fn test_match_content_exact_path_wins() {
        let exp = expected(&[("Movie/DISC_1.ISO", 100), ("Movie/DISC_2.ISO", 200)]);
        let disk = vec![
            ("Movie/DISC_1.ISO".to_string(), 100),
            ("Movie/DISC_2.ISO".to_string(), 200),
        ];
        let out = match_content_files(&exp, &disk);
        assert_eq!(out[0].matched_rel_path.as_deref(), Some("Movie/DISC_1.ISO"));
        assert_eq!(out[1].matched_rel_path.as_deref(), Some("Movie/DISC_2.ISO"));
    }

    #[test]
    fn test_match_content_same_name_different_folder() {
        let exp = expected(&[("Banner of the Stars III OVA (2005) R2J/DISC_1.ISO", 100)]);
        let disk = vec![("BANNER_S3/DISC_1.ISO".to_string(), 100)];
        let out = match_content_files(&exp, &disk);
        assert_eq!(
            out[0].matched_rel_path.as_deref(),
            Some("BANNER_S3/DISC_1.ISO")
        );
    }

    #[test]
    fn test_match_content_ambiguous_size_left_unmatched() {
        let exp = expected(&[("a/file.bin", 100)]);
        let disk = vec![
            ("x/data.bin".to_string(), 100),
            ("y/other.bin".to_string(), 100),
        ];
        let out = match_content_files(&exp, &disk);
        assert_eq!(out[0].matched_rel_path, None);
    }

    #[test]
    fn test_match_content_duplicate_name_same_size_picks_first() {
        let exp = expected(&[("a/file.bin", 100)]);
        let disk = vec![
            ("x/file.bin".to_string(), 100),
            ("y/file.bin".to_string(), 100),
        ];
        let out = match_content_files(&exp, &disk);
        assert_eq!(out[0].matched_rel_path.as_deref(), Some("x/file.bin"));
    }

    #[test]
    fn test_match_content_zero_size_never_auto_matched() {
        let exp = expected(&[("a/empty.nfo", 0)]);
        let disk = vec![("x/empty.nfo".to_string(), 0)];
        let out = match_content_files(&exp, &disk);
        assert_eq!(out[0].matched_rel_path, None);
    }

    #[test]
    fn test_match_content_size_unique_fallback() {
        let exp = expected(&[("Movie/DISC_1.ISO", 100)]);
        let disk = vec![("elsewhere/data.bin".to_string(), 100)];
        let out = match_content_files(&exp, &disk);
        assert_eq!(
            out[0].matched_rel_path.as_deref(),
            Some("elsewhere/data.bin")
        );
    }

    #[test]
    fn test_resolve_under_rejects_escape() {
        let base = std::path::Path::new("/tmp/base");
        assert!(resolve_under(base, "a/b.txt").is_ok());
        assert!(resolve_under(base, "/etc/passwd").is_err());
        assert!(resolve_under(base, "../escape.txt").is_err());
        assert!(resolve_under(base, "a/../../escape.txt").is_err());
        assert!(resolve_under(base, "").is_err());
    }

    #[test]
    fn test_apply_torrent_renames_end_to_end() {
        let root = std::env::temp_dir().join(format!("nebulae-rename-test-{}", std::process::id()));
        let src_dir = root.join("BANNER_S3");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("DISC_1.ISO"), vec![0u8; 100]).unwrap();
        fs::write(src_dir.join("DISC_2.ISO"), vec![0u8; 200]).unwrap();

        let renames = vec![
            TorrentRenameEntry {
                from_rel: "BANNER_S3/DISC_1.ISO".to_string(),
                to_rel: "Banner of the Stars III OVA (2005) R2J/DISC_1.ISO".to_string(),
            },
            TorrentRenameEntry {
                from_rel: "BANNER_S3/DISC_2.ISO".to_string(),
                to_rel: "Banner of the Stars III OVA (2005) R2J/DISC_2.ISO".to_string(),
            },
        ];
        let applied = apply_torrent_renames_impl(&root, &renames).unwrap();
        assert_eq!(applied, 2);

        let moved = root
            .join("Banner of the Stars III OVA (2005) R2J")
            .join("DISC_1.ISO");
        assert!(moved.exists());
        assert_eq!(fs::metadata(&moved).unwrap().len(), 100);
        assert!(!src_dir.join("DISC_1.ISO").exists());
        // The old folder itself is never deleted; only matched files move.
        assert!(src_dir.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_parse_dialog_output_user_cancel() {
        let res = parse_dialog_output(false, Some(1), b"", b"");
        assert_eq!(res, Ok(None));
    }

    #[test]
    fn test_parse_dialog_output_empty_selection() {
        let res = parse_dialog_output(true, Some(0), b"\n", b"");
        assert_eq!(res, Ok(None));
    }

    #[test]
    fn test_parse_dialog_output_nonexistent_path() {
        let res = parse_dialog_output(
            true,
            Some(0),
            b"/tmp/non_existent_nebulae_path_12345\n",
            b"",
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("does not exist or is unreadable"));
    }

    #[test]
    fn test_parse_dialog_output_path_with_spaces() {
        let temp_dir = std::env::temp_dir().join("nebulae test with spaces");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    fn sample_torrent(
        id: &str,
        hash: &str,
        name: &str,
        save_path: &str,
        progress: f32,
    ) -> TorrentItem {
        TorrentItem {
            id: id.to_string(),
            name: name.to_string(),
            total_bytes: 1_000_000,
            downloaded_bytes: (1_000_000.0 * progress) as u64,
            uploaded_bytes: 500_000,
            download_rate: 0,
            upload_rate: 0,
            seconds_downloading: 100,
            seconds_seeding: 50,
            progress,
            status: if progress >= 1.0 {
                "Seeding".to_string()
            } else {
                "Downloading".to_string()
            },
            eta_seconds: None,
            seeds: Some(10),
            peers: 5,
            ratio: 0.5,
            category: "movies".to_string(),
            is_media: true,
            poster_url: None,
            raw_name: Some(name.to_string()),
            content_path: Some(format!("{save_path}/{name}")),
            info_hash: hash.to_string(),
            save_path: save_path.to_string(),
            historical_save_path: None,
            added_on: 1_700_000_000,
            completed_on: if progress >= 1.0 {
                Some(1_700_001_000)
            } else {
                None
            },
            comment: "".to_string(),
            created_by: "".to_string(),
            piece_size: 16384,
            total_pieces: 100,
            sequential_download: false,
            first_last_piece_prio: false,
            sort_order: None,
            super_seeding: false,
            last_checked: None,
            tags: Vec::new(),
            files: Vec::new(),
            trackers: Vec::new(),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            bandwidth_priority: None,
            peer_limit: None,
            racing_mode: None,
            error_string: None,
            media_meta: None,
        }
    }

    #[test]
    fn test_destructive_removal_blocks_shared_files() {
        let t1 = sample_torrent("1", "hash1", "SharedMovie", "/downloads", 1.0);
        let t2 = sample_torrent("2", "hash2", "SharedMovie", "/downloads", 1.0);
        let all = vec![t1.clone(), t2.clone()];

        // Attempting to destructively remove t1 while t2 shares the path must fail
        let res = check_destructive_removal_safety(Some(&t1), &all, false);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("shared with another active torrent"));
    }

    #[test]
    fn test_destructive_removal_blocks_external_media() {
        let t1 = sample_torrent("1", "hash1", "StandaloneMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        // If marked external, destructive deletion must be blocked
        let res = check_destructive_removal_safety(Some(&t1), &all, true);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("Target media is an external or reclassified item"));
    }

    #[test]
    fn test_destructive_removal_blocks_ambiguous_root_path() {
        let mut t1 = sample_torrent("1", "hash1", "", "/", 1.0);
        t1.raw_name = Some("".to_string());
        let all = vec![t1.clone()];

        let res = check_destructive_removal_safety(Some(&t1), &all, false);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("ambiguous or unverified"));
    }

    #[test]
    fn test_destructive_removal_allows_independent_torrent() {
        let t1 = sample_torrent("1", "hash1", "IndependentMovie", "/downloads", 1.0);
        let t2 = sample_torrent("2", "hash2", "OtherMovie", "/downloads", 1.0);
        let all = vec![t1.clone(), t2.clone()];

        let res = check_destructive_removal_safety(Some(&t1), &all, false);
        assert!(res.is_ok());
    }

    #[test]
    fn test_should_reclassify_as_external_rules() {
        let completed = sample_torrent("1", "hash1", "Movie1", "/downloads", 1.0);
        let incomplete = sample_torrent("2", "hash2", "Movie2", "/downloads", 0.5);
        let shared_a = sample_torrent("3", "hash3", "SharedMovie", "/downloads", 1.0);
        let shared_b = sample_torrent("4", "hash4", "SharedMovie", "/downloads", 1.0);
        let all = vec![
            completed.clone(),
            incomplete.clone(),
            shared_a.clone(),
            shared_b.clone(),
        ];

        // 1. Completed standalone torrent -> Reclassifies
        assert!(should_reclassify_as_external(Some(&completed), &all, true));

        // 2. Incomplete torrent kept on disk -> Does NOT reclassify as completed External
        assert!(!should_reclassify_as_external(
            Some(&incomplete),
            &all,
            false
        ));

        // 3. Completed torrent whose files are still shared with another active torrent -> Does NOT reclassify yet
        assert!(!should_reclassify_as_external(Some(&shared_a), &all, true));
    }

    #[test]
    fn test_parse_dialog_output_path_with_unicode() {
        let temp_dir = std::env::temp_dir().join("nebulae_テスト_日本語_файл");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_dialog_output_path_with_quotes_and_metachars() {
        let temp_dir = std::env::temp_dir().join("nebulae_test's & $VAR");
        fs::create_dir_all(&temp_dir).unwrap();
        let path_str = temp_dir.to_string_lossy().to_string();

        let res = parse_dialog_output(true, Some(0), format!("{path_str}\n").as_bytes(), b"");
        assert_eq!(res, Ok(Some(path_str)));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_dialog_output_dialog_error() {
        let res = parse_dialog_output(false, Some(127), b"", b"zenity: command not found\n");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("zenity: command not found"));
    }

    #[test]
    fn test_inspect_torrent_file_returns_metainfo_bytes() {
        let raw_meta = crate::engine::metainfo::RawMetainfo {
            announce: Some("http://tracker.example.com/announce".to_string()),
            announce_list: None,
            comment: Some("Test Comment".to_string()),
            created_by: Some("Nebulae Test".to_string()),
            creation_date: Some(1234567890),
            info: crate::engine::metainfo::RawInfo {
                name: Some("test_video.mkv".to_string()),
                piece_length: Some(262144),
                pieces: vec![0u8; 20],
                length: Some(1048576),
                files: None,
                private: Some(1),
            },
        };
        let bencode_bytes = serde_bencode::to_bytes(&raw_meta).unwrap();
        let res = inspect_torrent_file(None, Some(bencode_bytes.clone())).unwrap();
        assert_eq!(res.name, "test_video.mkv");
        assert_eq!(res.total_bytes, 1048576);
        assert!(res.is_private);
        assert_eq!(res.metainfo_bytes, Some(bencode_bytes));
    }

    #[test]
    fn inspect_magnet_scheme_is_case_insensitive() {
        let result = inspect_torrent_file(
            Some("MAGNET:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Sample".into()),
            None,
        )
        .unwrap();
        assert_eq!(result.info_hash, "0123456789abcdef0123456789abcdef01234567");
        assert_eq!(result.name, "Sample");
    }

    #[test]
    fn inspect_adc_scheme_advises_resolution_command() {
        let result = inspect_torrent_file(Some("adc://torrent/12345".into()), None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Use resolve_adc_torrent to resolve metainfo"));
    }

    #[test]
    fn test_operation_snapshot_serialization() {
        let snap = TorrentOperationSnapshot {
            operation_id: "op-123".to_string(),
            torrent_id: "hash_abc".to_string(),
            kind: TorrentOperationKind::Remove { delete_files: true },
            phase: TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
            destination: None,
            safe_error: None,
            outcome: None,
            revision: 1,
            created_at: 1000,
        };
        let json_str = serde_json::to_string(&snap).unwrap();
        assert!(json_str.contains("\"type\":\"remove\""));
        assert!(json_str.contains("\"delete_files\":true"));
        assert!(json_str.contains("\"phase\":\"removing_torrent_and_deleting_files\""));

        let deserialized: TorrentOperationSnapshot = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized, snap);
    }

    #[test]
    fn test_operation_registry_deduplication() {
        let t1 = sample_torrent("t_dedup", "hash_dedup", "DedupMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        // Clear any leftover state
        remove_active_operation("t_dedup", Some("hash_dedup"));

        let mut transitions = Vec::new();

        // Initial remove request succeeds
        let res1 = run_remove_lifecycle_step(
            "t_dedup",
            false,
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| transitions.push(s.clone()),
        );
        assert!(res1.is_ok());

        // Now simulate in-progress operation
        let in_progress = TorrentOperationSnapshot {
            operation_id: "op-active".to_string(),
            torrent_id: "hash_dedup".to_string(),
            kind: TorrentOperationKind::Remove {
                delete_files: false,
            },
            phase: TorrentOperationPhase::RemovingTorrent,
            destination: None,
            safe_error: None,
            outcome: None,
            revision: 1,
            created_at: 1000,
        };
        register_active_operation("t_dedup", Some("hash_dedup"), &in_progress);

        // Duplicate call with same delete_files returns Ok without running engine remove
        let mut engine_called = false;
        let res2 = run_remove_lifecycle_step(
            "t_dedup",
            false,
            Some(&t1),
            &all,
            false,
            |_id, _del| {
                engine_called = true;
                Ok(())
            },
            |_id, _del| Ok(()),
            |_s| {},
        );
        assert!(
            res2.is_ok(),
            "Duplicate remove request must reuse active result"
        );
        assert!(
            !engine_called,
            "Engine remove must not be re-run on duplicate request"
        );

        // Clean up
        remove_active_operation("t_dedup", Some("hash_dedup"));
    }

    #[test]
    fn test_operation_registry_rejects_conflicting_operation() {
        remove_active_operation("t_conflict", Some("hash_conflict"));

        let in_progress = TorrentOperationSnapshot {
            operation_id: "op-move".to_string(),
            torrent_id: "hash_conflict".to_string(),
            kind: TorrentOperationKind::Move {
                destination: "/new_path".to_string(),
            },
            phase: TorrentOperationPhase::Pending,
            destination: Some("/new_path".to_string()),
            safe_error: None,
            outcome: None,
            revision: 1,
            created_at: 1000,
        };
        register_active_operation("t_conflict", Some("hash_conflict"), &in_progress);

        let t1 = sample_torrent(
            "t_conflict",
            "hash_conflict",
            "ConflictMovie",
            "/downloads",
            1.0,
        );
        let all = vec![t1.clone()];

        // Attempting to remove while move is active must fail
        let res = run_remove_lifecycle_step(
            "t_conflict",
            false,
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |_s| {},
        );
        assert!(res.is_err(), "Conflicting operation must be rejected");
        assert!(res.unwrap_err().contains("already in progress"));

        remove_active_operation("t_conflict", Some("hash_conflict"));
    }

    #[test]
    fn test_concurrent_reservation_is_atomic() {
        remove_active_operation("t_atomic", Some("hash_atomic"));

        // First reservation succeeds
        let res1 = reserve_active_remove_operation(
            "t_atomic",
            Some("hash_atomic"),
            true,
            TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
        );
        assert!(matches!(res1, Ok(ReservationResult::Reserved(_))));

        // Concurrent/second reservation sees active operation and returns Duplicate
        let res2 = reserve_active_remove_operation(
            "t_atomic",
            Some("hash_atomic"),
            true,
            TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
        );
        assert!(matches!(res2, Ok(ReservationResult::Duplicate(_))));

        remove_active_operation("t_atomic", Some("hash_atomic"));
    }

    #[test]
    fn test_complete_alias_dismissal() {
        remove_active_operation("t_dismiss", Some("hash_dismiss"));

        let in_progress = TorrentOperationSnapshot {
            operation_id: "op-dismiss-1".to_string(),
            torrent_id: "hash_dismiss".to_string(),
            kind: TorrentOperationKind::Remove { delete_files: true },
            phase: TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
            destination: None,
            safe_error: None,
            outcome: None,
            revision: 1,
            created_at: 1000,
        };
        register_active_operation("t_dismiss", Some("hash_dismiss"), &in_progress);

        // In-flight dismissal must be rejected
        let dismiss_in_flight = dismiss_active_operation("t_dismiss");
        assert!(dismiss_in_flight.is_err());
        assert!(dismiss_in_flight
            .unwrap_err()
            .contains("Only terminal errors can be dismissed"));

        // Transition to Error
        let err_snap = TorrentOperationSnapshot {
            phase: TorrentOperationPhase::Error,
            safe_error: Some("Simulated failure".to_string()),
            revision: 2,
            ..in_progress
        };
        register_active_operation("t_dismiss", Some("hash_dismiss"), &err_snap);

        // Dismissal by id must clear both id alias and info_hash alias
        let dismiss_res = dismiss_active_operation("t_dismiss");
        assert!(dismiss_res.is_ok());

        assert!(get_active_operation("t_dismiss").is_none());
        assert!(get_active_operation("hash_dismiss").is_none());
        assert!(get_active_operation("HASH_DISMISS").is_none());
    }

    #[test]
    fn test_error_not_successful_duplicate() {
        remove_active_operation("t_err_dup", Some("hash_err_dup"));

        let err_snap = TorrentOperationSnapshot {
            operation_id: "op-failed".to_string(),
            torrent_id: "hash_err_dup".to_string(),
            kind: TorrentOperationKind::Remove { delete_files: true },
            phase: TorrentOperationPhase::Error,
            destination: None,
            safe_error: Some("Cannot delete files: storage unverified".to_string()),
            outcome: None,
            revision: 2,
            created_at: 1000,
        };
        register_active_operation("t_err_dup", Some("hash_err_dup"), &err_snap);

        let t1 = sample_torrent(
            "t_err_dup",
            "hash_err_dup",
            "FailedMovie",
            "/downloads",
            1.0,
        );
        let all = vec![t1.clone()];

        // Must return Err and NOT return Ok as a duplicate!
        let res = run_remove_lifecycle_step(
            "t_err_dup",
            true,
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |_s| {},
        );
        assert!(res.is_err(), "Error record must not return Ok duplicate");
        assert!(res
            .unwrap_err()
            .contains("Cannot delete files: storage unverified"));

        remove_active_operation("t_err_dup", Some("hash_err_dup"));
    }

    #[test]
    fn test_reconciliation_failure_propagates_correct_message() {
        remove_active_operation("t_recon_fail", Some("hash_recon_fail"));

        let t1 = sample_torrent(
            "t_recon_fail",
            "hash_recon_fail",
            "ReconMovie",
            "/downloads",
            1.0,
        );
        let all = vec![t1.clone()];

        let mut recorded_phases = Vec::new();
        let mut engine_remove_called = 0;

        let res = run_remove_lifecycle_step(
            "t_recon_fail",
            false,
            Some(&t1),
            &all,
            false,
            |_id, _del| {
                engine_remove_called += 1;
                Ok(())
            },
            |_id, _del| Err("Simulated database write error".to_string()),
            |s| recorded_phases.push((s.phase.clone(), s.safe_error.clone())),
        );

        // Engine removal was invoked once and not retried
        assert_eq!(engine_remove_called, 1);

        // Must return the exact prescribed failure message: "Torrent removed; Media update failed."
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Torrent removed; Media update failed.");

        // Safe error on snapshot must be updated accordingly
        let last_phase = recorded_phases.last().unwrap();
        assert_eq!(last_phase.0, TorrentOperationPhase::Error);
        assert_eq!(
            last_phase.1,
            Some("Torrent removed; Media update failed.".to_string())
        );

        remove_active_operation("t_recon_fail", Some("hash_recon_fail"));
    }

    #[test]
    fn test_file_deletion_outcome_claims_unconfirmed() {
        remove_active_operation("t_outcome", Some("hash_outcome"));

        let t1 = sample_torrent(
            "t_outcome",
            "hash_outcome",
            "OutcomeMovie",
            "/downloads",
            1.0,
        );
        let all = vec![t1.clone()];

        let mut final_snapshot = None;

        let res = run_remove_lifecycle_step(
            "t_outcome",
            true, // delete_files = true
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| {
                if s.phase == TorrentOperationPhase::Complete {
                    final_snapshot = Some(s.clone());
                }
            },
        );

        assert!(res.is_ok());
        let snap = final_snapshot.expect("Complete phase must be emitted");
        assert_eq!(
            snap.outcome,
            Some("Torrent removed. File-deletion request processed; deletion of every file could not be verified.".to_string())
        );

        remove_active_operation("t_outcome", Some("hash_outcome"));
    }

    #[test]
    fn test_destructive_removal_safety_records_error_phase() {
        remove_active_operation("t_unsafe", Some("hash_unsafe"));

        let t1 = sample_torrent("t_unsafe", "hash_unsafe", "SharedMovie", "/downloads", 1.0);
        let t2 = sample_torrent("t_other", "hash_other", "SharedMovie", "/downloads", 1.0);
        let all = vec![t1.clone(), t2.clone()];

        let mut recorded_phases = Vec::new();
        let mut engine_called = false;

        let res = run_remove_lifecycle_step(
            "t_unsafe",
            true, // delete files = true while shared with t2
            Some(&t1),
            &all,
            false,
            |_id, _del| {
                engine_called = true;
                Ok(())
            },
            |_id, _del| Ok(()),
            |s| recorded_phases.push((s.phase.clone(), s.safe_error.clone())),
        );

        assert!(res.is_err());
        assert!(
            !engine_called,
            "Engine removal must not be called when safety check fails"
        );
        assert_eq!(
            recorded_phases,
            vec![
                (TorrentOperationPhase::RemovingTorrentAndDeletingFiles, None),
                (
                    TorrentOperationPhase::Error,
                    Some(
                        "Cannot delete files: One or more files are shared with another active torrent. Remove the other torrent first, or select 'Remove without deleting files'."
                            .to_string()
                    )
                )
            ]
        );

        remove_active_operation("t_unsafe", Some("hash_unsafe"));
    }

    #[test]
    fn test_remove_torrent_lifecycle_phases() {
        remove_active_operation("t_lifecycle", Some("hash_lifecycle"));

        let t1 = sample_torrent(
            "t_lifecycle",
            "hash_lifecycle",
            "SafeMovie",
            "/downloads",
            1.0,
        );
        let all = vec![t1.clone()];

        let mut recorded_phases = Vec::new();

        let res = run_remove_lifecycle_step(
            "t_lifecycle",
            true, // delete files
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| recorded_phases.push(s.phase.clone()),
        );

        assert!(res.is_ok());
        assert_eq!(
            recorded_phases,
            vec![
                TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
                TorrentOperationPhase::Reconciling,
                TorrentOperationPhase::Complete,
            ]
        );

        // Verify cleaned up from active map upon completion
        assert!(get_active_operation("hash_lifecycle").is_none());
    }

    #[test]
    fn test_keep_files_removal_lifecycle_phases() {
        remove_active_operation("t_keep", Some("hash_keep"));

        let t1 = sample_torrent("t_keep", "hash_keep", "KeepMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let mut recorded_phases = Vec::new();

        let res = run_remove_lifecycle_step(
            "t_keep",
            false, // delete files = false
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| recorded_phases.push(s.phase.clone()),
        );

        assert!(res.is_ok());
        assert_eq!(
            recorded_phases,
            vec![
                TorrentOperationPhase::RemovingTorrent,
                TorrentOperationPhase::Reconciling,
                TorrentOperationPhase::Complete,
            ]
        );

        assert!(get_active_operation("hash_keep").is_none());
    }

    #[test]
    fn test_move_lifecycle_phases() {
        remove_active_operation("t_move_sync", Some("hash_move_sync"));

        let mut recorded_phases = Vec::new();

        let res = run_move_lifecycle_step(
            "t_move_sync",
            "/new/destination/path",
            Some("hash_move_sync"),
            crate::engine::traits::MoveCompletionSemantics::Synchronous,
            |_id, _dest| Ok(()),
            |s| recorded_phases.push((s.phase.clone(), s.destination.clone(), s.outcome.clone())),
        );

        assert!(res.is_ok());
        assert_eq!(
            recorded_phases,
            vec![
                (
                    TorrentOperationPhase::Moving,
                    Some("/new/destination/path".to_string()),
                    None
                ),
                (
                    TorrentOperationPhase::Complete,
                    Some("/new/destination/path".to_string()),
                    Some("Moved to /new/destination/path".to_string())
                ),
            ]
        );

        // Verify cleaned up from active map upon completion
        assert!(get_active_operation("t_move_sync").is_none());
        assert!(get_active_operation("hash_move_sync").is_none());
    }

    #[test]
    fn test_move_lifecycle_unconfirmed_terminates_truthfully() {
        remove_active_operation("t_move_unconf", Some("hash_move_unconf"));

        let mut recorded_phases = Vec::new();

        let res = run_move_lifecycle_step(
            "t_move_unconf",
            "/remote/destination/path",
            Some("hash_move_unconf"),
            crate::engine::traits::MoveCompletionSemantics::Unconfirmed,
            |_id, _dest| Ok(()),
            |s| recorded_phases.push((s.phase.clone(), s.destination.clone(), s.outcome.clone())),
        );

        assert!(res.is_ok());
        assert_eq!(
            recorded_phases,
            vec![
                (
                    TorrentOperationPhase::Moving,
                    Some("/remote/destination/path".to_string()),
                    None
                ),
                (
                    TorrentOperationPhase::Complete,
                    Some("/remote/destination/path".to_string()),
                    Some("Location set; file relocation not confirmed by engine".to_string())
                ),
            ]
        );

        // Verify active reservation is cleared truthfully rather than left indefinitely active
        assert!(get_active_operation("t_move_unconf").is_none());
        assert!(get_active_operation("hash_move_unconf").is_none());
    }

    #[test]
    fn test_move_lifecycle_async_callback_leaves_moving_for_callback() {
        remove_active_operation("t_move_async", Some("hash_move_async"));

        let mut recorded_phases = Vec::new();

        let res = run_move_lifecycle_step(
            "t_move_async",
            "/async/destination/path",
            Some("hash_move_async"),
            crate::engine::traits::MoveCompletionSemantics::AsyncCallback,
            |_id, _dest| Ok(()),
            |s| recorded_phases.push((s.phase.clone(), s.destination.clone(), s.outcome.clone())),
        );

        assert!(res.is_ok());
        assert_eq!(
            recorded_phases,
            vec![(
                TorrentOperationPhase::Moving,
                Some("/async/destination/path".to_string()),
                None
            )]
        );

        // Verify active reservation remains in map awaiting callback
        let in_flight = get_active_operation("t_move_async").expect("Must remain in active map");
        assert_eq!(in_flight.phase, TorrentOperationPhase::Moving);

        // Simulated callback completion
        complete_active_move_operation(
            "t_move_async",
            Some("hash_move_async"),
            "/async/destination/path",
            None,
        );

        assert!(get_active_operation("t_move_async").is_none());
    }

    #[test]
    fn test_move_lifecycle_error() {
        remove_active_operation("t_move_err", Some("hash_move_err"));

        let mut recorded_phases = Vec::new();

        let res = run_move_lifecycle_step(
            "t_move_err",
            "/protected/destination",
            Some("hash_move_err"),
            crate::engine::traits::MoveCompletionSemantics::Synchronous,
            |_id, _dest| Err("Permission denied".to_string()),
            |s| recorded_phases.push((s.phase.clone(), s.safe_error.clone())),
        );

        assert!(res.is_err());
        assert_eq!(
            recorded_phases,
            vec![
                (TorrentOperationPhase::Moving, None),
                (
                    TorrentOperationPhase::Error,
                    Some("Move failed: Permission denied".to_string())
                ),
            ]
        );

        // Verify operation remains in active map as error so it can be dismissed
        let active =
            get_active_operation("t_move_err").expect("Error operation must remain in map");
        assert_eq!(active.phase, TorrentOperationPhase::Error);
        assert_eq!(
            active.safe_error,
            Some("Move failed: Permission denied".to_string())
        );

        remove_active_operation("t_move_err", Some("hash_move_err"));
    }

    #[test]
    fn test_conflicting_remove_blocked_by_active_move() {
        remove_active_operation("t_move_conflict", Some("hash_move_conflict"));

        let res = reserve_active_move_operation(
            "t_move_conflict",
            Some("hash_move_conflict"),
            "/path/a",
            TorrentOperationPhase::Moving,
        );
        assert!(matches!(res, Ok(ReservationResult::Reserved(_))));

        // Attempting to reserve remove on the moving torrent must fail
        let remove_res = reserve_active_remove_operation(
            "t_move_conflict",
            Some("hash_move_conflict"),
            false,
            TorrentOperationPhase::RemovingTorrent,
        );
        assert!(remove_res.is_err());
        assert!(remove_res.unwrap_err().contains("already in progress"));

        remove_active_operation("t_move_conflict", Some("hash_move_conflict"));
    }

    #[test]
    fn test_conflicting_move_blocked_by_active_remove() {
        remove_active_operation("t_remove_conflict", Some("hash_remove_conflict"));

        let res = reserve_active_remove_operation(
            "t_remove_conflict",
            Some("hash_remove_conflict"),
            false,
            TorrentOperationPhase::RemovingTorrent,
        );
        assert!(matches!(res, Ok(ReservationResult::Reserved(_))));

        // Attempting to reserve move on removing torrent must fail
        let move_res = reserve_active_move_operation(
            "t_remove_conflict",
            Some("hash_remove_conflict"),
            "/new/dir",
            TorrentOperationPhase::Moving,
        );
        assert!(move_res.is_err());
        assert!(move_res.unwrap_err().contains("already in progress"));

        remove_active_operation("t_remove_conflict", Some("hash_remove_conflict"));
    }

    #[test]
    fn test_duplicate_move_reservation_is_deduplicated() {
        remove_active_operation("t_move_dup", Some("hash_move_dup"));

        let res1 = reserve_active_move_operation(
            "t_move_dup",
            Some("hash_move_dup"),
            "/same/destination",
            TorrentOperationPhase::Moving,
        );
        assert!(matches!(res1, Ok(ReservationResult::Reserved(_))));

        // Same destination: returns Duplicate
        let res2 = reserve_active_move_operation(
            "t_move_dup",
            Some("hash_move_dup"),
            "/same/destination",
            TorrentOperationPhase::Moving,
        );
        assert!(matches!(res2, Ok(ReservationResult::Duplicate(_))));

        // Different destination: rejected as conflict
        let res3 = reserve_active_move_operation(
            "t_move_dup",
            Some("hash_move_dup"),
            "/different/destination",
            TorrentOperationPhase::Moving,
        );
        assert!(res3.is_err());
        assert!(res3.unwrap_err().contains("already in progress"));

        remove_active_operation("t_move_dup", Some("hash_move_dup"));
    }

    #[test]
    fn test_error_move_not_successful_duplicate() {
        remove_active_operation("t_move_err_dup", Some("hash_move_err_dup"));

        let err_snap = TorrentOperationSnapshot {
            operation_id: "op-move-failed".to_string(),
            torrent_id: "hash_move_err_dup".to_string(),
            kind: TorrentOperationKind::Move {
                destination: "/same/dest".to_string(),
            },
            phase: TorrentOperationPhase::Error,
            destination: Some("/same/dest".to_string()),
            safe_error: Some("Move failed: disk full".to_string()),
            outcome: None,
            revision: 2,
            created_at: 1000,
        };
        register_active_operation("t_move_err_dup", Some("hash_move_err_dup"), &err_snap);

        // Attempting to reserve move with matching destination must NOT return Ok Duplicate
        let res = reserve_active_move_operation(
            "t_move_err_dup",
            Some("hash_move_err_dup"),
            "/same/dest",
            TorrentOperationPhase::Moving,
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Move failed: disk full"));

        remove_active_operation("t_move_err_dup", Some("hash_move_err_dup"));
    }

    #[test]
    fn test_complete_active_move_clears_aliases() {
        remove_active_operation("t_move_alias", Some("hash_move_alias"));

        let res = reserve_active_move_operation(
            "t_move_alias",
            Some("hash_move_alias"),
            "/dest/folder",
            TorrentOperationPhase::Moving,
        );
        assert!(matches!(res, Ok(ReservationResult::Reserved(_))));

        assert!(get_active_operation("t_move_alias").is_some());
        assert!(get_active_operation("hash_move_alias").is_some());

        complete_active_move_operation(
            "t_move_alias",
            Some("hash_move_alias"),
            "/dest/folder",
            None,
        );

        assert!(get_active_operation("t_move_alias").is_none());
        assert!(get_active_operation("hash_move_alias").is_none());
    }

    #[test]
    fn test_complete_active_move_with_custom_metadata_failure_outcome() {
        remove_active_operation("t_move_meta_err", Some("hash_move_meta_err"));

        let res = reserve_active_move_operation(
            "t_move_meta_err",
            Some("hash_move_meta_err"),
            "/disk/new_location",
            TorrentOperationPhase::Moving,
        );
        assert!(matches!(res, Ok(ReservationResult::Reserved(_))));

        complete_active_move_operation(
            "t_move_meta_err",
            Some("hash_move_meta_err"),
            "/disk/new_location",
            Some("Files moved; saved location could not be updated.".to_string()),
        );

        // Cleared from active map so not left indefinitely active
        assert!(get_active_operation("t_move_meta_err").is_none());
    }

    #[test]
    fn test_remove_observable_while_engine_pending() {
        let op_key = "t_pending_obs";
        let info_hash = "hash_pending_obs";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "PendingMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let (engine_started_tx, engine_started_rx) = std::sync::mpsc::channel();
        let (release_engine_tx, release_engine_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            run_remove_lifecycle_step(
                op_key,
                true,
                Some(&t1),
                &all,
                false,
                |_id, _del| {
                    let _ = engine_started_tx.send(());
                    let _ = release_engine_rx.recv();
                    Ok(())
                },
                |_id, _del| Ok(()),
                |_s| {},
            )
        });

        engine_started_rx.recv().unwrap();

        // While pending, the operation is observable by id and info_hash
        let active_by_id =
            get_active_operation(op_key).expect("Operation must be observable by id");
        let active_by_hash =
            get_active_operation(info_hash).expect("Operation must be observable by info_hash");
        assert_eq!(active_by_id.operation_id, active_by_hash.operation_id);
        assert_eq!(
            active_by_id.phase,
            TorrentOperationPhase::RemovingTorrentAndDeletingFiles
        );

        let active_list = list_active_operations();
        assert!(
            active_list
                .iter()
                .any(|op| op.operation_id == active_by_id.operation_id),
            "Operation must appear in list_active_operations()"
        );

        let _ = release_engine_tx.send(());
        let res = handle.join().unwrap();
        assert!(res.is_ok());

        assert!(get_active_operation(op_key).is_none());
        remove_active_operation(op_key, Some(info_hash));
    }

    #[test]
    fn test_remove_pending_rejects_duplicate_and_conflicting_move() {
        let op_key = "t_pending_conflict";
        let info_hash = "hash_pending_conflict";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "ConflictMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let (engine_started_tx, engine_started_rx) = std::sync::mpsc::channel();
        let (release_engine_tx, release_engine_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            run_remove_lifecycle_step(
                op_key,
                true,
                Some(&t1),
                &all,
                false,
                |_id, _del| {
                    let _ = engine_started_tx.send(());
                    let _ = release_engine_rx.recv();
                    Ok(())
                },
                |_id, _del| Ok(()),
                |_s| {},
            )
        });

        engine_started_rx.recv().unwrap();

        // Duplicate remove with same delete_files returns Duplicate
        let dup_res = reserve_active_remove_operation(
            op_key,
            Some(info_hash),
            true,
            TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
        );
        assert!(matches!(dup_res, Ok(ReservationResult::Duplicate(_))));

        // Conflicting remove with delete_files=false is rejected
        let conflict_remove = reserve_active_remove_operation(
            op_key,
            Some(info_hash),
            false,
            TorrentOperationPhase::RemovingTorrent,
        );
        assert!(conflict_remove.is_err());
        assert!(conflict_remove.unwrap_err().contains("already in progress"));

        // Conflicting move is rejected
        let conflict_move = reserve_active_move_operation(
            op_key,
            Some(info_hash),
            "/other/path",
            TorrentOperationPhase::Moving,
        );
        assert!(conflict_move.is_err());
        assert!(conflict_move.unwrap_err().contains("already in progress"));

        let _ = release_engine_tx.send(());
        let res = handle.join().unwrap();
        assert!(res.is_ok());

        remove_active_operation(op_key, Some(info_hash));
    }

    #[test]
    fn test_remove_successful_acknowledgement_reports_unconfirmed_deletion() {
        let op_key = "t_ack_unconf";
        let info_hash = "hash_ack_unconf";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "AckMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let mut emitted_phases = Vec::new();
        let mut final_outcome = None;

        let res = run_remove_lifecycle_step(
            op_key,
            true,
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| {
                emitted_phases.push(s.phase.clone());
                if s.phase == TorrentOperationPhase::Complete {
                    final_outcome = s.outcome.clone();
                }
            },
        );

        assert!(res.is_ok());
        assert_eq!(
            emitted_phases,
            vec![
                TorrentOperationPhase::RemovingTorrentAndDeletingFiles,
                TorrentOperationPhase::Reconciling,
                TorrentOperationPhase::Complete,
            ]
        );
        assert_eq!(
            final_outcome,
            Some("Torrent removed. File-deletion request processed; deletion of every file could not be verified.".to_string())
        );

        remove_active_operation(op_key, Some(info_hash));
    }

    #[test]
    fn test_remove_known_rejection_does_not_reconcile_media() {
        let op_key = "t_reject";
        let info_hash = "hash_reject";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "RejectMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let mut reconcile_called = false;
        let mut last_snapshot = None;

        let res = run_remove_lifecycle_step(
            op_key,
            true,
            Some(&t1),
            &all,
            false,
            |_id, _del| {
                Err("Transmission RPC call 'torrent-remove' failed: torrent not found".to_string())
            },
            |_id, _del| {
                reconcile_called = true;
                Ok(())
            },
            |s| {
                last_snapshot = Some(s.clone());
            },
        );

        assert!(res.is_err());
        assert!(
            !reconcile_called,
            "Media reconciliation must NOT be called on engine rejection"
        );

        let snap = last_snapshot.expect("Error snapshot must be emitted");
        assert_eq!(snap.phase, TorrentOperationPhase::Error);
        assert_eq!(
            snap.safe_error,
            Some("Engine removal failed: Transmission RPC call 'torrent-remove' failed: torrent not found".to_string())
        );
        assert_eq!(snap.outcome, None);

        // Stays in active operations as Error so it can be observed/dismissed
        let active = get_active_operation(op_key).expect("Error operation must remain in map");
        assert_eq!(active.phase, TorrentOperationPhase::Error);

        remove_active_operation(op_key, Some(info_hash));
    }

    #[test]
    fn test_remove_timeout_uncertainty_produces_truthful_error_and_no_media_reconciliation() {
        let op_key = "t_timeout";
        let info_hash = "hash_timeout";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "TimeoutMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let mut reconcile_called = false;
        let mut last_snapshot = None;

        let res = run_remove_lifecycle_step(
            op_key,
            true,
            Some(&t1),
            &all,
            false,
            |_id, _del| {
                Err(
                    "Transmission RPC timeout: error sending request for url: operation timed out"
                        .to_string(),
                )
            },
            |_id, _del| {
                reconcile_called = true;
                Ok(())
            },
            |s| {
                last_snapshot = Some(s.clone());
            },
        );

        assert!(res.is_err());
        assert!(
            !reconcile_called,
            "Media reconciliation must NOT be called on transport timeout"
        );

        let snap = last_snapshot.expect("Error snapshot must be emitted");
        assert_eq!(snap.phase, TorrentOperationPhase::Error);
        assert_eq!(
            snap.safe_error,
            Some("The engine response was not received. Removal or file deletion may still be in progress. Check the torrent’s status before trying again.".to_string())
        );
        assert_eq!(
            snap.outcome, None,
            "Must produce no success claim on timeout"
        );

        let active = get_active_operation(op_key).expect("Error operation must remain in map");
        assert_eq!(active.phase, TorrentOperationPhase::Error);
        assert_eq!(
            active.safe_error,
            Some("The engine response was not received. Removal or file deletion may still be in progress. Check the torrent’s status before trying again.".to_string())
        );

        remove_active_operation(op_key, Some(info_hash));
    }

    #[test]
    fn test_keep_files_and_destructive_safety_guards_intact() {
        let op_key = "t_keep_guard";
        let info_hash = "hash_keep_guard";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "KeepGuardMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let mut emitted_phases = Vec::new();
        let mut final_outcome = None;

        // 1. Keep files (del = false) completes with truthful "Torrent removed"
        let res = run_remove_lifecycle_step(
            op_key,
            false,
            Some(&t1),
            &all,
            false,
            |_id, _del| Ok(()),
            |_id, _del| Ok(()),
            |s| {
                emitted_phases.push(s.phase.clone());
                if s.phase == TorrentOperationPhase::Complete {
                    final_outcome = s.outcome.clone();
                }
            },
        );

        assert!(res.is_ok());
        assert_eq!(
            emitted_phases,
            vec![
                TorrentOperationPhase::RemovingTorrent,
                TorrentOperationPhase::Reconciling,
                TorrentOperationPhase::Complete,
            ]
        );
        assert_eq!(
            final_outcome,
            Some("Torrent removed. Files kept.".to_string())
        );
        remove_active_operation(op_key, Some(info_hash));

        // 2. Destructive safety check blocks removal when shared with another active torrent
        let op_unsafe = "t_guard_unsafe";
        let hash_unsafe = "hash_guard_unsafe";
        remove_active_operation(op_unsafe, Some(hash_unsafe));

        let tu1 = sample_torrent(op_unsafe, hash_unsafe, "SharedRelease", "/downloads", 1.0);
        let tu2 = sample_torrent(
            "t_other_release",
            "hash_other_release",
            "SharedRelease",
            "/downloads",
            1.0,
        );
        let shared_all = vec![tu1.clone(), tu2.clone()];

        let mut guard_called = false;
        let guard_res = run_remove_lifecycle_step(
            op_unsafe,
            true, // destructive removal
            Some(&tu1),
            &shared_all,
            false,
            |_id, _del| {
                guard_called = true;
                Ok(())
            },
            |_id, _del| Ok(()),
            |_s| {},
        );

        assert!(guard_res.is_err());
        assert!(
            !guard_called,
            "Destructive engine call must be blocked by safety guard"
        );
        let active_err = get_active_operation(op_unsafe).expect("Safety error must be recorded");
        assert_eq!(active_err.phase, TorrentOperationPhase::Error);
        assert!(active_err
            .safe_error
            .unwrap()
            .contains("shared with another active torrent"));

        remove_active_operation(op_unsafe, Some(hash_unsafe));
    }

    #[test]
    fn test_registry_locks_released_during_engine_work() {
        let op_key = "t_lock_sync";
        let info_hash = "hash_lock_sync";
        remove_active_operation(op_key, Some(info_hash));

        let t1 = sample_torrent(op_key, info_hash, "LockSyncMovie", "/downloads", 1.0);
        let all = vec![t1.clone()];

        let (engine_started_tx, engine_started_rx) = std::sync::mpsc::channel();
        let (release_engine_tx, release_engine_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            run_remove_lifecycle_step(
                op_key,
                true,
                Some(&t1),
                &all,
                false,
                |_id, _del| {
                    let _ = engine_started_tx.send(());
                    let _ = release_engine_rx.recv();
                    Ok(())
                },
                |_id, _del| Ok(()),
                |_s| {},
            )
        });

        // Wait until engine_remove is executing
        engine_started_rx.recv().unwrap();

        // Deterministic synchronization test:
        // Attempt to lock get_active_operations_map() via try_lock.
        // If the registry mutex were held across engine_remove, try_lock would never succeed
        // because engine_remove is blocked awaiting release_engine_rx.
        let mut acquired = false;
        for _ in 0..50 {
            if let Ok(map) = get_active_operations_map().try_lock() {
                assert!(
                    map.contains_key(op_key),
                    "Registry remains readable and contains the active operation"
                );
                acquired = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            acquired,
            "Operation registry mutex must NOT be held across blocking engine call"
        );

        // Resume and complete engine callback
        let _ = release_engine_tx.send(());
        let res = handle.join().unwrap();
        assert!(res.is_ok());

        remove_active_operation(op_key, Some(info_hash));
    }
}

/// One dialog for external playback: accepts a release ROOT folder
/// (containing all disc folders, like a torrent download) or an ISO
/// disc image. Files and folders must both be selectable, so a plain
/// `choose folder`/`choose file` pair cannot be used. The panel is
/// presented NATIVELY on the app's main thread: an osascript-hosted
/// panel opens beneath the app window (invisible), which read as
/// "nothing happens".
/// Dedicated folder selection for external movie / disc releases (BDMV, VIDEO_TS, box sets).
#[tauri::command]
pub async fn select_disc_folder() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let picked = rfd::FileDialog::new()
            .set_title(
                "Add Movie - select the release ROOT folder that holds ALL of its disc folders",
            )
            .pick_folder();
        Ok(picked.map(|p| p.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| format!("folder picker join error: {e}"))?
}

/// Dedicated file selection for external ISO / IMG disc images.
#[tauri::command]
pub async fn select_disc_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let picked = rfd::FileDialog::new()
            .set_title("Add Movie - select an ISO disc image")
            .add_filter("Disc images (*.iso, *.img)", &["iso", "img"])
            .add_filter("All files", &["*"])
            .pick_file();
        Ok(picked.map(|p| p.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| format!("file picker join error: {e}"))?
}

/// Dedicated file selection for custom poster images.
#[tauri::command]
pub async fn select_image_file() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let picked = rfd::FileDialog::new()
            .set_title("Select Poster Image")
            .add_filter(
                "Image files (*.jpg, *.jpeg, *.png, *.webp)",
                &["jpg", "jpeg", "png", "webp"],
            )
            .add_filter("All files", &["*"])
            .pick_file();
        Ok(picked.map(|p| p.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| format!("file picker join error: {e}"))?
}

/// One dialog for external playback: accepts a release ROOT folder
/// (containing all disc folders, like a torrent download) or an ISO
/// disc image. Files and folders must both be selectable, so a plain
/// `choose folder`/`choose file` pair cannot be used. The panel is
/// presented NATIVELY on the app's main thread: an osascript-hosted
/// panel opens beneath the app window (invisible), which read as
/// "nothing happens".
#[tauri::command]
pub async fn select_disc_source(
    app: tauri::AppHandle,
    kind: Option<String>,
) -> Result<Option<String>, String> {
    if let Some(ref k) = kind {
        if k == "folder" {
            return select_disc_folder().await;
        } else if k == "file" {
            return select_disc_file().await;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
        app.run_on_main_thread(move || {
            let picked = unsafe { present_disc_panel() };
            let _ = tx.send(picked);
        })
        .map_err(|e| format!("main thread unavailable: {e}"))?;
        rx.recv().map_err(|_| "panel channel lost".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        let _ = &app;
        // Windows: release folders (BDMV, VIDEO_TS, multi-disc) are the primary
        // movie structure. Present the folder browser first. If cancelled,
        // present the ISO file picker.
        if let Some(picked_folder) = rfd::FileDialog::new()
            .set_title(
                "Add Movie - select the release ROOT folder (or cancel to choose an ISO file)",
            )
            .pick_folder()
        {
            return Ok(Some(picked_folder.to_string_lossy().to_string()));
        }
        let picked_file = rfd::FileDialog::new()
            .set_title("Add Movie - select an ISO disc image")
            .add_filter("Disc images (*.iso, *.img)", &["iso", "img"])
            .add_filter("All files", &["*"])
            .pick_file();
        Ok(picked_file.map(|p| p.to_string_lossy().to_string()))
    }

    #[cfg(target_os = "linux")]
    {
        let _ = &app;
        let out = std::process::Command::new("zenity")
            .args([
                "--file-selection",
                "--directory",
                "--title",
                "Add Movie - select the release ROOT folder",
            ])
            .output()
            .map_err(|e| format!("Failed to open picker: {e}"))?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !path.is_empty() {
            return Ok(Some(path));
        }
        let out = std::process::Command::new("zenity")
            .args([
                "--file-selection",
                "--title",
                "Add Movie - select an ISO disc image",
            ])
            .output()
            .map_err(|e| format!("Failed to open picker: {e}"))?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if out.status.success() && !path.is_empty() {
            Some(path)
        } else {
            None
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = &app;
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
unsafe fn present_disc_panel() -> Option<String> {
    use objc2::runtime::{AnyClass, AnyObject};
    let cstr_cls = |name: &str| AnyClass::get(std::ffi::CString::new(name).unwrap().as_c_str());
    let cls = cstr_cls("NSOpenPanel")?;
    let panel: *mut AnyObject = objc2::msg_send![cls, openPanel];
    if panel.is_null() {
        return None;
    }
    let ns_string = cstr_cls("NSString").unwrap();
    let ns_str = |s: &str| -> *mut AnyObject {
        let c = std::ffi::CString::new(s).unwrap();
        objc2::msg_send![ns_string, stringWithUTF8String: c.as_ptr()]
    };
    let _: () = objc2::msg_send![panel, setCanChooseFiles: true];
    let _: () = objc2::msg_send![panel, setCanChooseDirectories: true];
    let _: () = objc2::msg_send![panel, setAllowsMultipleSelection: false];
    let _: () = objc2::msg_send![panel, setPrompt: ns_str("Add Movie")];
    let _: () = objc2::msg_send![panel, setMessage: ns_str(
        "Select the release ROOT folder that holds ALL of its disc folders (like a torrent download) — or an ISO disc image. Picking a single disc folder plays only that disc."
    )];
    let rc: i64 = objc2::msg_send![panel, runModal];
    // NSModalResponseOK is 1 (the old NSOKButton); 1000 was wrong and
    // silently discarded every successful selection.
    if rc != 1 {
        return None; // cancelled
    }
    let url: *mut AnyObject = objc2::msg_send![panel, URL];
    if url.is_null() {
        return None;
    }
    let path: *mut AnyObject = objc2::msg_send![url, path];
    if path.is_null() {
        return None;
    }
    let cstr: *const std::os::raw::c_char = objc2::msg_send![path, UTF8String];
    if cstr.is_null() {
        return None;
    }
    Some(
        std::ffi::CStr::from_ptr(cstr)
            .to_string_lossy()
            .into_owned(),
    )
}

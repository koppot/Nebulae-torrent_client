use crate::engine::{EngineStatus, SharedEngine};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentFileItem {
    pub index: u32,
    pub name: String,
    pub size_bytes: u64,
    pub progress: f32,
    pub priority: u8, // 0: DoNotDownload, 1: Normal, 2: High, 3: Maximum
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TorrentItem {
    pub id: String,
    pub name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_rate: u64,
    pub upload_rate: u64,
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

    // qBittorrent metadata & control fields
    pub info_hash: String,
    pub save_path: String,
    pub added_on: u64,
    pub completed_on: Option<u64>,
    pub comment: String,
    pub created_by: String,
    pub piece_size: u32,
    pub total_pieces: u32,
    pub sequential_download: bool,
    pub first_last_piece_prio: bool,
    #[serde(default)]
    pub super_seeding: bool,
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
            });
        }
    }

    // 2. If source string is passed
    if let Some(src) = source {
        let trimmed = src.trim();

        // Base64 encoded torrent?
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
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
                    });
                }
            }
        }

        // Magnet link?
        if trimmed.starts_with("magnet:") {
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
            });
        }

        // Disk path?
        let norm_path = crate::engine::mock::normalize_source_path(trimmed);
        if norm_path.exists() {
            if norm_path.is_file() {
                if let Ok(parsed) = crate::engine::metainfo::ParsedTorrent::parse_file(&norm_path) {
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
                    });
                }
                if let Ok(meta) = norm_path.metadata() {
                    let total_bytes = meta.len();
                    let file_name = norm_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Local File".to_string());
                    let piece_size = recommended_piece_size_bytes(total_bytes);
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
                });
            }
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

#[tauri::command]
pub async fn list_torrents(engine: State<'_, SharedEngine>) -> Result<Vec<TorrentItem>, String> {
    engine.list_torrents()
}

#[tauri::command]
pub fn add_torrent(
    source: String,
    download_dir: Option<String>,
    bytes: Option<Vec<u8>>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    if let Some(raw) = bytes {
        if !raw.is_empty() {
            return engine.add_torrent_from_bytes(&raw, download_dir.as_deref());
        }
    }
    engine.add_torrent(&source, download_dir.as_deref())
}

#[tauri::command]
pub fn add_torrent_bytes(
    bytes: Vec<u8>,
    download_dir: Option<String>,
    engine: State<'_, SharedEngine>,
) -> Result<String, String> {
    engine.add_torrent_from_bytes(&bytes, download_dir.as_deref())
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
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("No save destination set".to_string());
    }
    let expanded = if trimmed == "~" {
        dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string())?
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or_else(|| "Cannot resolve home directory".to_string())?
            .join(rest)
    } else {
        std::path::PathBuf::from(trimmed)
    };
    if !expanded.is_absolute() {
        return Err(format!(
            "Save destination '{}' is not an absolute folder",
            trimmed
        ));
    }
    Ok(expanded)
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

#[tauri::command]
pub fn remove_torrent(
    id: String,
    delete_files: Option<bool>,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.remove_torrent(&id, delete_files.unwrap_or(false))
}

#[tauri::command]
pub async fn pause_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.pause_torrent(&id)
}

#[tauri::command]
pub async fn resume_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.resume_torrent(&id)
}

#[tauri::command]
pub async fn pause_all_torrents(engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.pause_all()
}

#[tauri::command]
pub async fn resume_all_torrents(engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.resume_all()
}

#[tauri::command]
pub async fn force_recheck_torrent(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.force_recheck(&id)
}

#[tauri::command]
pub async fn reannounce_torrent(id: String, engine: State<'_, SharedEngine>) -> Result<(), String> {
    engine.reannounce(&id)
}

#[tauri::command]
pub fn set_torrent_location(
    id: String,
    new_path: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.set_location(&id, &new_path)
}

#[tauri::command]
pub fn set_torrent_category(
    id: String,
    category: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.set_category(&id, &category)
}

#[tauri::command]
pub fn set_torrent_speed_limits(
    id: String,
    dl_kib: u32,
    ul_kib: u32,
    ratio_limit: f32,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.set_torrent_limits(&id, dl_kib, ul_kib, ratio_limit)
}

#[tauri::command]
pub async fn toggle_torrent_sequential(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    engine.toggle_sequential_download(&id)
}

#[tauri::command]
pub async fn toggle_torrent_first_last(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    engine.toggle_first_last_piece_priority(&id)
}

#[tauri::command]
pub fn toggle_torrent_super_seeding(
    id: String,
    engine: State<'_, SharedEngine>,
) -> Result<bool, String> {
    engine.toggle_super_seeding(&id)
}

#[tauri::command]
pub fn set_file_priority(
    id: String,
    file_index: u32,
    priority: u8,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.set_file_priority(&id, file_index, priority)
}

#[tauri::command]
pub fn rename_torrent(
    id: String,
    new_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent(&id, &new_name)
}

#[tauri::command]
pub fn rename_torrent_file(
    id: String,
    file_index: u32,
    new_file_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent_file(&id, file_index, &new_file_name)
}

#[tauri::command]
pub fn rename_torrent_folder(
    id: String,
    old_folder_path: String,
    new_folder_name: String,
    engine: State<'_, SharedEngine>,
) -> Result<(), String> {
    engine.rename_torrent_folder(&id, &old_folder_path, &new_folder_name)
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
            &params_clone.created_by,
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
        let dialog_app = app.clone();
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
        #[cfg(not(target_os = "macos"))]
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
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.FolderBrowserDialog
            $f.Description = "Select Folder for Torrent Creation"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.SelectedPath
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open folder picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
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
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.OpenFileDialog
            $f.Title = "Select File for Torrent Creation"
            $f.Filter = "All Files (*.*)|*.*"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.FileName
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() { Ok(None) } else { Ok(Some(path)) }
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
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.OpenFileDialog
            $f.Filter = "Torrent & ADC Files (*.torrent;*.adc)|*.torrent;*.adc|BitTorrent Files (*.torrent)|*.torrent|ADC Metainfo Files (*.adc)|*.adc|All Files (*.*)|*.*"
            $f.Title = "Select .torrent or .adc file"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.FileName
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open file picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
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
            let script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.FolderBrowserDialog
            $f.Description = "Select Download Directory"
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                $f.SelectedPath
            }
            "#;
            let output = crate::utils::process::run_hidden_powershell(script)
                .map_err(|e| format!("Failed to open directory picker: {e}"))?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
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
pub fn open_torrent_folder(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Folder does not exist: {path}"));
    }
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open").arg(&p).status();
    #[cfg(target_os = "windows")]
    let opened = std::process::Command::new("explorer").arg(&p).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let opened = std::process::Command::new("xdg-open").arg(&p).status();
    match opened {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("File manager exited with {s}")),
        Err(e) => Err(format!("Failed to open folder: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
}

/// One dialog for external playback: accepts a release ROOT folder
/// (containing all disc folders, like a torrent download) or an ISO
/// disc image. Files and folders must both be selectable, so a plain
/// `choose folder`/`choose file` pair cannot be used. The panel is
/// presented NATIVELY on the app's main thread: an osascript-hosted
/// panel opens beneath the app window (invisible), which read as
/// "nothing happens".
#[tauri::command]
pub async fn select_disc_source(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();

    #[cfg(target_os = "macos")]
    {
        app.run_on_main_thread(move || {
            let picked = unsafe { present_disc_panel() };
            let _ = tx.send(picked);
        })
        .map_err(|e| format!("main thread unavailable: {e}"))?;
        return Ok(rx.recv().map_err(|_| "panel channel lost".to_string())?);
    }

    #[cfg(target_os = "windows")]
    {
        let _ = &tx;
        // Windows: file dialog for ISO first; folder dialog for a release
        // root when the file dialog is cancelled.
        let iso = r#"
        Add-Type -AssemblyName System.Windows.Forms
        $f = New-Object System.Windows.Forms.OpenFileDialog
        $f.Title = "Add Movie - select an ISO disc image (or cancel for a folder)"
        $f.Filter = "Disc images (*.iso;*.img)|*.iso;*.img|All files|*.*"
        if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $f.FileName }
        "#;
        if let Ok(out) = crate::utils::process::run_hidden_powershell(iso) {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(Some(path));
            }
        }
        let folder = r#"
        Add-Type -AssemblyName System.Windows.Forms
        $f = New-Object System.Windows.Forms.FolderBrowserDialog
        $f.Description = "Add Movie - select the release ROOT folder that holds ALL of its disc folders"
        if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $f.SelectedPath }
        "#;
        let out = crate::utils::process::run_hidden_powershell(folder)
            .map_err(|e| format!("Failed to open folder picker: {e}"))?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if path.is_empty() { None } else { Some(path) })
    }

    #[cfg(target_os = "linux")]
    {
        let _ = &tx;
        // Linux: zenity file selection (ISO); directory selection when
        // the file dialog is cancelled.
        let out = std::process::Command::new("zenity")
            .args([
                "--file-selection",
                "--title",
                "Add Movie - select an ISO disc image",
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
                "--directory",
                "--title",
                "Add Movie - select the release ROOT folder",
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
}

#[cfg(target_os = "macos")]
unsafe fn present_disc_panel() -> Option<String> {
    use objc2::runtime::{Class, Object};
    let cstr_cls = |name: &str| Class::get(std::ffi::CString::new(name).unwrap().as_c_str());
    let cls = cstr_cls("NSOpenPanel")?;
    let panel: *mut Object = objc2::msg_send![cls, openPanel];
    if panel.is_null() {
        return None;
    }
    let ns_string = cstr_cls("NSString").unwrap();
    let ns_str = |s: &str| -> *mut Object {
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
    let url: *mut Object = objc2::msg_send![panel, URL];
    if url.is_null() {
        return None;
    }
    let path: *mut Object = objc2::msg_send![url, path];
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

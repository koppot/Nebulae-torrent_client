use crate::db::Database;
use crate::engine::SharedEngine;
use crate::qbit_import::locator::{DefaultLocator, LocatorTrait};
use crate::qbit_import::reader::QbitReader;
use crate::qbit_import::remover::QbitRemover;
use crate::qbit_import::types::{
    ImportOptions, ImportSummaryReport, QbitProcessCheck, QbitTorrent, QbitTorrentSummary,
    ScanResult,
};
use crate::qbit_import::QbitImporter;
use crate::utils::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;
use tracing::warn;

type QbitCache = Option<(PathBuf, Vec<QbitTorrent>)>;

#[derive(Default)]
pub struct QbitImportState(pub Arc<Mutex<QbitCache>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QbitLocationDto {
    pub data_dir: String,
    pub config_dir: Option<String>,
    pub source_description: String,
    pub has_sqlite_db: bool,
    pub has_legacy_backup: bool,
    pub is_qbit_running: bool,
    pub is_writable: bool,
}

#[tauri::command]
pub fn qbit_auto_locate() -> Result<QbitLocationDto, String> {
    let locator = DefaultLocator::new();
    let location = locator
        .locate()
        .ok_or_else(|| "Could not automatically locate qBittorrent directory".to_string())?;

    let is_running = locator.is_process_running().0;
    let is_writable = locator.check_dir_writable(&location.data_dir);

    Ok(QbitLocationDto {
        data_dir: location.data_dir.to_string_lossy().to_string(),
        config_dir: location.config_dir.map(|p| p.to_string_lossy().to_string()),
        source_description: location.source_description,
        has_sqlite_db: location.has_sqlite_db,
        has_legacy_backup: location.has_legacy_backup,
        is_qbit_running: is_running,
        is_writable,
    })
}

#[tauri::command]
pub fn qbit_validate_dir(dir: String) -> Result<QbitLocationDto, String> {
    let locator = DefaultLocator::new();
    let p = Path::new(&dir);
    let location = locator.validate_dir(p)?;

    let is_running = locator.is_process_running().0;
    let is_writable = locator.check_dir_writable(&location.data_dir);

    Ok(QbitLocationDto {
        data_dir: location.data_dir.to_string_lossy().to_string(),
        config_dir: location.config_dir.map(|c| c.to_string_lossy().to_string()),
        source_description: location.source_description,
        has_sqlite_db: location.has_sqlite_db,
        has_legacy_backup: location.has_legacy_backup,
        is_qbit_running: is_running,
        is_writable,
    })
}

#[tauri::command]
pub fn qbit_check_running() -> QbitProcessCheck {
    let (running, name) = DefaultLocator::new().is_process_running();
    let message = if running {
        "qBittorrent is currently running. It must be closed to safely remove torrents.".to_string()
    } else {
        "qBittorrent is closed.".to_string()
    };
    QbitProcessCheck {
        is_running: running,
        process_name: name,
        message,
    }
}

#[tauri::command]
pub async fn qbit_scan_torrents(
    dir: String,
    state: State<'_, QbitImportState>,
    engine: State<'_, SharedEngine>,
) -> Result<ScanResult, String> {
    let locator = DefaultLocator::new();
    let data_dir = PathBuf::from(&dir);
    let location = locator.validate_dir(&data_dir)?;

    let config_dir_ref = location.config_dir.as_deref();
    let category_paths = QbitReader::load_category_save_paths(config_dir_ref);

    // 1. Read legacy BT_backup
    let legacy_torrents = QbitReader::read_legacy_bt_backup(&location.data_dir, &category_paths);

    // 2. Read SQLite torrents.db
    let sqlite_file = location.data_dir.join("torrents.db");
    let sqlite_torrents = if sqlite_file.is_file() {
        QbitReader::read_sqlite_db(&sqlite_file, &category_paths).unwrap_or_else(|e| {
            warn!("Failed reading SQLite torrents.db: {e}");
            Vec::new()
        })
    } else {
        Vec::new()
    };

    // 3. Merge sources
    let (mut merged_torrents, format_desc) =
        QbitReader::merge_sources(legacy_torrents, sqlite_torrents);

    // 4. Mark deduplication against existing Nebula torrents
    let existing_hashes: std::collections::HashSet<String> = match engine.list_torrents() {
        Ok(list) => list
            .into_iter()
            .map(|t| t.info_hash.to_lowercase())
            .collect(),
        Err(_) => std::collections::HashSet::new(),
    };

    for t in &mut merged_torrents {
        if existing_hashes.contains(&t.info_hash.to_lowercase()) {
            t.already_in_nebula = true;
        }
    }

    // 5. Generate UI summaries
    let mut summaries = Vec::new();
    for t in &merged_torrents {
        let source_format = match t.source {
            crate::qbit_import::QbitSourceType::LegacyFile { .. } => "Legacy BT_backup".to_string(),
            crate::qbit_import::QbitSourceType::SqliteRow { .. } => {
                "SQLite torrents.db".to_string()
            }
        };

        let status_label = if t.already_in_nebula {
            "Already in Nebula".to_string()
        } else if t.missing_path {
            "Missing path".to_string()
        } else {
            t.state_string.clone()
        };

        summaries.push(QbitTorrentSummary {
            info_hash: t.info_hash.clone(),
            name: t.name.clone(),
            size_bytes: t.total_size,
            save_path: t.save_path.clone(),
            effective_path: t.save_path.clone(),
            progress: t.progress,
            state: t.state_string.clone(),
            category: t.category.clone().unwrap_or_else(|| "None".to_string()),
            status_label,
            already_in_nebula: t.already_in_nebula,
            missing_path: t.missing_path,
            source_format,
        });
    }

    // Sort summaries: alphabetical by name
    summaries.sort_by_key(|a| a.name.to_lowercase());

    let is_running = locator.is_process_running().0;
    let is_writable = locator.check_dir_writable(&location.data_dir);
    let mut warnings = Vec::new();
    if is_running {
        warnings.push(
            "qBittorrent is currently running. Resume data may be slightly stale until qBittorrent is closed."
                .to_string(),
        );
    }
    if !is_writable {
        warnings.push(
            "qBittorrent data directory is read-only. Optional removal will be unavailable."
                .to_string(),
        );
    }

    // Cache scanned items in State
    if let Ok(mut lock) = state.0.lock() {
        *lock = Some((location.data_dir.clone(), merged_torrents));
    }

    Ok(ScanResult {
        qbit_dir: location.data_dir.to_string_lossy().to_string(),
        storage_format: format_desc,
        torrents: summaries,
        warnings,
        is_qbit_running: is_running,
        is_writable,
    })
}

#[tauri::command]
pub async fn qbit_execute_import(
    options: ImportOptions,
    state: State<'_, QbitImportState>,
    engine: State<'_, SharedEngine>,
    db: State<'_, Arc<Database>>,
    paths: State<'_, Arc<AppPaths>>,
) -> Result<ImportSummaryReport, String> {
    let (data_dir, all_torrents) = {
        let lock = state.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        lock.clone()
            .ok_or_else(|| "No scanned torrents found in cache. Please scan first.".to_string())?
    };

    let selected_set: std::collections::HashSet<String> = options
        .info_hashes
        .iter()
        .map(|h| h.to_lowercase())
        .collect();

    let target_torrents: Vec<QbitTorrent> = all_torrents
        .into_iter()
        .filter(|t| selected_set.contains(&t.info_hash.to_lowercase()))
        .collect();

    if target_torrents.is_empty() {
        return Err("No torrents selected for import".to_string());
    }

    let backup_root = paths.base_data_dir.join("qbit-import-backups");
    let (report, _) = QbitImporter::execute_import(
        target_torrents,
        options,
        &data_dir,
        &backup_root,
        engine.inner().clone(),
        db.inner().clone(),
        &paths.logs_dir,
    )
    .await;

    Ok(report)
}

#[tauri::command]
pub fn qbit_restore_backup(
    manifest_path: String,
    confirm: bool,
    whole_profile_confirmation: Option<String>,
) -> Result<usize, String> {
    let p = Path::new(&manifest_path);
    let proc_checker = crate::qbit_import::remover::SystemProcessChecker;
    let mode = if let Some(phrase) = whole_profile_confirmation {
        crate::qbit_import::remover::RestoreMode::WholeProfileRollback {
            confirmation_phrase: phrase,
        }
    } else {
        crate::qbit_import::remover::RestoreMode::Selective
    };
    QbitRemover::restore_from_manifest_with_mode(p, &proc_checker, confirm, mode)
}

use super::locator::LocatorTrait;
use super::types::{QbitSourceType, QbitTorrent, TorrentNotRemovedInfo};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrashMethod {
    RecycleBin,
    MacTrash,
    PermanentDelete,
}

pub trait TrashProvider: Send + Sync {
    fn delete_file(&self, path: &Path) -> Result<TrashMethod, String>;
}

pub struct DefaultTrashProvider;

impl TrashProvider for DefaultTrashProvider {
    fn delete_file(&self, path: &Path) -> Result<TrashMethod, String> {
        #[cfg(target_os = "macos")]
        {
            // Try macOS AppleScript/Finder trash
            if let Ok(canon) = path.canonicalize() {
                let script = format!(
                    "tell application \"Finder\" to delete POSIX file \"{}\"",
                    canon.to_string_lossy().replace('\"', "\\\"")
                );
                let res = std::process::Command::new("osascript")
                    .args(["-e", &script])
                    .output();
                if let Ok(out) = res {
                    if out.status.success() {
                        return Ok(TrashMethod::MacTrash);
                    }
                }
            }

            // Fallback to permanent delete if trash is not available (e.g. external network volume)
            fs::remove_file(path).map_err(|e| format!("Failed to delete file: {e}"))?;
            Ok(TrashMethod::PermanentDelete)
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;
            // Best effort Windows recycle bin via PowerShell or Win32 API, fallback to remove_file
            let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let script = format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
                canon.to_string_lossy().replace('\'', "''")
            );
            let res = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .output();

            if let Ok(out) = res {
                if out.status.success() {
                    return Ok(TrashMethod::RecycleBin);
                }
            }

            fs::remove_file(path).map_err(|e| format!("Failed to remove file: {e}"))?;
            Ok(TrashMethod::PermanentDelete)
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            fs::remove_file(path).map_err(|e| format!("Failed to remove file: {e}"))?;
            Ok(TrashMethod::PermanentDelete)
        }
    }
}

pub trait ProcessChecker: Send + Sync {
    fn is_qbittorrent_running(&self) -> bool;
}

pub struct SystemProcessChecker;
impl ProcessChecker for SystemProcessChecker {
    fn is_qbittorrent_running(&self) -> bool {
        super::locator::DefaultLocator::new().is_process_running().0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub info_hash: String,
    pub original_files: Vec<String>,
    pub backup_files: Vec<String>,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub timestamp: String,
    pub backup_folder: String,
    pub qbit_data_dir: String,
    pub entries: Vec<BackupEntry>,
    pub manual_restore_instructions: String,
}

pub struct QbitRemover;

impl QbitRemover {
    /// HARD SAFETY GUARD: Validates that a path is 100% strictly safe to delete:
    /// (a) Must resolve to inside detected qBittorrent data dir (rejects junctions/symlink traversal and ..)
    /// (b) Must sit directly in BT_backup
    /// (c) Filename must match ^[0-9a-fA-F]{40,64}\.(torrent|fastresume)$
    /// REJECTS ANY PAYLOAD PATH, SAVE PATH, OR ROOT PATH.
    pub fn validate_path_safety(file_path: &Path, qbit_data_dir: &Path) -> Result<(), String> {
        // 1. Path must have a file name
        let file_name = file_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| {
                format!(
                    "Invalid file path '{}': missing filename",
                    file_path.display()
                )
            })?;

        // 2. Strict filename regex: 40 to 64 hex characters followed by .torrent or .fastresume
        let is_valid_name = {
            let parts: Vec<&str> = file_name.split('.').collect();
            if parts.len() != 2 {
                false
            } else {
                let stem = parts[0];
                let ext = parts[1].to_lowercase();
                (ext == "torrent" || ext == "fastresume")
                    && (stem.len() >= 40 && stem.len() <= 64)
                    && stem.chars().all(|c| c.is_ascii_hexdigit())
            }
        };

        if !is_valid_name {
            return Err(format!(
                "SAFETY GUARD REFUSAL: File '{}' does not match safe qBittorrent entry pattern (^[0-9a-fA-F]{{40,64}}\\.(torrent|fastresume)$). Deletion rejected.",
                file_name
            ));
        }

        // 3. Parent directory must be named 'BT_backup'
        let parent = file_path.parent().ok_or_else(|| {
            format!(
                "Invalid path '{}': no parent directory",
                file_path.display()
            )
        })?;

        if parent.file_name().and_then(|f| f.to_str()) != Some("BT_backup") {
            return Err(format!(
                "SAFETY GUARD REFUSAL: File '{}' must reside directly inside 'BT_backup'. Parent is '{:?}'. Deletion rejected.",
                file_name,
                parent.file_name()
            ));
        }

        // 4. Must resolve inside qbit_data_dir without directory traversal
        let parent_parent = parent.parent().ok_or_else(|| {
            format!(
                "Invalid path '{}': missing grand-parent directory",
                file_path.display()
            )
        })?;

        // Normalize paths for comparison
        let normalized_data_dir = qbit_data_dir
            .canonicalize()
            .unwrap_or_else(|_| qbit_data_dir.to_path_buf());
        let normalized_parent = parent_parent
            .canonicalize()
            .unwrap_or_else(|_| parent_parent.to_path_buf());

        if normalized_parent != normalized_data_dir {
            return Err(format!(
                "SAFETY GUARD REFUSAL: Path '{}' escapes the qBittorrent data directory '{}'. Deletion rejected.",
                file_path.display(),
                qbit_data_dir.display()
            ));
        }

        Ok(())
    }

    /// Creates and verifies a backup before any deletion occurs.
    /// If verification fails, ABORTS removal immediately.
    pub fn create_and_verify_backup(
        torrents_to_remove: &[QbitTorrent],
        qbit_data_dir: &Path,
        backup_root: &Path,
    ) -> Result<BackupManifest, String> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let backup_dir = backup_root.join(&timestamp);
        fs::create_dir_all(&backup_dir)
            .map_err(|e| format!("Failed to create backup directory {:?}: {e}", backup_dir))?;

        let mut manifest_entries = Vec::new();
        let mut has_sqlite_entries = false;
        let mut sqlite_db_path: Option<PathBuf> = None;

        for t in torrents_to_remove {
            match &t.source {
                QbitSourceType::LegacyFile {
                    torrent_path,
                    fastresume_path,
                } => {
                    let mut original_files = Vec::new();
                    let mut backup_files = Vec::new();

                    // Backup .torrent file
                    if torrent_path.is_file() {
                        let fname = torrent_path.file_name().unwrap();
                        let target = backup_dir.join(fname);
                        fs::copy(torrent_path, &target).map_err(|e| {
                            format!(
                                "Failed copying {:?} to backup {:?}: {e}",
                                torrent_path, target
                            )
                        })?;

                        // Verify size
                        let src_len = fs::metadata(torrent_path).map(|m| m.len()).unwrap_or(0);
                        let dst_len = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                        if src_len != dst_len || src_len == 0 {
                            return Err(format!(
                                "Backup verification failed: size mismatch for {:?} (src={}, dst={})",
                                fname, src_len, dst_len
                            ));
                        }

                        original_files.push(torrent_path.to_string_lossy().to_string());
                        backup_files.push(target.to_string_lossy().to_string());
                    }

                    // Backup .fastresume file
                    if fastresume_path.is_file() {
                        let fname = fastresume_path.file_name().unwrap();
                        let target = backup_dir.join(fname);
                        fs::copy(fastresume_path, &target).map_err(|e| {
                            format!(
                                "Failed copying {:?} to backup {:?}: {e}",
                                fastresume_path, target
                            )
                        })?;

                        // Verify size
                        let src_len = fs::metadata(fastresume_path).map(|m| m.len()).unwrap_or(0);
                        let dst_len = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                        if src_len != dst_len || src_len == 0 {
                            return Err(format!(
                                "Backup verification failed: size mismatch for {:?} (src={}, dst={})",
                                fname, src_len, dst_len
                            ));
                        }

                        original_files.push(fastresume_path.to_string_lossy().to_string());
                        backup_files.push(target.to_string_lossy().to_string());
                    }

                    manifest_entries.push(BackupEntry {
                        info_hash: t.info_hash.clone(),
                        original_files,
                        backup_files,
                        source_type: "Legacy BT_backup".to_string(),
                    });
                }
                QbitSourceType::SqliteRow { info_hash, db_path } => {
                    has_sqlite_entries = true;
                    if sqlite_db_path.is_none() {
                        sqlite_db_path = Some(db_path.clone());
                    }
                    manifest_entries.push(BackupEntry {
                        info_hash: info_hash.clone(),
                        original_files: vec![db_path.to_string_lossy().to_string()],
                        backup_files: vec![backup_dir
                            .join("torrents.db")
                            .to_string_lossy()
                            .to_string()],
                        source_type: "SQLite torrents.db".to_string(),
                    });
                }
            }
        }

        // If any torrent was from SQLite, backup torrents.db and verify integrity
        if has_sqlite_entries {
            if let Some(ref db_path) = sqlite_db_path {
                let target_db = backup_dir.join("torrents.db");
                fs::copy(db_path, &target_db).map_err(|e| {
                    format!(
                        "Failed to copy SQLite database {:?} to backup: {e}",
                        db_path
                    )
                })?;

                // Also copy -wal and -shm if present
                let wal = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
                if wal.is_file() {
                    let _ = fs::copy(&wal, backup_dir.join("torrents.db-wal"));
                }
                let shm = PathBuf::from(format!("{}-shm", db_path.to_string_lossy()));
                if shm.is_file() {
                    let _ = fs::copy(&shm, backup_dir.join("torrents.db-shm"));
                }

                // Verify backup DB integrity
                let conn = Connection::open(&target_db).map_err(|e| {
                    format!("Failed opening backup SQLite database for verification: {e}")
                })?;
                let mut stmt = conn
                    .prepare("PRAGMA integrity_check")
                    .map_err(|e| format!("Failed preparing integrity check on backup: {e}"))?;
                let check_result: String = stmt
                    .query_row([], |row| row.get(0))
                    .map_err(|e| format!("Integrity check query failed on backup: {e}"))?;

                if check_result.to_lowercase() != "ok" {
                    return Err(format!(
                        "ABORT: SQLite backup integrity check failed with status: {check_result}"
                    ));
                }
                info!("Verified SQLite backup integrity: ok");
            }
        }

        let instructions = "To restore by hand: For legacy files, copy the .torrent and .fastresume files back into your qBittorrent BT_backup directory while qBittorrent is closed. For SQLite, copy torrents.db back into your qBittorrent data directory while qBittorrent is closed.".to_string();

        let manifest = BackupManifest {
            timestamp,
            backup_folder: backup_dir.to_string_lossy().to_string(),
            qbit_data_dir: qbit_data_dir.to_string_lossy().to_string(),
            entries: manifest_entries,
            manual_restore_instructions: instructions,
        };

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize manifest.json: {e}"))?;
        fs::write(backup_dir.join("manifest.json"), manifest_json.as_bytes())
            .map_err(|e| format!("Failed to write manifest.json: {e}"))?;

        info!(
            "Created and verified backup with {} entries at {:?}",
            manifest.entries.len(),
            backup_dir
        );

        Ok(manifest)
    }

    /// Executes the removal for confirmed-imported torrents
    pub fn execute_removal(
        confirmed_imported_torrents: &[QbitTorrent],
        qbit_data_dir: &Path,
        backup_root: &Path,
        process_checker: &dyn ProcessChecker,
        trash_provider: &dyn TrashProvider,
    ) -> (usize, Vec<TorrentNotRemovedInfo>, Option<String>) {
        if confirmed_imported_torrents.is_empty() {
            return (0, Vec::new(), None);
        }

        // 1. Process check before removal: qBittorrent MUST NOT be running
        if process_checker.is_qbittorrent_running() {
            let mut not_removed = Vec::new();
            for t in confirmed_imported_torrents {
                not_removed.push(TorrentNotRemovedInfo {
                    info_hash: t.info_hash.clone(),
                    name: t.name.clone(),
                    reason: "qBittorrent is running. Removal aborted for safety.".to_string(),
                });
            }
            return (0, not_removed, None);
        }

        // 2. Filter out torrents flagged with missing_path
        let mut eligible_torrents = Vec::new();
        let mut not_removed = Vec::new();

        for t in confirmed_imported_torrents {
            if t.missing_path {
                not_removed.push(TorrentNotRemovedInfo {
                    info_hash: t.info_hash.clone(),
                    name: t.name.clone(),
                    reason: "Download path unavailable; skipped removal for safety.".to_string(),
                });
            } else {
                eligible_torrents.push(t);
            }
        }

        if eligible_torrents.is_empty() {
            return (0, not_removed, None);
        }

        // 3. Create mandatory backup before any removal
        let backup_manifest = match Self::create_and_verify_backup(
            &eligible_torrents
                .iter()
                .map(|&t| t.clone())
                .collect::<Vec<_>>(),
            qbit_data_dir,
            backup_root,
        ) {
            Ok(manifest) => manifest,
            Err(e) => {
                error!(
                    "Backup creation/verification failed: {e}. Aborting all removal operations."
                );
                for t in &eligible_torrents {
                    not_removed.push(TorrentNotRemovedInfo {
                        info_hash: t.info_hash.clone(),
                        name: t.name.clone(),
                        reason: format!("Backup verification failed: {e}. Removal aborted."),
                    });
                }
                return (0, not_removed, None);
            }
        };

        let backup_folder = backup_manifest.backup_folder.clone();
        let mut removed_count = 0;
        let mut sqlite_hashes_to_remove: HashSet<String> = HashSet::new();
        let mut sqlite_db_path: Option<PathBuf> = None;

        // 4. Removal loop for legacy files + collecting SQLite hashes
        for t in &eligible_torrents {
            // Re-check process before each operation
            if process_checker.is_qbittorrent_running() {
                warn!("qBittorrent was launched during removal! Aborting remaining removals.");
                not_removed.push(TorrentNotRemovedInfo {
                    info_hash: t.info_hash.clone(),
                    name: t.name.clone(),
                    reason: "qBittorrent was launched during removal. Operation aborted."
                        .to_string(),
                });
                continue;
            }

            match &t.source {
                QbitSourceType::LegacyFile {
                    torrent_path,
                    fastresume_path,
                } => {
                    let mut file_error = None;

                    // (a) Delete .torrent
                    if torrent_path.exists() {
                        if let Err(e) = Self::validate_path_safety(torrent_path, qbit_data_dir) {
                            file_error = Some(e);
                        } else if let Err(e) = trash_provider.delete_file(torrent_path) {
                            file_error = Some(format!("Failed trashing .torrent: {e}"));
                        }
                    }

                    // (b) Delete .fastresume
                    if file_error.is_none() && fastresume_path.exists() {
                        if let Err(e) = Self::validate_path_safety(fastresume_path, qbit_data_dir) {
                            file_error = Some(e);
                        } else if let Err(e) = trash_provider.delete_file(fastresume_path) {
                            file_error = Some(format!("Failed trashing .fastresume: {e}"));
                        }
                    }

                    if let Some(err) = file_error {
                        not_removed.push(TorrentNotRemovedInfo {
                            info_hash: t.info_hash.clone(),
                            name: t.name.clone(),
                            reason: err,
                        });
                    } else {
                        // Idempotent: if files were already gone or trashed successfully
                        removed_count += 1;
                    }
                }
                QbitSourceType::SqliteRow { info_hash, db_path } => {
                    sqlite_hashes_to_remove.insert(info_hash.to_lowercase());
                    if sqlite_db_path.is_none() {
                        sqlite_db_path = Some(db_path.clone());
                    }
                }
            }
        }

        // 5. Execute SQLite deletions in a single transaction with rollback on error
        if !sqlite_hashes_to_remove.is_empty() {
            if let Some(ref db_path) = sqlite_db_path {
                match Self::execute_sqlite_removal_transaction(db_path, &sqlite_hashes_to_remove) {
                    Ok(count) => {
                        removed_count += count;
                    }
                    Err(e) => {
                        error!("SQLite removal transaction failed: {e}. All SQLite removals rolled back.");
                        for hash in &sqlite_hashes_to_remove {
                            let name = eligible_torrents
                                .iter()
                                .find(|t| t.info_hash.eq_ignore_ascii_case(hash))
                                .map(|t| t.name.clone())
                                .unwrap_or_else(|| hash.clone());
                            not_removed.push(TorrentNotRemovedInfo {
                                info_hash: hash.clone(),
                                name,
                                reason: format!("SQLite transaction rolled back: {e}"),
                            });
                        }
                    }
                }
            }
        }

        (removed_count, not_removed, Some(backup_folder))
    }

    fn execute_sqlite_removal_transaction(
        db_path: &Path,
        hashes_to_delete: &HashSet<String>,
    ) -> Result<usize, String> {
        let mut conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open SQLite database for removal: {e}"))?;

        // Inspect sqlite_master for table and column names
        let table_name: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY CASE WHEN name = 'torrents' THEN 0 ELSE 1 END LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("Could not find target table in torrents.db: {e}"))?;

        // Find id column
        let pragma_sql = format!("PRAGMA table_info(\"{table_name}\")");
        let mut col_stmt = conn
            .prepare(&pragma_sql)
            .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;
        let cols: Vec<String> = col_stmt
            .query_map([], |row| row.get(1))
            .map_err(|e| format!("Failed to read columns: {e}"))?
            .flatten()
            .collect();

        let id_col = cols
            .iter()
            .find(|c| {
                let l = c.to_lowercase();
                l.contains("hash") || l.contains("id")
            })
            .or_else(|| cols.first())
            .ok_or_else(|| format!("Could not find hash/id column in '{table_name}'"))?;

        drop(col_stmt);

        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to start transaction: {e}"))?;

        let mut deleted_count = 0;
        {
            let delete_sql = format!("DELETE FROM \"{table_name}\" WHERE LOWER(\"{id_col}\") = ?1");
            let mut delete_stmt = tx
                .prepare(&delete_sql)
                .map_err(|e| format!("Prepare delete statement failed: {e}"))?;

            for hash in hashes_to_delete {
                let rows_affected = delete_stmt
                    .execute(rusqlite::params![hash.to_lowercase()])
                    .map_err(|e| format!("Delete failed for {hash}: {e}"))?;
                if rows_affected > 0 {
                    deleted_count += 1;
                }
            }
        }

        // Commit transaction
        tx.commit()
            .map_err(|e| format!("Transaction commit failed: {e}"))?;

        // Run integrity check after commit
        let check_res: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|e| format!("PRAGMA integrity_check query failed: {e}"))?;

        if check_res.to_lowercase() != "ok" {
            return Err(format!("Integrity check failed after removal: {check_res}"));
        }

        Ok(deleted_count)
    }

    /// Restores a backup from manifest.json
    pub fn restore_from_manifest(manifest_path: &Path) -> Result<usize, String> {
        let content = fs::read_to_string(manifest_path)
            .map_err(|e| format!("Failed reading manifest: {e}"))?;
        let manifest: BackupManifest =
            serde_json::from_str(&content).map_err(|e| format!("Corrupt manifest.json: {e}"))?;

        let mut restored = 0;
        for entry in &manifest.entries {
            for (idx, b_path) in entry.backup_files.iter().enumerate() {
                if let Some(orig_path) = entry.original_files.get(idx) {
                    if Path::new(b_path).is_file() {
                        let orig = Path::new(orig_path);
                        if let Some(parent) = orig.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        fs::copy(b_path, orig).map_err(|e| {
                            format!("Failed to restore {:?} to {:?}: {e}", b_path, orig)
                        })?;
                        restored += 1;
                    }
                }
            }
        }

        Ok(restored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeTrash {
        trashed_paths: Mutex<Vec<PathBuf>>,
    }

    impl FakeTrash {
        fn new() -> Self {
            Self {
                trashed_paths: Mutex::new(Vec::new()),
            }
        }
    }

    impl TrashProvider for FakeTrash {
        fn delete_file(&self, path: &Path) -> Result<TrashMethod, String> {
            self.trashed_paths.lock().unwrap().push(path.to_path_buf());
            Ok(TrashMethod::MacTrash)
        }
    }

    struct FakeProcessChecker {
        running: bool,
    }

    impl ProcessChecker for FakeProcessChecker {
        fn is_qbittorrent_running(&self) -> bool {
            self.running
        }
    }

    #[test]
    fn safety_guard_accepts_valid_bt_backup_file() {
        let qbit_dir = PathBuf::from("/qbit_data");
        let valid_torrent = qbit_dir
            .join("BT_backup")
            .join("0123456789abcdef0123456789abcdef01234567.torrent");
        assert!(QbitRemover::validate_path_safety(&valid_torrent, &qbit_dir).is_ok());

        let valid_resume = qbit_dir
            .join("BT_backup")
            .join("0123456789abcdef0123456789abcdef01234567.fastresume");
        assert!(QbitRemover::validate_path_safety(&valid_resume, &qbit_dir).is_ok());
    }

    #[test]
    fn safety_guard_strictly_rejects_payload_and_save_paths() {
        let qbit_dir = PathBuf::from("/qbit_data");

        // Payload file
        let payload = PathBuf::from("/downloads/anime/episode01.mkv");
        assert!(QbitRemover::validate_path_safety(&payload, &qbit_dir).is_err());

        // Save path folder
        let save_folder = PathBuf::from("/downloads/anime");
        assert!(QbitRemover::validate_path_safety(&save_folder, &qbit_dir).is_err());

        // File outside BT_backup (e.g. qBittorrent.ini)
        let ini = qbit_dir.join("qBittorrent.ini");
        assert!(QbitRemover::validate_path_safety(&ini, &qbit_dir).is_err());

        // Directory traversal escape
        let traversal = qbit_dir
            .join("BT_backup")
            .join("../../../etc/passwd.torrent");
        assert!(QbitRemover::validate_path_safety(&traversal, &qbit_dir).is_err());

        // Bad extension or name
        let bad_ext = qbit_dir
            .join("BT_backup")
            .join("0123456789abcdef0123456789abcdef01234567.exe");
        assert!(QbitRemover::validate_path_safety(&bad_ext, &qbit_dir).is_err());
    }

    #[test]
    fn removal_aborts_when_qbittorrent_is_running() {
        let qbit_dir = PathBuf::from("/qbit_data");
        let backup_root = PathBuf::from("/nebula/backups");
        let proc = FakeProcessChecker { running: true };
        let trash = FakeTrash::new();

        let t = QbitTorrent {
            info_hash: "0123456789abcdef0123456789abcdef01234567".to_string(),
            name: "Test".to_string(),
            save_path: "/downloads".to_string(),
            download_path: None,
            category: None,
            tags: Vec::new(),
            content_layout: None,
            paused: false,
            auto_managed: true,
            sequential_download: false,
            first_last_piece_prio: false,
            ratio_limit: -1.0,
            seeding_time_limit: None,
            upload_limit_kib: 0,
            download_limit_kib: 0,
            added_time: 1000,
            completed_time: None,
            total_size: 100,
            progress: 1.0,
            state_string: "Seeding".to_string(),
            trackers: Vec::new(),
            mapped_files: Vec::new(),
            file_priorities: Vec::new(),
            raw_torrent_bytes: vec![1, 2, 3],
            source: QbitSourceType::LegacyFile {
                torrent_path: qbit_dir
                    .join("BT_backup")
                    .join("0123456789abcdef0123456789abcdef01234567.torrent"),
                fastresume_path: qbit_dir
                    .join("BT_backup")
                    .join("0123456789abcdef0123456789abcdef01234567.fastresume"),
            },
            already_in_nebula: false,
            missing_path: false,
        };

        let (removed, not_removed, backup) =
            QbitRemover::execute_removal(&[t], &qbit_dir, &backup_root, &proc, &trash);

        assert_eq!(removed, 0);
        assert_eq!(not_removed.len(), 1);
        assert!(not_removed[0].reason.contains("running"));
        assert!(backup.is_none());
        assert!(trash.trashed_paths.lock().unwrap().is_empty());
    }

    #[test]
    fn sqlite_transaction_deletes_only_target_rows_and_rolls_back_on_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("qbit_remover_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("torrents.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE torrents (info_hash TEXT PRIMARY KEY, metadata BLOB, resume_data BLOB)",
                [],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO torrents (info_hash, metadata, resume_data) VALUES (?1, ?2, ?3)",
                rusqlite::params!["hash1", b"meta1".to_vec(), b"res1".to_vec()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO torrents (info_hash, metadata, resume_data) VALUES (?1, ?2, ?3)",
                rusqlite::params!["hash2", b"meta2".to_vec(), b"res2".to_vec()],
            )
            .unwrap();
        }

        let mut set = HashSet::new();
        set.insert("hash1".to_string());

        let count = QbitRemover::execute_sqlite_removal_transaction(&db_path, &set).unwrap();
        assert_eq!(count, 1);

        {
            let conn = Connection::open(&db_path).unwrap();
            let mut stmt = conn.prepare("SELECT info_hash FROM torrents").unwrap();
            let remaining: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert_eq!(remaining, vec!["hash2".to_string()]);
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

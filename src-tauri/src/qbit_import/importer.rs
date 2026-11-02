use super::mapper::{ContentLayout, QbitMapper};
use super::types::{
    ImportOptions, ImportSummaryReport, QbitTorrent, TorrentFailInfo, TorrentSkipInfo,
};
use crate::db::{Database, TorrentDbMetadata};
use crate::engine::SharedEngine;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

pub struct QbitImporter;

impl QbitImporter {
    pub fn execute_import(
        selected_torrents: Vec<QbitTorrent>,
        options: ImportOptions,
        qbit_data_dir: &Path,
        backup_root: &Path,
        engine: SharedEngine,
        db: Arc<Database>,
        logs_dir: &Path,
    ) -> (ImportSummaryReport, Vec<QbitTorrent>) {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let log_file_name = format!("qbit_import_{timestamp}.log");
        let log_path = logs_dir.join(&log_file_name);

        let mut log_lines = Vec::new();
        log_lines.push(format!(
            "=== qBittorrent Import Session started at {timestamp} ==="
        ));
        log_lines.push(format!(
            "Total torrents selected: {}",
            selected_torrents.len()
        ));
        log_lines.push(format!(
            "Options: skip_recheck={}, remove_from_qbit={}",
            options.skip_recheck, options.remove_from_qbit
        ));

        let mut imported_torrents = Vec::new();
        let mut confirmed_persisted_torrents = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();
        let mut unsupported_settings = Vec::new();

        // Query existing torrents in Nebula for deduplication
        let existing_hashes: std::collections::HashSet<String> = match engine.list_torrents() {
            Ok(list) => list
                .into_iter()
                .map(|t| t.info_hash.to_lowercase())
                .collect(),
            Err(e) => {
                warn!("Could not fetch existing torrent list from engine: {e}");
                std::collections::HashSet::new()
            }
        };

        for torrent in selected_torrents {
            let h = torrent.info_hash.to_lowercase();

            // 1. Deduplication
            if existing_hashes.contains(&h) {
                log_lines.push(format!(
                    "[SKIP] {} ({}): Already present in Nebula",
                    torrent.name, h
                ));
                skipped.push(TorrentSkipInfo {
                    info_hash: h.clone(),
                    name: torrent.name.clone(),
                    reason: "Already in Nebula".to_string(),
                });
                continue;
            }

            // 2. Cross-OS check
            if let Some(err) = QbitMapper::check_cross_os(&torrent.save_path) {
                log_lines.push(format!("[FAIL] {} ({}): {err}", torrent.name, h));
                failed.push(TorrentFailInfo {
                    info_hash: h.clone(),
                    name: torrent.name.clone(),
                    error: err,
                });
                continue;
            }

            // 3. Raw metainfo validation
            if torrent.raw_torrent_bytes.is_empty() {
                log_lines.push(format!(
                    "[FAIL] {} ({}): Metainfo (.torrent) bytes missing",
                    torrent.name, h
                ));
                failed.push(TorrentFailInfo {
                    info_hash: h.clone(),
                    name: torrent.name.clone(),
                    error: "Missing torrent metainfo bytes".to_string(),
                });
                continue;
            }

            // 4. Resolve download directory
            let layout = ContentLayout::parse(torrent.content_layout.as_deref());
            let is_multi = !torrent.mapped_files.is_empty() || torrent.total_size > 0;
            let eff_download_dir = QbitMapper::compute_effective_download_dir(
                &torrent.save_path,
                &torrent.name,
                is_multi,
                &layout,
            );
            let download_dir_str = eff_download_dir.to_string_lossy().to_string();

            // 5. Add to engine in PAUSED state
            let add_res = engine
                .add_torrent_from_bytes_paused(&torrent.raw_torrent_bytes, Some(&download_dir_str));

            let added_hash = match add_res {
                Ok(hash) => hash,
                Err(e) => {
                    log_lines.push(format!(
                        "[FAIL] {} ({}): Engine add failed: {e}",
                        torrent.name, h
                    ));
                    failed.push(TorrentFailInfo {
                        info_hash: h.clone(),
                        name: torrent.name.clone(),
                        error: format!("Engine add failed: {e}"),
                    });
                    continue;
                }
            };

            // Ensure torrent is paused initially
            let _ = engine.pause_torrent(&added_hash);

            // 6. Apply mapped_files renames (root folder, nested folders, files) before hash check
            if !torrent.mapped_files.is_empty() {
                if let Some((orig_name, orig_paths)) =
                    crate::qbit_import::bencode::BVal::extract_torrent_files_and_name(
                        &torrent.raw_torrent_bytes,
                    )
                {
                    let rename_ops = QbitMapper::compute_rename_operations(
                        &orig_name,
                        &orig_paths,
                        &torrent.mapped_files,
                        &layout,
                    );
                    for op in rename_ops {
                        if op.is_folder {
                            if let Err(e) = engine.rename_torrent_folder(
                                &added_hash,
                                &op.old_path,
                                &op.new_name,
                            ) {
                                debug!(
                                    "Folder rename {:?} -> {:?} in {}: {}",
                                    op.old_path, op.new_name, added_hash, e
                                );
                            }
                        } else {
                            let idx = op.file_index.unwrap_or(0);
                            if let Err(e) =
                                engine.rename_torrent_file(&added_hash, idx, &op.new_name)
                            {
                                debug!(
                                    "File rename index {} to {:?} in {}: {}",
                                    idx, op.new_name, added_hash, e
                                );
                            }
                        }
                    }
                }
            }

            // 7. Apply per-file priorities
            for (idx, prio) in torrent.file_priorities.iter().enumerate() {
                let mapped = QbitMapper::map_file_priority(*prio);
                let _ = engine.set_file_priority(&added_hash, idx as u32, mapped);
            }

            // 8. Apply speed and ratio limits
            let _ = engine.set_torrent_limits(
                &added_hash,
                torrent.download_limit_kib,
                torrent.upload_limit_kib,
                torrent.ratio_limit,
            );

            // 9. Apply sequential download flag
            if torrent.sequential_download {
                let _ = engine.toggle_sequential_download(&added_hash);
            }

            // 10. Category & Labels
            let mut labels = torrent.tags.clone();
            if !labels.iter().any(|t| t == "Imported from qBittorrent") {
                labels.push("Imported from qBittorrent".to_string());
            }
            let labels_csv = labels.join(", ");

            if let Some(ref cat) = torrent.category {
                let _ = engine.set_category(&added_hash, cat);
            }

            // Record unsupported settings for report visibility
            if torrent.seeding_time_limit.is_some()
                && !unsupported_settings
                    .iter()
                    .any(|s: &String| s.contains("seedingTimeLimit"))
            {
                unsupported_settings.push(
                    "qBt-seedingTimeLimit: Seeding time limits not enforced by engine".to_string(),
                );
            }
            if torrent.auto_managed
                && !unsupported_settings
                    .iter()
                    .any(|s: &String| s.contains("auto_managed"))
            {
                unsupported_settings.push(
                    "auto_managed: Per-torrent automatic management override not supported (global setting applies)".to_string(),
                );
            }

            // 11. Persist to SQLite torrent_metadata
            let initial_last_checked = if options.skip_recheck {
                torrent.completed_time.or(Some(torrent.added_time))
            } else {
                None
            };

            let canonical_hash = torrent.info_hash.to_lowercase();
            let db_meta = TorrentDbMetadata {
                info_hash: canonical_hash.clone(),
                display_title: Some(torrent.name.clone()),
                category: torrent.category.clone(),
                labels: Some(labels_csv),
                is_watched: false,
                notes: Some("Imported from qBittorrent".to_string()),
                added_at: torrent.added_time,
                completed_on: torrent.completed_time,
                save_path: Some(torrent.save_path.clone()),
                download_limit_kib: torrent.download_limit_kib,
                upload_limit_kib: torrent.upload_limit_kib,
                ratio_limit: torrent.ratio_limit,
                sequential_download: torrent.sequential_download,
                first_last_piece_prio: torrent.first_last_piece_prio,
                super_seeding: false,
                racing_override: None,
                last_checked: initial_last_checked,
            };

            if let Err(e) = db.upsert_torrent_metadata(&db_meta) {
                warn!("Failed to persist metadata for imported torrent {canonical_hash}: {e}");
            }
            if added_hash.to_lowercase() != canonical_hash {
                let mut alt_meta = db_meta.clone();
                alt_meta.info_hash = added_hash.clone();
                let _ = db.upsert_torrent_metadata(&alt_meta);
            }

            // Verify persistence by reading back from DB (durable persistence guard)
            let persisted = db
                .get_torrent_metadata(&canonical_hash)
                .ok()
                .flatten()
                .or_else(|| db.get_torrent_metadata(&added_hash).ok().flatten())
                .is_some();
            if !persisted {
                log_lines.push(format!(
                    "[WARN] {} ({}): Metadata persistence verification returned empty",
                    torrent.name, canonical_hash
                ));
            } else {
                confirmed_persisted_torrents.push(torrent.clone());
            }

            log_lines.push(format!(
                "[OK] Imported {} ({}) -> save_path: {}",
                torrent.name, added_hash, torrent.save_path
            ));
            imported_torrents.push(torrent);
        }

        // 12. Optional removal phase (R4b & Section 5b)
        // Must run only on verified persisted torrents before auto-resume
        let (removed_count, not_removed, backup_dir) =
            if options.remove_from_qbit && !confirmed_persisted_torrents.is_empty() {
                let proc_checker = super::remover::SystemProcessChecker;
                let trash_provider = super::remover::DefaultTrashProvider;
                let (rc, nr, bd) = super::remover::QbitRemover::execute_removal(
                    &confirmed_persisted_torrents,
                    qbit_data_dir,
                    backup_root,
                    &proc_checker,
                    &trash_provider,
                );
                log_lines.push(format!(
                    "Removal phase: {} removed, {} not removed. Backup: {:?}",
                    rc,
                    nr.len(),
                    bd
                ));
                for nr_item in &nr {
                    log_lines.push(format!(
                        "[NOT REMOVED] {} ({}): {}",
                        nr_item.name, nr_item.info_hash, nr_item.reason
                    ));
                }
                (rc, nr, bd)
            } else {
                (0, Vec::new(), None)
            };

        // Determine safe resume flags:
        // If removal was requested for a torrent, but removal FAILED for it,
        // DO NOT automatically resume! Keep paused to prevent dual-client collisions.
        let failed_removal_hashes: std::collections::HashSet<String> = not_removed
            .iter()
            .map(|nr| nr.info_hash.to_lowercase())
            .collect();

        // 13. Schedule Recheck Queue (concurrency = 2) if skip_recheck is false
        if !options.skip_recheck && !imported_torrents.is_empty() {
            let recheck_items: Vec<(String, bool)> = imported_torrents
                .iter()
                .map(|t| {
                    let hash_lower = t.info_hash.to_lowercase();
                    let was_running = !t.paused;
                    let removal_failed =
                        options.remove_from_qbit && failed_removal_hashes.contains(&hash_lower);
                    let should_resume = was_running && !removal_failed;

                    if was_running && removal_failed {
                        log_lines.push(format!(
                            "[SAFETY] Withholding auto-resume for '{}' ({}): qBittorrent removal failed. Kept paused.",
                            t.name, t.info_hash
                        ));
                    }

                    (t.info_hash.clone(), should_resume)
                })
                .collect();

            let engine_clone = engine.clone();
            let db_clone = db.clone();

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    run_recheck_queue(recheck_items, engine_clone, db_clone).await;
                });
            } else {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(run_recheck_queue(recheck_items, engine_clone, db_clone));
                });
            }
        } else if options.skip_recheck {
            // If skipping recheck, resume torrents that were running in qBittorrent (except failed removal)
            for t in &imported_torrents {
                let hash_lower = t.info_hash.to_lowercase();
                let was_running = !t.paused;
                let removal_failed =
                    options.remove_from_qbit && failed_removal_hashes.contains(&hash_lower);
                if was_running && !removal_failed {
                    let _ = engine.resume_torrent(&t.info_hash);
                } else if was_running && removal_failed {
                    log_lines.push(format!(
                        "[SAFETY] Withholding auto-resume for '{}' ({}): qBittorrent removal failed. Kept paused.",
                        t.name, t.info_hash
                    ));
                }
            }
        }

        log_lines.push(format!(
            "Import session finished: {} imported, {} skipped, {} failed, {} removed.",
            imported_torrents.len(),
            skipped.len(),
            failed.len(),
            removed_count
        ));

        // Flush log file to disk
        let full_log = log_lines.join("\n");
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
        {
            let _ = f.write_all(full_log.as_bytes());
        }

        let summary = ImportSummaryReport {
            total_selected: imported_torrents.len() + skipped.len() + failed.len(),
            imported_count: imported_torrents.len(),
            skipped,
            failed,
            removed_count,
            not_removed,
            backup_dir,
            unsupported_settings,
            log_file: log_path.to_string_lossy().to_string(),
            log_preview: full_log,
        };

        (summary, imported_torrents)
    }
}

pub async fn run_recheck_queue(
    items: Vec<(String, bool)>,
    engine: SharedEngine,
    db: Arc<Database>,
) {
    info!(
        "Starting qBittorrent import recheck queue for {} torrents (max concurrency: 2)",
        items.len()
    );
    let semaphore = Arc::new(Semaphore::new(2));

    let mut tasks = Vec::new();
    for (hash, should_resume) in items {
        let permit = semaphore.clone().acquire_owned().await;
        let eng = engine.clone();
        let database = db.clone();

        let task = tokio::spawn(async move {
            let _permit = permit;
            info!("[recheck_queue] Triggering force_recheck for {}", hash);
            let force_res = tokio::task::spawn_blocking({
                let eng = eng.clone();
                let hash = hash.clone();
                move || eng.force_recheck(&hash)
            })
            .await
            .unwrap_or_else(|e| Err(format!("spawn_blocking failed: {e}")));

            if let Err(e) = force_res {
                warn!(
                    "[recheck_queue] force_recheck call failed for {}: {}. last_checked will not be stamped.",
                    hash, e
                );
                return;
            }

            // Poll torrent status until checking completes
            let mut saw_checking = false;
            let mut non_checking_ticks = 0;
            for _ in 0..600 {
                let list_res = tokio::task::spawn_blocking({
                    let eng = eng.clone();
                    move || eng.list_torrents()
                })
                .await
                .unwrap_or_else(|e| Err(format!("spawn_blocking failed: {e}")));

                if let Ok(list) = list_res {
                    if let Some(t) = list
                        .into_iter()
                        .find(|item| item.info_hash.eq_ignore_ascii_case(&hash))
                    {
                        let is_checking =
                            t.status == "Checking" || t.status == "Queued for checking";
                        if is_checking {
                            saw_checking = true;
                            non_checking_ticks = 0;
                        } else {
                            // Missing download path or fatal error terminates check without timestamping
                            if t.status == "Error" || !Path::new(&t.save_path).exists() {
                                warn!(
                                    "[recheck_queue] Torrent {} ended in error/missing state. last_checked not stamped.",
                                    hash
                                );
                                break;
                            }

                            // If we observed checking in progress, then exiting checking indicates completion.
                            // If checking has not yet been observed (engine delay or very fast small check),
                            // require a minimum grace window (4 ticks = 200ms) of stable non-checking before
                            // concluding verification has completed.
                            if saw_checking {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                let _ = database.set_torrent_last_checked(&hash, now);
                                info!(
                                    "[recheck_queue] Verification completed for {} (progress={:.1}%)",
                                    hash,
                                    t.progress * 100.0
                                );

                                if should_resume && t.progress >= 1.0 {
                                    info!(
                                        "[recheck_queue] Resuming 100% verified active torrent {}",
                                        hash
                                    );
                                    let _ = tokio::task::spawn_blocking({
                                        let eng = eng.clone();
                                        let hash = hash.clone();
                                        move || eng.resume_torrent(&hash)
                                    })
                                    .await;
                                }
                                break;
                            } else {
                                non_checking_ticks += 1;
                                if non_checking_ticks >= 4 {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0);
                                    let _ = database.set_torrent_last_checked(&hash, now);
                                    info!(
                                        "[recheck_queue] Verification completed (immediate check window) for {} (progress={:.1}%)",
                                        hash,
                                        t.progress * 100.0
                                    );

                                    if should_resume && t.progress >= 1.0 {
                                        info!(
                                            "[recheck_queue] Resuming 100% verified active torrent {}",
                                            hash
                                        );
                                        let _ = tokio::task::spawn_blocking({
                                            let eng = eng.clone();
                                            let hash = hash.clone();
                                            move || eng.resume_torrent(&hash)
                                        })
                                        .await;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });
        tasks.push(task);
    }

    for t in tasks {
        let _ = t.await;
    }
    info!("Recheck queue completed for all imported torrents.");
}

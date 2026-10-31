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
    pub async fn execute_import(
        selected_torrents: Vec<QbitTorrent>,
        options: ImportOptions,
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
        let mut skipped = Vec::new();
        let mut failed = Vec::new();

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
            let add_res =
                engine.add_torrent_from_bytes(&torrent.raw_torrent_bytes, Some(&download_dir_str));

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

            // 6. Apply mapped_files renames before hash check
            for (file_idx, renamed_path) in torrent.mapped_files.iter().enumerate() {
                if let Some(new_name) = Path::new(renamed_path).file_name().and_then(|n| n.to_str())
                {
                    if let Err(e) =
                        engine.rename_torrent_file(&added_hash, file_idx as u32, new_name)
                    {
                        debug!(
                            "Rename file {} in {} to {} skipped/failed: {}",
                            file_idx, added_hash, new_name, e
                        );
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

            // 11. Persist to SQLite torrent_metadata
            let initial_last_checked = if options.skip_recheck {
                torrent.completed_time.or(Some(torrent.added_time))
            } else {
                None
            };

            let db_meta = TorrentDbMetadata {
                info_hash: added_hash.clone(),
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
                warn!("Failed to persist metadata for imported torrent {added_hash}: {e}");
            }

            // Verify persistence by reading back from DB (as required by section 5b step 4)
            let persisted = db
                .get_torrent_metadata(&added_hash)
                .ok()
                .flatten()
                .is_some();
            if !persisted {
                log_lines.push(format!(
                    "[WARN] {} ({}): Metadata persistence verification returned empty",
                    torrent.name, added_hash
                ));
            }

            log_lines.push(format!(
                "[OK] Imported {} ({}) -> save_path: {}",
                torrent.name, added_hash, torrent.save_path
            ));
            imported_torrents.push(torrent);
        }

        // 12. Schedule Recheck Queue (concurrency = 2) if skip_recheck is false
        if !options.skip_recheck && !imported_torrents.is_empty() {
            let recheck_items: Vec<(String, bool)> = imported_torrents
                .iter()
                .map(|t| (t.info_hash.clone(), !t.paused))
                .collect();

            let engine_clone = engine.clone();
            let db_clone = db.clone();

            tokio::spawn(async move {
                run_recheck_queue(recheck_items, engine_clone, db_clone).await;
            });
        } else if options.skip_recheck {
            // If skipping recheck, resume torrents that were running in qBittorrent
            for t in &imported_torrents {
                if !t.paused {
                    let _ = engine.resume_torrent(&t.info_hash);
                }
            }
        }

        log_lines.push(format!(
            "Import phase complete: {} imported, {} skipped, {} failed.",
            imported_torrents.len(),
            skipped.len(),
            failed.len()
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
            removed_count: 0,
            not_removed: Vec::new(),
            backup_dir: None,
            log_file: log_path.to_string_lossy().to_string(),
            log_preview: full_log,
        };

        (summary, imported_torrents)
    }
}

async fn run_recheck_queue(items: Vec<(String, bool)>, engine: SharedEngine, db: Arc<Database>) {
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
            if let Err(e) = eng.force_recheck(&hash) {
                warn!(
                    "[recheck_queue] force_recheck call failed for {}: {}",
                    hash, e
                );
                return;
            }

            // Poll torrent status until checking completes
            let mut checked_once = false;
            for _ in 0..600 {
                // Up to 20 minutes per check
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                if let Ok(list) = eng.list_torrents() {
                    if let Some(t) = list
                        .into_iter()
                        .find(|item| item.info_hash.eq_ignore_ascii_case(&hash))
                    {
                        let is_checking =
                            t.status == "Checking" || t.status == "Queued for checking";
                        if is_checking {
                            checked_once = true;
                        } else if checked_once || t.progress >= 1.0 {
                            // Verification complete
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
                                let _ = eng.resume_torrent(&hash);
                            }
                            break;
                        }
                    }
                }
            }
        });
        tasks.push(task);
    }

    for t in tasks {
        let _ = t.await;
    }
    info!("Recheck queue completed for all imported torrents.");
}

use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::tracker::SeedTimeRecord;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// Number of consecutive 5-second missed polls before a torrent is considered
/// confirmed-removed.  3 misses ≈ 15 seconds — well inside the margin of a
/// normal client restart.
const MISS_THRESHOLD: i64 = 3;

pub fn spawn_seeding_rules_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            // `list_torrents` returns Err on connection/auth failure — skip
            // the whole cycle.  Ok(vec![]) is a valid successful response
            // meaning the client has no torrents; we MUST process it so that
            // the last remaining torrent can be reclassified when removed.
            let torrents = match engine.list_torrents() {
                Ok(t) => t,
                Err(e) => {
                    tracing::debug!("[seeding] list_torrents error (skipping cycle): {e}");
                    continue;
                }
            };

            // ── Active-torrent processing ─────────────────────────────────────
            let mut active_hashes: HashSet<String> = HashSet::new();

            for item in &torrents {
                let lower_hash = item.info_hash.to_lowercase();
                active_hashes.insert(lower_hash.clone());

                // F5: reset miss counter — the torrent is confirmed present.
                let _ = db.reset_sync_miss_count(&item.info_hash);
                let _ = db.reset_sync_miss_count(&lower_hash);

                // F10: upsert a durable snatched record for every completed
                // torrent so it survives removal from the client.
                if item.completed_on.is_some() || item.status == "Seeding" {
                    let now_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let snatched_at = item.completed_on.map(|c| c as i64).unwrap_or(now_secs);
                    let _ = db.upsert_snatched_record(
                        &item.info_hash,
                        &item.name,
                        item.total_bytes as i64,
                        snatched_at,
                        Some(item.save_path.as_str()),
                        None, // display_title — set by metadata import
                        None, // poster_path — set by metadata import
                        None, // provider
                        None, // provider_id
                    );
                }

                // Seeding rules (pre-existing logic)
                if item.status == "Seeding" {
                    let required_hours = 72u32;
                    let existing_opt = db.get_seed_time_record(&item.info_hash).ok().flatten();
                    let mut record = existing_opt.unwrap_or_else(|| {
                        SeedTimeRecord::new(
                            item.info_hash.clone(),
                            item.name.clone(),
                            required_hours,
                        )
                    });

                    record.seeding_time_seconds += 5;
                    if record.seeding_time_seconds >= record.required_seeding_seconds {
                        record.is_shield_satisfied = true;
                    }

                    let _ = db.upsert_seed_time_record(&record);

                    // Auto-pause if ratio limit is set and satisfied
                    if item.ratio_limit > 0.0 && item.ratio >= item.ratio_limit {
                        let _ = engine.pause_torrent(&item.id);
                    }
                }
            }

            // ── Removed-torrent detection (F5) ────────────────────────────────
            //
            // We always run this block — `list_torrents` returning Ok([]) is a
            // legitimate "client has no torrents" state, not an outage.
            // Outages are signalled by Err (handled above with `continue`).
            //
            // Safety gate: only increment miss counts for entries that have
            // `completed_on IS NOT NULL`, meaning they were fully downloaded at
            // least once.  Entries that are still purely in-progress (e.g. a
            // downloading torrent that the user cancelled before completion)
            // never accumulate misses and are never auto-reclassified.
            if let Ok(all_meta) = db.list_all_torrent_metadata() {
                for (hash, meta) in &all_meta {
                    let lower_hash = hash.to_lowercase();
                    // Skip entries already reclassified as External.
                    if meta.is_external {
                        // If the torrent is re-added, reconcile: clear External
                        // flag so it reappears in the Downloaded shelf.
                        if active_hashes.contains(hash) || active_hashes.contains(&lower_hash) {
                            tracing::info!("[seeding] {hash} re-added — clearing external flag");
                            let _ = db.clear_external_flag(hash);
                            let _ = db.clear_external_flag(&lower_hash);
                        }
                        continue;
                    }

                    if active_hashes.contains(hash) || active_hashes.contains(&lower_hash) {
                        // Present — miss counter already reset above.
                        continue;
                    }

                    // Reclassify when completed or when media exists on disk.
                    let files_exist = meta
                        .save_path
                        .as_deref()
                        .map(|p| std::path::Path::new(p).exists())
                        .unwrap_or(false);
                    if meta.completed_on.is_none() && !files_exist {
                        continue;
                    }

                    // Absent from client — increment miss counter.
                    let count = db.increment_sync_miss_count(hash).unwrap_or(0);
                    tracing::debug!(
                        "[seeding] {hash} absent from client ({count}/{MISS_THRESHOLD})"
                    );

                    if count >= MISS_THRESHOLD {
                        tracing::info!(
                            "[seeding] {hash} confirmed removed after {MISS_THRESHOLD} misses — marking external"
                        );
                        let _ = db.mark_as_external(hash);
                        let _ = db.mark_snatched_record_external(hash);
                        // The JSON sidecar (external_media.json) is not updated
                        // here because seeding.rs has no AppHandle.  The Tauri
                        // command `list_external_media` now merges the DB
                        // is_external rows with the sidecar so the frontend
                        // always sees a unified list.
                    }
                }
            }
        }
    });
}

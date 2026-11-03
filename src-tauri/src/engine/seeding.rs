use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::tracker::SeedTimeRecord;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// Number of consecutive 5-second missed polls before a torrent is considered
/// confirmed-removed.  3 misses ≈ 15 seconds — well inside the margin of a
/// normal client restart, so this is conservative.  The threshold avoids
/// false-positive reclassification during transient client outages.
const MISS_THRESHOLD: i64 = 3;

pub fn spawn_seeding_rules_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            let torrents = match engine.list_torrents() {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Guard: an empty list from the client most likely means a
            // connection failure, not that every torrent was removed.
            // Only do miss-count processing when the list is non-empty.
            let client_reported_results = !torrents.is_empty();

            // ── Active-torrent processing ─────────────────────────────────────
            let mut active_hashes: HashSet<String> = HashSet::new();

            for item in &torrents {
                active_hashes.insert(item.info_hash.clone());

                // F5: reset miss counter — the torrent is confirmed present.
                let _ = db.reset_sync_miss_count(&item.info_hash);

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
            // Only run when the client gave us a real (non-empty) response.
            if client_reported_results {
                if let Ok(all_meta) = db.list_all_torrent_metadata() {
                    for (hash, meta) in &all_meta {
                        // Skip entries already reclassified as External.
                        if meta.is_external {
                            // If the torrent is re-added, clear the external flag.
                            if active_hashes.contains(hash) {
                                let _ = db.clear_external_flag(hash);
                            }
                            continue;
                        }

                        if active_hashes.contains(hash) {
                            // Present — already reset above.
                            continue;
                        }

                        // Absent — increment miss counter.
                        let count = db.increment_sync_miss_count(hash).unwrap_or(0);
                        tracing::debug!(
                            "[seeding] {hash} absent from client ({count}/{MISS_THRESHOLD})"
                        );

                        if count >= MISS_THRESHOLD {
                            tracing::info!(
                                "[seeding] {hash} confirmed removed — marking is_external"
                            );
                            let _ = db.mark_as_external(hash);
                            let _ = db.mark_snatched_record_external(hash);
                            // Note: The JSON external_media sidecar is not updated
                            // here because `seeding.rs` does not hold an AppHandle.
                            // The frontend will see `is_external = true` via the
                            // next `list_all_torrent_metadata` call and can show the
                            // entry in the External section.  A dedicated
                            // `list_external_from_db` Tauri command exposes this.
                        }
                    }
                }
            }
        }
    });
}

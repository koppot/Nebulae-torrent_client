use crate::db::Database;
use crate::engine::SharedEngine;
use crate::models::tracker::SeedTimeRecord;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn_seeding_rules_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            let torrents = match engine.list_torrents() {
                Ok(t) => t,
                Err(_) => continue,
            };

            for item in torrents {
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
        }
    });
}

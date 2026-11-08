pub mod schema;

use crate::models::tracker::SeedTimeRecord;
use crate::models::vpn::{VpnProfile, VpnProtocol};
use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Configure WAL mode and crash safety
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
        ",
        )?;

        // Execute table initialization
        conn.execute(schema::CREATE_SETTINGS_TABLE, [])?;
        conn.execute(schema::CREATE_TORRENT_METADATA_TABLE, [])?;
        conn.execute(schema::CREATE_VPN_PROFILES_TABLE, [])?;
        conn.execute(schema::CREATE_SEED_TIME_TABLE, [])?;
        conn.execute(schema::CREATE_SNATCHED_RECORDS_TABLE, [])?;

        // Backward compatibility migration for torrent_metadata
        let extra_cols = [
            ("completed_on", "INTEGER"),
            ("save_path", "TEXT"),
            ("download_limit_kib", "INTEGER DEFAULT 0"),
            ("upload_limit_kib", "INTEGER DEFAULT 0"),
            ("ratio_limit", "REAL DEFAULT -1.0"),
            ("sequential_download", "INTEGER DEFAULT 0"),
            ("first_last_piece_prio", "INTEGER DEFAULT 0"),
            ("super_seeding", "INTEGER DEFAULT 0"),
            // Per-torrent Racing Mode choice: NULL = follow the global preset,
            // 1 = pinned on, 0 = pinned off.
            ("racing_override", "INTEGER"),
            // Feature 6: user-defined listing name (authoritative over provider title)
            ("user_display_title", "TEXT"),
            ("user_title_locked", "INTEGER DEFAULT 0"),
            // Feature 3: manual poster ordering
            ("sort_order", "INTEGER"),
            ("poster_path", "TEXT"),
            // Feature 8/9: metadata provider attribution
            ("provider", "TEXT"),
            ("provider_id", "TEXT"),
            // Feature 5: external reclassification tracking
            ("is_external", "INTEGER DEFAULT 0"),
            ("external_moved_at", "INTEGER"),
            ("sync_miss_count", "INTEGER DEFAULT 0"),
            ("last_checked", "INTEGER"),
        ];
        for (col, col_type) in extra_cols {
            let sql = format!("ALTER TABLE torrent_metadata ADD COLUMN {col} {col_type}");
            let _ = conn.execute(&sql, []);
        }

        // Cleanup contaminated Fuuka poster mappings in settings (non-Fuuka entries pointing to contaminated Fuuka hash or fuuka artwork)
        let _ = conn.execute(
            "DELETE FROM settings
             WHERE (key LIKE 'tmdb_poster:%' OR key LIKE 'mal_poster:%')
               AND (value LIKE '%93758aeca10f8450%' OR LOWER(value) LIKE '%fuuka%')
               AND LOWER(key) NOT LIKE '%fuuka%'",
            [],
        );

        // Reset any contaminated poster_path in torrent_metadata so it safely re-resolves genuine artwork
        let _ = conn.execute(
            "UPDATE torrent_metadata
             SET poster_path = NULL
             WHERE (poster_path LIKE '%93758aeca10f8450%' OR LOWER(poster_path) LIKE '%fuuka%')
               AND COALESCE(user_title_locked, 0) = 0
               AND LOWER(TRIM(COALESCE(user_display_title, display_title, ''))) NOT LIKE '%fuuka%'",
            [],
        );

        // Idempotent backfill of torrent_metadata.poster_path from settings cache
        // when poster_path is NULL or empty, preserving existing selections.
        let _ = conn.execute(
            "UPDATE torrent_metadata
             SET poster_path = (
                 SELECT s.value FROM settings s
                 WHERE (s.key = ('tmdb_poster:v3:' || LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title))))
                    OR s.key = ('tmdb_poster:v2:' || LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title)))))
                   AND (
                       (s.value NOT LIKE '%93758aeca10f8450%' AND LOWER(s.value) NOT LIKE '%fuuka%')
                       OR LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title))) LIKE '%fuuka%'
                   )
                 LIMIT 1
             )
             WHERE (poster_path IS NULL OR poster_path = '')
               AND COALESCE(user_title_locked, 0) = 0
               AND EXISTS (
                 SELECT 1 FROM settings s
                 WHERE (s.key = ('tmdb_poster:v3:' || LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title))))
                    OR s.key = ('tmdb_poster:v2:' || LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title)))))
                   AND (
                       (s.value NOT LIKE '%93758aeca10f8450%' AND LOWER(s.value) NOT LIKE '%fuuka%')
                       OR LOWER(TRIM(COALESCE(torrent_metadata.user_display_title, torrent_metadata.display_title))) LIKE '%fuuka%'
                   )
               )",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn reset_all_settings(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM settings", [])?;
        conn.execute("DELETE FROM vpn_profiles", [])?;
        conn.execute("DELETE FROM seed_time_records", [])?;
        Ok(())
    }

    // VPN Profiles CRUD
    pub fn insert_vpn_profile(&self, profile: &VpnProfile) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let protocol_str = match profile.protocol {
            VpnProtocol::WireGuard => "wireguard",
            VpnProtocol::OpenVpn => "openvpn",
        };
        let dns_json = serde_json::to_string(&profile.dns_servers).unwrap_or_default();

        conn.execute(
            "INSERT OR REPLACE INTO vpn_profiles (id, name, protocol, endpoint, interface_address, dns_servers, requires_auth, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                profile.id,
                profile.name,
                protocol_str,
                profile.endpoint,
                profile.interface_address,
                dns_json,
                if profile.requires_auth { 1 } else { 0 },
                profile.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_vpn_profiles(&self) -> Result<Vec<VpnProfile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, protocol, endpoint, interface_address, dns_servers, requires_auth, created_at FROM vpn_profiles ORDER BY created_at DESC")?;

        let profile_iter = stmt.query_map([], |row| {
            let protocol_str: String = row.get(2)?;
            let dns_json: String = row.get(5)?;
            let requires_auth_int: i32 = row.get(6)?;

            let protocol = if protocol_str == "openvpn" {
                VpnProtocol::OpenVpn
            } else {
                VpnProtocol::WireGuard
            };

            let dns_servers: Vec<String> = serde_json::from_str(&dns_json).unwrap_or_default();

            Ok(VpnProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                protocol,
                endpoint: row.get(3)?,
                interface_address: row.get(4)?,
                dns_servers,
                requires_auth: requires_auth_int == 1,
                created_at: row.get(7)?,
            })
        })?;

        let list = profile_iter.flatten().collect();
        Ok(list)
    }

    pub fn delete_vpn_profile(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM vpn_profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Seed Time Records
    pub fn upsert_seed_time_record(&self, record: &SeedTimeRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO seed_time_records (info_hash, torrent_name, seeding_time_seconds, required_seeding_seconds, is_shield_satisfied) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.info_hash,
                record.torrent_name,
                record.seeding_time_seconds as i64,
                record.required_seeding_seconds as i64,
                if record.is_shield_satisfied { 1 } else { 0 }
            ],
        )?;
        Ok(())
    }

    pub fn list_seed_time_records(&self) -> Result<Vec<SeedTimeRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT info_hash, torrent_name, seeding_time_seconds, required_seeding_seconds, is_shield_satisfied FROM seed_time_records")?;

        let iter = stmt.query_map([], |row| {
            let seeding_sec: i64 = row.get(2)?;
            let req_sec: i64 = row.get(3)?;
            let satisfied_int: i32 = row.get(4)?;

            Ok(SeedTimeRecord {
                info_hash: row.get(0)?,
                torrent_name: row.get(1)?,
                seeding_time_seconds: seeding_sec.max(0) as u64,
                required_seeding_seconds: req_sec.max(0) as u64,
                is_shield_satisfied: satisfied_int == 1,
            })
        })?;

        let list = iter.flatten().collect();
        Ok(list)
    }

    pub fn get_seed_time_record(&self, info_hash: &str) -> Result<Option<SeedTimeRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT info_hash, torrent_name, seeding_time_seconds, required_seeding_seconds, is_shield_satisfied FROM seed_time_records WHERE info_hash = ?1")?;

        let mut iter = stmt.query_map(params![info_hash], |row| {
            let seeding_sec: i64 = row.get(2)?;
            let req_sec: i64 = row.get(3)?;
            let satisfied_int: i32 = row.get(4)?;

            Ok(SeedTimeRecord {
                info_hash: row.get(0)?,
                torrent_name: row.get(1)?,
                seeding_time_seconds: seeding_sec.max(0) as u64,
                required_seeding_seconds: req_sec.max(0) as u64,
                is_shield_satisfied: satisfied_int == 1,
            })
        })?;

        if let Some(res) = iter.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    // Torrent Metadata CRUD (Phase 1 UI-facing persistence mirroring)
    pub fn upsert_torrent_metadata(&self, meta: &TorrentDbMetadata) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO torrent_metadata (
                info_hash, display_title, category, labels, is_watched, notes, added_at,
                completed_on, save_path, download_limit_kib, upload_limit_kib, ratio_limit,
                sequential_download, first_last_piece_prio, super_seeding, racing_override,
                last_checked
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(info_hash) DO UPDATE SET
                -- Only update display_title when the user has not locked it.
                display_title = CASE WHEN user_title_locked = 1
                    THEN torrent_metadata.display_title
                    ELSE excluded.display_title END,
                category = excluded.category,
                labels = excluded.labels,
                is_watched = excluded.is_watched,
                notes = excluded.notes,
                completed_on = excluded.completed_on,
                save_path = excluded.save_path,
                download_limit_kib = excluded.download_limit_kib,
                upload_limit_kib = excluded.upload_limit_kib,
                ratio_limit = excluded.ratio_limit,
                sequential_download = excluded.sequential_download,
                first_last_piece_prio = excluded.first_last_piece_prio,
                super_seeding = excluded.super_seeding,
                racing_override = excluded.racing_override,
                last_checked = COALESCE(excluded.last_checked, torrent_metadata.last_checked)",
            params![
                meta.info_hash,
                meta.display_title,
                meta.category,
                meta.labels,
                if meta.is_watched { 1 } else { 0 },
                meta.notes,
                meta.added_at as i64,
                meta.completed_on.map(|c| c as i64),
                meta.save_path,
                meta.download_limit_kib as i64,
                meta.upload_limit_kib as i64,
                meta.ratio_limit as f64,
                if meta.sequential_download { 1 } else { 0 },
                if meta.first_last_piece_prio { 1 } else { 0 },
                if meta.super_seeding { 1 } else { 0 },
                meta.racing_override.map(|v| if v { 1i64 } else { 0i64 }),
                meta.last_checked.map(|c| c as i64),
            ],
        )?;
        Ok(())
    }

    pub fn set_torrent_last_checked(&self, info_hash: &str, timestamp: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE torrent_metadata SET last_checked = ?1 WHERE info_hash = ?2",
            params![timestamp as i64, info_hash],
        )?;
        if updated == 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO torrent_metadata (info_hash, added_at, last_checked) VALUES (?1, ?2, ?3)",
                params![info_hash, now, timestamp as i64],
            )?;
        }
        Ok(())
    }

    /// Persist an explicit per-torrent Racing Mode choice: `Some(true)` /
    /// `Some(false)` pin the torrent on / off regardless of the global preset,
    /// `None` returns it to following the global preset.
    ///
    /// Targeted write on purpose: it never touches the other metadata columns.
    /// The readers drop any row whose `added_at` is NULL (and the choice with
    /// it on the next start), so a missing row is created with `added_at`
    /// stamped and an existing row with a NULL `added_at` is repaired.
    pub fn set_torrent_racing_override(&self, info_hash: &str, value: Option<bool>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let v: Option<i64> = value.map(|b| if b { 1 } else { 0 });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let updated = conn.execute(
            "UPDATE torrent_metadata
             SET racing_override = ?1, added_at = COALESCE(added_at, ?2)
             WHERE info_hash = ?3",
            params![v, now, info_hash],
        )?;
        if updated == 0 && v.is_some() {
            conn.execute(
                "INSERT INTO torrent_metadata (info_hash, added_at, racing_override)
                 VALUES (?1, ?2, ?3)",
                params![info_hash, now, v],
            )?;
        }
        Ok(())
    }

    pub fn set_torrent_super_seeding(&self, info_hash: &str, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE torrent_metadata SET super_seeding = ?1 WHERE info_hash = ?2",
            params![if enabled { 1 } else { 0 }, info_hash],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO torrent_metadata (info_hash, super_seeding) VALUES (?1, ?2)",
                params![info_hash, if enabled { 1 } else { 0 }],
            )?;
        }
        Ok(())
    }

    pub fn get_torrent_metadata(&self, info_hash: &str) -> Result<Option<TorrentDbMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT info_hash, display_title, category, labels, is_watched, notes, added_at,
                    completed_on, save_path, download_limit_kib, upload_limit_kib, ratio_limit,
                    sequential_download, first_last_piece_prio, super_seeding, racing_override,
                    user_display_title, user_title_locked, sort_order, poster_path, is_external, sync_miss_count,
                    last_checked
             FROM torrent_metadata WHERE info_hash = ?1",
        )?;
        let mut rows = stmt.query(params![info_hash])?;
        if let Some(row) = rows.next()? {
            let is_watched_int: i32 = row.get(4)?;
            let added_at_int: i64 = row.get(6)?;
            let completed_on_opt: Option<i64> = row.get(7)?;
            let dl_lim: i64 = row.get(9).unwrap_or(0);
            let ul_lim: i64 = row.get(10).unwrap_or(0);
            let ratio_lim: f64 = row.get(11).unwrap_or(-1.0);
            let seq_int: i32 = row.get(12).unwrap_or(0);
            let flp_int: i32 = row.get(13).unwrap_or(0);
            let ss_int: i32 = row.get(14).unwrap_or(0);
            let ro_opt: Option<i64> = row.get(15).unwrap_or(None);
            let utl_int: i32 = row.get(17).unwrap_or(0);
            let is_ext_int: i32 = row.get(20).unwrap_or(0);
            let miss_count: i64 = row.get(21).unwrap_or(0);
            let lc_opt: Option<i64> = row.get(22).unwrap_or(None);

            Ok(Some(TorrentDbMetadata {
                info_hash: row.get(0)?,
                display_title: row.get(1)?,
                category: row.get(2)?,
                labels: row.get(3)?,
                is_watched: is_watched_int == 1,
                notes: row.get(5)?,
                added_at: added_at_int.max(0) as u64,
                completed_on: completed_on_opt.map(|c| c.max(0) as u64),
                save_path: row.get(8)?,
                download_limit_kib: dl_lim.max(0) as u32,
                upload_limit_kib: ul_lim.max(0) as u32,
                ratio_limit: ratio_lim as f32,
                sequential_download: seq_int == 1,
                first_last_piece_prio: flp_int == 1,
                super_seeding: ss_int == 1,
                racing_override: ro_opt.map(|v| v == 1),
                user_display_title: row.get(16)?,
                user_title_locked: utl_int == 1,
                sort_order: row.get(18)?,
                poster_path: row.get(19)?,
                is_external: is_ext_int == 1,
                sync_miss_count: miss_count,
                last_checked: lc_opt.map(|c| c.max(0) as u64),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_all_torrent_metadata(
        &self,
    ) -> Result<std::collections::HashMap<String, TorrentDbMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT info_hash, display_title, category, labels, is_watched, notes, added_at,
                    completed_on, save_path, download_limit_kib, upload_limit_kib, ratio_limit,
                    sequential_download, first_last_piece_prio, super_seeding, racing_override,
                    user_display_title, user_title_locked, sort_order, poster_path, is_external, sync_miss_count,
                    last_checked
             FROM torrent_metadata",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_watched_int: i32 = row.get(4)?;
            let added_at_int: i64 = row.get(6)?;
            let completed_on_opt: Option<i64> = row.get(7)?;
            let dl_lim: i64 = row.get(9).unwrap_or(0);
            let ul_lim: i64 = row.get(10).unwrap_or(0);
            let ratio_lim: f64 = row.get(11).unwrap_or(-1.0);
            let seq_int: i32 = row.get(12).unwrap_or(0);
            let flp_int: i32 = row.get(13).unwrap_or(0);
            let ss_int: i32 = row.get(14).unwrap_or(0);
            let ro_opt: Option<i64> = row.get(15).unwrap_or(None);
            let utl_int: i32 = row.get(17).unwrap_or(0);
            let is_ext_int: i32 = row.get(20).unwrap_or(0);
            let miss_count: i64 = row.get(21).unwrap_or(0);
            let lc_opt: Option<i64> = row.get(22).unwrap_or(None);

            Ok(TorrentDbMetadata {
                info_hash: row.get(0)?,
                display_title: row.get(1)?,
                category: row.get(2)?,
                labels: row.get(3)?,
                is_watched: is_watched_int == 1,
                notes: row.get(5)?,
                added_at: added_at_int.max(0) as u64,
                completed_on: completed_on_opt.map(|c| c.max(0) as u64),
                save_path: row.get(8)?,
                download_limit_kib: dl_lim.max(0) as u32,
                upload_limit_kib: ul_lim.max(0) as u32,
                ratio_limit: ratio_lim as f32,
                sequential_download: seq_int == 1,
                first_last_piece_prio: flp_int == 1,
                super_seeding: ss_int == 1,
                racing_override: ro_opt.map(|v| v == 1),
                user_display_title: row.get(16)?,
                user_title_locked: utl_int == 1,
                sort_order: row.get(18)?,
                poster_path: row.get(19)?,
                is_external: is_ext_int == 1,
                sync_miss_count: miss_count,
                last_checked: lc_opt.map(|c| c.max(0) as u64),
            })
        })?;

        let mut map = std::collections::HashMap::new();
        for item in rows.flatten() {
            map.insert(item.info_hash.clone(), item);
        }
        Ok(map)
    }

    pub fn delete_torrent_metadata(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM torrent_metadata WHERE info_hash = ?1",
            params![info_hash],
        )?;
        Ok(())
    }

    // ── Feature 6: user-defined listing names ────────────────────────────────

    /// Set a user-chosen display name for a torrent entry.
    /// `locked = true` prevents automatic metadata syncs from overwriting it.
    pub fn set_user_display_title(&self, info_hash: &str, title: &str, locked: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let updated = conn.execute(
            "UPDATE torrent_metadata
             SET user_display_title = ?1, user_title_locked = ?2,
                 display_title = ?1,
                 added_at = COALESCE(added_at, ?3)
             WHERE info_hash = ?4",
            params![title, if locked { 1i64 } else { 0i64 }, now, info_hash],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO torrent_metadata
                 (info_hash, display_title, user_display_title, user_title_locked, added_at)
                 VALUES (?1, ?2, ?2, ?3, ?4)",
                params![info_hash, title, if locked { 1i64 } else { 0i64 }, now],
            )?;
        }
        Ok(())
    }

    /// Clear the user-defined title lock, reverting to provider-supplied metadata
    /// on the next sync.  The current display_title is left intact until the
    /// next sync overwrites it naturally.
    pub fn reset_user_display_title(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE torrent_metadata
             SET user_display_title = NULL, user_title_locked = 0
             WHERE info_hash = ?1",
            params![info_hash],
        )?;
        Ok(())
    }

    // ── Feature 3: manual poster ordering ────────────────────────────────────

    /// Persist a custom sort position for a single torrent poster.
    /// `sort_order = NULL` means "newly added / unordered (sort last)".
    pub fn set_torrent_sort_order(&self, info_hash: &str, sort_order: Option<i64>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let updated = conn.execute(
            "UPDATE torrent_metadata
             SET sort_order = ?1, added_at = COALESCE(added_at, ?2)
             WHERE info_hash = ?3",
            params![sort_order, now, info_hash],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO torrent_metadata (info_hash, sort_order, added_at)
                 VALUES (?1, ?2, ?3)",
                params![info_hash, sort_order, now],
            )?;
        }
        Ok(())
    }

    /// Retrieve the custom ordered list of downloaded torrent info_hashes.
    pub fn get_downloaded_media_order(&self) -> Result<Vec<String>> {
        if let Ok(Some(json)) = self.get_setting("media_downloaded_order") {
            if let Ok(hashes) = serde_json::from_str::<Vec<String>>(&json) {
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<String> = hashes
                    .into_iter()
                    .filter(|h| seen.insert(h.to_lowercase()))
                    .collect();
                if !unique.is_empty() {
                    return Ok(unique);
                }
            }
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT info_hash FROM torrent_metadata WHERE sort_order IS NOT NULL ORDER BY sort_order ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut list = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in rows.flatten() {
            if seen.insert(r.to_lowercase()) {
                list.push(r);
            }
        }
        Ok(list)
    }

    /// Retrieve the custom ordered list of external media paths.
    pub fn get_external_media_order(&self) -> Result<Vec<String>> {
        if let Ok(Some(json)) = self.get_setting("media_external_order") {
            if let Ok(paths) = serde_json::from_str::<Vec<String>>(&json) {
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<String> = paths
                    .into_iter()
                    .filter(|p| {
                        let key = p.trim().to_lowercase().replace('\\', "/");
                        seen.insert(key)
                    })
                    .collect();
                return Ok(unique);
            }
        }
        Ok(Vec::new())
    }

    /// Persist the custom ordered list of external media paths.
    pub fn set_external_media_order(&self, paths: &[String]) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<String> = paths
            .iter()
            .filter(|p| {
                let key = p.trim().to_lowercase().replace('\\', "/");
                seen.insert(key)
            })
            .cloned()
            .collect();
        let json = serde_json::to_string(&unique)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.set_setting("media_external_order", &json)
    }

    /// Persist a chosen poster path or URL for a torrent.
    pub fn set_torrent_poster(&self, info_hash: &str, poster_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let updated = conn.execute(
            "UPDATE torrent_metadata
             SET poster_path = ?1, added_at = COALESCE(added_at, ?2)
             WHERE info_hash = ?3",
            params![poster_path, now, info_hash],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO torrent_metadata (info_hash, poster_path, added_at)
                 VALUES (?1, ?2, ?3)",
                params![info_hash, poster_path, now],
            )?;
        }
        // Also update snatched_records if present
        let _ = conn.execute(
            "UPDATE snatched_records SET poster_path = ?1 WHERE info_hash = ?2",
            params![poster_path, info_hash],
        );
        Ok(())
    }

    /// Retrieve the persisted poster path or URL for a torrent.
    /// Checks torrent_metadata first, then snatched_records, and falls back to
    /// resolving from the settings cache by title when poster_path is null.
    pub fn get_torrent_poster(&self, info_hash: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT poster_path FROM torrent_metadata WHERE info_hash = ?1")?;
        let mut rows = stmt.query(params![info_hash])?;
        if let Some(row) = rows.next()? {
            let p: Option<String> = row.get(0)?;
            if let Some(path) = p {
                if !path.trim().is_empty() {
                    return Ok(Some(path));
                }
            }
        }

        // Fallback: check snatched_records
        let mut s_stmt =
            conn.prepare("SELECT poster_path FROM snatched_records WHERE info_hash = ?1")?;
        let mut s_rows = s_stmt.query(params![info_hash])?;
        if let Some(row) = s_rows.next()? {
            let p: Option<String> = row.get(0)?;
            if let Some(path) = p {
                if !path.trim().is_empty() {
                    return Ok(Some(path));
                }
            }
        }

        // Settings cache resolution when poster_path is null:
        // Lookup title from torrent_metadata or snatched_records
        let mut t_stmt = conn.prepare(
            "SELECT COALESCE(user_display_title, display_title) FROM torrent_metadata WHERE info_hash = ?1
             UNION ALL
             SELECT COALESCE(display_title, torrent_name) FROM snatched_records WHERE info_hash = ?1
             LIMIT 1",
        )?;
        let mut t_rows = t_stmt.query(params![info_hash])?;
        if let Some(row) = t_rows.next()? {
            let title_opt: Option<String> = row.get(0)?;
            if let Some(title) = title_opt {
                let trimmed = title.trim();
                if !trimmed.is_empty() {
                    let mut keys = vec![
                        format!("tmdb_poster:v3:{}", trimmed.to_lowercase()),
                        format!("tmdb_poster:v2:{}", trimmed.to_lowercase()),
                    ];
                    let cleaned = trimmed
                        .trim_end_matches(|c: char| c.is_whitespace())
                        .trim_end_matches("OVA")
                        .trim_end_matches("ova")
                        .trim_end_matches(|c: char| c.is_whitespace())
                        .to_string();
                    if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case(trimmed) {
                        keys.push(format!("tmdb_poster:v3:{}", cleaned.to_lowercase()));
                        keys.push(format!("tmdb_poster:v2:{}", cleaned.to_lowercase()));
                    }
                    for k in keys {
                        let mut cache_stmt =
                            conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
                        let mut cache_rows = cache_stmt.query(params![k])?;
                        if let Some(c_row) = cache_rows.next()? {
                            let val: String = c_row.get(0)?;
                            if !val.trim().is_empty() {
                                // Guard against Fuuka poster contamination for non-Fuuka entries
                                if val.contains("93758aeca10f8450")
                                    && !trimmed.to_lowercase().contains("fuuka")
                                {
                                    continue;
                                }
                                return Ok(Some(val));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // ── Feature 5: removed-torrent → External reclassification ───────────────

    /// Increment the consecutive missed-sync counter for a torrent.
    /// Returns the new count so callers can threshold-check inline.
    pub fn increment_sync_miss_count(&self, info_hash: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO torrent_metadata (info_hash, sync_miss_count, added_at)
             VALUES (?1, 1, ?2)
             ON CONFLICT(info_hash) DO UPDATE SET
                 sync_miss_count = COALESCE(sync_miss_count, 0) + 1",
            params![info_hash, now],
        )?;
        let count: i64 = conn.query_row(
            "SELECT sync_miss_count FROM torrent_metadata WHERE info_hash = ?1",
            params![info_hash],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Reset the miss counter after a torrent is seen again (re-added or
    /// recovered from a client outage).
    pub fn reset_sync_miss_count(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE torrent_metadata SET sync_miss_count = 0 WHERE info_hash = ?1",
            params![info_hash],
        )?;
        Ok(())
    }

    /// Mark a torrent entry as reclassified to External.
    pub fn mark_as_external(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "UPDATE torrent_metadata
             SET is_external = 1, external_moved_at = ?1, sync_miss_count = 0,
                 -- Preserve user-defined title when moving to External
                 user_title_locked = CASE WHEN user_display_title IS NOT NULL THEN 1 ELSE user_title_locked END
             WHERE info_hash = ?2",
            params![now, info_hash],
        )?;
        Ok(())
    }

    // ── Feature 10: snatched records persistence ──────────────────────────────

    /// Upsert a snatched record.  Never deletes existing records — only the
    /// user can explicitly remove snatch history.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_snatched_record(
        &self,
        info_hash: &str,
        torrent_name: &str,
        size_bytes: i64,
        snatched_at: i64,
        save_path: Option<&str>,
        display_title: Option<&str>,
        poster_path: Option<&str>,
        provider: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO snatched_records
             (info_hash, torrent_name, size_bytes, snatched_at, save_path,
              display_title, poster_path, provider, provider_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(info_hash) DO UPDATE SET
                 torrent_name   = COALESCE(excluded.torrent_name, snatched_records.torrent_name),
                 size_bytes     = COALESCE(excluded.size_bytes, snatched_records.size_bytes),
                 save_path      = COALESCE(excluded.save_path, snatched_records.save_path),
                 display_title  = COALESCE(excluded.display_title, snatched_records.display_title),
                 poster_path    = COALESCE(excluded.poster_path, snatched_records.poster_path),
                 provider       = COALESCE(excluded.provider, snatched_records.provider),
                 provider_id    = COALESCE(excluded.provider_id, snatched_records.provider_id)",
            params![
                info_hash,
                torrent_name,
                size_bytes,
                snatched_at,
                save_path,
                display_title,
                poster_path,
                provider,
                provider_id,
            ],
        )?;
        Ok(())
    }

    /// Update the external_moved_at timestamp on a snatched record when the
    /// torrent is reclassified to External.
    pub fn mark_snatched_record_external(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "UPDATE snatched_records SET external_moved_at = ?1 WHERE info_hash = ?2",
            params![now, info_hash],
        )?;
        Ok(())
    }

    // ── Feature 8/9: metadata provider attribution ────────────────────────────

    /// Store the provider (tmdb / imdb / mal) and its ID on a torrent entry.
    pub fn set_media_provider(
        &self,
        info_hash: &str,
        provider: &str,
        provider_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE torrent_metadata
             SET provider = ?1, provider_id = ?2
             WHERE info_hash = ?3",
            params![provider, provider_id, info_hash],
        )?;
        Ok(())
    }

    /// Retrieve the display_title stored in snatched_records.
    pub fn get_snatched_display_title(&self, info_hash: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT display_title FROM snatched_records WHERE info_hash = ?1")?;
        let mut rows = stmt.query(params![info_hash])?;
        if let Some(row) = rows.next()? {
            Ok(row.get(0)?)
        } else {
            Ok(None)
        }
    }

    /// Clear the `is_external` flag when a torrent is re-added to the client.
    pub fn clear_external_flag(&self, info_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE torrent_metadata
             SET is_external = 0, external_moved_at = NULL, sync_miss_count = 0
             WHERE info_hash = ?1",
            params![info_hash],
        )?;
        let _ = conn.execute(
            "UPDATE snatched_records
             SET external_moved_at = NULL
             WHERE info_hash = ?1",
            params![info_hash],
        );
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TorrentDbMetadata {
    pub info_hash: String,
    pub display_title: Option<String>,
    pub category: Option<String>,
    pub labels: Option<String>,
    pub is_watched: bool,
    pub notes: Option<String>,
    pub added_at: u64,
    pub completed_on: Option<u64>,
    pub save_path: Option<String>,
    pub download_limit_kib: u32,
    pub upload_limit_kib: u32,
    pub ratio_limit: f32,
    pub sequential_download: bool,
    pub first_last_piece_prio: bool,
    pub super_seeding: bool,
    pub racing_override: Option<bool>,
    // Feature 6: user-defined listing names
    pub user_display_title: Option<String>,
    pub user_title_locked: bool,
    // Feature 3: manual poster ordering
    pub sort_order: Option<i64>,
    pub poster_path: Option<String>,
    // Feature 5: removed-torrent tracking
    pub is_external: bool,
    pub sync_miss_count: i64,
    pub last_checked: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn memory_db() -> Database {
        Database::new(Path::new(":memory:")).expect("in-memory database")
    }

    #[test]
    fn racing_override_round_trips_and_keeps_off_distinct_from_unset() {
        let db = memory_db();

        // Clearing a choice that was never stored must not create a row.
        db.set_torrent_racing_override(HASH, None).unwrap();
        assert!(db.get_torrent_metadata(HASH).unwrap().is_none());

        // A choice creates the row, with the schema defaults for the rest.
        db.set_torrent_racing_override(HASH, Some(true)).unwrap();
        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, Some(true));
        assert_eq!(row.ratio_limit, -1.0);
        assert_eq!(row.download_limit_kib, 0);

        // An explicit "off" is a stored choice, not the same as "unset".
        db.set_torrent_racing_override(HASH, Some(false)).unwrap();
        let all = db.list_all_torrent_metadata().unwrap();
        assert_eq!(all[HASH].racing_override, Some(false));

        // Clearing returns the torrent to following the global preset.
        db.set_torrent_racing_override(HASH, None).unwrap();
        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, None);
    }

    #[test]
    fn racing_override_write_leaves_other_metadata_untouched() {
        let db = memory_db();
        let meta = TorrentDbMetadata {
            info_hash: HASH.to_string(),
            category: Some("tv".to_string()),
            added_at: 1_700_000_000,
            ratio_limit: 2.5,
            sequential_download: true,
            ..Default::default()
        };
        db.upsert_torrent_metadata(&meta).unwrap();

        db.set_torrent_racing_override(HASH, Some(true)).unwrap();

        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, Some(true));
        assert_eq!(row.category.as_deref(), Some("tv"));
        assert_eq!(row.added_at, 1_700_000_000);
        assert_eq!(row.ratio_limit, 2.5);
        assert!(row.sequential_download);
    }

    #[test]
    fn last_checked_round_trips_and_preserves_on_coalesce() {
        let db = memory_db();
        let mut meta = TorrentDbMetadata {
            info_hash: HASH.to_string(),
            added_at: 1_700_000_000,
            last_checked: Some(1_700_000_123),
            ..Default::default()
        };
        db.upsert_torrent_metadata(&meta).unwrap();
        assert_eq!(
            db.get_torrent_metadata(HASH).unwrap().unwrap().last_checked,
            Some(1_700_000_123)
        );

        // When updated without specifying last_checked, previous last_checked is preserved
        meta.last_checked = None;
        db.upsert_torrent_metadata(&meta).unwrap();
        assert_eq!(
            db.get_torrent_metadata(HASH).unwrap().unwrap().last_checked,
            Some(1_700_000_123)
        );

        // Targeted setter updates last_checked
        db.set_torrent_last_checked(HASH, 1_700_000_999).unwrap();
        assert_eq!(
            db.get_torrent_metadata(HASH).unwrap().unwrap().last_checked,
            Some(1_700_000_999)
        );
    }

    #[test]
    fn full_row_upsert_stores_the_racing_choice_it_carries() {
        let db = memory_db();
        let mut meta = TorrentDbMetadata {
            info_hash: HASH.to_string(),
            added_at: 1,
            ratio_limit: -1.0,
            racing_override: Some(false),
            ..Default::default()
        };
        db.upsert_torrent_metadata(&meta).unwrap();
        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, Some(false));

        // The engine keeps its metadata cache in step with this column,
        // because a full-row upsert writes whatever the row carries.
        meta.racing_override = None;
        db.upsert_torrent_metadata(&meta).unwrap();
        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, None);
    }

    #[test]
    fn racing_override_repairs_a_row_that_has_no_added_at() {
        let db = memory_db();
        // The super-seeding writer creates rows without added_at, which the
        // readers drop; storing a racing choice must not lose it with the row.
        db.set_torrent_super_seeding(HASH, true).unwrap();

        db.set_torrent_racing_override(HASH, Some(true)).unwrap();

        let row = db.get_torrent_metadata(HASH).unwrap().unwrap();
        assert_eq!(row.racing_override, Some(true));
        assert!(row.super_seeding);
        assert!(row.added_at > 0);
    }

    #[test]
    fn racing_override_column_is_added_to_an_existing_database() {
        let path = std::env::temp_dir().join(format!(
            "nebulae-racing-migration-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            // A database created before the column existed.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE torrent_metadata (
                    info_hash TEXT PRIMARY KEY, display_title TEXT, category TEXT,
                    labels TEXT, is_watched INTEGER DEFAULT 0, notes TEXT,
                    added_at INTEGER, completed_on INTEGER, save_path TEXT,
                    download_limit_kib INTEGER DEFAULT 0,
                    upload_limit_kib INTEGER DEFAULT 0,
                    ratio_limit REAL DEFAULT -1.0,
                    sequential_download INTEGER DEFAULT 0,
                    first_last_piece_prio INTEGER DEFAULT 0,
                    super_seeding INTEGER DEFAULT 0);
                 INSERT INTO torrent_metadata (info_hash, category, added_at)
                    VALUES ('old', 'keep', 5);",
            )
            .unwrap();
        }

        let db = Database::new(&path).unwrap();
        let rows = db.list_all_torrent_metadata().unwrap();
        assert_eq!(rows["old"].category.as_deref(), Some("keep"));
        assert_eq!(
            rows["old"].racing_override, None,
            "existing torrents start without a racing choice"
        );

        db.set_torrent_racing_override("old", Some(false)).unwrap();
        let row = db.get_torrent_metadata("old").unwrap().unwrap();
        assert_eq!(row.racing_override, Some(false));

        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn test_get_torrent_poster_resolves_from_settings_cache_when_poster_path_is_null() {
        let db = memory_db();
        let hash = "1111222233334444555566667777888899990000";

        // 1. Initially no metadata and no settings -> None
        assert_eq!(db.get_torrent_poster(hash).unwrap(), None);

        // 2. Insert metadata with title "Suzume (2022)" but poster_path = NULL
        let meta = TorrentDbMetadata {
            info_hash: hash.to_string(),
            display_title: Some("Suzume (2022)".to_string()),
            poster_path: None,
            added_at: 100,
            ..Default::default()
        };
        db.upsert_torrent_metadata(&meta).unwrap();

        // Still None because settings has no matching poster
        assert_eq!(db.get_torrent_poster(hash).unwrap(), None);

        // 3. Cache poster in settings under tmdb_poster:v3:suzume (2022)
        let expected_path = "/path/to/suzume_poster.webp";
        db.set_setting("tmdb_poster:v3:suzume (2022)", expected_path)
            .unwrap();

        // Resolves from settings cache!
        assert_eq!(
            db.get_torrent_poster(hash).unwrap().as_deref(),
            Some(expected_path)
        );

        // 4. If explicit custom poster is set, it overrides the settings cache
        let custom_path = "/path/to/custom_suzume.jpg";
        db.set_torrent_poster(hash, custom_path).unwrap();
        assert_eq!(
            db.get_torrent_poster(hash).unwrap().as_deref(),
            Some(custom_path)
        );
    }

    #[test]
    fn test_idempotent_backfill_from_settings_preserves_custom_and_locked_posters() {
        let path =
            std::env::temp_dir().join(format!("nebulae-backfill-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE torrent_metadata (
                     info_hash TEXT PRIMARY KEY, display_title TEXT, category TEXT,
                     labels TEXT, is_watched INTEGER DEFAULT 0, notes TEXT,
                     added_at INTEGER, completed_on INTEGER, save_path TEXT,
                     download_limit_kib INTEGER DEFAULT 0,
                     upload_limit_kib INTEGER DEFAULT 0,
                     ratio_limit REAL DEFAULT -1.0,
                     sequential_download INTEGER DEFAULT 0,
                     first_last_piece_prio INTEGER DEFAULT 0,
                     super_seeding INTEGER DEFAULT 0,
                     user_display_title TEXT, user_title_locked INTEGER DEFAULT 0,
                     sort_order INTEGER, poster_path TEXT, is_external INTEGER DEFAULT 0,
                     sync_miss_count INTEGER DEFAULT 0, last_checked INTEGER
                 );
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:fuuka', '/cache/fuuka.webp');
                 -- Item 1: null poster -> should be backfilled
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h1', 'Fuuka', NULL, 1);
                 -- Item 2: custom poster -> must NOT be overwritten
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h2', 'Fuuka', '/custom/custom_fuuka.png', 2);
                 -- Item 3: user locked title with null poster -> must NOT be overwritten
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                    VALUES ('h3', 'Fuuka', 1, NULL, 3);",
            )
            .unwrap();
        }

        // Initialize Database which runs migration backfill
        let db = Database::new(&path).unwrap();

        let m1 = db.get_torrent_metadata("h1").unwrap().unwrap();
        assert_eq!(m1.poster_path.as_deref(), Some("/cache/fuuka.webp"));

        let m2 = db.get_torrent_metadata("h2").unwrap().unwrap();
        assert_eq!(m2.poster_path.as_deref(), Some("/custom/custom_fuuka.png"));

        let m3 = db.get_torrent_metadata("h3").unwrap().unwrap();
        assert_eq!(m3.poster_path, None);

        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn test_corrupted_database_preserves_file_without_replacement() {
        let path = std::env::temp_dir().join(format!(
            "nebulae-corrupt-preserve-{}.db",
            std::process::id()
        ));
        let corrupt_content = b"THIS_IS_DEFINITELY_NOT_A_VALID_SQLITE_DATABASE_FILE";
        std::fs::write(&path, corrupt_content).unwrap();

        // Attempting to open with Database::new must fail with error
        let res = Database::new(&path);
        assert!(
            res.is_err(),
            "Database::new must return Err on corrupt database"
        );

        // The corrupt file MUST NOT be deleted, replaced, or truncated
        assert!(path.exists(), "Original corrupt file must remain on disk");
        let surviving_bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            surviving_bytes, corrupt_content,
            "File bytes must be strictly preserved without replacement"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_contaminated_fuuka_poster_cleanup_and_isolation() {
        let path = std::env::temp_dir().join(format!(
            "nebulae-contamination-cleanup-{}.db",
            std::process::id()
        ));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }

        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE torrent_metadata (
                     info_hash TEXT PRIMARY KEY, display_title TEXT, category TEXT,
                     labels TEXT, is_watched INTEGER DEFAULT 0, notes TEXT,
                     added_at INTEGER, completed_on INTEGER, save_path TEXT,
                     download_limit_kib INTEGER DEFAULT 0,
                     upload_limit_kib INTEGER DEFAULT 0,
                     ratio_limit REAL DEFAULT -1.0,
                     sequential_download INTEGER DEFAULT 0,
                     first_last_piece_prio INTEGER DEFAULT 0,
                     super_seeding INTEGER DEFAULT 0,
                     user_display_title TEXT, user_title_locked INTEGER DEFAULT 0,
                     sort_order INTEGER, poster_path TEXT, is_external INTEGER DEFAULT 0,
                     sync_miss_count INTEGER DEFAULT 0, last_checked INTEGER
                 );
                 CREATE TABLE snatched_records (
                     info_hash TEXT PRIMARY KEY, torrent_name TEXT NOT NULL,
                     size_bytes INTEGER DEFAULT 0, snatched_at INTEGER,
                     save_path TEXT, display_title TEXT, poster_path TEXT,
                     provider TEXT, provider_id TEXT, external_moved_at INTEGER
                 );

                 -- 1. Insert settings with Fuuka poster and contaminated entries
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:fuuka', '/thumbs/93758aeca10f8450.webp');
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:mary and a witch''s flower', '/thumbs/93758aeca10f8450.webp');
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:patlabor 2 the movie', '/thumbs/93758aeca10f8450.webp');
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:suzume', '/thumbs/3256ca8029bbbfd4.webp');

                 -- 2. Insert torrent_metadata entries
                 -- Fuuka with null poster
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h_fuuka', 'Fuuka', NULL, 1);
                 -- Mary with contaminated Fuuka poster
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h_mary', 'Mary and a witch''s flower', '/thumbs/93758aeca10f8450.webp', 2);
                 -- Patlabor with null poster
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h_patlabor', 'Patlabor 2 the movie', NULL, 3);
                 -- Suzume with null poster
                 INSERT INTO torrent_metadata (info_hash, display_title, poster_path, added_at)
                    VALUES ('h_suzume', 'Suzume', NULL, 4);
                 -- Custom locked poster on Mary
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                    VALUES ('h_custom', 'Mary and a witch''s flower', 1, '/custom/custom_mary.jpg', 5);",
            )
            .unwrap();
        }

        // Opening the Database executes startup purge and safe backfill
        let db = Database::new(&path).unwrap();

        // 1. Settings assertions:
        // Fuuka entry must be preserved
        assert_eq!(
            db.get_setting("tmdb_poster:v3:fuuka").unwrap().as_deref(),
            Some("/thumbs/93758aeca10f8450.webp")
        );
        // Suzume entry must be preserved
        assert_eq!(
            db.get_setting("tmdb_poster:v3:suzume").unwrap().as_deref(),
            Some("/thumbs/3256ca8029bbbfd4.webp")
        );
        // Contaminated non-Fuuka entries must be deleted
        assert_eq!(
            db.get_setting("tmdb_poster:v3:mary and a witch's flower")
                .unwrap(),
            None
        );
        assert_eq!(
            db.get_setting("tmdb_poster:v3:patlabor 2 the movie")
                .unwrap(),
            None
        );

        // 2. Metadata assertions:
        // Fuuka should be backfilled with Fuuka poster
        let m_fuuka = db.get_torrent_metadata("h_fuuka").unwrap().unwrap();
        assert_eq!(
            m_fuuka.poster_path.as_deref(),
            Some("/thumbs/93758aeca10f8450.webp")
        );

        // Mary's contaminated poster was reset to NULL
        let m_mary = db.get_torrent_metadata("h_mary").unwrap().unwrap();
        assert_eq!(m_mary.poster_path, None);

        // Patlabor was NOT backfilled with Fuuka poster
        let m_patlabor = db.get_torrent_metadata("h_patlabor").unwrap().unwrap();
        assert_eq!(m_patlabor.poster_path, None);

        // Suzume was backfilled with genuine Suzume poster
        let m_suzume = db.get_torrent_metadata("h_suzume").unwrap().unwrap();
        assert_eq!(
            m_suzume.poster_path.as_deref(),
            Some("/thumbs/3256ca8029bbbfd4.webp")
        );

        // Custom locked poster was preserved unchanged
        let m_custom = db.get_torrent_metadata("h_custom").unwrap().unwrap();
        assert_eq!(
            m_custom.poster_path.as_deref(),
            Some("/custom/custom_mary.jpg")
        );

        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn test_custom_locked_posters_and_non_provider_settings_preserved_across_inits() {
        let path = std::env::temp_dir().join(format!(
            "nebulae_test_locked_preserve_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE torrent_metadata (
                     info_hash TEXT PRIMARY KEY, display_title TEXT, category TEXT,
                     labels TEXT, is_watched INTEGER DEFAULT 0, notes TEXT,
                     added_at INTEGER, completed_on INTEGER, save_path TEXT,
                     download_limit_kib INTEGER DEFAULT 0,
                     upload_limit_kib INTEGER DEFAULT 0,
                     ratio_limit REAL DEFAULT -1.0,
                     sequential_download INTEGER DEFAULT 0,
                     first_last_piece_prio INTEGER DEFAULT 0,
                     super_seeding INTEGER DEFAULT 0,
                     user_display_title TEXT, user_title_locked INTEGER DEFAULT 0,
                     sort_order INTEGER, poster_path TEXT, is_external INTEGER DEFAULT 0,
                     sync_miss_count INTEGER DEFAULT 0, last_checked INTEGER
                 );
                 CREATE TABLE snatched_records (
                     info_hash TEXT PRIMARY KEY, torrent_name TEXT NOT NULL,
                     size_bytes INTEGER DEFAULT 0, snatched_at INTEGER,
                     save_path TEXT, display_title TEXT, poster_path TEXT,
                     provider TEXT, provider_id TEXT, external_moved_at INTEGER
                 );

                 -- Non-provider setting containing hash
                 INSERT INTO settings (key, value) VALUES ('custom_user_config', '{\"token\":\"93758aeca10f8450\"}');
                 -- Provider setting containing contaminated hash
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:another title', '/thumbs/93758aeca10f8450.webp');
                 INSERT INTO settings (key, value) VALUES ('mal_poster:v3:another title', '/thumbs/93758aeca10f8450.webp');
                 -- Provider setting containing fuuka keyword for non-fuuka title
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:patlabor 2 the movie', '/cache/fuuka.webp');
                 -- Genuine Fuuka settings
                 INSERT INTO settings (key, value) VALUES ('tmdb_poster:v3:fuuka (2017)', '/cache/fuuka.webp');

                 -- Item 1: Locked custom poster (even if file path contains hash)
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                     VALUES ('h_locked_hash', 'Special Edition', 1, '/custom/art_93758aeca10f8450.png', 10);

                 -- Item 2: Locked custom poster (arbitrary path)
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                     VALUES ('h_locked_custom', 'My Custom Title', 1, '/Users/user/Pictures/custom.jpg', 20);

                 -- Item 3: Unlocked item with contaminated hash
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                     VALUES ('h_unlocked_contam', 'Regular Item', 0, '/thumbs/93758aeca10f8450.webp', 30);

                 -- Item 4: Unlocked item with fuuka in path
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                     VALUES ('h_unlocked_fuuka_path', 'Mary and the Witch''s Flower', 0, '/cache/fuuka_scan.webp', 40);

                 -- Item 5: Genuine Fuuka torrent
                 INSERT INTO torrent_metadata (info_hash, display_title, user_title_locked, poster_path, added_at)
                     VALUES ('h_genuine_fuuka', 'Fuuka (2017)', 0, '/cache/fuuka.webp', 50);",
            )
            .unwrap();
        }

        // Pass 1: First startup init
        {
            let db = Database::new(&path).unwrap();

            // Non-provider setting must remain intact
            assert_eq!(
                db.get_setting("custom_user_config").unwrap().as_deref(),
                Some("{\"token\":\"93758aeca10f8450\"}")
            );

            // Provider contaminated settings must be purged
            assert_eq!(
                db.get_setting("tmdb_poster:v3:another title").unwrap(),
                None
            );
            assert_eq!(db.get_setting("mal_poster:v3:another title").unwrap(), None);
            assert_eq!(
                db.get_setting("tmdb_poster:v3:patlabor 2 the movie")
                    .unwrap(),
                None
            );

            // Genuine Fuuka setting must be preserved
            assert_eq!(
                db.get_setting("tmdb_poster:v3:fuuka (2017)")
                    .unwrap()
                    .as_deref(),
                Some("/cache/fuuka.webp")
            );

            // Locked items must be completely preserved
            let m1 = db.get_torrent_metadata("h_locked_hash").unwrap().unwrap();
            assert_eq!(
                m1.poster_path.as_deref(),
                Some("/custom/art_93758aeca10f8450.png")
            );
            assert!(m1.user_title_locked);

            let m2 = db.get_torrent_metadata("h_locked_custom").unwrap().unwrap();
            assert_eq!(
                m2.poster_path.as_deref(),
                Some("/Users/user/Pictures/custom.jpg")
            );
            assert!(m2.user_title_locked);

            // Unlocked contaminated items must be reset to NULL
            let m3 = db
                .get_torrent_metadata("h_unlocked_contam")
                .unwrap()
                .unwrap();
            assert_eq!(m3.poster_path, None);

            let m4 = db
                .get_torrent_metadata("h_unlocked_fuuka_path")
                .unwrap()
                .unwrap();
            assert_eq!(m4.poster_path, None);

            // Genuine Fuuka item must retain its poster
            let m5 = db.get_torrent_metadata("h_genuine_fuuka").unwrap().unwrap();
            assert_eq!(m5.poster_path.as_deref(), Some("/cache/fuuka.webp"));
        }

        // Pass 2: Second startup init (assert idempotence)
        {
            let db = Database::new(&path).unwrap();

            assert_eq!(
                db.get_setting("custom_user_config").unwrap().as_deref(),
                Some("{\"token\":\"93758aeca10f8450\"}")
            );

            let m1 = db.get_torrent_metadata("h_locked_hash").unwrap().unwrap();
            assert_eq!(
                m1.poster_path.as_deref(),
                Some("/custom/art_93758aeca10f8450.png")
            );

            let m2 = db.get_torrent_metadata("h_locked_custom").unwrap().unwrap();
            assert_eq!(
                m2.poster_path.as_deref(),
                Some("/Users/user/Pictures/custom.jpg")
            );
        }

        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn test_posters_per_row_persistence() {
        let db = memory_db();
        // Unset initially
        assert_eq!(db.get_setting("media_posters_per_row").unwrap(), None);

        // Persist 7
        db.set_setting("media_posters_per_row", "7").unwrap();
        assert_eq!(
            db.get_setting("media_posters_per_row").unwrap().as_deref(),
            Some("7")
        );

        // Clamped preference write
        let count: u32 = 15;
        let clamped = count.clamp(2, 12);
        db.set_setting("media_posters_per_row", &clamped.to_string())
            .unwrap();
        assert_eq!(
            db.get_setting("media_posters_per_row").unwrap().as_deref(),
            Some("12")
        );
    }

    #[test]
    fn test_poster_size_persistence() {
        let db = memory_db();
        // Unset initially
        assert_eq!(db.get_setting("media_poster_size").unwrap(), None);

        // Persist 200
        db.set_setting("media_poster_size", "200").unwrap();
        assert_eq!(
            db.get_setting("media_poster_size").unwrap().as_deref(),
            Some("200")
        );

        // Clamped preference write (bounds 100 to 300)
        let size: u32 = 450;
        let clamped = size.clamp(100, 300);
        db.set_setting("media_poster_size", &clamped.to_string())
            .unwrap();
        assert_eq!(
            db.get_setting("media_poster_size").unwrap().as_deref(),
            Some("300")
        );
    }

    #[test]
    fn test_poster_spacing_persistence() {
        let db = memory_db();
        // Unset initially
        assert_eq!(db.get_setting("media_poster_spacing").unwrap(), None);

        // Persist 24
        db.set_setting("media_poster_spacing", "24").unwrap();
        assert_eq!(
            db.get_setting("media_poster_spacing").unwrap().as_deref(),
            Some("24")
        );

        // Clamped preference write (bounds 4 to 48)
        let spacing: u32 = 64;
        let clamped = spacing.clamp(4, 48);
        db.set_setting("media_poster_spacing", &clamped.to_string())
            .unwrap();
        assert_eq!(
            db.get_setting("media_poster_spacing").unwrap().as_deref(),
            Some("48")
        );
    }

    #[test]
    fn test_downloaded_media_order_roundtrip_and_fallback() {
        let db = memory_db();

        // 1. Initially empty
        let initial = db.get_downloaded_media_order().unwrap();
        assert!(initial.is_empty());

        // 2. Fallback to torrent_metadata sort_order when setting not set
        db.set_torrent_sort_order("hash_b", Some(1)).unwrap();
        db.set_torrent_sort_order("hash_a", Some(0)).unwrap();
        db.set_torrent_sort_order("hash_c", Some(2)).unwrap();

        let fallback_order = db.get_downloaded_media_order().unwrap();
        assert_eq!(fallback_order, vec!["hash_a", "hash_b", "hash_c"]);

        // 3. Explicit media_downloaded_order setting takes priority
        let custom = vec![
            "hash_c".to_string(),
            "hash_a".to_string(),
            "hash_b".to_string(),
        ];
        let json = serde_json::to_string(&custom).unwrap();
        db.set_setting("media_downloaded_order", &json).unwrap();

        let loaded = db.get_downloaded_media_order().unwrap();
        assert_eq!(loaded, custom);
    }

    #[test]
    fn test_external_media_order_roundtrip() {
        let db = memory_db();

        // Initially empty
        let initial = db.get_external_media_order().unwrap();
        assert!(initial.is_empty());

        // Save order
        let order1 = vec![
            "/media/Fuuka (2017)".to_string(),
            "/media/Patlabor 2 (1993)".to_string(),
            "/media/Suzume (2022)".to_string(),
        ];
        db.set_external_media_order(&order1).unwrap();

        let loaded1 = db.get_external_media_order().unwrap();
        assert_eq!(loaded1, order1);

        // Reorder (drag Suzume to front)
        let order2 = vec![
            "/media/Suzume (2022)".to_string(),
            "/media/Fuuka (2017)".to_string(),
            "/media/Patlabor 2 (1993)".to_string(),
        ];
        db.set_external_media_order(&order2).unwrap();

        let loaded2 = db.get_external_media_order().unwrap();
        assert_eq!(loaded2, order2);

        // Deduplication on set and get: duplicates must be stripped
        let with_dupes = vec![
            "/media/Suzume (2022)".to_string(),
            "/media/Suzume (2022)".to_string(),
            "/media/Fuuka (2017)".to_string(),
            "/media/Patlabor 2 (1993)".to_string(),
            "/media/Fuuka (2017)".to_string(),
        ];
        db.set_external_media_order(&with_dupes).unwrap();
        let loaded3 = db.get_external_media_order().unwrap();
        assert_eq!(loaded3, order2);
    }
}

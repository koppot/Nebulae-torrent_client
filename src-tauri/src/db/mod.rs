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
        ];
        for (col, col_type) in extra_cols {
            let sql = format!("ALTER TABLE torrent_metadata ADD COLUMN {col} {col_type}");
            let _ = conn.execute(&sql, []);
        }

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
                sequential_download, first_last_piece_prio, super_seeding, racing_override
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(info_hash) DO UPDATE SET
                display_title = excluded.display_title,
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
                racing_override = excluded.racing_override",
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
            ],
        )?;
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
                    sequential_download, first_last_piece_prio, super_seeding, racing_override
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
                    sequential_download, first_last_piece_prio, super_seeding, racing_override
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
}

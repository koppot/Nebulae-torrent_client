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
}

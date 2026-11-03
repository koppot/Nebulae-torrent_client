use super::bencode::{parse_bencode, BVal};
use super::types::{QbitSourceType, QbitTorrent};
use rusqlite::Connection;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct QbitReader;

impl QbitReader {
    /// Read all categories from categories.json and qBittorrent.ini if available
    pub fn load_category_save_paths(config_dir: Option<&Path>) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Some(dir) = config_dir else {
            return map;
        };

        let cat_file = dir.join("categories.json");
        if cat_file.is_file() {
            if let Ok(content) = std::fs::read_to_string(&cat_file) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = val.as_object() {
                        for (k, v) in obj {
                            if let Some(sp) = v.get("save_path").and_then(|s| s.as_str()) {
                                if !sp.trim().is_empty() {
                                    map.insert(k.clone(), sp.trim().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        let ini_file = dir.join("qBittorrent.ini");
        if ini_file.is_file() {
            if let Ok(content) = std::fs::read_to_string(&ini_file) {
                let mut in_categories = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('[') && trimmed.ends_with(']') {
                        in_categories = trimmed.eq_ignore_ascii_case("[BitTorrent\\Categories]")
                            || trimmed.eq_ignore_ascii_case("[Categories]");
                        continue;
                    }
                    if in_categories && trimmed.contains('=') {
                        let mut parts = trimmed.splitn(2, '=');
                        if let (Some(cat_name), Some(save_path)) = (parts.next(), parts.next()) {
                            let k = cat_name.trim().to_string();
                            let v = save_path.trim().to_string();
                            if !k.is_empty() && !v.is_empty() && !map.contains_key(&k) {
                                map.insert(k, v);
                            }
                        }
                    }
                }
            }
        }

        map
    }

    /// Read legacy file storage from BT_backup: `<hash>.torrent` + `<hash>.fastresume`
    pub fn read_legacy_bt_backup(
        data_dir: &Path,
        category_paths: &HashMap<String, String>,
    ) -> Vec<QbitTorrent> {
        let bt_backup = data_dir.join("BT_backup");
        if !bt_backup.is_dir() {
            return Vec::new();
        }

        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&bt_backup) else {
            return Vec::new();
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext != "fastresume" {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if stem.is_empty() {
                continue;
            }

            let torrent_file = bt_backup.join(format!("{stem}.torrent"));
            if !torrent_file.is_file() {
                debug!(
                    "Skipping orphaned fastresume without torrent file: {:?}",
                    path
                );
                continue;
            }

            let Ok(fastresume_bytes) = std::fs::read(&path) else {
                warn!("Cannot read fastresume file: {:?}", path);
                continue;
            };

            let Ok(torrent_bytes) = std::fs::read(&torrent_file) else {
                warn!("Cannot read torrent file: {:?}", torrent_file);
                continue;
            };

            match Self::parse_qbit_entry(
                &stem,
                &torrent_bytes,
                &fastresume_bytes,
                QbitSourceType::LegacyFile {
                    torrent_path: torrent_file,
                    fastresume_path: path,
                },
                category_paths,
            ) {
                Ok(torrent) => out.push(torrent),
                Err(e) => {
                    warn!("Failed parsing qBittorrent entry for {stem}: {e}");
                }
            }
        }

        info!(
            "Read {} torrents from legacy BT_backup at {:?}",
            out.len(),
            bt_backup
        );
        out
    }

    /// Read SQLite storage from torrents.db using dynamic schema discovery
    pub fn read_sqlite_db(
        db_path: &Path,
        category_paths: &HashMap<String, String>,
    ) -> Result<Vec<QbitTorrent>, String> {
        if !db_path.is_file() {
            return Ok(Vec::new());
        }

        // Open read-only URI: file:... ?mode=ro
        let uri = format!("file:{}?mode=ro", db_path.to_string_lossy());
        let conn = Connection::open_with_flags(
            &uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .or_else(|_| Connection::open(db_path))
        .map_err(|e| format!("Failed to open SQLite database at {:?}: {e}", db_path))?;

        // Inspect sqlite_master for table name
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .map_err(|e| format!("sqlite_master query failed: {e}"))?;

        let table_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Failed to list tables: {e}"))?
            .flatten()
            .collect();

        // Prefer table named 'torrents' or first table
        let target_table = table_names
            .iter()
            .find(|t| t.eq_ignore_ascii_case("torrents"))
            .or_else(|| table_names.first())
            .ok_or_else(|| "No user tables found in torrents.db".to_string())?;

        // Inspect columns via PRAGMA table_info
        let pragma_sql = format!("PRAGMA table_info(\"{target_table}\")");
        let mut col_stmt = conn
            .prepare(&pragma_sql)
            .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;

        let cols: Vec<String> = col_stmt
            .query_map([], |row| row.get(1))
            .map_err(|e| format!("Failed to query columns: {e}"))?
            .flatten()
            .collect();

        debug!("Discovered columns in table '{}': {:?}", target_table, cols);

        let id_col = cols
            .iter()
            .find(|c| {
                let l = c.to_lowercase();
                l.contains("hash") || l.contains("id")
            })
            .or_else(|| cols.first())
            .ok_or_else(|| format!("Could not identify hash/id column in '{target_table}'"))?;

        let meta_col = cols
            .iter()
            .find(|c| {
                let l = c.to_lowercase();
                l.contains("meta") || l.contains("torrent")
            })
            .ok_or_else(|| format!("Could not identify metadata column in '{target_table}'"))?;

        let resume_col = cols
            .iter()
            .find(|c| {
                let l = c.to_lowercase();
                l.contains("resume") || l.contains("fastresume") || l.contains("data")
            })
            .ok_or_else(|| format!("Could not identify resume data column in '{target_table}'"))?;

        let query_sql = format!(
            "SELECT \"{id_col}\", \"{meta_col}\", \"{resume_col}\" FROM \"{target_table}\""
        );
        let mut select_stmt = conn
            .prepare(&query_sql)
            .map_err(|e| format!("Prepare query failed: {e}"))?;

        let mut rows = select_stmt
            .query([])
            .map_err(|e| format!("Execute query failed: {e}"))?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| format!("Read row failed: {e}"))? {
            // id column may be TEXT or BLOB
            let raw_id_res: Result<String, _> = row.get(0);
            let id_str = match raw_id_res {
                Ok(s) => s,
                Err(_) => {
                    let blob: Vec<u8> = row.get(0).unwrap_or_default();
                    to_hex(&blob)
                }
            };

            let meta_bytes: Vec<u8> = row
                .get(1)
                .map_err(|e| format!("Failed to read metadata BLOB for {id_str}: {e}"))?;
            let resume_bytes: Vec<u8> = row
                .get(2)
                .map_err(|e| format!("Failed to read resume data BLOB for {id_str}: {e}"))?;

            match Self::parse_qbit_entry(
                &id_str,
                &meta_bytes,
                &resume_bytes,
                QbitSourceType::SqliteRow {
                    info_hash: id_str.to_lowercase(),
                    db_path: db_path.to_path_buf(),
                },
                category_paths,
            ) {
                Ok(torrent) => out.push(torrent),
                Err(e) => {
                    warn!("Failed parsing SQLite row for {id_str}: {e}");
                }
            }
        }

        info!(
            "Read {} torrents from SQLite database at {:?}",
            out.len(),
            db_path
        );
        Ok(out)
    }

    /// Merges legacy and SQLite torrent lists by info-hash, preferring the newer entry
    pub fn merge_sources(
        legacy: Vec<QbitTorrent>,
        sqlite: Vec<QbitTorrent>,
    ) -> (Vec<QbitTorrent>, String) {
        if legacy.is_empty() && sqlite.is_empty() {
            return (Vec::new(), "Empty".to_string());
        }
        if legacy.is_empty() {
            return (sqlite, "SQLite torrents.db".to_string());
        }
        if sqlite.is_empty() {
            return (legacy, "Legacy BT_backup".to_string());
        }

        let mut map: HashMap<String, QbitTorrent> = HashMap::new();
        for t in legacy {
            map.insert(t.info_hash.clone(), t);
        }

        let mut overwritten = 0;
        for t in sqlite {
            if let Some(existing) = map.get_mut(&t.info_hash) {
                if t.added_time >= existing.added_time {
                    *existing = t;
                    overwritten += 1;
                }
            } else {
                map.insert(t.info_hash.clone(), t);
            }
        }

        let merged: Vec<QbitTorrent> = map.into_values().collect();
        let format_desc = format!(
            "Merged (Hybrid: {} total, {} resolved from newer SQLite)",
            merged.len(),
            overwritten
        );
        (merged, format_desc)
    }

    fn parse_qbit_entry(
        raw_stem: &str,
        torrent_bytes: &[u8],
        fastresume_bytes: &[u8],
        source: QbitSourceType,
        category_paths: &HashMap<String, String>,
    ) -> Result<QbitTorrent, String> {
        let resume_bval = parse_bencode(fastresume_bytes)
            .map_err(|e| format!("Corrupt fastresume bencode: {e}"))?;

        // 1. Resolve Info-Hash: verify or compute from torrent_bytes
        let computed_hash = compute_sha1_infohash(torrent_bytes);
        let info_hash = computed_hash.unwrap_or_else(|| raw_stem.to_lowercase());

        // 2. Name
        let qbt_name = resume_bval.dict_get_str("qBt-name");
        let torrent_name = extract_torrent_name(torrent_bytes);
        let name = qbt_name
            .or(torrent_name)
            .unwrap_or_else(|| info_hash.clone());

        // 3. Save path: prefer qBt-savePath if present and non-empty, otherwise save_path
        let qbt_save = resume_bval
            .dict_get_str("qBt-savePath")
            .filter(|s| !s.trim().is_empty());
        let lt_save = resume_bval
            .dict_get_str("save_path")
            .filter(|s| !s.trim().is_empty());
        let dl_path = resume_bval.dict_get_str("qBt-downloadPath");

        let mut save_path = qbt_save.or(lt_save).unwrap_or_default();

        // 4. Category & tags
        let category = resume_bval
            .dict_get_str("qBt-category")
            .filter(|c| !c.trim().is_empty());

        // Category save path fallback if save_path is empty
        if save_path.is_empty() {
            if let Some(ref cat) = category {
                if let Some(cat_sp) = category_paths.get(cat) {
                    save_path = cat_sp.clone();
                }
            }
        }

        let tags_str = resume_bval.dict_get_str("qBt-tags").unwrap_or_default();
        let tags: Vec<String> = tags_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // 5. Content layout
        let content_layout = resume_bval.dict_get_str("qBt-contentLayout");

        // 6. Flags & states
        let paused = resume_bval.dict_get_int("paused").unwrap_or(0) != 0;
        let auto_managed = resume_bval.dict_get_int("auto_managed").unwrap_or(1) != 0;
        let sequential_download = resume_bval.dict_get_int("sequential_download").unwrap_or(0) != 0;
        let first_last_piece_prio = resume_bval
            .dict_get_int("qBt-firstLastPiecePriority")
            .unwrap_or(0)
            != 0;

        let ratio_limit = resume_bval
            .dict_get_int("qBt-ratioLimit")
            .map(|r| (r as f32) / 1000.0)
            .unwrap_or(-1.0);
        let seeding_time_limit = resume_bval.dict_get_int("qBt-seedingTimeLimit");

        let upload_limit_kib = resume_bval
            .dict_get_int("upload_rate_limit")
            .or_else(|| resume_bval.dict_get_int("upload_limit"))
            .map(|l| (l / 1024).max(0) as u32)
            .unwrap_or(0);

        let download_limit_kib = resume_bval
            .dict_get_int("download_rate_limit")
            .or_else(|| resume_bval.dict_get_int("download_limit"))
            .map(|l| (l / 1024).max(0) as u32)
            .unwrap_or(0);

        let added_time = resume_bval
            .dict_get_int("added_time")
            .or_else(|| resume_bval.dict_get_int("qBt-addedTime"))
            .map(|t| t.max(0) as u64)
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            });

        let completed_time = resume_bval
            .dict_get_int("completed_time")
            .or_else(|| resume_bval.dict_get_int("completed_time"))
            .filter(|&t| t > 0)
            .map(|t| t as u64);

        // 7. Trackers
        let mut trackers = Vec::new();
        if let Some(announce_list) = resume_bval.dict_get_list("trackers") {
            for tier in announce_list {
                if let Some(list) = tier.as_list() {
                    for tr in list {
                        if let Some(url) = tr.as_str_lossy() {
                            if !url.trim().is_empty() && !trackers.contains(&url) {
                                trackers.push(url);
                            }
                        }
                    }
                } else if let Some(url) = tier.as_str_lossy() {
                    if !url.trim().is_empty() && !trackers.contains(&url) {
                        trackers.push(url);
                    }
                }
            }
        }

        // 8. Mapped files (renames relative to save path)
        let mut mapped_files = Vec::new();
        if let Some(mf_list) = resume_bval.dict_get_list("mapped_files") {
            for item in mf_list {
                if let Some(path_str) = item.as_str_lossy() {
                    mapped_files.push(path_str.replace('\\', "/"));
                }
            }
        }

        // 9. File priorities
        let mut file_priorities = Vec::new();
        if let Some(fp_list) = resume_bval.dict_get_list("file_priority") {
            for p in fp_list {
                if let Some(val) = p.as_int() {
                    file_priorities.push(val.clamp(0, 255) as u8);
                }
            }
        } else if let Some(fp_bytes) = resume_bval.dict_get_bytes("file_priority") {
            file_priorities.extend_from_slice(fp_bytes);
        }

        // 10. Total size and progress
        let (total_size, is_multi) = calculate_torrent_size(torrent_bytes);
        let total_downloaded = resume_bval
            .dict_get_int("total_downloaded")
            .unwrap_or(0)
            .max(0) as u64;

        let seed_status = resume_bval.dict_get_int("qBt-seedStatus").unwrap_or(0);
        let progress = if seed_status == 1 || (completed_time.is_some() && total_size > 0) {
            1.0f32
        } else if total_size > 0 {
            (total_downloaded as f32 / total_size as f32).clamp(0.0, 1.0)
        } else {
            0.0f32
        };

        let state_string = if paused {
            if progress >= 1.0 {
                "Paused Seeding".to_string()
            } else {
                "Paused Downloading".to_string()
            }
        } else if progress >= 1.0 {
            "Seeding".to_string()
        } else {
            "Downloading".to_string()
        };

        // Determine if save path is missing on current filesystem
        let missing_path = if save_path.is_empty() {
            true
        } else {
            !Path::new(&save_path).exists()
        };

        let _ = is_multi; // Used for content layout resolution in mapper

        Ok(QbitTorrent {
            info_hash,
            name,
            save_path,
            download_path: dl_path,
            category,
            tags,
            content_layout,
            paused,
            auto_managed,
            sequential_download,
            first_last_piece_prio,
            ratio_limit,
            seeding_time_limit,
            upload_limit_kib,
            download_limit_kib,
            added_time,
            completed_time,
            total_size,
            progress,
            state_string,
            trackers,
            mapped_files,
            file_priorities,
            raw_torrent_bytes: torrent_bytes.to_vec(),
            source,
            already_in_nebula: false,
            missing_path,
        })
    }
}

fn compute_sha1_infohash(torrent_bytes: &[u8]) -> Option<String> {
    let bval = parse_bencode(torrent_bytes).ok()?;
    let dict = bval.as_dict()?;
    let info_val = dict.get(b"info".as_ref())?;

    // Re-encode info dict to calculate SHA-1
    let mut info_bytes = Vec::new();
    encode_bval(info_val, &mut info_bytes);

    let mut hasher = Sha1::new();
    hasher.update(&info_bytes);
    let result = hasher.finalize();
    Some(to_hex(&result))
}

fn extract_torrent_name(torrent_bytes: &[u8]) -> Option<String> {
    let bval = parse_bencode(torrent_bytes).ok()?;
    let info = bval.dict_get_dict("info")?;
    let name_bval = info.get(b"name".as_ref())?;
    name_bval.as_str_lossy()
}

fn calculate_torrent_size(torrent_bytes: &[u8]) -> (u64, bool) {
    let Ok(bval) = parse_bencode(torrent_bytes) else {
        return (0, false);
    };
    let Some(info) = bval.dict_get_dict("info") else {
        return (0, false);
    };

    // Single file torrent has length in info
    if let Some(len) = info.get(b"length".as_ref()).and_then(|v| v.as_int()) {
        return (len.max(0) as u64, false);
    }

    // Multi-file torrent has files list in info
    if let Some(files) = info.get(b"files".as_ref()).and_then(|v| v.as_list()) {
        let mut sum = 0u64;
        for f in files {
            if let Some(flen) = f.dict_get_int("length") {
                sum += flen.max(0) as u64;
            }
        }
        return (sum, true);
    }

    (0, false)
}

fn encode_bval(val: &BVal, out: &mut Vec<u8>) {
    match val {
        BVal::Int(i) => {
            out.extend_from_slice(b"i");
            out.extend_from_slice(i.to_string().as_bytes());
            out.extend_from_slice(b"e");
        }
        BVal::Bytes(b) => {
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.extend_from_slice(b":");
            out.extend_from_slice(b);
        }
        BVal::List(list) => {
            out.extend_from_slice(b"l");
            for item in list {
                encode_bval(item, out);
            }
            out.extend_from_slice(b"e");
        }
        BVal::Dict(dict) => {
            out.extend_from_slice(b"d");
            for (k, v) in dict {
                out.extend_from_slice(k.len().to_string().as_bytes());
                out.extend_from_slice(b":");
                out.extend_from_slice(k);
                encode_bval(v, out);
            }
            out.extend_from_slice(b"e");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_synthetic_qbit_entry() {
        // Construct synthetic fastresume
        let mut fr = Vec::new();
        fr.extend_from_slice(b"d12:qBt-savePath16:/downloads/anime8:qBt-name11:NeonGenesis8:qBt-tags5:mecha14:qBt-seedStatusi1ee");

        // Construct synthetic metainfo with info dict
        let mut metainfo = Vec::new();
        metainfo.extend_from_slice(b"d4:infod6:lengthi1048576e4:name11:NeonGenesisee");

        let torrent = QbitReader::parse_qbit_entry(
            "0123456789abcdef0123456789abcdef01234567",
            &metainfo,
            &fr,
            QbitSourceType::LegacyFile {
                torrent_path: PathBuf::from("a.torrent"),
                fastresume_path: PathBuf::from("a.fastresume"),
            },
            &HashMap::new(),
        )
        .unwrap();

        assert_eq!(torrent.name, "NeonGenesis");
        assert_eq!(torrent.save_path, "/downloads/anime");
        assert_eq!(torrent.tags, vec!["mecha".to_string()]);
        assert_eq!(torrent.total_size, 1048576);
        assert_eq!(torrent.progress, 1.0);
    }

    #[test]
    fn reads_sqlite_fixture_db_with_arbitrary_column_names() {
        let temp_dir = std::env::temp_dir().join(format!("qbit_test_db_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("torrents.db");

        // Construct SQLite database with arbitrary column names
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE torrents (hash_identifier TEXT PRIMARY KEY, torrent_blob BLOB, resume_blob BLOB)",
                [],
            )
            .unwrap();

            let mut fr = Vec::new();
            fr.extend_from_slice(
                b"d12:qBt-savePath14:/media/library8:qBt-name6:Ghibli14:qBt-seedStatusi1ee",
            );

            let mut meta = Vec::new();
            meta.extend_from_slice(b"d4:infod6:lengthi5242880e4:name6:Ghibliee");

            conn.execute(
                "INSERT INTO torrents (hash_identifier, torrent_blob, resume_blob) VALUES (?1, ?2, ?3)",
                rusqlite::params!["abcdef0123456789abcdef0123456789abcdef01", meta, fr],
            )
            .unwrap();
        }

        let torrents = QbitReader::read_sqlite_db(&db_path, &HashMap::new()).unwrap();
        assert_eq!(torrents.len(), 1);
        let t = &torrents[0];
        assert_eq!(t.name, "Ghibli");
        assert_eq!(t.save_path, "/media/library");
        assert_eq!(t.total_size, 5242880);
        assert_eq!(t.progress, 1.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

use nebulae_torrent_client_lib::db::{Database, TorrentDbMetadata};
use nebulae_torrent_client_lib::engine::{MockTorrentEngine, SharedEngine, TorrentEngine};
use nebulae_torrent_client_lib::qbit_import::bencode::BVal;
use nebulae_torrent_client_lib::qbit_import::importer::run_recheck_queue;
use nebulae_torrent_client_lib::qbit_import::locator::{DefaultLocator, LocatorTrait};
use nebulae_torrent_client_lib::qbit_import::mapper::{ContentLayout, QbitMapper, RenameOperation};
use nebulae_torrent_client_lib::qbit_import::reader::QbitReader;
use nebulae_torrent_client_lib::qbit_import::remover::{
    DefaultTrashProvider, ProcessChecker, QbitRemover, RestoreMode, TrashMethod, TrashProvider,
    WHOLE_PROFILE_ROLLBACK_CONFIRMATION_PHRASE,
};
use nebulae_torrent_client_lib::qbit_import::types::{ImportOptions, QbitSourceType, QbitTorrent};
use nebulae_torrent_client_lib::qbit_import::QbitImporter;
use rusqlite::Connection;
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct MockProcessChecker {
    running: bool,
}
impl ProcessChecker for MockProcessChecker {
    fn is_qbittorrent_running(&self) -> bool {
        self.running
    }
}

struct MockTrash {
    deleted: Mutex<Vec<PathBuf>>,
    fail: bool,
}
impl MockTrash {
    fn new(fail: bool) -> Self {
        Self {
            deleted: Mutex::new(Vec::new()),
            fail,
        }
    }
}
impl TrashProvider for MockTrash {
    fn delete_file(&self, path: &Path) -> Result<TrashMethod, String> {
        if self.fail {
            return Err("Simulated Trash failure".to_string());
        }
        self.deleted.lock().unwrap().push(path.to_path_buf());
        Ok(TrashMethod::MacTrash)
    }
}

fn create_synthetic_metainfo(
    name: &str,
    is_multi: bool,
    files: &[(&str, &[u8])],
) -> (Vec<u8>, String) {
    let mut total_payload = Vec::new();
    let piece_len = 16384usize;

    for (_, data) in files {
        total_payload.extend_from_slice(data);
    }

    let mut pieces = Vec::new();
    for chunk in total_payload.chunks(piece_len) {
        let hash = Sha1::digest(chunk);
        pieces.extend_from_slice(&hash);
    }

    let mut bencode = Vec::new();
    bencode.extend_from_slice(b"d8:announce26:http://tracker.example.com4:infod");

    if is_multi {
        bencode.extend_from_slice(b"5:filesl");
        for (rel_path, data) in files {
            bencode.extend_from_slice(b"d6:lengthi");
            bencode.extend_from_slice(data.len().to_string().as_bytes());
            bencode.extend_from_slice(b"e4:pathl");
            for part in rel_path.split('/') {
                bencode.extend_from_slice(part.len().to_string().as_bytes());
                bencode.extend_from_slice(b":");
                bencode.extend_from_slice(part.as_bytes());
            }
            bencode.extend_from_slice(b"ee");
        }
        bencode.extend_from_slice(b"e4:name");
        bencode.extend_from_slice(name.len().to_string().as_bytes());
        bencode.extend_from_slice(b":");
        bencode.extend_from_slice(name.as_bytes());
    } else {
        bencode.extend_from_slice(b"6:lengthi");
        bencode.extend_from_slice(files[0].1.len().to_string().as_bytes());
        bencode.extend_from_slice(b"e4:name");
        bencode.extend_from_slice(name.len().to_string().as_bytes());
        bencode.extend_from_slice(b":");
        bencode.extend_from_slice(name.as_bytes());
    }

    bencode.extend_from_slice(b"12:piece lengthi");
    bencode.extend_from_slice(piece_len.to_string().as_bytes());
    bencode.extend_from_slice(b"e6:pieces");
    bencode.extend_from_slice(pieces.len().to_string().as_bytes());
    bencode.extend_from_slice(b":");
    bencode.extend_from_slice(&pieces);
    bencode.extend_from_slice(b"ee");

    // Compute info_hash: find "4:info" and take dict from 'd' to matching 'e'
    let info_start = bencode.windows(6).position(|w| w == b"4:info").unwrap() + 6;
    let info_end = bencode.len() - 1; // ends before the last root dict 'e'
    let info_slice = &bencode[info_start..info_end];
    let digest = Sha1::digest(info_slice);
    let info_hash = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    (bencode, info_hash)
}

#[test]
fn test_acceptance_original_file_layout() {
    let base_dir = std::env::temp_dir().join(format!("qbit_test_orig_{}", std::process::id()));
    let _ = fs::create_dir_all(&base_dir);
    let t_name = "OriginalTorrent";

    let eff = QbitMapper::compute_effective_download_dir(
        base_dir.to_str().unwrap(),
        t_name,
        true,
        &ContentLayout::Original,
    );
    assert_eq!(eff, base_dir);

    let disk_path = QbitMapper::compute_file_disk_path(
        &eff,
        t_name,
        "video/movie.mkv",
        true,
        &ContentLayout::Original,
    );
    assert_eq!(disk_path, base_dir.join("OriginalTorrent/video/movie.mkv"));

    let _ = fs::remove_dir_all(&base_dir);
}

#[test]
fn test_acceptance_renamed_top_level_directory() {
    let orig_name = "MyAnimeShow";
    let orig_paths = vec!["MyAnimeShow/ep01.mkv".to_string()];
    let mapped_files = vec!["MyAnimeShow_1080p/ep01.mkv".to_string()];

    let ops = QbitMapper::compute_rename_operations(
        orig_name,
        &orig_paths,
        &mapped_files,
        &ContentLayout::Original,
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0],
        RenameOperation {
            is_folder: true,
            file_index: None,
            old_path: "MyAnimeShow".to_string(),
            new_name: "MyAnimeShow_1080p".to_string(),
        }
    );
}

#[test]
fn test_acceptance_renamed_nested_directory() {
    let orig_name = "Cinema";
    let orig_paths = vec!["Cinema/extra_features/doc.mp4".to_string()];
    let mapped_files = vec!["Cinema/extras/doc.mp4".to_string()];

    let ops = QbitMapper::compute_rename_operations(
        orig_name,
        &orig_paths,
        &mapped_files,
        &ContentLayout::Original,
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0],
        RenameOperation {
            is_folder: true,
            file_index: None,
            old_path: "Cinema/extra_features".to_string(),
            new_name: "extras".to_string(),
        }
    );
}

#[test]
fn test_acceptance_renamed_individual_files() {
    let orig_name = "Album";
    let orig_paths = vec![
        "Album/track01.flac".to_string(),
        "Album/track02.flac".to_string(),
    ];
    let mapped_files = vec![
        "Album/01 - Intro.flac".to_string(),
        "Album/track02.flac".to_string(),
    ];

    let ops = QbitMapper::compute_rename_operations(
        orig_name,
        &orig_paths,
        &mapped_files,
        &ContentLayout::Original,
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0],
        RenameOperation {
            is_folder: false,
            file_index: Some(0),
            old_path: "Album/track01.flac".to_string(),
            new_name: "01 - Intro.flac".to_string(),
        }
    );
}

#[test]
fn test_acceptance_subfolder_and_nosubfolder_layouts() {
    let base = "/storage/downloads";
    let t_name = "Documentary";

    // Subfolder creates explicit subfolder for single-file
    let eff_sub =
        QbitMapper::compute_effective_download_dir(base, t_name, false, &ContentLayout::Subfolder);
    assert_eq!(eff_sub, PathBuf::from("/storage/downloads/Documentary"));

    // NoSubfolder places multi-file directly in base
    let eff_no =
        QbitMapper::compute_effective_download_dir(base, t_name, true, &ContentLayout::NoSubfolder);
    assert_eq!(eff_no, PathBuf::from("/storage/downloads"));

    let disk_path = QbitMapper::compute_file_disk_path(
        &eff_no,
        t_name,
        "Documentary/doc.mp4",
        true,
        &ContentLayout::NoSubfolder,
    );
    assert_eq!(disk_path, PathBuf::from("/storage/downloads/doc.mp4"));
}

#[test]
fn test_acceptance_skipped_files_and_per_file_priorities() {
    assert_eq!(QbitMapper::map_file_priority(0), 0); // 0 = Do Not Download / Skip
    assert_eq!(QbitMapper::map_file_priority(1), 1); // 1 = Normal
    assert_eq!(QbitMapper::map_file_priority(2), 2); // 2..7 = High
    assert_eq!(QbitMapper::map_file_priority(7), 2);
}

#[test]
fn test_acceptance_incomplete_payloads_and_qb_extension() {
    let temp_dir = std::env::temp_dir().join(format!("qbit_test_qbext_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    let incomplete_file = temp_dir.join("movie.mkv.!qB");
    fs::write(&incomplete_file, b"partial bytes").unwrap();

    let expected_target = temp_dir.join("movie.mkv");
    let found = QbitMapper::find_existing_payload_file(&expected_target);
    assert_eq!(found, Some(incomplete_file));

    // Normal complete file
    let complete_file = temp_dir.join("sample.mp4");
    fs::write(&complete_file, b"complete bytes").unwrap();
    let found_complete = QbitMapper::find_existing_payload_file(&complete_file);
    assert_eq!(found_complete, Some(complete_file));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_acceptance_unicode_paths() {
    let unicode_name = "スタジオジブリ_風の谷のナウシカ";
    let (bytes, hash) = create_synthetic_metainfo(
        unicode_name,
        true,
        &[("本編/Nausicaa.mkv", b"animation video bytes")],
    );
    assert!(!hash.is_empty());

    let (parsed_name, parsed_paths) = match BVal::extract_torrent_files_and_name(&bytes) {
        Some(res) => res,
        None => {
            let res = nebulae_torrent_client_lib::qbit_import::bencode::BencodeParser::new(&bytes)
                .parse();
            panic!("Failed to parse unicode torrent: {res:?}");
        }
    };
    assert_eq!(parsed_name, unicode_name);
    assert_eq!(
        parsed_paths,
        vec![format!("{unicode_name}/本編/Nausicaa.mkv")]
    );
}

#[test]
fn test_acceptance_missing_and_offline_download_paths() {
    let offline_path = "/Volumes/OfflineHardDrive_NonExistent_XYZ/Downloads";
    assert!(!Path::new(offline_path).exists());

    let (bytes, hash) = create_synthetic_metainfo("OfflineShow", false, &[("show.mkv", b"test")]);

    let t = QbitTorrent {
        info_hash: hash.clone(),
        name: "OfflineShow".to_string(),
        save_path: offline_path.to_string(),
        download_path: None,
        category: None,
        tags: Vec::new(),
        content_layout: None,
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1700000000,
        completed_time: None,
        total_size: 4,
        progress: 0.0,
        state_string: "Missing path".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: vec![1],
        raw_torrent_bytes: bytes,
        source: QbitSourceType::LegacyFile {
            torrent_path: PathBuf::from(format!("/qbit/BT_backup/{hash}.torrent")),
            fastresume_path: PathBuf::from(format!("/qbit/BT_backup/{hash}.fastresume")),
        },
        already_in_nebula: false,
        missing_path: true,
    };

    let temp_root = std::env::temp_dir().join(format!("qbit_test_offline_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let proc = MockProcessChecker { running: false };
    let trash = MockTrash::new(false);

    // Removal must reject missing_path
    let (rc, not_removed, _) = QbitRemover::execute_removal(
        &[t],
        &temp_root.join("qbit"),
        &temp_root.join("backup"),
        &proc,
        &trash,
    );
    assert_eq!(rc, 0);
    assert_eq!(not_removed.len(), 1);
    assert!(not_removed[0].reason.contains("unavailable"));

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_last_checked_survives_restart() {
    let temp_db_path = std::env::temp_dir().join(format!("qbit_test_db_{}.db", std::process::id()));
    let _ = fs::remove_file(&temp_db_path);

    let db = Database::new(&temp_db_path).expect("Failed to create db");
    let test_hash = "aabbccddeeff00112233445566778899aabbccdd";

    let meta = TorrentDbMetadata {
        info_hash: test_hash.to_string(),
        display_title: Some("Persisted Title".to_string()),
        category: Some("HD".to_string()),
        labels: Some("Imported from qBittorrent".to_string()),
        is_watched: false,
        notes: None,
        added_at: 1000,
        completed_on: None,
        save_path: Some("/downloads".to_string()),
        download_limit_kib: 0,
        upload_limit_kib: 0,
        ratio_limit: -1.0,
        sequential_download: true,
        first_last_piece_prio: false,
        super_seeding: false,
        racing_override: None,
        user_display_title: None,
        user_title_locked: false,
        sort_order: None,
        poster_path: None,
        is_external: false,
        sync_miss_count: 0,
        last_checked: None,
    };

    db.upsert_torrent_metadata(&meta).unwrap();
    let stamp = 1730000000u64;
    db.set_torrent_last_checked(test_hash, stamp).unwrap();

    drop(db);

    // Reopen database (simulate restart)
    let reopened = Database::new(&temp_db_path).expect("Failed to reopen db");
    let loaded = reopened.get_torrent_metadata(test_hash).unwrap().unwrap();
    assert_eq!(loaded.last_checked, Some(stamp));

    let _ = fs::remove_file(&temp_db_path);
}

#[test]
fn test_acceptance_backup_sqlite_online_api_and_presence_check() {
    let temp_root = std::env::temp_dir().join(format!("qbit_test_sql_bk_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let qbit_dir = temp_root.join("qbit");
    let backup_root = temp_root.join("backups");
    let _ = fs::create_dir_all(&qbit_dir);
    let _ = fs::create_dir_all(&backup_root);

    let db_path = qbit_dir.join("torrents.db");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE torrents (info_hash TEXT PRIMARY KEY, metadata BLOB, resume_data BLOB)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO torrents (info_hash, metadata, resume_data) VALUES ('hash_test_1', X'0102', X'0304')",
            [],
        )
        .unwrap();
    }

    let t = QbitTorrent {
        info_hash: "hash_test_1".to_string(),
        name: "TestSqlite".to_string(),
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
        total_size: 10,
        progress: 1.0,
        state_string: "Seeding".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: Vec::new(),
        raw_torrent_bytes: vec![1, 2, 3],
        source: QbitSourceType::SqliteRow {
            info_hash: "hash_test_1".to_string(),
            db_path: db_path.clone(),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let manifest = QbitRemover::create_and_verify_backup(&[t], &qbit_dir, &backup_root).unwrap();
    assert_eq!(manifest.entries.len(), 1);

    // Verify backup DB is valid SQLite and contains hash_test_1
    let backed_db = PathBuf::from(&manifest.backup_folder).join("torrents.db");
    assert!(backed_db.is_file());
    let bk_conn = Connection::open(&backed_db).unwrap();
    let check: String = bk_conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check.to_lowercase(), "ok");

    let count: i64 = bk_conn
        .query_row(
            "SELECT COUNT(*) FROM torrents WHERE info_hash = 'hash_test_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_safe_restore_manifest_round_trip() {
    let temp_root = std::env::temp_dir().join(format!("qbit_test_restore_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let qbit_dir = temp_root.join("qbit");
    let bt_backup = qbit_dir.join("BT_backup");
    let backup_root = temp_root.join("backups");
    let _ = fs::create_dir_all(&bt_backup);
    let _ = fs::create_dir_all(&backup_root);

    let valid_hash = "0123456789abcdef0123456789abcdef01234567";
    let t_file = bt_backup.join(format!("{valid_hash}.torrent"));
    let f_file = bt_backup.join(format!("{valid_hash}.fastresume"));
    fs::write(&t_file, b"torrent payload content").unwrap();
    fs::write(&f_file, b"fastresume payload content").unwrap();

    let t = QbitTorrent {
        info_hash: valid_hash.to_string(),
        name: "RestoreTest".to_string(),
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
        total_size: 10,
        progress: 1.0,
        state_string: "Seeding".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: Vec::new(),
        raw_torrent_bytes: vec![1, 2, 3],
        source: QbitSourceType::LegacyFile {
            torrent_path: t_file.clone(),
            fastresume_path: f_file.clone(),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let manifest = QbitRemover::create_and_verify_backup(&[t], &qbit_dir, &backup_root).unwrap();
    let manifest_path = PathBuf::from(&manifest.backup_folder).join("manifest.json");

    // Simulate files deleted
    fs::remove_file(&t_file).unwrap();
    fs::remove_file(&f_file).unwrap();
    assert!(!t_file.exists());
    assert!(!f_file.exists());

    let proc = MockProcessChecker { running: false };

    // Unconfirmed restore must fail
    assert!(QbitRemover::restore_from_manifest(&manifest_path, &proc, false).is_err());

    // Restore when qB is running must fail
    let proc_running = MockProcessChecker { running: true };
    assert!(QbitRemover::restore_from_manifest(&manifest_path, &proc_running, true).is_err());

    // Confirmed restore when qB is closed must succeed
    let restored_count = QbitRemover::restore_from_manifest(&manifest_path, &proc, true).unwrap();
    assert_eq!(restored_count, 2);
    assert!(t_file.exists());
    assert!(f_file.exists());
    assert_eq!(fs::read(&t_file).unwrap(), b"torrent payload content");
    assert_eq!(fs::read(&f_file).unwrap(), b"fastresume payload content");

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_restore_manifest_security_rejects_traversal() {
    let temp_root =
        std::env::temp_dir().join(format!("qbit_test_traversal_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let qbit_dir = temp_root.join("qbit");
    let _ = fs::create_dir_all(&qbit_dir);

    let malicious_manifest = serde_json::json!({
        "timestamp": "20261031-120000",
        "backup_folder": temp_root.to_string_lossy(),
        "qbit_data_dir": qbit_dir.to_string_lossy(),
        "entries": [{
            "info_hash": "0123456789abcdef0123456789abcdef01234567",
            "original_files": ["/etc/shadow"],
            "backup_files": [temp_root.join("fake.torrent").to_string_lossy()],
            "source_type": "Legacy BT_backup",
            "status": "BackedUp"
        }],
        "manual_restore_instructions": "none"
    });

    let fake_torrent = temp_root.join("fake.torrent");
    fs::write(&fake_torrent, b"fake").unwrap();

    let manifest_path = temp_root.join("manifest.json");
    fs::write(&manifest_path, malicious_manifest.to_string()).unwrap();

    let proc = MockProcessChecker { running: false };
    let res = QbitRemover::restore_from_manifest(&manifest_path, &proc, true);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("SECURITY REFUSAL"));

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_rejection_of_silent_permanent_deletion() {
    let provider = DefaultTrashProvider;
    let non_existent = PathBuf::from("/nonexistent/file/test.torrent");
    let res = provider.delete_file(&non_existent);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("prohibited"));
}

#[tokio::test]
async fn test_acceptance_end_to_end_import_with_unsupported_settings() {
    let temp_root = std::env::temp_dir().join(format!("qbit_test_e2e_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let db_path = temp_root.join("nebulae.db");
    let db = Arc::new(Database::new(&db_path).unwrap());
    let engine: SharedEngine = Arc::new(MockTorrentEngine::new());
    let logs_dir = temp_root.join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let save_dir = temp_root.join("downloads");
    let _ = fs::create_dir_all(&save_dir);

    let (bytes, hash) = create_synthetic_metainfo("E2E_Torrent", false, &[("test.mkv", b"test")]);

    let t = QbitTorrent {
        info_hash: hash.clone(),
        name: "E2E_Torrent".to_string(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        category: Some("Anime".to_string()),
        tags: vec!["HD".to_string()],
        content_layout: None,
        paused: true,
        auto_managed: true,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: 2.0,
        seeding_time_limit: Some(7200),
        upload_limit_kib: 100,
        download_limit_kib: 200,
        added_time: 1700000000,
        completed_time: Some(1700003600),
        total_size: 4,
        progress: 1.0,
        state_string: "Paused".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: vec![1],
        raw_torrent_bytes: bytes,
        source: QbitSourceType::LegacyFile {
            torrent_path: temp_root.join("t.torrent"),
            fastresume_path: temp_root.join("t.fastresume"),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let opts = ImportOptions {
        info_hashes: vec![hash.clone()],
        skip_recheck: true,
        remove_from_qbit: false,
    };

    let (summary, imported) = QbitImporter::execute_import(
        vec![t],
        opts,
        &temp_root.join("qbit"),
        &temp_root.join("backups"),
        engine.clone(),
        db.clone(),
        &logs_dir,
    );

    assert_eq!(summary.imported_count, 1);
    assert_eq!(imported.len(), 1);
    assert!(!summary.unsupported_settings.is_empty());
    assert!(summary
        .unsupported_settings
        .iter()
        .any(|s| s.contains("seedingTimeLimit")));
    assert!(summary
        .unsupported_settings
        .iter()
        .any(|s| s.contains("auto_managed")));

    // Verify durable DB persistence
    let persisted = db.get_torrent_metadata(&hash).unwrap().unwrap();
    assert_eq!(persisted.display_title, Some("E2E_Torrent".to_string()));
    assert_eq!(persisted.category, Some("Anime".to_string()));

    let _ = fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn test_acceptance_end_to_end_import_duplicate_skipped() {
    let temp_root = std::env::temp_dir().join(format!("qbit_test_dup_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_root);
    let db_path = temp_root.join("nebulae.db");
    let db = Arc::new(Database::new(&db_path).unwrap());
    let engine: SharedEngine = Arc::new(MockTorrentEngine::new());
    let logs_dir = temp_root.join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let save_dir = temp_root.join("downloads");
    let _ = fs::create_dir_all(&save_dir);

    let (bytes, hash) = create_synthetic_metainfo("Dup_Torrent", false, &[("test.mkv", b"test")]);

    // Pre-populate engine with this torrent
    let _ = engine.add_torrent_from_bytes(&bytes, Some(&save_dir.to_string_lossy()));

    let t = QbitTorrent {
        info_hash: hash.clone(),
        name: "Dup_Torrent".to_string(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        category: None,
        tags: Vec::new(),
        content_layout: None,
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1000,
        completed_time: None,
        total_size: 4,
        progress: 0.0,
        state_string: "Paused".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: vec![1],
        raw_torrent_bytes: bytes,
        source: QbitSourceType::LegacyFile {
            torrent_path: temp_root.join("dup.torrent"),
            fastresume_path: temp_root.join("dup.fastresume"),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let opts = ImportOptions {
        info_hashes: vec![hash.clone()],
        skip_recheck: true,
        remove_from_qbit: false,
    };

    let (summary, imported) = QbitImporter::execute_import(
        vec![t],
        opts,
        &temp_root.join("qbit"),
        &temp_root.join("backups"),
        engine.clone(),
        db.clone(),
        &logs_dir,
    );

    assert_eq!(summary.imported_count, 0);
    assert_eq!(imported.len(), 0);
    assert_eq!(summary.skipped.len(), 1);
    assert_eq!(summary.skipped[0].reason, "Already in Nebula");

    let _ = fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn test_acceptance_end_to_end_removal_failure_withholds_auto_resume() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebula_acceptance_withhold_resume_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();
    let qbit_dir = temp_root.join("qbit");
    fs::create_dir_all(&qbit_dir).unwrap();
    let backups_dir = temp_root.join("backups");
    let logs_dir = temp_root.join("logs");

    let db_path = temp_root.join("test.db");
    let db = Arc::new(Database::new(&db_path).unwrap());
    let engine = Arc::new(MockTorrentEngine::new());

    let (bytes, hash) =
        create_synthetic_metainfo("Active_Torrent", false, &[("Active_Torrent", &[0u8; 1024])]);

    // Payload exists so engine add succeeds
    fs::write(save_dir.join("Active_Torrent"), vec![0u8; 1024]).unwrap();

    // Source points to non-existent file in qbit directory so backup/removal will fail
    let t = QbitTorrent {
        name: "Active_Torrent".to_string(),
        info_hash: hash.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        category: Some("Test".to_string()),
        tags: vec!["Active".to_string()],
        content_layout: None,
        paused: false, // Active in qBittorrent, user wants it running
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        seeding_time_limit: None,
        ratio_limit: -1.0,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1000,
        completed_time: Some(2000),
        total_size: 1024,
        progress: 1.0,
        state_string: "Seeding".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: vec![1],
        raw_torrent_bytes: bytes,
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_dir.join("nonexistent.torrent"),
            fastresume_path: qbit_dir.join("nonexistent.fastresume"),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let opts = ImportOptions {
        info_hashes: vec![hash.clone()],
        skip_recheck: true,
        remove_from_qbit: true, // User requested removal
    };

    let (summary, imported) = QbitImporter::execute_import(
        vec![t],
        opts,
        &qbit_dir,
        &backups_dir,
        engine.clone(),
        db.clone(),
        &logs_dir,
    );

    // 1. Import itself succeeded
    assert_eq!(summary.imported_count, 1);
    assert_eq!(imported.len(), 1);

    // 2. Removal failed because source files were missing
    assert_eq!(summary.removed_count, 0);
    assert_eq!(summary.not_removed.len(), 1);
    assert_eq!(summary.not_removed[0].info_hash, hash);

    // 3. Engine verification: Torrent MUST REMAIN PAUSED to prevent dual-client collision
    let list = engine.list_torrents().unwrap();
    let torrent_item = list
        .iter()
        .find(|item| item.info_hash.eq_ignore_ascii_case(&hash))
        .unwrap();
    assert_eq!(
        torrent_item.status, "Paused",
        "Torrent whose removal failed must NOT be auto-resumed!"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

fn recursive_inventory(dir: &Path) -> BTreeMap<String, (u64, String)> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file() {
                    let rel = p.strip_prefix(dir).unwrap().to_string_lossy().to_string();
                    let bytes = fs::read(&p).unwrap_or_default();
                    let hash = Sha1::digest(&bytes)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    out.insert(rel, (bytes.len() as u64, hash));
                }
            }
        }
    }
    out
}

#[test]
fn test_acceptance_sqlite_selective_restoration_preserves_unrelated_torrents() {
    let temp_root = std::env::temp_dir().join(format!(
        "qbit_sqlite_selective_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let qbit_dir = temp_root.join("qbit");
    fs::create_dir_all(&qbit_dir).unwrap();
    let backup_root = temp_root.join("backups");
    fs::create_dir_all(&backup_root).unwrap();

    let db_path = qbit_dir.join("torrents.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE torrents (id TEXT PRIMARY KEY, torrent BLOB, resume BLOB);
         INSERT INTO torrents VALUES ('1111111111111111111111111111111111111111', X'1234', X'5678');"
    ).unwrap();
    drop(conn);

    let torrent_a = QbitTorrent {
        name: "TorrentA".to_string(),
        info_hash: "1111111111111111111111111111111111111111".to_string(),
        save_path: "/downloads".to_string(),
        download_path: None,
        category: None,
        tags: Vec::new(),
        content_layout: None,
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1000,
        completed_time: None,
        total_size: 10,
        progress: 1.0,
        state_string: "Paused".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: Vec::new(),
        raw_torrent_bytes: vec![1, 2, 3],
        source: QbitSourceType::SqliteRow {
            db_path: db_path.clone(),
            info_hash: "1111111111111111111111111111111111111111".to_string(),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    // 1. Create and verify backup
    let manifest =
        QbitRemover::create_and_verify_backup(&[torrent_a], &qbit_dir, &backup_root).unwrap();
    let manifest_path = PathBuf::from(&manifest.backup_folder).join("manifest.json");

    // 2. Simulate removal of Torrent A from active torrents.db
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "DELETE FROM torrents WHERE id = '1111111111111111111111111111111111111111'",
        [],
    )
    .unwrap();

    // 3. User adds unrelated Torrent B into active torrents.db after backup
    conn.execute(
        "INSERT INTO torrents VALUES ('2222222222222222222222222222222222222222', X'AAAA', X'BBBB')",
        [],
    ).unwrap();
    drop(conn);

    let proc = MockProcessChecker { running: false };

    // 4. Execute Selective Restore of Torrent A
    let restored = QbitRemover::restore_from_manifest(&manifest_path, &proc, true).unwrap();
    assert_eq!(restored, 1);

    // 5. Verify both Torrent A and Torrent B are present in active torrents.db (Torrent B survived!)
    let conn = Connection::open(&db_path).unwrap();
    let mut stmt = conn.prepare("SELECT id FROM torrents ORDER BY id").unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "1111111111111111111111111111111111111111");
    assert_eq!(rows[1], "2222222222222222222222222222222222222222");

    // 6. Test conflict prevention: modify Torrent A in active db and restore again
    conn.execute(
        "UPDATE torrents SET torrent = X'9999' WHERE id = '1111111111111111111111111111111111111111'",
        [],
    ).unwrap();
    drop(conn);

    let restored_conflict =
        QbitRemover::restore_from_manifest(&manifest_path, &proc, true).unwrap();
    assert_eq!(
        restored_conflict, 0,
        "Conflicting existing entry must NOT be overwritten!"
    );

    let conn = Connection::open(&db_path).unwrap();
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT torrent FROM torrents WHERE id = '1111111111111111111111111111111111111111'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        blob,
        vec![0x99, 0x99],
        "Existing entry data was preserved without silent overwrite!"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_whole_profile_rollback_requires_explicit_confirmation() {
    let temp_root = std::env::temp_dir().join(format!(
        "qbit_whole_profile_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let qbit_dir = temp_root.join("qbit");
    fs::create_dir_all(&qbit_dir).unwrap();
    let backup_root = temp_root.join("backups");
    fs::create_dir_all(&backup_root).unwrap();

    let db_path = qbit_dir.join("torrents.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE torrents (id TEXT PRIMARY KEY, torrent BLOB, resume BLOB);
         INSERT INTO torrents VALUES ('1111111111111111111111111111111111111111', X'11', X'11');",
    )
    .unwrap();
    drop(conn);

    let torrent_a = QbitTorrent {
        name: "TorrentA".to_string(),
        info_hash: "1111111111111111111111111111111111111111".to_string(),
        save_path: "/downloads".to_string(),
        download_path: None,
        category: None,
        tags: Vec::new(),
        content_layout: None,
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1000,
        completed_time: None,
        total_size: 10,
        progress: 1.0,
        state_string: "Paused".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: Vec::new(),
        raw_torrent_bytes: vec![1, 2, 3],
        source: QbitSourceType::SqliteRow {
            db_path: db_path.clone(),
            info_hash: "1111111111111111111111111111111111111111".to_string(),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let manifest =
        QbitRemover::create_and_verify_backup(&[torrent_a], &qbit_dir, &backup_root).unwrap();
    let manifest_path = PathBuf::from(&manifest.backup_folder).join("manifest.json");

    let proc = MockProcessChecker { running: false };

    // Mode with invalid confirmation phrase must fail
    let err = QbitRemover::restore_from_manifest_with_mode(
        &manifest_path,
        &proc,
        true,
        RestoreMode::WholeProfileRollback {
            confirmation_phrase: "Confirming restore".to_string(),
        },
    );
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("Confirmation phrase mismatch"));

    // Mode with exact confirmation phrase must succeed
    let res = QbitRemover::restore_from_manifest_with_mode(
        &manifest_path,
        &proc,
        true,
        RestoreMode::WholeProfileRollback {
            confirmation_phrase: WHOLE_PROFILE_ROLLBACK_CONFIRMATION_PHRASE.to_string(),
        },
    );
    assert!(res.is_ok());

    // Verify pre-restore safety copy was created
    let pre_restore_exists = fs::read_dir(&qbit_dir).unwrap().flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .contains("torrents.db.pre_restore_")
    });
    assert!(pre_restore_exists, "Pre-restore safety snapshot must exist");

    let _ = fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn test_acceptance_rename_maps_without_modifying_payload_and_detects_collisions() {
    let temp_root = std::env::temp_dir().join(format!(
        "qbit_rename_inventory_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();
    let qbit_dir = temp_root.join("qbit");
    fs::create_dir_all(&qbit_dir).unwrap();
    let backups_dir = temp_root.join("backups");
    let logs_dir = temp_root.join("logs");

    let db_path = temp_root.join("test.db");
    let db = Arc::new(Database::new(&db_path).unwrap());
    let engine = Arc::new(MockTorrentEngine::new());

    // Metainfo has root 'Series_Original' and file 'Season 1/episode01.mkv'
    let (bytes, hash) = create_synthetic_metainfo(
        "Series_Original",
        true,
        &[("Season 1/episode01.mkv", b"PAYLOAD_VIDEO_DATA_42")],
    );

    // Payload already downloaded on disk by qBittorrent under mapped names:
    // save_dir/Series_Mapped/S01/S01E01.mkv
    let target_file = save_dir
        .join("Series_Mapped")
        .join("S01")
        .join("S01E01.mkv");
    fs::create_dir_all(target_file.parent().unwrap()).unwrap();
    fs::write(&target_file, b"PAYLOAD_VIDEO_DATA_42").unwrap();

    // Record recursive inventory BEFORE import
    let inventory_before = recursive_inventory(&save_dir);
    assert_eq!(inventory_before.len(), 1);

    let t = QbitTorrent {
        name: "Series_Original".to_string(),
        info_hash: hash.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        category: Some("TV".to_string()),
        tags: vec!["1080p".to_string()],
        content_layout: Some("Original".to_string()),
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1000,
        completed_time: Some(2000),
        total_size: b"PAYLOAD_VIDEO_DATA_42".len() as u64,
        progress: 1.0,
        state_string: "Paused".to_string(),
        trackers: Vec::new(),
        mapped_files: vec!["Series_Mapped/S01/S01E01.mkv".to_string()],
        file_priorities: vec![1],
        raw_torrent_bytes: bytes,
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_dir.join("t.torrent"),
            fastresume_path: qbit_dir.join("t.fastresume"),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let opts = ImportOptions {
        info_hashes: vec![hash.clone()],
        skip_recheck: true,
        remove_from_qbit: false,
    };

    let (summary, imported) = QbitImporter::execute_import(
        vec![t],
        opts,
        &qbit_dir,
        &backups_dir,
        engine.clone(),
        db.clone(),
        &logs_dir,
    );

    assert_eq!(summary.imported_count, 1);
    assert_eq!(imported.len(), 1);

    // Record recursive inventory AFTER import
    let inventory_after = recursive_inventory(&save_dir);

    // Assert: ZERO payload paths, sizes, or SHA-1 hashes changed!
    assert_eq!(
        inventory_before, inventory_after,
        "Import must not move, rename, or modify downloaded payload files on disk!"
    );

    // Test collision between original and mapped paths
    let orig_paths = vec![
        "Root/track01.flac".to_string(),
        "Root/track02.flac".to_string(),
    ];
    // Both mapped to track01.flac (collision!)
    let colliding_mapped = vec![
        "Root/track01.flac".to_string(),
        "Root/track01.flac".to_string(),
    ];
    let ops = QbitMapper::compute_rename_operations(
        "Root",
        &orig_paths,
        &colliding_mapped,
        &ContentLayout::Original,
    );
    assert!(
        ops.is_empty(),
        "Colliding mapped files must be rejected and not produce rename operations!"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn test_acceptance_genuine_hash_checking_and_last_checked_states() {
    let temp_root = std::env::temp_dir().join(format!(
        "qbit_genuine_check_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();

    let db_path = temp_root.join("test.db");
    let db = Arc::new(Database::new(&db_path).unwrap());
    let engine = Arc::new(MockTorrentEngine::new());

    // 1. Correct synthetic payload
    let (good_bytes, good_hash) =
        create_synthetic_metainfo("GoodTorrent", false, &[("GoodTorrent", &[0x42u8; 1024])]);
    let good_file = save_dir.join("GoodTorrent");
    fs::write(&good_file, vec![0x42u8; 1024]).unwrap();

    // 2. Corrupt synthetic payload (contains CORRUPT_PAYLOAD marker)
    let (corrupt_bytes, corrupt_hash) = create_synthetic_metainfo(
        "CorruptTorrent",
        false,
        &[("CorruptTorrent", &[0x55u8; 1024])],
    );
    let corrupt_file = save_dir.join("CorruptTorrent");
    let mut corrupt_data = vec![0x55u8; 1024];
    corrupt_data[10..25].copy_from_slice(b"CORRUPT_PAYLOAD");
    fs::write(&corrupt_file, corrupt_data).unwrap();

    // 3. Missing payload path
    let (missing_bytes, missing_hash) = create_synthetic_metainfo(
        "MissingTorrent",
        false,
        &[("MissingTorrent", &[0x77u8; 1024])],
    );
    let missing_path = temp_root.join("nonexistent_dir").join("MissingTorrent");

    // Add all 3 torrents to engine
    engine
        .add_torrent_from_bytes(&good_bytes, Some(save_dir.to_str().unwrap()))
        .unwrap();
    engine
        .add_torrent_from_bytes(&corrupt_bytes, Some(save_dir.to_str().unwrap()))
        .unwrap();
    engine
        .add_torrent_from_bytes(&missing_bytes, Some(missing_path.to_str().unwrap()))
        .unwrap();

    // Ensure all 3 start paused
    engine.pause_torrent(&good_hash).unwrap();
    engine.pause_torrent(&corrupt_hash).unwrap();
    engine.pause_torrent(&missing_hash).unwrap();

    // Persist initial metadata without last_checked
    for h in &[&good_hash, &corrupt_hash, &missing_hash] {
        let meta = TorrentDbMetadata {
            info_hash: h.to_lowercase(),
            display_title: Some("Test".to_string()),
            category: None,
            labels: None,
            is_watched: false,
            notes: None,
            added_at: 1000,
            completed_on: None,
            save_path: if **h == missing_hash {
                Some(missing_path.to_string_lossy().to_string())
            } else {
                Some(save_dir.to_string_lossy().to_string())
            },
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            racing_override: None,
            user_display_title: None,
            user_title_locked: false,
            sort_order: None,
            poster_path: None,
            is_external: false,
            sync_miss_count: 0,
            last_checked: None,
        };
        db.upsert_torrent_metadata(&meta).unwrap();
    }

    // Take payload inventory before checking
    let inventory_before = recursive_inventory(&save_dir);

    // Run recheck queue: should_resume = true for all
    let items = vec![
        (good_hash.clone(), true),
        (corrupt_hash.clone(), true),
        (missing_hash.clone(), true),
    ];
    run_recheck_queue(items, engine.clone(), db.clone()).await;

    // Verify 1: Correct payload reached 100% verified progress and received last_checked
    let good_meta = db
        .get_torrent_metadata(&good_hash.to_lowercase())
        .unwrap()
        .unwrap();
    assert!(
        good_meta.last_checked.is_some(),
        "Good torrent must receive last_checked"
    );
    let good_stamp = good_meta.last_checked.unwrap();
    assert!(good_stamp > 0);

    let list = engine.list_torrents().unwrap();
    let good_item = list
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&good_hash))
        .unwrap();
    assert_eq!(
        good_item.progress, 1.0,
        "Good torrent reaches 100% progress"
    );

    // Verify 2: Corrupt payload is NOT reported as 100%, and is NOT resumed to active seeding
    let corrupt_meta = db
        .get_torrent_metadata(&corrupt_hash.to_lowercase())
        .unwrap()
        .unwrap();
    assert!(
        corrupt_meta.last_checked.is_some(),
        "Corrupt torrent completed check, received last_checked"
    );
    let corrupt_item = list
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&corrupt_hash))
        .unwrap();
    assert!(
        corrupt_item.progress < 1.0,
        "Corrupt payload is NOT reported as 100% (was {:.2})",
        corrupt_item.progress
    );
    assert_eq!(
        corrupt_item.status, "Paused",
        "Corrupt payload remains Paused; never auto-resumed"
    );

    // Verify 3: Missing path does NOT receive completed-check timestamp and transitions to Error
    let missing_meta = db
        .get_torrent_metadata(&missing_hash.to_lowercase())
        .unwrap()
        .unwrap();
    assert!(
        missing_meta.last_checked.is_none(),
        "Unavailable path must NOT receive last_checked timestamp"
    );
    let missing_item = list
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&missing_hash))
        .unwrap();
    assert_eq!(
        missing_item.status, "Error",
        "Missing path torrent transitions to Error status"
    );

    // Verify 4: Timestamps survive restart (reopening DB connection)
    drop(db);
    let restarted_db = Database::new(&db_path).unwrap();
    let reloaded_good = restarted_db
        .get_torrent_metadata(&good_hash.to_lowercase())
        .unwrap()
        .unwrap();
    assert_eq!(
        reloaded_good.last_checked,
        Some(good_stamp),
        "last_checked timestamp survives database restart"
    );

    // Verify 5: Payload files on disk were not downloaded, moved, or renamed during hash check
    let inventory_after = recursive_inventory(&save_dir);
    assert_eq!(
        inventory_before, inventory_after,
        "Hash check must not modify payload files on disk!"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_direct_fastresume_folder_scan_and_tilde() {
    let temp_root = std::env::temp_dir().join(format!("test_qbit_direct_{}", uuid::Uuid::new_v4()));
    let _ = fs::create_dir_all(&temp_root);

    // Create a custom direct folder with .fastresume and .torrent files directly inside (no BT_backup subfolder)
    let direct_folder = temp_root.join("my_qbt_backup");
    let _ = fs::create_dir_all(&direct_folder);

    let (bytes, hash) =
        create_synthetic_metainfo("DirectScan_Movie", false, &[("movie.mkv", b"test content")]);
    let torrent_file = direct_folder.join(format!("{hash}.torrent"));
    let fastresume_file = direct_folder.join(format!("{hash}.fastresume"));
    fs::write(&torrent_file, &bytes).unwrap();
    fs::write(&fastresume_file, b"d12:qBt-savePath15:/path/to/moviese").unwrap();

    let locator = DefaultLocator::new();
    let (has_sqlite, has_legacy) = DefaultLocator::inspect_dir(&direct_folder);
    assert!(
        has_legacy,
        "inspect_dir must recognize folder with direct .fastresume files"
    );
    assert!(!has_sqlite);

    let validated = locator
        .validate_dir(&direct_folder)
        .expect("validate_dir must succeed on direct fastresume folder");
    assert_eq!(validated.data_dir, direct_folder);

    let category_paths = HashMap::new();
    let read_torrents = QbitReader::read_legacy_bt_backup(&direct_folder, &category_paths);
    assert_eq!(
        read_torrents.len(),
        1,
        "read_legacy_bt_backup must read direct fastresume directory"
    );
    assert_eq!(
        read_torrents[0].info_hash.to_lowercase(),
        hash.to_lowercase()
    );

    // Tilde expansion test
    if let Some(_home) = dirs::home_dir() {
        let tilde_path = PathBuf::from("~/");
        let validated_tilde = locator.validate_dir(&tilde_path);
        assert!(
            validated_tilde.is_err(),
            "Home dir alone lacks torrents, but should error on content, not 'does not exist'"
        );
        let err_str = validated_tilde.unwrap_err();
        assert!(
            !err_str.contains("Directory does not exist"),
            "Tilde path must be expanded to real home dir: {err_str}"
        );
    }

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_acceptance_import_interruption_preserves_existing_database_records() {
    let temp_root =
        std::env::temp_dir().join(format!("test_qbit_interrupt_{}", uuid::Uuid::new_v4()));
    let _ = fs::create_dir_all(&temp_root);
    let db_path = temp_root.join("nebulae.db");
    let db = Arc::new(Database::new(&db_path).unwrap());

    // Step 1: Pre-populate database with pre-existing user records
    let existing_hashes = [
        "1111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222",
    ];
    for (i, h) in existing_hashes.iter().enumerate() {
        let meta = TorrentDbMetadata {
            info_hash: h.to_string(),
            display_title: Some(format!("Existing Movie {i}")),
            category: Some("Movies".to_string()),
            labels: Some("pre-existing".to_string()),
            is_watched: true,
            notes: Some("Important user note".to_string()),
            added_at: 1700000000 + i as u64,
            completed_on: Some(1700001000),
            save_path: Some(format!("/downloads/movie_{i}")),
            download_limit_kib: 100,
            upload_limit_kib: 50,
            ratio_limit: 2.0,
            sequential_download: true,
            first_last_piece_prio: true,
            super_seeding: false,
            racing_override: Some(true),
            user_display_title: Some(format!("Custom Renamed Movie {i}")),
            user_title_locked: true,
            sort_order: Some(i as i64),
            poster_path: Some(format!("/posters/movie_{i}.jpg")),
            is_external: false,
            sync_miss_count: 0,
            last_checked: Some(1700002000),
        };
        db.upsert_torrent_metadata(&meta).unwrap();
        db.set_user_display_title(h, &format!("Custom Renamed Movie {i}"), true)
            .unwrap();
        db.set_torrent_poster(h, &format!("/posters/movie_{i}.jpg"))
            .unwrap();
    }

    // Verify initial state
    assert_eq!(db.list_all_torrent_metadata().unwrap().len(), 2);

    // Step 2: Prepare new import batch of 3 torrents
    let (b1, h1) = create_synthetic_metainfo("Batch_Item_1", false, &[("f1.mkv", b"content1")]);
    let (b2, h2) = create_synthetic_metainfo("Batch_Item_2", false, &[("f2.mkv", b"content2")]);
    let (b3, h3) = create_synthetic_metainfo("Batch_Item_3", false, &[("f3.mkv", b"content3")]);

    let save_dir = temp_root.join("downloads");
    let _ = fs::create_dir_all(&save_dir);

    let create_t = |name: &str, h: &str, bytes: &[u8]| QbitTorrent {
        info_hash: h.to_string(),
        name: name.to_string(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        category: Some("New".to_string()),
        tags: vec!["new".to_string()],
        content_layout: None,
        paused: true,
        auto_managed: false,
        sequential_download: false,
        first_last_piece_prio: false,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        added_time: 1700005000,
        completed_time: Some(1700006000),
        total_size: bytes.len() as u64,
        progress: 1.0,
        state_string: "Seeding".to_string(),
        trackers: Vec::new(),
        mapped_files: Vec::new(),
        file_priorities: vec![1],
        raw_torrent_bytes: bytes.to_vec(),
        source: QbitSourceType::LegacyFile {
            torrent_path: temp_root.join(format!("{h}.torrent")),
            fastresume_path: temp_root.join(format!("{h}.fastresume")),
        },
        already_in_nebula: false,
        missing_path: false,
    };

    let batch = vec![
        create_t("Batch_Item_1", &h1, &b1),
        create_t("Batch_Item_2", &h2, &b2),
        create_t("Batch_Item_3", &h3, &b3),
    ];

    let engine: SharedEngine = Arc::new(MockTorrentEngine::new());
    let logs_dir = temp_root.join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let options = ImportOptions {
        info_hashes: vec![h1.clone(), h2.clone(), h3.clone()],
        skip_recheck: true,
        remove_from_qbit: false,
    };

    // Step 3: Simulate import interruption!
    // We execute import of only the first 2 items (simulating a kill / interruption before item 3)
    let partial_batch = vec![batch[0].clone(), batch[1].clone()];
    let (report1, _) = QbitImporter::execute_import(
        partial_batch,
        options.clone(),
        &temp_root,
        &temp_root.join("backup"),
        engine.clone(),
        db.clone(),
        &logs_dir,
    );
    assert_eq!(report1.imported_count, 2);

    // Step 4: Simulate application crash / force-quit and restart
    // Drop database connection and recreate connection from disk file
    drop(db);
    let restarted_db = Arc::new(Database::new(&db_path).unwrap());

    // CRITICAL ASSERTION: All pre-existing user records remain 100% intact!
    for (i, h) in existing_hashes.iter().enumerate() {
        let meta = restarted_db
            .get_torrent_metadata(h)
            .unwrap()
            .expect("Existing record must survive");
        assert_eq!(
            meta.user_display_title,
            Some(format!("Custom Renamed Movie {i}"))
        );
        assert!(
            meta.user_title_locked,
            "user_title_locked must be preserved across interruption"
        );
        assert_eq!(
            meta.poster_path,
            Some(format!("/posters/movie_{i}.jpg")),
            "poster_path must be preserved"
        );
        assert_eq!(meta.notes, Some("Important user note".to_string()));
    }

    // Verify imported canonical records are present in DB
    assert!(restarted_db
        .get_torrent_metadata(&h1.to_lowercase())
        .unwrap()
        .is_some());
    assert!(restarted_db
        .get_torrent_metadata(&h2.to_lowercase())
        .unwrap()
        .is_some());

    // Step 5: Retry the full batch import (items 1, 2, 3)
    // Items 1 and 2 already exist in engine; item 3 is new
    let (report2, _) = QbitImporter::execute_import(
        batch,
        options,
        &temp_root,
        &temp_root.join("backup"),
        engine.clone(),
        restarted_db.clone(),
        &logs_dir,
    );

    // Assert: Items 1 and 2 were skipped as duplicates, item 3 was imported
    assert_eq!(
        report2.skipped.len(),
        2,
        "Previously imported items must be skipped"
    );
    assert_eq!(
        report2.imported_count, 1,
        "Only remaining item 3 should be imported"
    );

    // All 3 canonical imported hashes exist plus 2 pre-existing
    for h in &[h1.to_lowercase(), h2.to_lowercase(), h3.to_lowercase()] {
        assert!(
            restarted_db.get_torrent_metadata(h).unwrap().is_some(),
            "Canonical imported hash must exist"
        );
    }
    for (i, h) in existing_hashes.iter().enumerate() {
        let meta = restarted_db
            .get_torrent_metadata(h)
            .unwrap()
            .expect("Existing record must survive");
        assert_eq!(
            meta.user_display_title,
            Some(format!("Custom Renamed Movie {i}"))
        );
    }

    let _ = fs::remove_dir_all(&temp_root);
}

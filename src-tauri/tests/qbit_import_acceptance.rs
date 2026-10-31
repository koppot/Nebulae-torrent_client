use nebulae_torrent_client_lib::db::{Database, TorrentDbMetadata};
use nebulae_torrent_client_lib::engine::{MockTorrentEngine, SharedEngine, TorrentEngine};
use nebulae_torrent_client_lib::qbit_import::bencode::BVal;
use nebulae_torrent_client_lib::qbit_import::mapper::{ContentLayout, QbitMapper, RenameOperation};
use nebulae_torrent_client_lib::qbit_import::remover::{
    DefaultTrashProvider, ProcessChecker, QbitRemover, TrashMethod, TrashProvider,
};
use nebulae_torrent_client_lib::qbit_import::types::{ImportOptions, QbitSourceType, QbitTorrent};
use nebulae_torrent_client_lib::qbit_import::QbitImporter;
use rusqlite::Connection;
use sha1::{Digest, Sha1};
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
    )
    .await;

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
    )
    .await;

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
    )
    .await;

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

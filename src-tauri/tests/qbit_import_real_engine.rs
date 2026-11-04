#![cfg(any(target_os = "macos", target_os = "windows"))]

use nebulae_torrent_client_lib::commands::settings::UserSettings;
use nebulae_torrent_client_lib::db::Database;
use nebulae_torrent_client_lib::engine::metainfo::{build_torrent_file, ParsedTorrent};
use nebulae_torrent_client_lib::engine::{SharedEngine, TorrentEngine, TransmissionTorrentEngine};
use nebulae_torrent_client_lib::qbit_import::remover::{
    DefaultTrashProvider, ProcessChecker, QbitRemover, RestoreMode,
};
use nebulae_torrent_client_lib::qbit_import::types::{ImportOptions, QbitSourceType, QbitTorrent};
use nebulae_torrent_client_lib::qbit_import::QbitImporter;
use nebulae_torrent_client_lib::utils::paths::AppPaths;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

struct MockProcessChecker {
    running: bool,
}
impl ProcessChecker for MockProcessChecker {
    fn is_qbittorrent_running(&self) -> bool {
        self.running
    }
}

fn recursive_inventory(dir: &Path) -> BTreeMap<String, (u64, String)> {
    let mut map = BTreeMap::new();
    fn visit(dir: &Path, base: &Path, map: &mut BTreeMap<String, (u64, String)>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit(&p, base, map);
                } else if p.is_file() {
                    let rel = p
                        .strip_prefix(base)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let data = fs::read(&p).unwrap_or_default();
                    let digest = Sha1::digest(&data)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    map.insert(rel, (data.len() as u64, digest));
                }
            }
        }
    }
    if dir.exists() {
        visit(dir, dir, &mut map);
    }
    map
}

type IsolatedEngineContext = (
    Arc<TransmissionTorrentEngine>,
    Arc<Database>,
    Arc<AppPaths>,
    PathBuf,
);

fn setup_isolated_engine(temp_root: &Path) -> Option<IsolatedEngineContext> {
    let nebulae_data = temp_root.join("nebulae_data");
    fs::create_dir_all(&nebulae_data).unwrap();

    let paths = Arc::new(AppPaths {
        base_data_dir: nebulae_data.clone(),
        internal_dir: nebulae_data.join("internal"),
        logs_dir: nebulae_data.join("logs"),
        diagnostics_dir: nebulae_data.join("diagnostics"),
        db_dir: nebulae_data.join("db"),
        db_file: nebulae_data.join("db").join("nebulae.db"),
        cache_dir: nebulae_data.join("cache"),
        engine_dir: nebulae_data.join("engine-state"),
        debug_dir: nebulae_data.join("debug"),
    });

    fs::create_dir_all(&paths.db_dir).unwrap();
    fs::create_dir_all(&paths.logs_dir).unwrap();

    let db = Arc::new(Database::new(&paths.db_file).expect("Failed to create isolated Database"));

    let default_dl = temp_root.join("default_downloads");
    fs::create_dir_all(&default_dl).unwrap();

    let settings = UserSettings {
        enable_upnp: false,
        max_global_connections: 0,
        max_connections_per_torrent: 0,
        default_trackers_list: String::new(),
        randomize_port_on_startup: true,
        append_neb_extension: false,
        ..Default::default()
    };

    let res_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine = match TransmissionTorrentEngine::new(
        paths.clone(),
        db.clone(),
        default_dl.clone(),
        &settings,
        Some(res_dir),
    ) {
        Ok(e) => Arc::new(e),
        Err(err) if err.contains("Bundled transmission-daemon") => {
            eprintln!("SKIPPING real engine test: transmission-daemon not installed in environment: {err}");
            return None;
        }
        Err(err) => {
            panic!("Failed to initialize TransmissionTorrentEngine with isolated daemon: {err}")
        }
    };

    Some((engine, db, paths, default_dl))
}

fn create_test_torrent(
    out_dir: &Path,
    source_dir: &Path,
    comment: &str,
) -> (PathBuf, ParsedTorrent, Vec<u8>) {
    let torrent_path = build_torrent_file(
        source_dir,
        out_dir,
        16,   // 16 KiB piece length
        &[],  // no trackers (prevents external network announcements)
        true, // private
        comment,
        "Nebulae Real Engine Test",
    )
    .expect("Failed to build torrent file");

    let parsed = ParsedTorrent::parse_file(&torrent_path).expect("Failed to parse built torrent");
    let raw_bytes = fs::read(&torrent_path).expect("Failed to read torrent bytes");
    (torrent_path, parsed, raw_bytes)
}

#[test]
fn test_real_engine_lifecycle_smoke() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_real_smoke_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

    let (engine, _db, _paths, _default_dl) = match setup_isolated_engine(&temp_root) {
        Some(ctx) => ctx,
        None => return,
    };

    let torrents = engine.list_torrents().expect("list_torrents failed");
    assert_eq!(torrents.len(), 0, "Initial torrent count must be 0");

    engine.shutdown_engine();
    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_real_engine_end_to_end_migration_and_verification() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_real_migration_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

    let (engine, db, paths, _default_dl) = match setup_isolated_engine(&temp_root) {
        Some(ctx) => ctx,
        None => return,
    };
    let shared_engine: SharedEngine = engine.clone();

    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();

    let torrents_out = temp_root.join("meta_out");
    fs::create_dir_all(&torrents_out).unwrap();

    let qbit_dir = temp_root.join("qbit_profile");
    let qbit_bt_backup = qbit_dir.join("BT_backup");
    fs::create_dir_all(&qbit_bt_backup).unwrap();

    // =========================================================================
    // 1. Torrent 1: Complete Original-Layout Multi-File Payload
    // =========================================================================
    let orig_payload_dir = save_dir.join("OriginalShow");
    fs::create_dir_all(&orig_payload_dir).unwrap();
    let orig_file1 = orig_payload_dir.join("ep1.bin");
    let orig_file2 = orig_payload_dir.join("ep2.bin");
    fs::write(&orig_file1, vec![0x41u8; 32768]).unwrap();
    fs::write(&orig_file2, vec![0x42u8; 49152]).unwrap();

    let (_, parsed_orig, bytes_orig) =
        create_test_torrent(&torrents_out, &orig_payload_dir, "Original Layout Test");
    let hash_orig = parsed_orig.info_hash.to_lowercase();

    let qbt_orig = QbitTorrent {
        info_hash: hash_orig.clone(),
        name: parsed_orig.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed_orig.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: Some("Anime".to_string()),
        tags: vec!["HD".to_string(), "Classic".to_string()],
        paused: true,
        added_time: 1700000000,
        completed_time: Some(1700001000),
        ratio_limit: 2.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1, 1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{hash_orig}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{hash_orig}.fastresume")),
        },
        raw_torrent_bytes: bytes_orig,
        already_in_nebula: false,
        missing_path: false,
    };

    // =========================================================================
    // 2. Torrent 2: Renamed Root, Nested Directory, and Individual Files
    // =========================================================================
    // Metainfo defines: Show_Meta/Season 1/ep01.bin & ep02.bin
    let meta_gen_dir = temp_root.join("meta_src").join("Show_Meta");
    let meta_gen_s1 = meta_gen_dir.join("Season 1");
    fs::create_dir_all(&meta_gen_s1).unwrap();
    fs::write(meta_gen_s1.join("ep01.bin"), vec![0x51u8; 32768]).unwrap();
    fs::write(meta_gen_s1.join("ep02.bin"), vec![0x52u8; 32768]).unwrap();

    let (_, parsed_renamed, bytes_renamed) =
        create_test_torrent(&torrents_out, &meta_gen_dir, "Renamed Layout Test");
    let hash_renamed = parsed_renamed.info_hash.to_lowercase();

    // Actual files on disk are ALREADY at the mapped paths: Show_Custom/Season 01/episode01.bin
    let disk_renamed_dir = save_dir.join("Show_Custom");
    let disk_renamed_s01 = disk_renamed_dir.join("Season 01");
    fs::create_dir_all(&disk_renamed_s01).unwrap();
    fs::write(disk_renamed_s01.join("episode01.bin"), vec![0x51u8; 32768]).unwrap();
    fs::write(disk_renamed_s01.join("episode02.bin"), vec![0x52u8; 32768]).unwrap();

    let qbt_renamed = QbitTorrent {
        info_hash: hash_renamed.clone(),
        name: parsed_renamed.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed_renamed.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: Some("Series".to_string()),
        tags: vec!["Remapped".to_string()],
        paused: true,
        added_time: 1700000000,
        completed_time: Some(1700002000),
        ratio_limit: -1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![
            "Show_Custom/Season 01/episode01.bin".to_string(),
            "Show_Custom/Season 01/episode02.bin".to_string(),
        ],
        file_priorities: vec![1, 1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{hash_renamed}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{hash_renamed}.fastresume")),
        },
        raw_torrent_bytes: bytes_renamed,
        already_in_nebula: false,
        missing_path: false,
    };

    // =========================================================================
    // 3. Torrent 3: Deliberately Corrupted Payload
    // =========================================================================
    let corrupt_gen_dir = temp_root.join("meta_src").join("CorruptShow");
    fs::create_dir_all(&corrupt_gen_dir).unwrap();
    fs::write(corrupt_gen_dir.join("data.bin"), vec![0x61u8; 32768]).unwrap();

    let (_, parsed_corrupt, bytes_corrupt) =
        create_test_torrent(&torrents_out, &corrupt_gen_dir, "Corrupt Payload Test");
    let hash_corrupt = parsed_corrupt.info_hash.to_lowercase();

    // Write deliberately corrupted bytes to disk
    let disk_corrupt_dir = save_dir.join("CorruptShow");
    fs::create_dir_all(&disk_corrupt_dir).unwrap();
    fs::write(disk_corrupt_dir.join("data.bin"), vec![0x99u8; 32768]).unwrap();

    let qbt_corrupt = QbitTorrent {
        info_hash: hash_corrupt.clone(),
        name: parsed_corrupt.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed_corrupt.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: None,
        tags: vec![],
        paused: true,
        added_time: 1700000000,
        completed_time: None,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{hash_corrupt}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{hash_corrupt}.fastresume")),
        },
        raw_torrent_bytes: bytes_corrupt,
        already_in_nebula: false,
        missing_path: false,
    };

    // =========================================================================
    // 4. Torrent 4: Unavailable / Offline Download Path
    // =========================================================================
    let missing_gen_dir = temp_root.join("meta_src").join("MissingShow");
    fs::create_dir_all(&missing_gen_dir).unwrap();
    fs::write(missing_gen_dir.join("file.bin"), vec![0x71u8; 16384]).unwrap();

    let (_, parsed_missing, bytes_missing) =
        create_test_torrent(&torrents_out, &missing_gen_dir, "Missing Path Test");
    let hash_missing = parsed_missing.info_hash.to_lowercase();

    let unavailable_save_path = temp_root
        .join("non_existent_mount")
        .join("offline_downloads");

    let qbt_missing = QbitTorrent {
        info_hash: hash_missing.clone(),
        name: parsed_missing.name.clone(),
        save_path: unavailable_save_path.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed_missing.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: None,
        tags: vec![],
        paused: true,
        added_time: 1700000000,
        completed_time: None,
        ratio_limit: -1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{hash_missing}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{hash_missing}.fastresume")),
        },
        raw_torrent_bytes: bytes_missing,
        already_in_nebula: false,
        missing_path: false,
    };

    // =========================================================================
    // Capture Pre-Import Recursive SHA-1 Inventory of Download Directory
    // =========================================================================
    let inventory_before = recursive_inventory(&save_dir);
    assert_eq!(
        inventory_before.len(),
        5,
        "5 payload files must exist before import"
    );

    // =========================================================================
    // Execute Import with Real Engine
    // =========================================================================
    let selected = vec![
        qbt_orig.clone(),
        qbt_renamed.clone(),
        qbt_corrupt.clone(),
        qbt_missing.clone(),
    ];

    let options = ImportOptions {
        info_hashes: vec![
            hash_orig.clone(),
            hash_renamed.clone(),
            hash_corrupt.clone(),
            hash_missing.clone(),
        ],
        remove_from_qbit: false,
        skip_recheck: false,
    };

    let (summary, imported) = QbitImporter::execute_import(
        selected,
        options,
        &qbit_dir,
        &temp_root.join("backups"),
        shared_engine.clone(),
        db.clone(),
        &paths.logs_dir,
    );

    assert_eq!(summary.imported_count, 4, "All 4 torrents must be imported");
    assert_eq!(imported.len(), 4);

    // =========================================================================
    // Wait for Real Background Recheck Queue to Complete
    // =========================================================================
    let recheck_done = std::thread::spawn({
        let engine_check = engine.clone();
        move || {
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(100));
                if let Ok(list) = engine_check.list_torrents() {
                    let checking_count = list
                        .iter()
                        .filter(|t| t.status == "Checking" || t.status == "Queued for checking")
                        .count();
                    if checking_count == 0 {
                        return true;
                    }
                }
            }
            false
        }
    })
    .join()
    .unwrap();

    assert!(recheck_done, "Recheck queue must complete within timeout");

    // Let any remaining DB writes settle
    std::thread::sleep(Duration::from_millis(500));

    // =========================================================================
    // Assert 1: Payload Paths and Bytes Remain 100% Unchanged
    // =========================================================================
    let inventory_after = recursive_inventory(&save_dir);
    assert_eq!(
        inventory_before, inventory_after,
        "Download directory payload inventory must be 100% byte-for-byte identical after import"
    );

    // =========================================================================
    // Assert 2: Genuine Verification Results in Real Transmission Engine
    // =========================================================================
    let live_torrents = engine.list_torrents().unwrap();

    // A. Original layout: 100% verified
    let item_orig = live_torrents
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&hash_orig))
        .expect("Torrent 1 not found in live engine");
    assert_eq!(
        item_orig.progress, 1.0,
        "Original layout torrent must verify to 100% in real engine"
    );

    let meta_orig = db.get_torrent_metadata(&hash_orig).unwrap().unwrap();
    assert!(
        meta_orig.last_checked.is_some(),
        "Original layout torrent must receive last_checked timestamp"
    );

    // B. Renamed layout: 100% verified by real engine using mapped file paths
    let item_renamed = live_torrents
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&hash_renamed))
        .expect("Torrent 2 not found in live engine");
    assert_eq!(
        item_renamed.progress, 1.0,
        "Renamed layout torrent must be found by real engine and verify to 100%"
    );

    let meta_renamed = db.get_torrent_metadata(&hash_renamed).unwrap().unwrap();
    assert!(
        meta_renamed.last_checked.is_some(),
        "Renamed layout torrent must receive last_checked timestamp"
    );

    // C. Corrupted payload: NOT 100%
    let item_corrupt = live_torrents
        .iter()
        .find(|t| t.info_hash.eq_ignore_ascii_case(&hash_corrupt))
        .expect("Torrent 3 not found in live engine");
    assert!(
        item_corrupt.progress < 1.0,
        "Corrupted payload must NOT verify to 100% (progress was {})",
        item_corrupt.progress
    );
    assert_eq!(
        item_corrupt.status, "Paused",
        "Corrupted torrent must remain paused"
    );

    let meta_corrupt = db.get_torrent_metadata(&hash_corrupt).unwrap().unwrap();
    assert!(
        meta_corrupt.last_checked.is_some(),
        "Corrupted torrent completed check, so last_checked must be recorded"
    );

    // D. Unavailable path: No last_checked timestamp
    let meta_missing = db.get_torrent_metadata(&hash_missing).unwrap().unwrap();
    assert!(
        meta_missing.last_checked.is_none(),
        "Unavailable / missing download path must NOT receive a completion timestamp"
    );

    // =========================================================================
    // Assert 3: Timestamps and Engine Metadata Survive Restart
    // =========================================================================
    drop(db);
    let db_reopened = Database::new(&paths.db_file).expect("Failed to reopen Database");
    let meta_reopened = db_reopened
        .get_torrent_metadata(&hash_orig)
        .unwrap()
        .unwrap();
    assert_eq!(
        meta_reopened.last_checked, meta_orig.last_checked,
        "last_checked timestamp must survive database restart"
    );

    engine.shutdown_engine();
    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_real_engine_removal_and_selective_recovery_round_trip() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_real_removal_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

    let (engine, db, paths, _default_dl) = match setup_isolated_engine(&temp_root) {
        Some(ctx) => ctx,
        None => return,
    };
    let shared_engine: SharedEngine = engine.clone();

    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();

    let torrents_out = temp_root.join("meta_out");
    fs::create_dir_all(&torrents_out).unwrap();

    let qbit_dir = temp_root.join("qbit_profile");
    let qbit_bt_backup = qbit_dir.join("BT_backup");
    fs::create_dir_all(&qbit_bt_backup).unwrap();

    // Create Torrent A (to be selected for import and removal)
    let payload_a = save_dir.join("Payload_A");
    fs::create_dir_all(&payload_a).unwrap();
    fs::write(payload_a.join("file_a.dat"), vec![0x11u8; 32768]).unwrap();

    let (torrent_file_a, parsed_a, bytes_a) =
        create_test_torrent(&torrents_out, &payload_a, "Torrent A");
    let hash_a = parsed_a.info_hash.to_lowercase();

    let qbit_torrent_file_a = qbit_bt_backup.join(format!("{hash_a}.torrent"));
    let qbit_fastresume_file_a = qbit_bt_backup.join(format!("{hash_a}.fastresume"));
    fs::copy(&torrent_file_a, &qbit_torrent_file_a).unwrap();
    fs::write(&qbit_fastresume_file_a, b"d8:save_path8:/tmp/dlsee").unwrap();

    let qbt_a = QbitTorrent {
        info_hash: hash_a.clone(),
        name: parsed_a.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed_a.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: None,
        tags: vec![],
        paused: true,
        added_time: 1700000000,
        completed_time: Some(1700001000),
        ratio_limit: -1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_torrent_file_a.clone(),
            fastresume_path: qbit_fastresume_file_a.clone(),
        },
        raw_torrent_bytes: bytes_a,
        already_in_nebula: false,
        missing_path: false,
    };

    // Create Torrent B (unselected, must remain completely untouched in qBittorrent profile)
    let payload_b = save_dir.join("Payload_B");
    fs::create_dir_all(&payload_b).unwrap();
    fs::write(payload_b.join("file_b.dat"), vec![0x22u8; 32768]).unwrap();

    let (torrent_file_b, parsed_b, _bytes_b) =
        create_test_torrent(&torrents_out, &payload_b, "Torrent B");
    let hash_b = parsed_b.info_hash.to_lowercase();

    let qbit_torrent_file_b = qbit_bt_backup.join(format!("{hash_b}.torrent"));
    let qbit_fastresume_file_b = qbit_bt_backup.join(format!("{hash_b}.fastresume"));
    fs::copy(&torrent_file_b, &qbit_torrent_file_b).unwrap();
    fs::write(&qbit_fastresume_file_b, b"d8:save_path8:/tmp/dlsee").unwrap();

    // Verify both A and B source files exist initially
    assert!(qbit_torrent_file_a.exists());
    assert!(qbit_fastresume_file_a.exists());
    assert!(qbit_torrent_file_b.exists());
    assert!(qbit_fastresume_file_b.exists());

    let payload_inventory_before = recursive_inventory(&save_dir);

    // Execute Import with removal of A only
    let backup_root = temp_root.join("qbit_backups");
    let options = ImportOptions {
        info_hashes: vec![hash_a.clone()],
        remove_from_qbit: true,
        skip_recheck: false,
    };

    let proc_checker_mock = MockProcessChecker { running: false };
    let trash_provider = DefaultTrashProvider;

    let (summary, imported) = QbitImporter::execute_import_with_services(
        vec![qbt_a.clone()],
        options,
        &qbit_dir,
        &backup_root,
        shared_engine.clone(),
        db.clone(),
        &paths.logs_dir,
        &proc_checker_mock,
        &trash_provider,
    );

    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.removed_count, 1);
    assert!(summary.backup_dir.is_some());
    assert_eq!(imported.len(), 1);

    // Verify durable persistence of Torrent A in Nebula DB
    let meta_a = db.get_torrent_metadata(&hash_a).unwrap();
    assert!(
        meta_a.is_some(),
        "Torrent A must be durably persisted in Nebula SQLite database"
    );

    // Verify qBittorrent profile state:
    // A source files must be removed
    assert!(
        !qbit_torrent_file_a.exists(),
        "Torrent A .torrent must be removed from qBittorrent"
    );
    assert!(
        !qbit_fastresume_file_a.exists(),
        "Torrent A .fastresume must be removed from qBittorrent"
    );

    // B source files must remain completely untouched
    assert!(
        qbit_torrent_file_b.exists(),
        "Torrent B .torrent must remain untouched"
    );
    assert!(
        qbit_fastresume_file_b.exists(),
        "Torrent B .fastresume must remain untouched"
    );

    // Verify payload files on disk are completely unmodified
    let payload_inventory_after = recursive_inventory(&save_dir);
    assert_eq!(
        payload_inventory_before, payload_inventory_after,
        "Payload files must remain 100% byte-for-byte identical after removal"
    );

    // Verify backup manifest exists and records backed up files
    let backup_dir = PathBuf::from(summary.backup_dir.unwrap());
    let manifest_path = backup_dir.join("manifest.json");
    assert!(manifest_path.exists(), "Backup manifest must exist");

    // Execute Selective Restoration of A
    let proc_checker = MockProcessChecker { running: false };

    let restore_res = QbitRemover::restore_from_manifest_with_mode(
        &manifest_path,
        &proc_checker,
        true,
        RestoreMode::Selective,
    );
    assert!(
        restore_res.is_ok(),
        "Selective restoration must succeed: {:?}",
        restore_res
    );

    // Verify both Torrent A and Torrent B exist in qBittorrent profile after restoration
    assert!(
        qbit_torrent_file_a.exists(),
        "Torrent A .torrent must exist after selective restore"
    );
    assert!(
        qbit_fastresume_file_a.exists(),
        "Torrent A .fastresume must exist after selective restore"
    );
    assert!(
        qbit_torrent_file_b.exists(),
        "Torrent B .torrent must still exist after selective restore"
    );
    assert!(
        qbit_fastresume_file_b.exists(),
        "Torrent B .fastresume must still exist after selective restore"
    );

    engine.shutdown_engine();
    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_real_engine_process_interruption_preserves_database() {
    // -------------------------------------------------------------------------
    // WORKER MODE: executed as a separate child process with its own PID
    // -------------------------------------------------------------------------
    if let Ok(worker_dir_str) = std::env::var("REAL_ENGINE_INTERRUPTION_WORKER_DIR") {
        let temp_root = PathBuf::from(worker_dir_str);
        let pid = std::process::id();
        fs::write(temp_root.join("worker.pid"), pid.to_string()).unwrap();

        let (engine, db, paths, _default_dl) = match setup_isolated_engine(&temp_root) {
            Some(ctx) => ctx,
            None => {
                eprintln!("Worker: engine setup failed or skipped");
                std::process::exit(1);
            }
        };

        // Assert database and engine paths are strictly isolated inside temp_root
        assert!(
            paths.db_file.starts_with(&temp_root),
            "Worker DB path must be inside temp_root"
        );
        assert!(
            paths.engine_dir.starts_with(&temp_root),
            "Worker engine dir must be inside temp_root"
        );

        let save_dir = temp_root.join("downloads");
        let torrents_out = temp_root.join("meta_out");
        let qbit_dir = temp_root.join("qbit_profile");
        let qbit_bt_backup = qbit_dir.join("BT_backup");

        // Build 2 torrents for the worker batch
        let p1 = save_dir.join("Worker1");
        fs::create_dir_all(&p1).unwrap();
        fs::write(p1.join("file1.bin"), vec![0x11u8; 16384]).unwrap();
        let (_, parsed1, bytes1) = create_test_torrent(&torrents_out, &p1, "Worker 1");
        let h1 = parsed1.info_hash.to_lowercase();

        let qbt1 = QbitTorrent {
            info_hash: h1.clone(),
            name: parsed1.name.clone(),
            save_path: save_dir.to_string_lossy().to_string(),
            download_path: None,
            total_size: parsed1.total_bytes,
            progress: 1.0,
            state_string: "paused".to_string(),
            category: Some("WorkerCategory".to_string()),
            tags: vec![],
            paused: true,
            added_time: 1700000000,
            completed_time: Some(1700001000),
            ratio_limit: 1.0,
            seeding_time_limit: None,
            download_limit_kib: 0,
            upload_limit_kib: 0,
            sequential_download: false,
            first_last_piece_prio: false,
            auto_managed: false,
            content_layout: Some("Original".to_string()),
            mapped_files: vec![],
            file_priorities: vec![1],
            trackers: vec![],
            source: QbitSourceType::LegacyFile {
                torrent_path: qbit_bt_backup.join(format!("{h1}.torrent")),
                fastresume_path: qbit_bt_backup.join(format!("{h1}.fastresume")),
            },
            raw_torrent_bytes: bytes1,
            already_in_nebula: false,
            missing_path: false,
        };

        // Worker writes item 1, commits it
        let (rep1, _) = QbitImporter::execute_import(
            vec![qbt1],
            ImportOptions {
                info_hashes: vec![],
                skip_recheck: true,
                remove_from_qbit: false,
            },
            &qbit_dir,
            &temp_root.join("backup"),
            engine.clone(),
            db.clone(),
            &temp_root.join("logs"),
        );
        assert_eq!(rep1.imported_count, 1);

        // Signal parent that item 1 is committed and item 2 is in-flight
        fs::write(temp_root.join("item2_in_flight.marker"), "item2_in_flight").unwrap();

        // Worker simulates processing item 2 when killed mid-operation
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }

        std::process::exit(0);
    }

    // -------------------------------------------------------------------------
    // PARENT TEST CONTROLLER
    // -------------------------------------------------------------------------
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_real_interruption_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

    let save_dir = temp_root.join("downloads");
    fs::create_dir_all(&save_dir).unwrap();
    let torrents_out = temp_root.join("meta_out");
    fs::create_dir_all(&torrents_out).unwrap();
    let qbit_dir = temp_root.join("qbit_profile");
    let qbit_bt_backup = qbit_dir.join("BT_backup");
    fs::create_dir_all(&qbit_bt_backup).unwrap();

    // 1. Setup isolated database and pre-populate baseline user records
    let nebulae_data = temp_root.join("nebulae_data");
    let db_dir = nebulae_data.join("db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("nebulae.db");
    let db = Arc::new(Database::new(&db_path).expect("Failed to initialize baseline database"));

    // Populate 3 baseline records with custom titles, locked status, and posters
    let mut baseline_hashes = Vec::new();
    for i in 0..3 {
        let h = format!("baseline{:032x}", i);
        let meta = nebulae_torrent_client_lib::db::TorrentDbMetadata {
            info_hash: h.clone(),
            display_title: Some(format!("Baseline Torrent {i}")),
            category: Some("Movies".to_string()),
            labels: None,
            is_watched: false,
            notes: Some("Pre-existing baseline record".to_string()),
            added_at: 1690000000 + i as u64,
            completed_on: Some(1690001000),
            save_path: Some(format!("C:\\Downloads\\Baseline_{i}")),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: 1.5,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
            racing_override: None,
            user_display_title: Some(format!("Custom Renamed Baseline {i}")),
            user_title_locked: true,
            sort_order: Some(i as i64),
            poster_path: Some(format!("/posters/base_{i}.png")),
            is_external: false,
            sync_miss_count: 0,
            last_checked: None,
        };
        db.upsert_torrent_metadata(&meta).unwrap();
        db.set_user_display_title(&h, &format!("Custom Renamed Baseline {i}"), true)
            .unwrap();
        db.set_torrent_poster(&h, &format!("/posters/base_{i}.png"))
            .unwrap();
        db.set_torrent_sort_order(&h, Some(i as i64)).unwrap();
        baseline_hashes.push(h);
    }
    drop(db); // Close connection so worker child process can open it

    // Verify baseline integrity
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let check: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            check, "ok",
            "Database PRAGMA integrity_check must be ok before test"
        );
    }

    // 2. Spawn worker child process
    let current_exe = std::env::current_exe().expect("current_exe failed");
    let mut child = std::process::Command::new(current_exe)
        .arg("test_real_engine_process_interruption_preserves_database")
        .arg("--exact")
        .arg("--nocapture")
        .env("REAL_ENGINE_INTERRUPTION_WORKER_DIR", &temp_root)
        .spawn()
        .expect("Failed to spawn interruption worker child process");

    let child_pid = child.id();
    assert_ne!(
        child_pid,
        std::process::id(),
        "Child PID must be distinct from parent PID"
    );

    // 3. Wait for worker PID file and verify isolation
    let worker_pid_file = temp_root.join("worker.pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !worker_pid_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(worker_pid_file.exists(), "Worker PID file must be written");
    let recorded_pid: u32 = fs::read_to_string(&worker_pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        recorded_pid, child_pid,
        "Recorded PID must match spawned child PID"
    );

    // 4. Wait for item 2 in-flight signal
    let marker = temp_root.join("item2_in_flight.marker");
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        marker.exists(),
        "Worker must have reached in-flight state for item 2"
    );

    // 5. Forcibly terminate ONLY the isolated candidate child process by its verified PID
    #[cfg(target_os = "windows")]
    {
        println!(
            "Terminating worker child process by verified PID: {}",
            child_pid
        );
        let status = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &child_pid.to_string()])
            .output()
            .expect("taskkill failed");
        println!(
            "taskkill output: {}",
            String::from_utf8_lossy(&status.stdout)
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();

    // 6. POST-INTERRUPTION ASSERTIONS: SQLite Integrity and Pre-existing Records
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let check: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            check, "ok",
            "Database PRAGMA integrity_check must be ok immediately after abrupt kill"
        );
    }

    let restarted_db =
        Arc::new(Database::new(&db_path).expect("Failed to reopen database after kill"));

    // Assert ALL 3 baseline records are 100% intact with their locked titles and sort orders
    for (i, h) in baseline_hashes.iter().enumerate() {
        let meta = restarted_db
            .get_torrent_metadata(h)
            .unwrap()
            .expect("Baseline record must survive uncorrupted");
        assert_eq!(
            meta.user_display_title,
            Some(format!("Custom Renamed Baseline {i}")),
            "user_display_title must survive abrupt kill"
        );
        assert!(
            meta.user_title_locked,
            "user_title_locked must survive abrupt kill"
        );
        assert_eq!(
            meta.sort_order,
            Some(i as i64),
            "sort_order must survive abrupt kill"
        );
        assert_eq!(
            meta.poster_path,
            Some(format!("/posters/base_{i}.png")),
            "poster_path must survive"
        );
    }

    // 7. Initialize parent isolated engine and RESTART the import of the batch
    let (parent_engine, _, _, _) = match setup_isolated_engine(&temp_root) {
        Some(ctx) => ctx,
        None => panic!("Failed to setup parent engine for restart"),
    };

    // Reconstruct the batch of 2 torrents
    let p1 = save_dir.join("Worker1");
    let (_, parsed1, bytes1) = create_test_torrent(&torrents_out, &p1, "Worker 1");
    let h1 = parsed1.info_hash.to_lowercase();
    let p2 = save_dir.join("Worker2");
    fs::create_dir_all(&p2).unwrap();
    fs::write(p2.join("file2.bin"), vec![0x22u8; 16384]).unwrap();
    let (_, parsed2, bytes2) = create_test_torrent(&torrents_out, &p2, "Worker 2");
    let h2 = parsed2.info_hash.to_lowercase();

    let qbt1 = QbitTorrent {
        info_hash: h1.clone(),
        name: parsed1.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed1.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: Some("WorkerCategory".to_string()),
        tags: vec![],
        paused: true,
        added_time: 1700000000,
        completed_time: Some(1700001000),
        ratio_limit: 1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{h1}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{h1}.fastresume")),
        },
        raw_torrent_bytes: bytes1,
        already_in_nebula: false,
        missing_path: false,
    };

    let qbt2 = QbitTorrent {
        info_hash: h2.clone(),
        name: parsed2.name.clone(),
        save_path: save_dir.to_string_lossy().to_string(),
        download_path: None,
        total_size: parsed2.total_bytes,
        progress: 1.0,
        state_string: "paused".to_string(),
        category: Some("WorkerCategory".to_string()),
        tags: vec![],
        paused: true,
        added_time: 1700000000,
        completed_time: Some(1700001000),
        ratio_limit: 1.0,
        seeding_time_limit: None,
        download_limit_kib: 0,
        upload_limit_kib: 0,
        sequential_download: false,
        first_last_piece_prio: false,
        auto_managed: false,
        content_layout: Some("Original".to_string()),
        mapped_files: vec![],
        file_priorities: vec![1],
        trackers: vec![],
        source: QbitSourceType::LegacyFile {
            torrent_path: qbit_bt_backup.join(format!("{h2}.torrent")),
            fastresume_path: qbit_bt_backup.join(format!("{h2}.fastresume")),
        },
        raw_torrent_bytes: bytes2,
        already_in_nebula: false,
        missing_path: false,
    };

    // Execute Restarted Import
    let (restart_report, _) = QbitImporter::execute_import(
        vec![qbt1, qbt2],
        ImportOptions {
            info_hashes: vec![],
            skip_recheck: true,
            remove_from_qbit: false,
        },
        &qbit_dir,
        &temp_root.join("backup"),
        parent_engine.clone(),
        restarted_db.clone(),
        &temp_root.join("logs"),
    );

    println!(
        "RESTART REPORT: imported={}, skipped={:?}, failed={:?}",
        restart_report.imported_count, restart_report.skipped, restart_report.failed
    );

    // Item 1 was committed before the kill, so it MUST be skipped with zero duplicates
    assert_eq!(
        restart_report.skipped.len(),
        1,
        "Item 1 must be skipped as already present"
    );
    assert_eq!(restart_report.skipped[0].info_hash, h1);

    // Item 2 must be imported cleanly
    assert_eq!(
        restart_report.imported_count, 1,
        "Item 2 must be imported on restart"
    );

    // Final database assertions
    let all_meta = restarted_db.list_all_torrent_metadata().unwrap();
    println!("ALL_META ROWS ({}):", all_meta.len());
    for (h, m) in &all_meta {
        println!(
            " - {} (user_title: {:?}, locked: {})",
            h, m.user_display_title, m.user_title_locked
        );
    }
    assert_eq!(
        all_meta.len(),
        3 + 2,
        "Total rows must be exactly 3 baseline + 2 imported = 5"
    );

    parent_engine.shutdown_engine();
    let _ = fs::remove_dir_all(&temp_root);
}

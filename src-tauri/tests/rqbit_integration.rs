use nebulae_torrent_client_lib::db::Database;
use nebulae_torrent_client_lib::engine::rqbit::RqbitTorrentEngine;
use nebulae_torrent_client_lib::engine::traits::{EngineHook, TorrentEngine};
use nebulae_torrent_client_lib::utils::paths::AppPaths;
use std::sync::Arc;

fn setup_test_engine(test_name: &str) -> (RqbitTorrentEngine, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "nebulae_rqbit_test_{}_{}",
        test_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
    ));
    std::fs::create_dir_all(&tmp).unwrap();

    let paths = Arc::new(AppPaths {
        base_data_dir: tmp.clone(),
        internal_dir: tmp.join("internal"),
        logs_dir: tmp.join("internal/logs"),
        diagnostics_dir: tmp.join("internal/diagnostics"),
        db_dir: tmp.join("internal/db"),
        db_file: tmp.join("internal/db/nebulae.db"),
        cache_dir: tmp.join("internal/cache"),
        engine_dir: tmp.join("internal/engine-state"),
        debug_dir: tmp.join("internal/debug"),
    });

    std::fs::create_dir_all(&paths.db_dir).unwrap();
    std::fs::create_dir_all(&paths.engine_dir).unwrap();

    let db = Arc::new(Database::new(&paths.db_file).expect("Failed to create test db"));
    let download_dir = tmp.join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();

    // Use ephemeral port 0 for concurrent test isolation
    let engine = RqbitTorrentEngine::new(paths, db, download_dir, 0)
        .expect("Failed to initialize RqbitTorrentEngine");

    (engine, tmp)
}

#[test]
fn test_rqbit_engine_lifecycle_and_hooks() {
    let (engine, tmp) = setup_test_engine("lifecycle");

    // 1. Initial listing must be empty
    let initial_list = engine.list_torrents().expect("list failed");
    assert_eq!(initial_list.len(), 0);

    // 2. Status check
    let status = engine.engine_status();
    assert_eq!(status.active_torrents, 0);
    assert_eq!(status.paused_torrents, 0);
    assert!(!status.kill_switch_engaged);

    // 3. Supported engine hooks
    assert!(engine.supports_hook(EngineHook::PrivateTorrentRestrictions));
    assert!(engine.supports_hook(EngineHook::PauseResumeKillSwitch));
    assert!(engine.supports_hook(EngineHook::TrackerDhtPexHandling));
    assert!(engine.supports_hook(EngineHook::PeerTelemetryCollection));
    assert!(engine.supports_hook(EngineHook::SocketInterfaceBinding));
    assert!(!engine.supports_hook(EngineHook::PieceRequestScheduling));

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rqbit_engine_add_pause_resume_remove() {
    let (engine, tmp) = setup_test_engine("add_pause_remove");

    // Create a dummy torrent file using metainfo builder
    let payload_dir = tmp.join("sample_release");
    std::fs::create_dir_all(&payload_dir).unwrap();
    std::fs::write(payload_dir.join("video.mkv"), vec![0xAB; 32768]).unwrap();

    let torrent_out = tmp.join("out");
    std::fs::create_dir_all(&torrent_out).unwrap();

    let torrent_file = nebulae_torrent_client_lib::engine::metainfo::build_torrent_file(
        &payload_dir,
        &torrent_out,
        16,
        &["https://tracker.asiandvdclub.org:2710/announce".to_string()],
        true,
        "ADC Archive Test",
        "Nebulae",
    )
    .expect("build torrent failed");

    let torrent_bytes = std::fs::read(&torrent_file).expect("read torrent file failed");

    // Add torrent to engine via bytes
    let download_target = tmp.join("dl_target");
    let info_hash = engine
        .add_torrent_from_bytes(&torrent_bytes, Some(&download_target.to_string_lossy()))
        .expect("add_torrent_from_bytes failed");

    assert!(!info_hash.is_empty());

    // Verify torrent listed
    let torrents = engine.list_torrents().expect("list failed");
    assert_eq!(torrents.len(), 1);
    let item = &torrents[0];
    assert_eq!(item.info_hash, info_hash);
    assert!(item.is_media); // video.mkv should be classified as media
    assert_eq!(item.category, "General");

    // Set category and limits
    engine
        .set_category(&info_hash, "Movies")
        .expect("set category failed");
    engine
        .set_torrent_limits(&info_hash, 1024, 512, 2.0)
        .expect("set limits failed");

    let torrents_after_meta = engine.list_torrents().expect("list failed");
    let item_after = &torrents_after_meta[0];
    assert_eq!(item_after.category, "Movies");
    assert_eq!(item_after.download_limit_kib, 1024);
    assert_eq!(item_after.upload_limit_kib, 512);

    // Pause and resume
    engine.pause_torrent(&info_hash).expect("pause failed");
    let after_pause = engine.list_torrents().expect("list failed");
    assert_eq!(after_pause[0].status, "Paused");

    engine.resume_torrent(&info_hash).expect("resume failed");

    // Remove torrent
    engine
        .remove_torrent(&info_hash, false)
        .expect("remove failed");
    let after_remove = engine.list_torrents().expect("list failed");
    assert_eq!(after_remove.len(), 0);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rqbit_engine_kill_switch_fail_closed() {
    let (engine, tmp) = setup_test_engine("kill_switch");

    assert!(!engine.is_kill_switch_active());
    engine
        .trigger_kill_switch()
        .expect("trigger kill switch failed");
    assert!(engine.is_kill_switch_active());

    // Adding torrent while kill switch is active must be rejected
    let res = engine.add_torrent(
        "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567",
        None,
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("VPN Kill Switch is active"));

    engine
        .reset_kill_switch()
        .expect("reset kill switch failed");
    assert!(!engine.is_kill_switch_active());

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rqbit_engine_bep27_private_torrent_isolation() {
    let (engine, tmp) = setup_test_engine("bep27");

    // 1. Build a BEP 27 private torrent (ADC tracker)
    let private_payload_dir = tmp.join("private_release");
    std::fs::create_dir_all(&private_payload_dir).unwrap();
    std::fs::write(
        private_payload_dir.join("exclusive_dvd.iso"),
        vec![0xEE; 16384],
    )
    .unwrap();

    let out_dir = tmp.join("torrents_out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let private_torrent = nebulae_torrent_client_lib::engine::metainfo::build_torrent_file(
        &private_payload_dir,
        &out_dir,
        16,
        &["https://tracker.asiandvdclub.org:2710/passkey1234567890/announce".to_string()],
        true, // is_private = true (BEP 27)
        "Asian DVD Club Exclusive",
        "Nebulae",
    )
    .expect("build private torrent failed");

    // 2. Build a public torrent
    let public_payload_dir = tmp.join("public_release");
    std::fs::create_dir_all(&public_payload_dir).unwrap();
    std::fs::write(
        public_payload_dir.join("public_data.bin"),
        vec![0xAA; 16384],
    )
    .unwrap();

    let public_torrent = nebulae_torrent_client_lib::engine::metainfo::build_torrent_file(
        &public_payload_dir,
        &out_dir,
        16,
        &["udp://tracker.opentrackr.org:1337/announce".to_string()],
        false, // is_private = false
        "Public Open Torrent",
        "Nebulae",
    )
    .expect("build public torrent failed");

    // 3. Verify parsed metainfo adheres to BEP 27 flags
    let parsed_priv =
        nebulae_torrent_client_lib::engine::metainfo::ParsedTorrent::parse_file(&private_torrent)
            .expect("parse private torrent failed");
    assert!(
        parsed_priv.is_private,
        "Private torrent must have is_private = true"
    );

    let parsed_pub =
        nebulae_torrent_client_lib::engine::metainfo::ParsedTorrent::parse_file(&public_torrent)
            .expect("parse public torrent failed");
    assert!(
        !parsed_pub.is_private,
        "Public torrent must have is_private = false"
    );

    // 4. Verify engine reports PrivateTorrentRestrictions support
    assert!(engine.supports_hook(EngineHook::PrivateTorrentRestrictions));

    // 5. Add both to rqbit engine
    let priv_bytes = std::fs::read(&private_torrent).unwrap();
    let priv_hash = engine
        .add_torrent_from_bytes(&priv_bytes, None)
        .expect("add private torrent failed");
    assert!(!priv_hash.is_empty());

    let pub_bytes = std::fs::read(&public_torrent).unwrap();
    let pub_hash = engine
        .add_torrent_from_bytes(&pub_bytes, None)
        .expect("add public torrent failed");
    assert!(!pub_hash.is_empty());

    let torrents = engine.list_torrents().expect("list failed");
    assert_eq!(torrents.len(), 2);
    assert!(torrents.iter().any(|t| t.info_hash == priv_hash));
    assert!(torrents.iter().any(|t| t.info_hash == pub_hash));

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_rqbit_engine_magnet_link_resolution() {
    let (engine, tmp) = setup_test_engine("magnet_resolution");

    // Add magnet link with dn display name and ADC tracker
    let magnet = "magnet:?xt=urn:btih:3b245504d5f12bbdbe12f73313a3711941d66669&dn=Classic+Asian+Cinema+Collection&tr=https%3A%2F%2Ftracker.asiandvdclub.org%3A2710%2Fannounce";
    let hash = engine.add_torrent(magnet, None).expect("add magnet failed");
    assert_eq!(hash, "3b245504d5f12bbdbe12f73313a3711941d66669");

    let torrents = engine.list_torrents().expect("list failed");
    assert_eq!(torrents.len(), 1);
    let item = &torrents[0];
    assert_eq!(item.info_hash, "3b245504d5f12bbdbe12f73313a3711941d66669");
    assert_eq!(item.name, "Classic Asian Cinema Collection");
    // Before peers connect to provide ut_metadata, state is honestly reported as Downloading Metadata
    assert_eq!(item.status, "Downloading Metadata");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

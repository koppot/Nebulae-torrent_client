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

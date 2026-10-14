use nebulae_torrent_client_lib::engine::{EngineHook, MockTorrentEngine, TorrentEngine};
use nebulae_torrent_client_lib::models::swarm::AdaptiveSwarmConfig;

#[test]
fn test_engine_initialization_and_listing() {
    let engine = MockTorrentEngine::new();
    let torrents = engine.list_torrents().expect("list torrents failed");
    assert_eq!(
        torrents.len(),
        0,
        "Default torrent engine must initialize with zero torrents"
    );
}

#[test]
fn test_engine_pause_and_resume() {
    let engine = MockTorrentEngine::with_sample_data();

    // Pause torrent 2
    engine.pause_torrent("swarm-test-2").expect("pause failed");
    let torrents = engine.list_torrents().unwrap();
    let t2 = torrents.iter().find(|t| t.id == "swarm-test-2").unwrap();
    assert_eq!(t2.status, "Paused");

    // Resume torrent 2
    engine
        .resume_torrent("swarm-test-2")
        .expect("resume failed");
    let torrents = engine.list_torrents().unwrap();
    let t2 = torrents.iter().find(|t| t.id == "swarm-test-2").unwrap();
    assert_eq!(t2.status, "Downloading");
}

#[test]
fn test_vpn_kill_switch_fail_closed_behavior() {
    let engine = MockTorrentEngine::with_sample_data();

    // Both torrents active initially
    let status = engine.engine_status();
    assert_eq!(status.active_torrents, 2);
    assert_eq!(status.paused_torrents, 0);
    assert!(!status.kill_switch_engaged);

    // Trigger VPN Kill Switch (e.g. tunnel unexpectedly drops)
    engine
        .trigger_kill_switch()
        .expect("trigger kill switch failed");

    // All transfers must immediately pause
    let status_after = engine.engine_status();
    assert_eq!(status_after.active_torrents, 0);
    assert_eq!(status_after.paused_torrents, 2);
    assert!(status_after.kill_switch_engaged);

    // Fail-Closed: Attempting to resume a torrent while kill switch is armed MUST FAIL
    let resume_result = engine.resume_torrent("swarm-test-1");
    assert!(
        resume_result.is_err(),
        "Resume must fail when kill switch is armed"
    );
    assert!(resume_result
        .unwrap_err()
        .contains("VPN Kill Switch is active"));

    // Fail-Closed: Adding new torrents while kill switch is armed MUST FAIL
    let add_result = engine.add_torrent("magnet:?xt=urn:btih:0123456789abcdef", None);
    assert!(
        add_result.is_err(),
        "Add torrent must fail when kill switch is armed"
    );

    // Transfers remain paused
    let torrents = engine.list_torrents().unwrap();
    assert!(torrents.iter().all(|t| t.status == "Paused"));

    // Reset kill switch
    engine
        .reset_kill_switch()
        .expect("reset kill switch failed");
    assert!(!engine.is_kill_switch_active());

    // Resuming is now permitted
    engine
        .resume_torrent("swarm-test-1")
        .expect("resume failed after reset");
    let t1 = engine
        .list_torrents()
        .unwrap()
        .into_iter()
        .find(|t| t.id == "swarm-test-1")
        .unwrap();
    assert_eq!(t1.status, "Seeding");
}

#[test]
fn test_smart_peering_observational_hook_boundaries() {
    let engine = MockTorrentEngine::new();

    // Supported engine hooks
    assert!(engine.supports_hook(EngineHook::PrivateTorrentRestrictions));
    assert!(engine.supports_hook(EngineHook::PauseResumeKillSwitch));
    assert!(engine.supports_hook(EngineHook::TrackerDhtPexHandling));
    assert!(engine.supports_hook(EngineHook::PeerTelemetryCollection));
    assert!(engine.supports_hook(EngineHook::SocketInterfaceBinding));

    // Observational boundary: external queue scheduling & connection admission not exposed by librqbit
    assert!(!engine.supports_hook(EngineHook::PieceRequestScheduling));
    assert!(!engine.supports_hook(EngineHook::ConnectionAdmission));
    assert!(!engine.supports_hook(EngineHook::ConnectionEviction));
    assert!(!engine.supports_hook(EngineHook::PerPeerPrioritization));
}

#[test]
fn test_adaptive_swarm_auto_tune() {
    // 1. Small swarm
    let small_swarm = AdaptiveSwarmConfig::auto_tune(6, 45, 1_000_000, 20);
    assert_eq!(small_swarm.active_connections_limit, 6);
    assert!(!small_swarm.endgame_active);

    // 2. Large swarm with gigabit rate
    let large_swarm = AdaptiveSwarmConfig::auto_tune(1200, 20, 100_000_000, 2);
    assert_eq!(large_swarm.active_connections_limit, 120);
    assert!(large_swarm.endgame_active); // <= 5 pieces remaining
    assert_eq!(large_swarm.disk_coalescing_mb, 64); // Gigabit rate scales buffer to 64 MiB
}

#[test]
fn test_zero_leechers_zero_upload_rate() {
    let engine = MockTorrentEngine::new();
    let torrent_id = engine
        .add_torrent(
            "magnet:?xt=urn:btih:abcdef1234567890&dn=TestZeroLeechers",
            None,
        )
        .expect("add torrent failed");

    // Sleep 1.5s to let the transfer loop tick
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let torrents = engine.list_torrents().unwrap();
    let t = torrents.iter().find(|item| item.id == torrent_id).unwrap();

    // Since this torrent has no active leechers requesting blocks (peers == 0),
    // upload_rate must strictly be 0.
    if t.peers == 0 {
        assert_eq!(
            t.upload_rate, 0,
            "Upload rate must be 0 when no peers are downloading from client"
        );
        assert_eq!(
            t.uploaded_bytes, 0,
            "Uploaded bytes must not increment without leechers"
        );
    }
}

#[test]
fn test_torrent_creation_from_directory_and_file() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "nebulae_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source_dir = tmp_dir.join("TestReleaseFolder");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(source_dir.join("disc1.iso"), vec![0x42; 65536]).unwrap();
    std::fs::write(source_dir.join("disc2.iso"), vec![0x43; 32768]).unwrap();

    let out_dir = tmp_dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let torrent_file = nebulae_torrent_client_lib::engine::metainfo::build_torrent_file(
        &source_dir,
        &out_dir,
        32,
        &["https://asiandvdclub.org/announce".to_string()],
        true,
        "Created with Nebulae Test",
        "Nebulae",
    )
    .expect("Torrent creation failed");

    assert!(torrent_file.exists());
    assert_eq!(
        torrent_file.file_name().unwrap(),
        "TestReleaseFolder.torrent"
    );

    // Parse the generated torrent file
    let parsed =
        nebulae_torrent_client_lib::engine::metainfo::ParsedTorrent::parse_file(&torrent_file)
            .unwrap();
    assert_eq!(parsed.name, "TestReleaseFolder");
    assert_eq!(parsed.total_bytes, 65536 + 32768);
    assert_eq!(parsed.files.len(), 2);
    assert!(parsed.is_private);
    assert_eq!(parsed.comment, "Created with Nebulae Test");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_torrent_folder_and_file_renaming() {
    let tmp_dir = std::env::temp_dir().join(format!("nebulae_rename_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros()));
    let release_dir = tmp_dir.join("Tokyo.Story.Series");
    let season_dir = release_dir.join("Season 1");
    let subs_dir = release_dir.join("Subs");
    std::fs::create_dir_all(&season_dir).unwrap();
    std::fs::create_dir_all(&subs_dir).unwrap();

    let ep1 = season_dir.join("Episode 1.mkv");
    let ep2 = season_dir.join("Episode 2.mkv");
    let sub = subs_dir.join("en.srt");
    std::fs::write(&ep1, b"mock video 1").unwrap();
    std::fs::write(&ep2, b"mock video 2").unwrap();
    std::fs::write(&sub, b"1\n00:00:00 --> 00:00:05\nHello\n").unwrap();

    let engine = MockTorrentEngine::new();
    let id = engine
        .add_torrent(&release_dir.to_string_lossy(), Some(&tmp_dir.to_string_lossy()))
        .expect("add torrent from dir failed");

    let torrents = engine.list_torrents().unwrap();
    let item = torrents.iter().find(|t| t.id == id).expect("torrent not found");
    assert_eq!(item.files.len(), 3);
    assert!(item.files.iter().any(|f| f.name == "Season 1/Episode 1.mkv"));
    assert!(item.files.iter().any(|f| f.name == "Season 1/Episode 2.mkv"));
    assert!(item.files.iter().any(|f| f.name == "Subs/en.srt"));

    // 1. Test folder renaming: "Season 1" -> "Season 01"
    engine
        .rename_torrent_folder(&id, "Season 1", "Season 01")
        .expect("rename folder failed");

    let torrents_after = engine.list_torrents().unwrap();
    let item_after = torrents_after.iter().find(|t| t.id == id).unwrap();
    assert!(item_after.files.iter().any(|f| f.name == "Season 01/Episode 1.mkv"));
    assert!(item_after.files.iter().any(|f| f.name == "Season 01/Episode 2.mkv"));
    assert!(item_after.files.iter().any(|f| f.name == "Subs/en.srt"));
    assert!(release_dir.join("Season 01/Episode 1.mkv").exists());
    assert!(!release_dir.join("Season 1").exists());

    // 2. Test file renaming preserving parent directory: "Subs/en.srt" -> "english.srt"
    let sub_file = item_after.files.iter().find(|f| f.name == "Subs/en.srt").unwrap();
    engine
        .rename_torrent_file(&id, sub_file.index, "english.srt")
        .expect("rename file failed");

    let torrents_after_file = engine.list_torrents().unwrap();
    let item_after_file = torrents_after_file.iter().find(|t| t.id == id).unwrap();
    assert!(item_after_file.files.iter().any(|f| f.name == "Subs/english.srt"));
    assert!(release_dir.join("Subs/english.srt").exists());
    assert!(!release_dir.join("Subs/en.srt").exists());

    // 3. Test safety guard protecting qBittorrent paths
    let qbit_id = engine.add_torrent("magnet:?xt=urn:btih:abc&dn=qbit_test", Some("/home/user/qbittorrent/downloads")).unwrap();
    let rename_guard_err = engine.rename_torrent_folder(&qbit_id, "folder", "new_folder");
    assert!(rename_guard_err.is_err());
    assert!(rename_guard_err.unwrap_err().contains("Safety Guard"));

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_remove_torrent_with_and_without_files() {
    let tmp_base = std::env::temp_dir().join(format!(
        "nebulae_remove_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
    ));
    std::fs::create_dir_all(&tmp_base).unwrap();

    let engine = MockTorrentEngine::new();

    // 1. Test Retention Path: remove_torrent(id, false)
    let retention_dir = tmp_base.join("KeepTorrent");
    let retention_sub = retention_dir.join("SubDir");
    std::fs::create_dir_all(&retention_sub).unwrap();
    let retention_file = retention_sub.join("keep_me.txt");
    std::fs::write(&retention_file, b"important payload").unwrap();

    let id_retention = engine
        .add_torrent(
            &retention_dir.to_string_lossy(),
            Some(&tmp_base.to_string_lossy()),
        )
        .expect("add retention torrent failed");

    assert!(engine.list_torrents().unwrap().iter().any(|t| t.id == id_retention));

    // Remove without deleting files
    engine
        .remove_torrent(&id_retention, false)
        .expect("remove_torrent without files failed");

    // Must be removed from in-memory engine list
    assert!(
        engine.list_torrents().unwrap().iter().all(|t| t.id != id_retention),
        "Torrent must be removed from memory list"
    );
    // Files on disk must still exist
    assert!(
        retention_file.exists(),
        "Payload file on disk must be preserved when delete_files is false"
    );
    assert!(
        retention_dir.exists(),
        "Torrent directory must be preserved when delete_files is false"
    );

    // 2. Test Deletion Path: remove_torrent(id, true)
    let delete_dir = tmp_base.join("DeleteTorrent");
    let delete_sub = delete_dir.join("SubDir");
    std::fs::create_dir_all(&delete_sub).unwrap();
    let delete_file = delete_sub.join("delete_me.txt");
    std::fs::write(&delete_file, b"disposable payload").unwrap();

    let id_delete = engine
        .add_torrent(
            &delete_dir.to_string_lossy(),
            Some(&tmp_base.to_string_lossy()),
        )
        .expect("add delete torrent failed");

    assert!(engine.list_torrents().unwrap().iter().any(|t| t.id == id_delete));

    // Remove with deleting files
    engine
        .remove_torrent(&id_delete, true)
        .expect("remove_torrent with delete_files failed");

    // Must be removed from in-memory engine list
    assert!(
        engine.list_torrents().unwrap().iter().all(|t| t.id != id_delete),
        "Torrent must be removed from memory list"
    );
    // Files on disk must be deleted
    assert!(
        !delete_file.exists(),
        "Payload file on disk must be deleted when delete_files is true"
    );
    assert!(
        !delete_dir.exists(),
        "Torrent directory must be deleted when delete_files is true"
    );

    // 3. Test External qBittorrent Safety Guard
    let qbit_dir = tmp_base.join("qbittorrent_downloads");
    std::fs::create_dir_all(&qbit_dir).unwrap();
    let qbit_file = qbit_dir.join("protected_qbit.mkv");
    std::fs::write(&qbit_file, b"qbit data").unwrap();

    let id_qbit = engine
        .add_torrent("magnet:?xt=urn:btih:fedcba9876543210&dn=qbit_test", Some(&qbit_dir.to_string_lossy()))
        .expect("add qbit torrent failed");

    // Attempting to delete with delete_files = true must be blocked by the safety guard
    let guard_err = engine.remove_torrent(&id_qbit, true);
    assert!(guard_err.is_err(), "Safety guard must reject deletion of qbittorrent managed path");
    assert!(guard_err.unwrap_err().contains("Safety Guard"));

    // Torrent must still be in the engine list
    assert!(
        engine.list_torrents().unwrap().iter().any(|t| t.id == id_qbit),
        "Torrent must remain in engine list if safety guard is triggered"
    );
    // Files on disk must be untouched
    assert!(qbit_file.exists(), "Protected qbittorrent file must remain untouched");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_base);
}



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

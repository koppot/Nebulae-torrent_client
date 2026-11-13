use nebulae_torrent_client_lib::commands::settings::test_rtorrent_server_connection;
use nebulae_torrent_client_lib::engine::remote::RemoteServerEngine;
use nebulae_torrent_client_lib::engine::traits::TorrentEngine;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn test_live_container_full_lifecycle() {
    let base_url = "http://127.0.0.1:9174";
    let token = "testtoken123";

    // 1. Connection probe (Settings -> Test Connection)
    let status = test_rtorrent_server_connection(base_url.to_string(), token.to_string())
        .expect("connection probe should succeed");
    assert_eq!(status.name, "nebulae-server");
    assert_eq!(status.engine, "transmission");
    assert!(!status.kill_switch_engaged);

    // 2. Auth rejection on invalid token
    let bad = test_rtorrent_server_connection(base_url.to_string(), "bad-token-xyz".to_string());
    assert!(bad.is_err());
    assert!(bad.unwrap_err().contains("Unauthorized"));

    // 3. Desktop engine interface (RemoteServerEngine)
    let engine = RemoteServerEngine::new(base_url.to_string(), token.to_string());

    // 4. Add torrent
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1b&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969";
    let expected_hash = "dd8255ecdc7ca55fb0bbf81323d87062db1f6d1b";
    let add_res = engine.add_torrent(magnet, None);
    assert!(add_res.is_ok(), "add_torrent should succeed: {:?}", add_res);

    // 5. Verify presence in torrent list
    let list_1 = engine
        .list_torrents()
        .expect("list_torrents should succeed");
    assert!(
        list_1.iter().any(|t| t.info_hash == expected_hash),
        "torrent must be listed after add"
    );

    // 6. Pause torrent
    engine
        .pause_torrent(expected_hash)
        .expect("pause should succeed");
    sleep(Duration::from_millis(500));
    let list_paused = engine
        .list_torrents()
        .expect("list_torrents should succeed");
    let paused_item = list_paused
        .iter()
        .find(|t| t.info_hash == expected_hash)
        .unwrap();
    assert_eq!(paused_item.status.to_lowercase(), "paused");

    // 7. Resume torrent
    engine
        .resume_torrent(expected_hash)
        .expect("resume should succeed");
    sleep(Duration::from_millis(500));
    let list_resumed = engine
        .list_torrents()
        .expect("list_torrents should succeed");
    let resumed_item = list_resumed
        .iter()
        .find(|t| t.info_hash == expected_hash)
        .unwrap();
    assert_ne!(resumed_item.status.to_lowercase(), "paused");

    // 8. Restart container and confirm persistence across restart
    let restart_status = std::process::Command::new("docker")
        .args(["restart", "nebulae-server-test-run"])
        .status()
        .expect("docker restart command must execute");
    assert!(
        restart_status.success(),
        "docker restart should exit with 0"
    );

    // Wait for bridge and Transmission to finish starting
    sleep(Duration::from_secs(7));

    // 9. Re-verify connection and torrent persistence after restart
    let status_after = test_rtorrent_server_connection(base_url.to_string(), token.to_string())
        .expect("connection probe should succeed after restart");
    assert_eq!(status_after.name, "nebulae-server");

    let list_after_restart = engine
        .list_torrents()
        .expect("list_torrents should succeed after restart");
    assert!(
        list_after_restart
            .iter()
            .any(|t| t.info_hash == expected_hash),
        "torrent must persist in list after container restart"
    );

    // 10. Clean up torrent
    engine
        .remove_torrent(expected_hash, true)
        .expect("remove_torrent should succeed");
    sleep(Duration::from_millis(500));
    let list_after_remove = engine
        .list_torrents()
        .expect("list_torrents should succeed");
    assert!(
        !list_after_remove
            .iter()
            .any(|t| t.info_hash == expected_hash),
        "torrent must be gone after removal"
    );
}

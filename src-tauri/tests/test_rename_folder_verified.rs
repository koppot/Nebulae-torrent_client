use nebulae_torrent_client_lib::commands::settings::UserSettings;
use nebulae_torrent_client_lib::db::Database;
use nebulae_torrent_client_lib::engine::metainfo::build_torrent_file;
use nebulae_torrent_client_lib::engine::{TorrentEngine, TransmissionTorrentEngine};
use nebulae_torrent_client_lib::utils::paths::AppPaths;
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[test]
fn test_rename_torrent_folder_comprehensive() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_rename_verif_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();

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
    let db = Arc::new(Database::new(&paths.db_file).unwrap());

    let default_dl = temp_root.join("default_downloads");
    fs::create_dir_all(&default_dl).unwrap();

    let settings = UserSettings {
        enable_upnp: false,
        randomize_port_on_startup: true,
        ..Default::default()
    };

    let res_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine = TransmissionTorrentEngine::new(
        paths.clone(),
        db.clone(),
        default_dl.clone(),
        &settings,
        Some(res_dir),
    )
    .expect("Failed to create engine");

    let payload_dir = default_dl.join("ShowRoot");
    let s1_dir = payload_dir.join("Season 1");
    fs::create_dir_all(&s1_dir).unwrap();
    fs::write(s1_dir.join("ep1.bin"), vec![0x31; 16384]).unwrap();
    fs::write(s1_dir.join("ep2.bin"), vec![0x32; 16384]).unwrap();

    let torrent_file =
        build_torrent_file(&payload_dir, &temp_root, 16, &[], true, "test", "creator")
            .expect("build torrent file");

    let torrent_bytes = fs::read(&torrent_file).unwrap();
    let hash = engine
        .add_torrent_from_bytes(&torrent_bytes, Some(&default_dl.to_string_lossy()))
        .expect("add torrent");

    let list = engine.list_torrents().unwrap();
    let numeric_id = list[0].id.clone();
    assert_eq!(list[0].name, "ShowRoot");

    // 1. Rename subfolder using info-hash ID and Windows backslashes:
    // "ShowRoot\\Season 1" -> "Season 01"
    let res1 = engine.rename_torrent_folder(&hash, "ShowRoot\\Season 1", "Season 01");
    assert!(
        res1.is_ok(),
        "Subfolder rename with backslashes and hash ID failed: {:?}",
        res1
    );

    // Verify on disk
    let s01_dir = default_dl.join("ShowRoot").join("Season 01");
    assert!(s01_dir.exists(), "Season 01 dir must exist on disk");
    assert!(!s1_dir.exists(), "Old Season 1 dir must no longer exist");

    // 2. Rename subfolder without root prefix (as relative path):
    // "Season 01" -> "Season 001"
    let res2 = engine.rename_torrent_folder(&numeric_id, "Season 01", "Season 001");
    assert!(
        res2.is_ok(),
        "Subfolder rename without root prefix failed: {:?}",
        res2
    );
    let s001_dir = default_dl.join("ShowRoot").join("Season 001");
    assert!(s001_dir.exists(), "Season 001 dir must exist on disk");

    // 3. Rename top-level root folder using numeric ID:
    // "ShowRoot" -> "RenamedShowRoot"
    let res3 = engine.rename_torrent_folder(&numeric_id, "ShowRoot", "RenamedShowRoot");
    assert!(res3.is_ok(), "Root folder rename failed: {:?}", res3);
    let renamed_root = default_dl.join("RenamedShowRoot");
    assert!(
        renamed_root.exists(),
        "RenamedShowRoot dir must exist on disk"
    );
    assert!(
        !payload_dir.exists(),
        "Old ShowRoot dir must no longer exist"
    );

    // Check torrent display name in engine
    let list_after = engine.list_torrents().unwrap();
    assert_eq!(list_after[0].name, "RenamedShowRoot");

    // 4. Renaming to same name returns Ok(()) as a no-op
    let res4 = engine.rename_torrent_folder(&hash, "RenamedShowRoot", "RenamedShowRoot");
    assert!(res4.is_ok(), "No-op rename must succeed");

    // 5. Invalid name with slash returns error
    let res5 = engine.rename_torrent_folder(&hash, "RenamedShowRoot", "Invalid/Name");
    assert!(res5.is_err(), "Invalid name with slash must fail");

    engine.shutdown_engine();
    let _ = fs::remove_dir_all(&temp_root);
}

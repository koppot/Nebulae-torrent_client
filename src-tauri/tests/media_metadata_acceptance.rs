use nebulae_torrent_client_lib::db::{Database, TorrentDbMetadata};
use std::fs;
use std::sync::Arc;

#[test]
fn test_metadata_poster_saving_and_preservation_independent_of_title_lock() {
    let temp_root = std::env::temp_dir().join(format!(
        "nebulae_meta_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let db_path = temp_root.join("nebulae.db");
    let db = Arc::new(Database::new(&db_path).unwrap());

    let hash = "1111222233334444555566667777888899990000";

    // 1. Insert initial record with raw title and no user lock
    let initial = TorrentDbMetadata {
        info_hash: hash.to_string(),
        display_title: Some("Raw.Torrent.Release.2024.1080p".to_string()),
        category: Some("Movies".to_string()),
        labels: None,
        is_watched: false,
        notes: None,
        added_at: 1700000000,
        completed_on: Some(1700001000),
        save_path: Some("/downloads/Raw.Torrent.Release.2024.1080p".to_string()),
        download_limit_kib: 0,
        upload_limit_kib: 0,
        ratio_limit: 1.0,
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
    db.upsert_torrent_metadata(&initial).unwrap();

    // 2. Set custom poster directly (user_title_locked remains false)
    db.set_torrent_poster(hash, "https://image.tmdb.org/t/p/w500/custom_poster.jpg")
        .unwrap();

    // Verify poster is set, title is unlocked
    let meta1 = db.get_torrent_metadata(hash).unwrap().unwrap();
    assert_eq!(
        meta1.poster_path,
        Some("https://image.tmdb.org/t/p/w500/custom_poster.jpg".to_string())
    );
    assert!(!meta1.user_title_locked, "user_title_locked must be false");

    // 3. Simulate engine / scraper rescan via upsert_torrent_metadata
    let mut rescan_meta = initial.clone();
    rescan_meta.display_title = Some("Updated.Raw.Torrent.Release.2024.1080p".to_string());
    db.upsert_torrent_metadata(&rescan_meta).unwrap();

    // CRITICAL ASSERTION: poster_path is preserved independently of title locking!
    let meta2 = db.get_torrent_metadata(hash).unwrap().unwrap();
    assert_eq!(
        meta2.poster_path,
        Some("https://image.tmdb.org/t/p/w500/custom_poster.jpg".to_string()),
        "Poster must be preserved across rescan even when title is unlocked"
    );
    assert_eq!(
        meta2.display_title,
        Some("Updated.Raw.Torrent.Release.2024.1080p".to_string()),
        "Unlocked display title updates on rescan"
    );

    // 4. Now lock user title
    db.set_user_display_title(hash, "My Custom Movie Title", true)
        .unwrap();
    let meta3 = db.get_torrent_metadata(hash).unwrap().unwrap();
    assert!(
        meta3.user_title_locked,
        "user_title_locked must now be true"
    );
    assert_eq!(
        meta3.user_display_title,
        Some("My Custom Movie Title".to_string())
    );
    assert_eq!(
        meta3.display_title,
        Some("My Custom Movie Title".to_string())
    );

    // 5. Simulate another rescan: locked title must NOT be overwritten
    let mut rescan_meta2 = initial.clone();
    rescan_meta2.display_title = Some("Overwriting.Attempt.2024".to_string());
    db.upsert_torrent_metadata(&rescan_meta2).unwrap();

    let meta4 = db.get_torrent_metadata(hash).unwrap().unwrap();
    assert_eq!(
        meta4.display_title,
        Some("My Custom Movie Title".to_string()),
        "Locked title must not be overwritten"
    );
    assert_eq!(
        meta4.poster_path,
        Some("https://image.tmdb.org/t/p/w500/custom_poster.jpg".to_string()),
        "Poster must remain preserved"
    );

    // 6. Test manual poster ordering
    db.set_torrent_sort_order(hash, Some(42)).unwrap();
    let meta5 = db.get_torrent_metadata(hash).unwrap().unwrap();
    assert_eq!(meta5.sort_order, Some(42));

    // 7. Test automatic External reclassification
    // When a torrent is marked is_external = 1 via mark_as_external, it appears in external queries
    let ext_hash = "2222333344445555666677778888999900001111";
    let external_item = TorrentDbMetadata {
        info_hash: ext_hash.to_string(),
        display_title: Some("External Removed Torrents Feature".to_string()),
        category: Some("Movies".to_string()),
        labels: None,
        is_watched: false,
        notes: None,
        added_at: 1700000000,
        completed_on: Some(1700001000),
        save_path: Some("/external_storage/Disc".to_string()),
        download_limit_kib: 0,
        upload_limit_kib: 0,
        ratio_limit: 1.0,
        sequential_download: false,
        first_last_piece_prio: false,
        super_seeding: false,
        racing_override: None,
        user_display_title: None,
        user_title_locked: false,
        sort_order: Some(1),
        poster_path: None,
        is_external: false,
        sync_miss_count: 5,
        last_checked: None,
    };
    db.upsert_torrent_metadata(&external_item).unwrap();
    db.set_user_display_title(ext_hash, "Retained External Metadata Title", true)
        .unwrap();
    db.set_torrent_poster(ext_hash, "/posters/ext.jpg").unwrap();
    db.mark_as_external(ext_hash).unwrap();

    let ext_meta = db.get_torrent_metadata(ext_hash).unwrap().unwrap();
    assert!(ext_meta.is_external, "is_external must be true");
    assert_eq!(
        ext_meta.user_display_title,
        Some("Retained External Metadata Title".to_string())
    );
    assert!(ext_meta.user_title_locked, "user_title_locked must be true");
    assert_eq!(ext_meta.poster_path, Some("/posters/ext.jpg".to_string()));

    let _ = fs::remove_dir_all(&temp_root);
}

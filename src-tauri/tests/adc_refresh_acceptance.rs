#![cfg(target_os = "windows")]

use nebulae_torrent_client_lib::commands::tracker::{
    build_profile_from_db, fetch_and_persist_adc_stats,
};
use nebulae_torrent_client_lib::db::Database;
use std::path::Path;

#[test]
fn test_windows_authenticated_adc_refresh() {
    let test_db_path = Path::new(r"C:\nebulae-test\adc_check\nebulae.db");
    if !test_db_path.exists() {
        eprintln!(
            "SKIPPING: isolated test db not found at C:\\nebulae-test\\adc_check\\nebulae.db"
        );
        return;
    }

    let db = Database::new(test_db_path).expect("Failed to open isolated test database");

    // 1. Build initial profile
    let initial_profile = build_profile_from_db(&db).expect("build_profile_from_db failed");
    println!(
        "Initial profile: is_linked={}, username={}",
        initial_profile.is_linked, initial_profile.username
    );
    assert!(
        initial_profile.is_linked,
        "Account must be linked in test database"
    );

    // 2. Perform live authenticated fetch_and_persist_adc_stats
    let result = fetch_and_persist_adc_stats(&db);
    match result {
        Ok(updated) => {
            println!("fetch_and_persist_adc_stats returned: {updated}");
            assert!(updated, "Expected stats fetch to update database");

            // 3. Build refreshed profile
            let fresh =
                build_profile_from_db(&db).expect("build_profile_from_db failed after refresh");
            println!(
                "Refreshed profile: username={}, uploaded={}, downloaded={}, bonus_points={}, ratio={}",
                fresh.username,
                fresh.uploaded_bytes,
                fresh.downloaded_bytes,
                fresh.bonus_points,
                fresh.ratio
            );
            assert!(!fresh.username.is_empty(), "Username must not be empty");
            assert!(
                fresh.uploaded_bytes > 0 || fresh.downloaded_bytes > 0,
                "Uploaded or downloaded bytes must be populated from tracker"
            );
        }
        Err(e) => {
            panic!("Live ADC stats fetch failed: {e}");
        }
    }
}

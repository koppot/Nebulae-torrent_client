#![cfg(target_os = "macos")]

use nebulae_torrent_client_lib::commands::tracker::fetch_and_persist_adc_stats;
use nebulae_torrent_client_lib::db::Database;
use nebulae_torrent_client_lib::utils::keyring::{
    clear_session_store_for_test, delete_secret, get_secret, passkey_secret_target, store_secret,
    trakt_client_id_target, vpn_secret_target,
};

#[test]
fn test_macos_credential_crud_lifecycle() {
    clear_session_store_for_test();

    let target = "test:sample_credential";
    let secret = "sample_secret_token_12345";

    // 1. Missing credential read returns Err immediately (no hang, no prompt)
    let initial = get_secret(target);
    assert!(
        initial.is_err(),
        "Reading a missing secret must return Err, not empty or default"
    );

    // 2. Store credential in session memory
    store_secret(target, secret).expect("store_secret must succeed");

    // 3. Read back credential
    let retrieved = get_secret(target).expect("get_secret must succeed after store");
    assert_eq!(retrieved, secret);

    // 4. Delete credential
    delete_secret(target).expect("delete_secret must succeed");

    // 5. Read back after deletion returns Err
    let after_delete = get_secret(target);
    assert!(
        after_delete.is_err(),
        "Reading a deleted secret must return Err"
    );
}

#[test]
fn test_macos_trakt_and_vpn_session_keys() {
    clear_session_store_for_test();

    let trakt_key = trakt_client_id_target();
    let vpn_key = vpn_secret_target("test-profile-uuid");

    assert!(get_secret(&trakt_key).is_err());
    assert!(get_secret(&vpn_key).is_err());

    store_secret(&trakt_key, "trakt_id_abc").expect("store trakt failed");
    store_secret(&vpn_key, "wg_private_key_xyz").expect("store vpn failed");

    assert_eq!(get_secret(&trakt_key).unwrap(), "trakt_id_abc");
    assert_eq!(get_secret(&vpn_key).unwrap(), "wg_private_key_xyz");

    delete_secret(&trakt_key).expect("delete trakt failed");
    delete_secret(&vpn_key).expect("delete vpn failed");

    assert!(get_secret(&trakt_key).is_err());
    assert!(get_secret(&vpn_key).is_err());
}

#[test]
fn test_macos_tracker_passkey_storage_and_sqlite_exclusion() {
    clear_session_store_for_test();

    let temp_root = std::env::temp_dir().join(format!("test_keychain_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).expect("Failed to create tempdir");
    let db_path = temp_root.join("test_keychain.db");
    let db = Database::new(&db_path).expect("Failed to initialize test database");

    let passkey_target = passkey_secret_target();
    let test_passkey = "my_super_secret_adc_passkey_1234";

    // Store in session storage
    store_secret(&passkey_target, test_passkey).expect("Failed to store passkey in session store");
    assert_eq!(get_secret(&passkey_target).unwrap(), test_passkey);

    // Verify that on macOS, SQLite database has NOT been populated with plaintext adc_passkey
    let stored_in_db = db.get_setting("adc_passkey").expect("get_setting failed");
    assert!(
        stored_in_db.is_none(),
        "Plaintext adc_passkey must NOT be stored in SQLite on macOS"
    );

    // Verify that session store deletion clears the passkey
    delete_secret(&passkey_target).expect("Failed to delete passkey");
    assert!(get_secret(&passkey_target).is_err());

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn test_macos_stats_refresh_graceful_when_credentials_missing_at_startup() {
    clear_session_store_for_test();

    let temp_root = std::env::temp_dir().join(format!("test_startup_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).expect("Failed to create tempdir");
    let db_path = temp_root.join("test_startup.db");
    let db = Database::new(&db_path).expect("Failed to initialize test database");

    // Set adc_linked = true in DB, simulating an account linked in a prior session
    db.set_setting("adc_linked", "true")
        .expect("Failed to set adc_linked");

    // Ensure session store is completely empty (simulating app cold launch)
    clear_session_store_for_test();
    assert!(get_secret(&passkey_secret_target()).is_err());

    // Call fetch_and_persist_adc_stats - must return Ok(false) without attempting network,
    // without hanging on Keychain prompt, and without crashing
    let refresh_result = fetch_and_persist_adc_stats(&db);
    assert!(
        refresh_result.is_ok(),
        "Refresh with missing session credentials must return Ok"
    );
    assert!(
        !refresh_result.unwrap(),
        "Refresh must return false when passkey is not in session store"
    );

    let _ = std::fs::remove_dir_all(&temp_root);
}

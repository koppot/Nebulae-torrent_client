use tracing::debug;
#[cfg(not(target_os = "macos"))]
use tracing::warn;

#[cfg(not(target_os = "macos"))]
use keyring::Entry;

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::sync::{OnceLock, RwLock};

#[cfg(not(target_os = "macos"))]
const SERVICE_NAME: &str = "nebulae_torrent_client";

#[cfg(target_os = "macos")]
static SESSION_CREDENTIALS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn get_session_map() -> &'static RwLock<HashMap<String, String>> {
    SESSION_CREDENTIALS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Store a secret.
///
/// On macOS, OS Keychain access is strictly prohibited. Secrets are kept in an
/// in-memory session map for the lifetime of the process only and are NOT persisted.
///
/// On Windows, secrets are stored in Windows Credential Manager via the keyring crate.
pub fn store_secret(target: &str, secret: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut map = get_session_map()
            .write()
            .map_err(|e| format!("Session credentials lock poisoned: {e}"))?;
        map.insert(target.to_string(), secret.to_string());
        debug!(
            target = target,
            "Stored credential in in-memory session store (macOS Keychain access disabled)"
        );
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let entry = Entry::new(SERVICE_NAME, target)
            .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
        entry
            .set_password(secret)
            .map_err(|e| format!("Failed to store secret for {target} in OS keychain: {e}"))?;
        debug!(target = target, "Stored secret in OS keychain");
        Ok(())
    }
}

pub fn trakt_client_id_target() -> String {
    "trakt:client_id".to_string()
}

/// Retrieve a secret.
///
/// On macOS, reads exclusively from the in-memory session map. Never calls Keychain Services.
///
/// On Windows, reads from Windows Credential Manager.
pub fn get_secret(target: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let map = get_session_map()
            .read()
            .map_err(|e| format!("Session credentials lock poisoned: {e}"))?;
        map.get(target)
            .cloned()
            .ok_or_else(|| format!("Secret for {target} not found in session storage"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let entry = Entry::new(SERVICE_NAME, target)
            .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
        entry
            .get_password()
            .map_err(|e| format!("Failed to read secret for {target} from OS keychain: {e}"))
    }
}

/// Delete a secret.
///
/// On macOS, removes from the in-memory session map. Never touches user's Keychain entries.
///
/// On Windows, deletes from Windows Credential Manager.
pub fn delete_secret(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut map = get_session_map()
            .write()
            .map_err(|e| format!("Session credentials lock poisoned: {e}"))?;
        map.remove(target);
        debug!(
            target = target,
            "Deleted secret from in-memory session store"
        );
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let entry = Entry::new(SERVICE_NAME, target)
            .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
        match entry.delete_credential() {
            Ok(_) => {
                debug!(target = target, "Deleted secret from OS keychain");
                Ok(())
            }
            Err(e) => {
                warn!(target = target, "Secret deletion encountered: {e}");
                Ok(())
            }
        }
    }
}

pub fn vpn_secret_target(profile_id: &str) -> String {
    format!("vpn:{}", profile_id)
}

pub fn passkey_secret_target() -> String {
    "adc:passkey".to_string()
}

#[cfg(target_os = "macos")]
pub fn clear_session_store_for_test() {
    if let Ok(mut map) = get_session_map().write() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_target_format() {
        assert_eq!(vpn_secret_target("test-uuid-123"), "vpn:test-uuid-123");
        assert_eq!(passkey_secret_target(), "adc:passkey");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_session_storage_roundtrip_and_deletion() {
        clear_session_store_for_test();

        let target = "test:macos_session_key";
        let secret = "test_secret_value_xyz";

        // Initial state: not found
        assert!(get_secret(target).is_err());

        // Store
        store_secret(target, secret).expect("store_secret failed");

        // Read back
        let retrieved = get_secret(target).expect("get_secret failed");
        assert_eq!(retrieved, secret);

        // Delete
        delete_secret(target).expect("delete_secret failed");

        // After deletion: not found
        assert!(get_secret(target).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_relaunch_simulation_clears_memory() {
        let target = "test:relaunch_key";
        store_secret(target, "temp_data").expect("store_secret failed");
        assert_eq!(get_secret(target).unwrap(), "temp_data");

        // Simulate app restart by clearing the in-memory session store
        clear_session_store_for_test();

        // Must return Err, demonstrating that credentials require re-entry on restart
        let res = get_secret(target);
        assert!(
            res.is_err(),
            "Credentials must not persist across simulated restarts"
        );
    }
}

#[cfg(not(target_os = "macos"))]
use keyring::Entry;
use tracing::{debug, warn};

#[cfg(not(target_os = "macos"))]
const SERVICE_NAME: &str = "nebulae_torrent_client";

#[cfg(target_os = "macos")]
fn secrets_path() -> Result<std::path::PathBuf, String> {
    let internal_dir = dirs::data_dir()
        .map(|d| d.join("Nebulae").join("internal"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".nebulae")))
        .ok_or_else(|| {
            "Could not determine local data directory for secrets storage".to_string()
        })?;

    if !internal_dir.exists() {
        let _ = std::fs::create_dir_all(&internal_dir);
    }
    Ok(internal_dir.join("secrets.json"))
}

#[cfg(target_os = "macos")]
fn load_secrets() -> std::collections::HashMap<String, String> {
    if let Ok(p) = secrets_path() {
        if let Ok(data) = std::fs::read(&p) {
            if let Ok(map) = serde_json::from_slice(&data) {
                return map;
            }
        }
    }
    std::collections::HashMap::new()
}

#[cfg(target_os = "macos")]
fn save_secrets(map: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let p = secrets_path()?;
    let json_bytes =
        serde_json::to_vec_pretty(map).map_err(|e| format!("Failed to serialize secrets: {e}"))?;

    use std::fs::OpenOptions;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    #[cfg(unix)]
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&p)
        .map_err(|e| format!("Failed to open secrets file {}: {e}", p.display()))?;

    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&p)
        .map_err(|e| format!("Failed to open secrets file {}: {e}", p.display()))?;

    use std::io::Write;
    file.write_all(&json_bytes)
        .map_err(|e| format!("Failed to write secrets file: {e}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn store_secret(target: &str, secret: &str) -> Result<(), String> {
    let mut map = load_secrets();
    map.insert(target.to_string(), secret.to_string());
    save_secrets(&map)?;
    debug!(
        target = target,
        "Stored secret in local isolated secrets store (macOS keychain bypassed)"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn store_secret(target: &str, secret: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, target)
        .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
    entry
        .set_password(secret)
        .map_err(|e| format!("Failed to store secret for {target} in OS keychain: {e}"))?;
    debug!(target = target, "Stored secret in OS keychain");
    Ok(())
}

pub fn trakt_client_id_target() -> String {
    "trakt:client_id".to_string()
}

#[cfg(target_os = "macos")]
pub fn get_secret(target: &str) -> Result<String, String> {
    let map = load_secrets();
    map.get(target)
        .cloned()
        .ok_or_else(|| format!("Secret for {target} not found in local secrets store"))
}

#[cfg(not(target_os = "macos"))]
pub fn get_secret(target: &str) -> Result<String, String> {
    let entry = Entry::new(SERVICE_NAME, target)
        .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
    entry
        .get_password()
        .map_err(|e| format!("Failed to read secret for {target} from OS keychain: {e}"))
}

#[cfg(target_os = "macos")]
pub fn delete_secret(target: &str) -> Result<(), String> {
    let mut map = load_secrets();
    if map.remove(target).is_some() {
        if let Err(e) = save_secrets(&map) {
            warn!(target = target, "Failed to persist deleted secret: {e}");
        }
    }
    debug!(
        target = target,
        "Deleted secret from local isolated secrets store"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn delete_secret(target: &str) -> Result<(), String> {
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

pub fn vpn_secret_target(profile_id: &str) -> String {
    format!("vpn:{}", profile_id)
}

pub fn passkey_secret_target() -> String {
    "adc:passkey".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_target_format() {
        assert_eq!(vpn_secret_target("test-uuid-123"), "vpn:test-uuid-123");
        assert_eq!(passkey_secret_target(), "adc:passkey");
    }

    #[test]
    fn test_secret_store_and_retrieve_roundtrip() {
        let target = format!("test:unit_test_{}", uuid::Uuid::new_v4());
        let secret = "super_secret_token_12345";

        assert!(get_secret(&target).is_err());
        assert!(store_secret(&target, secret).is_ok());
        assert_eq!(get_secret(&target).unwrap(), secret);
        assert!(delete_secret(&target).is_ok());
        assert!(get_secret(&target).is_err());
    }
}

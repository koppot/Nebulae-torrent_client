use keyring::Entry;
use tracing::{debug, warn};

const SERVICE_NAME: &str = "nebulae_torrent_client";

pub fn store_secret(target: &str, secret: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, target)
        .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
    entry
        .set_password(secret)
        .map_err(|e| format!("Failed to store secret for {target} in OS keychain: {e}"))?;
    debug!(target = target, "Stored secret in OS keychain");
    Ok(())
}

pub fn get_secret(target: &str) -> Result<String, String> {
    let entry = Entry::new(SERVICE_NAME, target)
        .map_err(|e| format!("Keyring initialization error for {target}: {e}"))?;
    entry
        .get_password()
        .map_err(|e| format!("Failed to read secret for {target} from OS keychain: {e}"))
}

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
}

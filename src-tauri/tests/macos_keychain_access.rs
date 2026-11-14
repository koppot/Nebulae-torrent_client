#![cfg(target_os = "macos")]

use nebulae_torrent_client_lib::utils::keyring::{passkey_secret_target, CredentialError};

#[test]
fn credential_failures_have_safe_adc_states() {
    assert_eq!(CredentialError::Missing.adc_state(), "credential_required");
    assert_eq!(
        CredentialError::AccessDenied.adc_state(),
        "credential_access_denied"
    );
    assert_eq!(
        CredentialError::StorageUnavailable.adc_state(),
        "credential_storage_unavailable"
    );
    assert_eq!(
        CredentialError::StorageError.adc_state(),
        "credential_storage_error"
    );
    assert_eq!(passkey_secret_target(), "adc:passkey");
}

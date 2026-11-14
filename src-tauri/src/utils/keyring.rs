//! Platform credential storage.
//!
//! macOS uses Security.framework SecItem operations with request-scoped
//! `kSecUseAuthenticationUIFail` for automatic access. An explicit user action
//! may retry without that option, allowing macOS to show its normal Keychain
//! authorization UI. No process-global interaction setting is changed.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};
use tracing::debug;

#[cfg(not(target_os = "macos"))]
use keyring::Entry;

const SERVICE_NAME: &str = "nebulae_torrent_client";

#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;
#[cfg(target_os = "macos")]
use core_foundation::string::{CFString, CFStringRef};
#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password_options, generic_password, set_generic_password_options,
    PasswordOptions,
};

#[cfg(target_os = "macos")]
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecUseAuthenticationUI: CFStringRef;
    static kSecUseAuthenticationUIFail: CFStringRef;
}

/// Safe, user-facing classification of a credential lookup failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialError {
    Missing,
    AccessDenied,
    StorageUnavailable,
    StorageError,
}

impl CredentialError {
    pub fn adc_state(self) -> &'static str {
        match self {
            Self::Missing => "credential_required",
            Self::AccessDenied => "credential_access_denied",
            Self::StorageUnavailable => "credential_storage_unavailable",
            Self::StorageError => "credential_storage_error",
        }
    }
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "Credential is not available.",
            Self::AccessDenied => "Secure credential access was denied or requires confirmation.",
            Self::StorageUnavailable => "The platform credential store is unavailable.",
            Self::StorageError => "The platform credential store returned an error.",
        })
    }
}

#[derive(Default)]
struct CachedCredential {
    secret: Option<String>,
    failure: Option<CredentialError>,
}

static CREDENTIALS: OnceLock<Mutex<HashMap<String, Arc<Mutex<CachedCredential>>>>> =
    OnceLock::new();

fn credential_cache() -> &'static Mutex<HashMap<String, Arc<Mutex<CachedCredential>>>> {
    CREDENTIALS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn target_cache(target: &str) -> Arc<Mutex<CachedCredential>> {
    let mut caches = credential_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(
        caches
            .entry(target.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(CachedCredential::default()))),
    )
}

fn cached_read(
    cache: &mut CachedCredential,
    force_retry: bool,
    read: impl FnOnce() -> Result<String, CredentialError>,
) -> Result<String, CredentialError> {
    if !force_retry {
        if let Some(secret) = &cache.secret {
            return Ok(secret.clone());
        }
        if let Some(error) = cache.failure {
            return Err(error);
        }
    } else {
        cache.failure = None;
    }

    match read() {
        Ok(secret) if !secret.trim().is_empty() => {
            cache.secret = Some(secret.clone());
            cache.failure = None;
            Ok(secret)
        }
        Ok(_) => {
            cache.secret = None;
            cache.failure = Some(CredentialError::Missing);
            Err(CredentialError::Missing)
        }
        Err(error) => {
            cache.secret = None;
            cache.failure = Some(error);
            Err(error)
        }
    }
}

fn cached_write(
    cache: &mut CachedCredential,
    secret: &str,
    write: impl FnOnce() -> Result<(), CredentialError>,
) -> Result<(), CredentialError> {
    if secret.trim().is_empty() {
        return Err(CredentialError::StorageError);
    }
    write()?;
    cache.secret = Some(secret.to_string());
    cache.failure = None;
    Ok(())
}

fn cached_delete(
    cache: &mut CachedCredential,
    delete: impl FnOnce() -> Result<(), CredentialError>,
) -> Result<(), CredentialError> {
    match delete() {
        Ok(()) | Err(CredentialError::Missing) => {
            cache.secret = None;
            cache.failure = Some(CredentialError::Missing);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Read a credential without allowing Keychain authorization UI.
///
/// Failures are memoized per target, so background refreshes do not repeatedly
/// probe an inaccessible credential. An explicit user action should call
/// [`get_secret_with_user_authorization`] to retry.
pub fn get_secret(target: &str) -> Result<String, CredentialError> {
    let target_cache = target_cache(target);
    let mut cache = target_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cached_read(&mut cache, false, || read_from_store(target, false))
}

/// Retry a credential read after an explicit user action.
///
/// On macOS this permits the normal Keychain UI. Callers must run this on a
/// blocking worker and only in response to a user choosing an unlock action.
pub fn get_secret_with_user_authorization(target: &str) -> Result<String, CredentialError> {
    let target_cache = target_cache(target);
    let mut cache = target_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cached_read(&mut cache, true, || read_from_store(target, true))
}

/// Store a credential without allowing Keychain authorization UI.
pub fn store_secret(target: &str, secret: &str) -> Result<(), CredentialError> {
    store_secret_inner(target, secret, false)
}

/// Store a credential after an explicit user action, allowing macOS to
/// authorize access to an existing item created by another app signature.
pub fn store_secret_with_user_authorization(
    target: &str,
    secret: &str,
) -> Result<(), CredentialError> {
    store_secret_inner(target, secret, true)
}

fn store_secret_inner(
    target: &str,
    secret: &str,
    allow_user_interaction: bool,
) -> Result<(), CredentialError> {
    if secret.trim().is_empty() {
        return Err(CredentialError::StorageError);
    }
    let target_cache = target_cache(target);
    let mut cache = target_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cached_write(&mut cache, secret, || {
        write_to_store(target, secret, allow_user_interaction)
    })?;
    debug!(
        target = target,
        "Stored credential in platform secure storage"
    );
    Ok(())
}

/// Delete a credential without allowing Keychain authorization UI.
pub fn delete_secret(target: &str) -> Result<(), CredentialError> {
    delete_secret_inner(target, false)
}

/// Delete a credential after an explicit user action, allowing macOS to show
/// Keychain authorization UI if the item requires it.
pub fn delete_secret_with_user_authorization(target: &str) -> Result<(), CredentialError> {
    delete_secret_inner(target, true)
}

fn delete_secret_inner(target: &str, allow_user_interaction: bool) -> Result<(), CredentialError> {
    let target_cache = target_cache(target);
    let mut cache = target_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cached_delete(&mut cache, || {
        delete_from_store(target, allow_user_interaction)
    })
}

pub fn trakt_client_id_target() -> String {
    "trakt:client_id".to_string()
}

pub fn vpn_secret_target(profile_id: &str) -> String {
    format!("vpn:{profile_id}")
}

pub fn passkey_secret_target() -> String {
    "adc:passkey".to_string()
}

#[cfg(target_os = "macos")]
fn password_options(target: &str, allow_user_interaction: bool) -> PasswordOptions {
    let mut options = PasswordOptions::new_generic_password(SERVICE_NAME, target);
    if !allow_user_interaction {
        // The authentication-UI policy is part of this SecItem request. Unlike
        // SecKeychainSetUserInteractionAllowed, it does not mutate process-wide
        // state or race unrelated Keychain operations.
        let (key, value) = unsafe {
            (
                CFString::wrap_under_get_rule(kSecUseAuthenticationUI),
                CFString::wrap_under_get_rule(kSecUseAuthenticationUIFail),
            )
        };
        #[allow(deprecated)]
        options.query.push((key, value.into_CFType()));
    }
    options
}

#[cfg(target_os = "macos")]
fn read_from_store(target: &str, allow_user_interaction: bool) -> Result<String, CredentialError> {
    generic_password(password_options(target, allow_user_interaction))
        .map_err(classify_security_error)
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| CredentialError::StorageError))
}

#[cfg(target_os = "macos")]
fn write_to_store(
    target: &str,
    secret: &str,
    allow_user_interaction: bool,
) -> Result<(), CredentialError> {
    set_generic_password_options(
        secret.as_bytes(),
        password_options(target, allow_user_interaction),
    )
    .map_err(classify_security_error)
}

#[cfg(target_os = "macos")]
fn delete_from_store(target: &str, allow_user_interaction: bool) -> Result<(), CredentialError> {
    delete_generic_password_options(password_options(target, allow_user_interaction))
        .map_err(classify_security_error)
}

#[cfg(target_os = "macos")]
fn classify_security_error(error: security_framework::base::Error) -> CredentialError {
    classify_security_status(error.code())
}

#[cfg(target_os = "macos")]
fn classify_security_status(status: i32) -> CredentialError {
    match status {
        -25300 => CredentialError::Missing, // errSecItemNotFound
        -25308 | -25315 | -25293 | -25243 | -128 => CredentialError::AccessDenied,
        -25291 | -25292 | -25294 | -25295 => CredentialError::StorageUnavailable,
        _ => CredentialError::StorageError,
    }
}

#[cfg(not(target_os = "macos"))]
fn read_from_store(target: &str, _allow_user_interaction: bool) -> Result<String, CredentialError> {
    Entry::new(SERVICE_NAME, target)
        .map_err(classify_keyring_error)?
        .get_password()
        .map_err(classify_keyring_error)
}

#[cfg(not(target_os = "macos"))]
fn write_to_store(
    target: &str,
    secret: &str,
    _allow_user_interaction: bool,
) -> Result<(), CredentialError> {
    Entry::new(SERVICE_NAME, target)
        .map_err(classify_keyring_error)?
        .set_password(secret)
        .map_err(classify_keyring_error)
}

#[cfg(not(target_os = "macos"))]
fn delete_from_store(target: &str, _allow_user_interaction: bool) -> Result<(), CredentialError> {
    Entry::new(SERVICE_NAME, target)
        .map_err(classify_keyring_error)?
        .delete_credential()
        .map_err(classify_keyring_error)
}

#[cfg(not(target_os = "macos"))]
fn classify_keyring_error(error: keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoEntry => CredentialError::Missing,
        keyring::Error::NoStorageAccess(_) => CredentialError::AccessDenied,
        keyring::Error::PlatformFailure(_) => CredentialError::StorageUnavailable,
        _ => CredentialError::StorageError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_secret_target_format() {
        assert_eq!(vpn_secret_target("test-uuid-123"), "vpn:test-uuid-123");
        assert_eq!(passkey_secret_target(), "adc:passkey");
        assert_eq!(trakt_client_id_target(), "trakt:client_id");
    }

    #[test]
    fn failed_lookup_is_memoized_and_explicit_retry_recovers() {
        let calls = AtomicUsize::new(0);
        let mut cache = CachedCredential::default();

        let first = cached_read(&mut cache, false, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(CredentialError::AccessDenied)
        });
        assert_eq!(first, Err(CredentialError::AccessDenied));

        let second = cached_read(&mut cache, false, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok("unexpected".to_string())
        });
        assert_eq!(second, Err(CredentialError::AccessDenied));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let recovered = cached_read(&mut cache, true, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok("synthetic-secret".to_string())
        });
        assert_eq!(recovered.as_deref(), Ok("synthetic-secret"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            cached_read(&mut cache, false, || panic!("cached value must be used")).as_deref(),
            Ok("synthetic-secret")
        );
    }

    #[test]
    fn synthetic_credential_lifecycle_survives_a_fresh_cache() {
        let synthetic_secret = "synthetic-keychain-lifecycle-secret";
        let mut synthetic_secure_store = HashMap::<String, String>::new();
        let mut first_process_cache = CachedCredential::default();

        cached_write(&mut first_process_cache, synthetic_secret, || {
            synthetic_secure_store
                .insert("synthetic-target".to_string(), synthetic_secret.to_string());
            Ok(())
        })
        .unwrap();

        // A new cache models a fresh process; its first read comes from the
        // persistent backend rather than process memory.
        let mut fresh_process_cache = CachedCredential::default();
        let read = cached_read(&mut fresh_process_cache, false, || {
            synthetic_secure_store
                .get("synthetic-target")
                .cloned()
                .ok_or(CredentialError::Missing)
        });
        assert_eq!(read.as_deref(), Ok(synthetic_secret));

        cached_delete(&mut fresh_process_cache, || {
            synthetic_secure_store
                .remove("synthetic-target")
                .map(|_| ())
                .ok_or(CredentialError::Missing)
        })
        .unwrap();
        assert_eq!(
            cached_read(&mut fresh_process_cache, false, || {
                panic!("a deleted item must be memoized as missing")
            }),
            Err(CredentialError::Missing)
        );
        assert!(synthetic_secure_store.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_error_codes_map_to_distinct_states() {
        assert_eq!(classify_security_status(-25300), CredentialError::Missing);
        assert_eq!(
            classify_security_status(-25308),
            CredentialError::AccessDenied
        );
        assert_eq!(
            classify_security_status(-25315),
            CredentialError::AccessDenied
        );
        assert_eq!(
            classify_security_status(-25291),
            CredentialError::StorageUnavailable
        );
        assert_eq!(classify_security_status(-50), CredentialError::StorageError);
    }
}

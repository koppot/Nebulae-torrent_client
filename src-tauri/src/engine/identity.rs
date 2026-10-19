use serde::{Deserialize, Serialize};
use std::path::Path;

/// Canonical tracker recognition pattern for Nebulae User-Agent:
/// /^Nebulae\/0\.([0-9]+)\.([0-9]+)/
pub const NEBULAE_USER_AGENT_PATTERN: &str = r"^Nebulae/0\.([0-9]+)\.([0-9]+)";

/// Canonical tracker recognition pattern for Nebulae Peer-ID prefix:
/// /^-NE0([0-9])([0-9])0-/
pub const NEBULAE_PEER_ID_PATTERN: &str = r"^-NE0([0-9])([0-9])0-";

/// Authoritative pinned Transmission release version bundled with Nebulae.
pub const BUNDLED_TRANSMISSION_VERSION: &str = "4.1.3";

/// Derives the 8-byte Azureus-style Peer ID prefix from an application semver string.
/// Format: `-NE0{minor}{patch}0-`
/// For 0.2.2: `-NE0220-`
/// For 0.2.3: `-NE0230-`
///
/// Encoding decision:
/// - Strictly conforms to tracker recognition pattern `/^-NE0([0-9])([0-9])0-/`.
/// - Major version MUST be 0.
/// - Minor and patch components MUST each be a single ASCII digit (0..=9).
/// - No clamping, no Base62 substitutions, and no beta/dev markers that violate the regex.
/// - Any unsupported version returns a descriptive `Err` reporting the limitation.
pub fn derive_nebulae_peer_id_prefix(version_str: &str) -> Result<String, String> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() < 3 {
        return Err(format!(
            "Invalid version format '{version_str}': expected major.minor.patch"
        ));
    }

    let major: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid major version '{}': must be numeric", parts[0]))?;

    if major != 0 {
        return Err(format!(
            "Unsupported version encoding: major version {major} cannot be represented in tracker pattern /^-NE0([0-9])([0-9])0-/"
        ));
    }

    let minor: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid minor version '{}': must be numeric", parts[1]))?;

    if minor > 9 {
        return Err(format!(
            "Unsupported version encoding: multi-digit minor version {minor} exceeds single-digit tracker pattern /^-NE0([0-9])([0-9])0-/"
        ));
    }

    let patch_raw = parts[2];
    if patch_raw.contains('-') || patch_raw.contains('+') {
        return Err(format!(
            "Unsupported version encoding: pre-release tag in '{patch_raw}' violates single-digit tracker pattern /^-NE0([0-9])([0-9])0-/"
        ));
    }

    let patch: u32 = patch_raw
        .parse()
        .map_err(|_| format!("Invalid patch version '{patch_raw}': must be numeric"))?;

    if patch > 9 {
        return Err(format!(
            "Unsupported version encoding: multi-digit patch version {patch} exceeds single-digit tracker pattern /^-NE0([0-9])([0-9])0-/"
        ));
    }

    let prefix = format!("-NE0{minor}{patch}0-");
    debug_assert_eq!(prefix.len(), 8);
    Ok(prefix)
}

/// Derives the client HTTP User-Agent string from the application semver.
/// For 0.2.2: `Nebulae/0.2.2`
/// Validates conformance against `/^Nebulae\/0\.([0-9]+)\.([0-9]+)/`.
pub fn derive_nebulae_user_agent(version_str: &str) -> Result<String, String> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() < 3 {
        return Err(format!(
            "Invalid version format '{version_str}': expected major.minor.patch"
        ));
    }

    let major: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid major version '{}': must be numeric", parts[0]))?;

    if major != 0 {
        return Err(format!(
            "Unsupported version encoding: major version {major} cannot be represented in tracker pattern /^Nebulae\\/0\\.([0-9]+)\\.([0-9]+)/"
        ));
    }

    let minor_str = parts[1];
    if minor_str.is_empty() || !minor_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "Invalid minor version '{minor_str}': must be numeric"
        ));
    }

    let patch_raw = parts[2].split('-').next().unwrap_or("");
    if patch_raw.is_empty() || !patch_raw.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "Invalid patch version in '{}': must be numeric",
            parts[2]
        ));
    }

    Ok(format!("Nebulae/{version_str}"))
}

/// Returns the current application version from Cargo metadata.
pub fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the authoritative bundled Transmission daemon upstream version.
pub fn get_bundled_transmission_version() -> &'static str {
    BUNDLED_TRANSMISSION_VERSION
}

/// Derives the upstream Transmission User-Agent from a version string.
pub fn derive_transmission_user_agent(version: &str) -> String {
    format!("Transmission/{version}")
}

/// Derives the upstream Transmission 8-byte Peer ID prefix from a version string.
/// For Transmission 4.1.3: `-TR4130-`
pub fn derive_transmission_peer_id_prefix(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts
        .first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(4);
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let patch = parts
        .get(2)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(3);

    format!("-TR{}{}{}0-", major % 10, minor % 10, patch % 10)
}

/// Returns the upstream Transmission User-Agent for the bundled version.
pub fn get_bundled_transmission_user_agent() -> String {
    derive_transmission_user_agent(get_bundled_transmission_version())
}

/// Returns the upstream Transmission 8-byte Peer ID prefix for the bundled version.
pub fn get_bundled_transmission_peer_id_prefix() -> String {
    derive_transmission_peer_id_prefix(get_bundled_transmission_version())
}

/// Probes whether the located transmission-daemon binary supports runtime identity overrides.
/// The presence of the symbol name is a capability hint, not proof of the active identity.
pub fn check_daemon_identity_support(daemon_path: &Path) -> bool {
    if let Ok(bytes) = std::fs::read(daemon_path) {
        let needle1 = b"TRANSMISSION_PEER_ID_PREFIX";
        let needle2 = b"TRANSMISSION_USER_AGENT";
        bytes.windows(needle1.len()).any(|w| w == needle1)
            && bytes.windows(needle2.len()).any(|w| w == needle2)
    } else {
        false
    }
}

/// Status payload returned to the Settings UI to accurately represent
/// active engine identity, configured profile, and engine capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineIdentityInfo {
    pub profile: String,
    pub configured_user_agent: String,
    pub configured_peer_id_prefix: String,
    pub active_user_agent: String,
    pub active_peer_id_prefix: String,
    pub bundled_transmission_version: String,
    pub bundled_transmission_user_agent: String,
    pub bundled_transmission_peer_id_prefix: String,
    pub nebulae_version: String,
    pub nebulae_user_agent: String,
    pub nebulae_peer_id_prefix: String,
    pub supports_runtime_overrides: bool,
    pub restart_required: bool,
    pub status_message: String,
    pub version_encoding_supported: bool,
    pub version_encoding_limitation: Option<String>,
}

/// Validates whether a User-Agent matches canonical tracker recognition pattern:
/// /^Nebulae\/0\.([0-9]+)\.([0-9]+)/
pub fn matches_tracker_user_agent_pattern(ua: &str) -> bool {
    if !ua.starts_with("Nebulae/0.") {
        return false;
    }
    let rest = &ua["Nebulae/0.".len()..];
    let parts: Vec<&str> = rest.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let minor = parts[0];
    let patch = parts[1].split('-').next().unwrap_or("");
    !minor.is_empty()
        && minor.chars().all(|c| c.is_ascii_digit())
        && !patch.is_empty()
        && patch.chars().all(|c| c.is_ascii_digit())
}

/// Validates whether an 8-byte Peer ID prefix matches canonical tracker recognition pattern:
/// /^-NE0([0-9])([0-9])0-/
pub fn matches_tracker_peer_id_pattern(prefix: &str) -> bool {
    let bytes = prefix.as_bytes();
    if bytes.len() != 8 {
        return false;
    }
    bytes.starts_with(b"-NE0")
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
        && bytes[6] == b'0'
        && bytes[7] == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nebulae_0_2_2_identity_and_patterns() {
        let version = "0.2.2";
        let ua = derive_nebulae_user_agent(version).expect("0.2.2 UA must succeed");
        let prefix = derive_nebulae_peer_id_prefix(version).expect("0.2.2 prefix must succeed");

        assert_eq!(ua, "Nebulae/0.2.2");
        assert_eq!(prefix, "-NE0220-");
        assert_eq!(prefix.len(), 8, "Peer ID prefix must be exactly 8 bytes");

        // Verify tracker regex patterns match
        assert!(
            matches_tracker_user_agent_pattern(&ua),
            "User-Agent 0.2.2 must match tracker recognition regex"
        );
        assert!(
            matches_tracker_peer_id_pattern(&prefix),
            "Peer ID prefix -NE0220- must match tracker recognition regex"
        );

        // Simulate full 20-byte Peer ID with 12 random alphanumeric characters
        let full_peer_id = format!("{prefix}a1b2c3d4e5f6");
        assert_eq!(
            full_peer_id.len(),
            20,
            "Full BitTorrent peer ID must be exactly 20 bytes"
        );
    }

    #[test]
    fn test_nebulae_0_2_3_identity_and_patterns() {
        let version = "0.2.3";
        let ua = derive_nebulae_user_agent(version).expect("0.2.3 UA must succeed");
        let prefix = derive_nebulae_peer_id_prefix(version).expect("0.2.3 prefix must succeed");

        assert_eq!(ua, "Nebulae/0.2.3");
        assert_eq!(prefix, "-NE0230-");
        assert_eq!(prefix.len(), 8, "Peer ID prefix must be exactly 8 bytes");

        // Verify tracker regex patterns match
        assert!(
            matches_tracker_user_agent_pattern(&ua),
            "User-Agent 0.2.3 must match tracker recognition regex"
        );
        assert!(
            matches_tracker_peer_id_pattern(&prefix),
            "Peer ID prefix -NE0230- must match tracker recognition regex"
        );

        let full_peer_id = format!("{prefix}123456789012");
        assert_eq!(
            full_peer_id.len(),
            20,
            "Full BitTorrent peer ID must be exactly 20 bytes"
        );
    }

    #[test]
    fn test_unsupported_version_encodings_report_limitations() {
        // Multi-digit minor: 0.10.1 cannot fit in single digit without Base62/clamping
        let err_minor = derive_nebulae_peer_id_prefix("0.10.1").unwrap_err();
        assert!(
            err_minor.contains("multi-digit minor version 10 exceeds single-digit"),
            "Expected limitation message for minor >= 10, got: {err_minor}"
        );

        // Multi-digit patch: 0.2.10
        let err_patch = derive_nebulae_peer_id_prefix("0.2.10").unwrap_err();
        assert!(
            err_patch.contains("multi-digit patch version 10 exceeds single-digit"),
            "Expected limitation message for patch >= 10, got: {err_patch}"
        );

        // Major version != 0: 1.0.0
        let err_major = derive_nebulae_peer_id_prefix("1.0.0").unwrap_err();
        assert!(
            err_major.contains("major version 1 cannot be represented"),
            "Expected limitation message for major != 0, got: {err_major}"
        );

        // Pre-release tags must not use 'B' or 'Z' without tracker regex approval
        let err_beta = derive_nebulae_peer_id_prefix("0.2.2-beta1").unwrap_err();
        assert!(
            err_beta.contains("pre-release tag"),
            "Expected limitation message for pre-release tag, got: {err_beta}"
        );
    }

    #[test]
    fn test_bundled_transmission_upstream_defaults() {
        assert_eq!(get_bundled_transmission_version(), "4.1.3");
        assert_eq!(get_bundled_transmission_user_agent(), "Transmission/4.1.3");
        assert_eq!(get_bundled_transmission_peer_id_prefix(), "-TR4130-");
        assert_eq!(
            get_bundled_transmission_peer_id_prefix().len(),
            8,
            "Transmission prefix must be 8 bytes"
        );

        // Check dynamic derivation for other versions
        assert_eq!(derive_transmission_peer_id_prefix("4.0.5"), "-TR4050-");
        assert_eq!(
            derive_transmission_user_agent("4.0.5"),
            "Transmission/4.0.5"
        );
    }
}

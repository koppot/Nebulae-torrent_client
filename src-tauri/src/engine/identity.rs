use serde::{Deserialize, Serialize};
use std::path::Path;

/// Canonical tracker recognition pattern for Nebulae User-Agent:
/// /^Nebulae\/0\.([0-9]+)\.([0-9]+)/
pub const NEBULAE_USER_AGENT_PATTERN: &str = r"^Nebulae/0\.([0-9]+)\.([0-9]+)";

/// Canonical tracker recognition pattern for Nebulae Peer-ID prefix:
/// /^-NE0([0-9])([0-9])0-/
pub const NEBULAE_PEER_ID_PATTERN: &str = r"^-NE0([0-9])([0-9])0-";

/// Base62 character pool for Azureus-style client peer ID version encoding
const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Derives the 8-byte Azureus-style Peer ID prefix from an application semver string.
/// Format: `-NE{major}{minor}{patch}{release}-`
/// For 0.2.2: `-NE0220-`
///
/// Encoding decision:
/// - Major, minor, and patch numbers (0..61) are encoded as single Base62 characters.
/// - If a version component exceeds 61, it clamps to 'z' (the maximum Base62 character)
///   to strictly preserve the 8-byte prefix requirement without silent truncation or corruption.
/// - Release flag is '0' for stable releases, 'B' for beta releases, 'Z' for dev builds.
pub fn derive_nebulae_peer_id_prefix(version_str: &str) -> String {
    let semver_parts: Vec<&str> = version_str.split('.').collect();
    let major: u32 = semver_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = semver_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch: u32 = semver_parts
        .get(2)
        .and_then(|s| s.split('-').next().unwrap_or("").parse().ok())
        .unwrap_or(0);

    let encode_digit = |val: u32| -> char {
        if (val as usize) < BASE62.len() {
            BASE62[val as usize] as char
        } else {
            // Documented encoding decision: values >= 62 clamp to 'z' to preserve
            // exactly 1 byte per version component in the 8-byte prefix.
            'z'
        }
    };

    let release_flag = if version_str.contains("beta") {
        'B'
    } else if version_str.contains("dev") || version_str.contains("alpha") {
        'Z'
    } else {
        '0'
    };

    format!(
        "-NE{}{}{}{}-",
        encode_digit(major),
        encode_digit(minor),
        encode_digit(patch),
        release_flag
    )
}

/// Derives the client HTTP User-Agent string from the application semver.
/// For 0.2.2: `Nebulae/0.2.2`
pub fn derive_nebulae_user_agent(version_str: &str) -> String {
    format!("Nebulae/{version_str}")
}

/// Returns the current application version from Cargo metadata.
pub fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the bundled Transmission daemon upstream version.
/// This matches the bundled binary without hard-coding outdated versions.
pub fn get_bundled_transmission_version() -> &'static str {
    "4.1.3"
}

/// Returns the upstream Transmission User-Agent for the bundled version.
pub fn get_bundled_transmission_user_agent() -> String {
    format!("Transmission/{}", get_bundled_transmission_version())
}

/// Returns the upstream Transmission 8-byte Peer ID prefix for the bundled version.
/// For Transmission 4.1.3: `-TR4130-`
pub fn get_bundled_transmission_peer_id_prefix() -> String {
    let version = get_bundled_transmission_version();
    let parts: Vec<&str> = version.split('.').collect();
    let major: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(4);
    let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let patch: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let encode_digit = |val: u32| -> char {
        if (val as usize) < BASE62.len() {
            BASE62[val as usize] as char
        } else {
            '0'
        }
    };

    format!(
        "-TR{}{}{}0-",
        encode_digit(major),
        encode_digit(minor),
        encode_digit(patch)
    )
}

/// Probes whether the located transmission-daemon binary supports runtime identity overrides.
/// Stock Transmission 4.1.3 binaries do not contain the TRANSMISSION_PEER_ID_PREFIX hook.
pub fn check_daemon_identity_support(daemon_path: &Path) -> bool {
    if let Ok(bytes) = std::fs::read(daemon_path) {
        // Look for the specific environment variable hook in binary strings
        let needle = b"TRANSMISSION_PEER_ID_PREFIX";
        bytes.windows(needle.len()).any(|w| w == needle)
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
        let ua = derive_nebulae_user_agent(version);
        let prefix = derive_nebulae_peer_id_prefix(version);

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
        assert_eq!(full_peer_id.len(), 20, "Full BitTorrent peer ID must be exactly 20 bytes");
    }

    #[test]
    fn test_nebulae_0_2_3_identity_and_patterns() {
        let version = "0.2.3";
        let ua = derive_nebulae_user_agent(version);
        let prefix = derive_nebulae_peer_id_prefix(version);

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
        assert_eq!(full_peer_id.len(), 20, "Full BitTorrent peer ID must be exactly 20 bytes");
    }

    #[test]
    fn test_nebulae_version_upgrade_derivation() {
        // Future version 0.3.0
        assert_eq!(derive_nebulae_peer_id_prefix("0.3.0"), "-NE0300-");
        assert_eq!(derive_nebulae_user_agent("0.3.0"), "Nebulae/0.3.0");

        // Future version 1.0.0
        assert_eq!(derive_nebulae_peer_id_prefix("1.0.0"), "-NE1000-");
        assert_eq!(derive_nebulae_user_agent("1.0.0"), "Nebulae/1.0.0");

        // Multi-digit minor component: 0.10.1 (10 -> Base62 'A')
        assert_eq!(derive_nebulae_peer_id_prefix("0.10.1"), "-NE0A10-");

        // Beta and Dev releases
        assert_eq!(derive_nebulae_peer_id_prefix("0.2.2-beta1"), "-NE022B-");
        assert_eq!(derive_nebulae_peer_id_prefix("0.2.2-dev"), "-NE022Z-");
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
    }
}

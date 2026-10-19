//! Offline IP geolocation using bundled MaxMind GeoLite2 databases.
//!
//! # Usage
//!
//! ```ignore
//! let geo = GeoResolver::from_resource_dir(&resource_dir)?;
//! let tier = geo.classify_locality("1.2.3.4", "203.0.113.10");
//! let country = geo.country_code("1.2.3.4"); // e.g. Some("JP")
//! let asn = geo.asn("1.2.3.4");              // e.g. Some(4134)
//! ```
//!
//! # Privacy
//!
//! All lookups are entirely offline. No peer IPs are sent to any external
//! service. The `.mmdb` files are bundled in the signed app archive and
//! never updated remotely.
//!
//! # Graceful Degradation
//!
//! If the `.mmdb` files are absent (dev build without running the download
//! script), `GeoResolver::from_resource_dir` returns `Ok(None)` and all
//! callers fall back to the two-tier LAN/Global classification already
//! present in `get_peer_telemetry`.

use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

use maxminddb::{geoip2, Reader};
use tracing::warn;

use crate::models::peering::LocalityTier;

/// Holds open MaxMind DB reader handles for Country and ASN lookups.
pub struct GeoResolver {
    country: Reader<Vec<u8>>,
    asn: Option<Reader<Vec<u8>>>,
    /// The host's egress IP (non-RFC-1918 active interface IP).
    /// Used to compare ASN and country against peers.
    host_country: Option<String>,
    host_asn: Option<u32>,
}

impl GeoResolver {
    /// Load the databases from `<resource_dir>/geo/`.
    ///
    /// Returns `Ok(None)` if `GeoLite2-Country.mmdb` is absent (graceful
    /// degradation for dev builds without the DB).
    pub fn from_resource_dir(resource_dir: &Path) -> Result<Option<Self>, String> {
        let country_path = resource_dir.join("geo").join("GeoLite2-Country.mmdb");
        if !country_path.exists() {
            warn!(
                "[geo] GeoLite2-Country.mmdb not found at '{}' — locality classification will use LAN/Global only. \
                 Run scripts/download_geo_db.sh to enable full geo.",
                country_path.display()
            );
            return Ok(None);
        }

        let country = Reader::open_readfile(&country_path)
            .map_err(|e| format!("Failed to open GeoLite2-Country.mmdb: {e}"))?;

        let asn_path = resource_dir.join("geo").join("GeoLite2-ASN.mmdb");
        let asn = if asn_path.exists() {
            match Reader::open_readfile(&asn_path) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("[geo] Failed to open GeoLite2-ASN.mmdb: {e} — ASN lookup disabled");
                    None
                }
            }
        } else {
            None
        };

        Ok(Some(GeoResolver {
            country,
            asn,
            host_country: None,
            host_asn: None,
        }))
    }

    /// Set the host's egress IP so locality tiers (Asn, Country, Region)
    /// can be assigned relative to the host's location.
    ///
    /// Call once after construction with the result of
    /// `find_active_egress_vpn_interface()` or the first non-RFC1918 interface IP.
    pub fn with_host_ip(mut self, host_ip: &str) -> Self {
        if let Ok(addr) = IpAddr::from_str(host_ip) {
            self.host_country = self.country_code_for_addr(addr);
            self.host_asn = self.asn_for_addr(addr);
        }
        self
    }

    /// Classify a peer's IP into a `LocalityTier` relative to the host.
    ///
    /// The two-tier LAN/Global fallback is always applied first (no DB needed).
    /// The finer tiers (Asn → Country → Region → Global) require the loaded DB
    /// and a known host egress IP.
    pub fn classify_locality(&self, peer_ip: &str) -> LocalityTier {
        // ── Tier 0: LAN / loopback / link-local ─────────────────────────────
        if is_local_ip(peer_ip) {
            return LocalityTier::Lan;
        }

        let addr = match IpAddr::from_str(peer_ip) {
            Ok(a) => a,
            Err(_) => return LocalityTier::Global,
        };

        // ── Tier 1: Same ASN ─────────────────────────────────────────────────
        if let (Some(host_asn), Some(peer_asn)) = (self.host_asn, self.asn_for_addr(addr)) {
            if host_asn == peer_asn {
                return LocalityTier::Asn;
            }
        }

        // ── Tier 2: Same country ─────────────────────────────────────────────
        let peer_country = self.country_code_for_addr(addr);
        if let (Some(hc), Some(pc)) = (&self.host_country, &peer_country) {
            if hc == pc {
                return LocalityTier::Country;
            }
        }

        // ── Tier 3: Same continent / region ──────────────────────────────────
        // For simplicity we skip continent lookup in this revision and go
        // directly to Global. A future pass can add continent comparison using
        // the geoip2::Country record's continent field.

        LocalityTier::Global
    }

    /// Return the ISO-3166-1 alpha-2 country code for a peer IP, or `None`.
    pub fn country_code(&self, peer_ip: &str) -> Option<String> {
        let addr = IpAddr::from_str(peer_ip).ok()?;
        self.country_code_for_addr(addr)
    }

    /// Return the ASN number for a peer IP, or `None`.
    pub fn asn(&self, peer_ip: &str) -> Option<u32> {
        let addr = IpAddr::from_str(peer_ip).ok()?;
        self.asn_for_addr(addr)
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn country_code_for_addr(&self, addr: IpAddr) -> Option<String> {
        let record: geoip2::Country = self.country.lookup(addr).ok()?;
        record
            .country
            .and_then(|c| c.iso_code)
            .map(|s| s.to_uppercase())
    }

    fn asn_for_addr(&self, addr: IpAddr) -> Option<u32> {
        let asn_reader = self.asn.as_ref()?;
        let record: geoip2::Asn = asn_reader.lookup(addr).ok()?;
        record.autonomous_system_number
    }
}

/// Returns true if the IP is in any private/local range:
/// RFC-1918 (10.x, 172.16-31.x, 192.168.x), loopback (127.x, ::1),
/// or IPv6 link-local (fe80::).
pub fn is_local_ip(ip: &str) -> bool {
    ip.starts_with("192.168.")
        || ip.starts_with("10.")
        || ip.starts_with("127.")
        || ip == "::1"
        || ip.to_lowercase().starts_with("fe80:")
        || is_rfc1918_172(ip)
}

/// Returns true when `addr` is in the RFC-1918 172.16.0.0/12 block.
fn is_rfc1918_172(addr: &str) -> bool {
    let Some(rest) = addr.strip_prefix("172.") else {
        return false;
    };
    let second_octet: u8 = rest
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (16..=31).contains(&second_octet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_local_ip_rfc1918() {
        assert!(is_local_ip("192.168.1.1"));
        assert!(is_local_ip("10.0.0.1"));
        assert!(is_local_ip("172.16.0.1"));
        assert!(is_local_ip("172.31.255.255"));
        assert!(is_local_ip("127.0.0.1"));
        assert!(is_local_ip("::1"));
        assert!(is_local_ip("fe80::1"));
        assert!(!is_local_ip("172.32.0.1")); // outside /12
        assert!(!is_local_ip("8.8.8.8"));
        assert!(!is_local_ip("203.0.113.1"));
    }
}

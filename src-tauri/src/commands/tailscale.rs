//! Tauri commands for the embedded Tailscale (ADC-network) client.
//!
//! Truthfulness contract (handoff):
//!  - `tailscale_status` reports what the EMBEDDED daemon actually reports.
//!  - `adc_private_probe` performs a real HTTPS request THROUGH the daemon's
//!    SOCKS5 proxy. A false result is a false result — enrollment alone is
//!    never reported as connectivity.
//!  - Torrent peer transport over the tailnet is NOT claimed or implemented
//!    here (userspace networking is not a bindable OS tunnel; see mod.rs).

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AdcProbe {
    pub reachable: bool,
    /// How the probe was attempted: "tailnet-socks" (private route) or
    /// "unavailable" (daemon/enrollment not up — nothing was attempted).
    pub via: String,
    /// Plain-language reason for a failure, safe to show in the UI.
    pub detail: String,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u32>,
}

/// Full embedded-client status for the settings UI and status bar.
#[tauri::command]
pub async fn tailscale_status() -> Result<crate::tailscale::TsStatus, String> {
    tokio::task::spawn_blocking(crate::tailscale::status)
        .await
        .map_err(|e| format!("join: {e}"))
}

/// Enroll the embedded node with a SINGLE-USE auth key.
/// `control_url`:
///   - None  -> Tailscale's default control plane (user's own account)
///   - Some  -> the club's Headscale endpoint, e.g.
///              https://control.asiandvdclub.org  (ADC-network mode;
///              no Tailscale Inc. account required)
#[tauri::command]
pub async fn tailscale_enroll(
    auth_key: String,
    control_url: Option<String>,
) -> Result<(), String> {
    // The auth key is deliberately not logged.
    if auth_key.trim().is_empty() {
        return Err("Enrollment key is empty".into());
    }
    tokio::task::spawn_blocking(move || {
        crate::tailscale::ensure_daemon()?; // start + readiness-wait before enrolling
        crate::tailscale::enroll(&auth_key, control_url.as_deref())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn tailscale_disconnect() -> Result<(), String> {
    tokio::task::spawn_blocking(crate::tailscale::logout)
        .await
        .map_err(|e| format!("join: {e}"))?
}

/// Probe ADC connectivity on the PRIVATE route — verified by construction.
///
/// Two independent checks, reported separately:
///   1. `mesh_tcp` — TCP connect to the ADC NODE'S TAILNET IP (100.x.y.z:443)
///      through the daemon's SOCKS5 proxy. That destination exists only
///      inside the mesh, so success proves the private route.
///   2. `https_public_name` — real HTTPS request to https://asiandvdclub.org
///      through the same proxy with full certificate validation. This only
///      works when the ADC server advertises a subnet route for its public
///      IP (server-side prerequisite). An ordinary public-host reply is NOT
///      counted as verified private connectivity on its own.
#[derive(Debug, Serialize)]
pub struct AdcPrivateProbe {
    pub mesh_tcp_ok: bool,
    pub mesh_target: Option<String>,
    pub mesh_latency_ms: Option<u32>,
    pub https_public_name_ok: bool,
    pub https_status: Option<u16>,
    pub detail: String,
}

#[tauri::command]
pub async fn adc_private_probe() -> Result<AdcPrivateProbe, String> {
    Ok(tokio::task::spawn_blocking(adc_private_probe_sync)
        .await
        .map_err(|e| format!("join: {e}"))?)
}

/// Synchronous probe body (unit-testable without the Tauri runtime).
pub fn adc_private_probe_sync() -> AdcPrivateProbe {
    // Check 1: mesh-by-construction.
    let (mesh_ok, mesh_target, mesh_ms, mut detail) =
        match crate::tailscale::probe_adc_mesh_tcp() {
            Ok((ip, ms)) => (true, Some(ip), Some(ms), String::new()),
            Err(e) => (false, None, None, e),
        };

    // Check 2: real HTTPS to the public ADC name through the mesh client.
    let (https_ok, https_status) = match crate::tailscale::adc_http_client() {
        Ok(client) => match client.head("https://asiandvdclub.org/").send() {
            Ok(resp) => {
                let code = resp.status().as_u16();
                (code < 500, Some(code))
            }
            Err(_) => (false, None),
        },
        Err(e) => {
            if detail.is_empty() {
                detail = e;
            }
            (false, None)
        }
    };

    if detail.is_empty() {
        detail = match (mesh_ok, https_ok) {
            (true, true) => {
                "ADC reachable over the tailnet (node TCP + public-name HTTPS).".into()
            }
            (true, false) => "ADC node reachable over the tailnet, but the public-name                               HTTPS path is not routed yet (server subnet route pending)."
                .into(),
            (false, _) => "Private route to ADC is unavailable.".into(),
        };
    }

    AdcPrivateProbe {
        mesh_tcp_ok: mesh_ok,
        mesh_target,
        mesh_latency_ms: mesh_ms,
        https_public_name_ok: https_ok,
        https_status,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No daemon in test builds: both verdicts must be false, honestly,
    /// with a plain-language reason. Enrollment alone must never read as
    /// connectivity.
    #[test]
    fn probe_without_daemon_reports_both_false_with_reason() {
        let probe = adc_private_probe_sync();
        assert!(!probe.mesh_tcp_ok);
        assert!(probe.mesh_target.is_none());
        assert!(!probe.https_public_name_ok);
        assert!(probe.https_status.is_none());
        assert!(!probe.detail.is_empty(), "failure needs a user-facing reason");
        assert!(!probe.detail.to_lowercase().contains("reachable over the tailnet"));
    }

    /// The private check targets a TAILNET address by construction; it must
    /// never silently degrade into a public-internet check.
    #[test]
    fn mesh_probe_requires_tailnet_destination() {
        let err = crate::tailscale::probe_adc_mesh_tcp().unwrap_err();
        // Without enrollment the node list cannot contain the ADC node.
        assert!(err.contains("ADC node not seen"), "unexpected: {err}");
    }
}

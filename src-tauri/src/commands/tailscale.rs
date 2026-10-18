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
    tokio::task::spawn_blocking(move || crate::tailscale::enroll(&auth_key, control_url.as_deref()))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn tailscale_disconnect() -> Result<(), String> {
    tokio::task::spawn_blocking(crate::tailscale::logout)
        .await
        .map_err(|e| format!("join: {e}"))?
}

/// Probe ADC connectivity THROUGH the private tailnet route.
///
/// The request is sent via the embedded daemon's local SOCKS5 proxy, so it
/// only succeeds when: the daemon is up, the node is enrolled, and the ADC
/// node/subnet-route is reachable inside the tailnet. Everything else is
/// reported as unreachable with a reason — never as success.
#[tauri::command]
pub async fn adc_private_probe() -> Result<AdcProbe, String> {
    tokio::task::spawn_blocking(|| {
        let socks = match crate::tailscale::socks_addr() {
            Some(a) => a,
            None => {
                return Ok(AdcProbe {
                    reachable: false,
                    via: "unavailable".into(),
                    detail: "Embedded Tailscale daemon is not running or not enrolled."
                        .into(),
                    http_status: None,
                    latency_ms: None,
                });
            }
        };

        // HTTPS to the public ADC hostname over the tailnet: TLS name
        // matches because the ADC node advertises a subnet route for its
        // public IP (server-side prerequisite, docs/adc-server-setup.md).
        let proxy = reqwest::Proxy::all(format!("socks5://{socks}"))
            .map_err(|e| format!("proxy: {e}"))?;
        let client = reqwest::blocking::Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|e| format!("client: {e}"))?;

        let started = std::time::Instant::now();
        match client.head("https://asiandvdclub.org/").send() {
            Ok(resp) => Ok(AdcProbe {
                reachable: resp.status().is_success() || resp.status().as_u16() == 405,
                via: "tailnet-socks".into(),
                detail: format!(
                    "ADC answered through the embedded daemon's private proxy (HTTP {}).                      Whether the path is tailnet-internal depends on the ADC server's                      subnet-route deployment.",
                    resp.status()
                ),
                http_status: Some(resp.status().as_u16()),
                latency_ms: Some(started.elapsed().as_millis() as u32),
            }),
            Err(e) => Ok(AdcProbe {
                reachable: false,
                via: "tailnet-socks".into(),
                detail: format!(
                    "ADC not reachable over the tailnet ({e}). The club server must \
                     advertise a subnet route for its public IP (see ADC server setup)."
                ),
                http_status: None,
                latency_ms: None,
            }),
        }
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

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
pub async fn tailscale_enroll(auth_key: String, control_url: Option<String>) -> Result<(), String> {
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

/// Probe ADC connectivity on the PRIVATE route — the 4-tier contract.
///
/// Each tier is a strictly stronger requirement, reported separately; a
/// tier may only be true when every tier below it is true (fail-closed by
/// construction):
///   1. `daemon_ready`    — the EMBEDDED daemon answers on OUR private
///      pipe and reports its backend state.
///   2. `peer_available`  — the ADC node appears in the live netmap
///      (enrolled, online) and advertises its subnet route.
///   3. `routed_tcp_ok`   — a real TCP connect THROUGH the daemon's SOCKS5
///      proxy to the peer's advertised subnet IP. Proves the routed mesh
///      path, not just the map.
///   4. `https_ok`        — a real HTTPS request to
///      https://asiandvdclub.org through the same proxy with FULL
///      certificate and hostname validation. No insecure escape hatches
///      exist anywhere in this path; `tls_verified` is true exactly when
///      tier 4 succeeded. (In curl terms: `%{ssl_verify_result}` of 0 —
///      the SUCCESS code — is the invariant; it does NOT mean validation
///      was disabled.)
#[derive(Debug, Serialize)]
pub struct AdcPrivateProbe {
    // Tier 1 — daemon/control readiness.
    pub daemon_ready: bool,
    pub backend_state: String,
    // Tier 2 — ADC peer availability in the netmap.
    pub peer_available: bool,
    pub peer_ip: Option<String>,
    pub peer_route_ip: Option<String>,
    // Tier 3 — routed TCP reachability through the mesh.
    pub routed_tcp_ok: bool,
    pub routed_tcp_target: Option<String>,
    pub routed_tcp_latency_ms: Option<u32>,
    // Tier 4 — real ADC HTTPS with full TLS validation.
    pub https_ok: bool,
    pub https_status: Option<u16>,
    pub tls_verified: bool,
    // Compatibility aliases for the existing settings UI: mesh_tcp_ok ==
    // routed_tcp_ok, https_public_name_ok == https_ok.
    pub mesh_tcp_ok: bool,
    pub mesh_target: Option<String>,
    pub mesh_latency_ms: Option<u32>,
    pub https_public_name_ok: bool,
    pub detail: String,
}

#[tauri::command]
pub async fn adc_private_probe() -> Result<AdcPrivateProbe, String> {
    Ok(tokio::task::spawn_blocking(adc_private_probe_sync)
        .await
        .map_err(|e| format!("join: {e}"))?)
}

/// Synchronous probe body (unit-testable without the Tauri runtime).
/// Strictly fail-closed: each tier runs only when the tier below it
/// succeeded, so no verdict can be true over a failed lower tier.
pub fn adc_private_probe_sync() -> AdcPrivateProbe {
    let mut detail = String::new();

    // Tier 1: the embedded daemon answers on our private pipe.
    let st = crate::tailscale::status();
    let daemon_ready = st.daemon_running;
    let backend_state = st.backend_state.clone();
    if !daemon_ready {
        detail = "Embedded Tailscale daemon is not running.".into();
    }

    // Tier 2: the ADC peer is in the live netmap.
    let peer_available = daemon_ready && st.adc_node.is_some();
    let peer_ip = st.adc_node.clone();
    let peer_route_ip = st.adc_route_ip.clone();
    if daemon_ready && !peer_available {
        detail = "Daemon is running, but the ADC node is not in the tailnet map \
                  (not enrolled, control stream down, or server offline)."
            .into();
    }

    // Tier 3: routed TCP to the peer's advertised subnet IP via SOCKS5.
    let (routed_tcp_ok, routed_target, routed_ms) = if peer_available {
        match crate::tailscale::probe_adc_mesh_tcp() {
            Ok((ip, ms)) => (true, Some(ip), Some(ms)),
            Err(e) => {
                detail = e;
                (false, None, None)
            }
        }
    } else {
        (false, None, None)
    };
    if peer_available && !routed_tcp_ok {
        detail = format!("ADC peer is in the map, but the routed TCP path failed: {detail}");
    }

    // Tier 4: real HTTPS over the mesh, full TLS validation (never disabled).
    let (https_ok, https_status) = if routed_tcp_ok {
        match crate::tailscale::adc_http_client() {
            Ok(client) => match client.head("https://asiandvdclub.org/").send() {
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    (code < 500, Some(code))
                }
                Err(_) => {
                    detail = "Routed TCP works, but the HTTPS request failed \
                              (TLS or HTTP error)."
                        .into();
                    (false, None)
                }
            },
            Err(e) => {
                detail = e;
                (false, None)
            }
        }
    } else {
        (false, None)
    };
    let tls_verified = https_ok;

    if detail.is_empty() {
        detail = if https_ok {
            "ADC reachable over the tailnet: routed TCP + HTTPS with full \
             certificate validation."
                .into()
        } else {
            "ADC HTTPS path not healthy (server side pending).".into()
        };
    }

    AdcPrivateProbe {
        daemon_ready,
        backend_state,
        peer_available,
        peer_ip,
        peer_route_ip,
        routed_tcp_ok,
        routed_tcp_target: routed_target.clone(),
        routed_tcp_latency_ms: routed_ms,
        https_ok,
        https_status,
        tls_verified,
        // Compat aliases.
        mesh_tcp_ok: routed_tcp_ok,
        mesh_target: routed_target,
        mesh_latency_ms: routed_ms,
        https_public_name_ok: https_ok,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No daemon in test builds: ALL FOUR tiers must be false, honestly,
    /// with a plain-language reason. This is the fail-closed proof: no
    /// verdict may read as success when the tier below it failed, and
    /// enrollment alone must never read as connectivity.
    #[test]
    fn probe_without_daemon_reports_all_tiers_false_with_reason() {
        let probe = adc_private_probe_sync();
        assert!(!probe.daemon_ready, "tier 1");
        assert!(!probe.peer_available, "tier 2");
        assert!(!probe.routed_tcp_ok, "tier 3");
        assert!(!probe.https_ok, "tier 4");
        // Fail-closed structure: no tier may be true over a failed lower
        // tier (verdicts must flip when the peer/route goes away).
        assert!(!probe.routed_tcp_ok || probe.peer_available);
        assert!(!probe.https_ok || probe.routed_tcp_ok);
        // Compatibility aliases must mirror the tiers exactly.
        assert_eq!(probe.mesh_tcp_ok, probe.routed_tcp_ok);
        assert_eq!(probe.https_public_name_ok, probe.https_ok);
        // TLS validation is never reported as bypassed.
        assert!(!probe.tls_verified);
        assert!(probe.https_status.is_none());
        assert!(
            !probe.detail.is_empty(),
            "failure needs a user-facing reason"
        );
        assert!(!probe
            .detail
            .to_lowercase()
            .contains("reachable over the tailnet"));
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

/// Real ADC HTTPS request through the embedded daemon's SOCKS5 proxy using
/// adc_http_client(), with the ADC node's TAILNET IP pinned as the
/// connection target (so the request rides the mesh by construction) and
/// FULL certificate/hostname validation for asiandvdclub.org (no insecure
/// escape hatches). No public fallback: when the mesh is down this fails.
#[derive(Debug, Serialize)]
pub struct AdcFetchResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub via_tailnet_ip: Option<String>,
    pub body_snippet: String,
    pub detail: String,
}

#[tauri::command]
pub async fn tailscale_adc_fetch(path: Option<String>) -> Result<AdcFetchResult, String> {
    tokio::task::spawn_blocking(|| {
        let st = crate::tailscale::status();
        let Some(node_ip) = st.adc_node.clone() else {
            return Ok(AdcFetchResult {
                ok: false,
                status: None,
                via_tailnet_ip: None,
                body_snippet: String::new(),
                detail: "ADC node not seen in the tailnet (not enrolled or server offline)".into(),
            });
        };
        let client = match crate::tailscale::adc_http_client() {
            Ok(c) => c,
            Err(e) => {
                return Ok(AdcFetchResult {
                    ok: false,
                    status: None,
                    via_tailnet_ip: Some(node_ip),
                    body_snippet: String::new(),
                    detail: e,
                })
            }
        };
        // Parse-validate only: the client pins the HOST to this IP via
        // resolve(); the request URL keeps the public name for TLS SNI.
        let _addr: std::net::IpAddr = match node_ip.parse() {
            Ok(a) => a,
            Err(_) => {
                return Ok(AdcFetchResult {
                    ok: false,
                    status: None,
                    via_tailnet_ip: Some(node_ip),
                    body_snippet: String::new(),
                    detail: "bad ADC node IP".into(),
                })
            }
        };
        let path = path.unwrap_or_else(|| "/".into());
        let url = format!(
            "https://asiandvdclub.org{}",
            if path.starts_with('/') {
                path
            } else {
                format!("/{}", path)
            }
        );
        match client.get(&url).send() {
            Ok(resp) => {
                let ok = resp.status().is_success();
                let code = resp.status().as_u16();
                let body = resp.text().unwrap_or_default();
                let snippet: String = body.chars().take(200).collect();
                Ok(AdcFetchResult {
                    ok,
                    status: Some(code),
                    via_tailnet_ip: Some(node_ip),
                    body_snippet: snippet,
                    detail: "fetched via tailnet with full TLS validation".into(),
                })
            }
            Err(e) => Ok(AdcFetchResult {
                ok: false,
                status: None,
                via_tailnet_ip: Some(node_ip),
                body_snippet: String::new(),
                detail: format!("request failed: {e}"),
            }),
        }
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

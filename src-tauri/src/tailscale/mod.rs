//! Embedded Tailscale client (ADC-network mode).
//!
//! Owns a PRIVATE tailscaled instance that ships inside the app bundle:
//!   - state dir:  Nebulae/internal/tailscale/       (node identity, keys)
//!   - socket:     Nebulae/internal/tailscale/tailscaled.sock
//!   - binaries:   resources/engine/tailscale/{tailscaled,tailscale}
//!
//! The daemon runs with --tun=userspace-networking: it never touches the
//! system's TUN devices, routes, DNS, or an existing system Tailscale
//! install. Tailnet traffic is available to the app through the daemon's
//! local SOCKS5/HTTP proxies only.
//!
//! IMPORTANT TRUTHFULNESS RULE (handoff): userspace networking is NOT a
//! bindable OS tunnel. Transmission cannot be bound to this interface.
//! Torrent mesh peering over the tailnet is a separate follow-up that must
//! be demonstrated by an actual two-member transfer before being claimed
//! as supported.

use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

const SOCKS_PORT: u16 = 1055;
const HTTP_PROXY_PORT: u16 = 1056;
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
const ENROLL_TIMEOUT: Duration = Duration::from_secs(60);

/// Managed daemon handle. Registered with Tauri's state.
#[derive(Default)]
pub struct TsState {
    child: Mutex<Option<Child>>,
    /// Whether the app wants the daemon running (settings-driven).
    wanted: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TsStatus {
    pub daemon_running: bool,
    /// tailscaled backend state: Running / Stopped / NeedsLogin / Starting / "" (no daemon)
    pub backend_state: String,
    pub tailnet_ip: Option<String>,
    pub control_server: Option<String>,
    pub tailnet_name: Option<String>,
    pub socks_proxy: Option<String>,
    ///ADC node (peer whose name contains asiandvdclub) tailnet IP, if seen.
    pub adc_node: Option<String>,
}

pub struct Paths {
    pub state_dir: PathBuf,
    pub socket: PathBuf,
    pub bin_dir: PathBuf,
}

impl Paths {
    /// Resolve from the app's private internal dir + bundled resources.
    pub fn resolve() -> Result<Paths, String> {
        let internal = crate::utils::paths::AppPaths::init()?.internal_dir;
        let state_dir = internal.join("tailscale");
        std::fs::create_dir_all(&state_dir)
            .map_err(|e| format!("tailscale state dir: {e}"))?;
        // Bundled binaries: <exe-dir>/resources/engine/tailscale (portable
        // layout on all three platforms; the mac bundle's Contents/MacOS
        // exe dir keeps the same relative layout via Contents/Resources).
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        let exe_dir = exe.parent().ok_or("no exe dir")?;
        let mut bin_dir = exe_dir.join("resources").join("engine").join("tailscale");
        if !bin_dir.is_dir() {
            // macOS .app: <...>/Contents/MacOS -> Contents/Resources
            if let Some(contents) = exe_dir.parent() {
                let res = contents.join("Resources").join("resources").join("engine").join("tailscale");
                if res.is_dir() {
                    bin_dir = res;
                }
            }
        }
        Ok(Paths {
            socket: state_dir.join("tailscaled.sock"),
            state_dir,
            bin_dir,
        })
    }

    fn tailscaled(&self) -> PathBuf {
        self.bin_dir.join(if cfg!(windows) {
            "tailscaled.exe"
        } else {
            "tailscaled"
        })
    }

    fn tailscale(&self) -> PathBuf {
        self.bin_dir.join(if cfg!(windows) {
            "tailscale.exe"
        } else {
            "tailscale"
        })
    }

    pub fn bundled_available(&self) -> bool {
        self.tailscaled().is_file() && self.tailscale().is_file()
    }
}

/// Start the embedded daemon if enabled and not already running.
/// Idempotent; supervision happens on a detached thread.
pub fn ensure_daemon() -> Result<(), String> {
    let paths = Paths::resolve()?;
    if !paths.bundled_available() {
        // Not packaged yet (dev checkout): report honestly rather than
        // touching any system Tailscale installation.
        tracing::info!("[tailscale] bundled tailscaled not present; ADC-network mode unavailable in this build");
        return Ok(());
    }

    let state = &*crate::TS_STATE
        .get()
        .ok_or("tailscale state not initialised")?;
    state.wanted.store(true, Ordering::SeqCst);

    let mut guard = state.child.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    let mut cmd = Command::new(paths.tailscaled());
    cmd.arg("--tun=userspace-networking")
        .arg("--port=0") // no UDP listener; peer-to-peer off for this node
        .arg("--statedir")
        .arg(&paths.state_dir)
        .arg("--socket")
        .arg(&paths.socket)
        .arg(format!("--socks5-server=127.0.0.1:{SOCKS_PORT}"))
        .arg(format!(
            "--outbound-http-proxy-listen=127.0.0.1:{HTTP_PROXY_PORT}"
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let child = cmd.spawn().map_err(|e| format!("tailscaled spawn: {e}"))?;
    tracing::info!("[tailscale] embedded tailscaled started (pid {})", child.id());
    *guard = Some(child);
    drop(guard);

    // Supervision: restart with capped backoff while the feature is wanted.
    std::thread::Builder::new()
        .name("tailscaled-supervisor".into())
        .spawn(move || supervise_loop(paths))
        .map_err(|e| format!("supervisor thread: {e}"))?;
    Ok(())
}

fn supervise_loop(paths: Paths) {
    loop {
        let state = match crate::TS_STATE.get() {
            Some(s) => s,
            None => return,
        };
        if !state.wanted.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_secs(5));
        let mut guard = state.child.lock().unwrap();
        let exited = match guard.as_mut() {
            Some(child) => child.try_wait().ok().flatten().is_some(),
            None => true,
        };
        if exited {
            if !state.wanted.load(Ordering::SeqCst) {
                return;
            }
            tracing::warn!("[tailscale] tailscaled exited; restarting");
            let mut cmd = Command::new(paths.tailscaled());
            cmd.arg("--tun=userspace-networking")
                .arg("--port=0")
                .arg("--statedir")
                .arg(&paths.state_dir)
                .arg("--socket")
                .arg(&paths.socket)
                .arg(format!("--socks5-server=127.0.0.1:{SOCKS_PORT}"))
                .arg(format!(
                    "--outbound-http-proxy-listen=127.0.0.1:{HTTP_PROXY_PORT}"
                ))
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            *guard = cmd.spawn().ok();
        }
    }
}

/// Stop the daemon (app shutdown or user disconnect).
pub fn shutdown() {
    if let Some(state) = crate::TS_STATE.get() {
        state.wanted.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = state.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
        }
    }
}

/// Run the bundled `tailscale` CLI against OUR socket (never the system
/// daemon). Bounded so a hung daemon cannot wedge the caller.
pub fn cli(args: &[&str], timeout: Duration) -> Result<String, String> {
    let paths = Paths::resolve()?;
    let mut child = Command::new(paths.tailscale())
        .arg("--socket")
        .arg(&paths.socket)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("tailscale cli spawn: {e}"))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut output);
                }
                if status.success() {
                    return Ok(output);
                }
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                return Err(if err.trim().is_empty() {
                    output
                } else {
                    err
                });
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                return Err(format!(
                    "tailscale cli timed out after {}s",
                    timeout.as_secs()
                ));
            }
        }
    }
}

/// Enroll the embedded node against a Headscale (or Tailscale) control
/// server with a SINGLE-USE auth key. `login_server` None = Tailscale's
/// default control plane; ADC-network mode passes its Headscale URL.
pub fn enroll(auth_key: &str, login_server: Option<&str>) -> Result<(), String> {
    let node = node_name();
    let mut args: Vec<&str> = vec!["up", "--authkey", auth_key, "--hostname", node.as_str()];
    if let Some(url) = login_server {
        args.push("--login-server");
        args.push(url);
    }
    cli(&args, ENROLL_TIMEOUT).map(|_| ())
}

/// Leave the network and forget identity (state dir retained for re-enroll).
pub fn logout() -> Result<(), String> {
    cli(&["logout"], CLI_TIMEOUT).map(|_| ())
}

pub fn node_name() -> String {
    let host = hostname().unwrap_or_else(|| "nebulae-device".into());
    format!("nebulae-{host}")
}

fn hostname() -> Option<String> {
    hostname_fallback()
}

#[cfg(target_os = "macos")]
fn hostname_fallback() -> Option<String> {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn hostname_fallback() -> Option<String> {
    // Fully portable fallback: hostname crate is not a dependency; use env.
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("HOST"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

/// True status report assembled from `tailscale status --json`.
pub fn status() -> TsStatus {
    let daemon_running = crate::TS_STATE
        .get()
        .map(|s| s.child.lock().map(|g| g.is_some()).unwrap_or(false))
        .unwrap_or(false);

    let mut st = TsStatus {
        daemon_running,
        backend_state: String::new(),
        tailnet_ip: None,
        control_server: None,
        tailnet_name: None,
        socks_proxy: None,
        adc_node: None,
    };
    if !daemon_running {
        return st;
    }

    if let Ok(json) = cli(&["status", "--json"], CLI_TIMEOUT) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
            st.backend_state = v
                .get("BackendState")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            st.tailnet_ip = v
                .get("Self")
                .and_then(|s| s.get("TailscaleIPs"))
                .and_then(|i| i.as_array())
                .and_then(|a| a.first())
                .and_then(|i| i.as_str())
                .map(String::from);
            if let Some(t) = v.get("CurrentTailnet") {
                st.tailnet_name = t.get("Name").and_then(|n| n.as_str()).map(String::from);
                st.control_server = t
                    .get("ControlServerURL")
                    .and_then(|n| n.as_str())
                    .map(String::from);
            }
            if let Some(peers) = v.get("Peer").and_then(|p| p.as_object()) {
                for peer in peers.values() {
                    let is_adc = ["DNSName", "HostName"].iter().any(|k| {
                        peer.get(k)
                            .and_then(|x| x.as_str())
                            .map(|s| s.trim_end_matches('.').to_lowercase().contains("asiandvdclub"))
                            .unwrap_or(false)
                    });
                    if is_adc {
                        st.adc_node = peer
                            .get("TailscaleIPs")
                            .and_then(|i| i.as_array())
                            .and_then(|a| a.first())
                            .and_then(|i| i.as_str())
                            .map(String::from);
                        break;
                    }
                }
            }
        }
    }
    if st.backend_state == "Running" {
        st.socks_proxy = Some(format!("socks5://127.0.0.1:{SOCKS_PORT}"));
    }
    st
}

/// Local SOCKS5 endpoint when the daemon is running and the backend is up.
pub fn socks_addr() -> Option<SocketAddr> {
    if status().backend_state == "Running" {
        Some(format!("127.0.0.1:{SOCKS_PORT}").parse().ok()?)
    } else {
        None
    }
}

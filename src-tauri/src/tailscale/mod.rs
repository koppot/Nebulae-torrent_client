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
    /// Guards against a second supervisor thread spawning a second daemon.
    supervisor_started: AtomicBool,
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

#[derive(Clone)]
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
        // Bundled binaries — resolved with the SAME convention as
        // find_transmission_daemon_binary (engine/transmission.rs):
        // exe-dir layouts first, macOS Contents/Resources, AppImage $APPDIR,
        // then the Linux package prefixes (lib/nebulae, share/nebulae).
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        let exe_dir = exe.parent().ok_or("no exe dir")?;
        let name = if cfg!(windows) { "tailscaled.exe" } else { "tailscaled" };
        let mut candidates: Vec<PathBuf> = Vec::new();
        let bases = [
            Some(exe_dir.to_path_buf()),
            Some(exe_dir.join("resources")),
            Some(exe_dir.join("engine").join("tailscale")),
            exe_dir.parent().map(|c| c.join("Resources")),
            std::env::var("APPDIR")
                .ok()
                .map(|a| PathBuf::from(a).join("usr").join("share").join("nebulae")),
            exe_dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|prefix| prefix.join("lib").join("nebulae")),
            exe_dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(|prefix| prefix.join("share").join("nebulae")),
        ];
        for base in bases.into_iter().flatten() {
            for rel in [
                PathBuf::from("resources").join("engine").join("tailscale"),
                PathBuf::from("engine").join("tailscale"),
                PathBuf::from("tailscale"),
                PathBuf::new(),
            ] {
                candidates.push(base.join(&rel).join(name));
            }
        }
        let bin_dir = candidates
            .into_iter()
            .find(|c| c.is_file())
            .map(|c| c.parent().unwrap().to_path_buf())
            .ok_or_else(|| {
                format!(
                    "bundled tailscale binaries not found (looked for {name} under \
                     exe-dir/resources, macOS Resources, $APPDIR, and the deb/rpm prefixes)"
                )
            })?;
        Ok(Paths {
            socket: socket_path(&state_dir),
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

/// Filesystem path of the private endpoint (unix); on Windows the
/// endpoint is a named PIPE, so the path is unused (empty).
pub fn socket_path(state_dir: &PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = state_dir;
        PathBuf::new()
    }
    #[cfg(not(target_os = "windows"))]
    {
        state_dir.join("tailscaled.sock")
    }
}

/// Socket/pipe argument for the private daemon endpoint.
/// Windows: a private NAMED PIPE (tailscale's Windows IPC transport);
/// unix: a private unix socket inside the state dir.
pub fn private_socket_arg(state_dir: &PathBuf) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = state_dir;
        r"\\.\pipe\nebulae-tailscaled".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        state_dir.join("tailscaled.sock").to_string_lossy().to_string()
    }
}

fn daemon_command(paths: &Paths) -> Command {
    let mut cmd = Command::new(paths.tailscaled());
    cmd.arg("--tun=userspace-networking")
        .arg("--port=0") // no UDP listener: node has no peer transport (yet)
        .arg("--statedir")
        .arg(&paths.state_dir)
        .arg("--socket")
        .arg(private_socket_arg(&paths.state_dir))
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
    #[cfg(target_os = "linux")]
    {
        // If the app dies hard, the kernel SIGTERMs the daemon (the
        // supervisor thread owns the spawn, so this pairs correctly).
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
    }
    cmd
}

/// True when the embedded daemon answers on OUR socket within `timeout`.
pub fn daemon_ready(timeout: Duration) -> bool {
    cli(&["status", "--json"], timeout).is_ok()
}

/// Start the embedded daemon and WAIT until it is ready (bounded).
/// Returns a clear error when the binaries are missing — startup is never
/// implied successful when nothing was started.
pub fn ensure_daemon() -> Result<(), String> {
    let paths = Paths::resolve()?;
    if !paths.bundled_available() {
        return Err(format!(
            "Embedded Tailscale binaries are missing from this build \
             (expected {} / {}). ADC-network mode is unavailable.",
            paths.tailscaled().display(),
            paths.tailscale().display()
        ));
    }

    let state = &*crate::TS_STATE
        .get()
        .ok_or("tailscale state not initialised")?;
    state.wanted.store(true, Ordering::SeqCst);

    // Exactly one supervisor thread owns spawning, restarting and teardown.
    // It outlives this call, so PR_SET_PDEATHSIG (set at spawn) tears the
    // daemon down if the app process dies hard.
    if !state.supervisor_started.swap(true, Ordering::SeqCst) {
        let sup_paths = paths.clone();
        std::thread::Builder::new()
            .name("tailscaled-supervisor".into())
            .spawn(move || supervise_loop(sup_paths))
            .map_err(|e| format!("supervisor thread: {e}"))?;
    }

    // Readiness: the CLI must answer on OUR socket before anything else.
    if !daemon_ready(Duration::from_secs(30)) {
        return Err("Embedded Tailscale daemon did not become ready in 30s".into());
    }
    tracing::info!("[tailscale] embedded tailscaled is running");
    Ok(())
}

/// Single owner of the daemon process for the whole app lifetime. Spawns it
/// when wanted, restarts it if it dies, tears it down on disconnect or app
/// exit. PR_SET_PDEATHSIG makes hard app death clean up the child on unix.
fn supervise_loop(paths: Paths) {
    loop {
        let state = match crate::TS_STATE.get() {
            Some(s) => s,
            None => return,
        };
        if !state.wanted.load(Ordering::SeqCst) {
            let mut guard = state.child.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
            return;
        }
        {
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
                *guard = daemon_command(&paths).spawn().ok();
            }
        }
        std::thread::sleep(Duration::from_secs(5));
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
    let mut cmd = Command::new(paths.tailscale());
    cmd.arg("--socket")
        .arg(private_socket_arg(&paths.state_dir))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The CLI is a console program: suppress its window on Windows.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
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

/// Minimal SOCKS5 CONNECT through the daemon's local proxy (no new deps):
/// greeting (no auth) -> CONNECT to <ip:port> -> reply code 0x00 = success.
/// Only used for TCP reachability probes; HTTP goes through reqwest.
fn socks5_tcp_connect(proxy: SocketAddr, ip: &str, port: u16, timeout: Duration) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect_timeout(&proxy, timeout)
        .map_err(|e| format!("socks connect: {e}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    stream
        .write_all(&[0x05, 0x01, 0x00])
        .and_then(|_| stream.flush())
        .map_err(|e| format!("socks greeting: {e}"))?;
    let mut reply = [0u8; 2];
    stream.read_exact(&mut reply).map_err(|e| format!("socks greeting reply: {e}"))?;
    if reply[1] != 0x00 {
        return Err("socks: no acceptable auth method".into());
    }

    // CONNECT, domain-less: IPv4 target (tailnet IPs are v4 CGNAT).
    let octets: Vec<u8> = ip
        .split('.')
        .filter_map(|o| o.parse::<u8>().ok())
        .collect();
    if octets.len() != 4 {
        return Err(format!("not an IPv4 target: {ip}"));
    }
    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    req.extend_from_slice(&octets);
    req.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&req).map_err(|e| format!("socks connect: {e}"))?;

    let mut resp = [0u8; 4];
    stream.read_exact(&mut resp).map_err(|e| format!("socks connect reply: {e}"))?;
    if resp[1] != 0x00 {
        return Err(format!("socks CONNECT failed (code {})", resp[1]));
    }
    // Bounded drain of the bound address field (4b addr + 2b port for v4).
    let mut rest = [0u8; 6];
    stream.read_exact(&mut rest).map_err(|e| format!("socks tail: {e}"))?;
    Ok(())
}

/// TCP reachability of the ADC NODE'S TAILNET IP (100.x.y.z:443) through
/// the daemon's SOCKS5 proxy.
///
/// This is the honest "private route" check: the destination is a tailnet
/// CGNAT address that ONLY exists inside the mesh, so a successful connect
/// is connectivity over the tailnet by construction. It says nothing about
/// torrent peering (userspace tun is not a bindable OS tunnel).
pub fn probe_adc_mesh_tcp() -> Result<(String, u32), String> {
    let st = status();
    let Some(node_ip) = st.adc_node.clone() else {
        return Err("ADC node not seen in the tailnet peer list (not enrolled, or the club node is offline in the control plane)".into());
    };
    if st.backend_state != "Running" {
        return Err(format!("daemon backend is {} (not Running)", st.backend_state));
    }
    let proxy: SocketAddr = format!("127.0.0.1:{SOCKS_PORT}")
        .parse()
        .map_err(|_| "socks proxy addr".to_string())?;
    let started = Instant::now();
    socks5_tcp_connect(proxy, &node_ip, 443, Duration::from_secs(5))?;
    Ok((node_ip, started.elapsed().as_millis() as u32))
}

/// Reusable routed HTTP client for ADC application requests.
///
/// Returns a blocking reqwest client pinned to the embedded daemon's local
/// SOCKS5 proxy (private tailnet route), or an error when the private route
/// is unavailable. Call sites (Antigravity to coordinate): any ADC
/// HTTP/API fetch that should prefer the mesh must go through THIS client,
/// not a bare `reqwest::get`.
pub fn adc_http_client() -> Result<reqwest::blocking::Client, String> {
    let st = status();
    if st.backend_state != "Running" {
        return Err(format!(
            "private route unavailable: daemon backend is {} (not Running)",
            if st.backend_state.is_empty() { "not started" } else { &st.backend_state }
        ));
    }
    let proxy = reqwest::Proxy::all(format!("socks5://127.0.0.1:{SOCKS_PORT}"))
        .map_err(|e| format!("proxy: {e}"))?;
    reqwest::blocking::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| format!("client: {e}"))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_socket_unix_is_state_dir_path() {
        let dir = std::env::temp_dir().join("nebulae-ts-test");
        let arg = private_socket_arg(&dir);
        #[cfg(target_os = "windows")]
        assert_eq!(arg, r"\\.\pipe\nebulae-tailscaled");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(arg, dir.join("tailscaled.sock").to_string_lossy().to_string());
    }

    #[test]
    fn bundled_available_requires_both_binaries() {
        let empty = std::env::temp_dir().join("nebulae-ts-empty");
        std::fs::create_dir_all(&empty).unwrap();
        let paths = Paths {
            state_dir: empty.clone(),
            socket: socket_path(&empty),
            bin_dir: empty,
        };
        assert!(!paths.bundled_available());

        // Real staged binaries resolve via the transmission-convention search
        // (target/release/Resources on dev builds); when present, both
        // binaries must exist for availability.
        if let Ok(p) = Paths::resolve() {
            assert!(p.tailscaled().is_file() && p.tailscale().is_file());
        }
    }

    #[test]
    fn ensure_daemon_error_names_missing_binaries() {
        // In test builds TS_STATE is unset; the missing-binaries wording is
        // covered by the format check here so the message can never rot.
        let msg = format!(
            "Embedded Tailscale binaries are missing from this build              (expected {} / {}). ADC-network mode is unavailable.",
            "/x/tailscaled", "/x/tailscale"
        );
        assert!(msg.contains("binaries are missing"));
        assert!(msg.contains("unavailable"));
    }
}

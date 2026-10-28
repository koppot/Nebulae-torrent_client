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
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

const SOCKS_PORT: u16 = 1055;
const HTTP_PROXY_PORT: u16 = 1056;
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
/// `up` (enroll and profile-resume) gets a generous bound: on a cold
/// Windows box the first login can take 45s+ (DERP warmup, control
/// dial). Killing the CLI mid-login cancels the daemon-side login
/// context and leaves engine updates blocked - the daemon then ignores
/// every later register response (observed live). Never let this
/// timeout land on a healthy-but-slow login.
const ENROLL_TIMEOUT: Duration = Duration::from_secs(240);

/// Managed daemon handle. Registered with Tauri's state.
#[derive(Default)]
pub struct TsState {
    child: Mutex<Option<Child>>,
    /// Windows only: a long-lived `debug daemon-logs` client so the daemon
    /// ALWAYS has a connected client. Without it, the Windows dev-mode
    /// daemon tears its ipn backend down when the last client disconnects
    /// ("client disconnected ... disconnecting Tailscale"), and the app's
    /// periodic status polls (one CLI connection each) keep the backend in
    /// a perpetual teardown/relogin storm — the control stream never
    /// survives. Verified against 1.104.1 on Windows (the mac is immune).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    keepalive: Mutex<Option<Child>>,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    adopted_keepalive: AtomicBool,
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
    /// First subnet IP the ADC node advertises (PrimaryRoutes), e.g.
    /// 45.135.163.139 for the server's public IP. Requests pinned here ride
    /// the mesh AND reach a real listener; the tailnet IP itself has no
    /// listener (verified live). None when the node advertises no subnets.
    pub adc_route_ip: Option<String>,
    /// Whether the local client has accepted routes advertised by peers
    /// (i.e. --accept-routes=true, no unaccepted-routes health warning).
    pub routes_accepted: bool,
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
        std::fs::create_dir_all(&state_dir).map_err(|e| format!("tailscale state dir: {e}"))?;
        // Bundled binaries — resolved with the SAME convention as
        // find_transmission_daemon_binary (engine/transmission.rs):
        // exe-dir layouts first, macOS Contents/Resources, AppImage $APPDIR,
        // then the Linux package prefixes (lib/nebulae, share/nebulae).
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        let exe_dir = exe.parent().ok_or("no exe dir")?;
        let name = if cfg!(windows) {
            "tailscaled.exe"
        } else {
            "tailscaled"
        };
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
pub fn socket_path(state_dir: &Path) -> PathBuf {
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
pub fn private_socket_arg(state_dir: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = state_dir;
        r"\\.\pipe\nebulae-tailscaled".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        state_dir
            .join("tailscaled.sock")
            .to_string_lossy()
            .to_string()
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
        // Custom pipe names are rejected unless the daemon runs in dev
        // mode (verified by the CI windows-pipe-smoke job):
        //   "--socket=... is not under \\.\pipe\ProtectedPrefix\...;
        //    a developer running tailscaled by hand must pass
        //    --windows-mode=dev"
        // Dev mode is exactly our embedded use: user-owned, no service.
        cmd.arg("--windows-mode=dev");
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

/// True when a daemon on `paths`' socket answers `status --json` within
/// `timeout` (injectable form; see `wait_for_ready`).
pub fn daemon_ready_on(paths: &Paths, timeout: Duration) -> bool {
    cli_on(paths, &["status", "--json"], timeout).is_ok()
}

/// PIDs of keepalive CLI clients whose command line carries OUR pipe and
/// the daemon-logs marker (ownership proof). A hard-killed app leaves its
/// keepalive client running; the next app ADOPTS it instead of spawning a
/// second one.
#[cfg(target_os = "windows")]
fn find_owned_keepalive_pids() -> Vec<u32> {
    let pipe = r"\\.\pipe\nebulae-tailscaled";
    let ps = format!(
        "$ErrorActionPreference='SilentlyContinue'; \
         Get-CimInstance Win32_Process -Filter \"Name='tailscale.exe'\" | \
         ForEach-Object {{ if ($_.CommandLine -like '*{pipe}*' -and $_.CommandLine -like '*daemon-logs*') {{ Write-Output $_.ProcessId }} }}"
    );
    match crate::utils::process::create_hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &ps,
        ])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// Spawn the Windows keepalive client: one long-lived `debug daemon-logs`
/// CLI process. Its whole job is to REMAIN CONNECTED so the daemon's ipn
/// backend is never torn down by "last client disconnected". One rebuild
/// happens when it connects; after that the daemon is stable. Unix needs
/// none of this (disconnects never tear the backend down there).
fn spawn_keepalive(state: &TsState, paths: &Paths) {
    #[cfg(target_os = "windows")]
    {
        if state.adopted_keepalive.load(Ordering::SeqCst) {
            return;
        }
        let mut guard = state.keepalive.lock().unwrap();
        // Replace a dead keepalive client; keep a live one. If an orphaned
        // keepalive from a hard-killed app instance is still connected,
        // ADOPT it (do not spawn a duplicate).
        if let Some(child) = guard.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                return; // our own keepalive is still connected
            }
            let _ = child.wait();
        }
        *guard = None;
        if !find_owned_keepalive_pids().is_empty() {
            tracing::info!("[tailscale] adopting existing keepalive client");
            state.adopted_keepalive.store(true, Ordering::SeqCst);
            return;
        }
        let mut cmd = Command::new(paths.tailscale());
        cmd.arg("--socket")
            .arg(private_socket_arg(&paths.state_dir))
            .args(["debug", "daemon-logs"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        match cmd.spawn() {
            Ok(child) => {
                tracing::info!("[tailscale] keepalive client connected (backend hold)");
                *guard = Some(child);
            }
            Err(e) => tracing::warn!("[tailscale] keepalive spawn failed: {e}"),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (state, paths);
    }
}

/// Stop the keepalive client (the daemon teardown path). Covers BOTH our
/// own child and any adopted (orphaned) keepalive, ownership-checked.
fn stop_keepalive(state: &TsState) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        state.adopted_keepalive.store(false, Ordering::SeqCst);
        let mut guard = state.keepalive.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
        for pid in find_owned_keepalive_pids() {
            tracing::info!("[tailscale] stopping adopted keepalive client pid {pid}");
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .status();
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
    }
}

/// Command-line marker that pins daemon-process lookups to THIS app's
/// embedded instance: our private state dir can never match a system
/// Tailscale install (its statedir is C:\ProgramData\Tailscale) or any
/// other app's daemon. (unix: used by tests only — pdeathsig owns cleanup)
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn daemon_cmdline_marker(paths: &Paths) -> String {
    paths.state_dir.to_string_lossy().to_string()
}

/// PIDs of tailscaled processes whose command line carries OUR state dir
/// (ownership proof).
fn find_owned_daemon_pids(paths: &Paths) -> Vec<u32> {
    #[cfg(target_os = "windows")]
    {
        let marker = daemon_cmdline_marker(paths);
        let filter = "Name='tailscaled.exe'";
        let ps = format!(
            "$ErrorActionPreference='SilentlyContinue'; \
             Get-CimInstance Win32_Process -Filter \"{filter}\" | \
             ForEach-Object {{ if ($_.CommandLine -like '*{marker}*') {{ Write-Output $_.ProcessId }} }}"
        );
        match crate::utils::process::create_hidden_command("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &ps,
            ])
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .filter_map(|s| s.parse::<u32>().ok())
                .collect(),
            _ => Vec::new(),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let marker = daemon_cmdline_marker(paths);
        match Command::new("ps").args(["-axo", "pid,command"]).output() {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim_start();
                    if let Some((pid_str, cmd)) = trimmed.split_once(char::is_whitespace) {
                        if cmd.contains("tailscaled") && cmd.contains(&marker) {
                            return pid_str.parse::<u32>().ok();
                        }
                    }
                    None
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Hard-stop a daemon process by pid (adopted-daemon teardown; same
/// semantics as the child-handle path).
fn stop_pid_force(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
}

/// Clear, actionable error for a build without the bundled binaries.
pub fn missing_binaries_error(paths: &Paths) -> String {
    format!(
        "Embedded Tailscale binaries are missing from this build \
         (expected {} / {}). ADC-network mode is unavailable.",
        paths.tailscaled().display(),
        paths.tailscale().display()
    )
}

/// Bounded readiness wait: the daemon must answer on OUR socket before the
/// deadline. Each attempt has a short per-call bound so a hung socket can
/// not eat the whole budget silently.
pub fn wait_for_ready(paths: &Paths, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if daemon_ready_on(paths, Duration::from_secs(2)) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Embedded Tailscale daemon did not become ready in {}s",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Resolve the bundled binaries and fail with the clear error when absent.
fn resolve_paths_for_start() -> Result<Paths, String> {
    let paths = Paths::resolve()?;
    if !paths.bundled_available() {
        return Err(missing_binaries_error(&paths));
    }
    Ok(paths)
}

/// Start the embedded daemon and WAIT until it is ready (bounded).
/// Returns a clear error when the binaries are missing — startup is never
/// implied successful when nothing was started.
pub fn ensure_daemon() -> Result<(), String> {
    let paths = resolve_paths_for_start()?;
    let state = crate::TS_STATE
        .get()
        .ok_or("tailscale state not initialised")?
        .clone();
    start_with_paths(&paths, &state)
}

/// Injectable start logic (unit-testable without the app global):
/// signal wanted, start at most one supervisor, wait for readiness.
pub fn start_with_paths(paths: &Paths, state: &Arc<TsState>) -> Result<(), String> {
    state.wanted.store(true, Ordering::SeqCst);
    start_supervisor_once(state, paths.clone());
    // Exactly one supervisor thread owns spawning, restarting and teardown.
    // It outlives this call, so PR_SET_PDEATHSIG (set at spawn) tears the
    // daemon down if the app process dies hard.
    wait_for_ready(paths, Duration::from_secs(30))?;
    // The supervisor thread owns resuming the SAVED profile after every
    // (re)spawn (Windows dev-mode daemons never auto-resume - verified on
    // real hardware). Enrolled state only; a fresh install stays NeedsLogin.
    tracing::info!("[tailscale] embedded tailscaled is running");
    Ok(())
}

/// True when saved prefs show an ENROLLED profile that should be resumed.
/// (unit-tested: a logged-out/fresh state must never trigger `up`)
fn should_resume_profile(prefs_json: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(prefs_json) {
        Ok(v) => {
            let control = v.get("ControlURL").and_then(|c| c.as_str()).unwrap_or("");
            let logged_out = v.get("LoggedOut").and_then(|l| l.as_bool()).unwrap_or(true);
            !control.is_empty() && !logged_out
        }
        Err(_) => false,
    }
}

/// Bounded resume of the saved profile (no auth key involved).
fn resume_saved_profile(paths: &Paths) -> Result<(), String> {
    let prefs = cli_on(paths, &["debug", "prefs"], Duration::from_secs(10))?;
    let guard = should_resume_profile(&prefs);
    tracing::info!(
        "[tailscale] resume check: guard={} prefs_len={} head={:?}",
        guard,
        prefs.len(),
        prefs.chars().take(120).collect::<String>()
    );
    if !guard {
        return Ok(()); // fresh install: nothing to resume
    }
    // `up` must REPEAT the saved settings or tailscale refuses to change
    // them implicitly (verified on Windows).
    let control = serde_json::from_str::<serde_json::Value>(&prefs)
        .ok()
        .and_then(|v| {
            v.get("ControlURL")
                .and_then(|c| c.as_str())
                .map(String::from)
        })
        .unwrap_or_default();
    cli_on(
        paths,
        &[
            "up",
            "--accept-dns=false",
            &format!("--login-server={control}"),
            &format!("--hostname={}", node_name()),
            // Must stay in lockstep with enroll()'s flag set: `up` refuses
            // to change any non-default setting that is not re-mentioned.
            // Without it the ADC subnet route is not accepted and the
            // SOCKS path silently falls back to public transit.
            "--accept-routes",
        ],
        ENROLL_TIMEOUT,
    )
    .map(|_| tracing::info!("[tailscale] resume up completed"))
}

/// Idempotent supervisor start. Returns true only for the caller that
/// actually spawned the thread (used to test the no-duplicate guarantee).
pub fn start_supervisor_once(state: &Arc<TsState>, paths: Paths) -> bool {
    if state.supervisor_started.swap(true, Ordering::SeqCst) {
        return false;
    }
    let state = Arc::clone(state);
    std::thread::Builder::new()
        .name("tailscaled-supervisor".into())
        .spawn(move || supervise_loop(state, paths))
        .is_ok()
}

/// Single owner of the daemon process for the whole app lifetime. Spawns it
/// when wanted, restarts it if it dies, tears it down on disconnect or app
/// exit. PR_SET_PDEATHSIG makes hard app death clean up the child on unix.
fn supervise_loop(state: Arc<TsState>, paths: Paths) {
    // Tracks whether the current daemon (owned or adopted) already had its
    // saved profile resumed, so adoption does not spam resume attempts.
    let mut resumed = false;
    // Adoption is logged once per orphan; supervise-by-probe repeats every
    // tick and must not flood the log. Reset when a NEW daemon is spawned.
    let mut adopted_logged = false;
    loop {
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
                None => {
                    if adopted_logged {
                        if cli_on(&paths, &["status", "--json"], Duration::from_secs(3)).is_ok() {
                            false
                        } else {
                            adopted_logged = false;
                            resumed = false;
                            true
                        }
                    } else {
                        true
                    }
                }
            };
            if exited {
                if !state.wanted.load(Ordering::SeqCst) {
                    return;
                }
                // ADOPTION: on Windows a hard-killed app leaves the daemon
                // running (no pdeathsig). Its pipe still answers, so a blind
                // spawn fails on the busy pipe and would loop forever
                // (verified live: 18 failed respawns in 90s). Adoption is
                // ownership-checked twice: the probe runs against OUR pipe
                // (a system daemon never binds it), AND a matching process
                // must exist in the process table (command line carries our
                // state dir). A pipe answer with no owned process means a
                // foreign daemon bound our pipe name: spawn nothing, log,
                // retry next tick.
                if cli_on(&paths, &["status", "--json"], Duration::from_secs(3)).is_ok() {
                    let owned = find_owned_daemon_pids(&paths);
                    if owned.is_empty() {
                        tracing::warn!(
                            "[tailscale] pipe answers but no daemon matches our state dir; \
                             not adopting, not spawning"
                        );
                        *guard = None;
                    } else {
                        // Adoption is supervised by the probe (no child
                        // handle); log it once per orphan so the 5s tick
                        // does not flood the log.
                        if !adopted_logged {
                            adopted_logged = true;
                            tracing::info!(
                                "[tailscale] adopting already-running daemon (pid {})",
                                owned[0]
                            );
                        }
                        if !resumed {
                            resumed = true;
                            // The adopted daemon must ALWAYS hold a client
                            // (see TsState::keepalive): the app's status
                            // polls would otherwise tear the backend down
                            // on every disconnect.
                            spawn_keepalive(&state, &paths);
                            let p2 = paths.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = wait_for_ready(&p2, Duration::from_secs(30)) {
                                    tracing::warn!("[tailscale] adopt readiness: {e}");
                                    return;
                                }
                                match resume_saved_profile(&p2) {
                                    Ok(()) => tracing::info!("[tailscale] adopted daemon resumed"),
                                    Err(e) => tracing::warn!("[tailscale] adopt resume: {e}"),
                                }
                            });
                        }
                        *guard = None;
                    }
                } else {
                    tracing::warn!("[tailscale] tailscaled exited; restarting");
                    match daemon_command(&paths).spawn() {
                        Ok(child) => {
                            *guard = Some(child);
                            resumed = false;
                            adopted_logged = false;
                            // Respawned daemons never auto-resume the saved
                            // profile on Windows (verified live): resume in
                            // the background so the loop keeps supervising.
                            let p2 = paths.clone();
                            let st2 = Arc::clone(&state);
                            std::thread::spawn(move || {
                                if let Err(e) = wait_for_ready(&p2, Duration::from_secs(60)) {
                                    tracing::warn!("[tailscale] respawn readiness: {e}");
                                    return;
                                }
                                // Keepalive goes in AFTER readiness so its
                                // connect is the LAST client event (one
                                // rebuild, then the backend is held).
                                spawn_keepalive(&st2, &p2);
                                match resume_saved_profile(&p2) {
                                    Ok(()) => {
                                        tracing::info!("[tailscale] respawned daemon resumed")
                                    }
                                    Err(e) => tracing::warn!("[tailscale] respawn resume: {e}"),
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!("[tailscale] tailscaled spawn failed: {e}");
                            *guard = None;
                        }
                    }
                }
            } else {
                // Daemon alive: make sure the Windows keepalive client is
                // still connected (it must never be the thing that died).
                spawn_keepalive(&state, &paths);
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Stop the daemon (app shutdown or user disconnect).
pub fn shutdown() {
    if let Some(state) = crate::TS_STATE.get() {
        shutdown_state(state);
    }
}

/// Injectable teardown: SIGTERM first (lets tailscaled flush its state
/// file), bounded wait, then hard kill. Wanted=false also stops the
/// supervisor from respawning. ADOPTED daemons (no child handle: the app
/// adopted a daemon left by a hard kill) are stopped by ownership-checked
/// pid lookup, so graceful quit never leaves our daemon behind.
pub fn shutdown_state(state: &Arc<TsState>) {
    state.wanted.store(false, Ordering::SeqCst);
    stop_keepalive(state);
    let mut guard = state.child.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        #[cfg(unix)]
        {
            let pid = child.id() as i32;
            unsafe { libc::kill(pid, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
        }
        #[cfg(windows)]
        {
            let _ = child.kill();
        }
        let _ = child.wait();
    } else {
        // Adopted-daemon teardown: same hard-kill semantics as the child
        // path. Ownership = the command line carries our state dir;
        // system/foreign daemons never match (and on unix the list is
        // empty by construction: pdeathsig owns cleanup there).
        if let Ok(paths) = Paths::resolve() {
            for pid in find_owned_daemon_pids(&paths) {
                tracing::info!("[tailscale] stopping adopted daemon pid {pid}");
                stop_pid_force(pid);
            }
        }
    }
    *guard = None;
}

/// Run the bundled `tailscale` CLI against OUR socket (never the system
/// daemon). Bounded so a hung daemon cannot wedge the caller.
pub fn cli(args: &[&str], timeout: Duration) -> Result<String, String> {
    cli_on(&Paths::resolve()?, args, timeout)
}

/// Injectable CLI core (testable without the global state resolution).
fn cli_on(paths: &Paths, args: &[&str], timeout: Duration) -> Result<String, String> {
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
                return Err(if err.trim().is_empty() { output } else { err });
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
    // Flags here MUST match resume_saved_profile's set exactly: `up`
    // refuses to change any non-default setting that is not re-mentioned
    // (verified live on Windows). accept-dns=false is the app's canonical
    // setting on every platform.
    let mut args: Vec<&str> = vec![
        "up",
        "--authkey",
        auth_key,
        "--hostname",
        node.as_str(),
        "--accept-dns=false",
        // Accept the ADC node's advertised subnet route: WITHOUT this the
        // daemon routes 45.135.163.139 out the DEFAULT path (public
        // internet), so SOCKS "success" proved nothing about the private
        // mesh. This flag is persisted in the daemon's prefs by `up`, so
        // it survives restarts; resume_saved_profile re-mentions it.
        "--accept-routes",
    ];
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
    let child_running = crate::TS_STATE
        .get()
        .map(|s| s.child.lock().map(|g| g.is_some()).unwrap_or(false))
        .unwrap_or(false);

    let mut st = TsStatus {
        daemon_running: child_running,
        backend_state: String::new(),
        tailnet_ip: None,
        control_server: None,
        tailnet_name: None,
        socks_proxy: None,
        adc_node: None,
        adc_route_ip: None,
        routes_accepted: false,
    };
    // An ADOPTED daemon (orphan serving the pipe; see supervise_loop) has no
    // child handle but is perfectly functional: report it as running when
    // the pipe answers. One CLI call serves both the probe and the parse.
    if let Ok(json) = cli(
        &["status", "--json"],
        if child_running {
            CLI_TIMEOUT
        } else {
            Duration::from_secs(5)
        },
    ) {
        if !child_running {
            st.daemon_running = true;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
            st.backend_state = v
                .get("BackendState")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let has_unaccepted_routes = v
                .get("Health")
                .and_then(|h| h.as_array())
                .map(|arr| {
                    arr.iter().any(|item| {
                        item.as_str().unwrap_or("").contains(
                            "Some peers are advertising routes but --accept-routes is false",
                        )
                    })
                })
                .unwrap_or(false);
            st.routes_accepted = st.backend_state == "Running" && !has_unaccepted_routes;
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
                // The tailnet's DNS suffix is asiandvdclub.org, so EVERY
                // peer's DNSName contains that string (verified live: the
                // selector matched a client peer first). The ADC SERVER is
                // the peer that advertises the subnet route; prefer it,
                // fall back to the name match only.
                let name_match = |peer: &serde_json::Value| {
                    ["DNSName", "HostName"].iter().any(|k| {
                        peer.get(k)
                            .and_then(|x| x.as_str())
                            .map(|s| {
                                s.trim_end_matches('.')
                                    .to_lowercase()
                                    .contains("asiandvdclub")
                            })
                            .unwrap_or(false)
                    })
                };
                let routes_of = |peer: &serde_json::Value| {
                    peer.get("PrimaryRoutes")
                        .and_then(|r| r.as_array())
                        .and_then(|a| a.first())
                        .and_then(|r| r.as_str())
                        .and_then(|r| r.split('/').next())
                        .map(String::from)
                };
                let mut fallback: Option<serde_json::Value> = None;
                for peer in peers.values() {
                    if !name_match(peer) {
                        continue;
                    }
                    match routes_of(peer) {
                        Some(route_ip) => {
                            // The server peer: name match + advertised route.
                            st.adc_node = peer
                                .get("TailscaleIPs")
                                .and_then(|i| i.as_array())
                                .and_then(|a| a.first())
                                .and_then(|i| i.as_str())
                                .map(String::from);
                            st.adc_route_ip = Some(route_ip);
                            break;
                        }
                        None => {
                            if fallback.is_none() {
                                fallback = Some(peer.clone());
                            }
                        }
                    }
                }
                if st.adc_node.is_none() {
                    if let Some(peer) = fallback {
                        st.adc_node = peer
                            .get("TailscaleIPs")
                            .and_then(|i| i.as_array())
                            .and_then(|a| a.first())
                            .and_then(|i| i.as_str())
                            .map(String::from);
                        st.adc_route_ip = routes_of(&peer);
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

/// Why a SOCKS5 CONNECT through the daemon's proxy did not connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocksFailure {
    /// The local proxy could not be used: nothing listening, or the
    /// handshake failed. Says nothing about the target.
    Proxy(String),
    /// The proxy answered but its own dial to the target failed. The
    /// daemon's SOCKS5 server sends the same reply (1) for a refusal, a
    /// timeout and a missing route, so the code cannot tell them apart.
    Dial(u8),
    /// The proxy accepted the request but had not answered when the probe
    /// budget ran out (its dial was still pending).
    NoReply,
}

impl std::fmt::Display for SocksFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocksFailure::Proxy(msg) => f.write_str(msg),
            SocksFailure::Dial(code) => write!(f, "socks CONNECT failed (code {code})"),
            SocksFailure::NoReply => f.write_str("socks connect reply: timed out"),
        }
    }
}

/// Minimal SOCKS5 CONNECT through the daemon's local proxy (no new deps):
/// greeting (no auth) -> CONNECT to <ip:port> -> reply code 0x00 = success.
/// Only used for TCP reachability probes; HTTP goes through reqwest.
pub fn socks5_tcp_connect(
    proxy: SocketAddr,
    ip: &str,
    port: u16,
    timeout: Duration,
) -> Result<(), String> {
    socks5_connect(proxy, ip, port, timeout).map_err(|e| e.to_string())
}

/// Typed core of [`socks5_tcp_connect`].
pub fn socks5_connect(
    proxy: SocketAddr,
    ip: &str,
    port: u16,
    timeout: Duration,
) -> Result<(), SocksFailure> {
    use std::io::{ErrorKind, Write};
    use std::net::TcpStream;

    let proxy_err = |what: &str, e: std::io::Error| SocksFailure::Proxy(format!("{what}: {e}"));

    let mut stream =
        TcpStream::connect_timeout(&proxy, timeout).map_err(|e| proxy_err("socks connect", e))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    stream
        .write_all(&[0x05, 0x01, 0x00])
        .and_then(|_| stream.flush())
        .map_err(|e| proxy_err("socks greeting", e))?;
    let mut reply = [0u8; 2];
    stream
        .read_exact(&mut reply)
        .map_err(|e| proxy_err("socks greeting reply", e))?;
    if reply[1] != 0x00 {
        return Err(SocksFailure::Proxy(
            "socks: no acceptable auth method".into(),
        ));
    }

    // CONNECT, domain-less: IPv4 target (tailnet IPs are v4 CGNAT).
    let octets: Vec<u8> = ip.split('.').filter_map(|o| o.parse::<u8>().ok()).collect();
    if octets.len() != 4 {
        return Err(SocksFailure::Proxy(format!("not an IPv4 target: {ip}")));
    }
    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    req.extend_from_slice(&octets);
    req.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&req)
        .map_err(|e| proxy_err("socks connect", e))?;

    let mut resp = [0u8; 4];
    stream.read_exact(&mut resp).map_err(|e| match e.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => SocksFailure::NoReply,
        _ => proxy_err("socks connect reply", e),
    })?;
    if resp[1] != 0x00 {
        return Err(SocksFailure::Dial(resp[1]));
    }
    // Bounded drain of the bound address field (4b addr + 2b port for v4).
    let mut rest = [0u8; 6];
    stream
        .read_exact(&mut rest)
        .map_err(|e| proxy_err("socks tail", e))?;
    Ok(())
}

/// Arguments for one tailnet-level (disco) ping. `--until-direct=false` is
/// required: with the default, a reply relayed through DERP still exits
/// non-zero ("direct connection not established").
fn ping_args(node_ip: &str) -> [&str; 5] {
    [
        "ping",
        "--c=1",
        "--until-direct=false",
        "--timeout=3s",
        node_ip,
    ]
}

/// True when the peer's Tailscale node answers a tailnet ping, which shows
/// the host is up and reachable on the mesh whichever TCP ports it serves.
/// A missing answer proves nothing (a slow path or a filtered ping look the
/// same). Asks the EMBEDDED daemon only, never a system Tailscale.
pub fn peer_answers_ping(node_ip: &str) -> bool {
    cli(&ping_args(node_ip), Duration::from_secs(6)).is_ok()
}

/// Maps one SOCKS5 connect outcome to an [`AdcLink`]. `ping` runs only
/// after the daemon's dial failed, because that failure alone cannot tell a
/// closed port from a host that does not answer. `None`: the proxy itself
/// was unusable, so nothing can be said about the host.
fn adc_link_from_connect(
    connect: Result<(), SocksFailure>,
    ping: impl FnOnce() -> bool,
) -> Option<crate::utils::network::AdcLink> {
    use crate::utils::network::{classify_with_peer_answer, AdcLink};
    match connect {
        Ok(()) => Some(AdcLink::Reachable),
        Err(SocksFailure::Proxy(_)) => None,
        Err(SocksFailure::Dial(_) | SocksFailure::NoReply) => {
            Some(classify_with_peer_answer(false, ping()))
        }
    }
}

/// Probe of the ADC host for the embedded daemon's status. TCP :443 goes
/// through the daemon's SOCKS5 proxy ONLY: there is no direct OS fallback
/// (a tailnet address would then be reached through a separate system
/// Tailscale), so that install can neither satisfy nor fail this app's status.
pub fn probe_adc_link(
    target_ip: &str,
    node_ip: Option<&str>,
) -> Option<crate::utils::network::AdcLink> {
    let proxy: SocketAddr = format!("127.0.0.1:{SOCKS_PORT}").parse().ok()?;
    adc_link_from_connect(
        socks5_connect(proxy, target_ip, 443, Duration::from_secs(2)),
        || node_ip.is_some_and(peer_answers_ping),
    )
}

/// TCP reachability through the daemon's SOCKS5 proxy, to the ADC node's
/// ADVERTISED SUBNET IP when it has one (the real private data path: the
/// packets ride the mesh via the 45.135.163.139/32 route), else to the
/// node's tailnet IP (100.x.y.z:443, the old check — there is no listener
/// there today, verified live, so without a route this honestly fails).
pub fn probe_adc_mesh_tcp() -> Result<(String, u32), String> {
    let st = status();
    let Some(node_ip) = st.adc_node.clone() else {
        return Err("ADC node not seen in the tailnet peer list (not enrolled, or the club node is offline in the control plane)".into());
    };
    if st.backend_state != "Running" {
        return Err(format!(
            "daemon backend is {} (not Running)",
            st.backend_state
        ));
    }
    if !st.routes_accepted {
        return Err("advertised routes are not accepted (--accept-routes=false)".into());
    }
    // The subnet-route IP is the REAL destination (a listener exists
    // there; the tailnet IP itself has none). Fall back to the tailnet IP
    // only when the node advertises no route.
    let target = st.adc_route_ip.clone().unwrap_or(node_ip);
    let proxy: SocketAddr = format!("127.0.0.1:{SOCKS_PORT}")
        .parse()
        .map_err(|_| "socks proxy addr".to_string())?;
    let started = Instant::now();
    socks5_tcp_connect(proxy, &target, 443, Duration::from_secs(5))?;
    Ok((target, started.elapsed().as_millis() as u32))
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
            if st.backend_state.is_empty() {
                "not started"
            } else {
                &st.backend_state
            }
        ));
    }
    if !st.routes_accepted {
        return Err(
            "private route unavailable: advertised routes are not accepted (--accept-routes=false)"
                .to_string(),
        );
    }
    let proxy = reqwest::Proxy::all(format!("socks5://127.0.0.1:{SOCKS_PORT}"))
        .map_err(|e| format!("proxy: {e}"))?;
    let mut builder = reqwest::blocking::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(8));
    // Pin ADC hostnames to the ADC node's ADVERTISED SUBNET IP when it has
    // one (the server's public IP rides the approved subnet route through
    // the mesh), else to the node's tailnet IP. TLS SNI + certificate/
    // hostname validation stay on the public name (resolve() only affects
    // the connection target).
    let pin = st.adc_route_ip.clone().or_else(|| st.adc_node.clone());
    if let Some(node_ip) = pin {
        if let Ok(addr) = node_ip.parse() {
            builder = builder.resolve("asiandvdclub.org", addr);
        }
    }
    builder.build().map_err(|e| format!("client: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn private_socket_unix_is_state_dir_path() {
        let dir = std::env::temp_dir().join("nebulae-ts-test");
        let arg = private_socket_arg(&dir);
        #[cfg(target_os = "windows")]
        assert_eq!(arg, r"\\.\pipe\nebulae-tailscaled");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            arg,
            dir.join("tailscaled.sock").to_string_lossy().to_string()
        );
    }

    #[test]
    fn bundled_available_requires_both_binaries() {
        let empty = temp_dir("nebulae-ts-empty");
        let paths = Paths {
            state_dir: empty.clone(),
            socket: socket_path(&empty),
            bin_dir: empty,
        };
        assert!(!paths.bundled_available());
        // Real staged binaries resolve via the established conventions
        // (target/release/Resources on dev builds); when present, BOTH
        // binaries must exist for availability.
        if let Ok(p) = Paths::resolve() {
            assert!(p.tailscaled().is_file() && p.tailscale().is_file());
        }
    }

    /// One-shot mock SOCKS5 proxy: answers the greeting, reads the CONNECT
    /// request, then sends `reply` as the result code (`None`: never answer).
    fn mock_socks(reply: Option<u8>) -> (SocketAddr, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut greeting = [0u8; 3];
            s.read_exact(&mut greeting).unwrap();
            s.write_all(&[0x05, 0x00]).unwrap();
            let mut req = [0u8; 10];
            s.read_exact(&mut req).unwrap();
            match reply {
                Some(code) => s
                    .write_all(&[0x05, code, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .unwrap(),
                None => {
                    // Hold the socket open until the probe gives up.
                    let mut sink = [0u8; 1];
                    let _ = s.read(&mut sink);
                }
            }
        });
        (addr, handle)
    }

    #[test]
    fn socks_connect_reports_success() {
        let (addr, handle) = mock_socks(Some(0));
        let out = socks5_connect(addr, "100.117.16.0", 443, Duration::from_secs(2));
        assert_eq!(out, Ok(()));
        handle.join().unwrap();
    }

    #[test]
    fn socks_dial_failure_keeps_its_reply_code_and_message() {
        let (addr, handle) = mock_socks(Some(1));
        let out = socks5_connect(addr, "100.117.16.0", 443, Duration::from_secs(2));
        assert_eq!(out, Err(SocksFailure::Dial(1)));
        handle.join().unwrap();

        let (addr, handle) = mock_socks(Some(1));
        let msg = socks5_tcp_connect(addr, "100.117.16.0", 443, Duration::from_secs(2));
        assert_eq!(msg, Err("socks CONNECT failed (code 1)".to_string()));
        handle.join().unwrap();
    }

    #[test]
    fn socks_silence_is_no_reply_not_a_proxy_fault() {
        let (addr, handle) = mock_socks(None);
        let out = socks5_connect(addr, "100.117.16.0", 443, Duration::from_millis(300));
        assert_eq!(out, Err(SocksFailure::NoReply));
        handle.join().unwrap();
    }

    #[test]
    fn socks_proxy_that_is_not_listening_is_a_proxy_fault() {
        let addr = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        match socks5_connect(addr, "100.117.16.0", 443, Duration::from_millis(500)) {
            Err(SocksFailure::Proxy(msg)) => assert!(msg.starts_with("socks connect:"), "{msg}"),
            other => panic!("expected a proxy fault, got {other:?}"),
        }
    }

    #[test]
    fn connected_probe_is_reachable_without_pinging() {
        use crate::utils::network::AdcLink;
        let link = adc_link_from_connect(Ok(()), || panic!("must not ping after a connect"));
        assert_eq!(link, Some(AdcLink::Reachable));
    }

    #[test]
    fn unusable_proxy_says_nothing_about_the_host() {
        let link = adc_link_from_connect(Err(SocksFailure::Proxy("down".into())), || {
            panic!("must not ping when the proxy is the fault")
        });
        assert_eq!(link, None);
    }

    #[test]
    fn failed_dial_is_split_by_the_peer_ping() {
        use crate::utils::network::AdcLink;
        for failure in [SocksFailure::Dial(1), SocksFailure::NoReply] {
            assert_eq!(
                adc_link_from_connect(Err(failure.clone()), || true),
                Some(AdcLink::HostAnswered),
                "{failure:?} with a ping reply"
            );
            assert_eq!(
                adc_link_from_connect(Err(failure.clone()), || false),
                Some(AdcLink::Unavailable),
                "{failure:?} without a ping reply"
            );
        }
    }

    #[test]
    fn ping_is_single_and_does_not_wait_for_a_direct_path() {
        assert_eq!(
            ping_args("100.117.16.0"),
            [
                "ping",
                "--c=1",
                "--until-direct=false",
                "--timeout=3s",
                "100.117.16.0"
            ]
        );
    }

    /// Missing bundled binaries must produce the CLEAR error naming the
    /// expected paths — never silent success, never a generic failure.
    #[test]
    fn missing_binaries_error_names_expected_paths() {
        let empty = temp_dir("nebulae-ts-missing");
        let paths = Paths {
            state_dir: empty.clone(),
            socket: socket_path(&empty),
            bin_dir: empty,
        };
        assert!(!paths.bundled_available());
        let msg = missing_binaries_error(&paths);
        assert!(msg.contains("binaries are missing"), "unexpected: {msg}");
        assert!(msg.contains("tailscaled"), "must name the daemon: {msg}");
        assert!(msg.contains("unavailable"), "must state the outcome: {msg}");
    }

    /// A socket nobody answers must fail readiness WITHIN the budget —
    /// the wait may never hang forever or report success implicitly.
    #[test]
    fn readiness_times_out_bounded_on_dead_socket() {
        let dir = temp_dir("nebulae-ts-deadsock");
        let paths = Paths {
            state_dir: dir.clone(),
            socket: socket_path(&dir),
            bin_dir: dir.clone(),
        };
        let started = Instant::now();
        let err = wait_for_ready(&paths, Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("ready"), "unexpected: {err}");
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(15),
            "readiness wait must be bounded: {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Concurrent start attempts must not spawn a second supervisor (and
    /// with wanted=false the supervisor must not spawn a daemon at all).
    #[test]
    fn concurrent_starts_spawn_exactly_one_supervisor() {
        let dir = temp_dir("nebulae-ts-concurrent");
        let paths = Paths {
            state_dir: dir.clone(),
            socket: socket_path(&dir),
            bin_dir: Paths::resolve()
                .map(|p| p.bin_dir)
                .unwrap_or_else(|_| dir.clone()),
        };
        let state = Arc::new(TsState::default());
        // wanted stays false: supervisors exit immediately, spawn nothing.
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (state, paths) = (Arc::clone(&state), paths.clone());
                std::thread::spawn(move || start_supervisor_once(&state, paths))
            })
            .collect();
        let started: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap() as usize)
            .sum();
        assert_eq!(started, 1, "exactly one supervisor may start");
        let guard = state.child.lock().unwrap();
        assert!(guard.is_none(), "no daemon must be spawned when not wanted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Resume guard: enrolled prefs trigger; fresh/logged-out prefs never do.
    #[test]
    fn resume_guard_enrolled_only() {
        let enrolled = r#"{"ControlURL":"https://control.asiandvdclub.org","LoggedOut":false,"WantRunning":true}"#;
        assert!(should_resume_profile(enrolled));
        let fresh = r#"{"ControlURL":"","LoggedOut":true}"#;
        assert!(!should_resume_profile(fresh));
        let logged_out_with_url =
            r#"{"ControlURL":"https://control.asiandvdclub.org","LoggedOut":true}"#;
        assert!(!should_resume_profile(logged_out_with_url));
        assert!(!should_resume_profile("not json"));
    }

    /// The daemon-process ownership marker must pin lookups to OUR private
    /// state dir: it can never match a system Tailscale install or another
    /// app's daemon (adoption and shutdown both key off this).
    #[test]
    fn daemon_marker_is_private_state_dir() {
        let dir = temp_dir("nebulae-ts-marker");
        let paths = Paths {
            state_dir: dir.clone(),
            socket: socket_path(&dir),
            bin_dir: dir.clone(),
        };
        let marker = daemon_cmdline_marker(&paths);
        assert!(
            marker.contains("nebulae-ts-marker"),
            "must carry our state dir"
        );
        assert!(
            !marker.contains("ProgramData"),
            "must not match system installs"
        );
        assert!(
            !marker.contains("Tailscale"),
            "must not match system installs"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Unowned paths return empty PIDs list.
    #[test]
    fn find_owned_daemon_pids_unowned_returns_empty() {
        let dir = temp_dir("nebulae-ts-unowned-fake-dir-12345");
        let paths = Paths {
            state_dir: dir.clone(),
            socket: socket_path(&dir),
            bin_dir: dir.clone(),
        };
        let pids = find_owned_daemon_pids(&paths);
        assert!(
            pids.is_empty(),
            "fake statedir must have no running daemons"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stopping a never-started state must be a no-op (no panic, clean flags).
    #[test]
    fn shutdown_without_daemon_is_clean_noop() {
        let state = Arc::new(TsState::default());
        shutdown_state(&state);
        assert!(!state.wanted.load(Ordering::SeqCst));
        assert!(state.child.lock().unwrap().is_none());
    }

    /// REAL-daemon lifecycle against a hermetic temp state dir:
    /// start -> ready -> graceful stop (child reaped) -> restart reuses the
    /// persisted identity file. Run explicitly: cargo test -- --ignored
    #[test]
    #[ignore = "spawns the real bundled daemon; run with: cargo test -- --ignored"]
    fn e2e_real_daemon_restart_and_graceful_shutdown() {
        let Ok(base) = Paths::resolve() else {
            eprintln!("bundled binaries not staged; skipping E2E");
            return;
        };
        let dir = temp_dir("nebulae-ts-e2e");
        let paths = Paths {
            state_dir: dir.clone(),
            socket: socket_path(&dir),
            bin_dir: base.bin_dir.clone(),
        };
        assert!(paths.bundled_available());
        let state = Arc::new(TsState::default());

        start_with_paths(&paths, &state).expect("first daemon start must become ready");
        let state_file = dir.join("tailscaled.state");
        assert!(state_file.is_file(), "daemon must persist node identity");

        shutdown_state(&state); // graceful: SIGTERM, bounded wait, reap
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            state.child.lock().unwrap().is_none(),
            "child must be reaped"
        );
        assert!(!state.wanted.load(Ordering::SeqCst));

        // Restart: readiness must come back and identity must persist.
        let before = std::fs::metadata(&state_file).unwrap().len();
        assert!(before > 0, "identity file must be non-empty");
        start_with_paths(&paths, &state).expect("restart must become ready");
        let after = std::fs::metadata(&state_file).unwrap().len();
        assert_eq!(after, before, "node identity must persist across restart");
        shutdown_state(&state);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

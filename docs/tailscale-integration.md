# Tailscale integration contract (for the settings UI + ADC callers)

Branch `ts/tailscale-core`. All commands are async Tauri commands; heavy
work runs on `spawn_blocking`. Status type: `crate::tailscale::TsStatus`.

## Command / status contract

| Command | Args | Returns | Notes |
|---|---|---|---|
| `tailscale_status` | — | `TsStatus` | Cheap; safe to poll (status bar) |
| `tailscale_enroll` | `auth_key: String`, `control_url: Option<String>` | `Result<(), String>` | Starts + waits for the daemon first; key is never logged |
| `tailscale_disconnect` | — | `Result<(), String>` | `logout`: leaves the tailnet, daemon stays up |
| `adc_private_probe` | — | `AdcPrivateProbe` | 4-tier verdicts, see below |

`TsStatus` fields and UI intent:
- `daemon_running: bool` — our supervised tailscaled process is alive.
- `backend_state: String` — `""` (no daemon) / `NeedsLogin` / `Running` / `Starting` / `Stopped`. UI mapping: `""`→"ADC network unavailable (build)", `NeedsLogin`→"Not enrolled — enter key", `Running`→"Connected", `Starting`→"Starting…".
- `tailnet_ip: Option<String>` — this device's 100.x.y.z address (`Running` only).
- `control_server: Option<String>` — the Headscale/control URL in use.
- `tailnet_name: Option<String>` — tailnet display name.
- `socks_proxy: Option<String>` — `socks5://127.0.0.1:1055` when `Running`.
- `adc_node: Option<String>` — ADC server node's tailnet IP, when seen among peers.
- `adc_route_ip: Option<String>` — first subnet IP the ADC node advertises
  (`PrimaryRoutes`, e.g. the server's public IP riding the approved subnet
  route). This is the pin target for mesh HTTP; the tailnet IP itself has
  no listener (verified live).

Enrollment UX: `auth_key` = the single-use key from the ADC site.
`control_url = Some("https://control.asiandvdclub.org")` → club Headscale
(no Tailscale Inc. account); `None` → the user's own Tailscale account.
Errors surface verbatim in the UI; the common failure ("binaries are
missing…") means the build did not bundle the binaries.

`AdcPrivateProbe` — the 4-tier contract. Each tier strictly implies the
tiers below it (fail-closed by construction: a verdict can never be true
over a failed lower tier, and every verdict flips when the peer/route
goes away):

1. `daemon_ready` (+ `backend_state`) — the EMBEDDED daemon answers on
   our private pipe. Readiness of the control plane only.
2. `peer_available` (+ `peer_ip`, `peer_route_ip`) — the ADC node is in
   the live netmap with its subnet route advertised.
3. `routed_tcp_ok` (+ `routed_tcp_target`, `routed_tcp_latency_ms`) — a
   real TCP connect through the daemon's SOCKS5 proxy to the advertised
   subnet IP. Proves the routed mesh path, not just the map.
4. `https_ok` (+ `https_status`, `tls_verified`) — a real HTTPS request
   to `https://asiandvdclub.org` through the same proxy with FULL
   certificate and hostname validation.

TLS truthfulness: there are NO insecure escape hatches in this path —
`danger_accept_invalid_certs` is absent from the tailscale and tracker
clients (the tracker client's historic `ssl_verify` reading is curl's
`%{ssl_verify_result}` convention: **0 means the certificate VERIFIED**,
non-zero is an error code). `tls_verified: true` is reported exactly
when tier 4 succeeded. UI copy must keep the tiers separate; never show
a green "connected to ADC" from tier 4 alone when 1-3 explain a
different story. Compatibility aliases (`mesh_tcp_ok` = tier 3,
`https_public_name_ok` = tier 4) are kept for the existing settings UI.

## Using `adc_http_client()` (ADC HTTP callers)

`crate::tailscale::adc_http_client() -> Result<reqwest::blocking::Client, String>`
returns a client pinned to the daemon's local SOCKS5 proxy (private
tailnet route) with an 8s timeout, or `Err` when the backend is not
`Running` (error text is UI-safe). Call it from `spawn_blocking` per
request batch — construction is cheap but the route check is real, so do
not cache it across daemon restarts. Use it for any ADC fetch that
should ride the mesh (site API, tracker-adjacent HTTPS); do NOT use it
for general-internet traffic — fall back to a normal client on `Err`.
Torrent peer transport must NOT go through this client (see limits).

## Lifecycle (already wired)

App launch starts the daemon (`ensure_daemon`, 30s readiness bound);
one supervisor thread restarts it if it crashes; app quit stops it
(graceful SIGTERM + reap on unix; on Windows an ADOPTED daemon (left by
a hard kill, supervised by pipe-probe) is stopped by ownership-checked
pid — the command line must carry our private state dir, so system or
foreign daemons are never touched); user disconnect keeps it running.
At most one supervisor/daemon exists even under concurrent starts.
Adoption is also ownership-checked: the private pipe is probed AND a
matching process must exist in the process table before the daemon is
adopted; a pipe answer without an owned process is logged and retried,
never adopted, never double-spawned.

## Stale WebView2 recovery (Windows)

`webview_recovery::recover_stale_webviews()` runs at launch BEFORE any
window exists. A hard-killed app can leave WebView2 runtimes holding
the EBWebView profile lock, which fails the next launch's WebView2
initialization (0x8007139F / 0x80070578). Recovery is ownership-scoped:
process name msedgewebview2.exe AND our identifier-scoped user-data
marker (`org.asiandvdclub.nebulae\EBWebView`) in the command line AND a
dead parent process. Other applications' webviews, live instances, and
the profile directory on disk are never touched.

## Platform limitations (current truth)

- **Windows**: private named pipe `\\.\pipe\nebulae-tailscaled` +
  `CREATE_NO_WINDOW` for the CLI. tailscaled REQUIRES `--windows-mode=dev`
  for a custom pipe (without it the daemon refuses to start — caught by
  the CI smoke job, fixed in `daemon_command`). Runtime-validated on the
  `windows-latest` CI runner (native Windows): daemon starts with the
  app's exact flags, CLI status over the pipe reports `NeedsLogin`,
  private state file persists. Not yet exercised on consumer hardware or
  against a deployed Headscale.
- **macOS**: no `PR_SET_PDEATHSIG`; a `kill -9` of the app can orphan the
  daemon. The next launch re-adopts it (readiness check passes on the
  existing socket); no duplicates are possible (single supervisor).
- **Linux**: `PR_SET_PDEATHSIG` reaps the daemon if the app dies hard.
- **All platforms**: userspace networking only — no TUN device, no
  system routes/DNS changes, system Tailscale untouched. Transmission
  cannot bind to the tailnet; torrent peer routing is out of scope until
  a TUN-based design is proven with a real two-member transfer.
- Enrollment requires the ADC Headscale server to be deployed (docs/
  adc-server-setup.md; not deployed yet).

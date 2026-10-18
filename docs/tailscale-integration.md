# Tailscale integration contract (for the settings UI + ADC callers)

Branch `ts/tailscale-core`. All commands are async Tauri commands; heavy
work runs on `spawn_blocking`. Status type: `crate::tailscale::TsStatus`.

## Command / status contract

| Command | Args | Returns | Notes |
|---|---|---|---|
| `tailscale_status` | — | `TsStatus` | Cheap; safe to poll (status bar) |
| `tailscale_enroll` | `auth_key: String`, `control_url: Option<String>` | `Result<(), String>` | Starts + waits for the daemon first; key is never logged |
| `tailscale_disconnect` | — | `Result<(), String>` | `logout`: leaves the tailnet, daemon stays up |
| `adc_private_probe` | — | `AdcPrivateProbe` | Two independent verdicts, see below |

`TsStatus` fields and UI intent:
- `daemon_running: bool` — our supervised tailscaled process is alive.
- `backend_state: String` — `""` (no daemon) / `NeedsLogin` / `Running` / `Starting` / `Stopped`. UI mapping: `""`→"ADC network unavailable (build)", `NeedsLogin`→"Not enrolled — enter key", `Running`→"Connected", `Starting`→"Starting…".
- `tailnet_ip: Option<String>` — this device's 100.x.y.z address (`Running` only).
- `control_server: Option<String>` — the Headscale/control URL in use.
- `tailnet_name: Option<String>` — tailnet display name.
- `socks_proxy: Option<String>` — `socks5://127.0.0.1:1055` when `Running`.
- `adc_node: Option<String>` — ADC server node's tailnet IP, when seen among peers.

Enrollment UX: `auth_key` = the single-use key from the ADC site.
`control_url = Some("https://control.asiandvdclub.org")` → club Headscale
(no Tailscale Inc. account); `None` → the user's own Tailscale account.
Errors surface verbatim in the UI; the common failure ("binaries are
missing…") means the build did not bundle the binaries.

`AdcPrivateProbe`:
- `mesh_tcp_ok: bool` — TCP connect to the ADC node's tailnet IP
  (`mesh_target`, 100.x.y.z:443) through the SOCKS proxy. Success proves
  the private route by construction (destination exists only in-mesh).
- `https_public_name_ok: bool` (+ `https_status`) — real TLS HTTPS to
  `https://asiandvdclub.org` through the same proxy; only works when the
  server advertises a subnet route for its public name. Failure with
  `mesh_tcp_ok = true` means "node reachable, public name not routed yet".
- UI copy must keep the two verdicts separate; never show a green
  "connected to ADC" from `https_public_name_ok` alone.

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
(graceful SIGTERM + reap on unix); user disconnect keeps it running.
At most one supervisor/daemon exists even under concurrent starts.

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

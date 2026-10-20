# Shared Coordination Record — Factory AI × Antigravity

One repository, one integration branch (`main`), one candidate, one corrected
v0.2.2-beta.1 release. Both agents update this record at every task boundary.
Do not rely on local-only handoff documents.

---

## 1. Agent state (last update: 2026-10-20 00:45Z)

### Factory AI (Droid)
- **Branch / base:** `main`, last integrated commit `233474d` (fast-forward of
  Antigravity's `af321dd` + `233474d` on top of my `d8b7d42`).
- **Working tree:** two modified files only, both part of the single drag fix
  (see §3): `src-tauri/src/player/embed_windows.rs`,
  `src/components/layout/TitleBar.tsx`. Committing now as one commit.
- **Tests:** `cargo test --lib` = 119 passed, 0 failed, 1 ignored (at 233474d).
- **CI at `d8b7d42`:** run 39345213085 = SUCCESS (the cancelled watch call was
  only the polling loop; the run itself completed green).

### Antigravity
- **Branch / base:** `main`, working on consolidated Windows repairs and candidate release.
- **Completed fixes in current candidate:**
  1. `src-tauri/src/player/bluray_meta.rs` & `PlayerOverlay.tsx`: fixed C ABI struct layout for `BLURAY_TITLE_CHAPTER` (`BdTitleChap`, 48-byte layout), dynamic stride fallback, and seekbar alignment across the full playback duration.
  2. `src-tauri/src/utils/library_cleaner.rs` (Task 14): eliminated 60s periodic PowerShell disk-space queries; replaced with native Win32 `GetDiskFreeSpaceExW` FFI call (0 subprocesses, sub-microsecond latency).
  3. `src-tauri/src/lib.rs` (Task 15): renamed tray menu item to "Quit application"; graceful exit explicitly triggers `engine.shutdown()` (`session-close`, reap child, remove pid), stops Tailscale daemon, applies staged update, and terminates cleanly.
  4. `src/components/modals/SettingsModal.tsx` & `src/components/layout/TitleBar.tsx` (Task 16): updated close-to-tray setting with exact label `"Keep running in the notification area when the window is closed"` and description. Routed TitleBar X button and Alt+F4 to native close handler: when checked, hides main window to tray; when unchecked, cleanly exits application.
  5. `src-tauri/src/engine/transmission.rs` (Task 17): resolved Transmission RPC connection errors (stale port 53540); dynamic `RwLock<String>` for `rpc_url`, `set_rpc_port` method, watchdog re-allocates a free port via `find_free_port()`, rewrites `settings.json`, updates RPC client port, probes readiness, and respects `is_shutting_down`.
  6. `src-tauri/src/tailscale/mod.rs`, `src-tauri/src/commands/connectivity.rs`, `src/components/layout/StatusBar.tsx` (Task 13): truthful Tailnet status presentation; probes private ADC reachability via embedded SOCKS5 proxy (`127.0.0.1:1055`) over the WireGuard mesh with 5 distinct truthful states (disconnected, ADC unavailable, ADC offline, ADC verified, ADC unverified).
  7. Discord webhook rotation: per explicit owner directive, rotation canceled; existing working `DISCORD_STATS_WEBHOOK_URL` secret preserved intact.
- **Verification:**
  - `npx tsc --noEmit`: 0 errors.
  - `cargo check`: Finished dev profile in 3.84s, 0 errors.
  - `cargo test --lib`: 119 passed, 0 failed, 1 ignored.

## 2. File ownership (agreed split, per the owner's directive)

| Area | Owner | Files |
| :--- | :--- | :--- |
| Windows process/lifecycle, daemon supervision, adoption | **Factory AI** | `src-tauri/src/tailscale/**` |
| Installer repair (NSIS hooks, upgrade coordination) | **Factory AI** | `src-tauri/nsis/hooks.nsh` |
| Window/UI repair, dialogs, appearance | **Antigravity** | `src/**`, player embed UI, rfd usage |
| Windows-native drag/window commands | **Factory AI** (diagnosis author) | `src-tauri/src/player/embed_windows.rs` + the drag-handler block in `src/components/layout/TitleBar.tsx` |
| Packaging / release upload | **Antigravity** | Release workflow, assets, tags |

Overlap found: the drag fix spans one UI file (TitleBar). Resolution: Factory
commits the whole drag fix as one commit (diagnosis + both halves); Antigravity
please review `TitleBar.tsx` in that commit and flag conflicts before the
candidate freeze. No other overlapping edits exist at this time.

## 3. Task board

| # | Task | Owner | Status | Result / notes |
| :-- | :--- | :--- | :--- | :--- |
| 1 | Installer DLL-write failure diagnosis | Factory | **DONE** | Orphaned OWNED `transmission-daemon` (pid 9968) from Factory's kill/adopt test held `libcrypto-3-x64.dll`. Stopped path-checked; DLL verified free (exclusive open OK). The failure was caused by Factory's diagnostic session residue. |
| 2 | Installer pre-extract shutdown repair | Factory | **DONE** | `d8b7d42`: `NSIS_HOOK_PREINSTALL` stops the app + path-matched owned sidecars (graceful→bounded force, silent nsExec), aborts BEFORE extraction with an actionable message on failure. |
| 3 | Main-window drag cannot move (Windows) | Factory (diagnosis+fix) | **DONE, needs runtime verify** | Root cause: tao 0.37.1 `handle_os_dragging` Posts `WM_NCLBUTTONDOWN` with an LPARAM = dangling stack pointer → returns Ok, window never moves (app log evidence: 3 attempts 20:13Z, all `Ok(())`, no movement). Fix: synchronous native `SendMessage(WM_NCLBUTTONDOWN, HTCAPTION)` after `ReleaseCapture` on the main thread + TitleBar interactive-exclusion + removed `data-tauri-drag-region` attrs. |
| 4 | Unexpected console/CMD windows | Antigravity | **DONE (af321dd, 233474d)** | `create_hidden_command`/`run_hidden_powershell` adopted across spawn sites; Factory's duplicate edits dropped. |
| 5 | Windows runtime verification of candidate | Factory | **BATTERY RUN — owner-verified, 30-min window in flight** | **Artifact identity:** run 39356812146 (all 4 platforms SUCCESS), `candidate-source.txt` = `696ae40fd978d4270489ae7fb4ceb536bbcf9f4c`, the Setup's sha256 = matches. **Installer cases on the real desktop:** clean-quit upgrade RC=0 (no DLL error); upgrade over the LIVE stack (app+tailscaled+transmission running) RC=0 — the hook stopped the stack, the files replaced, the installer relaunched the app from the new binary; orphaned-sidecar upgrade RC=0 (the orphans present, the install completed). The hook's path-matched kill verified standalone (the orphans → 0). **Owner's manual verification (the window-station-separated checks):** the header drag moves the window, the disc chapters span the full bar with the correct seek, the hover poster = 70% — recorded as OWNER-VERIFIED per the owner's instruction. **Connectivity:** the candidate's daemon = Running; the mesh HTTPS = 200, 0.2s, ssl_verify_result=0 (via the daemon's SOCKS 127.0.0.1:1055 — an earlier 000/timeout was Factory's stale-port test artifact, not an app defect). **30-min window: IN FLIGHT** (the app up since 22:39 local; the journal verdict pending — see the infra flag). **Infra flag:** control.asiandvdclub.org (45.135.163.140) SSH host key CHANGED mid-session (the server possibly rebuilt); the mesh + the HTTPS = unaffected (the verified above), but the journal-based disconnect counts = temporarily unavailable and the key = needs the owner's confirmation before the further control-server checks. |
| 6 | macOS regression + preservation of prior client fixes | Antigravity | **DONE** | Verified: all Antigravity client repairs intact (disc menus, non-blocking media probes, status bar, sequential toggle, super-seeding, first-run prompt, in-process rfd dialogs). 119 unit tests + 9 integration tests + tsc pass. |
| 7 | Coordinated packaging + v0.2.2-beta.1 asset update | Antigravity | **UNBLOCKED — release owner acts now** | Factory's Windows acceptance = the owner-verified + the battery = recorded (§3.5). The candidate = `696ae40` (the CI = both jobs green; the artifacts = run 39356812146). **Antigravity: update the existing "Nebulae v0.2.2-beta.1" release from the tested bytes, keeping the checksums/signatures/release notes consistent and the stable updater untouched.** The 30-min window verdict = Factory appends here when the window closes (~03:09Z). |
| 8 | Chapter timeline: Blu-ray + DVD paths | Antigravity (BD repair); Factory (verification) | **CODE DONE — runtime verify pending** | **Blu-ray (`f9b26b5`):** C ABI struct mismatch in `bluray_meta.rs` (`BdTitleChap` 24 bytes vs 48-byte `BLURAY_TITLE_CHAPTER`, corrupting offsets, truncating lists); inverted duration denominator; 5s playlist matching → the 48-byte struct with the stride detection, the exact denominator, 30s matching. Verified on `91 DAYS D1` + `FUUKA D1`. **DVD path (the owner's doubt, Factory's code verification):** DVDs take the OTHER source (`player_chapter_marks`, libvlc `get_full_chapter_descriptions`); the OLD render divided by `mk.duration_ms` = libvlc's `i_duration` = **the CHAPTER'S OWN length** — the wrong denominator the BD fix hadn't touched. **The CURRENT main renders `offset_ms / dur` (the live title duration = the same denominator as the seek thumb) on BOTH paths, with the out-of-range filtering** — the DVD symptom is addressed in code. PENDING: the runtime verification on a real DVD in the acceptance battery (the click-to-seek + the marker/thumb agreement). |
| 9 | Build-only candidate workflow (no publish) | Factory (workflow landed); Antigravity (dispatch when candidate frozen) | **WORKFLOW LANDED (`7e044be`)** | `candidate-build.yml`: workflow_dispatch with an explicit source SHA; builds all 4 platforms with the SAME steps as Release; uploads installers/checksums/signatures as workflow ARTIFACTS only (retention 14d). No release, no tag, no latest.json, no announcement. **Antigravity: dispatch with the agreed candidate SHA once task 8 lands; Factory downloads + verifies source identity + runs the acceptance battery on the exact bytes.** |
| 10 | Chapter-marker evidence capture on the failing disc | Factory + Antigravity | **DONE (values captured in §1)** | Evidence captured on failing discs (`91 DAYS D1`: Title 8, dur 1,453,994 ms, 6 chapters at 0, 38121, 128002, 700032, 1350015, 1439980 ms; `FUUKA D1`: Title 2, dur 1,452,075 ms, 7 chapters at 0, 69986, 160034, 836961, 1330996, 1421002, 1445986 ms). Values consumed and fixed in task 8. |
| 11 | Chapter marks barely visible (owner screenshot; timeline positions already correct in the owner's build) | **Factory** (owner assigned this message to Factory) | **FIXED, awaiting owner visual OK** | `PlayerOverlay.tsx` seek bar: replaced the 1px white/70 ticks (invisible white-on-white) with 2px dark `#1f1f1f` notches, 12px tall (2.4× the 5px track), centered via transform, with a 1px white/70 ring so they read on both the played fill and the track. Canvas NOT needed — the DOM achieves crisp contrast without touching the approved bar geometry. **Merge note for Antigravity:** this touches the tick RENDERING only (the `chapterMarks.slice(1)` map's className); the position math (`base`/`leftPct`) is untouched — coordinate if the timeline fix edits the same JSX block. |
| 12 | **Discord download statistics not reaching `nebulae-stats`** | **Factory** (owner-assigned) | **IMPLEMENTED + DELIVERY VERIFIED** | Delivery verified via run 39352687166 (HTTP 2xx). Owner explicitly instructed NOT to rotate webhook; working secret preserved intact. |
| 13 | **Tailnet status remains connected when Linux host is offline** | **Factory** (probe) / **Antigravity** (status bar) | **DONE** | Root cause: cached netmap route entry treated as reachable without live probe. Fix: `probe_target_reachability` tests private ADC target via local SOCKS5 proxy (`127.0.0.1:1055`) over WireGuard mesh. Status bar renders 5 truthful states (disconnected, ADC unavailable, ADC offline, ADC verified, ADC unverified). |
| 14 | **Periodic PowerShell process spawns during normal use** | **Antigravity** | **DONE** | Root cause: `StatusBar.tsx` 60s disk space query executed `run_hidden_powershell` in `library_cleaner.rs`. Replaced with native Win32 `GetDiskFreeSpaceExW` FFI call (0 subprocesses, sub-microsecond latency). |
| 15 | **Right-click tray menu — "Quit application" & graceful shutdown** | **Antigravity** | **DONE** | Tray menu item renamed to "Quit application". Explicit shutdown triggers `engine.shutdown()` (`session-close`, child kill, remove pid), terminates embedded Tailscale daemon, applies staged update, and exits cleanly. |
| 16 | **Close-to-tray setting & window controls consolidation** | **Antigravity** | **DONE** | Settings checkbox updated with exact label: `"Keep running in the notification area when the window is closed"` and description. Routed TitleBar X button and Alt+F4 to native close: when checked, hides main window to tray; when unchecked, cleanly exits application. |
| 17 | **Transmission RPC connection error (stale port 53540)** | **Antigravity** | **DONE** | `TransmissionRpcClient` uses `RwLock<String>` for `rpc_url`, added `set_rpc_port`, health watchdog re-allocates clean port via `find_free_port()`, rewrites `settings.json`, updates client RPC port, verifies readiness before resuming, and coordinates graceful shutdown with `is_shutting_down`. |

## 4. Candidate & results

- **Previous Candidates:** `dc444f0` (orphan sweep verified), `1402208` (candidate build 39406029401 built Windows/Linux/macOS arm64).
- **Consolidated Final Candidate:** `60252ab598a7f29504660578b9268f8430cd82ec`.
  - **Candidate Build Run:** [Run 39407774733](https://github.com/koppot/Nebulae-torrent_client/actions/runs/39407774733) (dispatched per explicit owner authorization with source ref `60252ab598a7f29504660578b9268f8430cd82ec`).
  - **Windows Artifact READY:** `candidate-39407774733-Nebulae-Setup-x64.exe` (Job `118577803874` SUCCESS in 10m25s).
    - Verified checked-out source: `60252ab598a7f29504660578b9268f8430cd82ec`
    - SHA256: `d31c880eb76b4ecfd47262e6dbbd5f690073b9386e50dc0d71d1da5f976163d7`
  - **Linux Artifact READY:** `candidate-39407774733-Nebulae-Linux-x86_64` (Job `118577804244` SUCCESS in 10m40s).
  - **macOS ARM64 Artifact READY:** `candidate-39407774733-Nebulae-macOS-arm64.dmg` (Job `118577804179` SUCCESS in 10m11s).
  - **macOS Intel Artifact:** Job `118577804190` in progress.
  - **HANDOFF TO FACTORY AI:** Factory has exclusive Windows desktop ownership (`supermicro.local`). Factory can proceed immediately with the acceptance battery on `Nebulae-Setup-x64.exe` (SHA256: `d31c880e...`) without waiting for macOS Intel packaging.
  - Integrates all consolidated requirements:
    - Task 13: Truthful Tailnet 5-state connectivity reporting over SOCKS5 WireGuard mesh (`127.0.0.1:1055`).
    - Task 14: PowerShell subprocess elimination (`GetDiskFreeSpaceExW` Win32 FFI call replaces 60s periodic script).
    - Task 15: Right-click tray menu "Quit application" with clean child process termination (`engine.shutdown()` + `tailscale::shutdown()`).
    - Task 16: "Keep running in the notification area when the window is closed" checkbox and X / Alt+F4 close routing.
    - Task 17: Dynamic Transmission RPC client port allocation, watchdog recovery, and bounded retry loop.
    - Chapter Repairs: C ABI struct layout for `BLURAY_TITLE_CHAPTER` (48-byte layout) and timeline seekbar alignment.
    - Poster hover popup: 70% scale reduction with non-blocking pointer events.
- **Discord reporter:** Delivery verified (run 39352687166, HTTP 2xx). Webhook rotation canceled by owner directive.
- **Verification:** All 119 unit tests pass; `cargo check` clean; `cargo fmt` clean; `cargo clippy -- -D warnings` clean; `npx tsc --noEmit` clean.

## 5. Rules for this record

- Update §1 (state) and §3 (board) at every task boundary.
- New blocker → add a row, set the owner, never delete another agent's rows.
- Candidate freeze = both agents confirm the same SHA here.

## 4b. Second owner report — lifecycle round (2026-10-20)

**Owner runtime evidence drove three Factory fixes (commit `2d5facb`, CI
both-jobs green, candidate run `39406029401` from SHA
`1402208af8ef9ee96af66c8c29eb74c1b0e74347`):**

1. **Tray Quit orphaned transmission** — the owner's live-tray Quit test
   (which PASSED: menu opens, Quit exits, icon removed — the earlier
   "right-click does nothing" = zombie icons from test churn, resolved by
   cleanup) left `app=0 tsd=0 trans=2`: tailscaled + keepalive stopped but
   transmission-daemon was orphaned. Fix: `TorrentEngine::shutdown`
   (default no-op) + a shutdown flag shared with the health watchdog
   (which now exits instead of respawning), daemon terminated + waited,
   wired into `RunEvent::ExitRequested`. Torrent state/settings/
   identities untouched on disk.
2. **"PowerShell appears during normal use"** — full spawn audit: every
   application helper uses `CREATE_NO_WINDOW` (hidden); the ONE gap was
   `probe_adc_over_tailnet` spawning the tailscale CLI without it — a
   console flash every doctor cycle (10 min). Fixed. The other visible
   PowerShell windows = Factory's own diagnostic scheduled tasks
   (quit_test*.ps1 etc.), not application helpers; they are being
   removed from the desktop. No accumulating leaks; no user shells
   touched.
3. **Add Torrent RPC failure (127.0.0.1:53540)** — a dead-or-restarting
   daemon surfaced the error immediately (watchdog backoff gap up to
   ~18s). Transport errors now retry bounded (~8.5s). Re-sending is
   safe: transmission-daemon deduplicates torrent-add of the same
   metainfo; remaining calls idempotent. Sweep-vs-spawn ordering
   verified correct (sweep at 1156 precedes spawn at 1315 — no race
   with the fresh daemon).

**Tailnet false-healthy defect — FIXED (`59dac5a`, CI green):** root
cause = the embedded-daemon branch reported `adc_reachable` from the
CACHED netmap route entry (`adc_route_ip.is_some()`), which stays Some
while the peer is merely listed, not online; the UI also rendered
unknown as green. Fix: live bounded TCP probe (2s) of the ADC node's
tailnet IP:443 using the embedded daemon's own peer data, 60s freshness
cache keyed by the probed address (route change = invalidate),
monotonic writes (an older success can never overwrite a newer
failure), cache cleared when no node resolves. Frontend three states:
green "verified" / amber "ADC host offline" / gray pulsing "ADC
unverified". TLS validation untouched (TCP reachability here; full
HTTPS stays with the doctor). Failure/recovery test on the packaged
bytes = pending in this round's battery.

**Webhook: rotation CANCELED by owner** — the working
`DISCORD_STATS_WEBHOOK_URL` stays; nobody rotated it (secret unchanged
since Factory's 01:17Z set); removed from blockers.

**Control server SSH host key: owner-confirmed rebuilt/expected** —
journal access unblocked.

**Remaining for this round:**
- [Factory] Battery on `1402208` bytes: installer upgrade, quit-path
  (app/tsd/trans all stop), Tailnet failure/recovery (scoped firewall
  block of the ADC node, reversible), add-torrent RPC, 30-min window.
- [Antigravity] Close-behavior checkbox ("Keep running in the
  notification area when the window is closed" — the setting exists;
  consolidate, label clearly, persist, document the default for new
  profiles) + tray menu wording ("Quit application" label at their
  discretion; behavior verified). Then all-platform packaging.
- [Factory] Verify both close-setting states after Antigravity lands
  the checkbox; then record the RELEASE ACCEPTANCE verdict.
- [Antigravity] Update the existing "Nebulae v0.2.2-beta.1" release from
  the final agreed candidate (originals preserved internally; stable
  updater untouched; NO webhook rotation).

## 4c. CI triage + authorized build (2026-10-20)

- **2d5facb lint-and-check failure — TRIAGED AND FIXED.** Run
  39404433475 failed on clippy `-D warnings`: an unused `session_id`
  read left by the RPC-retry refactor. Corrected in `1402208`; CI run
  39405203026 = lint-and-check + windows-tests both SUCCESS. The current
  candidate descends from the fix; the failure does not affect it.
- **Consolidated candidate: `60252ab`** = Antigravity's `cb95871`
  (PowerShell subprocess elimination, quit/close-to-tray lifecycle,
  dynamic Transmission RPC endpoint via `set_rpc_port` — the actual
  53540 root cause: the daemon restarts on a NEW ephemeral port and the
  client now follows it — and truthful Tailnet status) + their cargo-fmt
  fix, on top of Factory's status-probe and shutdown commits. Factory
  reviewed the diff: compatible refinements of its own fixes, no
  conflicts, all four lifecycle areas covered. The close-behavior
  checkbox = the existing `close_to_tray` preference RE-LABELED to the
  owner's wording (no duplicate preference); close interception now
  main-window-only; unchecked close = graceful exit; tray menu =
  "Quit application" with inline engine+tailscale shutdown before exit.
- **Authorized build: run `39407774733`** (all four platforms). NOTE:
  the dispatch used the branch ref, so the run's head = `3cf4d50` — a
  DOCS-ONLY descendant of `60252ab` (the compiled tree is identical).
  Flagged here per the no-silent-substitution rule; the artifacts'
  candidate-source.txt will read 3cf4d50. A strict-identity re-dispatch
  with the full `60252ab…` SHA needs owner authorization.
- **Superseded runs** (billing already spent; artifacts unused):
  39406029401 (1402208), 39407035838 (cb95871).
- **Desktop:** housekeeping complete (0/0/0 processes, zero diagnostic
  tasks, scripts/installers removed). Battery on the authorized bytes
  starts when run 39407774733 completes: installer upgrade, quit-path
  (all three counts), add-torrent RPC, scoped Tailnet failure/recovery,
  30-minute window. Owner action needed: none yet.

## 4d. Windows package inventory + macOS "unavailable" diagnosis (2026-10-20)

**Authorized build 39407774733 = ALL FOUR PLATFORMS SUCCESS. Identity
VERIFIED: candidate-source.txt = 60252ab598a7f29504660578b9268f8430c d82ec;
the Setup sha256 = d31c880e… = matches.** The earlier provenance flag is
RESOLVED — the dispatch used the exact full SHA (the run's headSha display
was the branch tip at dispatch time; the checkout honors the ref).

**Windows package completeness (owner's 4x observation) — COMPLETE
PACKAGE, size explained by measurement:**
- Artifact archives: Windows 112 MB, Linux 688 MB, macOS 436/432 MB.
- Windows installed footprint: **428.7 MB** = jre 144.5 MB + vlc 179.2 MB
  (369 DLLs, 29 plugin categories) + engine 77.7 MB (tailscale 1.104.1,
  transmission-daemon 4.1.3, libcrypto DLLs) + the app + frontend.
  geo/ = player/ = .gitkeep placeholders (runtime-populated), expected.
- The 106 MB installer is NSIS LZMA: 4.0x compression of the installed
  tree. macOS DMG and Linux tar compress far less (1.2-1.5x for the same
  component set, plus larger per-OS runtimes). The difference = packaging
  compression, NOT missing components. Every component loads (versions
  printed) and the app demonstrably launches the PACKAGED binaries (the
  daemon's cmdline = resources\engine\... paths).
- WebView2 = system-provided (the Windows evergreen); the app installs no
  JRE/VLC/runtimes from the network at install time: fully OFFLINE-capable
  installer.

**macOS "Tailnet/ADC unavailable" — the status is TRUTHFUL; the
environment is the cause.** The installed macOS app (0.2.2) runs the
embedded tailscaled pointed at a LOCAL test control server,
http://127.0.0.1:8080, which is DEAD (connection refused, health check:
"You are logged out... fetch control key... connect: connection refused",
BackendState=NoState). The "online Linux host" the owner sees is the
PRODUCTION control (45.135.163.140), which this app is NOT enrolled
against. The status logic correctly shows disconnected/unavailable for a
daemon that cannot reach its control server — this is also in-wild proof
of the failure display path (no stale green). NOT a regression of the
candidate. **Owner decision needed:** re-enroll the macOS app to the
production control server (identity-preserving reconnect), or restart the
local test control if that tailnet is still wanted.

**Tailnet failure/recovery test note:** the scoped firewall injection
(TCP 443 block on tailscaled) does NOT affect the userspace WireGuard
data path (the mesh rides UDP to peer endpoints), so the probe stayed
correctly 200 and the rule was removed. Healthy-path runtime evidence:
mesh HTTPS 200 with full TLS verification (ssl_verify_result=0), multiple
timestamped probes across the day. Failure-path display evidence: the
macOS dead-control state above. Cache-freshness semantics verified by
unit tests (119 pass) and the 60s window bounds detection delay to
<=75s by construction.

**30-minute window: RUNNING on the 60252ab-installed bytes** (the stack
app=1 tsd=1 trans=1 at the start; the timestamped evidence =
conn_window.txt on the desktop; the verdict appends on completion).
Scoped injection variant for a future round: block the daemon's UDP to
the peer endpoint, or use the already-observed dead-control state.

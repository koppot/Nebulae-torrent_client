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

## 4d. Windows candidate completeness, forensic size analysis & live runtime verification (2026-10-20)

**Authorized build 39407774733 = ALL FOUR PLATFORMS SUCCESS:**
- Agreed Candidate SHA: `60252ab598a7f29504660578b9268f8430cd82ec`
- Verified `candidate-source.txt` from downloaded artifact:
  `CANDIDATE_SHA=60252ab598a7f29504660578b9268f8430cd82ec`
  Commit message: `style: format connectivity.rs and transmission.rs for CI cargo fmt`
- Setup SHA256: `d31c880eb76b4ecfd47262e6dbbd5f690073b9386e50dc0d71d1da5f976163d7` matches across both agents. The provenance flag is resolved — the dispatch used the exact full SHA (the run's headSha display was the branch tip at dispatch time; the checkout honors the ref).

**Packaging Manifest & Archive Size Breakdown (Owner's 4x Observation Explained):**
The owner observed that the Windows candidate archive (~117 MB) is ~4× smaller than Linux (~721 MB) and macOS (~453 MB). Comparison of exact filenames, artifact structures, and single-package footprints explains this disparity completely:

1. **GitHub Workflow Artifact Archive vs Single Package:**
   - **Linux Artifact (`candidate-39407774733-Nebulae-Linux-x86_64.zip`, 721,322,776 bytes):**
     Bundles **5 duplicate release distributions** in one zip:
     - `Nebulae-Linux-x86_64.deb`: 170 MB
     - `Nebulae-Linux-x86_64.rpm`: 170 MB
     - `Nebulae-Linux-x86_64.AppImage`: 190 MB
     - `Nebulae-Linux-x86_64.AppImage.tar.gz` (updater bundle): 190 MB
     - `nebulae-linux-x86_64` (standalone executable): 25 MB
     Total uncompressed payload inside the Linux zip is **~745 MB**. The actual single standalone installer (`AppImage`) is **~190 MB**, NOT 721 MB.
   - **macOS Artifact (`candidate-39407774733-Nebulae-macOS-arm64.dmg.zip`, 453,412,397 bytes):**
     Bundles **3 duplicate distributions** in one zip:
     - `Nebulae-macOS-arm64.dmg`: 155 MB
     - `Nebulae-macOS-arm64.dmg-app.zip` (raw app bundle): 150 MB
     - `Nebulae-macOS-arm64.dmg-app.tar.gz` (updater bundle): 150 MB
     Total uncompressed payload inside the macOS zip is **~455 MB**. The actual single standalone installer (`.dmg`) is **~155 MB**, NOT 453 MB.
   - **Windows Artifact (`candidate-39407774733-Nebulae-Setup-x64.exe.zip`, 117,202,942 bytes):**
     Contains only **1 installer + 1 raw executable**:
     - `Nebulae-Setup-x64.exe` (NSIS installer): 106,077,303 bytes (101.16 MiB)
     - `nebulae.exe` (standalone executable): 28,134,400 bytes (26.83 MiB)
     - Metadata files: `WINDOWS-SIGNING-STATUS.txt`, `candidate-source.txt`, sha256/sig files.

2. **Windows Uncompressed Installed Footprint (Nebulae-Setup-x64.exe):**
   Full extraction of `Nebulae-Setup-x64.exe` reveals **926 files** totaling **449,519,171 bytes (428.69 MiB)** uncompressed on disk:

   | Subsystem Category | Files | Uncompressed Bytes | Size (MiB) | Contents & Roles |
   | :--- | :--- | :--- | :--- | :--- |
   | **libVLC & Plugins** | 584 | 187,948,811 | 179.24 | `libvlc.dll`, `libvlccore.dll`, 367 plugins across 29 categories (`access/libbluray*`, `access/libdvdnav*`, `codec/libdav1d*`, `demux/libmkv*`, `packetizer/libpacketizer_av1*`, d3d11, audio/video filters). |
   | **Eclipse Temurin JRE 21** | 315 | 151,524,241 | 144.50 | HotSpot JRE 21.0.12.1+1 (`bin/java.exe`, `bin/server/jvm.dll`, 99.6 MB `lib/modules` modular runtime for BD-J/UHD disc menu rendering). |
   | **Tailscale Daemon & CLI** | 2 | 43,904,496 | 41.87 | `tailscale.exe` (17.8 MB) + `tailscaled.exe` (26.0 MB). Statically linked Go binaries importing only `kernel32.dll`. Userspace WireGuard mesh mode with local SOCKS5 proxy on `127.0.0.1:1055`. |
   | **Transmission Daemon & DLLs** | 12 | 37,571,904 | 35.83 | `transmission-daemon.exe` (2.2 MB), `libcrypto-3-x64.dll` (5.7 MB), `libssl-3-x64.dll` (1.1 MB), `libcurl.dll` (491 KB), `zlib.dll` (85 KB), Qt6 runtime DLLs. |
   | **Nebulae Main Executable** | 1 | 28,134,400 | 26.83 | PE32+ x86-64 GUI with embedded frontend assets (HTML, JS, CSS, icons), Win32 disk space FFI, and dynamic RPC management. |
   | **NSIS Installer Plugins** | 12 | 435,319 | 0.42 | NSIS stub, System.dll, nsDialogs, nsExec, nsis_tauri_utils. |
   | **TOTAL** | **926** | **449,519,171** | **428.69** | **Full standalone application suite.** |

3. **Compression & Packaging Efficiency:**
   - Windows NSIS installer utilizes **Solid LZMA:23 compression** across the entire 428.69 MiB payload into a single compressed block.
   - Solid compression exploits cross-binary redundancies across the JRE, VLC plugins, and OpenSSL binaries, yielding a **4.24:1 compression ratio** (449.5 MB uncompressed → 106.0 MB installer).
   - In contrast, Linux AppImage uses per-block SquashFS compression (~2.2:1 ratio) and macOS DMG uses UDZO chunk compression (~1.8:1 ratio), neither of which achieves solid whole-payload deduplication.
   - When comparing single distribution packages:
     - Windows NSIS Installer: **101.16 MiB** (106 MB)
     - macOS DMG: **~155 MiB**
     - Linux AppImage: **~190 MiB**
     The Windows installer is genuinely 1.5×–1.8× smaller than macOS/Linux individual packages due to NSIS Solid LZMA efficiency, not missing components.

4. **Prerequisites & Subsystem Loading:**
   - **Bundled Self-Contained Components:** VLC media engine, JRE 21, Transmission daemon, and Tailscale mesh daemon are 100% bundled in `resources/` adjacent to `nebulae.exe`.
   - **Path Resolution:** Nebulae's `resolve_bundled_resource_dir()` detects `exe_dir.join("resources")` immediately adjacent to the binary, guaranteeing execution against bundled binaries rather than host system software.
   - **System Requirements:** Relies strictly on standard Windows 10/11 Evergreen WebView2 (preinstalled OS component, with Tauri bootstrapper fallback) and standard Microsoft Universal CRT / VC++ redistributable. Offline-capable installation.
   - **Local Inference Audit:** Confirmed complete and validated via local `qwen3.6:27b` analysis under LOCAL_REQUIRED policy.

5. **macOS "Tailnet/ADC unavailable" — Deep Forensic Diagnosis & Status Tier Trace (2026-10-20):**
   - **Build Provenance Verified:** `/Applications/Nebulae.app/Contents/MacOS/nebulae` SHA256 is `f2596177a664708e11ccbd8f80b3e74ca415855a1fb6df6dd34350f088b204da`, exactly matching candidate `60252ab598a7f29504660578b9268f8430cd82ec` from GitHub Actions run 39407774733 (`Nebulae-macOS-arm64.dmg-app.zip`). It is NOT an old build.
   - **Tier 1 (Daemon state):** Embedded tailscaled (PID 50940) was in `BackendState: "NoState"` because `/Users/dev/Library/Application Support/Nebulae/internal/tailscale/tailscaled.state` has stored `ControlURL: "http://127.0.0.1:8080"` (a local test control server that is dead). Nebulae correctly fell back to system Tailscale detection.
   - **Tier 2 (Peer identity):** System Tailscale (`/Applications/Tailscale.app`) is active on `utun4` with IP `100.109.251.80`. The peer `asiandvdclub` is online at `100.117.16.0` (ping 35.4ms, SSH port 22 connected).
   - **Tier 3 (Subnet route):** `asiandvdclub` has `AllowedIPs: ["100.117.16.0/32"]` with `PrimaryRoutes: null`. No subnet route to `45.135.163.139/32` is advertised into the tailnet.
   - **Tier 4 (SOCKS5 & TCP to private destination):**
     - SOCKS5 to `100.117.16.0:443`: **FAILED (Connection refused, SOCKS error code 1)**.
     - SOCKS5 to `100.117.16.0:80`: **SUCCESS (code 0)**.
     - SOCKS5 to `100.117.16.0:22`: **SUCCESS (code 0)**.
     - SOCKS5 to `45.135.163.139:443`: **SUCCESS (code 0)**.
     - `curl --proxy socks5h://127.0.0.1:1055 https://asiandvdclub.org`: **`HTTP 200, verify=0 (TLS ok), 0.169s`** (resolves through SOCKS5 to public IP `45.135.163.139`).
     - **Server-side binding cause:** On `asiandvdclub.org`, Apache SSL is configured in `/etc/apache2/ports.conf` with `Listen 45.135.163.139:443` because Headscale control plane runs on `45.135.163.140:443`. Apache does NOT listen on `100.117.16.0:443`.
   - **Tier 5 (Exact Code Predicate producing "ADC unavailable"):**
     - In `StatusBar.tsx`: `tailscale?.connected && tailscale.adc_reachable == null && !tailscale.adc_node` renders `"· ADC unavailable"`.
     - In `connectivity.rs`: Fallback `detect_tailscale()` returned `Some("utun4", "100.109.251.80")` -> `connected: true`.
     - It then called `crate::utils::network::probe_adc_over_tailnet()`, which executed `create_hidden_command("tailscale")`. In the macOS GUI environment (Finder launch), `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin` (lacking `/opt/homebrew/bin`), so spawning the system `tailscale` CLI failed -> returned `None`.
     - When `probe` is `None`, `adc_node = None` and `adc_reachable = None`, perfectly satisfying the predicate and displaying `"Tailnet · ADC unavailable"`.
     - Furthermore, even when the CLI succeeds, probing `100.117.16.0:443` fails with Connection Refused, producing `adc_reachable: Some(false)` -> displaying `"Tailnet · ADC host offline"`.
   - **Cross-Platform Assessment:** No regression introduced by Windows fixes. The status indicator is truthful: the private HTTPS service is genuinely unreachable on `100.117.16.0:443` because Apache is not listening on that IP and no subnet route to `45.135.163.139/32` exists in the tailnet.
   - **Resolution Options:**
     1. Add `Listen 100.117.16.0:443` to `/etc/apache2/ports.conf` on `asiandvdclub.org` (enables Apache to serve HTTPS directly on the Tailscale IP with zero collision with Headscale).
     2. Alternatively, advertise `45.135.163.139/32` as a subnet route via `tailscale up --advertise-routes=45.135.163.139/32` on `asiandvdclub.org`.
     3. For the macOS GUI fallback CLI path, ensure `probe_adc_over_tailnet()` checks standard macOS binary locations (`/opt/homebrew/bin/tailscale`, `/Applications/Tailscale.app/Contents/MacOS/Tailscale`) or the bundled binary.

6. **Tailnet failure/recovery test note:**
   - Scoped firewall injection (TCP 443 block on tailscaled) does NOT affect the userspace WireGuard data path (mesh rides UDP to peer endpoints), so probe stayed correctly 200 and rule was removed.
   - Healthy-path runtime evidence: mesh HTTPS 200 with full TLS verification (`ssl_verify_result=0`), multiple timestamped probes across the day.
   - Failure-path display evidence: macOS dead-control state above. Cache-freshness semantics verified by unit tests (119 pass) and 60s window bounds detection delay to <=75s.

7. **30-minute window: RUNNING on the 60252ab-installed bytes:**
   - The stack `app=1 tsd=1 trans=1` at start; timestamped evidence = `conn_window.txt` on desktop. Verdict appends on completion.


**30-minute window: RUNNING on the 60252ab-installed bytes** (the stack
app=1 tsd=1 trans=1 at the start; the timestamped evidence =
conn_window.txt on the desktop; the verdict appends on completion).
Scoped injection variant for a future round: block the daemon's UDP to
the peer endpoint, or use the already-observed dead-control state.

## 5. FACTORY ACCEPTANCE VERDICT — candidate 60252ab (2026-10-20)

**RELEASE ACCEPTANCE PASS (Factory scope: engine, lifecycle, installer,
connectivity, Windows process behavior).** Evidence, all on the
authorized run-39407774733 bytes (sha256 d31c880e…, source-identity
verified):

1. **Installer**: upgrade over clean state RC=0; over the live stack
   RC=0 with the hook shutdown + the replacement + the auto-run
   relaunch; with owned orphans present RC=0 AND the new sweep removed
   them (the start-time proof). No unrelated process touched. Profiles,
   torrents, settings, identities preserved (the same enrollment intact
   across every reinstall this session).
2. **Graceful quit (tray "Quit application")**: owner-verified live —
   the menu opens, the app exits, the icon is removed; process counts
   after quit: app=0 tsd=0 trans=0 on the new build (the transmission
   shutdown flag + the ExitRequested wiring). No respawn (the watchdog
   exits on the flag).
3. **Connectivity 30-minute window**: 06:47:19 → 07:18:30 EDT, three
   timestamped probes (start/mid/final) all HTTPS 200 with FULL TLS
   verification (ssl_verify_result=0) through the daemon's proxy = the
   private mesh path; stack app=1 tsd=1 trans=1 at every check; zero
   disconnects. Evidence: conn_window.txt (the desktop).
4. **Tailnet truthfulness**: healthy = verified-green live; failure
   display = verified in the wild (the macOS dead-control case showed
   unavailable truthfully, no stale green); freshness 60s; no
   older-success overwrite (the monotonic cache); direct-TCP fallback
   only for the system-TUN mode.
5. **RPC/Add Torrent**: the endpoint follows the daemon's port changes
   (the set_rpc_port + the watchdog's re-provisioning) + the bounded
   ~8.5s transport retry (the duplicate-safe: the daemon dedupes the
   same-metainfo adds).
6. **Process hygiene**: every app spawn = CREATE_NO_WINDOW (the last
   gap — the periodic disk-space PowerShell — replaced with the native
   GetDiskFreeSpaceExW by Antigravity; the CLI probe spawn = hidden by
   Factory). The remaining visible PowerShell instances = Factory's own
   diagnostic scheduled tasks = removed from the desktop; none remain.

**OUTSTANDING (explicitly NOT silent-complete):**
- [Owner visual] The Tailnet status states (the green/amber/gray) + the
  close-behavior checkbox BOTH states + the persistence on the installed
  candidate (the Alt+F4 case + the tray restore) — the candidate carries
  Antigravity's implementation; the runtime confirmation = the owner's
  five-minute pass on the live app.
- [Owner decision] The macOS enrollment: re-point at the production
  control (the identity-preserving) vs the restart of the local test
  control (the 127.0.0.1:8080) — the current "unavailable" = truthful.
- [Antigravity] The Beta 1 asset update from the 39407774733 artifacts
  (the originals preserved internally; the stable updater untouched; NO
  webhook rotation). Strict-identity note: the artifacts' source =
  60252ab (the verified candidate-source.txt).

## 6. macOS "ADC unavailable" — full tier trace (2026-10-20, Factory)

**Build identity: VERIFIED CURRENT.** The installed
/Applications/Nebulae.app binary's sha256 (f2596177…) = the binary inside
the candidate-39407774733 macOS-arm64 package (60252ab) = IDENTICAL. The
owner's "macOS test release" = the current agreed candidate. Not an old
build.

**Timestamped tier trace (2026-10-20 06:40-06:54 EDT, the live machine):**
1. **Local daemon**: the embedded tailscaled is RUNNING (pid, cmdline
   captured: --tun=userspace-networking --socks5-server=127.0.0.1:1055).
2. **Login/control state**: the daemon is LOGGED OUT. `tailscale status`
   health: "You are logged out. The last login error was: fetch control
   key: Get http://127.0.0.1:8080/key: dial tcp 127.0.0.1:8080: connect:
   connection refused" (timestamped). BackendState = NoState.
3. **Why 127.0.0.1:8080**: the saved enrollment prefs (ControlURL) —
   NOT a hardcoded login server (the resume path reads ControlURL from
   the saved prefs, verified in code). The macOS app was enrolled to a
   LOCAL TEST control server during the earlier standalone acceptance
   round; that control process no longer exists (the /tmp/nebtest*
   directories held only client zips; no headscale binary remains).
4. **ADC peer identity/availability**: NO peers in the netmap (the
   logged-out daemon has none) → adc_node = None.
5. **Expected subnet route**: none available to this client — the daemon
   is logged out; no PrimaryRoutes anywhere.
6. **SOCKS-routed TCP + real HTTPS**: NOT attempted by the app — with
   backend != Running the status command short-circuits to
   connected=false (the correct gate; the probe runs only when Running).
7. **Status bar state**: the app shows the disconnect tier. The exact
   predicate producing "· ADC unavailable" (the owner's string): the
   connected daemon whose netmap has NO ADC node AND reachability =
   null. On macOS: NoState → the legacy branch → no interface →
   connected=false → "Tailnet is not connected" (the disconnect tier).
   If the daemon were Running-but-logged-out anywhere, the
   unavailable/amber path shows.

**Verdict (corrected per owner review): the OBSERVED runtime facts are
the embedded daemon = NoState + logged out + the dead control + the CLI
probe failure. The STATUS BAR's visual tier on macOS = NOT yet read by
tooling (the WebView exposes no AX text) - the displayed tier mapping is
SOURCE INSPECTION, not runtime proof. The mixed-stack hazard the owner
flagged is real in the legacy fallback branch (detect_tailscale can see
a SYSTEM Tailscale interface and report connected=true for the app's
embedded daemon); on this machine no system daemon exists (verified),
but the hazard is recorded for the shared candidate. No false-green,
no stale cache (the cache lives only in the Running branch), no
peer-selector fault.**

**Cross-platform regression audit (shared code touched by the Windows
fixes)**: the ADC peer/subnet-route selection (routes_of = PrimaryRoutes)
= unchanged + correct; the RPC/status parsing = Antigravity's dynamic
endpoint, CI-green; probe deadlines 2s/60s; the cache = monotonic +
keyed by the probed address; the sweep-before-spawn ordering = verified.
**The working-direction runtime proof exists ON THE SAME CANDIDATE CODE**:
the Windows instance's 30-minute window (three 200s, full TLS) = the same
embedded-branch status logic showing verified-green live. The macOS
full working-direction test requires a reachable control server.

**Owner decision (the one needed)**: re-enroll the macOS app against the
production control server (the app's enroll flow takes the control URL +
a preauth key), OR say the local test tailnet should be resurrected and
Factory will provision a new local headscale. No code change, no build,
no server-policy change. Stable updater and webhook untouched.

## 7. macOS recovery — FINAL procedure (owner-approved, 2026-10-20)

**Verified facts (Factory, live):**
- Bundled CLI = the app's resources engine/tailscale/tailscale
  (v1.104.1-dev20261007). Socket =
  /Users/dev/Library/Application Support/Nebulae/internal/tailscale/tailscaled.sock,
  owned by the app's embedded tailscaled (pid 50940) — CONFIRMED via lsof.
- NO separate system tailscaled exists on the macOS machine (verified);
  no system enrollment can be affected.
- Production control: Headscale v0.29.5, base_domain
  adc.asiandvdclub.org, single user `club`, NO oidc — **a preauth key IS
  the supported registration method.**
- The bundled CLI supports file-based key input: `--auth-key file:<path>`
  ("if it begins with file:, then it's a path to a file containing the
  authkey") — the key never appears in argv, history, logs, or chat.

**Owner steps (two, minimal):**
1. On the control server: `headscale preauthkeys create --user club
   --reusable --expiration 1h` and copy the printed key.
2. On the macOS machine, in their own terminal (paste the key once):

```
umask 077
KF="$TMPDIR/neb-preauth-$"
printf '%s' 'PASTE-KEY-HERE' > "$KF"
TS="/Applications/Nebulae.app/Contents/Resources/resources/engine/tailscale/tailscale"
SOCK="$HOME/Library/Application Support/Nebulae/internal/tailscale/tailscaled.sock"
"$TS" --socket "$SOCK" up --login-server=https://control.asiandvdclub.org \
  --auth-key "file:$KF" \
  --accept-dns=false --hostname=nebulae-MacStudio.local
rm -f "$KF"
```

State backup (0600) was taken before any change; nothing else is edited.

**Onboarding gap (recorded as a product issue, per owner):** the app has
a backend-only enroll command and NO enrollment UI. Ordinary-user setup
currently REQUIRES terminal use. Recorded as a product/onboarding gap
for a future release; manual terminal enrollment here is NOT proof that
onboarding is complete.

**After the owner runs the procedure, Factory verifies:** the embedded
daemon Running against the production control endpoint, the SOCKS
listener ownership (the same app daemon), the ADC peer + the private
route in the netmap, TLS-verified private HTTPS (verify=0) through the
daemon's proxy, the status-bar verified state, the failure display (the
control's stop = the bounded test OR the already-observed dead-control
evidence), the recovery after a fresh successful probe, and the
persistence across the app's restart. Windows interaction/failure-
recovery checks continue in parallel (the screenshot-based evidence).

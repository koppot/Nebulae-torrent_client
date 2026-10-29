# Nebulae 0.2.4 on Windows x64: evaluation and repair plan

Author: Factory B (Droid), session `droid-environment-f93df990`. Written 2026-10-28.

Source snapshot: branch `factory-b/windows-0.2.4`, based on tag `v0.2.4` =
commit `3fa0d9a4338d2a959078a39aa577a1566d1472a5` (tree `167a7322eb4822357cb6fbe684ca56c8d579526a`).
Nothing in this document has been pushed, tagged or published.

Evidence legend

- **V** verified by a tool run in this session (the tool is named).
- **S** static source reading at the snapshot, with `file:line`.
- **U** uncertain. Only a run on Windows can settle it.

No Windows run has happened. Nothing below is runtime-verified on Windows.
Line numbers are for `3fa0d9a`. Severity: **B** blocker, **H** high, **M** medium, **L** low.

## 1. Summary

1. `v0.2.4` already exists upstream and CI has already built and published a Windows installer from it.
   That installer ships **stock VLC 3.0.23**. It has none of the macOS menu repairs at runtime (section 3).
2. The Rust and TypeScript player logic is platform neutral and already contains the macOS repairs.
   What Windows lacks is the **patched libvlc.dll and Blu-ray plugin** those repairs call into.
   Without them the first Blu-ray Play still shows the First Play footage, and the menu-visible signal is absent.
3. The published installer would not start the Transmission engine on a Windows machine that lacks
   the Visual C++ runtime (P-01, verified), ships no offline WebView2 (P-02), no licences (P-07) and no GeoLite2 data (P-08).
4. Several Windows lifecycle bugs can kill the wrong process, leak the tracker passkey into logs, or hang the
   Tailscale helper (W-01 to W-08).
5. No Windows build or test environment is available to Factory B (section 8). All fixes in section 6
   are therefore checked on Linux only until one is authorised.

## 2. What exists upstream

| Item | State | Evidence |
|---|---|---|
| Tag `v0.2.4` | annotated, commit `3fa0d9a`, tagger time 2026-10-28T22:58:49Z | V `git` |
| Release run `40784658668` (tag push) | **failed**: macOS arm64 leg stopped in VLC `configure` (`Missing libav or FFmpeg. Pass --disable-avcodec`); Windows, Linux and macOS x64 legs passed; `aggregate-release` and `verify-linux-distros` skipped | V `gh run view` |
| GitHub Release `Nebulae v0.2.4` | published, not draft, marked Latest, 2026-10-28T23:04:13Z. Has Windows, Linux and macOS x64 assets. **No `latest.json`, no macOS arm64 assets** (the manifest is written only by `aggregate-release`) | V `gh release view` |
| CI run `40784348854` (push to main) | `lint-and-check` **failed** (clippy `-D warnings`: unused variable `app`, `src-tauri/src/commands/settings.rs:397`, also fails on Windows); `windows-tests` passed | V `gh run view` |

Published Windows assets (not built by Factory B; Factory B did not touch them):

| Asset | Size (bytes) | SHA-256 |
|---|---:|---|
| `Nebulae-Setup-x64.exe` | 108,905,767 | `4211640df87971d17863daf03d3083f0098b3bb0cbe6085c56ad646e6f58d661` (matches the published `.sha256`) |
| `nebulae.exe` | 30,418,432 | `f1d3907729506502d73e34631cb97ce20b880b2d5940a0bbed488e251022a54b` |
| `Nebulae-Setup-x64.exe.sig` | 408 | minisign updater signature |
| `WINDOWS-SIGNING-STATUS.txt` | 283 | states UNSIGNED, provider none (SignPath declined 2026-10-14) |

Audit of that installer (V: hash-verified download, extracted with 7-Zip, `scripts/windows/pe_audit.py`):
926 files, 435 MB unpacked, 483 PE files. Top level: `nebulae.exe`, `uninstall.exe`,
`verify-payload-signature.exe`, `resources/`. Under `resources/`: `vlc` (582 files, 181 MB, stock zip contents),
`jre` (315 files, 144 MB, full Temurin JRE), `engine/transmission` (12 files, 35 MB), `engine/tailscale`
(`tailscale.exe`, `tailscaled.exe`, 41 MB), `geo` and `player` (each only a `.gitkeep`), `build-manifest.json`.

## 3. Mac-fix parity checklist

Status words: **present** (platform-neutral code is already in the tree), **port** (Windows work needed),
**blocked** (needs the patched Windows runtime or a decision), **N/A** (platform-specific to macOS),
**nowhere** (not implemented on any platform at `3fa0d9a`).

Where the stock Windows runtime differs from the patched runtime, the "Stock VLC" column is what the
published installer does today; "Patched" is what the repaired runtime of section 5 is expected to do.

| # | Mac repair | Where it lives | Stock VLC on Windows today | Patched runtime | Status |
|---|---|---|---|---|---|
| M-01 | Blu-ray initial Play lands on the authored menu, no FBI/warning flash | `--bluray-menu` option (`media.rs:578-1558`, `engine.rs:1091-1258`); patch `bluray-menu-first` calls `blurayTopMenuCall()` right after `bd_play()` | Open-time menu call absent: First Play footage plays. `navigate_menu` is None, so the code falls back to `set_title(menu)`, which UO masks (`media.rs:855-875`). Possible false "at menu", or 15 s watcher fallback reopen (`media.rs:1039-1076`) | Lands on the menu before the first `bd_read` (macOS-verified only) | **port, blocked** (patched `libvlc.dll` + `liblibbluray_plugin.dll`) |
| M-02 | Manual Menu without replaying First Play | `player_return_to_menu` (`media.rs:1742-1770`), `dispatch_main_menu` (`1788-1813`) | `set_title` only (masked); the reopen path replays First Play | `navigate_menu` goes straight to the top menu | **port, blocked** |
| M-03 | Menu-visible ground truth through libVLC / ABI / Rust FFI | `libvlc_media_player_get_bluray_menu_open` (`engine.rs:82,195-202,1024`), `menu_watch.rs` | Symbol absent, returns None; the spoofable title-report path plus a 1 s hold is used | Some(true/false) | **blocked**. The Rust FFI is present and optional |
| M-04 | Bounded retries; user action beats the watcher | `menu_watch.rs` (poll 200 ms, fast poll 25 ms for 4 s, 2 s retry grid, 15 s window, 1 s confirm), `session_gen` fences, `open_lock` | Same code | Same code | **present** |
| M-05 | Stable crop | `letterbox.rs` (held crop, trust gate, 2 confirmations), `framing_tick` (`media.rs:2455-2588`) | Logic runs. The window is not refitted (E-05) | Same | **present**; window fit **port** |
| M-06 | Black-frame protection | `letterbox.rs::is_trustworthy` (>=35% lit pixels, abstains on Unknown) | Same. U: whether the D3D11 snapshot is of the cropped or uncropped frame | Same | **present** (U) |
| M-07 | Narrow vertical-bar crop | `MIN_BAR_FRACTION` 0.4% and the 4:3 exception (`letterbox.rs:362-368`); snapshot sampling | Cross-platform | Same | **present**; window fit **port** |
| M-08 | DVD main feature | `feature_lock.rs`, `spawn_feature_lock` (`media.rs:1908-2067`), `player_play_main_feature` | Present. U: the menu title is recognised by the English name "DVD Menu"; if Windows dvdnav names it differently the lock fails closed | Same | **present** (U) |
| M-09 | Pause / resume | `playerStore.ts`, `PlayerOverlay.tsx:298-471`, `player_toggle_pause` | Works | Same | **present** |
| M-10 | No "Play Main Feature" button once the menu has rendered | `PlayerOverlay.tsx:1360-1372` renders it whenever `player.isDisc` | Button always shown | Same unless implemented | **nowhere** (mentioned only in the repair ledger, section 13.3) |
| M-11 | Sequential multi-disc continuation | only manual disc switching (`PlayerOverlay.tsx:1308-1331`) | n/a | n/a | **nowhere** |
| M-12 | Branding, Tailnet and dependency fixes | `d60b25d` branding; `646f9c7` Tailnet ADC-offline; `2c35dce` release builds load only the bundled libbluray (macOS); macOS packaging scripts | Branding and Tailnet fix are in the shared code. The libbluray rule and the packaging scripts are macOS-only. Windows needs its own dependency staging (sections 5 and 6) | n/a | **present** (shared); Windows dependency work **port** |
| M-13 | ASI-14 menu-bar speed readout | macOS menu-bar title; package `asi14-menu-bar-speed-package.zip` | Windows tray icons have a tooltip, no title text | n/a | **N/A** on Windows; a tooltip variant is possible and not included |

## 4. Findings

### 4.1 Packaging and installer (P)

| ID | Sev | Ev. | Finding |
|---|---|---|---|
| P-01 | B | V | `transmission-daemon.exe` imports `MSVCP140`, `VCRUNTIME140`, `VCRUNTIME140_1`. The only copies are in `resources/jre/bin/`, which Windows does not search when the daemon starts from `resources/engine/transmission/`. A machine without the Visual C++ runtime cannot start the engine. `nebulae.exe` itself imports only system DLLs and UCRT API sets (static VC runtime). |
| P-02 | H | S | WebView2 mode is the default `downloadBootstrapper`. A machine without WebView2 and without internet cannot install. |
| P-03 | B | V | The Windows VLC is the stock 3.0.23 zip. `libvlc.dll` has 316 exports, has `libvlc_media_player_navigate`, and **lacks** `libvlc_media_player_navigate_menu` and `libvlc_media_player_get_bluray_menu_open`. Its Blu-ray plugin is libbluray 1.4.0 (static, with libxml2 and FreeType) plus the matching 1.4.0 jars in `plugins/access/`. |
| P-04 | H | S | Inputs are not pinned or are inconsistent. `release.yml:344-354` downloads VLC 3.0.23 with no checksum; `candidate-build.yml`, `build-test.yml`, `build-windows-updater-test.yml` use 3.0.21; `bundle-vlc-windows.ps1` carries a placeholder SHA that is never checked. Temurin comes from the Adoptium "latest 21" API with no hash (`release.yml:177-255`). Transmission 4.1.3 MSI is SHA-pinned but an optional source build exists whose `git apply ... 2>$null` hides a failed patch. The Windows build job calls `Add-MpPreference` to add a Defender exclusion. |
| P-05 | M | V | Baggage shipped: `verify-payload-signature.exe` (a CI tool, a stray `[[bin]]` in `Cargo.toml`); about 28 MB of Qt6/dbus DLLs next to the daemon (the daemon imports only `libcrypto`, `libcurl`, `libssl` and the VC runtime); VLC `msi/`, `skins/` and similar. |
| P-06 | M | V | `resources/build-manifest.json` is a stale macOS arm64 manifest (git revision `1e4c924`) from `resources/**/*`. |
| P-07 | H | V | No licence or notice files for VLC, libbluray, Transmission, Temurin, Tailscale, VC++ runtime. |
| P-08 | B | V | `resources/geo` holds only `.gitkeep`. GeoLite2-Country.mmdb and GeoLite2-ASN.mmdb are now REQUIRED (not optional) for 0.2.4. The databases are NOT in the repository (`.gitignore` excludes `src-tauri/resources/geo/*.mmdb`) and `scripts/download_geo_db.sh` requires a MaxMind license key that Factory B does not hold. Release blocker: no authorized source for the database; cannot stage, verify, or ship it. |
| P-09 | H | S | NSIS: `src-tauri/nsis/installer.nsi` is orphaned (no `nsis.template` in `tauri.conf.json`); only `hooks.nsh` is wired (installer hooks). In `hooks.nsh`: `netsh` firewall rules fail silently when not elevated (install mode is `currentUser`); magnet, `.torrent` and `.adc` handlers are deleted unconditionally, also during updates and including other applications' registrations; association writes duplicate the bundler's; `taskkill /F /T /IM` kills by image name and hits unrelated Transmission or Tailscale processes; the "delete data" checkbox targets `org.asiandvdclub.nebulae`, not the real `%APPDATA%\Nebulae\internal`; no x64 guard. |
| P-10 | M | S | CI gaps: no Windows install, launch or PE smoke test; Tauri CLI floats (`^2`); no Rust toolchain pin; `targets: "all"` also builds an unused WiX MSI. `ci.yml` `windows-tests` passed this time. |
| P-11 | M | S | Signing and updater. Authenticode runs through SignPath only when `vars.WINDOWS_SIGN_PROVIDER` is set (it is not: unsigned; the app does not verify Authenticode on update). The minisign updater key (`TAURI_SIGNING_PRIVATE_KEY`) is always required because `createUpdaterArtifacts` is true. `generate-updater-manifest.py` is used only by `build-test.yml` and checks for `setup.exe`, not `Nebulae-Setup-x64.exe`. `release.yml` `workflow_dispatch` defaults `tag_name` to `v0.9.0`, a feed hazard. |
| P-12 | U | S | `engine/tailscale` has no `wintun.dll`. Fine if the daemon runs in userspace-networking mode; check in a Windows run. |

### 4.2 Runtime and lifecycle (W)

All S unless stated. Fix direction in brackets.

| ID | Sev | Finding |
|---|---|---|
| W-01 | H | Engine start, AppPaths, DB, the Tailscale thread and `cleanup_orphan_daemon` run before the single-instance plugin (`lib.rs:96-340`). A second launch can kill the first launch's daemon. [take a named mutex at the top of `run()`, or move this work into `.setup()`; `webview_recovery` also runs first] |
| W-02 | H | The pid-file kill is `taskkill /F /PID` with no image check (`engine/transmission.rs:213-292`). A reused pid kills an unrelated process. [add `/FI "IMAGENAME eq transmission-daemon.exe"`] |
| W-03 | M | Piped stdout of the Tailscale CLI is not drained (`tailscale/mod.rs:766-812`, `network.rs:217-250`); a full pipe stalls the child. [drain on reader threads] |
| W-04 | M | The PowerShell process scan builds an unescaped `-like` pattern (`tailscale/mod.rs:365-379`); the Transmission scan escapes only `'`. [use `IndexOf`, double the quotes] |
| W-05 | M | The 3 s quit watchdog (`diagnostics.rs:164-182`) can cut the Tailscale teardown short. |
| W-06 | M | Session close is followed at once by a kill (`transmission.rs:1727`, `1856`). |
| W-07 | M | Readiness waits only 5 s (`transmission.rs:1381-1403`); cold Windows starts with antivirus can exceed it. |
| W-08 | H | The tracker passkey feed URL is logged at info (`engine/enforcement.rs:95`) and the reqwest error at `:116` can carry it. [redact; never log URLs with the key] |
| W-09 | M | The tracing file layer bypasses the sanitiser and has no rotation (`logger.rs`). |
| W-10 | M | The passkey is also stored in SQLite (`tracker.rs:124`). |
| W-11 | M | The second-instance handler drops `adc:` and `.adc` arguments that the first-instance path handles (`lib.rs:352-357` vs `303-310`). |
| W-12 | M | The legacy Tailscale status and `heal_stopped_tailscale` may use a system `tailscale` (`network.rs:204-349`, `connectivity.rs:486-520`). [compile out on Windows] |
| W-13 | M | Silent exit when DB or paths initialisation fails (`lib.rs:107-128`). [show a dialog and log] |
| W-14 | L | The Tailscale thread starts before tracing is initialised (`lib.rs:97` vs `119`). |
| W-15 | M | `settings.json` is written non-atomically (`transmission.rs:1166`, `1273`). [write temp then rename] |
| W-16 | M | Application data sits under Roaming (`paths.rs:20`); large databases roam in domain setups. [`data_local_dir` plus a migration] |
| W-17 | M | The Tailscale mutex is held across probes (`tailscale/mod.rs:590-702`); `status()` blocks inside an async command (`connectivity.rs:437`, `commands/tailscale.rs:27`). |
| W-18 | M | Fixed ports 1055 and 1056; no Windows Job Object, so a crash of the app leaves the daemon running. |
| W-19 | H | A hard-coded TMDB key sits in `utils/tmdb.rs:20`, and reqwest errors can include `api_key=` in logs. (The value is not reproduced here.) |
| W-20 | M | A `[diag]` `println!` and the `NEBULAE_TEST_CDP` remote-debug hook remain in release code (`lib.rs:420-443`). |
| W-21 | L | `/tmp/nebulae-vlc-cb.log` shim (`engine.rs:701`) is env-gated, but `engine.rs:684` has an `.expect`. |
| W-22 | L | `open_torrent_folder` treats an `explorer.exe` exit code of 1 as failure (`commands/torrent.rs:1377-1399`); Explorer returns 1 on success. |
| W-23 | L | No `WM_ENDSESSION` handling: logoff or shutdown is not a clean quit. |
| W-24 | L | `apply_autostart` runs `reg delete` on every boot (`lib.rs:402`, `commands/settings.rs:486`). |
| W-25 | M | Windows clippy `unused variable app` at `commands/settings.rs:397`. It breaks CI lint now. [`_app`, or a cfg-gated use] |
| W-26 | L | Bare `PATH` lookups for helper tools remain (console flashes are fixed). |

Refuted or already fixed: `library_cleaner.rs:362` unwrap is guarded; console-window flashes are fixed;
`peer_answers_ping` and `probe_adc_link` use the Windows named pipe correctly;
the `racing_override` migration is idempotent (a second launch still migrates before the gate).

### 4.3 Player and window embedding (E)

| ID | Sev | Ev. | Finding |
|---|---|---|---|
| E-01 | H | S | Minimize then restore leaves the video window hidden: `main_wndproc` hides it on `IsIconic` (`embed_windows.rs:606-611`), and the restore branch (`612-624`) uses `SWP_NOACTIVATE` without `SWP_SHOWWINDOW`. |
| E-02 | M | S | Minimize (`minimize_to_tray` defaults to true, `embed_windows.rs:790-830`) and close-to-tray (`lib.rs:612-637`) hide only the main window. The video is an unowned top-level `WS_POPUP` (`embed_windows.rs:201-219`) and stays on screen while audio plays on. [handle `WM_SHOWWINDOW`, or make main the owner] |
| E-03 | M | U | Z-order and always-on-top: `player_set_always_on_top` (`media.rs:4103`) changes main only, so the video may not follow. |
| E-04 | M | S | Every reopen runs a hide, restore-rect and re-show cycle (`stop_session` then `start_playback_inner`): automatic fallback, Play Main Feature, Menu from a bypass session and disc switching. A likely remaining flash source even with a patched VLC. [`keep_overlay` flag for open-after-stop] |
| E-05 | M | S | `fit_player_window_to_aspect` is a no-op (`embed_windows.rs:868-870`), so a cropped picture keeps its bars; the 16:9 reshape (`351-362`) skips fullscreen but not maximised windows. |
| E-06 | M | S | `leave_fullscreen_if_active` and `update_blackout_state` are no-ops (`embed_windows.rs:738-739`): closing the player leaves a fullscreen window fullscreen, and the saved blackout setting does nothing on Windows. [implement, or hide the toggle] |
| E-07 | M | S | `engine.rs` `bind()` panics when a required symbol is missing (`961-968`, `987-998`); a panic under a mutex poisons the player state. `embed_windows.rs:77` calls `var_set_checked` directly. [return `Result`, make `var_set_checked` optional] |
| E-08 | M | S | ISO chapter marks in `player_disc_chapters` shell out to `hdiutil` without a cfg gate (`media.rs:2949-3001`). |
| E-09 | M | S | `bluray_meta.rs` probes `libbluray.dll` (`:127-132`). The stock runtime has libbluray inside the plugin; the repaired runtime must either ship a standalone DLL under a name the code probes or rely on the native MPLS reader. |
| E-10 | L | S | The asset scope lists `/Volumes`, `/tmp`, `/private/tmp`; on Windows only `$HOME` applies (see F-06). |
| E-11 | L | S | EDR (`embed_windows.rs:61-97`) sets `var_SetChecked` values; the parity matrix says D3D consumption is unproven. Not one of the player fixes. |

Not applicable on Windows: the `vt-305` (VideoToolbox) and `hdr-tonemap` (caopengllayer, glconv_cvpx) patches.

### 4.4 Frontend and IPC contract (F)

| ID | Sev | Finding |
|---|---|---|
| F-01 | H | `get_user_settings` (`SplashScreen.tsx:42`) and `inspect_torrent_source` (`CreateTorrentModal.tsx:39`) have no backend command. |
| F-02 | L | `player://screen-changed` is emitted (`embed_windows.rs:641`) and never listened for. |
| F-03 | L | `player_disc_chapters`, `player_disc_display_title`, `player_always_on_top` are registered twice (`lib.rs:764-777`). `player_menu_navigate_menu` (`media.rs:1783`) is not registered. |
| F-04 | H | Path logic assumes `/`: `App.tsx:54-55`, `playerStore.ts:44-49` and `179-189` (case-sensitive), `${save_path}/${name}`, `AddTorrentModal.tsx:261` and `:454`; resume keys are not normalised (`playerStore.ts:287-340`). |
| F-05 | M | The speed total sums the since-start averages (`torrentStore.ts:68-99` to `StatusBar.tsx:89-90`); it should sum `current_*_rate ?? *_rate`. Factory A owns StatusBar, torrentStore and ASI-6. |
| F-06 | M | The asset scope is POSIX-only, so posters and thumbnails outside `$HOME` do not load on Windows. |
| F-07 | M | `settingsStore.ts:183-200` overwrites settings after a failed load; overlapping saves; poll and telemetry races; three drop handlers; a listen/unlisten race. |
| F-08 | L | `console.log` at `PlayerOverlay.tsx:1042`. |
| F-09 | M | Fonts load from Google Fonts and the CSP is null: remote requests, and no fonts offline. |
| F-10 | L | `HelpModal` has macOS-only install text. |
| F-11 | U | `SettingsModal` `get_disc_player_status` behaviour on Windows. |

Tests that exist: `tests/playerSessionFence.test.ts` (`node --test`), about 45 Rust test modules (CI runs `cargo test`).
There is no `test` script in `package.json`.

## 5. Target Windows runtime (what the repaired installer must contain)

```
{app}\nebulae.exe
{app}\resources\vlc\                 official VLC 3.0.23 win64 zip (pinned, hash-verified), minus msi/ skins/ docs
{app}\resources\vlc\libvlc.dll       built from VLC 3.0.23 + navigate-menu, bluray-playlist-id, bluray-menu-first
{app}\resources\vlc\plugins\access\liblibbluray_plugin.dll   same patched source, libbluray 1.5.0 linked statically
{app}\resources\vlc\plugins\access\libbluray-j2se-1.5.0.jar, libbluray-awt-j2se-1.5.0.jar   (replace the 1.4.0 pair)
{app}\resources\jre\                 Temurin JRE 21.0.12+8 (pinned), found through JAVA_HOME set in lib.rs:29-61
{app}\resources\engine\transmission\ daemon + libcrypto/libssl/libcurl/zlib from the pinned MSI + app-local VC++ runtime DLLs
{app}\resources\engine\tailscale\    tailscale.exe, tailscaled.exe from the pinned MSI
{app}\resources\geo\                 GeoLite2 data (licence terms to be confirmed)
{app}\resources\licenses\            notices for every bundled component
```

Why a hybrid: `libvlc_media_player_get_bluray_menu_open` reads an input-thread variable that no public API reaches,
so the libVLC entry points themselves must be patched. Rust cannot reach them from `libvlccore.dll` alone.
The three needed patches live in `patches/vlc-3.0.23/` (`vt-305` and `hdr-tonemap` are macOS only).

Build facts established from the VLC 3.0.23 sources (V, read from the verified tarball):

- `lib/Makefile.am` links `libvlc` with `-export-symbols $(srcdir)/libvlc.sym` on every platform, so the patch that
  adds the three functions to `lib/libvlc.sym` also exports them from `libvlc.dll` on mingw.
- The stock Blu-ray plugin is libbluray 1.4.0 with libxml2 and FreeType linked in (about 2.0 MB), loads
  `libbluray-j2se-1.4.0.jar` from its own directory and finds Java through `JAVA_HOME`.
- A Windows build therefore needs libbluray 1.5.0 (jars of the same version; libbluray refuses jars of another version),
  libxml2 and FreeType for mingw, and a mingw-w64 toolchain. VLC's own contrib system builds these.
- The stock `libvlccore.dll` stays. The patched `libvlc.dll` links by name to it, as on macOS.

Inputs are pinned in `scripts/windows/inputs.lock.json` (once added) and verified by the fetch script before use.

## 6. Repair plan

Work is on `factory-b/windows-0.2.4`, in focused commits, nothing pushed. Status is as of the commit that last
touched this file; `git log -- docs/windows` shows it.

| Area | IDs | Planned change | Status |
|---|---|---|---|
| Audit tooling | P-01, P-05 | `scripts/windows/pe_audit.py` with unit tests | **done** (41 tests pass on Linux) |
| Input pinning and fetch | P-04, P-02, P-07 | lock file, fetch/verify script, licence set | **done** (13 inputs pinned by sha256 with provenance; fetch verifies size+hash) |
| Staging and packaging config | P-01, P-05, P-06, P-08, P-09, P-10 | stage script with an allow-list, app-local VC++ runtime next to the daemon, offline WebView2, no CI-only binaries, NSIS hook fixes | **mostly done**: staging run passes the audit (917 files, zero unresolved imports, zero vcredist); hooks.nsh rewritten (Restart-Manager-scoped sidecar stop, elevation-gated firewall rules, ownership-checked handler/rule removal, /UPDATE guard, real data dirs, x64 guard) and syntax-clean under makensis 3.11; `webviewInstallMode` is now `offlineInstaller`; the CI-only bin is gated behind `ci-verify-payload-signature`. `stage_runtime.py` now has `--geo-dir`/`--require-geo` so a release build fails loudly without both GeoLite2 databases. Remaining: GeoLite2 staging is BLOCKED (P-08 — Factory B has no MaxMind licence key), pinned WebView2 preseed in the candidate workflow, CI pins (P-10) |
| Windows runtime fixes | W-02, W-03, W-04, W-12, W-15, W-22, W-25 | taskkill image check; Tailscale CLI output drained on reader threads; PowerShell -like wildcards escaped; heal_stopped_tailscale uses embedded CLI (not system PATH); settings.json atomic write; Explorer exit code 1 treated as success; `_app` clippy fix | **done** (cargo check windows-gnu passes; clippy `unused variable app` gone) |
| Windows runtime fixes | W-01, W-05..W-11, W-13..W-24, W-26 | remaining lifecycle, logging, path, mutex and port fixes as listed | open |
| Player/window fixes | E-01 to E-09 | as listed | open |
| IPC and path fixes | F-01 to F-07 | missing commands, path helper with tests, asset scope | open |
| Patched VLC for Windows | M-01 to M-03, P-03 | mingw build recipe and a CI job | open, **blocked on a build environment** |
| Candidate workflow | P-10 | `windows-candidate.yml`: pinned runner, `--bundles nsis`, PE audit, silent install, launch and uninstall smoke test, artifacts only | open, **blocked on authorisation to push** |
| Mac features missing everywhere | M-10, M-11 | wait for Factory A's design, then adopt | open |

Installer license page: the NSIS template shows one only when `bundle.license` is set; it is unset today and stays
unset from this lane, because that key is global (it would also change the Linux packages and any future WiX MSI) and
adds an obligatory click for every installer. Licence coverage is instead shipped as files under
`resources/licenses/` (P-07 fix) plus the in-repo `THIRD-PARTY-NOTICES.md`. Turning those into a license page is a
cross-platform product decision for the maintainers.

### GeoLite2 release check (REQUIRED, blocked)

GeoLite2 is a required dependency for Nebulae 0.2.4, not optional. The release checks are:

1. **Edition / build identity:** `GeoLite2-Country.mmdb` and `GeoLite2-ASN.mmdb`, current weekly build from MaxMind (edition IDs `GeoLite2-Country` and `GeoLite2-ASN`). Build/date identity is whatever MaxMind serves at download time; record it in the build manifest.
2. **Authorized source:** MaxMind's download service (`download.maxmind.com`), via `scripts/download_geo_db.sh`, which requires a MaxMind account signup and a license key. GeoLite2 is free but the key is mandatory — no key, no download.
3. **Packaged location:** `{app}\resources\geo\GeoLite2-Country.mmdb` and `{app}\resources\geo\GeoLite2-ASN.mmdb` in the installed tree (code tries `<resource dir>/resources/geo/` first, then `<resource dir>/geo/`).
4. **Integrity verification:** `stage_runtime.py` stages both files (from `--geo-dir`) and records their SHA-256 in `resources/build-manifest.json`; `pe_audit` covers the rest of the tree. The `.mmdb` files themselves are validated by opening them with the `maxminddb` crate (the engine logs `[geo] GeoLite2 resolver loaded` on success).
5. **Lookup verification (blocked):** with a real database, open `GeoResolver::from_resource_dir` and run representative lookups — e.g. `country_code("8.8.8.8")` → `Some("US")`, `asn("8.8.8.8")` → `Some(15169)`, and a private IP → `LocalityTier::Lan`. This cannot run without the database.
6. **Release gate:** `stage_runtime.py --require-geo` fails the staging run when either database is absent. Missing, unreadable, invalid, or incorrectly located database files block release.

**Blocker:** Factory B has no MaxMind license key and no authorized source for the databases. `.gitignore` excludes `src-tauri/resources/geo/*.mmdb` and they are not in `inputs.lock.json`. The download script, the notice text, the staging slots and the release gate are all in place; only the credentials are missing. Do not substitute another product and do not silently omit the database.

## 7. Verification status

Done (Linux):

- Hash-verified downloads of every pinned input; the published installer extracted and audited (section 2).
- `libvlc.dll` of the stock zip compared with the symbols the Rust code needs (316 exports; the two custom ones missing).
- `pe_audit.py` cross-checked against `pefile` on 369 PE files with no mismatch, and covered by synthetic-PE unit tests (41 tests in `scripts/windows/tests` pass).
- VLC 3.0.23 source read for the export mechanism (section 5).
- Full stock staging run audited: 917 files, 395 MB, 483 PE files, every import resolves, no vcredist dependency left.
- `cargo check` and a `--release` build of `nebulae.exe` for `x86_64-pc-windows-gnu` both pass (warnings unchanged: `embed_windows.rs:24`, `settings.rs:397`, `association.rs:18`). The GNU exe imports only system DLLs plus `WebView2Loader.dll` (dynamic); the bundler ships that DLL for `-gnu` targets when it sits next to the exe.
- NSIS hooks compile under makensis 3.11 (the version tauri-bundler 2.10.2 uses) via `scripts/windows/tests/nsis/hooks-harness.nsi`; the exercise caught and fixed one real bug (`${StrLoc}` is not callable from uninstall sections; the hooks generate and use `un.StrLoc`).

Not done (nothing of this exists yet):

- Any Windows run of the exe, any installer build or install, any playback, any patched-VLC build, any MSVC build (the release route).
- Whether the repaired runtime fixes the Blu-ray First Play flash on Windows. It is unproven even on macOS in the repair ledger.
- `clippy -- -D warnings` is still red (`settings.rs:397` unused `app`, plus the two older warnings); fixed with the runtime repairs.

## 8. Access needed to finish

Factory B has no Windows machine and no hosted CI access of its own. To build and test the installer, one of:

1. **A Windows 10/11 x64 machine or VM registered to this Factory account (Remote Control)**, with administrator
   rights, 40 GB free and internet access, plus a second clean VM or snapshot for the clean-environment install test.
2. **Explicit permission to push one scratch branch** (for example `factory-b/windows-0.2.4-candidate`, no tags,
   no release, no change to `main` or to existing workflows) to `koppot/Nebulae-torrent_client` and to run one new,
   artifacts-only workflow on GitHub-hosted Windows and Ubuntu runners. This gives a real MSVC build, NSIS installer,
   PE audit and an install/launch/uninstall smoke test. It cannot prove behaviour on a machine without the Visual C++
   runtime, because hosted images have it installed.

Not available to Factory B and not requested: the minisign updater private key, an Authenticode certificate,
publishing rights. Factory B will not generate replacement keys. The candidate build is unsigned and has no updater
artifacts.

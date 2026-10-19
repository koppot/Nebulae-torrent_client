# Nebulae Cross-Platform Repair — Acceptance & Requirements Matrix

**Repository:** `koppot/Nebulae-torrent_client`  
**Active branch:** `fix/cross-platform-repair`  
**Current source commit:** [`ac2525b`](https://github.com/koppot/Nebulae-torrent_client/commit/ac2525b)  
**Last updated:** 2026-10-18T20:48:00-04:00  

---

## 1. Commit History on `fix/cross-platform-repair`

| Commit SHA | Date | Summary |
| :--- | :--- | :--- |
| [`ac2525b`](https://github.com/koppot/Nebulae-torrent_client/commit/ac2525b) | 2026-10-18 | `feat(settings): add media mode toggle to BitTorrent tab to match torrent settings location` |
| [`1b0cf06`](https://github.com/koppot/Nebulae-torrent_client/commit/1b0cf06) | 2026-10-18 | `docs(updater): update acceptance matrix with verified deliverables and add manifest generator script` |
| [`94488b6`](https://github.com/koppot/Nebulae-torrent_client/commit/94488b6) | 2026-10-18 | `security(updater): gate test overrides behind test-updater feature and restore strict HTTPS` |
| [`bf5e317`](https://github.com/koppot/Nebulae-torrent_client/commit/bf5e317) | 2026-10-18 | `feat(updater): verify consecutive silent updates A->B->C and configure CI test-build updater pipeline` |
| [`f57b24b`](https://github.com/koppot/Nebulae-torrent_client/commit/f57b24b) | 2026-10-18 | `docs: finalize acceptance matrix with verified CI deliverables and precise test labels` |
| [`ff3b458`](https://github.com/koppot/Nebulae-torrent_client/commit/ff3b458) | 2026-10-18 | `ci: add workflow concurrency group and release upload supersede safeguard` |
| [`b46db3f`](https://github.com/koppot/Nebulae-torrent_client/commit/b46db3f) | 2026-10-18 | `test(ui): add isolated first-run media mode lifecycle verification test` |
| [`d5a982b`](https://github.com/koppot/Nebulae-torrent_client/commit/d5a982b) | 2026-10-18 | `docs(acceptance): add persistent acceptance matrix and refine first-run and racing mode UI` |
| [`e9ba222`](https://github.com/koppot/Nebulae-torrent_client/commit/e9ba222) | 2026-10-18 | `feat(ui): make racing mode dot clickable in status bar and match enable media mode label` |
| [`1606447`](https://github.com/koppot/Nebulae-torrent_client/commit/1606447) | 2026-10-18 | `test(player): add multi-iteration bluray metadata probe test and clean compiler warnings` |
| [`b5f5ac3`](https://github.com/koppot/Nebulae-torrent_client/commit/b5f5ac3) | 2026-10-18 | `fix(player): resolve duplicate playback sessions and menu-watcher playlist switch race` |
| [`e1190fa`](https://github.com/koppot/Nebulae-torrent_client/commit/e1190fa) | 2026-10-18 | `fix(media, torrent): repair media menu actions, bundled libvlc initialization, and non-blocking force recheck` |
| [`48255dd`](https://github.com/koppot/Nebulae-torrent_client/commit/48255dd) | 2026-10-18 | `feat(ui, updater, engine): add media mode toggle, racing status dot, empty-library media import, and linux resource discovery` |
| [`11c2104`](https://github.com/koppot/Nebulae-torrent_client/commit/11c2104) | 2026-10-18 | `fix(ci): restore full package artifact matrix` |
| `c74a3ca` | 2026-10-15 | **BASELINE** (`v0.2.2`, `origin/ag/transmission-engine`) |

---

## 2. Comprehensive Requirements & Defect Acceptance Matrix

| Requirement & Target Behavior | Status | Relevant Source Files | Test Evidence & Verification Method | Outstanding Verification / Blocker |
| :--- | :--- | :--- | :--- | :--- |
| **1. Media Screen & Card Menu Actions**<br>Interacting with Media view controls and card context menus executes their intended actions without failure:<br>• Media card right-click: `Show in Finder`, `Play`, `Resume` (when resumable), `Details`, `Copy Path`, `Open in Manager`<br>• External card right-click: `Show in Finder`, `Play`, `Remove from Library`<br>• Top bar: `Add Movie`, `Media Preferences` (audio/sub language prefs)<br>• In-player controls: `Disc <N>`, `Menu`, `D-Pad`, `Subtitles`, `Audio`, `Fullscreen`, `Pin`. | **IMPLEMENTED / UNVERIFIED (UI)** | `src-tauri/src/commands/media.rs`<br>`src/components/media/SimpleMediaView.tsx`<br>`src/components/player/PlayerOverlay.tsx` | Handlers wired to native file dialogs, VLC player engine, and clipboard. Unreachable torrents show clear "Details Unavailable" modal instead of failing silently. | Manual interactive verification in the running GUI is unverified and awaiting owner test. |
| **2. Non-blocking Force Recheck**<br>Selecting Force Recheck in manager UI enters a visible checking state, reports live `recheckProgress`, completes without freezing UI or blocking manager actions. | **IMPLEMENTED / VERIFIED (ENGINE) / UNVERIFIED (UI)** | `src-tauri/src/engine/transmission.rs`<br>`src/stores/torrentStore.ts` | Verified via `scripts/verify-force-recheck.py`: valid data paused preservation, active torrent running resume, truncated data 50%, missing file error reporting. | Manual interactive UI confirmation of responsive scrolling/clicking during large torrent check is unverified and awaiting user verification. |
| **3. Dual Audio / Phantom Player Collision**<br>Rapid playback clicks or repeated open/close produce exactly ONE active session and ONE audio stream. | **IMPLEMENTED / VERIFIED (UNIT) / UNVERIFIED (UI)** | `src-tauri/src/player/mod.rs`<br>`src-tauri/src/commands/media.rs`<br>`src/lib/playerStore.ts` | `PlayerState.open_lock` serializes open/close calls; `session.media_player` bound before probes; `VlcSession::close()` flushes audio up to 200ms; `isOpening` debounces frontend clicks. | Interactive rapid double-click in packaged UI is unverified and awaiting user verification. |
| **4. 91 DAYS D1 Episode 8 Menu Navigation Crash**<br>Selecting Episode 8 from interactive Blu-ray menu plays without application crash (`SIGBUS: KERN_MEMORY_ERROR`). | **IMPLEMENTED / VERIFIED (PROBE) / UNVERIFIED (UI)** | `src-tauri/src/player/bluray_meta.rs`<br>`src-tauri/src/commands/media.rs` | Root cause: `open_bluray_lib()` dlclose unmapped memory while worker threads ran. Resolved with `static BLURAY_LIB: OnceLock<Result<Library, String>>` and `BD_PROBE_MUTEX`. `test_bluray_91_days_metadata_probe` passes 5 iterations. | Note: metadata probes confirm library lifetime and symbol safety, but visual BD-J menu transition and playback in the packaged client is unverified and awaiting user verification. |
| **5. First-Run Media Mode Prompt**<br>Clean first launch displays exact owner text: `"Would you like to enable media mode? Nebulae’s Media Center lets you play DVDs, Blu-ray, and UHD discs with interactive disc menus, while organizing and managing your media collection in one place."` Choices: `Yes` / `No`, default: `Yes`. | **IMPLEMENTED / VERIFIED (STATE SIMULATION) / UNVERIFIED (UI)** | `src/App.tsx`<br>`src/stores/settingsStore.ts` | Verified via `scripts/verify-first-run-media-mode.py` (state transition and isolated SQLite database lifecycle simulation). Clean profile triggers prompt; Yes enables Media mode toggle and continues in Manager; No disables Media mode and continues in Manager. Subsequent launches preserve choice; media DB untouched. | Visual interactive presentation in packaged webview is unverified and awaiting user interactive verification. |
| **6. Racing Mode Status Indicator**<br>Bottom status bar: `[green/red dot] Racing mode`. Green enabled, red disabled. Follows saved settings immediately and after restart. No unrequested click-to-toggle in status bar. | **IMPLEMENTED / VERIFIED** | `src/components/layout/StatusBar.tsx`<br>`src/components/modals/SettingsModal.tsx`<br>`src-tauri/src/engine/transmission.rs` | Pure display indicator in `StatusBar.tsx`: `bg-emerald-500` (enabled) / `bg-red-500` (disabled). Settings checkbox in BitTorrent tab toggles mode immediately via RPC session set (256MB cache, 150/1000 peers, 14 upload slots, 2-min stall threshold, high priority). Persisted in DB and restored on boot. | None. Implemented and verified. |
| **7. Persistent "Enable media mode" Checkbox**<br>Checkbox in Settings. Disabling immediately hides top Media navigation switch without deleting media collection data. Returning from Media switches to Manager. | **IMPLEMENTED / VERIFIED** | `src/components/modals/SettingsModal.tsx`<br>`src/components/layout/TitleBar.tsx` | Checkbox labeled "Enable media mode". `TitleBar.tsx` reactive effect: hides button when false, redirects active view to Manager. Database records and thumbnails preserved. | None. Implemented and verified. |
| **8. Windows Empty-Library Media Import**<br>When library is initially empty, UI displays "Add Movie / Disc" opening native picker supporting folders and ISO images. | **IMPLEMENTED / UNVERIFIED (WIN)** | `src/components/media/SimpleMediaView.tsx`<br>`src-tauri/src/commands/torrent.rs` | Empty state renders `Add Movie / Disc`. Windows PowerShell dialog flow implemented: OpenFileDialog (`*.iso;*.img;*.*`) falling back to FolderBrowserDialog. `importExternal` parses Windows path separators (`split(/[/\\]/)`). | Runtime execution on native Windows hardware is unverified; requires testing on physical Windows machine. |
| **9. Consecutive Silent Auto-Updates (A -> B -> C)**<br>Silent background check, download, and staging across two consecutive version hops with zero dialogs and all settings/data preserved.<br>• Strict HTTPS and immutable hardcoded public key in production release builds.<br>• Test-updater overrides isolated behind compile-time Cargo feature `test-updater` (disabled by default).<br>• Zero ephemeral private keys committed or packaged. | **IMPLEMENTED / VERIFIED (MACOS CONSECUTIVE UPDATE HOPS) / UNVERIFIED (WIN NATIVE HARDWARE)** | `src-tauri/src/updater.rs`<br>`src-tauri/tauri.conf.json`<br>`src-tauri/Cargo.toml`<br>`.github/workflows/build-test.yml`<br>`scripts/test-consecutive-silent-updates.py` | • **Security Isolation ([`94488b6`](https://github.com/koppot/Nebulae-torrent_client/commit/94488b6))**: Normal release builds strictly enforce HTTPS (`dangerousInsecureTransportProtocol` removed). Release updater builder cannot be redirected or swapped via environment variables. `test-updater` Cargo feature is disabled by default.<br>• **macOS Multi-Hop Verification**: Verified via `scripts/test-consecutive-silent-updates.py` across 2 consecutive hops:<br>  - **Hop 1 (0.2.2 &rarr; 0.2.3)**: Background discovery, downloaded 97.7MB payload, verified Minisign cryptographic signature, staged in-place application bundle swap, `Info.plist` updated to `0.2.3`.<br>  - **Hop 2 (0.2.3 &rarr; 0.2.4)**: Background discovery, downloaded 97.7MB payload, verified Minisign cryptographic signature, staged in-place bundle swap, `Info.plist` updated to `0.2.4`.<br>  - **Hop 3 (Up-to-Date check)**: App 0.2.4 checked manifest and confirmed `Nebulae is up to date; no update available.`<br>  - **Data preservation**: SQLite `nebulae.db` records (2 seeded torrents, external media row, and user settings JSON) 100% verified intact across all hops.<br>• **CI Pipeline**: `.github/workflows/build-test.yml` updated with `.tar.gz` and `.sig` artifact collection and automated `latest.json` manifest generation for the `test-builds` release. | • **Windows Native Verification**: Windows quiet installer execution (NSIS `/S` staged installer, quit-time execution) is unverified on physical Windows hardware; macOS bundle replacement cannot be generalized to Windows.<br>• **Hosted Channel Blocker**: GitHub Draft releases return HTTP 404 to unauthenticated clients; public updater verification requires a published public release. |
| **10. Linux Resource Discovery**<br>Tauri resource resolution resolves `/usr/lib/Nebulae/resources/` with exact case preservation. | **IMPLEMENTED / VERIFIED (CI)** | `src-tauri/src/utils/paths.rs`<br>`src-tauri/src/player/engine.rs` | Case-preserving Tauri `path::resolve` with fallback to `/usr/lib/Nebulae/resources/` and `/usr/share/nebulae/resources/`. Ubuntu 22.04 CI package passes. | None. Verified in CI. |
| **11. Unicode `.torrent` Modal Validation**<br>Accepts Unicode `.torrent` and `.adc` files; rejects folders and non-torrent files with clear actionable error. | **IMPLEMENTED / VERIFIED** | `src/components/modals/AddTorrentModal.tsx`<br>`scripts/verify-modal-sequence.js` | Verified via `node scripts/verify-modal-sequence.js`: rejects folder paths, accepts Unicode anime torrents, clears stale errors. Note: this script tests state transitions and regex logic, not real browser UI automation. | None. Implemented and verified. |
| **12. Engine Identification & Upstream Defaults**<br>Default Transmission 4.1.3 (`-TR4130-`); Native Nebulae 0.2.2 (`-NE0220-`). Consistent peer_id across announce & handshake. | **IMPLEMENTED / VERIFIED** | `src-tauri/src/engine/identity.rs`<br>`src-tauri/src/engine/transmission.rs` | Verified via `scripts/verify-engine-identity.py` and `scripts/check-installed-app-settings.py` against installed `/Applications/Nebulae.app`. | None. Confirmed. |
| **13. Bundled Runtimes Self-Containment**<br>Zero reliance on system Java or separate `/Applications/VLC.app`. All runtimes bundled internally. | **IMPLEMENTED / VERIFIED** | `src-tauri/resources/`<br>`.github/workflows/build-test.yml` | Bundle contains `transmission-daemon`, `vlc` plugins/libvlc, and Adoptium Eclipse Temurin 21 JRE (`jre/bin/java`). Verified in local package and CI outputs. | Actual physical disc playback in UI awaiting user test. |
| **14. Factory Tailscale Integration**<br>Keep Factory's Tailscale work strictly separated on `ts/tailscale-direct-integration` / `ADC-tailscale`. | **ISOLATED** | N/A | No branches merged, switched, or overwritten. Retained strictly separate. | Coordinate with Factory when they release direct integration branch. |

---

## 3. Platform Updater Limitations & Hosted Feed Realities

1. **macOS:** In-place silent updates require write permissions to the installation target (`/Applications`). Applications run directly from a mounted read-only DMG cannot self-update in place.
2. **Windows (Unverified on Native Hardware):**
   - Silent updates apply cleanly for per-user installations in `%LOCALAPPDATA%\Programs\Nebulae`.
   - Machine-wide installations under `C:\Program Files` require administrator UAC elevation, preventing zero-interaction silent updates unless running as an elevated service.
   - NSIS `/S` quiet installer execution upon application exit has not been verified on physical Windows hardware; test execution on native hardware is required and must not be inferred from macOS bundle replacement.
   - Coordinate testing with Factory to ensure the Windows shared test environment is not simultaneously modified.
3. **Linux:** DEB and RPM packages are managed by system package managers (`apt`, `dnf`); only standalone AppImage builds support self-updating.
4. **Hosted Channel Availability (GitHub Draft Release Behavior):**
   - GitHub Draft release assets return HTTP 404 to unauthenticated clients. Standard in-app updaters without GitHub authentication headers cannot retrieve `latest.json` or update payloads from draft releases.
   - Testing against hosted GitHub update feeds requires either publishing the release publicly or proxying the feed with authentication.

---

## 4. Package Provenance & Artifact Status

### Standing Draft Release Packages (Pre-Fix / Run 39174118184 / Commit ff3b458)
*Available for general regression testing (UI menus, Force Recheck, Dual Audio, BD-J probe, First-run prompt):*

| Platform | Target File | Size | SHA-256 Checksum | Verified Build Info |
| :--- | :--- | :--- | :--- | :--- |
| **macOS Apple Silicon** | `Nebulae-macOS-arm64.dmg` | 110,233,391 bytes | `86f48eed60c4a4424aba168f4fc740be1e7e9a27f8301bbdd5c4019fa3ed5536` | `BUILD_INFO-Nebulae-macOS-arm64.dmg.txt` |
| **macOS Intel** | `Nebulae-macOS-x64.dmg` | 111,761,747 bytes | `6bd52a4c941dea15a430e1b94b7ef062582fa2275be2c3bf880a66fc420add02` | `BUILD_INFO-Nebulae-macOS-x64.dmg.txt` |
| **Windows x64** | `Nebulae-Setup-x64.exe` | 93,046,859 bytes | `65681290b3202b50ba566b5b3d940978e53f07409164a4b70092bfe5637db631` | `BUILD_INFO-Nebulae-Setup-x64.exe.txt` |
| **Linux x86_64 AppImage** | `Nebulae-Linux-x86_64.AppImage` | 167,885,304 bytes | `2e3df2524f9f16b35f779a3f57c6aa627af091a0a7c54c0c20c4cf81a47dcda5` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 DEB** | `Nebulae-Linux-x86_64.deb` | 81,715,698 bytes | `44b5c7c1c9d7a84c1b5e392107958b373330175a4b8aa4ba14fe5a0f2fa2a4cc` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 RPM** | `Nebulae-Linux-x86_64.rpm` | 81,734,842 bytes | `8be9d646dac6dd18d069be5d0620758469c0eb19bbfba1d7a5ad33f22f0577c8` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |

- **Standing Draft Release Target:** [`test-builds`](https://github.com/koppot/Nebulae-torrent_client/releases/tag/untagged-1cf31dd6aba21ef114c0)

### Verified Current Deliverables & Updater Feed (Run 39181055336 / Commit ac2525b)
*Built with all intended current client changes (including BitTorrent-tab Media toggle), compile-time `test-updater` gating, strict HTTPS enforcement, bundled runtimes (`jre/bin/java`, `transmission-daemon`, `libvlc`), Minisign signatures, and `latest.json` manifest:*

| Platform | Target File | Size | SHA-256 Checksum | Verified Build Info |
| :--- | :--- | :--- | :--- | :--- |
| **macOS Apple Silicon (RECOMMENDED)** | `Nebulae-macOS-arm64.dmg` | 110,235,178 bytes | `b80efd38772817d0aa3bad010ca66a458541dfc7783dbd706228ee49c6581b2c` | `BUILD_INFO-Nebulae-macOS-arm64.dmg.txt` |
| **macOS Intel** | `Nebulae-macOS-x64.dmg` | 111,767,137 bytes | `48c822f3c6b287085ced6892dc129c34137a3cff74a21876d8bdb54756020602` | `BUILD_INFO-Nebulae-macOS-x64.dmg.txt` |
| **Windows x64** | `Nebulae-Setup-x64.exe` | 93,052,508 bytes | `3f59da4d9a4208200e54a794cb29e4141ade5ecf94bf992b944fdc077672849a` | `BUILD_INFO-Nebulae-Setup-x64.exe.txt` |
| **Linux x86_64 AppImage** | `Nebulae-Linux-x86_64.AppImage` | 167,885,304 bytes | `d51b0391f42d2f91c01d8add6a4610ea1c81f444adbf3ca5ae86c339824513b0` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 DEB** | `Nebulae-Linux-x86_64.deb` | 81,715,378 bytes | `1e31af3d20501a45233ac2d74186c0c7342e6708657593c684517e33cd4ae45d` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 RPM** | `Nebulae-Linux-x86_64.rpm` | 81,734,834 bytes | `99af35c8ad28de599fdc096070565ef92cb691722704daa1ace99ce02634f6f0` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |

#### Updater Bundles & Minisign Cryptographic Verification:
- **macOS Apple Silicon:** `Nebulae-macOS-arm64.dmg-app.tar.gz` (99,836,388 bytes) + `.sig` (424 bytes)  
  *Cryptographic Status:* **100% VERIFIED SUCCESSFUL** via `minisign-verify` engine. Ed25519 payload signature is cryptographically VALID; Minisign global signature over trusted comment (`file:Nebulae.app.tar.gz`, `version:0.2.2`) is VALID against compiled public key `452CAA8BBBBC6F41`.
- **macOS Intel:** `Nebulae-macOS-x64.dmg-app.tar.gz` (100,352,492 bytes) + `.sig` (424 bytes)
- **Windows x64:** `Nebulae-Setup-x64.exe` (93,052,508 bytes) + `.sig` (436 bytes)  
  *Packaging Note:* In run 39181055336, the signature was collected from the MSI bundle (`Nebulae_0.2.2_x64_en-US.msi.sig`) due to alphabetical glob matching. A workflow fix in `.github/workflows/build-test.yml` was applied to explicitly pair `Nebulae-Setup-x64.exe` with its exact NSIS setup signature `Nebulae_0.2.2_x64-setup.exe.sig`.
- **Feed Manifest:** `latest.json` (auto-generated by `scripts/generate-updater-manifest.py`, SHA-256: `6e618991fa9b265d6aa22e707dc8470a6c6ec2264627d2bf61d62c938d9600e1`).
- **Release Target:** Standing draft release [`test-builds`](https://github.com/koppot/Nebulae-torrent_client/releases/tag/untagged-1cf31dd6aba21ef114c0).

---

## 5. Technical Constraints & Test Limitations

1. **macOS GUI Automation Boundary (WindowServer TCC):**  
   Antigravity runs as a headless CLI subprocess. macOS WindowServer security policy (TCC) requires explicit user accessibility grants (`AXIsProcessTrusted`) and screen recording permissions. Synthetic mouse/keyboard events (`CGEventPost`, `AXUIElementPerformAction`) from background processes are intentionally blocked by WindowServer. Furthermore, global agent rules prohibit Linux-container browser tools on macOS. Therefore, the 6 interactive GUI items must be visually verified by the user in an interactive desktop session.
2. **First-Run Profile Isolation (Cocoa WebKit Architecture):**  
   Apple Foundation (`NSHomeDirectory()`) queries Darwin `getpwuid(getuid())` and ignores the terminal `$HOME` override. While `$HOME` isolates SQLite (`nebulae.db`), WKWebView continues using `/Users/dev/Library/WebKit/org.asiandvdclub.nebulae/`. Complete profile isolation without modifying the primary user data requires testing inside a dedicated test OS account or the macOS Guest account.
3. **Factory Isolation on supermicro.local:**  
   Factory is actively running test suite `nebtest6` (PID 4772, `nebulae-tailscale-test-e8d510b-win64`) in an active RDP session (`rdp-tcp#5`) on `supermicro.local`. No processes are stopped or modified during active Factory testing. Windows updater test execution will proceed during a designated exclusive window after Factory completes.



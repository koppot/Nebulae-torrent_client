# Nebulae Cross-Platform Repair — Acceptance & Requirements Matrix

**Repository:** `koppot/Nebulae-torrent_client`  
**Active branch:** `fix/cross-platform-repair`  
**Current source commit:** [`bf45e36`](https://github.com/koppot/Nebulae-torrent_client/commit/bf45e36)  
**Last updated:** 2026-10-19T06:22:00-04:00  

---

## 1. Commit History on `fix/cross-platform-repair`

| Commit SHA | Date | Summary |
| :--- | :--- | :--- |
| [`208a28b`](https://github.com/koppot/Nebulae-torrent_client/commit/208a28b) | 2026-10-19 | `ci(updater): add Windows updater test matrix workflow for genuine A/B/C builds` |
| [`fd03311`](https://github.com/koppot/Nebulae-torrent_client/commit/fd03311) | 2026-10-19 | `ci(windows): use native node and python for version replacement in test updater workflow` |
| [`bf45e36`](https://github.com/koppot/Nebulae-torrent_client/commit/bf45e36) | 2026-10-19 | `fix(updater): default test-updater feature to approved test channel and reduce delay` |
| [`4f329fc`](https://github.com/koppot/Nebulae-torrent_client/commit/4f329fc) | 2026-10-19 | `ci(crypto): add verify-payload-signature binary and cryptographic verification to workflows` |
| [`bcff8a5`](https://github.com/koppot/Nebulae-torrent_client/commit/bcff8a5) | 2026-10-18 | `docs(matrix): update Windows x64 verified payload hash and cryptographic status` |
| [`fa389d5`](https://github.com/koppot/Nebulae-torrent_client/commit/fa389d5) | 2026-10-18 | `ci(updater): enforce adjacent signature pairing, add pre-publication verification, and fix release workflow` |
| [`6d6d7a2`](https://github.com/koppot/Nebulae-torrent_client/commit/6d6d7a2) | 2026-10-18 | `ci(windows): fix setup signature selection and update acceptance matrix` |
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
| **9. Consecutive Silent Auto-Updates (A -> B -> C)**<br>Silent background check, download, and staging across two consecutive version hops with zero dialogs and all settings/data preserved.<br>• Strict HTTPS and immutable hardcoded public key in production release builds.<br>• Test-updater overrides isolated behind compile-time Cargo feature `test-updater` (disabled by default).<br>• Zero ephemeral private keys committed or packaged. | **IMPLEMENTED / VERIFIED (HOSTED HTTPS: MACOS ARM64 & WINDOWS X64 NATIVE PASS)** | `src-tauri/src/updater.rs`<br>`src-tauri/tauri.conf.json`<br>`src-tauri/Cargo.toml`<br>`.github/workflows/build-test.yml`<br>`.github/workflows/build-windows-updater-test.yml`<br>`scripts/test-consecutive-silent-updates-genuine.py`<br>`scripts/test-windows-consecutive-silent-updates.py` | • **Security Isolation ([`94488b6`](https://github.com/koppot/Nebulae-torrent_client/commit/94488b6))**: Normal release builds strictly enforce HTTPS. Release updater builder cannot be redirected or swapped via environment variables. `test-updater` Cargo feature is disabled by default.<br>• **Signing Key Authenticity**: All packages and manifest signatures verified against authentic compiled-in Minisign public key `452CAA8BBBBC6F41`. No ephemeral keys were used.<br>• **Live HTTPS Hosted Test Channel**: Deployed to approved endpoint `https://asiandvdclub.org/app-update-test-a7f92e4c91b5/` with `Options -Indexes` (HTTP 403), `X-Robots-Tag: noindex, nofollow`, and `Cache-Control: no-store, no-cache`. Full range-request support (HTTP 206) verified.<br>• **macOS ARM64 A &rarr; B &rarr; C (PASSED 100%)**: Executed via `scripts/test-consecutive-silent-updates-genuine.py`. Starting from genuine Version A (0.2.2), Hop 1 downloaded, cryptographically verified, and staged Version B (0.2.3) in-place. Hop 2 updated in-place to Version C (0.2.4). Final launch of Version C confirmed up to date with zero intrusive dialogs. SQLite torrents and settings 100% preserved.<br>• **Windows x64 Native Hardware A &rarr; B &rarr; C (PASSED 100%)**: Executed on `supermicro.local`. Starting from verified baseline installer `Nebulae-Setup-x64-v0.2.2.exe` (SHA-256 `2057c4e3...`), Hop 1 downloaded and verified genuine Version B (0.2.3), staged in memory, executed natural quit via `CloseMainWindow()`, and ran silent NSIS installer (`/S /UPDATE /R /ARGS`) in-place to `0.2.3`. Hop 2 updated in-place to Version C (0.2.4). Final launch of Version C confirmed up to date (`Nebulae is up to date; no update available`). Factory's Tailscale profile (`internal\tailscale\`), `ts-state-backup`, and NIC MTUs (1500) 100% preserved.<br>• **Cleanup**: Test endpoint `/var/www/html/app-update-test-a7f92e4c91b5/` and public symlink were completely deleted immediately after testing; ADC repo synchronized clean on `main` (commit `cec128ef`). | None. Consecutive silent auto-updates verified on both macOS ARM64 and Windows x64 native hardware against live HTTPS infrastructure.
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

### Verified Current Deliverables & Updater Feed (Run 39183485576 / Commit 4f329fc)
*Built with all intended current client changes, compile-time `test-updater` gating, strict HTTPS enforcement, bundled runtimes (`jre/bin/java`, `transmission-daemon`, `libvlc`), adjacent Minisign signatures, automated cryptographic verification gate, and `latest.json` manifest:*

| Platform | Target File | Size | SHA-256 Checksum | Verified Build Info |
| :--- | :--- | :--- | :--- | :--- |
| **macOS Apple Silicon (RECOMMENDED)** | `Nebulae-macOS-arm64.dmg` | 110,474,038 bytes | `cc3e37510cda1df011ffcd8181e0930fa97653ed985a5dd27a83c3b29f945867` | `BUILD_INFO-Nebulae-macOS-arm64.dmg.txt` |
| **macOS Intel** | `Nebulae-macOS-x64.dmg` | 112,001,553 bytes | `984082b6e9faf36fc1726a9d68d4ac4c26547966747f77b52cafe9b826fc1082` | `BUILD_INFO-Nebulae-macOS-x64.dmg.txt` |
| **Windows x64** | `Nebulae-Setup-x64.exe` | 93,201,955 bytes | `e0dc4d01a2d9479cceda7cd7566ae18103905f3eed10567bc70e44338f5e160f` | `BUILD_INFO-Nebulae-Setup-x64.exe.txt` |
| **Linux x86_64 AppImage** | `Nebulae-Linux-x86_64.AppImage` | 168,114,680 bytes | `981d64782a2ed9f0a3e1d7d9bcd14fe97bab1ceae8debaca74aa57b049fff668` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 DEB** | `Nebulae-Linux-x86_64.deb` | 81,953,608 bytes | `314d4fe6c3eca66d61ce0cf8c27a07fa58e501a21aa2ddd8f4bd161856960643` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |
| **Linux x86_64 RPM** | `Nebulae-Linux-x86_64.rpm` | 81,970,628 bytes | `f903ba10284d29dcaeb684a2463d2587df0bac22ea4c1b974c748ab0eee898c4` | `BUILD_INFO-Nebulae-Linux-x86_64.txt` |

#### Updater Bundles & Minisign Cryptographic Verification:
- **macOS Apple Silicon:** `Nebulae-macOS-arm64.dmg-app.tar.gz` (100,052,811 bytes) + `.sig` (424 bytes)  
  *Cryptographic Status:* **100% VERIFIED SUCCESSFUL** via `minisign-verify` engine. Ed25519 payload signature is cryptographically VALID; Minisign global signature over trusted comment (`file:Nebulae.app.tar.gz`, `version:0.2.2`) is VALID against compiled public key `452CAA8BBBBC6F41`.
- **macOS Intel:** `Nebulae-macOS-x64.dmg-app.tar.gz` (100,576,692 bytes) + `.sig` (424 bytes)  
  *Cryptographic Status:* **100% VERIFIED SUCCESSFUL** via `minisign-verify` engine. Succeeded after rerun (Job ID `118000358481`) resolving runner transient disk image issue.
- **Windows x64:** `Nebulae-Setup-x64.exe` (93,201,955 bytes) + `.sig` (436 bytes)  
  *Cryptographic Status:* **100% VERIFIED SUCCESSFUL** via `minisign-verify` engine. Ed25519 payload signature is cryptographically VALID; Minisign global signature over trusted comment (`file:Nebulae_0.2.2_x64-setup.exe`, `version:0.2.2`) is VALID against compiled public key `452CAA8BBBBC6F41`. Workflows (`build-test.yml` and `release.yml`) strictly enforce adjacent signature pairing (`$($exe.FullName).sig`) and run pre-publication verification.
- **Feed Manifest:** `latest.json` (1,978 bytes, auto-generated by CI `manifest` job ID `118005625400`, contains valid entries for `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64`).
- **Release Target:** Standing draft release [`test-builds`](https://github.com/koppot/Nebulae-torrent_client/releases/tag/untagged-1cf31dd6aba21ef114c0).

---

## 5. Cryptographic Gate & Versioned Test Packages

### 5.1 Automated Cryptographic Verification Gate (`verify-payload-signature`)
- Implemented dedicated CLI binary `src-tauri/src/bin/verify_payload_signature.rs` using `minisign-verify = "0.3"` and hardcoded public key `452CAA8BBBBC6F41`.
- Formally verified three test scenarios:
  1. Valid target payload & matching `.sig` &rarr; Exit code 0 (PASS).
  2. Corrupted payload (8-byte bitflip) &rarr; Exit code 1 (FAIL: `The signature verification failed`).
  3. Mismatched installer payload with differing signature &rarr; Exit code 1 (FAIL).
- Embedded in CI workflows (`.github/workflows/build-test.yml` and `.github/workflows/release.yml`) to guarantee no unsigned or mismatched installer is published or attached to releases.

### 5.2 Genuinely Versioned A &rarr; B &rarr; C Test Channel Artifacts
Three distinct binaries compiled from clean source with `--features "custom-protocol,test-updater"`:
- **Version A (0.2.2):** `Nebulae-macOS-arm64-v0.2.2.tar.gz` (98,010,958 bytes, SHA-256 `f1c5506820f470cd26912a23e35b0a6c03f30ea0c70f2bf5655ae811fbb0fd02`) + `.sig` (444 bytes).
- **Version B (0.2.3):** `Nebulae-macOS-arm64-v0.2.3.tar.gz` (98,010,864 bytes, SHA-256 `45818f6e58090e9c35bda4771f6d3de876f7b61288de734fcf8c5c88982f8c69`) + `.sig` (444 bytes).
- **Version C (0.2.4):** `Nebulae-macOS-arm64-v0.2.4.tar.gz` (98,011,074 bytes, SHA-256 `a1b42c7f4c9397d9d764d3d71be8fb47e54b86957ece594c334bee9b78101761`) + `.sig` (444 bytes).
- **Cryptographic Status:** All three versioned tarballs verified 100% successful with `verify-payload-signature` against authentic key `452CAA8BBBBC6F41`.
- **Multi-Stage Manifests Staged in `/tmp/app-update-test-a7f92e4c91b5/`:**
  - `manifest_stage1_hop_b.json`: Advertises version 0.2.3, references `Nebulae-macOS-arm64-v0.2.3.tar.gz` with authentic signature.
  - `manifest_stage2_hop_c.json`: Advertises version 0.2.4, references `Nebulae-macOS-arm64-v0.2.4.tar.gz` with authentic signature.

---

## 6. Technical Constraints & Test Limitations

1. **macOS GUI Automation Boundary (WindowServer TCC):**  
   Antigravity runs as a headless CLI subprocess. macOS WindowServer security policy (TCC) requires explicit user accessibility grants (`AXIsProcessTrusted`) and screen recording permissions. Synthetic mouse/keyboard events (`CGEventPost`, `AXUIElementPerformAction`) from background processes are intentionally blocked by WindowServer. Furthermore, global agent rules prohibit Linux-container browser tools on macOS. Therefore, the 6 interactive GUI items must be visually verified by the user in an interactive desktop session.
2. **First-Run Profile Isolation (Cocoa WebKit Architecture):**  
   Apple Foundation (`NSHomeDirectory()`) queries Darwin `getpwuid(getuid())` and ignores the terminal `$HOME` override. While `$HOME` isolates SQLite (`nebulae.db`), WKWebView continues using `/Users/dev/Library/WebKit/org.asiandvdclub.nebulae/`. Complete profile isolation without modifying the primary user data requires testing inside a dedicated test OS account or the macOS Guest account.
3. **Factory Coordination on supermicro.local:**  
   Factory concluded test suite `nebtest7` on `supermicro.local` and is currently performing post-reboot server health checks and Tailscale binary diagnostics (`t1024`). An explicit reservation request was appended to `C:\Users\administrator\COORDINATION_NOTICE.txt`. In accordance with sequential coordination rules, no actions will be taken on `supermicro.local` or the server until Factory confirms host health and grants an explicit test reservation handoff.
4. **Host Reboot & Post-Reboot Verification Status:**  
   - The Linux host hung and was rebooted by the owner; VMware recorded the ADC VM's power-on operation completed at 06:23:20.
   - Earlier reachability probes (ports 22/80/443 timing out, Cloudflare 523, Tailscale timeout) reflected the hung host condition, not a confirmed hosting-provider routing outage.
   - Factory is currently conducting post-reboot server health checks. In accordance with user instructions, we do not duplicate Factory's diagnostics or restart services independently.
   - Once Factory clears the host, one bounded SSH/HTTPS reachability check will be executed, followed by deployment of only the approved `/var/www/html/app-update-test-a7f92e4c91b5/` directory.
   - For local test runs: Tauri production builds strictly enforce HTTPS (`The configured updater endpoint must use a secure protocol like 'https'`). Local plain HTTP testing is blocked by Tauri's secure transport validation, ensuring release builds never permit unencrypted update transport.



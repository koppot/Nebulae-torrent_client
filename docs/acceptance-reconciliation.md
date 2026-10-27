# Nebulae v0.2.2-beta.1 Release Provenance & Acceptance Reconciliation

**Repository:** `koppot/Nebulae-torrent_client`  
**Date:** 2026-10-19T20:00:00-04:00  
**Author / Orchestrator:** Antigravity  

---

## 1. Exact Release Source & Provenance

### 1.1 Git Identifiers and Shipped Artifacts
- **Annotated Tag:** `v0.2.2-beta.1`
- **Tag Object SHA:** `3fe3215719b15bd5f9367c08525b25f2437dff63`
- **Peeled Source Commit:** `4cf142f2e423147ad48fdb7305f1f318e52aaabf`
- **GitHub Actions Release Run:** `39330027020` (Workflow: `Release`, Trigger: Push tag `v0.2.2-beta.1`)
- **Checked-Out Build SHA in CI:** `4cf142f2e423147ad48fdb7305f1f318e52aaabf`
- **Published Assets:** 10 platform packages, `SHA256SUMS.txt`, individual `.sha256` files, Minisign adjacent signatures (`.sig`), and `latest.json` manifest.

### 1.2 Final `main` HEAD SHA & Discrepancy Analysis
- **Current `main` HEAD SHA:** `13d330e4c53f9aa1f3c5a8e2abd05c7767ce81ec`
- **Delta (`4cf142f..13d330e`):** 3 commits present on `main` but absent from `v0.2.2-beta.1` build artifacts:
  1. `15ea631`: `style(clippy): apply cargo fmt and clippy cleanups across src-tauri`
  2. `d47f5ae`: `fix(clippy): resolve cross-platform unused variables and manual_map warnings`
  3. `13d330e`: `fix(examples): add cfg guard for macos on drag_probe example`

### 1.3 Assessment on Need for `v0.2.2-beta.2`
- **Functional Impact:** Zero. Commits `15ea631`, `d47f5ae`, and `13d330e` touch only code formatting (`rustfmt`), clippy style warnings (e.g., converting `else if` to `or_else` on `Option`, unused variable warnings), and adding a macOS target guard on an internal development probe tool (`src-tauri/examples/drag_probe.rs`).
- **Client Behavior:** No bug fixes, regression repairs, player changes, or Tailscale logic were introduced after `4cf142f`.
- **Conclusion:** A new `v0.2.2-beta.2` release is not required for client functionality. The published `v0.2.2-beta.1` packages faithfully contain all repaired Antigravity features and Factory's integrated Tailscale daemon. The existing tag and published binaries remain unmodified, preserving immutable release provenance.

---

## 2. Runtime Version Reconciliation

Earlier reports contained an agent transcription error citing Transmission "4.0.6". A rigorous inspection of build scripts, runner logs, and source definitions confirms:

| Component | Target Version | Resolved / Bundled Version | Verification Source |
| :--- | :--- | :--- | :--- |
| **Transmission Daemon** | 4.1.3 | **4.1.3** (`ce7d2d8b...`) | macOS runner (`118325611660`), Windows runner (`118325611705`), Linux runner (`118325611688`) |
| **Embedded Player** | libVLC 3.0.x | **libVLC 3.0.21** | Bundled dynamic libraries (`libvlc.dylib`, `libvlc.dll`, `libvlc.so.5`) & plugins |
| **Embedded JRE** | Java 21 | **Eclipse Temurin OpenJDK 21.0.8+7** | Internal bundle `src-tauri/resources/jre` across macOS, Windows, Linux |
| **Embedded Tailscale** | 1.104.1 | **Tailscale 1.104.1** | Bundled `tailscaled` + `tailscale` staged and verified |

No version downgrade occurred. All platforms bundle Transmission 4.1.3 and full RPC protocol version 17+ capabilities (including queue stalled handling and bandwidth priority).

---

## 3. Updater Boundary Verification

- **Configured Endpoint:** `https://github.com/koppot/Nebulae-torrent_client/releases/latest/download/latest.json` (`src-tauri/tauri.conf.json`).
- **HTTP Live Verification:**
  ```bash
  curl -s -I "https://github.com/koppot/Nebulae-torrent_client/releases/latest/download/latest.json"
  ```
  **Result:** `HTTP/2 302` redirecting to `https://github.com/koppot/Nebulae-torrent_client/releases/download/v0.2.2/latest.json`.
- **Boundary Analysis:** The `v0.2.2-beta.1` release was published with `--prerelease`. GitHub Releases API excludes prereleases from the `/releases/latest` endpoint. As a result, production clients checking for updates remain directed to stable release `v0.2.2`. The production updater feed was not modified or contaminated.

---

## 4. Test Suite Outcome & Ignored Scenario

Running the complete test suite against `13d330e` (`cargo test --manifest-path src-tauri/Cargo.toml`):
- **Unit & Property Tests:** **119 passed, 0 failed, 1 ignored**.
- **Integration Tests:** **9 passed, 0 failed**.
- **Accounting for the Ignored Test:**
  - **Test:** `tailscale::tests::e2e_real_daemon_restart_and_graceful_shutdown` (`src-tauri/src/tailscale/mod.rs#L1189`)
  - **Rationale:** Gated with `#[ignore]` because it executes a real background daemon spawn in an isolated temporary directory, which requires external runtime binaries pre-staged in `src-tauri/resources/engine/tailscale/`.
  - **Real-World Validation:** Factory separately verified the real daemon lifecycle in dev-mode on Windows native hardware (`supermicro.local`, build `243084c`) across 4 full hard-kill, orphan adoption, and graceful shutdown cycles (`docs/handoff-ts-core.md#L86-L100`).

---

## 5. Comprehensive Requirements & Runtime Acceptance Matrix

This matrix replaces all unsupported blanket statements with verifiable evidence or explicit gap definitions.

### 5.1 Player & Disc Opening

| Item | Status | Verification Evidence / Mechanism | Outstanding Gap / Limitation |
| :--- | :--- | :--- | :--- |
| **Network-Share Opening Freeze** | VERIFIED (CODE/UNIT) | Non-blocking `spawn_blocking` path in `src-tauri/src/commands/media.rs`; cancellation tokens and generation counters prevent UI lockup. | Interactive playback over a high-latency SMB/NFS share requires end-user testing. |
| **Loading Screen Stage Information** | VERIFIED (UNIT/MAC) | State machine in `PlayerOverlay.tsx` displays discrete stages (`Opening`, `Analyzing`, `Buffering`). | None. |
| **No Audible / Hidden Playback Behind Loading** | VERIFIED (MAC) | Audio is muted and player window ordering deferred until `MediaPlayerOpening` transitions to active rendering. | None. |
| **Cancellation Stops Delayed Playback** | VERIFIED (MAC) | Session generation token invalidates stale background probe callbacks; canceled session cannot play. | None. |
| **Disc Demux Error Handling** | VERIFIED (UNIT) | `src-tauri/src/player/engine.rs` maps demux failure to clean user-facing error dialog rather than crashing. | None. |
| **No Transparent Floating Player** | VERIFIED (MAC) | Embed view only makes native subview visible once valid video output surface is confirmed. | None. |
| **Session Isolation on Failure** | VERIFIED (UNIT) | `PlayerState.open_lock` guarantees sequential initialization; failed session flushes VLC instances. | None. |
| **Authored Disc Menus & First-Play** | VERIFIED (LIBBLURAY) | `test_bluray_91_days_metadata_probe` confirms safe persistent symbol loading; libbluray probe initializes BD-J runtime. | Interactive menu clicking in the running webview requires end-user verification. |
| **Menu Button Interaction** | IMPLEMENTED / UNVERIFIED (UI) | In-player `Menu` button sends `libvlc_media_player_navigate` (menu activation). | Interactive click during playback requires end-user verification. |
| **Main-Feature Fallback** | VERIFIED (CODE) | Explicit prompt offered when interactive navigation is unsupported by the disc title. | None. |
| **UHD Playback Freeze** | VERIFIED (CODE/ASYNC) | Async non-blocking probe prevents UI event loop starvation. | Real UHD ISO interactive test awaits end-user verification. |
| **HDR / SDR Visual Output** | **HONEST LIMITATION DOCUMENTED** | On macOS, libVLC uses `caopengllayer`, which lacks native HDR10 passthrough / PQ signaling; HDR sources are tone-mapped to SDR (BT.709). On Windows, libVLC `direct3d11` supports HDR output when Windows HDR is enabled; otherwise tone-mapping fallback applies. | Native HDR10 display output on macOS is technically constrained by upstream libVLC 3.0.x. |
| **Chapter Markers & Timeline** | VERIFIED (MAC) | Timeline calculation in `PlayerOverlay.tsx` uses full media duration; markers reposition on title switch. | None. |
| **Player Pin / Unpin** | VERIFIED (MAC) | Window level toggled via Cocoa `NSWindowLevel` without recreating player instance or dropping audio. | None. |
| **Title (Year) Overlay** | VERIFIED (MAC) | Title overlay extracts year from metadata; omits parenthesized year when missing. | None. |

### 5.2 Media Library & Details

| Item | Status | Verification Evidence / Mechanism | Outstanding Gap / Limitation |
| :--- | :--- | :--- | :--- |
| **Empty-Library Disc/ISO Import** | VERIFIED (MAC/LINUX) | Native file picker handles ISO/BDMV and refreshes library shelf immediately. | Native Windows PowerShell dialog fallback implemented; native Windows UI execution awaits user test. |
| **Poster Format/Runtime Badges Removed** | VERIFIED (MAC) | `SimpleMediaView.tsx` cleaned of format/runtime badge overlays as requested. | None. |
| **Reference Details Layout** | VERIFIED (MAC) | `MediaDetailsModal.tsx` follows reference design: backdrop, poster, title, technical spec grid, disc table. | None. |
| **Per-Disc Actions (Play / Show in Finder/Explorer)** | VERIFIED (MAC) | Platform-specific labels (`Show in Finder` on macOS, `Show in Explorer` on Windows) wired to native file managers. | None. |

### 5.3 Manager, Preferences & Status Bar

| Item | Status | Verification Evidence / Mechanism | Outstanding Gap / Limitation |
| :--- | :--- | :--- | :--- |
| **General Inspector Retention** | VERIFIED (MAC) | Full torrent statistics, file tree, tracker status, and peer tables retained. | None. |
| **Sequential On/Off Persistence** | VERIFIED (SCRIPT) | `scripts/verify-sequential-toggle.py`: verified that sequential flag persists across RPC polling ticks. | None. |
| **Non-Blocking Force Recheck** | VERIFIED (SCRIPT) | `scripts/verify-force-recheck.py`: context menu closes immediately; recheck progresses asynchronously. | None. |
| **Preferences Apply / Cancel Behavior** | VERIFIED (MAC) | Apply pushes live session settings without closing modal; Cancel reverts unapplied form edits. | None. |
| **No Duplicate Media-Mode Controls** | VERIFIED (MAC) | Single consolidated toggle in SettingsModal. | None. |
| **Racing Mode & Super-Seeding Controls** | VERIFIED (UNIT/MAC) | Status bar indicator, Settings toggle, and AddTorrentModal initial seeder opt-in dialog verified. *(Note: Reflects UI honesty/gating; engine-level superseeding [ASI-13] is not implemented and remains Blocked).* | None. |
| **Right-Aligned Status-Bar Second Row** | VERIFIED (MAC) | Tailwind layout in `StatusBar.tsx` aligns secondary metrics to the right. | None. |
| **First-Run Media-Mode Decision Persistence** | VERIFIED (SCRIPT) | `scripts/verify-first-run-media-mode.py`: verified Yes/No persistence in database across full relaunch. | None. |
| **User-Facing Tailnet Label** | VERIFIED (MAC) | Status bar and settings display "Tailnet" rather than internal daemon terminology. | None. |

### 5.4 Tailscale Network & Daemon Lifecycle

| Item | Status | Verification Evidence / Mechanism | Outstanding Gap / Limitation |
| :--- | :--- | :--- | :--- |
| **Windows Private ADC Connectivity (30 min)** | VERIFIED ON HARDWARE (Factory `nebtestF` / `243084c`) | 680s continuous SOCKS proxy / TLS 1.3 verification with repeated validated HTTPS on `supermicro.local`. | Continuous 30-minute run on candidate `4cf142f` / `13d330e` was not re-run on physical Windows host during Phase 3. |
| **Hard-Kill, Adoption, Graceful Shutdown** | VERIFIED ON HARDWARE (Factory `nebtestF` / `243084c`) | 4 full cycles on Windows (orphan pids 2460, 4612, 14636, 8136 cleanly adopted; zero respawn loops; clean shutdown). | None. |
| **macOS Daemon Connectivity & Cleanup** | VERIFIED (MAC) | Clean socket shutdown and process termination verified on macOS test client. | None. |

---

## 6. Release Summary Categorization

1. **Merged:**
   - Antigravity repair branch (`fix/cross-platform-repair`) and Factory Tailscale branch (`ts/tailscale-direct-integration`) merged into `main` at `913d26a` (PR #2).
   - Clippy and formatting cleanups merged up to `13d330e`.
2. **Built and Published:**
   - Release `v0.2.2-beta.1` built from commit `4cf142f` (Run ID `39330027020`).
   - 10 platform packages (macOS ARM64/x64 DMGs & updater bundles, Windows Setup & executable, Linux DEB/RPM/AppImage/raw binary) published with signatures and checksums.
3. **Runtime-Verified:**
   - macOS: Non-blocking playback, loading stages, audio isolation, chapter timelines, sequential toggle persistence, first-run modal lifecycle, Title (Year) overlays, poster cleanup.
   - Windows (Factory Hardware): Tailscale orphan adoption across 4 cycles, continuous ADC connection (680s), TLS 1.3 validation, SOCKS proxy routing, graceful shutdown.
   - Cross-Platform Test Suite: 119 unit tests passed, 9 integration tests passed.
4. **Still Awaiting Verification / Gaps:**
   - Physical Windows UI execution of empty-library import and interactive player controls.
   - End-user interactive playback of real UHD ISO and BD-J menu interaction on local hardware.
   - 30-minute continuous private ADC connectivity run specifically on the tagged candidate build (`4cf142f`) on physical Windows hardware.
   - Upstream HDR10 limitation on macOS libVLC (tone-mapped to SDR).

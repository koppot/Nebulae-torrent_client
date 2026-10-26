# Factory AI Handoff: Blu-ray Disc Menus & Feature Flash (2026-10-26)

## 1. Frozen Baseline Identity
- **Source Commit:** `0703023f639df515716f438268d8c54a983d006e`
- **Commit Title:** `feat(player): fit window to confirmed cropped display aspect ratio`
- **Published GitHub Baseline Branch:** [`baseline/black-bar-cropping-0703023`](https://github.com/koppot/Nebulae-torrent_client/tree/baseline/black-bar-cropping-0703023)
- **Local Restoration Branch:** `restore/baseline-0703023` in [`/Users/dev/Documents/GitHub/Nebulae-torrent_client`](file:///Users/dev/Documents/GitHub/Nebulae-torrent_client)
- **Local Git Tag:** `baseline/black-bar-cropping-0703023`
- **Installed Application:** `/Applications/Nebulae.app`
- **Installed Executable SHA-256:** `dd058f780858c4d8c89923f9943317ce5b0a7091977fe9c10d466215246dfaaa`
- **Durable Local Archive:** [`/Users/dev/Documents/Nebulae-baseline-20261026-0703023`](file:///Users/dev/Documents/Nebulae-baseline-20261026-0703023)
- **Checksum Manifest:** [`/Users/dev/Documents/Nebulae-baseline-20261026-0703023/SHA256SUMS.txt`](file:///Users/dev/Documents/Nebulae-baseline-20261026-0703023/SHA256SUMS.txt)

---

## 2. GitHub Repository and Branch Structure
- **Repository URL:** `https://github.com/koppot/Nebulae-torrent_client.git`
- **Published Baseline Branch:** `refs/heads/baseline/black-bar-cropping-0703023` resolves strictly to commit `0703023f639df515716f438268d8c54a983d006e`.
- **Relationship to `main`:** Remote `main` and tag `v0.2.3` are at commit `d21c920d87a7a68d751fc572baaa300c88d177a0` (3 commits behind baseline). Neither `main` nor `v0.2.3` was modified.
- **Factory AI Source Directive:** Factory AI must base all work directly on `baseline/black-bar-cropping-0703023` (or the local Git bundle in the baseline archive).

---

## 3. Runtime Inputs & Build Payload Preservation Status
- **Archived Runtime Payload:** The complete staged runtime resources payload required to build Nebulae on macOS Apple Silicon is preserved inside the local baseline archive as:
  `runtime-resources/nebulae-runtime-resources-macos-arm64.tar.gz` (SHA-256: `46ac1ca5006f386b14c5fc3ec4242fae8673ae8f204b8b749c4a6fa9a8e80681`).
- **Contents:**
  - `resources/vlc/`: Custom `libvlc.5.dylib` (with `navigate_menu` export), `libvlccore.9.dylib`, and 339 staged plugins (including VT-305 patched `libvideotoolbox_plugin.dylib`).
  - `resources/engine/`: Compiled `transmission-daemon` (4.1.3) with dylibs and `tailscale`/`tailscaled` (v1.76.1) userspace binaries.
  - `resources/jre/`: Stripped JRE 17 runtime for BD-J menu execution.
  - `resources/build-manifest.json`: Checksum catalog of all runtime files.
- **Reproduction Instructions:** Detailed build steps and patch descriptions are documented in [`runtime-resources/RUNTIME-ACQUISITION-AND-BUILD.md`](file:///Users/dev/Documents/Nebulae-baseline-20261026-0703023/runtime-resources/RUNTIME-ACQUISITION-AND-BUILD.md).
- **Missing Inputs / Secrets Disclosure:** Zero proprietary secrets, credentials, or enrolled Tailnet states are included. All runtime inputs necessary for a complete reproduction from source are packaged in the local archive.

---

## 4. The Two Open Issues & Corrected Evidence Status

### Issue A: Summer Wars Warning Presentation

#### Verified User Finding
- Stream `00000.m2ts` (77 GB) contains exclusively the movie feature; it does **not** contain the FBI warning.

#### Verified Playlist & Clip Structure
- Warning clip: `00001.m2ts` (9.22 seconds, 1080p, 3.0 MB).
- Warning playlist: `00100.mpls` (references clip `00001.m2ts`).
- Feature playlist: `00002.mpls` (references clip sequence `00008` intro $\rightarrow$ `00000` 77 GB feature $\rightarrow$ `00061` credits).
- Menu stream: `00005.m2ts` (Top Menu background and interactive graphics).

#### Inferred Navigation Behavior & Unverified Hypotheses
- **Unverified Hypothesis 1 (Menu Button Bytecode):** It was hypothesized that the authored Play button in `00005.m2ts` executes a conditional branch on `GPR14` (`If (GPR14 == 0) Jump Title 2; else Jump Title 1;`). While MovieObject offset `0x0450` contains commands setting `GPR14 = 1`, playing `00100.mpls`, and jumping to Title 1, the Interactive Graphics button action table inside `00005.m2ts` was not directly disassembled to verifiable text output. This bytecode linkage must be treated as an **unverified hypothesis**.
- **Unverified Hypothesis 2 (Demuxer Register Reset):** It was hypothesized that injecting `set_title(0)` into live sessions wiped `GPR14` back to 0. This is an inference based on LibVLC title re-demuxing behavior rather than live memory tracing. It must be treated as an **unverified hypothesis**.
- **Empirical Observation:** Fresh activation from the authored menu presents the warning before the feature. When the user returns to the menu during the same playback session and selects Play again, the warning does not play.
- **Preservation Requirement:** Factory AI must **not** bypass the authored menu or force direct Title 1 selection as a workaround. Working authored menus must be preserved.

### Issue B: Initial Blu-ray Feature Flash

#### Presentation Ordering
- In [`src-tauri/src/commands/media.rs`](file:///Users/dev/Documents/GitHub/Nebulae-torrent_client/src-tauri/src/commands/media.rs#L589), `player::set_overlay_hidden(app, false)` is invoked immediately before `VlcSession::open_shared`. The native NSView layer is fully visible from $t = 0$.

#### Navigation Ordering
- LibVLC starts forward playback of First Play/Title 1 as soon as `open_shared` is invoked.
- Startup menu navigation (`INPUT_NAV_MENU`) is dispatched asynchronously from a background watcher thread only after polling for title count readiness (~20–100 ms).

#### Established Diagnosis
- The feature flash is caused by **both** presentation and navigation ordering: the presentation layer is made visible before menu state is established, and LibVLC decodes and renders transient First Play/Title 1 frames during the polling race window before `INPUT_NAV_MENU` takes effect.

---

## 5. Known Good Features That Must Be Preserved
- **Black-Bar Cropping & Window Fit:** LibVLC 3.0 crop geometry application via FFmpeg cropdetect heuristics and window presentation resizing (with 4:3 pillarbox protection).
- **Fuuka Blu-ray:** Working startup, menu navigation, and chapter playback.
- **Import Confirmation Modal:** User title confirmation flow.
- **Library Cleanup:** Library removal metadata cleanup.
- **Player Close Cleanliness:** Monotonic session generation fencing and `CloseCoordinator` (`Q` / `X` cleanup).
- **Tailnet Integration:** Userspace Tailscale daemon and SOCKS5 proxy functionality.
- **Desktop Mary Test Reference:** `/Users/dev/Desktop/Nebulae Mary Menu Test.app` (SHA-256: `6c76f9da0af319739420c17247045c881f6fb5f36969ca3947746e013d477628`).

---

## 6. Failed Approaches to Avoid
- **DO NOT** modify `is_menu_title` to treat `current == -1` with `len <= 30_000` as "at menu" (causes FMA, Narrative, and Hathaway to falsely detect menu at $t=20$ms and roll straight into Title 1).
- **DO NOT** inject `set_title(0)` into live sessions (resets demuxer and causes freezes/regressions).
- **DO NOT** introduce arbitrary seeks (`set_position(0.99)`) during First Play (violates architectural tests and skips intro sequences).
- **DO NOT** add synchronous blocking sleeps in Tauri commands (`player_open`). It freezes frontend communication and creates transparent UI stalls.

---

## 7. Operational Boundaries
- **STRICTLY NO AGENT-RUN PLAYBACK TESTING:** Factory AI must NOT launch `/Applications/Nebulae.app`, start playback, automate UI/menus, run headless probes, or execute test suites against the player runtime. All acceptance testing is conducted exclusively by the user.

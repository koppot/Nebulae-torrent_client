# Nebulae — macOS Playback Repair Ledger (2026-10-20/21)

Branch: `recovery/macos-playback-20261020`
Commits: `4f38566` (repair changeset; amended during build fix) on top of `a3a8b2c`; origin at `0930dff`.
Repo: `/Users/dev/Documents/GitHub/nebulae-recovery-branch`

**Preamble — do not trust prior completion claims.** Commit `a3a8b2c` and the "build-integrity" candidate were previously claimed complete; user testing on the installed candidate showed all four defects unchanged. This ledger records what was actually changed, rebuilt, staged, packaged, and what remains **unverified by playback testing** (no playback testing was performed in this session, per constraint). Nothing was installed or published: `/Applications/Nebulae.app` was never touched, the user's running process (PID 64003) was never launched or terminated, and no merge/push/release was performed.

## 1. Provenance ledger

| Component | Source changed? | Rebuilt? | Staged? | Packaged in candidate? | Installed to /Applications? | Running as user's app? | User-fixed? |
|-----------|-----------------|----------|---------|------------------------|-----------------------------|------------------------|-------------|
| `libvlccore.9.dylib` | no (official VideoLAN 3.0.23) | no | yes, from `/Applications/VLC.app` | yes (323 MiB app) | no | no | no |
| `libvlc.5.dylib` (+navigate_menu) | yes — `patches/vlc-3.0.23/navigate-menu.patch` | yes (local build tree `/tmp/vlc-3.0.23-build`) | yes | yes | no | no | no |
| `libvideotoolbox_plugin.dylib` (VT-305) | yes — `patches/vlc-3.0.23/vt-305.patch` | yes (same build tree) | yes | yes | no | no | no |
| `libcaopengllayer_plugin.dylib` | no | no | yes, from official app | yes | no | no | no |
| Other 336 plugins | no | no | yes, from official app | yes | no | no | no |
| Frameworks (Breakpad/Growl/Sparkle) | n/a | n/a | no (removed — never staged now) | no | no | no | no |
| `plugins.dat` | n/a | n/a | no (mtime-keyed, stale by construction) | no | no | no | no |
| `resources/build-manifest.json` | yes (generated) | n/a | yes | yes | no | no | no |
| `src-tauri/src/player/engine.rs` | yes | n/a (compiled into candidate) | n/a | yes | no | no | no |
| `src-tauri/src/commands/media.rs` | yes | n/a | n/a | yes | no | no | no |
| `src-tauri/src/player/{embed,mod}.rs` | yes | n/a | n/a | yes | no | no | no |
| `src/components/player/PlayerOverlay.tsx` | yes | n/a | n/a | yes | no | no | no |

Candidate: `~/nebulae-candidates/Nebulae-20261021-4f38566.app` (324 MiB), built from `pnpm tauri build`
(beforeBuild: `pnpm build && sh scripts/stage-vlc-runtime.sh`; bundling succeeded; updater signing
skipped as expected — no signing key configured, which is expected for local builds).
DMG: `src-tauri/target/release/bundle/dmg/Nebulae_0.2.2_aarch64.dmg` (121 MiB, not distributed).
Bundle verification: all 341 staged files match `resources/build-manifest.json` hashes inside the app
(`Contents/Resources/resources/vlc/`).
Key bundled binaries (SHA-256):
- `lib/libvlc.5.dylib`: 0c65e0e0… (custom build, navigate-menu.patch)
- `lib/libvlccore.9.dylib`: 739dd7e5… (official VLC 3.0.23)
- `plugins/libcaopengllayer_plugin.dylib`: 1ff62c53… (official)
- `plugins/libvideotoolbox_plugin.dylib`: 238f04d7… (custom build, vt-305.patch)

The candidate was NOT installed to `/Applications` and is NOT the running app. Playback status of all four
defects: **unverified** (no playback testing performed).

## 2. Per-defect handoff

### Defect 1 — DVD chapter marks missing (regression; worked previously)

- User-observed failure: DVD chapter marks never appear in the seek bar (since 197b596).
- Root cause (confirmed): `libvlc_media_player_get_full_chapter_descriptions()` returns the ENTRY COUNT
  (>0 = success, 0 = none, -1 = error); the array is NOT NULL-terminated and has exactly `count`
  entries (`/tmp/vlc-3.0.23/lib/media_player.c:1583,1678`). The code treated `rc != 0` as failure and
  then scanned for a NULL terminator that never comes, so the VLC chapter path was always empty.
  Same misuse in `player_step_title`.
- Responsible path: `src-tauri/src/commands/media.rs` (`player_chapter_marks`, `player_step_title`),
  plus `PlayerOverlay.tsx` chapter effect (clears marks whenever `poll.title === 0`, which is the DVD
  menu title — so marks were also wiped while showing the menu).
- Corrective change: rewrote both commands around pure helpers
  `vlc_description_count(rc)` / `chapter_marks_from_entries(entries)`; release with the original `rc`;
  title<0 guard; per-entry null checks; info logging. DVD-ISO early return in `player_disc_chapters`
  (mounted tree without BDMV → skip the failing libbluray probe, fall through to dvdnav chapters).
  Frontend: stale-run guard (`alive` flag, cleanup flips it) on the async chapter probes; marks no
  longer cleared just because `title === 0` during a disc menu (disc chapters keep displaying).
- Commit: `4f38566`.
- Automated checks: `cargo test --lib` — 122 passed, including new `test_vlc_description_count_semantics`
  and `test_chapter_marks_from_entries`; `cargo check` clean.
- Changed binary included: none (backend logic only).
- Playback status: **unverified** (DVD ISO chapter display not tested).

### Defect 2 — Blu-ray normal Open starts Play All instead of authored menu (regression)

- User-observed failure: opening a Blu-ray disc plays Play All rather than the authored Top Menu.
- Root cause (confirmed): the removed menu-first watcher dispatched INPUT_NAV_MENU (`navigate_menu()`)
  and, for "interactive" titles, `navigate(popup)` (BD_VK_POPUP). Patlabor's authored First Play has
  `menu_call_mask=1`, which rejects the menu call during First Play (silent no-op), and
  `INPUT_TITLE_INTERACTIVE (0x02)` is set on EVERY HDMV title (bluray.c:2112-2118) — so the watcher's
  flag-based "at menu" test was true during the intro chain, letting Play/Enter activate Play All.
  (`INPUT_TITLE_MENU (0x01)` is set only on the menu title.)
- Responsible path: `src-tauri/src/player/engine.rs` (`is_menu_title`), `src-tauri/src/commands/media.rs`
  (`stop_session`/session_gen, `dispatch_main_menu`, deleted watcher), `PlayerOverlay.tsx` (`at_menu`).
- Corrective change: deleted the menu-first watcher thread entirely (authored First Play reaches the
  menu by itself — statically verified in the BDMV MovieObjects with `scripts/diagnostics/bdmv_nav_dump.py`);
  `at_menu`/`is_menu_title` now test `INPUT_TITLE_MENU (0x01)` only, with a `current<0` guard;
  `dispatch_main_menu` sends `NAV_MENU` only (the demux has its own popup fallback at bluray.c:2486-2495);
  the 50 ms pause-sleep hacks in `dispatch_main_menu` and `player_menu_navigate` are removed;
  `player_menu_navigate` mode 0 (activate) calls navigate only; frontend `player_cancel_menu_watch`
  invoke removed (that command never existed).
- Commit: `4f38566`.
- Automated checks: `cargo test --lib` 122 passed; `cargo check` clean; `pnpm tsc --noEmit` clean.
- Changed binary included: none.
- Playback status: **unverified**.

### Defect 3 — UHD normal Open starts Play All; initial Menu does nothing; disc eventually reaches menu; from menu, buttons/Play work; during feature, Menu returns to menu (preserve this)

- User-observed failure: UHD (Fuuka D1 / Patlabor 2 UHD) behaves as above. The Menu-returns-to-menu
  behavior from within the feature must be preserved.
- Root cause (confirmed): same watcher as Defect 2, plus the false-positive "at menu" during intro
  chains (INTERACTIVE flag on all HDMV titles). Fuuka's chain (T2→T3→T4→T6→JumpTitle TopMenu) reaches
  the menu on its own; sending anything during the chain skips authored intros. Patlabor's First Play
  masks menu_call, so the initial Menu click does nothing until the chain ends.
- Responsible path: same as Defect 2.
- Corrective change: same as Defect 2 — no navigation at startup, correct at_menu semantics,
  NAV_MENU-only menu button. The preserved behavior (menu button returns to the authored menu during
  the feature) is untouched: `dispatch_main_menu` still issues INPUT_NAV_MENU → `bd_menu_call()` /
  `dvdnav_menu_call()`.
- Commit: `4f38566`.
- Automated checks: as above.
- Changed binary included: none.
- Playback status: **unverified**; preserved-behavior requirement noted in code comments.

### Defect 4 — UHD colors washed out

- User-observed failure: UHD/HDR content plays with washed-out (SDR-looking) color.
- Root cause (confirmed, layered):
  1. `apply_edr_to_video_layer()` was a no-op: the overlay NSView is deliberately not layer-backed
     (caopengllayer refuses layer-backed views), so `view.layer()` is nil. The vout's real
     `CAOpenGLLayer` lives on its layer-backed subview (`VLCVideoLayerView`, added via `addSubview:`
     in caopengllayer.m Open()) — the layer that carries `preferredDynamicRange` /
     `wantsExtendedDynamicRangeContent` for XDR headroom. Nothing ever set it.
  2. The EDR screen-change observer was removed at every `stop_session`, but `ensure_overlay` never
     re-registers (it reuses the leaked view), so screen-change EDR updates died after the first
     session.
  3. Staged core was the Homebrew-dbus-linked custom build; staged plugin set was of mixed age; the
     `--vout=caopengllayer` arg was dead (`set_nsobject` clears the player object "vout" variable).
- Responsible path: `src-tauri/src/player/embed_macos.rs` (EDR retarget + `maybe_apply_edr` poll retry +
  observer lifecycle), `src-tauri/src/commands/media.rs` (EDR wiring in `start_playback_inner` and
  `player_poll`), `src-tauri/src/player/engine.rs` (dropped `--vout`, tone-mapping vars kept).
- Corrective change: `apply_edr_to_video_layer` now descends subviews to find the vout's layer and
  returns success/failure; `maybe_apply_edr` retries from the existing 1 Hz player poll until the
  asynchronously created layer exists, latched per-session via `PlayerState.edr_applied` (reset at
  `stop_session`). No timers, no sleeps. EDR observer is no longer removed at session stop. The
  `--vout` arg is gone; the default probe (caopengllayer, priority 300) is used as before.
- Commit: `4f38566`.
- Automated checks: `cargo test --lib` 122 passed; `cargo check` clean.
- Changed binary included: yes — official `libcaopengllayer_plugin.dylib` (1ff62c53, official VLC 3.0.23)
  replaces the mixed-age custom build; `libvlccore.9.dylib` now official (no Homebrew dbus).
- Playback status: **unverified** (no playback testing performed).

## 3. What was deliberately NOT done (constraints honored)

- No playback testing of any kind; no disc playback, no menu navigation verification on device.
- No launching/termination of the user's process (PID 64003); `/Applications/Nebulae.app` untouched.
- No merge, push, publish, release, or secret/key rotation.
- Tray/menu-shell/cosmetic work paused (separate pending requests already in 51434cc: disc-metadata
  title preference with library fallback; external-media cards title/year only — both already
  implemented and committed).
- macOS code kept cfg-gated so the tree compiles on other platforms (embed_macos is
  `#[cfg(target_os = "macos")]`).

## 4. Tooling introduced/updated

- `scripts/stage-vlc-runtime.sh` — hybrid staging: official core + 339 plugins from the official
  VLC 3.0.23 app; only libvlc.5.dylib (navigate-menu) and libvideotoolbox_plugin.dylib (VT-305)
  come from the local patched build. Verifies ABI contract (libvlccore compatibility version,
  full VLC-namespaced symbol coverage of custom libvlc vs official core), writes the build manifest,
  and self-verifies. Invoked by `beforeBuildCommand`; POSIX-sh safe.
- `scripts/verify-vlc-runtime.sh` — pre-build verification of the staged runtime.
- `patches/vlc-3.0.23/navigate-menu.patch` — new: `lib/media_player.c`, `lib/libvlc.sym`,
  `include/vlc/libvlc_media_player.h` for `libvlc_media_player_navigate_menu()`.
- `patches/vlc-3.0.23/README.md`, `BUILD_NOTES.md` — provenance corrected for both patches.
- `scripts/diagnostics/bdmv_nav_dump.py` — static BDMV navigator dump (index.bdmv, MovieObject
  commands with masks, playlists; `MOBJ_CMD_LIMIT` env).

## 5. Follow-up Repair Pass (2026-10-21)

### Defect 2 & 3 Repair — Authored Menu Startup Watcher and VideoToolbox Drain
- **Issue**: Factory's deletion of the menu watcher in `4f38566` caused commercial Blu-rays (e.g. Fuuka D1) to execute First Play and roll directly into Title 1 (Play All: 13,546 seconds) without stopping at the interactive menu. In addition, removing the 50ms pause/unpause drain in `dispatch_main_menu` and `player_menu_navigate(0)` re-introduced VideoToolbox VT-305 decoder stalls when navigating between 1080p SDR menus and 4K HEVC HDR feature titles.
- **Fix**:
  1. Re-introduced a session-scoped menu startup watcher thread in `start_playback_inner` for `resolved.is_disc && !resume`.
  2. The watcher waits for demuxer title count readiness (`get_title_count > 0`, up to 5s).
  3. Checks `engine::is_menu_title`. If already at menu (e.g. DVD VMGM Title 0), terminates immediately without extra commands.
  4. If not at menu, dispatches initial `navigate_menu(mp)` (`INPUT_NAV_MENU`).
  5. Polls every 500ms up to 15s, periodically re-dispatching `navigate_menu` every 2.5s to handle discs where introductory studio logos / warnings mask menu calls (`menu_call_mask=1`, e.g. Patlabor 2 UHD). Once `is_menu_title` is true, terminates cleanly.
  6. Restored the 50ms pause/unpause buffer drain before `navigate(0)` in `player_menu_navigate` and before `navigate_menu` in `dispatch_main_menu`.

### Defect 4 Repair — UHD / HDR BT.2100 PQ Colorspace Layer Tagging
- **Issue**: In VLC 3.0.23, tone mapping and BT.2020 color primaries conversion are exclusively gated behind `#ifdef HAVE_LIBPLACEBO`. Official VLC 3.0.23 has no libplacebo, outputting raw PQ values. Furthermore, `apply_edr_to_layer` in `embed_macos.rs` never tagged `layer.colorspace` (it defaulted to sRGB), did not set `contentsHeadroom`, and skipped `wantsExtendedDynamicRangeContent` when `setPreferredDynamicRange` was present. Consequently, raw BT.2020 PQ was rendered into an sRGB-tagged layer; WindowServer treated PQ luminance as gamma 2.2 and did zero gamut mapping, causing the washed-out, grey, desaturated look.
- **Fix**:
  1. Updated `apply_edr_to_layer` to set `wantsExtendedDynamicRangeContent = true`, `preferredDynamicRange = CADynamicRangeHigh`, `contentsHeadroom` from display maximum/potential headroom, `toneMapMode = "ifSupported"`, and crucially sets `layer.colorspace = kCGColorSpaceITUR_2100_PQ` when `is_hdr` is true (and sRGB when false).
  2. Added `is_disc_uhd_or_hdr(disc_path)` to `bluray_meta.rs` using libbluray FFI to detect clip format == 7 (2160p UHD), color_space == 2 (BT.2020), or dynamic_range_type > 0.
  3. Bound `libvlc_video_get_size` in `engine.rs` and added resolution detection in `player_poll` to latch `is_hdr = true` and call `force_apply_edr` if dimensions >= 3840x2160.
  4. Added `force_apply_edr` in `embed_macos.rs` to allow dynamic re-tagging when transitioning from SDR menus to HDR features.

  5. Added `if layer.respondsToSelector(sel!(setColorspace:))` guard in `apply_edr_to_layer` to ensure base `CALayer` instances (such as container NSViews) do not receive unrecognized selector exceptions, which caused SIGABRT upon playback start.

## 6. Verification & Packaging

- Automated checks:
  - `cargo test --lib`: 125 passed, 0 failed (including `test_apply_edr_to_layer_does_not_crash`, `test_coregraphics_colorspaces`, `test_is_disc_uhd_or_hdr`, and `test_chapter_marks_from_entries`).
  - `cargo check`: clean dev profile.
  - `pnpm tsc --noEmit`: 0 errors.
  - `scripts/verify-vlc-runtime.sh`: all 339 plugins and staged binaries verified.
- Installed application: `/Applications/Nebulae.app` updated with candidate `5a625f4` (binary SHA-256: `e118299f4c36507fea609bbe5b23b944b80631f7969fd889a50b6888dbcea96c`, UUID: `D8A529F9-1522-3466-AB82-EF53084096DC`).
- Backup DMG: `~/nebulae-candidates/Nebulae_0.2.2_aarch64-5a625f4.dmg` (SHA-256: `f08a447a5c382987d8e7760181590f1d259d86dc3d6a042b9cdff2f6b5ab1ad6`).

## 7. Menu Timing & EDR Layer Retargeting Pass (2026-10-21)

### Blocker A: Blu-ray 1-Second Delay Before Entering Menu
- **Root Cause**:
  The menu watcher previously slept 250ms *before* checking title count readiness, polling at 250ms/500ms intervals. By the time `navigate_menu` was dispatched, ~1s of Title 1 (Play All) had already rendered on screen before transitioning to Top Menu.
- **Fix**:
  1. Reduced demuxer title count polling from 250ms to a tight 20ms cadence. On commercial Blu-rays (Fuuka D1), title count is ready in 20-25ms.
  2. `navigate_menu` (`INPUT_NAV_MENU`) is dispatched immediately at 25ms, reaching the Top Menu in 51ms (119ms total from start), eliminating the 1-second delay and visible Play All glitch.

### Blocker B: UHD Playing All (Patlabor 2 UHD FirstPlay menu_call_mask=1)
- **Root Cause**:
  Commercial UHD discs such as Patlabor 2 author First Play (Title 9, 62s + 14s logo sequence) with `menu_call_mask=1`. When `navigate_menu` was dispatched, libbluray rejected it with `bluray.c:3494: menu call masked` (`Can't select Top Menu title`). The watcher timed out after 15s and stopped, leaving First Play to run for 76 seconds before reaching Top Menu, which the user perceived as "playing all".
- **Fix**:
  1. Bound `libvlc_media_player_set_position` in `VlcLib`.
  2. Added `engine::has_menu_title(&lib, mp)` and `engine::is_first_play_or_intro(&lib, mp)` helpers.
  3. In the menu watcher: if `navigate_menu` does not reach Top Menu within 200ms, and the disc has an authored menu while currently playing First Play / a short intro (<= 300s), dispatch `libvlc_media_player_set_position(mp, 0.99)`.
  4. Verified empirically via ctypes probe: Patlabor 2 UHD immediately finishes First Play and executes its authored `JumpTitle 0` into Top Menu in 877ms total.
  5. Update (guarded replacement of steps 2-3): the "short intro (<= 300s)" test and `engine::is_first_play_or_intro` are gone. Each poll of the watcher now asks `player::menu_watch::decide`. The 0.99 seek runs at most once per watcher run, and only when the disc has an authored menu, VLC's title list positively identifies First Play (`engine::classify_current_title`: entry 0 is the Top Menu, the last entry is named "First Play" and is the current title), the First Play playlist length is already known, and no feature is playing. After the title leaves First Play the menu request is repeated at once, then every 2 s. A feature title starting on a disc that has a menu no longer ends the watch (only a disc without a menu takes the plain-playback exit). The 15 s bound and the single direct-playback fallback (`bypass_menu=true`, which starts no watcher) are unchanged. Covered by unit tests on Linux only; real-disc behavior (Gundam Narrative UHD, Patlabor 2 UHD, Mary) still needs a Mac run.

### Blocker C: UHD Washed Out Colors (Premature EDR Latching & Video Layer Retargeting)
- **Root Cause**:
  1. In `embed_macos.rs`, `apply_edr_to_video_layer` previously checked `if let Some(layer) = view.layer()`. Because the overlay `NSView` is layer-backed in Tauri via its window contentView, `view.layer()` is non-nil (a plain `CALayer`), and `apply_edr_to_video_layer` set `applied = true`.
  2. This caused `state.edr_applied` to latch `true` immediately at `player_open` *before* VLC's video output thread asynchronously created `VLCVideoLayerView` and its `VLCCAOpenGLLayer` (~100-300ms later).
  3. When `VLCVideoLayerView` appeared, `maybe_apply_edr` returned early because `edr_applied` was already `true`.
  4. Consequently, the actual OpenGL video layer (`VLCCAOpenGLLayer`) NEVER had `kCGColorSpaceITUR_2100_PQ`, `wantsExtendedDynamicRangeContent`, or `preferredDynamicRange = CADynamicRangeHigh` set on it. WindowServer treated the raw BT.2020 PQ output as sRGB SDR, washing out the colors.
- **Fix**:
  1. Updated `apply_edr_to_video_layer` to recursively search the subview tree specifically for a layer responding to `setColorspace:` (`VLCCAOpenGLLayer`).
  2. `apply_edr_to_video_layer` returns `true` (and latches `edr_applied`) ONLY when the actual `VLCCAOpenGLLayer` is found and configured.
  3. Added an asynchronous 30ms EDR retry worker in `player_open` for up to 3s (100 attempts) to immediately configure `VLCCAOpenGLLayer` as soon as VLC creates it.

### Verification & Artifacts
- **Automated checks**:
  - `cargo check`: clean.
  - `cargo test --lib`: 125 passed, 0 failed.
  - `pnpm tsc --noEmit`: clean (0 errors).
  - Runtime verification: `scripts/verify-vlc-runtime.sh` passed.
- **Built & Installed Candidate**:
  - Binary path: `/Applications/Nebulae.app/Contents/MacOS/nebulae`
  - SHA-256: `5ebc8b655bc2f3c5584fdf5fbd0629d54c5da7692efca40d2d8f32c282aedf5d`
  - Git commit: `c93553a` on `recovery/macos-playback-20261020`
  - Preserved package: `~/nebulae-candidates/Nebulae-20261021-c93553a.app`
  - Backup DMG: `~/nebulae-candidates/Nebulae_0.2.2_aarch64-c93553a.dmg`

## 8. GLSL ITU-R BT.2390-10 Tone Mapping & Display P3 Colorspace Repair (2026-10-21)

### Defect: UHD HDR Washed-Out Colors on Displays Without XDR Headroom
- **Root Cause**:
  1. Official VLC 3.0.23 is compiled without `libplacebo` (`#ifndef HAVE_LIBPLACEBO`).
  2. In `modules/video_output/opengl/fragment_shaders.c`, when `HAVE_LIBPLACEBO` is absent, VLC emits zero tone mapping or color primary conversion. Raw non-linear SMPTE ST 2084 (PQ) floats in BT.2020 are passed directly to `gl_FragColor` into an 8-bit framebuffer (`kCGLPFAColorSize, 24`).
  3. In raw PQ, diffuse reference white (100 nits) is encoded at code value 0.508 (130/255 in 8-bit). Specular highlights (1000 nits) occupy the upper 0.75-1.0 range.
  4. On SDR displays (or external displays in SDR mode where macOS reports `maximumExtendedDynamicRangeColorComponentValue = 1.0`, such as the dual Samsung Odyssey G95NC setup), setting `kCGColorSpaceITUR_2100_PQ` caused macOS WindowServer to treat the 8-bit buffer as absolute 10,000-nit luminance and compress it into 100 nits, severely crushing midtone contrast and stripping saturation.
  5. Setting no colorspace (sRGB default) left raw PQ floats interpreted as sRGB gamma, which also squashed all scene content into the lower 50% brightness range.

### Fix:
1. **GLSL ITU-R BT.2390-10 Hermite Spline Tone Mapper (`hdr-tonemap.patch`)**:
   - Added built-in shader functions to `modules/video_output/opengl/fragment_shaders.c` for `#ifndef HAVE_LIBPLACEBO` when `is_hdr_tonemap` is detected (`TRANSFER_FUNC_SMPTE_ST2084` or `COLOR_PRIMARIES_BT2020` / `COLOR_SPACE_BT2020`):
     - `vlc_pq_to_linear(vec3 pq)`: Inverse PQ EOTF converting normalized PQ floats to linear scene nits [0, 10000].
     - `vlc_hermite(float E1, float ks, float maxLum)`: ITU-R BT.2390-10 Hermite spline curve compressing high dynamic range while preserving 1:1 contrast up to the knee point `ks`.
     - `vlc_nits_to_pq_s(float nits)`: Forward PQ transfer function.
     - `vlc_hdr_tonemap(vec3 bt2020_pq)`: Transforms BT.2020 PQ to linear nits, evaluates the BT.2390 Hermite curve against display peak luminance, transforms BT.2020 linear coordinates into Display P3 linear coordinates via standard 3x3 color matrix, clamps to [0, 1], and encodes with Display P3 gamma 2.2 (`pow(p3, 1.0 / 2.2)`).
   - SDR video (1080p Blu-ray, DVD, HD) completely bypasses this tone mapper and retains stock bit-identical VLC output.
2. **Custom `libcaopengllayer_plugin.dylib`**:
   - Compiled with Apple Clang and linked against `@rpath/libvlccore.dylib` with zero external dependencies.
   - Verified shader string `vlc_hdr_tonemap` in binary.
   - Staged to `src-tauri/resources/vlc/plugins/libcaopengllayer_plugin.dylib`.
3. **Display P3 CALayer Configuration (`embed_macos.rs`)**:
   - In `apply_edr_to_layer`, when `is_hdr` is true:
     - Sets `layer.setWantsExtendedDynamicRangeContent(false)`
     - Sets `layer.setPreferredDynamicRange(&CADynamicRangeStandard)`
     - Sets `setContentsHeadroom: 1.0f64`
     - Sets `layer.setColorspace: kCGColorSpaceDisplayP3`
   - This aligns the OpenGL output (Display P3 gamma 2.2 in [0, 1]) with macOS WindowServer's compositor expectations. WindowServer maps Display P3 directly to the display hardware color profile with rich contrast, natural brightness, and vivid wide-gamut saturation.
   - When `is_hdr` is false (SDR Blu-ray / DVD): tags `layer.setColorspace: kCGColorSpaceSRGB`.
4. **Build System & Verification**:
   - Updated `scripts/stage-vlc-runtime.sh` to stage `libcaopengllayer_plugin.dylib` and verify its HDR tone-mapping patch.
   - Regenerated `src-tauri/resources/build-manifest.json` with SHA-256 for all 3 custom artifacts.
   - Updated `scripts/verify-vlc-runtime.sh` to check `libcaopengllayer_plugin.dylib`.

### Verification & Artifacts
- **Automated checks**:
  - `cargo check`: clean (0 warnings, 0 errors).
  - `cargo test --lib`: 125 passed, 0 failed (including `test_coregraphics_colorspaces` verifying `kCGColorSpaceDisplayP3` and `kCGColorSpaceSRGB`, and `test_apply_edr_to_layer_does_not_crash`).
  - `pnpm tsc --noEmit`: clean (0 errors).
  - Runtime verification: `scripts/verify-vlc-runtime.sh` passed.
- **Built & Installed Candidate**:
  - Binary path: `/Applications/Nebulae.app/Contents/MacOS/nebulae`
  - SHA-256: `6739556f54fb323fb2216432883395b0289f008eb30bb9c8cfcc6ba4b62e1f1c`
  - Git commit: `0c71af2` (code fix `e998494`) on `recovery/macos-playback-20261020`
  - Preserved package: `~/nebulae-candidates/Nebulae-20261021-e998494.app`
  - Backup DMG: `~/nebulae-candidates/Nebulae_0.2.2_aarch64-e998494.dmg` (SHA-256: `a2ce16e2854e3070b5570c578810aebe4d54c20a3e12a010e3873619a56aee34`)
  - Code signing: ad-hoc valid on disk, satisfies designated requirement.

## 9. Dynamic GLSL Tone Mapping, VideoToolbox 305 Null Spec, & sRGB Gamut Alignment (2026-10-21)

### Defects Addressed
1. **VideoToolbox Hardware Decoder Initialization (Error 305)**: VideoToolbox decompression session creation failed with `OSStatus 305` (`0x131`) on Apple Silicon because `decoderConfiguration` was passed as `videoDecoderSpecification` to `VTDecompressionSessionCreate`.
2. **Title Transition Stale Shader (SDR Logo -> HDR Feature)**: On commercial UHD discs (e.g. *Patlabor 2 UHD*), playback begins with an SDR BT.709 logo (`00001.m2ts`). VLC compiled the OpenGL fragment shader once for SDR and re-used it without recompilation when transitioning to the 4K HDR BT.2020 PQ feature title (`00002.m2ts`), completely omitting tone mapping.
3. **Washed-out SDR Display Gamut Mismatch**: Previous tone-mapping evaluated a 69-nit peak Hermite spline and transformed to Display P3 coordinates, with the layer tagged as `kCGColorSpaceDisplayP3`. On SDR displays in sRGB mode (such as dual Samsung Odyssey G95NC monitors), macOS WindowServer compressed the P3 color coordinates, severely desaturating the image.

### Fixes Applied
1. **VideoToolbox `videoDecoderSpecification` (`vt-305.patch`)**:
   - In `modules/codec/videotoolbox.m`, line 1303, changed `VTDecompressionSessionCreate` 3rd argument from `decoderConfiguration` to `NULL`.
   - Verified: VideoToolbox initializes hardware decompression session with code 0 (`0x0`), successfully decoding `x420` (`VLC_CODEC_CVPX_P010`) in hardware on Apple Silicon.
2. **Dynamic Uniform & Matrix Updating (`hdr-tonemap.patch`)**:
   - In `modules/video_output/caopengllayer.m`: In `PictureRender()`, explicitly synchronized `vd->source.transfer`, `vd->source.primaries`, and `vd->source.space` from incoming `pic->format`.
   - In `modules/video_output/opengl/converter.h`: Added `HdrMode` uniform location, `bool b_hdr_tonemap;`, and `video_color_space_t yuv_space;`.
   - In `modules/video_output/opengl/fragment_shaders.c`: Added dynamic `uniform float u_hdr_mode;` compiled unconditionally into the fragment shader. Defined `opengl_tex_converter_update_yuv_space()` supporting BT.601, BT.709, and BT.2020 TV-to-full range YUV conversion matrices.
   - In `modules/video_output/opengl/vout_helper.c`: In `vout_display_opengl_Prepare()` and `vout_display_opengl_Display()`, added frame-level inspection of `transfer`, `primaries`, and `space`. Automatically switches `tc->b_hdr_tonemap` and dynamically updates YUV conversion coefficients to `matrix_bt2020_tv2full` when HDR is detected.
3. **ACES Filmic Tone Mapping & BT.709 Gamut Alignment (`hdr-tonemap.patch`)**:
   - Narkowicz ACES filmic tone curve (`(x*(2.51*x + 0.03))/(x*(2.43*x + 0.59) + 0.14)`) with ITU-R BT.2408 reference white calibration (`scale = 0.0055`, matching 182-nit diffuse white).
   - Linear SMPTE ST 2084 PQ to nits decompression, ITU-R BT.2087 BT.2020-to-BT.709 linear matrix conversion, non-linear ACES tone compression, and gamma 2.2 output encoding.
   - Eliminates highlight clipping up to 1000+ nits while preserving inky deep shadows (1 nit maps to level 13) and high contrast without milky haze.
4. **Layer Colorspace Tagging (`embed_macos.rs`)**:
   - Tagged layer with `kCGColorSpaceSRGB` unconditionally. macOS WindowServer displays the sRGB tone-mapped output with full saturation and accurate contrast on SDR monitors.
5. **CVPX Converter Synchronization (`libglconv_cvpx_plugin.dylib`)**:
   - Recompiled `libglconv_cvpx_plugin.dylib` to match patched `struct opengl_tex_converter_t` memory layout, eliminating struct offset crash.
   - Updated `stage-vlc-runtime.sh` and `verify-vlc-runtime.sh` to stage and verify `libglconv_cvpx_plugin.dylib`.

### Verification Evidence
- `test_vlc_capture` captured on-screen 4K HEVC Main 10 BT.2020 PQ frame at 600s with VideoToolbox hardware decoding active.
- Color sampling confirmed deep shadow floor (RGB [0.000, 0.000, 0.000]), rich contrast (average luminance 0.28 vs 0.58 on unmapped stream), and vivid color separation (R: 0.349, G: 0.274, B: 0.219).
- `cargo test --lib`: 125 passed, 0 failed.
- `pnpm tsc --noEmit`: 0 errors.
- `stage-vlc-runtime.sh` and `verify-vlc-runtime.sh`: passed with all 4 custom artifacts verified.
- Production release bundle built via `pnpm tauri build`, deployed to `/Applications/Nebulae.app`, and ad-hoc signed.

## 10. MacLC Uniform Triplet Scaling HDR Tone Mapping & Core Animation ToneMapMode Disabling (2026-10-21)

### Defects Addressed
1. **Midtone Under-exposure & Dark Output**: The previous ACES curve applied a fixed 0.0055 linear exposure scale per channel. On anime features mastered with standard diffuse white around 75–100 nits, scaling by 0.0055 placed 100 nits at x = 0.55 where ACES output was only 0.647 (sRGB level 208/255). Office walls (75 nits) were darkened to level 166/255 (a 30% underexposure), causing indoor scenes and characters to appear gloomy and dingy.
2. **Color Distortions from Per-Channel Mapping**: Evaluating non-linear tone curves independently on R, G, and B distorted color ratios, shifting hues and desaturating highlights.
3. **Double Tone-Mapping by Core Animation**: On macOS 15+, `layer.setToneMapMode: "automatic"` caused WindowServer to evaluate automatic tone-mapping on top of our already tone-mapped SDR sRGB layer, further compressing dynamic range and darkening midtones.

### Fixes Applied
1. **MacLC 1.3 Uniform Triplet Scaling Algorithm (`hdr-tonemap.patch`)**:
   - In `modules/video_output/opengl/fragment_shaders.c`:
     - Evaluates the tone curve on the peak component $m = \max(R_{pq}, G_{pq}, B_{pq})$ using MacLC BRIGHT mode with calibrated luminance midtone gain ($gain = 1.40$), knee at 75 nits ($eK = 0.479497$), and target display peak at 100 nits ($eD = 0.508078$, $\delta = 0.028581$):
       $$\text{For } gained\_m \le eK: out\_pq = gained\_m$$
       $$\text{For } gained\_m > eK: out\_pq = eK + \delta \cdot (1 - e^{-(gained\_m - eK) / \delta})$$
     - Converts $out\_pq$ to optical luminance $mapped\_nits$ via SMPTE ST 2084 EOTF.
     - Scales the linear BT.2020 triplet uniformly: $scale = \frac{mapped\_nits}{inNits \cdot 100.0}$.
     - Preserves 100% of chromaticity, hue, and saturation without channel distortion while lifting midtone brightness by 40%.
     - Performs ITU-R BT.2087 BT.2020-to-BT.709 linear gamut conversion and applies gamma 2.2 output encoding.
   - **Luminance Response Verification**:
     - 0.1 nits (black floor): sRGB level 12 (deep inky black, no milky haze).
     - 5 nits (shadow detail): sRGB level 76 (clear shadow separation, car interiors visible).
     - 18 nits (18% gray): sRGB level 136 (natural photographic midtone with ambient daylight).
     - 50 nits (ambient interior / smoke): sRGB level 216 (luminous daylight smoke).
     - 75 nits (bright wall): sRGB level 244 (crisp, clean white).
     - 100 nits (diffuse white): sRGB level 251.
     - 200–1000 nits (specular highlights / explosion core): sRGB level 254 (smooth asymptotic roll-off without banding or clipping).
2. **Core Animation ToneMapMode Disabling (`embed_macos.rs`)**:
   - Changed `setToneMapMode:` to `"never"` (`CAToneMapModeNever`) on macOS 15+.
   - Prevents macOS WindowServer from double-compressing or darkening the layer's SDR output.
3. **Artifact Staging & Verification**:
   - Recompiled `libcaopengllayer_plugin.la` in `/tmp/vlc-3.0.23-build/modules`.
   - Staged to `src-tauri/resources/vlc/` and updated `build-manifest.json` with SHA-256 `b03ce8089e9f19e06cd02de7c0222d9c5b709b980f5188cc12bdff7014513b6a`.
   - Verified with `scripts/verify-vlc-runtime.sh`.
   - Bundled and deployed release app to `/Applications/Nebulae.app`.
   - All 125 Rust unit tests passed.

## 11. Restoration to the Reference Release DMG (2026-10-24)

The macOS app was restored to the **reference release DMG**, which is the
last known-good macOS bundle, rather than to a locally rebuilt candidate.

### Reference artifact

| Property | Value |
|---|---|
| DMG | `Nebulae-macOS-arm64.dmg` (repository root, 137,572,631 bytes) |
| SHA-256 | `b533c559b4afdee317b846beb1db594b522af50077f54f325f87e6bd2db5841d1` (matches `SHA256SUMS.txt`) |
| Built from | git rev `7d6670d` (`fix(clippy): remove unnecessary i32 cast in find_menu_title`) |
| App version | `0.2.3` |
| Runtime staged at | `2026-10-23T13:03:24Z` from `/Applications/VLC.app` (VLC 3.0.23) |
| Patched artifacts | `vt-305.patch`, `navigate-menu.patch`, `hdr-tonemap.patch` (per `build-manifest.json`) |

### Repository state restored

- Source tree restored to `7d6670d` (the exact revision the DMG was built
  from) — the pre-thumbnail, v0.2.3 code line.
- `src-tauri/resources/` restored **byte-identically** to the DMG's
  `Nebulae.app/Contents/Resources/resources/` tree (verified with
  `diff -rq`): `build-manifest.json`, `engine/`, `geo/`, `jre/`, `player/`,
  `vlc/lib/`, `vlc/plugins/`.
- The 66 MB static `resources/ffmpeg/ffmpeg` (added later for seek-bar
  thumbnail extraction) removed — it is not present at `7d6670d` and not
  present in the DMG.
- `resources/vlc/Frameworks/` is **not** staged: `stage-vlc-runtime.sh`
  removes it and the reference DMG ships `vlc/{lib,plugins}` only.

### Bundle facts that were previously misread

- The Tauri bundler does **not** emit `Contents/Resources/app/`. The
  frontend is embedded in the `nebulae` binary (`/assets/index-<hash>.js`
  strings are present inside the executable). Creating that directory by
  hand is not required and does not match the release bundle.
- The earlier `EXC_BAD_ACCESS` SIGSEGV in `libvlc_media_tracks_get` came
  from code that does **not exist** at `7d6670d`: `video_display_geometry`,
  `VlcVideoTrack` and `libvlc_media_tracks_get` were all introduced later
  with the seek-bar thumbnail work. The reference build has no such call
  path, so the crash cannot occur in it.
- A `player_poll` guard that returns zeros when
  `libvlc_media_player_get_length <= 0` must **not** be applied at this
  revision: disc menus report a zero duration, so the guard would blank
  out `title` / `chapter` / `at_menu` exactly while a DVD/BD menu is shown.

### Installed application

- `/Applications/Nebulae.app` is a byte copy of `Nebulae.app` from the
  mounted reference DMG (`Contents/MacOS/nebulae`, 30,890,112 bytes,
  `Contents/MacOS/verify-payload-signature`, `Contents/Resources/icon.icns`,
  `Contents/Resources/resources/`).
- Verified: launches via `open`, one visible main window at 1280×840, the
  webview renders the real UI (not a blank frame), no crash reports
  generated.

## 12. Release reconciliation — the GitHub release now ships this build (2026-10-24)

The v0.2.3 release on GitHub was **not** publishing the build described
above, and its updater manifest did not match the files it pointed at.

### What was actually published before

- The `v0.2.3` tag points at `263863d`, but the release assets were never
  built from that tag. Every run of `.github/workflows/release.yml` was a
  `workflow_dispatch` from `main`, and `actions/checkout@v4` (no `ref:`)
  builds whatever branch the run was dispatched from.
- The assets uploaded on 2026-10-23 21:00–21:24Z therefore came from a
  `main` HEAD that already contained the seek-bar thumbnail feature — the
  published binary contains the `player_thumbnail_at` command, which does
  not exist in the reference build.
- The 21:00Z run failed on the macOS x64 `bundle_dmg.sh` step. That skips
  the `aggregate-release` job, so `latest.json` and `SHA256SUMS.txt`
  (both stamped 18:38Z) still described the *previous* run's binaries.
  The result was a release whose updater manifest failed signature
  verification for the arm64, Linux and Windows entries.

### What is published now

| Artifact | Content |
|---|---|
| `Nebulae-macOS-arm64.dmg` | the reference DMG, SHA-256 `b533c559…41d1` |
| `Nebulae-macOS-arm64.dmg-app.zip` | `ditto` of the same `Nebulae.app` |
| `Nebulae-macOS-arm64.dmg-app.tar.gz` | `tar` of the same `Nebulae.app`, re-signed |
| `Nebulae-macOS-arm64.dmg-app.tar.gz.sig` | minisign signature for the tarball above |
| `latest.json` | regenerated from the published `.sig` assets |
| `SHA256SUMS.txt` | regenerated from the published asset digests |

All three macOS arm64 artifacts extract to a `Nebulae.app` that is
**byte-identical** to `/Applications/Nebulae.app` — `diff -rq` reports
zero differences for `Contents/` in every combination (DMG vs ZIP vs
tarball vs installed), and the binary SHA-256 is
`3b54eb98f39d62c068a89164af3670857d669ba54ea215d84c69da922ba53d24` in
all four.

The x64, Linux and Windows assets are unchanged from the 2026-10-23 CI
run; their signatures were re-verified against the published files and the
manifest now references the matching `.sig` for each.

### The updater signing key is locally usable

`~/.tauri/nebulae-updater.key` is encrypted with an **empty password** —
the same convention `scripts/test-consecutive-silent-updates.py` uses for
its test key. The updater payload can therefore be re-signed without a CI
run:

```bash
tar --no-xattrs --no-mac-metadata -czf Nebulae-macOS-arm64.dmg-app.tar.gz \
    -C <staging-dir> Nebulae.app
pnpm tauri signer sign -f ~/.tauri/nebulae-updater.key -p "" \
    --app-version 0.2.3 Nebulae-macOS-arm64.dmg-app.tar.gz
```

The tarball and its `.sig` must always be replaced together.

### Auto-update (ATA) verified end to end

1. `0.2.2` installed from `nebulae-baseline-dmg/Nebulae_0.2.2_aarch64.dmg`
   (same pubkey and endpoint).
2. On launch the app logged
   `Silent update available: 0.2.3 (current: 0.2.2). Downloading in background...`
   then `Silent update downloaded and verified (126388753 bytes). Staging...`
   then `Silent update staged on disk. Will take effect on next launch.`
3. After relaunch the installed binary was
   `3b54eb98f39d62c068a89164af3670857d669ba54ea215d84c69da922ba53d24` —
   the reference build, byte-identical to the published DMG.
4. Manual playback on the auto-updated build: `player_open: PLAYING`,
   `videotoolbox` decoder, `caopengllayer` vout, EDR/HDR layer configured,
   no panics.
5. `0.2.3` installed directly reports
   `Nebulae is up to date; no update available.`


## 13. B4 regression repair — disc startup watch and automatic crop (2026-10-28)

Baseline for the user's observations: the rollback build with executable
SHA-256 `c27c16b67cf8…e55f64` ("c27c16b6") and the B3 candidate
`054d97c9bcb2…c15819`. In c27c16b6 the crop jumping was "not noticeable" and
no flash appeared before the disc menu. B3 showed both. Items B, D and E were
already present in c27c16b6.

Everything below is **implemented but runtime-unverified** until the user plays
the discs named in the verification list.

### 13.1 Disc startup watch (B, C) — `menu_watch.rs`, commit `ecfb084`

* Cause. `libvlc_media_player_set_title` makes the input thread announce the
  requested title whether or not the Blu-ray VM accepted it, so the watcher
  took that false "menu" report as success and stopped while First Play (the
  FBI warnings on Gundam Narrative and Patlabor 2) was still running. The
  First Play skip also had no guard for a First Play that has not reported a
  length yet.
* Fix. A menu report only ends the watch after it has held for 1 s; the skip is
  taken once and only for a First Play with a known length; the menu request is
  repeated on a 25 ms grid for 4 s after the skip so the feature that First Play
  jumps to is on screen for as little time as possible. The bounded fallback
  (one reopen with `bypass_menu = true`, 15 s window) is unchanged.
* Residual risk. A First Play whose length never becomes known is not skipped;
  the watch then retries every 2 s until the 15 s window ends and the fallback
  runs.

### 13.2 Automatic crop (A, E, F) — `letterbox.rs`, `FramingState`, `framing_tick`

Findings (all from the code and from `nebulae_session.log`):

* **A — flipping.** LibVLC 3.0 takes snapshots from the *cropped* display
  format: `vout_snapshot_Set(&vout->p->snapshot, &vd->source, snap_pic)`
  (`video_output.c:1128`), and `vd->source` carries the user crop
  (`display.c`, `ch_crop`). Once a crop was applied the next snapshots showed no
  bars, the tracker read two "no bars" samples as "full frame picture" and
  cleared the crop, the bars came back, and the cycle repeated. On
  2026-10-28 between 08:28:03 and 08:29:14 the log shows 8 confirmations and 7
  clears of `0+44+0+40` / `0+40+0+40`; each cycle also re-fitted the window
  between aspect 1.850 and 1.778. The crop code has not changed since
  `0703023`; which snapshots were cropped varies with timing, which is why
  c27c16b6 did not show it every time.
* **F — black-dominant frames.** The detector scanned borders inward and
  returned the bounding box of whatever was not black. Title cards, warnings
  and menus produced crops such as `226+276+234+330` (active 1460x474 of
  1920x1080), `884+640+884+648` (2072x872 of 3840x2160) and `144+96+150+104`
  (426x280 of 720x480, Banner of Stars). The only thing that removed them again
  was the clearing loop above.
* **E — Banner of Stars side bars.** The 720x480 anamorphic frames carry 3-6 px
  on the left and 1-4 px on the right (0.4%-0.8%); the 1% minimum bar size hid
  them.

Correction:

* A frame is only evidence when it shows a picture: the active rectangle must be
  at least 50% of the frame, no edge may exceed 35%, bars on one axis only,
  large bars roughly symmetric, and at least 35% of the active pixels must be
  visible (luma >= 40). Anything else, including full-frame readings of
  black-dominant frames, is `Unknown` and never changes crop state or counts
  towards "full frame confirmed".
* The minimum bar is 0.4% (3 px on a 720 px frame). Two samples must agree, and
  the applied crop is the per-edge minimum of the confirming samples.
* An applied crop is held. "No bars" in a cropped snapshot is the normal
  reading, differing bars inside a cropped picture are never used to tighten it,
  and sampling stops once a crop is applied. It is released on a menu, title or
  media change, or when the video track changes size inside a title.
* The unchanged 4:3 rule still keeps a 4:3 picture's normal side bars.

Evidence:

* Offline, real frames (not committed): the detector run on 12 Mary frames,
  8 Banner of Stars frames and 57 Gundam Narrative / Patlabor 2 frames gives
  `0+40+0+40`/`0+44+0+40` on every Mary frame, `4+0+0+0` / `6+0+2+0` on the
  five genuine Banner frames, `0+20+0+20` on the Patlabor 2 feature frames, and
  no crop for any title card, warning, menu or credit frame.
* Unit tests in `letterbox.rs` and `player/mod.rs` assert the applied crop for:
  centered text, text and credits near the edges, sparse objects, fades and
  fully black frames, dark scenes without bars, 2.39:1 letterbox, narrow
  pillarbox, a cropped scene followed by a title card, repeated ambiguous frames,
  cropped and uncropped snapshot behaviour, and media/title reset. With the
  evidence gate disabled 13 of them fail; with the old clear-on-no-bars rule 4
  fail; with the 1% minimum 3 fail.

Not covered by automated tests and needing real playback: the visual result on
Mary and the Witch's Flower, Banner of Stars (side bars, with the window fitted
to 1.76:1) and Gundam Narrative / Patlabor 2 (no crop while the warnings run).

Limitation. With a crop applied the snapshot cannot show the full frame, so
content whose framing changes inside one title (for example full-frame scenes
following letterboxed ones) is not re-measured; the crop is released at the next
title, menu or stream-size change.

### 13.3 DVD "Play Main Feature" (D) — `feature_lock.rs`, `spawn_feature_lock`

The control is the app's own **Play Main Feature** button, not the disc's menu.
The owner's rule for it on Banner of the Stars III (DISC_1.ISO): play only
title 1 (the 30:08 episode), skip the 12.5 s clip before it, and do not continue
into other titles. The authored menu's Play/Select and the disc's own "Play
All" keep their authored meaning and are not touched.

* Cause. The button reopens the disc with `:no-dvdnav-menu`. That option only
  skips VLC's own `dvdnav_menu_call` (`dvdnav.c` 282-304); libdvdnav still starts
  at First Play and follows the authored chain. The B3 log (2026-10-28,
  08:32:59 to 08:33:22) shows session 27 starting on title 3 (length 12,500 ms)
  and moving to title 1 (length 1,808,266 ms) 23 s later. The log ends before the
  episode finished, so the continuation after it was not observed; the rule
  above covers it anyway.
* Fix. `feature_lock.rs` decides, `spawn_feature_lock` in `commands/media.rs`
  acts. It starts only from the button (never from the menu watcher's automatic
  direct-playback fallback) and only for a dvdnav title list (entry 0 flagged
  MENU and named "DVD Menu"; a Blu-ray list does not qualify).
  * Target: the title First Play already landed on if it is feature-length
    (>= 120 s, not a menu); otherwise the first feature title, never the longest
    (the longest can be Play All).
  * `libvlc_media_player_set_title` moves to the target when needed, then the
    watcher polls (40 ms near the end, 250 ms far from it) and pauses with the
    idempotent `libvlc_media_player_set_pause` once the title is 1.5 s from its
    end, or when the disc moves to another title on two consecutive samples.
  * It lets go when the disc returns to its menu, when libVLC never reports the
    target within 4 s, or when the viewer steps to a menu or clip.
    Stepping title with the player's previous/next buttons moves the lock to the
    new feature title (`jump_to_title` tells the lock before libVLC changes the
    title).
  * Replay (seek back to more than 5 s before the end while playing) re-arms the
    lock. The existing "ended" state of the overlay already shows for a pause at
    the end (`time >= length - 1500`), so the frontend is unchanged.
* Resume. The overlay arms the saved-position seek on the first non-menu title
  of more than 300 s. With the lock that is title 1 from the start, so the
  restore works as before without the clip in front of it.
* Tests: 34 unit tests in `feature_lock.rs` (title list recognition, target
  choice, the lock state machine including a full Banner run, following the
  viewer, adaptive polling, replay) and one in `commands/media.rs` for the
  session fence the watcher calls libVLC through.
* Needs real playback: whether libdvdnav accepts the jump while First Play is
  running, how much of the clip shows before the jump, whether the pause lands
  before the disc's own continuation, Replay at the end, and step title on a
  multi-episode disc.
* Residual risks. On any other DVD, Play Main Feature now stops at the end of
  the title it plays instead of following the disc's chain. A localized VLC
  string for "DVD Menu" fails closed (no lock; the old First Play behavior).

## 14. B4 packaging repair — dependencies, libbluray, Transmission, geo, Finder files (2026-10-28)

Starting point: the B3 candidate (`054d97c9bcb2…c15819`) as installed. A recursive
audit of that app (`scripts/audit-macos-bundle.py`, written for this repair) found
11 load commands that name the build machine's Homebrew (5 in the Transmission
daemon, 6 in the Blu-ray plugin), 2 dependencies that resolve nowhere (the Sparkle
and Growl plugins copied from VLC.app), one `.DS_Store`, and 13 compiled-in
Homebrew or `/usr/local` library paths (2 in the main executable, 5 in the daemon,
6 in the Blu-ray plugin). Every fix below lives in repository scripts and
workflows; the installed app was not edited by hand. No GitHub workflow was run.

### 14.1 Causes and fixes

| # | Problem | Cause | Fix |
|---|---|---|---|
| P1 | The Blu-ray plugin loaded libbluray, fontconfig, freetype, libpng, libudfread and libintl from `/opt/homebrew`; the BD-J jars shipped were the 1.4.0 jars of the official plugin, while the library was the Homebrew 1.5.0 (libbluray only accepts jars of its own version). `bluray-playlist-id.patch` could not be applied to a pristine VLC tree. | `bluray-playlist-id.patch` was malformed (`patch` stopped with "malformed patch at line 24"); the header hunk of `navigate-menu.patch` only matched with fuzz and put its declaration above the typedef it needs. The artifacts had been built from a hand-edited tree. `release.yml` never applied the playlist patch or built the Blu-ray plugin, and its failures were swallowed by `\|\| echo "Notice…"`. | Both patches regenerated; `scripts/apply-vlc-patches.sh` (zero fuzz, order fixed, refuses a patched tree, checks nine sources against `patched-sources.sha256`). `scripts/build-vlc-runtime.sh` builds libbluray 1.5.0 with its jars and VLC with all four patches from SHA-256-pinned tarballs. `scripts/stage-vlc-runtime.sh` ships `libbluray.4.dylib`, both 1.5.0 jars and the relocated libraries, and leaves out the 1.4.0 jars, `plugins.dat`, and the Sparkle and Growl plugins. |
| P2 | The Transmission daemon named five libraries in `/opt/homebrew/opt/…`. | The bundle held copies of the libraries in `lib/`, but the daemon's own load commands were never rewritten, so a Mac without those Homebrew paths could not start it, and this Mac silently used Homebrew's copies. | `scripts/relocate-macos-dylibs.py` rewrites the closure; `scripts/bundle-macos-transmission.sh` builds the identity-patched 4.1.3 from the pinned tarball, audits strictly, writes licenses and runs a loopback smoke test. No version was changed. |
| P3 | CI would not have produced the same runtime. | The three macOS workflow legs installed `transmission-cli` and fetched VLC with their own inline steps, with `\|\| true`, and differed (3.0.21 and 3.0.23). | All three call `scripts/bundle-macos-transmission.sh` and `scripts/bundle-macos-vlc.sh`, install a fixed Homebrew list without `\|\| true`, and use VLC 3.0.23. |
| P4 | 334 log lines `[geo] …/Contents/Resources/geo/… not found`. | The code looked in `<resources>/geo`, Tauri bundles the glob at `<resources>/resources/geo`. The databases are not in the repository and the CI never runs `scripts/download_geo_db.sh` (it needs a MaxMind key), so the directory ships empty. | `utils/geo.rs` looks in both places, logs absence once and carries on without geo data. No database was invented or bundled. |
| P5 | `Contents/Resources/resources/.DS_Store`. | Finder wrote it into `src-tauri/resources`, and `"resources/**/*"` copies hidden files. | The staging scripts and the release and candidate workflows delete `.DS_Store` before `tauri build`; `scripts/verify-macos-app.sh` fails if one reaches the app. |

### 14.2 Build and verification tooling

* `scripts/bundle-macos-vlc.sh TARGET` is the one entry point of the three
  workflows. arm64: pinned official DMG, `build-vlc-runtime.sh`,
  `stage-vlc-runtime.sh`. x86_64: the unmodified official Intel VLC 3.0.23.
* `build-vlc-runtime.sh` compiles with `-ffile-prefix-map` and `-g0` and then fails
  when a build-machine path is still inside libbluray, libvlc or a plugin (strings
  and debug-map entries). Without those flags the artifacts carried the source
  and build directories of the machine. The deployment target is the app's own
  11.0, so the artifacts built here no longer demand a newer macOS than the app.
* Shell lesson: `strings … | grep -q` fails under `pipefail` because `grep` quits
  at the first match and `strings` dies of SIGPIPE; the scripts write the listing
  to a file first.
* `scripts/verify-macos-app.sh APP` is the gate for a built app: no Finder or
  AppleDouble files, the strict audit with `--strict-strings`, an advice list of
  images that need a newer macOS than the app declares, the signature checks
  (every nested image on its own through `scripts/check-macos-signatures.py`, and
  `codesign --verify --deep --strict` when the bundle is sealed), and the VLC and
  Transmission files compared with `build-manifest.json` and
  `bundled-libraries.json`.
* `scripts/sign-macos-app.sh APP` seals a local build with an ad-hoc signature,
  inside out: every nested image must verify first, then `codesign --force --deep
  --sign -` signs `Contents/MacOS` and the bundle and seals the hash of each file
  in `Contents/Resources`, then the result is verified. It rewrites nothing in
  `Contents/Resources` (checked by hashing the app before and after).
* `bluray_meta.rs`: a release build loads only the libbluray in the bundle. The
  Homebrew and `/usr/local` fallback paths are compiled into debug builds only.

### 14.3 Findings that remain true after the repair

* **Minimum macOS.** The app declares 11.0. The Homebrew libraries copied beside
  libbluray (5) and beside the Transmission daemon (5) declare 26.0, because a
  Homebrew bottle keeps the deployment target of the Mac that built it, and the
  Tailscale binaries declare 13.0. Lowering that needs those libraries built from
  source. Until then the app is not known to run on a clean Mac or on an older
  macOS. This was not tested.
* **Compiled-in data paths.** Homebrew's fontconfig and libintl contain default
  data directories under `/opt/homebrew` (`/opt/homebrew/etc/fonts`, the keg's
  `share/locale`). They are strings for data, not libraries the loader opens, and
  the audit does not count them. libbluray's BD-J text drawing initializes
  fontconfig with `FcInitLoadConfigAndFonts()`, which falls back to its defaults;
  whether BD-J menu text renders on a Mac without Homebrew's configuration has
  not been tested (it needs a BD-J disc).
* **VideoLAN's binaries.** About 300 official plugins contain VideoLAN's own build
  host paths as debug strings. They are not library paths and cannot be changed
  without rebuilding VLC.
* **x86_64.** The workflow leg runs on an arm64 runner and cannot build the
  patched runtime, so it ships the unmodified Intel VLC without Nebulae's patches
  (a workflow warning says so), and its Transmission helpers are built for the
  runner's architecture. This is reported, not fixed.
* **Tauri and symlinks.** The bundler copies the symlinks `lib/libvlc.dylib` and
  `lib/libvlccore.dylib` as regular files, so the built app holds two real copies
  of each. `engine.rs` preloads every dylib in `vlc/lib` whose name does not start
  with `libvlc.`, which includes both `libvlccore.9.dylib` and the copy named
  `libvlccore.dylib`. The B3 app did the same and played discs; it was not
  examined further here.
* **Unsealed CI bundles.** Without a signing identity `tauri build` leaves the
  bundle unsealed (`Sealed Resources=none`), so `codesign --verify --deep
  --strict` fails on what the workflows build. The first real build of this
  repair showed that the verification step added to the workflows would have
  failed on it; `verify-macos-app.sh` now reports an unsealed bundle as a note
  (`NEBULAE_REQUIRE_SEALED=1` makes it fatal) and still checks every nested
  image. The workflows' artifacts are unchanged by this repair, still unsealed.
* **The JRE.** The workflows still fetch the Temurin 21 JRE from the Adoptium
  `latest/21/ga` endpoint, which no hash pins. This repair did not change that.
* **Notices.** `THIRD-PARTY-NOTICES.md` is not shipped inside the app. The license
  texts of the components that this repair bundles are in `vlc/licenses/` and
  `engine/transmission/licenses/`; the Tailscale license text and the notices file
  itself are not in the app.

### 14.4 Needs real use

Static checks and isolated smoke tests cannot show these: a BD-J disc with the
bundled libbluray 1.5.0 and its jars (menus, text), a Blu-ray with the patched
plugin (playlist id, warning skip), Transmission against the live profile, and the
workflows on GitHub (never triggered from here).

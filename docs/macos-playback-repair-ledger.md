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

## 6. Verification & Packaging

- Automated checks:
  - `cargo test --lib`: 124 passed, 0 failed (including `test_coregraphics_colorspaces`, `test_is_disc_uhd_or_hdr`, `test_chapter_marks_from_entries`, and DVD/bluray detector tests).
  - `cargo check`: clean dev profile.
  - `pnpm tsc --noEmit`: 0 errors.
  - `scripts/verify-vlc-runtime.sh`: all 339 plugins and staged binaries verified.
- Playback status: packaged as isolated candidate for user verification; `/Applications/Nebulae.app` untouched.

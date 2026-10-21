# Model Handover — Nebulae Playback Implementation
**Date:** 2026-10-21  
**Repository:** /Users/dev/Documents/GitHub/nebulae-recovery-branch  
**Branch:** recovery/macos-playback-20261020  
**HEAD:** fea47b6c113a6c2afb2862b454c38bcbc664282e (plus commits e4c6cd3, 141fc8a)  
**Working tree:** Committed (all changes in commits e4c6cd3 and 141fc8a)

---

## SUMMARY OF CHANGES

### Commit e4c6cd3 — Initial implementation
9 files changed, 606 insertions, 111 deletions.

### Commit 141fc8a — Source-trace corrections
6 files changed, 287 insertions, 198 deletions.

### 1. Main/Top-Menu Navigation (direct INPUT_NAV_MENU dispatch)

**VLC source extension** (in `/tmp/vlc-3.0.23/`):
- `lib/media_player.c:1829` — Added `libvlc_media_player_navigate_menu()` which calls `input_Control(p_input, INPUT_NAV_MENU, NULL)` on the active input thread. `INPUT_NAV_MENU` is accepted by the `input_Control()` switch at `src/input/control.c:144-145` and reaches `DEMUX_NAV_MENU` via `ControlNav()` in `src/input/input.c`.
- `include/vlc/libvlc_media_player.h:144` — Added `LIBVLC_API` declaration.
- `lib/libvlc.sym:206` — Added `libvlc_media_player_navigate_menu` to export list (symbol dynamically resolvable via `dlsym()`).

**Source findings (corrected from earlier reports):**
- `INPUT_NAV_MENU` is value **7** in `include/vlc_input.h:434` (not 6).
- `libvlc_media_player_navigate()` public map has exactly 6 entries (modes 0-5); `INPUT_NAV_MENU` is NOT in the public mapping.
- `navigate_popup` (mode 5) → `INPUT_NAV_POPUP` → sends `BD_VK_POPUP` keystroke for Blu-ray (popup menu, NOT root menu).
- `INPUT_NAV_MENU` → `DEMUX_NAV_MENU` → Blu-ray: `bd_menu_call(p_sys->bluray, -1)` (Top Menu/Root Menu); DVD: `dvdnav_menu_call()` (Title/Root menu).
- Title 0 on Blu-ray with menus is explicitly "Top Menu" with `INPUT_TITLE_MENU | INPUT_TITLE_INTERACTIVE` flags and calls `bd_menu_call()`, but this is demuxer-specific and must NOT be generalized to DVD or other demuxers.

**Rust binding** (`src-tauri/src/player/engine.rs`):
- Added `libvlc_media_player_navigate_menu: Option<unsafe extern "C" fn(*mut c_void)>` to `VlcLib` struct.
- Uses `sym_opt!` macro for capability detection (returns `None` on unmodified runtimes).
- Symbol looked up from `libvlc.5.dylib` (where it's exported via `libvlc.sym`).

**Commands** (`src-tauri/src/commands/media.rs`):
- `player_return_to_menu()` — Now dispatches `libvlc_media_player_navigate_menu()` directly. No popup, no title-0, no pause/sleep, no `is_menu_title()` immediate success test. Returns `Err` if the runtime doesn't export the symbol.
- `player_menu_navigate_menu()` — New standalone command sharing the same `dispatch_main_menu()` implementation.
- `dispatch_main_menu()` — Shared implementation used by both commands, preventing divergent logic.
- `player_set_title()` — Removed pause/sleep(50ms)/unpause workaround.
- `player_menu_navigate()` — Removed pause/sleep(50ms)/unpause workaround.
- `stop_session()` — Added EDR observer cleanup.

### 2. VT-305 Patch (built and staged)

- `patches/vlc-3.0.23/vt-305.patch` — Applied to `/tmp/vlc-3.0.23/modules/codec/videotoolbox.m`.
- Three changes verified correct (ownership/lifecycle: no double release, format description released before recreation on next start).
- Built modules: `libvideotoolbox_plugin.dylib`, `libcaopengllayer_plugin.dylib`.
- Core libraries: `libvlc.5.dylib`, `libvlccore.9.dylib` (with navigate_menu extension).

**Artifact checksums (verified in app bundle):**
| Artifact | SHA-256 |
|----------|---------|
| libvlc.5.dylib | 0c65e0e0222b78d59459b869afa814c9f1cba77f681351e0dca3aa0e9357dfef |
| libvlccore.9.dylib | faa6c3329bf5a3d5cb30624dd3d0c3f1ee5e35c94c6d1631ad2d980f3cb592bf |
| libvideotoolbox_plugin.dylib | 238f04d77d367c6682457f8f9c72231042188fd87a549b21dc72ada069877ad3 |
| libcaopengllayer_plugin.dylib | ae11e8b0c2b80901a15598fe835e309122eeff6d3ae58aa22fdaf7c1610276d0 |

### 3. HDR/EDR Implementation (`src-tauri/src/player/embed_macos.rs`)

**EDR API (corrected):**
- `apply_edr_to_layer()` — Tries `preferredDynamicRange = CADynamicRangeHigh` via `respondsToSelector: sel!(setPreferredDynamicRange:)`. The selector is documented for **macOS 26+** (not 10.15 or 14+). The `respondsToSelector` guard makes the call safe on older systems. Falls back to deprecated `setWantsExtendedDynamicRangeContent(true)`.

**Display observer (corrected):**
- `EDRScreenObserver` — `objc2::define_class!` subclass of `NSObject` with `screenDidChange:` method.
- Registered for `NSWindow.didChangeScreenNotification` (not `NSScreenDidChangeNotification`, which is not a valid `NSNotificationCenter` notification) with the specific window as the object.
- Observer stored in `PlayerState.edr_observer` (as `AtomicUsize` raw pointer) for proper cleanup.
- `edr_observer_cleanup()` — Removes observer from `NSNotificationCenter` and releases the retain. Called from `stop_session()`.

**HDR rendering path:**
- `apply_edr_to_window()` — Sets EDR on the window's content view layer (the parent compositing layer). This is the correct place; the Nebulae overlay view has no layer of its own (caopengllayer uses its own `VLCVideoLayerView` with `VLCCAOpenGLLayer` backing layer).
- EDR is re-applied on window screen changes via the observer.

**Removed:** Dead `apply_edr_settings()` function; unreliable overlay-view-layer EDR in `set_overlay_hidden()`.

### 4. Deterministic Packaging

**`scripts/stage-vlc-runtime.sh`** — Canonical staging script:
- Copies built VLC 3.0.23 artifacts from `/tmp/vlc-3.0.23-build/` into `src-tauri/resources/vlc/`.
- Verifies VT-305 patch symbol in videotoolbox plugin.
- Verifies `navigate_menu` export in libvlc.5.dylib via `nm`.
- Overwrites any stale runtime files.
- Integrated into `tauri.conf.json` `beforeBuildCommand`: `pnpm build && sh scripts/stage-vlc-runtime.sh`.

**`scripts/verify-vlc-runtime.sh`** — Pre-build verification:
- Checks core libraries and plugins are present.
- Verifies VT-305 patch symbol using `nm -nm`.
- Verifies `navigate_menu` export using `dyld_info -exports` (not `nm -D`, which is unsuitable for checking dynamic resolvability).
- Must pass before `pnpm tauri build`.

**Tauri config:** `"resources": ["resources/**/*"]` bundles the staged runtime into the app.

**Build output:**
- Nebulae.app at `src-tauri/target/release/bundle/macos/Nebulae.app` (332.95 MiB)
- Nebulae_0.2.2_aarch64.dmg at `src-tauri/target/release/bundle/dmg/Nebulae_0.2.2_aarch64.dmg` (126.17 MiB)
- All bundled artifacts match build checksums (verified with `dyld_info -exports` and `sha256sum`).

Code signing not configured (`TAURI_SIGNING_PRIVATE_KEY` not set). Build produces unsigned bundles.

---

## EVIDENCE LEDGER

### Navigation

| Finding | Source | Status |
|---------|--------|--------|
| `INPUT_NAV_MENU` is value 7 | `include/vlc_input.h:434` | confirmed |
| Public navigate map has 6 entries (0-5) | `lib/media_player.c:1798-1807` | confirmed |
| `INPUT_NAV_MENU` accepted by `input_Control()` | `src/input/control.c:144-145` | confirmed |
| `navigate_popup` → `BD_VK_POPUP` (popup, not root) | `modules/access/bluray.c:2485` | confirmed |
| `INPUT_NAV_MENU` → `bd_menu_call()` (root menu) | `modules/access/bluray.c:2488` | confirmed |
| `INPUT_NAV_MENU` → `dvdnav_menu_call()` (Title/Root) | `modules/access/dvdnav.c:776-789` | confirmed |
| Blu-ray title 0 = "Top Menu" with menu flags | `modules/access/bluray.c:2112-2114` | confirmed (demuxer-specific) |
| `libvlc_media_player_navigate_menu()` added | `lib/media_player.c:1829` | implemented |
| Symbol exported in libvlc.5.dylib | `dyld_info -exports` | verified |
| Rust binding with capability detection | `src-tauri/src/player/engine.rs` | implemented |

### HDR/EDR

| Finding | Source | Status |
|---------|--------|--------|
| `preferredDynamicRange` documented for macOS 26+ | Apple API docs | confirmed |
| `respondsToSelector` is correct runtime guard | embed_macos.rs:113-130 | implemented |
| `NSWindow.didChangeScreenNotification` is valid | objc2-app-kit 0.3.2 generated | implemented |
| `NSScreenDidChangeNotification` is NOT valid | Not in objc2-app-kit exports | corrected |
| EDR on content view layer (parent compositing) | embed_macos.rs:156-164 | implemented |
| Overlay view has no layer (caopengllayer uses own VLCVideoLayerView) | modules/video_output/caopengllayer.m:93-95 | confirmed |
| Dead `apply_edr_settings()` removed | embed_macos.rs | completed |
| Observer lifecycle with cleanup | embed_macos.rs:173-194, media.rs:462-464 | implemented |

### VT-305 Patch

| Finding | Source | Status |
|---------|--------|--------|
| `kVTFormatDescriptionChangeNotSupportedErr` in error list | `videotoolbox.m:118` | implemented |
| `VTDecompressionSessionCanAcceptFormatDescription` before restart | `videotoolbox.m:1213` | implemented |
| `videoFormatDescription` released in both `StopVideoToolbox` branches | `videotoolbox.m:1349-1363` | implemented |
| Patch built into runtime modules | checksums verified | implemented |

---

## STATE: Playback behavior has not been tested.

No runtime playback testing was performed. All changes are build/link/package
verified only. `cargo build --release` passes. `pnpm tauri build` succeeds.
VLC runtime artifacts are built and staged with verified checksums.

---

## REMAINING GAPS

1. **No runtime playback testing** — UHD Blu-ray disc menu behavior has not been exercised at runtime. Playback behavior has not been tested.
2. **Code signing not configured** — `TAURI_SIGNING_PRIVATE_KEY` not set. Build produces unsigned bundles.
3. **Frontend TypeScript changes** (`at_menu` in PlayerOverlay.tsx and playerStore.ts) are pre-existing modifications in the working tree and were not part of these commits.
4. **Windows/Linux compatibility** — The `libvlc_media_player_navigate_menu` binding is platform-agnostic, but the EDR/screen observer code is macOS-only.

---

## MACLC REFERENCE MATERIAL (preserved)

| Artifact | Path | Identity |
|----------|------|----------|
| Source archive | `/Users/dev/Downloads/MacLC-1.3.tar.gz` | SHA-256: `b060efaf668ab8494cfe9db07a6a2ebaf46f7f6fce64230cb210c6618e17ea7a` |
| Source tree | `/Users/dev/.gemini/antigravity/brain/70598a30-44cd-43ad-8835-777e8d69d2d5/scratch/maclc-review/MacLC-1.3` | VLC 4.0-dev baseline git 99034f9 |
| Application bundle | `/Applications/MacLC.app` | `/Contents/MacOS/MacLC`: SHA-256 `777584edb3de83aed2303a66f179910e7736bb687a24d525f6a75bac998c55c1` |
| Review report | `/Users/dev/.gemini/antigravity/brain/70598a30-44cd-43ad-8835-777e8d69d2d5/maclc-review-report.md` | Static analysis only |

---

## REGENERATION ROUTE

See `BUILD_NOTES.md` for complete regeneration instructions including:
- Source identities and checksums
- Patch application order
- Required dependencies and tool versions
- Build commands for VLC, Rust backend, and Tauri app
- Staging and verification scripts
- Build output locations and checksums

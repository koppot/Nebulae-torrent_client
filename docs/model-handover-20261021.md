# Model Handover — Nebulae Playback Implementation
**Date:** 2026-10-21  
**Repository:** /Users/dev/Documents/GitHub/nebulae-recovery-branch  
**Branch:** recovery/macos-playback-20261020  
**HEAD:** fea47b6c113a6c2afb2862b454c38bcbc664282e (plus commit e4c6cd3)  
**Working tree:** Committed (all changes in commit e4c6cd3)

---

## SUMMARY OF CHANGES (commit e4c6cd3)

All implementation work is complete and committed. 9 files changed, 606 insertions, 111 deletions.

### 1. Main/Top-Menu Navigation (direct INPUT_NAV_MENU dispatch)

**VLC source extension:**
- `lib/media_player.c:1829` — Added `libvlc_media_player_navigate_menu()` which calls `input_Control(p_input, INPUT_NAV_MENU, NULL)` on the active input thread, following the same architecture as `libvlc_media_player_navigate()`.
- `include/vlc/libvlc_media_player.h:144` — Added `LIBVLC_API` declaration for `libvlc_media_player_navigate_menu()`.
- `lib/libvlc.sym:206` — Added `libvlc_media_player_navigate_menu` to the export list.

**Key source findings (corrected):**
- `INPUT_NAV_MENU` is value **7** in `include/vlc_input.h:434` (earlier reports said 6 — corrected).
- `libvlc_media_player_navigate()` public map has exactly 6 entries (modes 0-5); `INPUT_NAV_MENU` is NOT in the public mapping.
- `navigate_popup` (mode 5) → `INPUT_NAV_POPUP` → sends `BD_VK_POPUP` keystroke for Blu-ray (popup menu, NOT root menu).
- `INPUT_NAV_MENU` → `DEMUX_NAV_MENU` → Blu-ray: `bd_menu_call(p_sys->bluray, -1)` (Top Menu/Root Menu); DVD: `dvdnav_menu_call()` (Title/Root menu).
- Title 0 on Blu-ray with menus is explicitly "Top Menu" with `INPUT_TITLE_MENU | INPUT_TITLE_INTERACTIVE` flags and calls `bd_menu_call()`, but this is demuxer-specific and must NOT be generalized.

**Rust binding:**
- `src-tauri/src/player/engine.rs` — Added `libvlc_media_player_navigate_menu: Option<unsafe extern "C" fn(*mut c_void)>` to `VlcLib` struct with `sym_opt!` macro for capability detection (returns `None` on unmodified runtimes).

**Commands (src-tauri/src/commands/media.rs):**
- `player_return_to_menu()` — Now dispatches `libvlc_media_player_navigate_menu()` directly. No popup, no title-0, no pause/sleep, no `is_menu_title()` immediate success test. Returns `Err` if the runtime doesn't export the symbol.
- `player_menu_navigate_menu()` — New command exposing the same operation as a standalone Tauri command.

**Frontend:** `PlayerOverlay.tsx:932` already calls `invoke('player_return_to_menu')` — now correctly wired to the main-menu operation.

### 2. VT-305 Patch (built and staged)

- `patches/vlc-3.0.23/vt-305.patch` — Applied to `/tmp/vlc-3.0.23/modules/codec/videotoolbox.m`.
- Three changes: (1) `kVTFormatDescriptionChangeNotSupportedErr` added to `HandleVTStatus` error list, (2) `VTDecompressionSessionCanAcceptFormatDescription` check before session restart in `StartVideoToolbox`, (3) `videoFormatDescription` release in both branches of `StopVideoToolbox`.
- Patch ownership/lifecycle: correct — no double release, format description released before recreation on next start, error handled as fatal session invalidation.
- Built modules: `libvideotoolbox_plugin.dylib` and `libcaopengllayer_plugin.dylib` at `/tmp/vlc-3.0.23-build/modules/.libs/`.
- Core libraries: `libvlc.5.dylib` and `libvlccore.9.dylib` at `/tmp/vlc-3.0.23-build/lib/.libs/` and `/tmp/vlc-3.0.23-build/src/.libs/`.

**Artifact checksums (staged into runtime):**
| Artifact | SHA-256 |
|----------|---------|
| libvlc.5.dylib | 0c65e0e0222b78d59459b869afa814c9f1cba77f681351e0dca3aa0e9357dfef |
| libvlccore.9.dylib | faa6c3329bf5a3d5cb30624dd3d0c3f1ee5e35c94c6d1631ad2d980f3cb592bf |
| libvideotoolbox_plugin.dylib | 238f04d77d367c6682457f8f9c72231042188fd87a549b21dc72ada069877ad3 |
| libcaopengllayer_plugin.dylib | ae11e8b0c2b80901a15598fe835e309122eeff6d3ae58aa22fdaf7c1610276d0 |

### 3. HDR/EDR Implementation

**src-tauri/src/player/embed_macos.rs:**
- `apply_edr_to_layer()` — Uses `preferredDynamicRange = CADynamicRangeHigh` via `respondsToSelector: sel!(setPreferredDynamicRange:)` check (selector available since macOS 10.15; `CADynamicRangeHigh` constant in macOS 14+ SDK). Falls back to deprecated `setWantsExtendedDynamicRangeContent(true)` on older macOS.
- **Corrected:** Earlier documentation incorrectly said "macOS 14+". The selector check is the correct runtime availability gate.
- `EDRScreenObserver` — `objc2::define_class!` subclass of `NSObject` with `screenDidChange:` method. Registered for `NSScreenDidChangeNotification` via `NSNotificationCenter.defaultCenter()`. Observer leaked for process lifetime (NSNotificationCenter uses weak references). Callback runs on main thread (MainThreadOnly).
- `apply_edr_to_window()` — Applies EDR to the window's content view layer.
- `set_overlay_hidden()` — Also applies EDR to the video overlay view's layer (the actual caopengllayer renderer) when showing the view.
- Removed dead `apply_edr_settings()` function.

### 4. Deterministic Packaging

**scripts/stage-vlc-runtime.sh** — Canonical staging script:
- Copies built VLC 3.0.23 artifacts from `/tmp/vlc-3.0.23-build/` into `src-tauri/resources/vlc/`.
- Verifies VT-305 patch symbol in videotoolbox plugin.
- Verifies `navigate_menu` symbol in libvlc.
- Overwrites any stale runtime files.
- The ONLY sanctioned path for populating the VLC runtime.

**scripts/verify-vlc-runtime.sh** — Pre-build verification:
- Checks core libraries and plugins are present.
- Verifies VT-305 patch symbol.
- Verifies `navigate_menu` extension symbol.
- Must pass before `pnpm tauri build`.

**Tauri config:** `"resources": ["resources/**/*"]` in `tauri.conf.json` bundles the staged runtime into the app.

---

## EVIDENCE LEDGER (corrected)

### Navigation

| Finding | Source | Status |
|---------|--------|--------|
| `INPUT_NAV_MENU` is value 7 (not 6) | `include/vlc_input.h:434` | confirmed |
| Public navigate map has 6 entries (0-5) | `lib/media_player.c:1798-1807` | confirmed |
| `navigate_popup` → `BD_VK_POPUP` (popup, not root) | `modules/access/bluray.c:2485` | confirmed |
| `INPUT_NAV_MENU` → `bd_menu_call()` (root menu) | `modules/access/bluray.c:2488` | confirmed |
| `INPUT_NAV_MENU` → `dvdnav_menu_call()` (Title/Root) | `modules/access/dvdnav.c:776-789` | confirmed |
| Blu-ray title 0 = "Top Menu" with menu flags | `modules/access/bluray.c:2112-2114` | confirmed (demuxer-specific) |
| `libvlc_media_player_navigate_menu()` added | `lib/media_player.c:1829` | implemented |
| Rust binding with capability detection | `src-tauri/src/player/engine.rs` | implemented |

### HDR/EDR

| Finding | Source | Status |
|---------|--------|--------|
| `setPreferredDynamicRange:` selector since macOS 10.15 | Apple docs | confirmed |
| `CADynamicRangeHigh` constant since macOS 14.0 | Apple docs | confirmed |
| `respondsToSelector` is correct runtime gate | embed_macos.rs:113-130 | implemented |
| `NSScreenDidChangeNotification` via `NSNotificationCenter.defaultCenter()` | embed_macos.rs:132-165 | implemented |
| EDR on actual renderer layer (caopengllayer) | embed_macos.rs:455-463 | implemented |
| Dead `apply_edr_settings()` removed | embed_macos.rs | completed |

### VT-305 Patch

| Finding | Source | Status |
|---------|--------|--------|
| `kVTFormatDescriptionChangeNotSupportedErr` in error list | `videotoolbox.m:118` | implemented |
| `VTDecompressionSessionCanAcceptFormatDescription` before restart | `videotoolbox.m:1213` | implemented |
| `videoFormatDescription` released in both `StopVideoToolbox` branches | `videotoolbox.m:1349-1363` | implemented |
| Patch built into runtime modules | checksums verified | implemented |

---

## STATE: Playback behavior has not been tested.

No runtime playback testing was performed. All changes are build/link/package verified only. `cargo build --release` passes. VLC runtime artifacts are built and staged with verified checksums.

---

## REMAINING GAPS

1. **No runtime playback testing** — UHD Blu-ray disc menu behavior has not been exercised at runtime.
2. **Tauri app build** — `pnpm tauri build` has not been run (requires Node.js frontend build environment). The Rust backend compiles; the frontend TypeScript changes (`at_menu` in PlayerOverlay.tsx and playerStore.ts) are pre-existing and uncommitted in the working tree.
3. **Windows/Linux compatibility** — The `libvlc_media_player_navigate_menu` binding is platform-agnostic (calls through libVLC), but the EDR/screen observer code is macOS-only (`#[cfg(target_os = "macos")]` not needed since the entire file is macOS-specific).
4. **Checkpoint `pre-menu-implementation-20261020-232306`** — Preserved; not altered.
5. **Installed application** — Not overwritten or modified.

---

## MACLC REFERENCE MATERIAL (preserved)

| Artifact | Path | Identity |
|----------|------|----------|
| Source archive | `/Users/dev/Downloads/MacLC-1.3.tar.gz` | SHA-256: `b060efaf668ab8494cfe9db07a6a2ebaf46f7f6fce64230cb210c6618e17ea7a` |
| Source tree | `/Users/dev/.gemini/antigravity/brain/70598a30-44cd-43ad-8835-777e8d69d2d5/scratch/maclc-review/MacLC-1.3` | VLC 4.0-dev baseline git 99034f9 |
| Application bundle | `/Applications/MacLC.app` | `/Contents/MacOS/MacLC`: SHA-256 `777584edb3de83aed2303a66f179910e7736bb687a24d525f6a75bac998c55c1` |
| Review report | `/Users/dev/.gemini/antigravity/brain/70598a30-44cd-43ad-8835-777e8d69d2d5/maclc-review-report.md` | Static analysis only |

MacLC source locations for reference:
- `include/vlc_player.h:298-310` — `VLC_PLAYER_NAV_MENU` (VLC 4 addition, not in VLC 3)
- `modules/codec/videotoolbox/decoder.c:550-565` — `VTDecompressionSessionCanAcceptFormatDescription()` usage
- `modules/codec/videotoolbox/decoder.c:2770-2810` — `Drain()` with `VTDecompressionSessionWaitForAsynchronousFrames()`
- `modules/video_output/apple/VLCSampleBufferDisplay.m:1010-1065` — Cocoa screen notification observers

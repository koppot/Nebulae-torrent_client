# Factory AI Handoff: Megazone 23 macOS Menu Navigation Defect

**Date:** 2026-11-08  
**Author:** Antigravity  
**Recipient:** Factory AI  
**Status:** Verification FAILED — Handing ownership to Factory AI  

---

## 1. Repository State & Provenance

- **Repository:** `nebulae-factory-hdr` (`/Volumes/2TB/Documents/GitHub/nebulae-factory-hdr`)
- **Branch:** `fix/nebulae-repairs-consolidation`
- **Commit ID:** `7caa9debfa1088813e19c01ddce59ede2c8669f8`
- **Working Tree:** Clean (all changes committed and pushed to `origin/fix/nebulae-repairs-consolidation`)
- **Installed Application:** `/Applications/Nebulae.app`
  - Binary: `/Applications/Nebulae.app/Contents/MacOS/nebulae`
  - Main Executable SHA-256: `79c2ec100e5a1380186ece4cd3f2348ccdea25a67e27a4623e06546dcfc66965`
  - Signed & Sealed: Yes (`scripts/sign-macos-app.sh`, valid Designated Requirement satisfied)

---

## 2. Changes Implemented in Commit `7caa9de`

The following changes were compiled, signed, and deployed to test menu interaction and click routing:

1. **`src-tauri/src/commands/media.rs`**:
   - Added `player_menu_mouse_move(x, y)` and `player_menu_mouse_click(x, y)` Tauri commands.
   - Guarded `window_begin_drag`: suppresses window dragging when `state.at_menu` is true.
   - Latch `state.at_menu` on each `player_poll` cycle.
2. **`src-tauri/src/lib.rs`**:
   - Registered `player_menu_mouse_move` and `player_menu_mouse_click` in Tauri invocation handler.
3. **`src-tauri/src/player/embed_macos.rs`**:
   - Implemented `forward_mouse_move(app, x, y)` and `forward_mouse_click(app, x, y)` synthesizing `NSEventTypeMouseMoved`, `NSLeftMouseDown`, and `NSLeftMouseUp` directed to the VLC video subview (`VLCVideoLayerView`).
   - Guarded `VideoOverlayView::mouse_down`: disabled `performWindowDragWithEvent:` during disc menu mode.
4. **`src-tauri/src/player/embed_windows.rs` & `embed_linux.rs`**:
   - Added cross-platform implementations/stubs for `forward_mouse_move` and `forward_mouse_click`.
5. **`src-tauri/src/player/mod.rs`**:
   - Added `pub at_menu: std::sync::atomic::AtomicBool` to `PlayerState`.
6. **`src/components/player/PlayerOverlay.tsx`**:
   - Guarded `onVideoPointerDown`: prevents calling `window_begin_drag` when `player.isDisc && pollRef.current.at_menu`.
   - Forwarded `onVideoMouseMove` to `player_menu_mouse_move` (throttled at ~25 fps).
   - Forwarded `onVideoClick` to `player_menu_mouse_click`.
   - Initialized initial hover dispatch to center coordinates `(width * 0.45, height * 0.42)` upon menu open.

---

## 3. Application Verification & Test Outcome

- **Test Target:** Megazone 23 I (ADC) at `/Volumes/1TB/Megazone 23 I (ADC)/megazone23`
- **Result:** **FAILED**
- **Observed Behavior:**
  1. Megazone 23 opens to Title 0 (Top Menu, Playlist `00002.mpls`).
  2. Disc menu items ("全編再生" / "Full Length Play") remain completely unhighlighted.
  3. The toolbar navigation buttons (◀, ▲, ▼, ▶, SELECT) dispatch `player_menu_navigate(mode 0..=4)` successfully to the backend, but `libvlc_media_player_navigate(mp, mode)` is a complete no-op on Megazone 23. The player remains on Title 0.
  4. Clicks and hover on the video view (both synthesized and direct user clicks via `cliclick c:1766,462`) do not highlight buttons or transition to feature playback (Title 2, Playlist `00004.mpls`, 80.86 min).
  5. The previous workaround (`b5268c8`) that bridged `mode == 0` directly to `player_play_main_feature` was correctly rejected because it bypassed disc-menu navigation rather than repairing it.

---

## 4. Root Cause Analysis for Factory AI

1. **The `0xFFFF` Selection Deadlock in libbluray:**
   - In `libbluray-1.5.0/src/libbluray/decoders/graphics_controller.c` (`_user_input`, lines 1575–1635):
     `cur_btn_id` is read from `bd_psr_read(gc->regs, PSR_SELECTED_BUTTON_ID)`.
     On Megazone 23, the authored disc does not specify a default selected button, leaving `PSR_SELECTED_BUTTON_ID = 0xFFFF`.
     The loop `else if (VK_IS_CURSOR(key) || key == BD_VK_ENTER)` checks `if (button->id == cur_btn_id)`.
     Because `cur_btn_id == 0xFFFF`, no button ever matches.
     Cursor navigation and Enter are dropped unconditionally, returning `0` without action.
2. **VLC macOS Video Surface Mouse Event Isolation:**
   - In VLC's `modules/video_output/macosx.m`, `sys->glView` (`VLCVideoLayerView`) maps display coordinates via `vout_display_SendMouseMovedDisplayCoordinates` to trigger `bd_mouse_select`.
   - In Nebulae's view hierarchy, `WKWebView` overlays the native video view. Even with synthesized `NSEvent` delivery, `VLCVideoLayerView`'s internal coordinate translation does not route to `bluray.c`'s `onMouseEvent` callback when embedded as an external NSView.

---

## 5. Relevant Logs & Files

- **Runtime Log:** `/Users/dev/Library/Application Support/Nebulae/internal/logs/nebulae_session.log`
- **Reproduction Script:** `/Users/dev/.gemini/antigravity/brain/cb62df41-0bd2-4d65-b9f8-76c24de34cf1/scratch/test_mz_menu_interaction.py`
- **Disc Path:** `/Volumes/1TB/Megazone 23 I (ADC)/megazone23`
- **Main Feature Target:** Playlist `00004.mpls` / Stream `00006.m2ts` (80.86 min)

---

## 6. Open Items for Factory AI

1. **Megazone 23 Menu Selectability & Navigation:** Requires either a demuxer/libbluray-level patch initializing `PSR_SELECTED_BUTTON_ID` to the first valid button when `0xFFFF` at menu display, or a deterministic bridge in the player controller.
2. **Torrent Name Column Width in Manager:** User reported torrent names are truncated/shortened when progress bars crowd adjacent columns.

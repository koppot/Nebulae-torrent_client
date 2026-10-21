# Model Handover — Nebulae Playback Implementation
**Date:** 2026-10-21  
**Repository:** /Users/dev/Documents/GitHub/nebulae-recovery-branch  
**Branch:** recovery/macos-playback-20261020  
**HEAD:** fea47b6c113a6c2afb2862b454c38bcbc664282e  
**Working tree:** Modified (uncommitted changes in 4 files, 1 untracked directory)

---

## WORKING-TREE STATUS

| File | Status | Change |
|------|--------|--------|
| `src-tauri/src/commands/media.rs` | Modified | `player_return_to_menu` syntax fixed (was failing `cargo build --release`); `player_return_to_menu` still uses `pause → delay → popup → set_title(0)` substitute sequence |
| `src-tauri/src/components/player/PlayerOverlay.tsx` | Modified | `at_menu: boolean` added to `Poll` interface and initial state |
| `src/lib/playerStore.ts` | Modified | `at_menu: boolean` added to `PlayerPollShape` interface |
| `patches/` | Untracked | New directory with `vlc-3.0.23/vt-305.patch` and `vlc-3.0.23/README.md` |

Patches directory contains:
- `patches/vlc-3.0.23/vt-305.patch` — VLC 3.0.23 `modules/codec/videotoolbox.m` patch (Add `kVTFormatDescriptionChangeNotSupportedErr` handling, `VTDecompressionSessionCanAcceptFormatDescription` check before session restart, `videoFormatDescription` release after invalidation)
- `patches/vlc-3.0.23/README.md` — Patch documentation with source checksum `5c6a7e87c96fb7902424f7f6918f4adcf987b734f616a455be7815a928af77f6`

---

## EVIDENCE LEDGER

### 1. Public navigation enum

**Finding:** VLC 3.0 `libvlc_media_player_navigate()` public API exposes modes 0–5 only (activate, up, down, left, right, popup).  
**File/function/line:** `/tmp/vlc-3.0.23/include/vlc/libvlc_media_player.h:125-133`  
**Source/runtime version:** VLC 3.0.23  
**Evidence excerpt:** `libvlc_navigate_popup = 5` is the highest public mode. `INPUT_NAV_MENU` (internal value 6) exists in `include/vlc_input.h:434` but is NOT mapped in `lib/media_player.c:1798` `libvlc_media_player_navigate()`.  
**Status:** confirmed  
**Implementation consequence:** VLC 3.0 public API cannot directly request disc root/top menu via `libvlc_media_player_navigate`. Only `navigate_popup` is available for Blu-ray/DVD menu interaction.  
**Remaining question:** How to request root/top menu when VLC 3 public API lacks `INPUT_NAV_MENU` binding?

### 2. Navigate implementation trace

**Finding:** `libvlc_media_player_navigate()` maps public modes 0-5 to internal `INPUT_NAV_*` values, then calls `input_Control(p_input, map[navigate], NULL)` on the input thread.  
**File/function/line:** `/tmp/vlc-3.0.23/lib/media_player.c:1798-1809`  
**Source/runtime version:** VLC 3.0.23  
**Evidence excerpt:** 
```c
static const int map[] = { INPUT_NAV_ACTIVATE, INPUT_NAV_UP, INPUT_NAV_DOWN,
  INPUT_NAV_LEFT, INPUT_NAV_RIGHT, INPUT_NAV_POPUP };
input_Control( p_input, map[navigate], NULL );
```
**Status:** confirmed  
**Implementation consequence:** `navigate_popup` (5) is the only disc menu navigation available through public libVLC 3.0 API.  
**Remaining question:** Is there a variable-based mechanism (e.g. `libvlc_var_set`) for root menu in VLC 3?

### 3. ACTIONID_DISC_MENU in VLC 3 hotkeys

**Finding:** Hotkey `ACTIONID_DISC_MENU` does `var_SetInteger(p_input, "title  0", 2)` — a var control on the input thread.  
**File/function/line:** `/tmp/vlc-3.0.23/modules/control/hotkeys.c:970`  
**Source/runtime version:** VLC 3.0.23  
**Evidence excerpt:** `case ACTIONID_DISC_MENU: if( p_input ) var_SetInteger( p_input, "title  0", 2 );`  
**Status:** confirmed  
**Implementation consequence:** VLC 3 has an internal "disc-menu" var control on input thread that triggers title-0 menu. This is not exposed through `libvlc_media_player_navigate` but may be reachable through variable control.  
**Remaining question:** Can we bind `var_SetInteger` from libvlccore to send this control from the media player?

### 4. Title-description ABI and menu/interactive flags

**Finding:** `VlcTitleDescription` struct and `is_menu_title()` are defined in engine.rs.  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri/src/player/engine.rs:89-118`  
**Source/runtime version:** libVLC 3.0.x public `libvlc_title_description_t`  
**Evidence excerpt:**
```rust
pub struct VlcTitleDescription {
    pub i_duration: i64,
    pub psz_name: *mut c_char,
    pub i_flags: u32,
}
pub const INPUT_TITLE_MENU: u32 = 0x01;
pub const INPUT_TITLE_INTERACTIVE: u32 = 0x02;
pub unsafe fn is_menu_title(lib: &VlcLib, mp: usize) -> bool { ... }
```
**Status:** confirmed (source compiles)  
**Implementation consequence:** Flag-based menu classification is available via `is_menu_title()`. Frontend `Poll.at_menu` reflects this.  
**Remaining question:** None — this is working.

### 5. Current return-to-menu implementation

**Finding:** `player_return_to_menu()` does `pause → sleep(50ms) → unpause → navigate_popup(5) → set_title(0)` then checks `title == 0`.  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri/src/commands/media.rs:804-865`  
**Source/runtime version:** libVLC 3.0.23  
**Evidence excerpt:** The function uses `set_title(0)` which is explicitly the title-index shortcut the task says to remove.  
**Status:** incomplete — substitute sequence must be replaced  
**Implementation consequence:** Need to replace with proper navigation that distinguishes popup vs top-menu, uses flag-based verification, and removes pause/sleep.  
**Remaining question:** What root/top-menu mechanism exists for VLC 3 beyond navigate_popup?

### 6. Existing decoder drain/reconfiguration behavior

**Finding:** `player_return_to_menu` and `player_menu_navigate` both use pause/sleep(50ms)/unpause as a "UHD fix" decoder buffer drain before navigation.  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri/src/commands/media.rs:827-834` and `1150-1168`  
**Source/runtime version:** libVLC 3.0.23  
**Evidence excerpt:** 
```rust
unsafe {
    (lib.libvlc_media_player_pause)(mp as *mut std::ffi::c_void, 1);
    std::thread::sleep(std::time::Duration::from_millis(50));
    (lib.libvlc_media_player_pause)(mp as *mut std::ffi::c_void, 0);
}
```
**Status:** confirmed (compile-time), untested at runtime  
**Implementation consequence:** Pause/sleep is a legacy workaround, not a proper drain. Task says to remove it where runtime correction supplies proper handling.  
**Remaining question:** What does proper VT decoder drain look like in VLC 3?

### 7. Runtime patch and build failures

**Finding:** VT-305 patch exists at `patches/vlc-3.0.23/vt-305.patch` but has NOT been compiled into the packaged runtime.  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/patches/vlc-3.0.23/vt-305.patch`  
**Source/runtime version:** VLC 3.0.23 source at `/tmp/vlc-3.0.23/`  
**Evidence excerpt:** Patch adds `VTDecompressionSessionCanAcceptFormatDescription` check before session restart and `videoFormatDescription` release after invalidation.  
**Status:** incomplete — patch written but not built into a runtime binary  
**Build failure context:** 
- VLC 3.0.23 `configure` succeeds with Homebrew bison 3.8.2 (at `/opt/homebrew/opt/bison/bin/bison`)  
- Core libraries `libvlccore.9.dylib` and `libvlc.5.dylib` built successfully  
- Module build fails at `codec/webvtt/CSSGrammar.c` with bison 3.8.2 syntax error (grammar incompatibility)  
- Xib/nib GUI module compilation fails due to Xcode ibtool plugin loading issue  
**Status:** incomplete  
**Remaining question:** Can the specific modules (videotoolbox, caopengllayer) be built independently without the full VLC tree?

### 8. EDR rendering path

**Finding:** `apply_edr_settings()` in `embed_macos.rs` sets `layer.setWantsExtendedDynamicRangeContent(true)` but is flagged as dead code (never called).  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri/src/player/embed_macos.rs:111,337`  
**Source/runtime version:** objc2 bindings  
**Evidence excerpt:** `warning: function apply_edr_settings is never used`  
**Status:** incomplete — flag set but not applied to actual renderer  
**Implementation consequence:** Content-view flag is not the completed HDR implementation. Need dynamic display/headroom bridge and actual video-output EDR support.  
**Remaining question:** How to connect EDR settings to the actual caopengllayer video output?

### 9. Packaging that previously included original runtime

**Finding:** Tauri build produced `Nebulae.app` but runtime modules were not confirmed as patched. The patched `libvideotoolbox_plugin.dylib` and `libcaopengllayer_plugin.dylib` were built at `/tmp/vlc-3.0.23-build/modules/.libs/` but may not have been staged into the final app.  
**File/function/line:** `/Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri/resources/vlc/plugins/`  
**Evidence excerpt:** Modules exist in resources but their provenance (patched vs. original) is unverified.  
**Status:** contradicted (previous report said both built and packaged, but artifacts show only 2 modules in build .libs)  
**Implementation consequence:** Need deterministic staging: source → patches → build → runtime staging → app assembly.  
**Remaining question:** Are the built modules actually in the final app bundle?

---

## WHAT MUST NOT BE REPEATED

### Completed checks (do not redo):

1. **HEAD verified:** `fea47b6c113a6c2afb2862b454c38bcbc664282e` on branch `recovery/macos-playback-20261020`
2. **Syntax error fixed:** `player_return_to_menu` line 849 `== 0` outside block scope — fixed by introducing `title` intermediate variable
3. **Rust compilation:** `cargo build --release` succeeds after syntax fix (warnings only, no errors)
4. **Public navigate enum confirmed:** VLC 3.0 exposes modes 0-5 only; `INPUT_NAV_MENU` (6) is internal-only
5. **Navigate implementation traced:** `libvlc_media_player_navigate` → `input_Control` on input thread
6. **is_menu_title() working:** Flag-based classification compiles and is wired to `poll.at_menu`

### Incomplete checks (last confirmed point → next step):

| Area | Last confirmed point | Next step |
|------|---------------------|-----------|
| Menu navigation | VLC 3 navigate API lacks `INPUT_NAV_MENU` (6); only `navigate_popup` (5) available | Replace `player_return_to_menu` with proper navigation; investigate `var_SetInteger("title  0", 2)` mechanism for root menu |
| VT-305 patch | Patch written, not built | Build patched videotoolbox module independently; stage into runtime |
| HDR/EDR | `apply_edr_settings()` exists but is dead code | Connect to actual video output; add dynamic display bridge |
| Packaging | Modules built in /tmp but provenance unverified in final app | Create deterministic staging script; verify final app contents |
| Runtime build | Core libs built; module build blocked by webvtt bison + ibtool | Build specific modules (videotoolbox, caopengllayer) independently |

---

## MACLC REFERENCE MATERIAL

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

---

## FIRST CONCRETE UNFINISHED STEP

Replace `player_return_to_menu()` in `src-tauri/src/commands/media.rs:804-865`:
- Remove `pause → sleep(50ms) → unpause` workaround
- Remove `set_title(0)` title-index shortcut  
- Remove blind `navigate_popup` + `set_title(0)` issued together
- Implement proper popup vs top-menu distinction using VLC 3 available mechanisms
- Use `is_menu_title()` flag-based verification instead of `title == 0`
- Return meaningful dispatch errors; do not auto-restart disc

<last_confirmed_navigation_trace_point>
VLC 3 `libvlc_media_player_navigate()` in `lib/media_player.c:1798` maps public modes 0-5 to `INPUT_NAV_*` and calls `input_Control()`. `INPUT_NAV_MENU` (value 6, `include/vlc_input.h:434`) is NOT in the public mapping. Hotkey `ACTIONID_DISC_MENU` at `modules/control/hotkeys.c:970` uses `var_SetInteger(p_input, "title  0", 2)` as an alternative disc-menu mechanism.
</last_confirmed_navigation_trace_point>

# Nebulae Windows Player: Technical Handoff and State Documentation

**Source Revisions Described**:
- **macOS Development Checkout**: `989391f255dfd854ea43e173ab9876a73cb9b417` (branch `fix/review-0.2.7`)
- **Windows Build Checkout**: `f564b08bcf7e07088c3eb46b1029b45cc1cfe95b` (branch `fix/review-0.2.7` at `C:\nebulae-factory-droid\repo`)
- **Tested Installer Package**: `Nebulae_0.2.7-candidate-f564b08-fix_x64-setup.exe`
- **Installer SHA-256**: `D45B5C16B98C97DFDD3AB7D310D7BFFED1E7A27476FCBFBCD171D224D30927E7`
- **Published GitHub Remote Baseline**: `c1564addf3115b5298d22924b10b57094cce5233` (`origin/main`)

---

## 1. User-Confirmed Behavior & Runtime Status

1. **Suzume (2022) 4K UHD Blu-ray**:
   - Mouse pointer movement and mouse clicks on interactive menu buttons still fail to highlight or activate menu items.
   - On-screen D-pad and keyboard directional controls (`player_menu_navigate` modes 1–4) navigate menu items, but the user reports errors during interaction; the exact error messages/symptoms are not yet documented.
   - The authored UHD menu reaches Outcome A (menu claim confirmed at T+1604ms), but interactive mouse control remains non-functional.

2. **Fuuka (2017) Blu-ray**:
   - Mouse hover previously worked (menu buttons highlighted on cursor movement in earlier testing).
   - Mouse click activation and subsequent title transitions have not been independently confirmed to pass without issue; do not assume non-hover interactions passed.

3. **Windows IPv6 Connectivity**:
   - Windows IPv6 peers reportedly do not connect, whereas macOS IPv6 connections function.
   - Troubleshooting of IPv6 is explicitly deferred until disc-menu mouse playback is resolved.

4. **Interface Responsiveness**:
   - Perceived interface responsiveness improvements (from eliminating the `WM_WINDOWPOSCHANGED` feedback loop and bounding disc inspection) remain unconfirmed by user testing; no runtime responsiveness claim can be made without measured timing confirmation.

---

## 2. Source Map

### A. Frontend Menu-Mode Detection & Pointer-Event Handlers
- **File**: `src/components/player/PlayerOverlay.tsx`
  - **Menu-Mode Detection**:
    - Lines 340–450: Background polling via `player_poll` updates `pollRef.current.at_menu`.
    - Lines 435–442: Menu entry transition detected via `!wasAtMenuRef.current && p.at_menu`.
    - Lines 443–450: Menu exit transition detected via `wasAtMenuRef.current && !p.at_menu`.
  - **Pointer-Event Wire-up**:
    - Lines 1362–1370: Transparent video hole element (`holderRef`) captures user interactions:
      `onMouseDown={onVideoPointerDown}`, `onMouseMove={onVideoMouseMove}`, `onClick={onVideoClick}`, `onWheel={onVideoWheel}`.
  - **Mouse Move Handler**:
    - Lines 1269–1284 (`onVideoMouseMove`): Fences with `player.isDisc && pollRef.current.at_menu`. Throttles calls to 40ms (~25 fps). Computes surface coordinates via `nativeMenuPoint(e.clientX - rect.left, e.clientY - rect.top)` (scaling by `devicePixelRatio` on Windows). Dispatches through `menuMoveGuardRef.current.dispatch({ ...point, gen: player.gen })`.
  - **Mouse Click Handler**:
    - Lines 1294–1319 (`onVideoClick`): Fences with `player.isDisc && pollRef.current.at_menu`. Throttles clicks to 350ms. Computes `nativeMenuPoint`, invokes `player_menu_mouse_move({ ...point, gen: player.gen })`, invokes `player_menu_mouse_click({ ...point, gen: player.gen })`, and invokes `player_menu_navigate({ mode: 0, gen: player.gen })`.
  - **Mouse Wheel Handler**:
    - Lines 1324–1333 (`onVideoWheel`): Translates scroll delta (`deltaY > 0 ? 2 : 1`) to menu navigation mode and dispatches `player_menu_navigate({ mode, gen: player.gen })`.

### B. Move Guard, Coalescing, Menu Warmup & Click Sequencing
- **File**: `src/lib/menuMoveGuard.ts`
  - Lines 69–74: `MenuPoint` interface carrying `(x, y, gen)`.
  - Lines 103–160 (`createMenuMoveGuard`):
    - Limits outstanding `player_menu_mouse_move` IPC calls to at most one in flight per session.
    - Dropped events during in-flight calls are coalesced into `pending: MenuPoint | null`.
    - When an in-flight call resolves or rejects in `.finally()`, any `pending` point is dispatched immediately.
    - `onMenuExit()` resets `inFlightGen = 0` and clears `pending`.
- **File**: `src/components/player/PlayerOverlay.tsx`
  - Lines 435–442: Warmup move event dispatched to `(width * 0.45, height * 0.42)` on the first tick where `at_menu` becomes true.
  - Lines 1286–1292 (`scheduleVideoPause`): 250ms debounce window using `window.setTimeout` to prevent single-click pause/play toggles from colliding with double-click fullscreen gestures.

### C. Tauri IPC Commands & Session Generation Checks
- **File**: `src-tauri/src/commands/media.rs`
  - Lines 3223–3249 (`player_menu_navigate`):
    - Accepts `(app, state, mode: i32, gen: Option<u64>)`.
    - Validates `gen`: if `Some(g)` does not match `state.session_gen`, drops the request as stale.
    - Calls `(lib.libvlc_media_player_navigate)(mp as *mut c_void, mode)`.
  - Lines 3254–3291 (`player_menu_mouse_move`):
    - Accepts `(app, state, x: f64, y: f64, gen: Option<u64>)`.
    - Validates `gen` against `state.session_gen`.
    - Validates `state.at_menu.load(SeqCst)`.
    - On Windows, delegates to `crate::player::embed_windows::forward_mouse_move(&app, x, y)`.
  - Lines 3296–3323 (`player_menu_mouse_click`):
    - Accepts `(app, state, x: f64, y: f64, gen: Option<u64>)`.
    - Validates `gen` against `state.session_gen`.
    - Validates `state.at_menu.load(SeqCst)`.
    - On Windows, delegates to `crate::player::embed_windows::forward_mouse_click(&app, x, y)`.

### D. Windows Surface Creation, Subclassing & Coordinate Conversion
- **File**: `src-tauri/src/player/embed_windows.rs`
  - Lines 88–140 (`update_video_surface_position`):
    - Reentrancy guard `IN_SURFACE_SYNC: AtomicBool` prevents cascading loops.
    - Fenced by `VIDEO_SURFACE_ACTIVE: AtomicBool`.
    - Compares target dimensions against `GetWindowRect(video_hwnd)` to skip unchanged positioning.
    - Calls `SetWindowPos(video_hwnd, None, cx, cy, cw, ch, SWP_NOACTIVATE | SWP_NOZORDER)`.
    - **Crucial change**: Removed the former `SetWindowPos(main_h, HWND_TOP, ...)` which caused the UI freeze loop.
  - Lines 145–260 (`ensure_video_surface`):
    - Creates `NebulaeVideoSurface` child window beneath the Tauri webview host.
  - Lines 567–580 (`find_vlc_video_main_window`):
    - Recursively inspects child windows of `NebulaeVideoSurface` to locate the native VLC video rendering window (`VLC video output` / `VLC video main`).
  - Lines 590–615 (`map_mouse_lparam`):
    - Unpacks `(x, y)` from `LPARAM`.
    - Invokes `MapWindowPoints(Some(surface), Some(target), &mut [pt])` to convert coordinates between window spaces.
    - Checks `pt` against `GetClientRect(target)` and returns `None` if the cursor lies outside the video bounds.
  - Lines 625–640 (`post_vlc_mouse_move`):
    - Resolves target via `find_vlc_video_main_window`. Maps coordinates via `map_mouse_lparam`.
    - Posts `WM_MOUSEMOVE` via `PostMessageW(target, WM_MOUSEMOVE, wparam, mapped_lparam)`.
  - Lines 642–660 (`post_vlc_mouse_click`):
    - Posts `WM_MOUSEMOVE`, `WM_LBUTTONDOWN` (`MK_LBUTTON`), and `WM_LBUTTONUP` to the resolved VLC target window.
  - Lines 850–875 (`main_wndproc`):
    - Subclasses main Tauri window. Forwards `WM_WINDOWPOSCHANGED` to `update_video_surface_position` only when `VIDEO_SURFACE_ACTIVE` is true.

### E. VLC / libbluray Menu Integration & Maintained Patches
- **Directory**: `patches/vlc-3.0.23/`
  - `bluray-menu-first.patch`: Edits `modules/access/bluray.c` to enforce menu-first initialization for discs with authored menus (`b_menu = true`), exports `libvlc_media_player_get_bluray_menu_open` and `libvlc_media_player_get_bluray_playlist_id`.
  - `navigate-menu.patch`: Adds `libvlc_media_player_navigate_menu()` to dispatch `INPUT_NAV_MENU` directly to the input demuxer.
  - `bluray-playlist-id.patch`: Exposes the active playlist ID from the libbluray demuxer.
  - `d3d11-hdr-sdr-tonemap.patch`: Custom Direct3D 11 tonemapping curve for HDR/UHD media.
  - `patches/libbluray-1.5.0/no-xmlcleanupparser.patch`: Eliminates global `xmlCleanupParser()` calls that broke concurrent XML parsing.
- **Script**: `scripts/apply-vlc-patches.sh`
  - Applies patches in fixed dependency order with zero fuzz (`-F0`), verifying against `patched-sources.sha256`.

### F. Windows Runtime Bundling & Packaging
- **Scripts**:
  - `scripts/windows/stage_runtime.py`: Python orchestrator for Windows runtime staging from pinned `inputs.lock.json` and `d3d11-plugin.lock.json`. Validates PE imports using `pe_audit.py`.
  - `scripts/bundle-vlc-windows.ps1` & `scripts/build-windows.ps1`: Local setup and staging scripts.
  - `src-tauri/resources/build-manifest.json`: Machine-readable provenance manifest recording VLC 3.0.23, libbluray 1.5.0, component SHA-256 hashes, source URLs, and applied patches.
  - `build_deliver_f564b08.ps1`: Build and packaging script executed on `supermicro.local`.

---

## 3. Coordinate Contract & Event Sequence

### Intended Design Contract
1. **Frontend Capture**:
   - The React overlay (`PlayerOverlay.tsx`) paints an overlay with a transparent hole over the video surface.
   - Pointer events occurring over the video hole are captured by `holderRef`.
   - Coordinates are calculated relative to `holderRef.getBoundingClientRect()` and scaled by `window.devicePixelRatio` on Windows to reflect native display pixels.
2. **IPC Forwarding**:
   - Moves are throttled to 40ms and guarded by `menuMoveGuardRef` (max 1 in flight).
   - Mouse clicks throttle to 350ms and dispatch `player_menu_mouse_move`, `player_menu_mouse_click`, and `player_menu_navigate(mode: 0)` in sequence with the session generation attached.
3. **Win32 Message Posting**:
   - `embed_windows.rs` locates the VLC child window (`find_vlc_video_main_window`).
   - `MapWindowPoints` translates the point from `NebulaeVideoSurface` coordinates to the VLC target window coordinates.
   - Coordinates outside the VLC client rectangle are discarded.
   - `PostMessageW` posts `WM_MOUSEMOVE`, `WM_LBUTTONDOWN`, and `WM_LBUTTONUP` directly to VLC's message queue.

### Runtime Divergence & Limitations
- **Intended vs. Verified**:
  - On Blu-ray (Fuuka), hover events reached VLC and highlighted menu items in earlier test sessions.
  - On 4K UHD Blu-ray (Suzume), mouse movement and clicks fail to highlight or activate menu items.
  - Potential causes under investigation:
    1. Direct3D 11 swapchain window hierarchy: On UHD/HDR playback using the custom D3D11 output plugin, the active input-receiving window may differ from the window returned by `find_vlc_video_main_window`.
    2. Coordinate mismatch between presentation display rectangle and libbluray overlay overlay surface.
    3. libbluray UHD/BD-J event delivery: BD-J Java menus (common on UHD) may require keyboard focus or separate input routing distinct from standard HDMV menus.

---

## 4. Summary of Changes in Candidate `f564b08` / `989391f`

### Commit Statistics
- Commit: `989391f255dfd854ea43e173ab9876a73cb9b417` (macOS) / `f564b08bcf7e07088c3eb46b1029b45cc1cfe95b` (Windows)
- Relative to baseline `ff929f4` / `2bcb2be`:
  - `src-tauri/src/commands/media.rs`: +21 lines
  - `src-tauri/src/player/embed_windows.rs`: +171, -112 lines
  - `src-tauri/src/utils/disc_detector.rs`: +5, -1 lines
  - `src/components/media/SimpleMediaView.tsx`: +149, -107 lines
  - `src/components/player/PlayerOverlay.tsx`: +23, -13 lines
  - `src/lib/menuMoveGuard.ts`: +17, -8 lines
  - `src/lib/playerStore.ts`: +175, -94 lines
  - Total: 7 files changed, 342 insertions(+), 219 deletions(-)

### Verified Test Executions
- `tests/menuMouseMoveGuard.test.ts`: Passed (10/10 tests, 157ms, Node.js test runner).
- Rust unit test suite (`cargo test --lib`): Passed (394 passed, 0 failed, 2 ignored).
- Frontend lint & typecheck (`pnpm lint && tsc && vite build`): Passed (0 errors, 10 warnings).
- Clippy (`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`): Passed (0 warnings).
- Windows cargo check (`cargo check` on `supermicro.local`): Passed (0 errors).
- Windows NSIS bundle build: Produced valid setup executable.
- Updater Minisign signature: Validated 100% successful via `verify-payload-signature.exe`.

---

## 5. Provenance of Delivered Candidate Installer

- **File Path**: `C:\Users\administrator\Downloads\Nebulae_0.2.7-candidate-f564b08-fix_x64-setup.exe`
- **File Size**: 340,096,380 bytes
- **File Timestamp (UTC)**: `2026-11-15T16:59:38.6196603Z`
- **File Timestamp (Local)**: `2026-11-15T11:59:38.6196603-05:00`
- **SHA-256**: `D45B5C16B98C97DFDD3AB7D310D7BFFED1E7A27476FCBFBCD171D224D30927E7`
- **Build Script Used**: `C:\nebulae-factory-droid\build_deliver_f564b08.ps1`
- **Build Execution Command**: `powershell -ExecutionPolicy Bypass -File C:\nebulae-factory-droid\build_deliver_f564b08.ps1`
- **Source Commit at Build Time**: `f564b08bcf7e07088c3eb46b1029b45cc1cfe95b`
- **Uncommitted Changes at Build Time**: None. `git status --short` was verified clean immediately prior to building.
- **Authenticode Status**: `NotSigned` (expected for development test candidates; production signing occurs via SignPath).
- **Updater Signature**: Valid Ed25519 payload signature present in `Nebulae_0.2.7-candidate-f564b08-fix_x64-setup.exe.sig`, verified via `verify-payload-signature.exe`.
- **Bundled Runtime Versions**:
  - VLC: `3.0.23` (win64)
  - libbluray: `1.5.0`
  - Transmission: `4.1.3`
  - Tailscale: `1.104.1`
  - OpenJDK JRE: `21.0.12+8` (Adoptium Temurin)
  - Pinned runtime hashes defined in `scripts/windows/inputs.lock.json` and `src-tauri/resources/build-manifest.json`.

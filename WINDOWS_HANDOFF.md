# Nebulae Windows — Per-Defect Handoff Report

**Baseline:** commit `51fb31c` on `main` (pushed to `origin/main` as `51fb31c`; format fix `fa849a4` also pushed)
**Repo:** `koppot/Nebulae-torrent_client`
**Candidate:** `C:\Users\administrator\Nebulae-Windows-Test\windows-candidate/` (GNU-built, checksums verified)
**CI verified:** Windows MSVC compile + unit tests PASSED (runs 39813765173, 39813883006)

---

## Defect 1 — Windows had no EDR retry worker (was macOS-only)

**Cause:** The EDR retry block in `media.rs` was `#[cfg(target_os = "macos")]` only. Windows never retried tone-mapping after the async video layer was created, so HDR/UHD content presented with washed-out colors on Windows.

**Change:** Widened cfg gate to `#[cfg(any(target_os = "macos", target_os = "windows"))]` so Windows gets both the immediate `maybe_apply_edr()` call and the 30ms/3s (100-iteration) retry worker thread.

**Files/commit:**
- `src-tauri/src/commands/media.rs:599-620` — EDR retry block cfg widened
- `src-tauri/src/commands/media.rs:479` — `stop_session` `edr_applied=false` reset cfg widened
- Part of 51fb31c-adjacent change (cfg widening); also `fa849a4` (format fix)

**Verification:** macOS `cargo test` 126 passed; CI `windows-latest` `cargo test --lib` PASSED on MSVC (run 39813883006)

**Included in candidate:** Yes

**Tested:** macOS tested (126 tests). Windows unit tests passed on MSVC CI. End-to-end HDR on Windows video vout **untested** (blocked by Antigravity's missing-picture investigation).

---

## Defect 2 — Windows had no native key forwarding from video overlay

**Cause:** macOS `VideoOverlayView.keyDown` emits `player-native-key` events. Windows had no equivalent — the overlay `STATIC` window had no WNDPROC subclass, so keyboard input on the video surface was not forwarded to the frontend.

**Change:** Added `overlay_wndproc` WM_KEYDOWN subclass on the overlay HWND, plus `windows_key_name(vk, lparam)` mapper (space, Enter, Escape, arrows, printable chars via `GetKeyNameTextW`). The `OVERLAY_WNDPROC` static stores the original wndproc for chaining.

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs:328-417` — `overlay_wndproc`, `windows_key_name`, `OVERLAY_WNDPROC`
- Part of 51fb31c baseline; `fa849a4` (format fix)

**Verification:** CI `windows-latest` `cargo test --lib` PASSED on MSVC (run 39813883006); `overlay_app_handle_returns_none_before_init` unit test passes

**Included in candidate:** Yes

**Tested:** Windows unit tests passed on MSVC CI. Runtime key forwarding **untested** (no playback testing authorized; Windows video picture missing).

---

## Defect 3 — Windows had no window corner radius

**Cause:** macOS applied rounded corners via Core Animation. Windows had no equivalent — the main window had square corners on all builds.

**Change:** Added `apply_window_corner_radius` / `set_window_corner_radius` using `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`. Called at app setup from `lib.rs`. Added `Win32_Graphics_Dwm` to Windows `windows` crate features in `Cargo.toml`.

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs:426-448` — corner radius functions
- `src-tauri/src/lib.rs` — setup call `#[cfg(any(target_os = "macos", target_os = "windows"))]`
- `src-tauri/Cargo.toml` — `Win32_Graphics_Dwm` feature
- Fixed `DWMWCP_ROUND` u32 type mismatch at commit `90da75f`; `PostMessageW` lint fixed at `90da75f`
- `fa849a4` (format fix)

**Verification:** `cargo check --target x86_64-pc-windows-gnu` PASSED (local); CI `windows-latest` `cargo check --lib` PASSED on MSVC (run 39813883006)

**Included in candidate:** Yes

**Tested:** MSVC compile + unit tests passed on CI. Runtime corner radius **untested** (no playback testing).

---

## Defect 4 — Windows had no EDR screen-change observer

**Cause:** macOS `EDRScreenObserver` listens for `NSWindowDidChangeScreenNotification` and re-applies EDR on screen change. Windows had no equivalent — moving the window between HDR/SDR displays left stale tone-mapping state.

**Change:** Added `main_wndproc` WM_DISPLAYCHANGE subclass on the main HWND — resets `edr_applied` latch, re-applies corner radius, emits `player://screen-changed`. `register_display_change_handler` called from `ensure_overlay`. `MAIN_WNDPROC` static stores original wndproc.

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs:393-453` — `main_wndproc`, `MAIN_WNDPROC`, `register_display_change_handler`
- Part of 51fb31c baseline (commit message: "add WM_DISPLAYCHANGE screen-change observer for EDR/tone-mapping re-application"); `fa849a4` (format fix)

**Verification:** CI `windows-latest` `cargo test --lib` PASSED on MSVC (run 39813883006)

**Included in candidate:** Yes

**Tested:** Windows unit tests passed on MSVC CI. Runtime screen-change behavior **untested**.

---

## Defect 5 — Windows video overlay rendered nothing (Direct3D picture missing)

**Cause:** `WS_EX_TRANSPARENT` and `WS_DISABLED` on the overlay HWND prevented Direct3D11 from rendering to the window surface; the overlay sat below the transparent webview and was never raised above it.

**Change:** Removed `WS_EX_TRANSPARENT`/`WS_DISABLED` (commit `60193b4`); raised overlay to `HWND_TOP` via `SetWindowPos` so Direct3D renders directly to screen (commit `0d49e3d` — already on `main` ahead of 51fb31c).

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs` — `attach_main_window_child` (`60193b4` removed styles; `0d49e3d` added `HWND_TOP`)
- `src/components/player/PlayerOverlay.tsx` — bar cutout + native mouse controls (`0d49e3d`)

**Verification:** `cargo check --target x86_64-pc-windows-gnu` PASSED (local); CI MSVC compile passed

**Included in candidate:** `60193b4` yes; `0d49e3d` is on `main` but NOT part of the 51fb31c candidate build

**Tested:** Compile tested on MSVC CI. Runtime video rendering **untested** — Windows disc playback broken (no picture); Antigravity actively investigating; do not modify video-output path.

---

## Defect 6 — HDR / EDR colorspace consumption on Windows video vout (UNPROVEN)

**Cause:** Windows uses Direct3D HWND vout, not Core Animation. The VLC `var_SetChecked` tone-mapping vars (`tone-mapping=3`, `param=1.0`, `desat=0.5`) are set on the media player object, but there is no evidence the Windows video-output path consumes them.

**Change:** `apply_edr_to_video_layer` ports the macOS EDR logic to VLC var calls. VLC args `--tone-mapping=3 --tone-mapping-param=1.0 --tone-mapping-desat=0.5` are passed on all platforms. EDR retry + screen-change observer re-apply on Windows.

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs:93-118` — `apply_edr_to_video_layer`
- `src-tauri/src/player/engine.rs` — VLC tone-mapping args (all platforms)

**Status:** **Unproven** — no end-to-end evidence on Windows

**Corrective change:** **None permitted** — user directed: do not modify Windows player video-output path or related pipeline files Antigravity is working in

**Deferred:** HDR tone-mapping consumption verification — blocked by Antigravity's missing-picture investigation

---

## Defect 7 — Native fullscreen frame toggle (Partial)

**Cause:** macOS has native key forwarding + manual frame toggle. Windows uses Tauri `set_fullscreen()` API only; no Win32 borderless fullscreen (WS style change + adjust window rect).

**Change:** `toggle_fullscreen` in `embed_windows.rs:534-562` uses Tauri API. `0d49e3d` (on `main`) adds `WM_LBUTTONDBLCLK` → `player-native-key` "f" for double-click fullscreen on the overlay.

**Files/commit:**
- `src-tauri/src/player/embed_windows.rs:534-562` — `toggle_fullscreen`
- `src-tauri/src/player/embed_windows.rs` — `WM_LBUTTONDBLCLK` handler (`0d49e3d`)

**Status:** **Partial** — Tauri API covers standard fullscreen; native Win32 borderless not implemented

**Deferred:** Native Win32 borderless fullscreen — lower user impact; Tauri API covers the common case

---

## Summary Table

| # | Defect | Cause → Change | Files | Commit | CI Verified | In Candidate | Tested/Untested |
|---|--------|---------------|-------|--------|-------------|--------------|-----------------|
| 1 | No EDR retry on Windows | cfg macOS-only → widened to any(macos, windows) | `media.rs` | 51fb31c-adjacent | ✅ MSVC test passed | Yes | Unit tested; runtime untested |
| 2 | No native key forwarding | No WNDPROC subclass → added `overlay_wndproc` + `windows_key_name` | `embed_windows.rs` | 51fb31c | ✅ MSVC test passed | Yes | Unit tested; runtime untested |
| 3 | No corner radius | No DwmSetWindowAttribute → added corner radius fns | `embed_windows.rs`, `lib.rs`, `Cargo.toml` | 51fb31c + `90da75f` | ✅ MSVC check passed | Yes | Compile tested; runtime untested |
| 4 | No screen-change observer | No WM_DISPLAYCHANGE → added `main_wndproc` | `embed_windows.rs` | 51fb31c | ✅ MSVC test passed | Yes | Unit tested; runtime untested |
| 5 | No video picture | WS_EX_TRANSPARENT/WS_DISABLED → removed; HWND_TOP added | `embed_windows.rs`, `PlayerOverlay.tsx` | `60193b4`, `0d49e3d` | ✅ MSVC check passed | `60193b4` yes; `0d49e3d` no (ahead of 51fb31c) | Compile tested; runtime untested (Antigravity) |
| 6 | HDR colorspace unproven | VLC vars set but Windows vout consumption unproven | `embed_windows.rs`, `engine.rs` | 51fb31c | N/A | Yes | **Blocked** — do not touch video path |
| 7 | Native fullscreen partial | Tauri API only; no Win32 borderless | `embed_windows.rs` | 51fb31c + `0d49e3d` | ✅ MSVC check passed | Partial | Compile tested; runtime untested |

---

## Verification Evidence

| Check | Toolchain | Result | Source |
|-------|-----------|--------|--------|
| `cargo check --lib` | MSVC (CI `windows-latest`) | PASSED | Run 39813883006 (fa849a4) |
| `cargo test --lib` | MSVC (CI `windows-latest`) | PASSED | Run 39813883006 (fa849a4) |
| `cargo fmt --check` | Linux (CI `ubuntu-latest`) | PASSED | Run 39813883006 (fa849a4) |
| `cargo check --target x86_64-pc-windows-gnu` | GNU (local) | PASSED (1 pre-existing warning) | Local Supermicro |
| `cargo build --release --target x86_64-pc-windows-gnu` | GNU (local) | PASSED | Local Supermicro |
| macOS `cargo check` / `cargo clippy` / `cargo test` | macOS | All PASSED (126 tests) | Local macOS |

---

## Candidate Package Checksums (verified)

| File | SHA256 | Match |
|------|--------|-------|
| `nebulae.exe` | `5363980c854567132d47d13c1818199380780d6c0ec02be2954d688c6a658f7d` | ✅ |
| `nebulae_torrent_client_lib.dll` | `65b4daeb6479ce7f511d89c5be67e999dd000d98d36bc5a28caf6d877c37c7c5` | ✅ |
| `WebView2Loader.dll` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` | ✅ |

Location: `C:\Users\administrator\Nebulae-Windows-Test\windows-candidate/`
Built from clean target at commit `51fb31c` (GNU toolchain).

---

## Constraints Honored

- No public release published
- v0.2.3 tag not moved
- No signing config/secrets changed
- No playback testing performed
- Windows player video-output path not modified (Antigravity's domain)
- macOS behavior preserved; all existing work intact

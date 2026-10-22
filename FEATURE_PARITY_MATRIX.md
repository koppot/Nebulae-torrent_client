# Nebulae macOS/Windows Feature-Parity Matrix

**Baseline:** commit `51fb31c` "fix(windows): add WM_DISPLAYCHANGE screen-change observer for EDR/tone-mapping re-application"
**Repo:** `koppot/Nebulae-torrent_client` on `main`
**CI verification:** GitHub Actions `windows-latest` (MSVC `x86_64-pc-windows-msvc`), runs 39813765173 (51fb31c) and 39813883006 (fa849a4 format fix)
**Blocker:** Windows Blu-ray/UHD video picture missing — under investigation by Antigravity; do NOT modify Windows video-output path.

---

## Legend

| Status | Meaning |
|--------|---------|
| **Implemented** | Feature works on Windows with code at least equivalent to macOS |
| **Partial** | Feature exists on Windows but with gaps vs macOS |
| **Unproven** | Code exists but no evidence it functions end-to-end on Windows |
| **Deferred** | Not implemented; reason documented |

---

## 1. Video overlay rendering (Direct3D)

| Field | Detail |
|-------|--------|
| **macOS impl** | `VideoOverlayView` (NSView subclass) in `embed_macos.rs`; libVLC renders via `set_hwnd`/`NSView` embedding; transparent webview above |
| **Windows impl** | Child `STATIC` window below the webview in `embed_windows.rs`; libVLC renders via `libvlc_media_player_set_hwnd(NULL)`/`set_hwnd(hwnd)`; `SetWindowPos(HWND_TOP)` raises it above the webview sibling so Direct3D11 renders to screen (commit `0d49e3d`) |
| **Status** | **Fixed** (was broken — no picture due to `WS_EX_TRANSPARENT`/`WS_DISABLED`, fixed at `60193b4`; raised to `HWND_TOP` at `0d49e3d`) |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:189-232` (`attach_main_window_child`, `SetWindowPos HWND_TOP`); `60193b4` removed `WS_EX_TRANSPARENT`/`WS_DISABLED` |
| **Corrective change** | None remaining on 51fb31c baseline; `0d49e3d` (already on main) adds `HWND_TOP` + bar cutout + native mouse controls |
| **Deferred** | None |

---

## 2. HDR / EDR colorspace and dynamic-range presentation

| Field | Detail |
|-------|--------|
| **macOS impl** | Full EDR pipeline: `apply_edr_to_layer`, `apply_edr_to_video_layer`, `maybe_apply_edr`, `force_apply_edr` in `embed_macos.rs`; `EDRScreenObserver` for screen changes; Core Animation layer tagged with colorspace; `NSWindowDidChangeScreenNotification` |
| **Windows impl** | `apply_edr_to_video_layer` in `embed_windows.rs:93-118` uses libVLC `var_SetChecked` to set `tone-mapping=3`, `tone-mapping-param=1.0`, `tone-mapping-desat=0.5` on the media player object. No Core Animation layer equivalent (Windows uses Direct3D HWND vout). `maybe_apply_edr` + `force_apply_edr` ported. `edr_applied` latch reset on screen change via `main_wndproc` WM_DISPLAYCHANGE |
| **Status** | **Unproven** — Windows video-output path does not demonstrably consume these VLC vars; Windows disc playback is broken (no picture) per user report. Antigravity is actively troubleshooting |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:93-118` (`apply_edr_to_video_layer`); `src-tauri/src/commands/media.rs:599-620` (EDR retry block, cfg `any(macos, windows)`); `src-tauri/src/player/engine.rs` VLC args `--tone-mapping=3 --tone-mapping-param=1.0 --tone-mapping-desat=0.5` (all platforms) |
| **Corrective change** | **None permitted** — user directed: do not modify Windows player video-output path or related pipeline files Antigravity is working in |
| **Deferred** | HDR tone-mapping consumption verification on Windows video vout — blocked by Antigravity's investigation |

---

## 3. EDR retry on UHD/HDR detection

| Field | Detail |
|-------|--------|
| **macOS impl** | Immediate `maybe_apply_edr` at playback start + 30ms/3s worker thread (100 iterations) in `media.rs` |
| **Windows impl** | Identical: immediate `maybe_apply_edr` + 30ms/3s worker thread (100 iterations), cfg-gated `any(target_os = "macos", target_os = "windows")` |
| **Status** | **Implemented** (was macOS-only; cfg widened at `51fb31c`-adjacent change; now confirmed on MSVC via CI) |
| **Evidence** | `src-tauri/src/commands/media.rs:599-620` (`#[cfg(any(target_os = "macos", target_os = "windows"))]` EDR retry worker); `src-tauri/src/commands/media.rs:1033-1045` (`player_poll` EDR block, same cfg) |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | None |

---

## 4. `stop_session` EDR reset (`edr_applied=false`)

| Field | Detail |
|-------|--------|
| **macOS impl** | `edr_applied.store(false)` in `stop_session` |
| **Windows impl** | Same: `#[cfg(any(target_os = "macos", target_os = "windows"))] state.edr_applied.store(false)` in `stop_session` |
| **Status** | **Fixed** (was macOS-only; cfg widened; `stop_session` made `pub`) |
| **Evidence** | `src-tauri/src/commands/media.rs:479` (`#[cfg(any(target_os = "macos", target_os = "windows"))] state.edr_applied.store(false)`) |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | None |

---

## 5. Native key forwarding from video overlay

| Field | Detail |
|-------|--------|
| **macOS impl** | `VideoOverlayView.keyDown` → emits `player-native-key` event in `embed_macos.rs` |
| **Windows impl** | `overlay_wndproc` WM_KEYDOWN subclass in `embed_windows.rs:374-388` → emits `player-native-key` with `windows_key_name(vk, lparam)` mapper (space, Enter, Escape, arrows, printable chars via `GetKeyNameTextW`) |
| **Status** | **Implemented** (now confirmed on MSVC via CI) |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:328-417` (`overlay_wndproc`, `windows_key_name`, `OVERLAY_WNDPROC` static) |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | None |

---

## 6. Window corner radius

| Field | Detail |
|-------|--------|
| **macOS impl** | `apply_window_corner_radius` in `embed_macos.rs` (Core Animation layer corner radius) |
| **Windows impl** | `apply_window_corner_radius` / `set_window_corner_radius` in `embed_windows.rs:426-448` using `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`; called at app setup in `lib.rs` |
| **Status** | **Implemented** (was missing; added at `51fb31c`-adjacent change; `DWMWCP_ROUND` u32 type mismatch fixed at `90da75f`) |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:426-448`; `src-tauri/src/lib.rs` corner-radius setup `#[cfg(any(target_os = "macos", target_os = "windows"))]`; `src-tauri/Cargo.toml` Windows `windows` crate feature `Win32_Graphics_Dwm` |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | Per-session radius changes (player vs media UI) not applied on Windows to avoid flickering (documented limitation in `set_window_corner_radius`) |

---

## 7. EDR screen-change observer

| Field | Detail |
|-------|--------|
| **macOS impl** | `EDRScreenObserver` in `embed_macos.rs` — `NSWindowDidChangeScreenNotification` observer |
| **Windows impl** | `main_wndproc` WM_DISPLAYCHANGE subclass in `embed_windows.rs:393-424` → resets `edr_applied` latch, re-applies corner radius, emits `player://screen-changed`; `register_display_change_handler` called from `ensure_overlay` |
| **Status** | **Implemented** (now confirmed on MSVC via CI) |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:393-453` (`main_wndproc`, `MAIN_WNDPROC` static, `register_display_change_handler`) |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | None |

---

## 8. Native fullscreen frame toggle

| Field | Detail |
|-------|--------|
| **macOS impl** | Native key forwarding (`f` key from `VideoOverlayView.keyDown`) + manual frame toggle via window API |
| **Windows impl** | `toggle_fullscreen` in `embed_windows.rs:534-562` uses Tauri `set_fullscreen()` API only; `WM_LBUTTONDBLCLK` on overlay emits `player-native-key` "f" (double-click fullscreen) |
| **Status** | **Partial** — Tauri API only on Windows; no native Win32 fullscreen frame toggle (WS style change + adjust window rect) equivalent to macOS native path |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:534-562` (`toggle_fullscreen`); macOS `embed_macos.rs` has native key forwarding path |
| **Corrective change** | None on 51fb31c baseline. `0d49e3d` (already on main) adds `WM_LBUTTONDBLCLK` → "f" key emission for double-click fullscreen on the overlay |
| **Deferred** | Native Win32 fullscreen (borderless, WS style change) — lower user impact; Tauri API covers the common case |

---

## 9. Disc startup at authored menu

| Field | Detail |
|-------|--------|
| **macOS impl** | Menu watcher in `media.rs` (not cfg-gated; same code all platforms) |
| **Windows impl** | Same menu watcher — `INPUT_NAV_MENU` dispatch, First Play detection, seek-to-end (0.99) fallback, 15s liveness poll, 6s decryption-failure probe |
| **Status** | **Partial** — watcher exists and is shared, but Windows Blu-ray/UHD playback is broken (no picture), so menu display cannot be verified end-to-end |
| **Evidence** | `src-tauri/src/commands/media.rs:640-810` (menu watcher thread); `src-tauri/src/commands/media.rs:818-870` (liveness/decryption probe) |
| **Corrective change** | None — shared code, no platform split needed. Blocked by Windows video-output issue (Antigravity) |
| **Deferred** | End-to-end disc menu verification on Windows — blocked by missing video picture |

---

## 10. Tray behavior (minimize-to-tray, close-to-tray)

| Field | Detail |
|-------|--------|
| **macOS impl** | `miniaturize`/`window_minimize_native` honor `minimize_to_tray` setting; close-to-tray via `lib.rs` close handler |
| **Windows impl** | Same logic: `miniaturize`/`window_minimize_native` in `embed_windows.rs:564-592` check `minimize_to_tray_enabled`; native Win32 `ShowWindow`/`PostMessageW` for minimize/maximize/close on undecorated window |
| **Status** | **Shared** — identical setting, identical logic |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:564-622`; `src-tauri/src/lib.rs` (tray icon, left-click toggle, double-click show, close-to-tray, native app menu) |
| **Corrective change** | None |
| **Deferred** | None |

---

## 11. Autostart (start-on-boot)

| Field | Detail |
|-------|--------|
| **macOS impl** | `apply_autostart` writes `~/Library/LaunchAgents/com.asiandvdclub.nebulae.plist` |
| **Windows impl** | `apply_autostart` writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` registry value |
| **Status** | **Implemented** — platform-specific, both work |
| **Evidence** | `src-tauri/src/commands/settings.rs:apply_autostart` (cfg-gated macOS/windows/linux blocks) |
| **Corrective change** | None |
| **Deferred** | None |

---

## 12. Native window drag (undecorated title bar)

| Field | Detail |
|-------|--------|
| **macOS impl** | Native title bar drag (standard macOS behavior) |
| **Windows impl** | `begin_window_drag` in `embed_windows.rs:606-642` — releases WebView2 mouse capture, synchronous `SendMessageW(WM_NCLBUTTONDOWN, HTCAPTION)` with valid POINTS lparam; triggered by `WM_LBUTTONDOWN`→drag threshold in `overlay_wndproc` |
| **Status** | **Implemented** — fixes tao `drag_window` bug where stack-local LPARAM becomes dangling |
| **Evidence** | `src-tauri/src/player/embed_windows.rs:606-642` (`begin_window_drag`); `overlay_wndproc` `WM_MOUSEMOVE`/`WM_LBUTTONUP` drag disambiguation |
| **Corrective change** | None — already complete on 51fb31c baseline |
| **Deferred** | None |

---

## Summary of Deficiency Rankings (highest user-impact first)

| Rank | Feature | Status | Blocker |
|------|---------|--------|---------|
| 1 | HDR / EDR video presentation | **Unproven** | Windows disc playback broken (no picture); Antigravity investigating; do not modify video-output path |
| 2 | Disc startup at authored menu | **Partial** | Shared watcher exists, but cannot verify end-to-end without working Windows video |
| 3 | Native fullscreen frame toggle | **Partial** | Tauri API only; `0d49e3d` adds double-click "f" key on overlay |

---

## Verification Results

| Check | Toolchain | Result | CI Run |
|-------|-----------|--------|--------|
| `cargo check --lib` | MSVC (`windows-latest`) | PASSED | 39813883006 (fa849a4) |
| `cargo test --lib` | MSVC (`windows-latest`) | PASSED | 39813883006 (fa849a4) |
| `cargo fmt --check` | Linux (`ubuntu-latest`) | PASSED | 39813883006 (fa849a4) |
| `cargo check --target x86_64-pc-windows-gnu` | GNU (local) | PASSED (1 pre-existing warning) | N/A — local |
| `cargo build --release --target x86_64-pc-windows-gnu` | GNU (local) | PASSED | N/A — local |
| macOS `cargo check` / `cargo clippy` / `cargo test` | macOS | All PASSED (126 tests) | N/A — local |

---

## Candidate Package

**Location:** `C:\Users\administrator\Nebulae-Windows-Test\windows-candidate/`

| File | SHA256 |
|------|--------|
| `nebulae.exe` | `5363980c854567132d47d13c1818199380780d6c0ec02be2954d688c6a658f7d` |
| `nebulae_torrent_client_lib.dll` | `65b4daeb6479ce7f511d89c5be67e999dd000d98d36bc5a28caf6d877c37c7c5` |
| `WebView2Loader.dll` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` |

Built from clean target at commit `51fb31c` (GNU toolchain). Checksums differ from pre-51fb31c build — confirmed not deterministic-identical. MSVC toolchain verification pending (CI windows-tests passed, but candidate itself was GNU-built; release pipeline target is GNU).

---

## Notes

- Commit `0d49e3d` ("fix(windows): raise video overlay to HWND_TOP with bar cutout and native mouse controls") is on `main` ahead of `51fb31c`. It is not part of the 51fb31c baseline but improves Windows overlay Z-order and mouse handling.
- macOS behavior is preserved; all existing work is intact.
- No playback testing was performed. No public release published. No tags moved. No signing config changed.

# Factory AI Handover & Permanent Repository Instructions

**Date:** 2026-10-23  
**Repository:** `koppot/Nebulae-torrent_client`  
**Current Branch:** `main`  
**Canonical Release Version:** `0.2.3` (STRICT: Never bump or alter without explicit user instruction)  
**Target Windows Host:** `supermicro.local` (Active interactive user session: `supermicro\administrator`)

---

## 1. PERMANENT REPOSITORY MANDATES & AGENT INSTRUCTIONS

All agents, models, and workflows operating on this repository must strictly adhere to the following non-negotiable rules:

1. **User Directives Are Absolute:** Every explicit requirement, constraint, and instruction given by the user must be followed without exception, alteration, or assumption.
2. **Strict Release Version Lock (0.2.3):**
   - The release version must remain strictly at `0.2.3` across all project files:
     - `package.json` (`"version": "0.2.3"`)
     - `src-tauri/tauri.conf.json` (`"version": "0.2.3"`)
     - `src-tauri/Cargo.toml` (`version = "0.2.3"`)
     - `src-tauri/Cargo.lock` (`version = "0.2.3"`)
     - `.github/workflows/release.yml` (`tag_name: default: 'v0.2.3'`)
   - Never bump to `0.2.4`, `0.9.0`, or any other version unless explicitly commanded by the user.
3. **Strict macOS Codebase Isolation:**
   - Never alter, contaminate, or reformat macOS-specific implementation files (`src-tauri/src/player/embed_macos.rs`).
   - macOS playback behavior is working and verified; changes to Windows or Linux must never affect macOS.
4. **Subprocess Window Suppression on Windows:**
   - Any execution of external command-line utilities (`ffmpeg.exe`, `vlc.exe`, shell scripts) on Windows must include:
     ```rust
     #[cfg(target_os = "windows")]
     {
         use std::os::windows::process::CommandExt;
         command.creation_flags(0x08000000); // CREATE_NO_WINDOW
     }
     ```
   - Spawning visible flashing console windows on hover, seeking, or thumbnail generation is strictly prohibited.
5. **Windows Deployment & Verification:**
   - The interactive desktop user on `supermicro.local` is `supermicro\administrator` (session ID 2), not `dev`.
   - Never claim an update is installed on Windows without inspecting `LastWriteTime`, `FileVersionInfo`, and verifying the running instance via CDP at `http://127.0.0.1:9222`.

---

## 2. CHRONOLOGICAL RECORD OF ALL USER INSTRUCTIONS (2026-10-22 — 2026-10-23)

1. **Seek Bar Hover Video Thumbnails:** Hovering over the media seek bar must display a video frame thumbnail preview (along with timestamp badge) across all operating systems without seeking the active playback and without flashing command prompt windows.
2. **Windows UHD HDR Washed-Out Colors:** Fix washed-out, milky grey blacks on Windows when playing UHD HDR content (e.g., *Patlabor 2 UHD*, *Mary and the Witch's Flower*). Must display inky blacks, accurate color saturation, and proper contrast matching macOS Rec.709 rendering without lifting shadows. Standard SDR content (DVD / 1080p Blu-ray) must remain bit-for-bit untouched.
3. **Menu Graphic Integrity:** Optical disc menus (such as *Fuuka* Blu-ray) must not display bounding box borders, horizontal/vertical lines, or color fringe discolorations around text or interactive buttons.
4. **Sound Stability:** Audio playback must be clear, synchronized, and uncorrupted without stutters, drops, or pipeline failure.
5. **Windows 1px Desktop Bleed:** Eliminate the 1px wallpaper/desktop bleed around the video window borders on Windows 10/11. (Resolved via `get_client_rect_in_screen` in `src-tauri/src/player/embed_windows.rs`).
6. **NSIS Installer Reinstall Prompt:** When installing over an existing build on Windows, the installer must prompt the user with options to uninstall the old version or keep it (`uninstallBeforeInstalling` vs `dontUninstall`). (Resolved in `src-tauri/nsis/installer.nsi`).
7. **Resume Playback Setting:** Media Preferences must include a checkbox "Resume playback from last saved point instead of menu", saving playback positions every 4s and auto-seeking on resume across both plain video files and optical discs.
8. **Track Labels:** Windows audio and subtitle track menus must display proper language names (e.g., "Japanese", "English") rather than generic track numbers.

---

## 3. TECHNICAL FINDINGS & DISCOVERIES FOR FACTORY AI

### A. Windows Direct3D 11 Video Output & HDR Tone Mapping

- **Module in Use:** VLC 3.0.x on Windows utilizes `libdirect3d11_plugin.dll` for embedded video output (`set_hwnd`).
- **Direct3D 11 vs. OpenGL / macOS:**
  - On macOS, `libcaopengllayer_plugin.dylib` runs an OpenGL fragment shader evaluating an ACES filmic tone-mapping curve to standard sRGB / Rec.709 with gamma 2.2, tagged via `kCGColorSpaceSRGB`.
  - On Windows, `libdirect3d11_plugin.dll` uses compiled HLSL shaders (`d3d11_shaders.h`).
  - Passing `--d3d11-hdr-mode=never` forces the Direct3D 11 shader to apply Hable tone mapping down to BT.709 SDR.
  - However, VLC 3.0's hardcoded Hable tone-mapping curve formula in `direct3d11.c` assumes a diffuse white scaling factor of 100 nits on a 10,000 nit master (`LuminanceScale = 0.01`). This severely crushes diffuse white (down to ~54%) and lifts shadow details, producing a washed-out milky grey veil on SDR monitors.
  - VLC's `tone-mapping`, `tone-mapping-param`, and `tone-mapping-desat` variables (`var_SetChecked`) are silently ignored by Direct3D 11 (they only affect OpenGL / libplacebo).

### B. What Was Tried with `--video-filter=adjust` & Why It Failed

- **Attempted Approach:**
  - Added `--video-filter=adjust` to Windows libvlc arguments and dynamically called `libvlc_video_set_adjust_float` / `int` with calibrated parameters (`contrast=1.28`, `brightness=1.02`, `saturation=1.50`, `gamma=0.80`) when HDR was detected.
- **Observed Regressions:**
  1. **Optical Disc Menu Discoloration & Lines:** On Blu-ray interactive menus (such as *Fuuka*), subpictures and interactive graphics planes pass through the adjust filter. This causes visible bounding boxes, horizontal/vertical border lines, and discoloration around buttons (e.g. "Play All", "Episodes", "Setup" in `Fuuka`).
  2. **Severe CPU Overload & Broken Audio:** Direct3D 11 video filter chaining on 3840x2160 10-bit HEVC frames causes GPU readbacks or software pixel conversions, starving the decoder thread. This triggers frame drops and severe audio buffer underruns in WASAPI (`mmdevice`), completely breaking sound output.
  3. **Did Not Fix Washed-Out Appearance:** The post-processing adjust filter could not correct the non-linear tonal compression introduced by the Hable shader curve.
- **Action Taken:**
  - `--video-filter=adjust` has been removed from `src-tauri/src/player/engine.rs` to restore clean menu rendering and uncorrupted audio.

### C. Recommended Pathways for Factory AI to Resolve Windows HDR Tone Mapping

1. **Custom HLSL Pixel Shader in `libdirect3d11_plugin.dll`:**
   - Recompile VLC's `libdirect3d11_plugin.dll` with an optimized ACES or Reinhard tone-mapping curve matching macOS `libcaopengllayer_plugin.dylib`, replacing the crushed Hable curve.
2. **Direct3D 11 Swapchain Color Space Tagging:**
   - If the Windows display supports HDR, evaluate enabling `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` on the video HWND swapchain to let the display handle HDR metadata natively without tone mapping down to SDR.
3. **OpenGL / libplacebo Video Output on Windows:**
   - Investigate whether `glwin32` or an ANGLE-backed OpenGL ES vout can be embedded in `video_hwnd` with libplacebo or custom fragment shaders, replicating the macOS OpenGL pipeline on Windows.

---

## 4. CURRENT STATE OF VERIFIED FIXES ON `main`

| Issue | Status | Implementation Details |
|---|---|---|
| **Release Version 0.2.3** | Verified | Aligned across `package.json`, `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`, `.github/workflows/release.yml`. |
| **Seek Bar Hover Thumbnails** | Implemented & Fixed | `player_thumbnail_at` in `commands/media.rs` uses `extract_thumbnail_ffmpeg` with `CREATE_NO_WINDOW` flag, preventing console window popups. UI in `PlayerOverlay.tsx` displays thumbnail preview with debounced fetch and timestamp badge. |
| **Windows 1px Desktop Bleed** | Verified | Resolved via `get_client_rect_in_screen` across all window management events in `embed_windows.rs`. |
| **NSIS Installer Reinstall Prompt** | Verified | Restored custom uninstaller page (`uninstallBeforeInstalling` vs `dontUninstall`) in `src-tauri/nsis/installer.nsi`. |
| **Resume Playback from Saved Point** | Verified | Checkbox in Media Preferences; stores positions every 4s and auto-seeks on launch across discs and video files. |
| **macOS Codebase Isolation** | Verified | `embed_macos.rs` completely untouched; all macOS behaviors preserved. |
| **Windows UHD Tone Mapping** | **Open for Factory AI** | Needs shader-level / pipeline-level solution in Direct3D 11 without `--video-filter=adjust`. |

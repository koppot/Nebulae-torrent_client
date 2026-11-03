# Nebulae 0.2.4 Windows Modified VLC Runtime Build Specification & Source Reconciliation

**Document ID:** `SPEC-WIN-VLC-0.2.4-REV1`  
**Date:** 2026-10-29  
**Authors:** Antigravity (Windows Build/Packaging Owner)  
**Reviewer:** Factory B (Runtime Architecture & Source Reconciliation Owner)  
**Linear Issue:** [`ASI-15`](https://linear.app/asiandvdclub/issue/ASI-15/windows-x64-024-evaluation-repair-and-installer-factory-b)  
**Notion Handoff:** [Nebulae — Shared Engineering Handoff](https://app.notion.com/p/406edf7148ed8163bd6aeedda9ffa170)

---

## 1. Executive Summary & Purpose

The initial candidate installer (`Nebulae_0.2.4_x64-setup.exe`, SHA-256 `30a8742c...`) built on Supermicro substituted an unmodified VideoLAN VLC 3.0.23 runtime overlaid with only two rebuilt DLLs (`libvlc.dll` and `liblibbluray_plugin.dll`). This candidate has been formally **rejected for release** because stock VLC is not an acceptable substitute for Nebulae's required playback behavior.

This document establishes the verified source baseline, presents the 100% source reconciliation against pristine upstream archives, clarifies the Windows rendering and HDR architecture, and defines the reviewable build input package for producing a fully consistent, modified Windows runtime.

---

## 2. Upstream Source Baseline & Pinned Checksums

The complete runtime is constructed from pinned, checksum-verified upstream sources. All archives are preserved locally under `/Users/dev/Documents/GitHub/Nebulae-factory-repair/src-tauri/vlc-runtime-build/downloads/` and `/Users/dev/nebulae-deps-20261028/`:

| Component | Pinned Version | Upstream Source URL | Algorithm & Checksum |
| :--- | :--- | :--- | :--- |
| **VLC Media Player** | `3.0.23` | `https://download.videolan.org/pub/videolan/vlc/3.0.23/vlc-3.0.23.tar.xz` | **SHA-256:** `e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0` |
| **libbluray** | `1.5.0` | `https://download.videolan.org/pub/videolan/libbluray/1.5.0/libbluray-1.5.0.tar.xz` | **SHA-256:** `f676408e91a5d321abf8b8d4dfdae36205c297dab5c54c3ec519639025f474a2` |
| **Apache Ant** (Build Tool) | `1.10.18` | `https://archive.apache.org/dist/ant/binaries/apache-ant-1.10.18-bin.tar.xz` | **SHA-512:** `3cc0c48a744dd21e8e7fed30c555e8efcac709839c527c57600ab6d7d02383207d7c619d434bb1c9b5ff3f956c77a0c587b1dcfbf3c72ec9d78300caf82c0567` |
| **VideoLAN VLC Contribs** | `3.0.23 win64` | `https://get.videolan.org/vlc/3.0.23/win64/vlc-3.0.23-win64.zip` | **SHA-256:** `992d19dbd0b8a7cde9167d2f7780b1ef6f92acc8a71acfa736101a21f35181e1` |
| **Temurin OpenJDK JRE** | `21.0.12+8` | `https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.12%2B8/OpenJDK21U-jre_x64_windows_hotspot_21.0.12_8.zip` | **SHA-256:** `b8aa18fef5edb69bee8618f99677d66d0873d22cb40d974c15ac9ffcdecf73ba` |
| **GeoLite2 Country DB** | `2026-10` | MaxMind GeoLite2 Country database | **SHA-256:** `54e327d9fe9214e568c5b8f8ce97a7f7e7087b7de10b0560640417695d153797` |
| **GeoLite2 ASN DB** | `2026-10` | MaxMind GeoLite2 ASN database | **SHA-256:** `ab93d4ebe5287ab9a459bcd67f3d5b358e261666c16fd246543c8ba771a78bf4` |

---

## 3. Source Reconciliation Results

A full recursive diff was executed comparing the pristine, unpacked upstream tarballs against the active build trees used for the accepted Mac runtime (`/Users/dev/Documents/GitHub/Nebulae-factory-repair/src-tauri/vlc-runtime-build/`):

### 3.1 VLC 3.0.23 Source Reconciliation
- **Result:** Exactly 9 files differ from the pristine VideoLAN tarball. Every single difference is accounted for by the 5 patches in `patches/vlc-3.0.23/`:
  1. `include/vlc/libvlc_media_player.h` (SHA-256: `de3cc083...`)
  2. `lib/libvlc.sym` (SHA-256: `5e3f55d4...`)
  3. `lib/media_player.c` (SHA-256: `f70a9a1f...`)
  4. `modules/access/bluray.c` (SHA-256: `d33786ae...`)
  5. `modules/codec/videotoolbox.m` (SHA-256: `ef722854...`)
  6. `modules/video_output/caopengllayer.m` (SHA-256: `49a689fb...`)
  7. `modules/video_output/opengl/converter.h` (SHA-256: `e79a80ec...`)
  8. `modules/video_output/opengl/fragment_shaders.c` (SHA-256: `bc5bbc70...`)
  9. `modules/video_output/opengl/vout_helper.c` (SHA-256: `01a3d07d...`)
- **Conclusion:** There are **zero undocumented C/C++ patches** in VLC. The 5 patches in `patches/vlc-3.0.23/` constitute 100% of the modified source baseline.

### 3.2 libbluray 1.5.0 Reconciliation
- **Result:** A recursive diff between pristine `libbluray-1.5.0.tar.xz` and the build tree returned **0 differences**.
- **Conclusion:** The native `libbluray` library is 100% standard VideoLAN libbluray 1.5.0 without custom C patches. The customization is in VLC's demuxer (`modules/access/bluray.c`), which links to libbluray 1.5.0, and the companion compilation of `libbluray-j2se-1.5.0.jar`.

---

## 4. Patch-to-Source-to-Output Matrix

The following matrix documents every patch in the series, its technical purpose, platform scope, and affected Windows build outputs:

| Patch Name | Source Files Changed | Intended Behavior | Platform Applicability | Windows Target Output | Status in Candidate |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`vt-305.patch`** | `modules/codec/videotoolbox.m` | Prevents VideoToolbox error 305 on format transitions by reusing valid sessions. | **macOS Only** | None (Windows uses D3D11/DXVA2) | **Platform Specific** |
| **`navigate-menu.patch`** | `include/vlc/libvlc_media_player.h`<br>`lib/libvlc.sym`<br>`lib/media_player.c` | Exposes `libvlc_media_player_navigate_menu()` to directly send `INPUT_NAV_MENU` to input thread. | **Cross-platform** | `libvlc.dll` | **Static implementation evidence** |
| **`hdr-tonemap.patch`** | `modules/video_output/caopengllayer.m`<br>`modules/video_output/opengl/converter.h`<br>`modules/video_output/opengl/fragment_shaders.c`<br>`modules/video_output/opengl/vout_helper.c` | Adds GLSL ITU-R BT.2390 tone-mapping and BT.709 gamut mapping shaders (`vlc_pq_to_nits`, `vlc_hdr_tonemap`). | **Output Dependent** (OpenGL pipeline) | `plugins/video_output/libgl_plugin.dll` (if OpenGL is used) | **Known missing** (Stock D3D11 & GL reused) |
| **`bluray-playlist-id.patch`** | `include/vlc/libvlc_media_player.h`<br>`lib/libvlc.sym`<br>`lib/media_player.c`<br>`modules/access/bluray.c` | Publishes active MPLS ID on `BD_EVENT_PLAYLIST` to input thread; adds `libvlc_media_player_get_bluray_playlist_id()`. | **Cross-platform** | `libvlc.dll`<br>`plugins/access/liblibbluray_plugin.dll` | **Static implementation evidence** |
| **`bluray-menu-first.patch`** | `include/vlc/libvlc_media_player.h`<br>`lib/libvlc.sym`<br>`lib/media_player.c`<br>`modules/access/bluray.c` | Opens disc directly on Top Menu via `blurayTopMenuCall()` with UO restriction lowering; publishes menu visibility on `BD_EVENT_MENU`; adds `libvlc_media_player_get_bluray_menu_open()`. | **Cross-platform** | `libvlc.dll`<br>`plugins/access/liblibbluray_plugin.dll` | **Static implementation evidence** |

---

## 5. Windows Rendering Path & Corrected HDR Plan

### 5.1 Architecture Inspection of the Windows Video Pipeline
Inspection of `src-tauri/src/player/engine.rs` and `src-tauri/src/player/embed_windows.rs` establishes how Nebulae actually initializes video on Windows:
1. **No `--vout` argument is passed** in `engine.rs` (it only passes `--tone-mapping=3`, `--tone-mapping-param=1.0`, `--tone-mapping-desat=0.5`).
2. On Windows, `libvlc_media_player_set_hwnd` attaches playback to a native child window (`STATIC` class) positioned below the transparent webview.
3. In VLC 3.0.23, the default module probe priorities for Windows video output are:
   - `direct3d11.c`: Priority **300**
   - `direct3d9.c`: Priority **280**
   - `glwin32.c` (OpenGL): Priority **275**
4. Consequently, **Windows automatically selects Direct3D 11 (`libdirect3d11_plugin.dll`)**.

### 5.2 The HDR Disconnect Identified
- The `--tone-mapping` variables configured in `engine.rs` and the `apply_edr_to_video_layer` stub in `embed_windows.rs` (`var_set_checked("tone-mapping", ...)`) are **exclusively consumed by `modules/video_output/opengl/`**. Direct3D 11 does not read or inherit them.
- `direct3d11.c` handles HDR via its own options:
  - `d3d11-hdr-mode=auto`: Checks monitor capabilities; outputs native PQ/BT.2020 to the DXGI swapchain if the monitor supports HDR, or invokes DirectX 11 VideoProcessor tone-mapping (`d3d11_tonemap.cpp`) if the display is SDR.
  - `d3d11-hdr-mode=never`: Always converts HDR to SDR.

### 5.3 Concrete Resolution Options for Windows HDR
- **Option 1: Direct3D 11 Native HDR (Recommended for Windows):**
  - Retain `libdirect3d11_plugin.dll` as the primary Windows video output (highest performance, hardware DWM compositing, native Windows 10/11 HDR display pass-through).
  - Update `src-tauri/src/player/engine.rs` to configure Direct3D 11 HDR controls (`--d3d11-hdr-mode=auto`) on Windows, replacing the inert OpenGL tone-mapping variables.
- **Option 2: OpenGL Shader Parity (`glwin32`):**
  - Rebuild `plugins/video_output/libgl_plugin.dll` from the patched `fragment_shaders.c` and `converter.h`.
  - Force `--vout=glwin32` in `engine.rs` on Windows.
  - *Risk:* Severe performance impact on Windows, software surface fallback on non-NVIDIA GPUs, and potential Z-order flicker beneath WebView2.

---

## 6. Reproducible MinGW-w64 Build Recipe for Windows

Building the modified Windows runtime requires a MinGW-w64 cross-compilation environment (`x86_64-w64-mingw32`). It cannot be built using MSVC.

### 6.1 Preparation Commands
```bash
# 1. Unpack pristine sources
tar -xf downloads/vlc-3.0.23.tar.xz
cd vlc-3.0.23

# 2. Apply the five patches in strict order (zero fuzz)
../../scripts/apply-vlc-patches.sh .

# 3. Compile libbluray 1.5.0 with BD-J
cd ../libbluray-1.5.0
meson setup build --prefix=$PREFIX --libdir=lib -Dbdj_jar=enabled -Dant_home=$ANT_HOME
ninja -C build install
```

### 6.2 VLC Module Compilation (MinGW-w64)
```bash
cd ../vlc-3.0.23
mkdir -p build-win64 && cd build-win64

# Configure against VideoLAN win64 prebuilt contribs
../configure \
    --host=x86_64-w64-mingw32 \
    --enable-shared \
    --disable-static \
    --prefix=$VLC_OUT \
    PKG_CONFIG_LIBDIR=$CONTRIBS/lib/pkgconfig

# Build core and required modified modules
make -j$(nproc) src/libvlccore.la lib/libvlc.la
make -j$(nproc) modules/access/liblibbluray_plugin.la
make -j$(nproc) modules/video_output/libdirect3d11_plugin.la
make -j$(nproc) modules/video_output/libgl_plugin.la
```

---

## 7. Division of Ownership & Review Handoff

- **Factory B Action Requested:**
  1. Review this specification for source completeness and verify that no auxiliary patches were missed.
  2. Confirm the HDR architectural decision: approve Option 1 (Direct3D 11 native HDR configuration) or Option 2 (OpenGL rebuilt `libgl_plugin.dll`).
  3. Validate or supply the MinGW-w64 cross-compiled binary closure for `libvlc.dll`, `libvlccore.dll`, and `liblibbluray_plugin.dll`.
- **Antigravity Ownership:**
  1. Owns Windows execution, local staging via `stage_runtime.py`, NSIS bundling, and live runtime acceptance testing on Supermicro.
  2. Will not package another candidate until the modified runtime binaries and HDR strategy are confirmed by Factory B.

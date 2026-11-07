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
| **`hdr-tonemap.patch`** | `modules/video_output/caopengllayer.m`<br>`modules/video_output/opengl/converter.h`<br>`modules/video_output/opengl/fragment_shaders.c`<br>`modules/video_output/opengl/vout_helper.c` | Adds GLSL ITU-R BT.2390 tone-mapping and BT.709 gamut mapping shaders (`vlc_pq_to_nits`, `vlc_hdr_tonemap`). | **Output Dependent** (OpenGL pipeline) | `plugins/video_output/libgl_plugin.dll` (if OpenGL is used) | **Not used on Windows** (OpenGL only; Windows uses `d3d11-hdr-sdr-tonemap.patch`) |
| **`bluray-playlist-id.patch`** | `include/vlc/libvlc_media_player.h`<br>`lib/libvlc.sym`<br>`lib/media_player.c`<br>`modules/access/bluray.c` | Publishes active MPLS ID on `BD_EVENT_PLAYLIST` to input thread; adds `libvlc_media_player_get_bluray_playlist_id()`. | **Cross-platform** | `libvlc.dll`<br>`plugins/access/liblibbluray_plugin.dll` | **Static implementation evidence** |
| **`bluray-menu-first.patch`** | `include/vlc/libvlc_media_player.h`<br>`lib/libvlc.sym`<br>`lib/media_player.c`<br>`modules/access/bluray.c` | Opens disc directly on Top Menu via `blurayTopMenuCall()` with UO restriction lowering; publishes menu visibility on `BD_EVENT_MENU`; adds `libvlc_media_player_get_bluray_menu_open()`. | **Cross-platform** | `libvlc.dll`<br>`plugins/access/liblibbluray_plugin.dll` | **Static implementation evidence** |
| **`d3d11-hdr-sdr-tonemap.patch`** | `modules/video_output/win32/d3d11_quad.c`<br>`d3d11_quad.h`<br>`d3d11_shaders.c`<br>`d3d11_shaders.h`<br>`direct3d11.c` | Replaces the stock Hable/gamma-2.2 PQ/HLG-to-SDR shader with a BT.2390 EETF on max(R,G,B) in absolute light (SDR white = 100 cd/m2, no gain, no clip), source-peak aware (MaxCLL, mastering max, 1000 nit fallback), HLG inverse OETF + OOTF, luma-preserving BT.2020 to BT.709 gamut desaturation. Also fixes the upstream `COLORSPACE_BT2020_YUV_TO_FULL_RGBA` green row (Kg applied twice). SDR and native-HDR swapchain paths unchanged. | **Windows only** (D3D11) | `plugins/video_output/libdirect3d11_plugin.dll` (built by `scripts/windows/build-d3d11-plugin.sh`, pinned in `scripts/windows/d3d11-plugin.lock.json`) | **Implemented and validated** (see `UHD-HDR-SDR-VALIDATION.md`) |

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

### 5.2 The HDR Disconnect Identified (original finding, corrected)
- The `--tone-mapping*` variables formerly configured in `engine.rs` and the `apply_edr_to_video_layer` stub in `embed_windows.rs` are consumed **only** by `modules/video_output/opengl/`. Direct3D 11 never read them, so they were inert on Windows. They are now macOS-only in `engine.rs`, and `apply_edr_to_video_layer` is a documented no-op on Windows.
- Earlier text in this spec claimed `d3d11-hdr-mode=auto` would invoke the DirectX 11 VideoProcessor tone-mapper on SDR displays. That is wrong for VLC 3.0.23: `d3d11-hdr-mode` (default `auto`) only chooses between native HDR output and the SDR shader path by display capability, and the VideoProcessor tonemapper is only reachable in the `generate` mode. Setting the option would not fix the SDR appearance. Stock D3D11 on an SDR swapchain applies a Hable operator with `LuminanceScale = 100/10000` followed by gamma 1/2.2, which renders mid-tones roughly 40% darker than a colorimetric conversion (the "washed out / dim" look).

### 5.3 Implemented Resolution (`d3d11-hdr-sdr-tonemap.patch`)
Windows keeps Direct3D 11 as the video output and the patched plugin replaces the HDR-to-SDR shader:
- **Decision:** `display->b_hdr_to_sdr` (SDR swapchain) selects tone mapping. If the swapchain is HDR (BT.2020/PQ capable output with luminance reported), the stock native passthrough path is used untouched. SDR sources never enter the tone mapper (bit-identical to stock).
- **Appearance:** PQ decoded to absolute light; BT.2390-10 EETF applied to max(R,G,B) with the source peak (MaxCLL, else mastering max, else 1000 nit; HLG 1000 nit) mapped to SDR white = 100 cd/m2; no brightness/contrast/saturation/gamma factor and no hard clip. BT.2020 to BT.709 out-of-gamut negatives are desaturated toward luma. Output is encoded with the same Rec.709/gamma-2.2 swapchain path as SDR video. HLG applies inverse OETF / 12 and a gamma 1.2 OOTF at 1000 nit before the same mapper.
- **Matrix fix:** the upstream BT.2020 YCbCr to RGB matrix used `-0.127007 / -0.440988` in the green row (the Kg factor was applied twice); corrected to `-0.187326 / -0.650424`. This affects saturated colors in both the tone-mapped and native-HDR paths.
- **Build parity:** the plugin must be configured with `HAVE_ID3D11VIDEODECODER` defined and the AMF headers present, otherwise the software-decode staging texture path is compiled out (crash in `Prepare()` for 10-bit software frames) or the AMF scaler is lost. `build-d3d11-plugin.sh` handles both and is bit-reproducible.
- **Fallback:** stock `libdirect3d11_plugin.dll` is rejected by `stage_runtime.py --require-patched` (sha, x64, marker string `HDR to SDR tone mapping:`, pinned lock hash). There is no fallback to the stock plugin in a modified runtime.
- **Limits:** HLG and native-HDR output were validated only for shader math in the SDR path; no HDR display or GPU is available on the test host, so native HDR passthrough is untouched stock behavior and untested on hardware.

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

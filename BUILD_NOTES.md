# Nebulae Build Notes — VLC Runtime & Navigation Extension

## Overview

Nebulae bundles a **hybrid VLC 3.0.23 runtime**:

- **Official VideoLAN binaries** (from `/Applications/VLC.app`, version
  exactly 3.0.23) provide `libvlccore.9.dylib` and the complete plugin set —
  the same binaries the reference VLC player uses, linking only system
  libraries.
- **Four locally built artifacts** carry Nebulae's patches (see
  `patches/vlc-3.0.23/`):
  - `lib/libvlc.5.dylib` — adds the `libvlc_media_player_navigate_menu()`
    export (`navigate-menu.patch`).
  - `plugins/libvideotoolbox_plugin.dylib` — VideoToolbox session-reuse fix
    (`vt-305.patch`).
  - `plugins/libcaopengllayer_plugin.dylib` — GLSL ITU-R BT.2390 tone mapping
    and BT.709 gamut mapping (`hdr-tonemap.patch`).
  - `plugins/libglconv_cvpx_plugin.dylib` — CVPX texture converter matched to
    patched OpenGL converter struct layout.

## Prerequisites

- macOS with Xcode CLT.
- Official **VLC 3.0.23** installed at `/Applications/VLC.app`
  (override with `VLC_OFFICIAL_APP`; any other version fails the ABI check).
- Rust + pnpm + Node.js for the app itself.
- Only for rebuilding the patched artifacts: Homebrew bison 3.8.2
  (`/opt/homebrew/opt/bison/bin/bison`).

## Building the patched artifacts (one-time / when patches change)

```bash
# 1. Unpack the VLC 3.0.23 release tarball
cd /tmp && tar xf vlc-3.0.23.tar.xz   # from videolan.org

# 2. Apply the patches
cd /tmp/vlc-3.0.23
patch -p1 < <repo>/patches/vlc-3.0.23/vt-305.patch
patch -p1 < <repo>/patches/vlc-3.0.23/navigate-menu.patch
patch -p1 < <repo>/patches/vlc-3.0.23/hdr-tonemap.patch
#    (navigate-menu.patch covers lib/media_player.c, lib/libvlc.sym, and the
#    public declaration in include/vlc/libvlc_media_player.h)

# 3. Configure + build ONLY the three artifacts (a full module build is
#    unnecessary and currently fails on webvtt's bison grammar)
mkdir -p /tmp/vlc-3.0.23-build && cd /tmp/vlc-3.0.23-build
../vlc-3.0.23/configure --prefix=/tmp/vlc-3.0.23-install \
    --disable-qt --disable-lua --disable-python --disable-test \
    --disable-doc --enable-vlc --enable-plugins \
    --disable-vdpau --disable-wayland
export PATH="/opt/homebrew/opt/bison/bin:$PATH"
make -j"$(sysctl -n hw.ncpu)" libvlc.la
make -j"$(sysctl -n hw.ncpu)" libvideotoolbox_plugin.la
make -j"$(sysctl -n hw.ncpu)" -C modules libcaopengllayer_plugin.la libglconv_cvpx_plugin.la
# Artifacts:
#   lib/.libs/libvlc.5.dylib
#   modules/.libs/libvideotoolbox_plugin.dylib
#   modules/.libs/libcaopengllayer_plugin.dylib
#   modules/.libs/libglconv_cvpx_plugin.dylib
```

## Staging the runtime

```bash
./scripts/stage-vlc-runtime.sh     # env: VLC_OFFICIAL_APP, VLC_BUILD_DIR
```

The script:

1. Verifies the official app is exactly 3.0.23.
2. Verifies the custom artifacts carry their patch symbols
   (`VTDecompressionSessionCanAcceptFormatDescription`,
   `_libvlc_media_player_navigate_menu` in the export trie).
3. Verifies the ABI contract: the custom libvlc's `@rpath/libvlccore.dylib`
   compatibility version equals the official core's, and every
   VLC-namespaced symbol the custom libvlc imports is exported by the
   official core.
4. Replaces `src-tauri/resources/vlc/{lib,plugins}`: official core +
   plugins, custom libvlc + videotoolbox plugin. No `Frameworks`, no
   `plugins.dat` (the cache is validated by mtime+size in
   `src/modules/bank.c`; bundling rewrites mtimes, so a shipped cache is
   always stale — the first launch does one clean full plugin scan and VLC
   writes a fresh cache itself).
5. Writes `src-tauri/resources/build-manifest.json` (per-file SHA-256 +
   source attribution + git rev).
6. Runs `scripts/verify-vlc-runtime.sh` (also runnable standalone).

`pnpm tauri build` re-runs staging automatically via `beforeBuildCommand`,
so every packaged build restages and re-verifies from the two sources.

## Building the app

```bash
pnpm tauri build
```

Outputs under `src-tauri/target/release/bundle/` (`macos/Nebulae.app`,
`dmg/Nebulae_<ver>_aarch64.dmg`). The build may end with an updater-signing
error when `TAURI_SIGNING_PRIVATE_KEY` is unset — the .app and .dmg are
already produced at that point; that failure is expected in local builds.

## Verifying a produced bundle

```bash
./scripts/verify-vlc-runtime.sh           # staged tree
diff <(shasum -a 256 src-tauri/resources/vlc/lib/libvlc.5.dylib | awk '{print $1}') \
     <(shasum -a 256 src-tauri/target/release/bundle/macos/Nebulae.app/Contents/Resources/vlc/lib/libvlc.5.dylib | awk '{print $1}')
```

`src-tauri/resources/build-manifest.json` records the exact hashes that were
staged for each build.

## Where playback-repair details live

Build instructions stop here on purpose. The defect analysis, per-defect
root causes, and the change/provenance ledger for the 2026-10-20/21 macOS
playback repair are in `docs/` (see `macos-playback-repair-ledger.md`).

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

## The reference release DMG (v0.2.3, the last known-good macOS build)

`Nebulae-macOS-arm64.dmg` (repository root) is the **canonical reference
artifact** for the macOS bundle.

| Property | Value |
|---|---|
| SHA-256 | `b533c559b4afdee317b846beb1db594b522af50077f54f325f87e6bd2db5841d1` |
| Git revision | `7d6670d` |
| App version | `0.2.3` (`CFBundleShortVersionString` / `CFBundleVersion`) |
| VLC runtime | 3.0.23 (official core + plugins, 4 patched artifacts) |
| Runtime staged at | `2026-10-23T13:03:24Z` |
| App bundle | `Nebulae.app` — `Contents/MacOS/{nebulae,verify-payload-signature}`, `Contents/Resources/{icon.icns,resources/}` |

The checksum is recorded in `SHA256SUMS.txt` and in
`Nebulae-macOS-arm64.dmg.sha256`; verify before use:

```bash
shasum -a 256 -c SHA256SUMS.txt          # or compare the single file
```

### Installing from the DMG

```bash
hdiutil attach Nebulae-macOS-arm64.dmg -nobrowse -readonly
cp -R "/Volumes/Nebulae 1/Nebulae.app" /Applications/
hdiutil detach "/Volumes/Nebulae 1"
open /Applications/Nebulae.app
```

### Two facts about the produced bundle that are easy to get wrong

1. **The frontend is embedded in the binary.** Tauri's bundler does *not*
   create `Contents/Resources/app/`, and it is *not* required — the compiled
   `nebulae` binary carries the `dist/` assets (visible as
   `/assets/index-<hash>.js` strings inside the executable). Copying a
   `dist/` tree into `Contents/Resources/app/` is harmless but is not what
   the release bundle looks like; the reference DMG has no such directory.
2. **The staged VLC tree has no `Frameworks/`.**
   `scripts/stage-vlc-runtime.sh` removes `resources/vlc/{lib,plugins,Frameworks}`
   and re-stages `lib` + `plugins` only. The release workflow copies
   `VLC.app/Contents/Frameworks` in before the staging script runs, and the
   staging script then removes it again. The reference DMG therefore ships
   `resources/vlc/{lib,plugins}` and nothing else — no `plugins.dat`
   (mtime-keyed cache) and no `Frameworks`.

The repository's `src-tauri/resources/` tree is kept byte-identical to the
DMG's `Nebulae.app/Contents/Resources/resources/` tree so a local rebuild
produces the same runtime payload.

## Where playback-repair details live

Build instructions stop here on purpose. The defect analysis, per-defect
root causes, and the change/provenance ledger for the 2026-10-20/21 macOS
playback repair are in `docs/` (see `macos-playback-repair-ledger.md`).

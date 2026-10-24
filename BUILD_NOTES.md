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

## The v0.2.3 GitHub release (what a download actually gets)

The v0.2.3 release on `koppot/Nebulae-torrent_client` was republished on
2026-10-24 so that the downloadable macOS arm64 artifact is the same
application that is installed, and so that the auto-updater manifest is
internally consistent.

### What was wrong before

The v0.2.3 tag points at `263863d`, but the release assets were never built
from that tag. Every re-run of `.github/workflows/release.yml` was a
`workflow_dispatch` from `main`, and `actions/checkout@v4` (no `ref:`)
builds whatever branch the run was dispatched from. The assets published on
2026-10-23 therefore came from a `main` HEAD that already contained the
seek-bar thumbnail feature (`player_thumbnail_at` is present in that
binary). On top of that, the 2026-10-23 21:00Z run failed on the macOS
x64 `bundle_dmg.sh` step, which skipped the `aggregate-release` job — so
`latest.json` and `SHA256SUMS.txt` still described the *previous* run's
binaries. The result was a release whose updater manifest did not match the
files it pointed at.

### What is published now

| Asset | SHA-256 | Notes |
|---|---|---|
| `Nebulae-macOS-arm64.dmg` | `b533c559b4afdee317b846beb1db594b522af50077f54f325f87e6bd2db5841d1` | byte-identical to the reference DMG; app binary `3b54eb98…3d24` (git rev `7d6670d`) |
| `Nebulae-macOS-arm64.dmg-app.zip` | `dfeee32a8cad95e396cf0c671233a5c52ebd868b6f65ecbf5c78c6d9a4a54eef` | re-packed from the same app bundle as the DMG |
| `Nebulae-macOS-arm64.dmg-app.tar.gz` | see `SHA256SUMS.txt` | updater bundle, built from the **same** app bundle as the DMG |
| `Nebulae-macOS-arm64.dmg-app.tar.gz.sig` | — | minisign signature, verifies against the pubkey embedded in `tauri.conf.json` |

All three macOS arm64 artifacts carry the **same** `Nebulae.app`: the DMG,
the ZIP and the updater tarball each extract to a bundle that is
byte-identical (`diff -rq` reports no differences) to the installed
`/Applications/Nebulae.app`. There is no second build hiding in the release.

The x64, Linux and Windows assets are unchanged from the 2026-10-23 CI run.
Their signatures were re-verified against the published files and
`latest.json` was regenerated from the published `.sig` assets, so every
entry in the manifest now matches the bytes it points at.

### The auto-update (ATA) path

The app reads its updater endpoint and minisign public key from
`src-tauri/tauri.conf.json`:

- endpoint: `https://github.com/koppot/Nebulae-torrent_client/releases/latest/download/latest.json`
  (GitHub currently resolves `releases/latest` to the **v0.2.3** release)
- pubkey: `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ1MkNBQThCQkJCQzZGNDEKUldSQmI3eTdpNm9zUmFSM2RQOVlHZllGeFM1dHlpNU5JRitYaytjK3BNMTFwMHYrZjBWcXllNVgK`,
  which is the same key as `~/.tauri/nebulae-updater.key.pub`

The private key at `~/.tauri/nebulae-updater.key` is encrypted with an
**empty password** — the same convention `scripts/test-consecutive-silent-updates.py`
uses for its test key. The updater payload can therefore be re-signed
locally, without a CI run:

```bash
tar --no-xattrs --no-mac-metadata -czf Nebulae-macOS-arm64.dmg-app.tar.gz \
    -C <staging-dir> Nebulae.app
pnpm tauri signer sign -f ~/.tauri/nebulae-updater.key -p "" \
    --app-version 0.2.3 Nebulae-macOS-arm64.dmg-app.tar.gz
```

Always replace the tarball **and** its `.sig` together — a tarball whose
signature does not match breaks the in-app updater for every platform.

### Verifying a published artifact

```bash
# download and check the manifest
curl -sL https://github.com/koppot/Nebulae-torrent_client/releases/latest/download/latest.json

# check the DMG against the published checksum
shasum -a 256 -c SHA256SUMS.txt
```

## Where playback-repair details live

Build instructions stop here on purpose. The defect analysis, per-defect
root causes, and the change/provenance ledger for the 2026-10-20/21 macOS
playback repair are in `docs/` (see `macos-playback-repair-ledger.md`).

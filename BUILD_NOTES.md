# Nebulae Build Notes — VLC Runtime & Navigation Extension

## Overview

This document describes the complete regeneration route for the Nebulae
candidate, including the VLC 3.0.23 runtime with VT-305 patch and the
libvlc_media_player_navigate_menu main-menu extension.

## Source Identity

### VLC 3.0.23 Source
- Source tree: `/tmp/vlc-3.0.23/`
- Source archive: `vlc-3.0.23.tar.gz` (from videolan.org)
- Build directory: `/tmp/vlc-3.0.23-build/`

### Applied Patches

#### VT-305 Patch (`patches/vlc-3.0.23/vt-305.patch`)
Applied to: `/tmp/vlc-3.0.23/modules/codec/videotoolbox.m`

Three changes:
1. `kVTFormatDescriptionChangeNotSupportedErr` added to `HandleVTStatus`
   error list (line ~118) — triggers session invalidation like other fatal
   VT errors.
2. `VTDecompressionSessionCanAcceptFormatDescription` check before session
   restart in `StartVideoToolbox` (line ~1213) — avoids VT error 305 on
   format transitions by reusing an existing compatible session.
3. `videoFormatDescription` release in both branches of `StopVideoToolbox`
   (lines ~1349-1363) — prevents format-description leak across session
   recreation.

Patch source checksum: recorded in `patches/vlc-3.0.23/README.md`

#### Main-Menu Navigation Extension

Applied to VLC 3.0.23 source:

**`lib/media_player.c`** — Added `libvlc_media_player_navigate_menu()`:
- Calls `input_Control(p_input, INPUT_NAV_MENU, NULL)` on the active input
  thread, following the same architecture as `libvlc_media_player_navigate()`.
- `INPUT_NAV_MENU` is value 7 in `include/vlc_input.h:434`.
- Reaches `DEMUX_NAV_MENU` via `control.c` switch (lines 144-145) and
  `ControlNav()` in `input.c`.
- Blu-ray: `bd_menu_call(p_sys->bluray, -1)` (Top Menu / Root Menu)
- DVD: `dvdnav_menu_call()` (Title / Root menu)

**`include/vlc/libvlc_media_player.h`** — Added `LIBVLC_API` declaration.

**`lib/libvlc.sym`** — Added `libvlc_media_player_navigate_menu` to export list.
This makes the symbol dynamically resolvable via `dlsym()` (used by libloading
0.8.9).

### Patch Application Order
1. Apply VT-305 patch to `modules/codec/videotoolbox.m`
2. Add `libvlc_media_player_navigate_menu()` to `lib/media_player.c`
3. Add declaration to `include/vlc/libvlc_media_player.h`
4. Add export to `lib/libvlc.sym`
5. Build VLC 3.0.23 (see Build Commands below)

## Build Commands

### Prerequisites
- macOS 26+ (darwin 25.6.0)
- Xcode CLI tools
- Homebrew: bison 3.8.2 (`/opt/homebrew/opt/bison/bin/bison`)
- Rust 1.99.0 (cargo)
- pnpm 12.4.2 + Node.js

### VLC 3.0.23 Build

```bash
# Configure
cd /tmp/vlc-3.0.23-build
../vlc-3.0.23/configure \
    --prefix=/tmp/vlc-3.0.23-install \
    --disable-qt \
    --disable-lua \
    --disable-python \
    --disable-test \
    --disable-doc \
    --enable-vlc \
    --enable-plugins \
    --disable-vdpau \
    --disable-wayland \
    2>&1 | tail -30

# Build core libraries and needed modules
make -j$(sysctl -n hw.ncpu) libvlc.la 2>&1 | tail -10
make -j$(sysctl -n hw.ncpu) libvideotoolbox_plugin.la 2>&1 | tail -10
make -j$(sysctl -n hw.ncpu) libcaopengllayer_plugin.la 2>&1 | tail -10
```

Note: The full module build is blocked by `codec/webvtt/CSSGrammar.c` (bison 3.8.2
grammar incompatibility) and GUI nib compilation (Xcode ibtool plugin issue).
Only the required modules (videotoolbox, caopengllayer) are built.

### Rust Backend Build

```bash
cd /Users/dev/Documents/GitHub/nebulae-recovery-branch/src-tauri
cargo build --release
```

### VLC Runtime Staging

```bash
cd /Users/dev/Documents/GitHub/nebulae-recovery-branch
sh scripts/stage-vlc-runtime.sh
```

Copies built artifacts from `/tmp/vlc-3.0.23-build/` into
`src-tauri/resources/vlc/` and verifies:
- VT-305 patch symbol in videotoolbox plugin
- `navigate_menu` export in libvlc.5.dylib

### Runtime Verification

```bash
sh scripts/verify-vlc-runtime.sh
```

Checks:
- Core libraries present (libvlc.5.dylib, libvlccore.9.dylib)
- Plugins present (videotoolbox, caopengllayer)
- VT-305 patch symbol in videotoolbox
- `navigate_menu` export in libvlc (via dyld_info -exports)

### Tauri Application Build

```bash
cd /Users/dev/Documents/GitHub/nebulae-recovery-branch
pnpm tauri build
```

The `beforeBuildCommand` in `tauri.conf.json` runs:
1. `pnpm build` — frontend build (vite)
2. `sh scripts/stage-vlc-runtime.sh` — VLC runtime staging

The `resources/**/*` glob in `tauri.conf.json` bundles the staged VLC runtime
into the app bundle.

### Code Signing

The Tauri bundler attempts code signing. Set `TAURI_SIGNING_PRIVATE_KEY`
environment variable for signed artifacts. Without it, the build produces
unsigned bundles (acceptable for development/distribution testing).

## Build Outputs

| Artifact | Path | SHA-256 |
|----------|------|---------|
| libvlc.5.dylib (build) | `/tmp/vlc-3.0.23-build/lib/.libs/libvlc.5.dylib` | 0c65e0e0222b78d59459b869afa814c9f1cba77f681351e0dca3aa0e9357dfef |
| libvlccore.9.dylib (build) | `/tmp/vlc-3.0.23-build/src/.libs/libvlccore.9.dylib` | faa6c3329bf5a3d5cb30624dd3d0c3f1ee5e35c94c6d1631ad2d980f3cb592bf |
| libvideotoolbox_plugin.dylib (build) | `/tmp/vlc-3.0.23-build/modules/.libs/libvideotoolbox_plugin.dylib` | 238f04d77d367c6682457f8f9c72231042188fd87a549b21dc72ada069877ad3 |
| libcaopengllayer_plugin.dylib (build) | `/tmp/vlc-3.0.23-build/modules/.libs/libcaopengllayer_plugin.dylib` | ae11e8b0c2b80901a15598fe835e309122eeff6d3ae58aa22fdaf7c1610276d0 |
| Nebulae.app | `src-tauri/target/release/bundle/macos/Nebulae.app` | — |
| Nebulae_0.2.2_aarch64.dmg | `src-tauri/target/release/bundle/dmg/Nebulae_0.2.2_aarch64.dmg` | — |
| Nebulae.app.tar.gz | `src-tauri/target/release/bundle/macos/Nebulae.app.tar.gz` | — |

All bundled artifacts in the `.app` match the build checksums above.

## Rust Source Changes

| File | Change |
|------|--------|
| `src-tauri/src/player/engine.rs` | Added `libvlc_media_player_navigate_menu: Option<...>` binding with `sym_opt!` macro for capability detection |
| `src-tauri/src/commands/media.rs` | Replaced `player_return_to_menu` with direct `navigate_menu()` dispatch; added `player_menu_navigate_menu()`; removed pause/sleep workarounds from `player_set_title` and `player_menu_navigate`; added EDR observer cleanup in `stop_session` |
| `src-tauri/src/player/embed_macos.rs` | Corrected EDR availability (macOS 26+, not 10.15); switched to `NSWindow.didChangeScreenNotification`; added `edr_observer_cleanup()` with proper lifecycle; removed unreliable overlay-view-layer EDR |
| `src-tauri/src/player/mod.rs` | Added `edr_observer: AtomicUsize` field to `PlayerState`; made `embed_macos` module `pub(crate)` |

## VLC Source Changes

| File | Change |
|------|--------|
| `lib/media_player.c` | Added `libvlc_media_player_navigate_menu()` function |
| `include/vlc/libvlc_media_player.h` | Added `LIBVLC_API` declaration |
| `lib/libvlc.sym` | Added `libvlc_media_player_navigate_menu` export |

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/stage-vlc-runtime.sh` | Canonical staging: copies built VLC artifacts to `src-tauri/resources/vlc/`, verifies symbols, overwrites stale files |
| `scripts/verify-vlc-runtime.sh` | Pre-build verification: checks core libs, plugins, VT-305 patch, navigate_menu export |

## Frontend Changes (pre-existing, uncommitted in working tree)

| File | Change |
|------|--------|
| `src/components/player/PlayerOverlay.tsx` | `at_menu: boolean` added to `Poll` interface |
| `src/lib/playerStore.ts` | `at_menu: boolean` added to `PlayerPollShape` interface |

## State: Playback Behavior Has Not Been Tested

No runtime playback testing was performed. All changes are build/link/package
verified only. `cargo build --release` passes. Tauri app bundle builds
successfully. VLC runtime artifacts are built and staged with verified
checksums.

## Checkpoint Preservation

Checkpoint `pre-menu-implementation-20261020-232306` is preserved and unaltered.
The installed application at `/Applications/MacLC.app` is not modified.

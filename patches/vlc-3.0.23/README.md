# VLC 3.0.23 Runtime Patches

Nebulae ships a **hybrid** VLC 3.0.23 runtime (assembled by
`scripts/stage-vlc-runtime.sh`): every library and plugin comes from the
official VideoLAN VLC 3.0.23 app bundle, except the two artifacts carrying
the patches in this directory, which are built locally from the VLC 3.0.23
release tarball.

| Artifact | Source | Patch |
|----------|--------|-------|
| `lib/libvlccore.9.dylib` | official `/Applications/VLC.app` | none |
| `plugins/*` (339 files) | official `/Applications/VLC.app` | none |
| `lib/libvlc.5.dylib` | local build | `navigate-menu.patch` |
| `plugins/libvideotoolbox_plugin.dylib` | local build | `vt-305.patch` |

## Source identity

- VLC 3.0.23 "Vetinari" release tarball (videolan.org), unpacked at
  `/tmp/vlc-3.0.23`, built in `/tmp/vlc-3.0.23-build` (see BUILD_NOTES.md).
- Official binaries: `/Applications/VLC.app`,
  `CFBundleShortVersionString` = 3.0.23.
- ABI contract: install name `@rpath/libvlccore.dylib`, compatibility
  version 10.0.0, current version 10.1.0 — identical between the official
  and locally built artifacts, and enforced by
  `scripts/verify-vlc-runtime.sh` together with full VLC-namespaced symbol
  coverage of the custom libvlc against the official core.

## vt-305.patch — `modules/codec/videotoolbox.m`

VideoToolbox session handling around format transitions (OSStatus 305,
`kVTVideoDecoderBadDataErr`):

1. `HandleVTStatus` also treats `kVTFormatDescriptionChangeNotSupportedErr`
   as a session-invalidating error.
2. `StartVideoToolbox` reuses the existing decode session when
   `VTDecompressionSessionCanAcceptFormatDescription()` says it accepts the
   new format, instead of tearing it down unconditionally.
3. `StopVideoToolbox` releases `videoFormatDescription` on both paths so it
   can be recreated cleanly.

Status: **compiled** into the staged videotoolbox plugin and shipped in
candidates since 2026-10-21. Device-level effect on VT-305 errors: not
re-tested in the 2026-10-21 repair session (no playback testing).

SHA-256 of the patched `videotoolbox.m`:
`5c6a7e87c96fb7902424f7f6918f4adcf987b734f616a455be7815a928af77f6`

## navigate-menu.patch — `lib/media_player.c` + `lib/libvlc.sym`

Adds `libvlc_media_player_navigate_menu()`, which issues
`input_Control(p_input, INPUT_NAV_MENU, NULL)` — the demuxer-level "go to
the disc's root/top menu" request:

- Blu-ray: `bd_menu_call()` (Top Menu)
- DVD: `dvdnav_menu_call()` (Title/Root menu)

This is deliberately distinct from `libvlc_media_player_navigate()` with
`libvlc_navigate_popup`, which only sends a popup keystroke (BD_VK_POPUP)
and is masked out during many discs' First Play titles (that made the Menu
button appear dead on such discs). The separate exported symbol also lets
the app detect the capability at runtime against unmodified libVLC builds.

Status: **compiled** into the staged `lib/libvlc.5.dylib`; the export is
verified via `dyld_info -exports` by `scripts/verify-vlc-runtime.sh`.

Provenance: written for Nebulae, not an upstream VLC patch. Regenerate with:
`diff -ruN` between the release tarball and the patched tree for
`lib/media_player.c` and `lib/libvlc.sym` (the declaration in
`include/vlc/libvlc_media_player.h` is also required; it is part of the
build tree and documented in BUILD_NOTES.md).

## Build requirements

- Xcode CLT; Homebrew bison 3.8.2 (system bison 2.3 cannot build webvtt's
  grammar — only relevant for a full module build, which Nebulae does not
  need: only `libvlc.la` and `libvideotoolbox_plugin.la` are built locally).

## Historical note

An earlier revision of this README described vt-305.patch as "not compiled
or tested" and described runtime integration as copying the whole locally
built plugin set. Both were wrong/stale: the plugin set is now official,
and only the two patched artifacts above come from the local build.

# VLC 3.0.23 Runtime Patches

Nebulae ships a **hybrid** VLC 3.0.23 runtime for macOS Apple silicon (assembled by
`scripts/stage-vlc-runtime.sh`): every library and plugin comes from the official
VideoLAN VLC 3.0.23 app, except the artifacts carrying the patches in this
directory, which `scripts/build-vlc-runtime.sh` builds from the VLC 3.0.23 release
tarball, and libbluray, which it builds from its own pinned tarball.

| Artifact | Source | Patch(es) |
|----------|--------|----------|
| `lib/libvlccore.9.dylib` | official VLC 3.0.23 app | none |
| `plugins/*` (331 files) | official VLC 3.0.23 app | none |
| `lib/libvlc.5.dylib` | built from the patched source | `navigate-menu.patch`, `bluray-playlist-id.patch`, `bluray-menu-first.patch` |
| `plugins/libvideotoolbox_plugin.dylib` | built from the patched source | `vt-305.patch` |
| `plugins/libcaopengllayer_plugin.dylib` | built from the patched source | `hdr-tonemap.patch` |
| `plugins/libglconv_cvpx_plugin.dylib` | built from the patched source | none of its own: it must match the converter struct that `hdr-tonemap.patch` changes |
| `plugins/liblibbluray_plugin.dylib` | built from the patched source | `bluray-playlist-id.patch`, `bluray-menu-first.patch` |
| `lib/libbluray.4.dylib`, `lib/libbluray-j2se-1.5.0.jar`, `lib/libbluray-awt-j2se-1.5.0.jar` | libbluray 1.5.0, built unmodified | none |

The Blu-ray plugin is linked to this libbluray 1.5.0, not to the libbluray 1.4.0
that is built into VideoLAN's own Blu-ray plugin, so the 1.4.0 jars of the
official plugin set are not shipped (libbluray only accepts jars of its own
version). The libraries libbluray needs (libudfread, fontconfig, freetype,
libpng, libintl) are copied from Homebrew next to it and relocated; they are
listed in `vlc/bundled-libraries.json` inside the app.

## Source identity

- VLC 3.0.23 "Vetinari" release tarball (videolan.org), SHA-256
  `e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0`, unpacked and
  patched by `scripts/build-vlc-runtime.sh` under `src-tauri/vlc-runtime-build/`
  (git-ignored). See `BUILD_NOTES.md`.
- libbluray 1.5.0 release tarball, SHA-256
  `f676408e91a5d321abf8b8d4dfdae36205c297dab5c54c3ec519639025f474a2`.
- Official binaries: the VLC 3.0.23 disk image of VideoLAN, whose SHA-256 is pinned
  in `scripts/bundle-macos-vlc.sh`; `CFBundleShortVersionString` = 3.0.23.
- ABI contract: install name `@rpath/libvlccore.dylib`, compatibility
  version 10.0.0, current version 10.1.0 — identical between the official
  and locally built artifacts, and enforced by
  `scripts/stage-vlc-runtime.sh` together with full VLC-namespaced symbol
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
`ef722854db01c2835b75cba934809627ac94fb97dc84cd9b91d041589ca57586`
(listed with the other patched sources in `patched-sources.sha256`; an earlier
revision of this README quoted a different value that did not match the file).

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

Provenance: written for Nebulae, not an upstream VLC patch. The patch now also
carries the declaration in `include/vlc/libvlc_media_player.h` (placed right
after `libvlc_media_player_navigate`), so applying it to a pristine tree yields
the header the build needs. Regenerate with `diff -ruN` between the release
tarball and the patched tree for the header, `lib/media_player.c` and
`lib/libvlc.sym`.

## hdr-tonemap.patch — `modules/video_output/opengl/fragment_shaders.c`

Implements built-in GLSL ITU-R BT.2390-10 Hermite spline tone mapping and
Display P3 color gamut conversion for UHD HDR video:

1. When playing HDR10 content (`TRANSFER_FUNC_SMPTE_ST2084` or
   `COLOR_PRIMARIES_BT2020` / `COLOR_SPACE_BT2020`) without `libplacebo`,
   VLC 3.0.23 previously emitted no tone mapping, leaving raw BT.2020 PQ floats
   in `gl_FragColor`. On SDR displays and monitors with headroom = 1.0 (such
   as Samsung Odyssey G95NC in SDR mode), this squashed all normal scene content
   into the lower 50% of the range and desaturated wide-gamut colors, causing
   the washed-out, dim, flat picture.
2. The patch injects:
   - PQ inverse EOTF (`vlc_pq_to_linear`): converts PQ values to linear nits.
   - ITU-R BT.2390-10 Hermite spline compression (`vlc_hermite`): preserves 1:1
     shadow and midtone contrast while rolling off highlights smoothly to peak white.
   - Wide-gamut matrix mapping: converts BT.2020 linear to Display P3 linear.
   - Display gamma 2.2 / sRGB OETF: encodes to Display P3 for rich, vivid,
     accurate color reproduction.
3. SDR videos (1080p Blu-ray, DVD, HD) bypass tone-mapping completely and remain
   bit-identical to stock VLC.

Status: **compiled** into `plugins/libcaopengllayer_plugin.dylib`.

## bluray-playlist-id.patch — `modules/access/bluray.c` + `lib/media_player.c` + `lib/libvlc.sym`

Exposes the active Blu-ray MPLS playlist number (0–99999) to Nebulae:

1. In `modules/access/bluray.c`, the `Open()` function creates a new integer
   variable `"bluray-playlist-id"` on the input thread (initialised to -1).
   Each time `BD_EVENT_PLAYLIST` fires (i.e. a new MPLS playlist begins),
   `var_SetInteger(p_demux->p_input, "bluray-playlist-id", e->param)` records
   the MPLS number on the input thread object.

2. In `lib/media_player.c`, a new exported function
   `libvlc_media_player_get_bluray_playlist_id()` reads this variable via
   `var_GetInteger(p_input, "bluray-playlist-id")`, returning -1 when no
   Blu-ray playlist is active.

3. `lib/libvlc.sym` exports the new symbol; the declaration is in
   `include/vlc/libvlc_media_player.h` (after
   `libvlc_media_player_navigate_menu`, which this patch builds on).

Nebulae uses this in `src-tauri/src/commands/media.rs` (warning-skip watcher)
to identify the exact warning playlist (e.g. Summer Wars `00100.mpls`) without
relying on duration heuristics. The flag is stored in
`<app_data_dir>/disc_warns.json`, keyed by SHA-1 of `BDMV/MovieObject.bdmv`
plus the byte-size of the identified warning MPLS file.

Status: **compiled** into both `lib/libvlc.5.dylib` (new export) and
`plugins/liblibbluray_plugin.dylib` (the `bluray.c` change).

Note: `liblibbluray_plugin.la` needs `-lintl` when it is linked (GNU gettext, used for
`_("Top Menu")` strings in `bluray.c`). `scripts/build-vlc-runtime.sh` passes the
Homebrew gettext link flags itself.

## bluray-menu-first.patch — `modules/access/bluray.c` + `lib/media_player.c` + `lib/libvlc.sym` + `include/vlc/libvlc_media_player.h`

Opens a menu-enabled Blu-ray on its authored top menu instead of its First
Play warning reel, and exposes the graphics controller's menu-visible state:

1. In `blurayOpen` (`b_menu` branch only), a new `blurayTopMenuCall()`
   requests the top menu immediately after `bd_play()`. `bd_play()` always
   lands on First Play, but libbluray's HDMV VM does not execute the First
   Play movie object until the first `bd_read()`, so the menu call re-targets
   the VM before a single intro/warning packet is decoded. When the disc has
   no usable top menu the call fails and First Play proceeds as authored.
2. `blurayTopMenuCall()` lowers libbluray's UO restriction enforcement to
   `BLURAY_PLAYER_SETTING_UO_RESTRICTION_DISABLED` only around the menu call.
   At the default RELAXED level a disc can mask the menu-call UO while its
   warning clips play, which made `INPUT_NAV_MENU` a silent no-op until the
   footage ended (B4 runtime finding: FBI/warning footage before the menu on
   every tested Blu-ray). The masking window is scoped to this navigation
   call: seeks and stream selection keep upstream enforcement.
   `DEMUX_NAV_MENU` and the top-menu case of `bluraySetTitle` use the same
   helper, so a manual Menu request during masked playback works as well.
3. `blurayOpen` creates a `bluray-menu-open` integer variable on the input
   thread (initialised to 0); the `BD_EVENT_MENU` handler publishes the
   graphics controller's interactive-menu-visible state to it.
   `libvlc_media_player_get_bluray_menu_open()` (new export) returns that
   state: 1 while an interactive menu is displayed, 0 while none is, -1 when
   the media is not Blu-ray. The current-title report cannot serve this
   purpose: `input.c`'s `INPUT_CONTROL_SET_TITLE` announces the requested
   title unconditionally, even when the disc's VM refused the menu call.

Nebulae's startup menu watcher confirms the menu against this signal, so a
rejected menu request can no longer end the watch while warnings still play.

Status: **compiled** into `lib/libvlc.5.dylib` (new export) and
`plugins/liblibbluray_plugin.dylib` (the `bluray.c` changes). Applies after
`bluray-playlist-id.patch` (same header, symbol list and `lib/media_player.c`).

## Applying the patches

Use `scripts/apply-vlc-patches.sh SOURCE_DIR` on a freshly unpacked VLC 3.0.23
tarball (SHA-256 `e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0`).
Do not call `patch` by hand: the order and the checks below are part of the
contract.

- Order: `vt-305`, `navigate-menu`, `hdr-tonemap`, `bluray-playlist-id`,
  `bluray-menu-first`. `bluray-playlist-id.patch` is a diff against the tree
  *after* `navigate-menu.patch`, because both edit the same header, symbol
  list and `lib/media_player.c`; `bluray-menu-first.patch` is a diff against
  the tree after `bluray-playlist-id.patch` for the same reason (and it also
  edits `modules/access/bluray.c`).
- Zero fuzz (`patch -F0`), dry run first. A fuzzy match can put a hunk in the
  wrong place and still report success.
- After the last patch, every patched source file is compared with
  `patched-sources.sha256`. A tree that is not byte-for-byte the reviewed tree
  fails the script.
- The script refuses a tree that was already patched, and fails on any `.rej`
  or `.orig` file. Nothing falls back to unpatched VLC.

Why this exists: before 2026-10-28 the set could not be applied to a pristine
tree. `bluray-playlist-id.patch` was malformed (`patch` stopped with
"malformed patch at line 24"), and the header hunk of `navigate-menu.patch`
only matched with fuzz, which placed the declaration above the typedef it needs.
The shipped artifacts had been built from a hand-edited tree, so the workflows
could not reproduce them (and `release.yml` never applied the playlist patch).
Both patches were regenerated from that tree; applying the five patches to the
pristine tarball now yields all nine patched files byte-identical to it.

## Build requirements

See `BUILD_NOTES.md`: Xcode command line tools, the Homebrew packages that the
workflows install (Homebrew bison replaces the system bison 2.3, which cannot build
webvtt's grammar; Homebrew gettext supplies libintl), and a JDK for the BD-J jars.
Only `libvlc.la`, `libvideotoolbox_plugin.la`, `libcaopengllayer_plugin.la`,
`libglconv_cvpx_plugin.la`, and `liblibbluray_plugin.la` are built; a full module
build is unnecessary.

## Historical note

An earlier revision of this README described vt-305.patch as "not compiled
or tested" and described runtime integration as copying the whole locally
built plugin set. Both were wrong/stale: the plugin set is now official,
and only the patched artifacts listed in the table above come from the local build.

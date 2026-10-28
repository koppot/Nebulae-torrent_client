# Nebulae Build Notes — VLC Runtime & Navigation Extension

## Overview

The macOS (Apple silicon) app bundles a **hybrid VLC 3.0.23 runtime**, the libraries that libbluray needs, and a Transmission daemon with its libraries. Repository scripts build or stage every one of them. The three macOS workflow legs (`release.yml`, `candidate-build.yml`, `build-test.yml`) call those same scripts, so a local build and a CI build produce the same payload.

- **Official VideoLAN binaries** (from the official VLC 3.0.23 disk image, whose SHA-256 is pinned in `scripts/bundle-macos-vlc.sh`) provide `libvlccore.9.dylib` and the plugin set. They link only system libraries.
- **Five files built by Nebulae** from the VLC 3.0.23 source tarball carry its patches (`patches/vlc-3.0.23/`, applied by `scripts/apply-vlc-patches.sh`):
  - `lib/libvlc.5.dylib` — adds `libvlc_media_player_navigate_menu()` (`navigate-menu.patch`) and `libvlc_media_player_get_bluray_playlist_id()` (`bluray-playlist-id.patch`).
  - `plugins/libvideotoolbox_plugin.dylib` — VideoToolbox session-reuse fix (`vt-305.patch`).
  - `plugins/libcaopengllayer_plugin.dylib` — GLSL ITU-R BT.2390 tone mapping and BT.709 gamut mapping (`hdr-tonemap.patch`).
  - `plugins/libglconv_cvpx_plugin.dylib` — CVPX texture converter matched to the patched OpenGL converter struct layout.
  - `plugins/liblibbluray_plugin.dylib` — publishes the active Blu-ray playlist number (`bluray-playlist-id.patch`); linked to libbluray 1.5.0.
- **libbluray 1.5.0 and its BD-J jars**, built from the pinned upstream tarball. libbluray only accepts jars of its own version, so the library and the two jars are built and shipped together. The Homebrew libraries it needs (libudfread, fontconfig, freetype, libpng, libintl) are copied next to it in `vlc/lib` and rewritten to load from inside the bundle.
- **Transmission 4.1.3** with the identity-override patch, built from the pinned source tarball, with its five Homebrew libraries, in `engine/transmission/`.

No step falls back to an unpatched or differently versioned component. A download whose checksum does not match, a patch that does not apply, a missing artifact, or a reference to the build machine in a shipped file stops the build.

## Prerequisites

- macOS on Apple silicon with the Xcode command line tools. The patched runtime links Homebrew libraries that exist only for the architecture of the build machine, so it is built on an arm64 Mac.
- Homebrew packages (the workflows install the same list):
  `brew install cmake meson ninja pkgconf bison libevent libdeflate libpsl miniupnpc gettext libudfread fontconfig freetype`
  Homebrew bison replaces the system bison 2.3, which cannot build the webvtt grammar of VLC. Run the scripts with `/opt/homebrew/bin` in `PATH`.
- A JDK 18 or newer (`javac`) for the BD-J jars: `JAVA_HOME`, or the newest one that `/usr/libexec/java_home` knows. Apache Ant is fetched by the script as a checksum-verified upstream release; it is a build tool and is not shipped.
- Network access to `download.videolan.org`, `archive.apache.org` and `github.com`. Every download is pinned by SHA-256 (SHA-512 for Ant) in the script that fetches it.
- Rust, Node.js and pnpm for the app itself.

## Building the runtime and the app

From the repository root:

```bash
./scripts/bundle-macos-transmission.sh               # engine/transmission: daemon, libraries, notices, smoke test
./scripts/bundle-macos-vlc.sh aarch64-apple-darwin   # vlc: build, stage, verify
find src-tauri/resources -name .DS_Store -type f -print -delete
pnpm tauri build
./scripts/verify-macos-app.sh src-tauri/target/release/bundle/macos/Nebulae.app
```

`beforeBuildCommand` in `tauri.conf.json` is only `pnpm build` (the frontend). It does **not** stage the runtime: run the two bundle scripts first, or the app is built with whatever is already in `src-tauri/resources/`. The `.DS_Store` line is there because `"resources": ["resources/**/*"]` copies hidden files into the app. The staging scripts delete them too, and `verify-macos-app.sh` fails if one reaches the bundle.

The build may end with an updater-signing error when `TAURI_SIGNING_PRIVATE_KEY` is unset. The `.app` and `.dmg` are already produced at that point, so for a local build that error is expected.

`./scripts/bundle-macos-vlc.sh x86_64-apple-darwin` stages the **unmodified** official Intel VLC 3.0.23 instead. Nebulae's patches are not in it, because the patched runtime needs Homebrew libraries of the build machine's architecture and the x86_64 leg runs on an arm64 runner. The script says so in its log and as a workflow warning.

## What each script does

| Script | Purpose |
|---|---|
| `scripts/bundle-macos-vlc.sh TARGET` | The single entry point for the VLC runtime. Downloads the official VLC 3.0.23 disk image (pinned SHA-256) and mounts it read-only. For `aarch64-apple-darwin` it runs `build-vlc-runtime.sh` and then `stage-vlc-runtime.sh`; for `x86_64-apple-darwin` it stages the unmodified official Intel runtime. |
| `scripts/build-vlc-runtime.sh` | Builds libbluray 1.5.0 (Meson) with its BD-J jars (Ant 1.10.18), then VLC 3.0.23 with the patches. The deployment target is `bundle.macOS.minimumSystemVersion` of `tauri.conf.json` (11.0). It compiles with `-ffile-prefix-map` and `-g0`, then fails if a build-machine path is still inside an artifact (strings or debug-map entries). Work directory: `src-tauri/vlc-runtime-build/` (git-ignored; `NEBULAE_VLC_WORK` overrides it). |
| `scripts/apply-vlc-patches.sh` | Applies the four patches in order with zero fuzz, then checks the nine patched sources against `patches/vlc-3.0.23/patched-sources.sha256`. |
| `scripts/stage-vlc-runtime.sh` | Combines the official files with the built ones in `src-tauri/resources/vlc/`, relocates the libraries libbluray needs, writes `licenses/`, `bundled-libraries.json` and `src-tauri/resources/build-manifest.json`, and runs the verification. |
| `scripts/verify-vlc-runtime.sh [DIR]` | Checks a staged runtime: files, patch symbols, one libbluray version, a strict recursive Mach-O audit, signatures, and a load test in a separate process. |
| `scripts/bundle-macos-transmission.sh` | Builds the identity-patched daemon from the pinned source, relocates its libraries, audits, writes the licenses and runs a loopback smoke test in a temporary profile. |
| `scripts/relocate-macos-dylibs.py` | Copies a dependency closure into the bundle, rewrites each load command, signs what it changed and records `bundled-libraries.json`. |
| `scripts/audit-macos-bundle.py` | Recursive Mach-O audit of a directory or an app (below). |
| `scripts/verify-macos-app.sh APP` | Final gate for a built app: no `.DS_Store`, strict audit, minimum-macOS advice, `codesign --verify --deep --strict`, and every VLC and Transmission file equal to what the staging scripts recorded. |

### The audit

`python3 scripts/audit-macos-bundle.py [--strict] [--strict-strings] [--arch ARCH] [--max-minos X.Y] [--json FILE] [-v] PATH` walks every Mach-O image under `PATH` and reports:

| Finding | Meaning |
|---|---|
| `EXTERNAL` | a load command, rpath or install name outside the bundle and the system (Homebrew, `/usr/local`, a build directory) |
| `UNRESOLVED` | a dependency that does not resolve to a file inside the bundle or to a system library |
| `ARCH`, `ID` | a wrong architecture; an install name that is an absolute path outside the bundle |
| `DSSTORE`, `SYMLINK` | Finder litter; a symlink that is absolute, dangling or leaves the bundle |
| `STRING` | an absolute external `.dylib` path compiled into an executable (`--strict-strings`) |
| `MINOS` | an image that needs a newer macOS than `--max-minos` (advice; `NEBULAE_ENFORCE_MINOS=1` makes it fatal in the scripts that report it) |

`--strict` exits 1 on any finding.

**Known advice.** The five Homebrew libraries beside libbluray and the five beside the Transmission daemon declare a minimum macOS of 26.0 (a Homebrew bottle keeps the deployment target of the Mac that built it), and the bundled Tailscale binaries declare 13.0, while the app declares 11.0. That cannot be lowered without building those libraries from source. The runtime is therefore not known to run on a clean Mac or on an older macOS than the one it was built on. The artifacts that Nebulae builds itself (libvlc, the four plugins, libbluray) target 11.0. The VideoLAN plugin binaries are untouched and contain VideoLAN's own build-host paths as debug strings.

## What `stage-vlc-runtime.sh` stages

1. It verifies that the official app is exactly 3.0.23 and that the built artifacts carry their patches (`VTDecompressionSessionCanAcceptFormatDescription`, `_libvlc_media_player_navigate_menu`, `_libvlc_media_player_get_bluray_playlist_id`, and the `bluray-playlist-id` string in the Blu-ray plugin).
2. It verifies the ABI contract: the custom libvlc's `@rpath/libvlccore.dylib` compatibility version equals the official core's, and every VLC-namespaced symbol the custom libvlc imports is exported by the official core.
3. It replaces `src-tauri/resources/vlc/{lib,plugins,licenses}`: the official core and plugins, the five built files, `libbluray.4.dylib` with the two 1.5.0 jars, and the relocated Homebrew libraries.
   Left out of the official set: `plugins.dat` (VLC validates it by mtime and size and bundling rewrites mtimes, so a shipped cache is always stale; the first launch writes a fresh one), the 1.4.0 jars that belong to the official Blu-ray plugin, and the two plugins of VLC.app that need Sparkle and Growl, which only VLC.app ships. There is no `Frameworks` directory.
4. It writes `src-tauri/resources/build-manifest.json` (per-file SHA-256, source attribution, symlinks, git revision, target architecture) and runs `scripts/verify-vlc-runtime.sh`.

`lib/libvlc.dylib` and `lib/libvlccore.dylib` are symlinks in the staged tree and are listed under `links` in the manifest. Tauri's bundler copies symlinks as regular files, so the built app has real files there; `verify-macos-app.sh` accepts both.

## Verifying a produced bundle

```bash
./scripts/verify-vlc-runtime.sh        # the staged tree
./scripts/verify-macos-app.sh src-tauri/target/release/bundle/macos/Nebulae.app
```

`src-tauri/resources/build-manifest.json` records the exact hashes that were staged for each build; `verify-macos-app.sh` compares the built app with it, and the Transmission libraries with `engine/transmission/bundled-libraries.json`.

## The reference release DMG (v0.2.3, the last known-good macOS build)

_This section is a record of the v0.2.3 reference artifact, built before the packaging repair of 2026-10-28. Its VLC runtime is the one in the table below (four patched artifacts, the official Blu-ray plugin); the runtime that the scripts above build differs from it. Build instructions are in the sections above._

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
| `Nebulae-macOS-arm64.dmg` | `6806b0b728c717e6488f1bbfbdd6f54a5aa5be32284181ffc605334599ca6ba8` | release build from consolidated main; includes player close repair and menu title fallback |
| `Nebulae-macOS-arm64.dmg-app.zip` | `72b15c957fa1d420db8cb3f4a7188c23535171bed746c02c8e603b2aaf1d5416` | re-packed from the same app bundle as the DMG |
| `Nebulae-macOS-arm64.dmg-app.tar.gz` | `4f5801c49086200fb61ba45572ee354244fceb49927b9903223fa333de1fa9c0` | updater bundle, built from the **same** app bundle as the DMG |
| `Nebulae-macOS-arm64.dmg-app.tar.gz.sig` | — | minisign signature, verifies against the pubkey embedded in `tauri.conf.json` |

All three macOS arm64 artifacts carry the **same** `Nebulae.app`: the DMG,
the ZIP and the updater tarball each extract to a bundle that is
byte-identical (`diff -rq` reports no differences). There is no second build hiding in the release.

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

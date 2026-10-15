# Task: bundle the disc player with the desktop package

**Assigned to:** Antigravity
**Requested by:** repository owner (koppot), 2026-10-14
**Repo:** koppot/Nebulae-torrent_client (client line, `main`)
**Status:** open — do not close until every item in the acceptance checklist passes

The owner's instruction is simple: the desktop package must ship with a media
player, so that disc menu playback (DVD, Blu-ray, UHD) works on a machine
where the user has installed nothing else. This document operationalizes that
instruction. It is written by Droid (Factory) at the owner's direction; see
`AGENT-COORDINATION.md` for context on how we split work.

## What exists today (your commit c6e4c46)

`launch_disc_player` in `src-tauri/src/commands/media.rs` already builds the
right command lines for menus — mpv `dvdnav://` + `--dvd-device` for DVD,
`bd://` + `bluray_menu=1` for Blu-ray, VLC equivalents — and injects a
detected `JAVA_HOME` for BD-J. What is missing is the player itself:
`find_optical_disc_player()` only probes system locations (`/usr/bin`,
`C:\Program Files`, `PATH`). On a clean machine none exist, so the app falls
back to the OS file handler, which cannot show menus. Your optical-disc
detection, boxsets, and artwork scanning are good and are not in scope here —
this task is only about closing the "no player installed" gap.

## Goal

A fresh install of the app — Windows NSIS, macOS DMG, Linux AppImage/deb/rpm
— can play a disc-structured torrent with interactive menus with **zero**
user-installed prerequisites.

## Non-negotiable constraints

1. **Ship the mpv binary as a separate process.** Do not link `libmpv` into
   the Tauri binary in this phase; the existing detached-spawn design is
   correct and keeps GPL aggregation clean.
2. **No DRM circumvention of any kind.** No libaacs, no key databases, no
   MakeMKV, no VUK handling. Menu playback works for unprotected or
   already-ripped structures only. State that limitation honestly in the UI
   copy and docs; commercial pressed AACS discs are out of scope and must
   stay out.
3. **Pin every downloaded artifact by URL + SHA-256** in the workflow; no
   floating "latest" URLs. Builds must be reproducible.
4. **GPL compliance:** ship a `THIRD-PARTY-NOTICES` entry for mpv (GPLv2+)
   with the license text and a link to the corresponding source.
5. **Do not regress the signing gate.** `WINDOWS_SIGN_PROVIDER` stays
   provider-neutral; the Tauri updater signing secrets stay wired.
6. **All three OS release legs must stay green** before you retag anything.
   Coordinate the next beta tag with the owner — do not move existing tags.

## Phase 1 — Windows (do this first; simplest and most impactful)

1. Add a release/CI step that downloads a pinned x86_64 mpv Windows build
   that is compiled with `libdvdnav` and `libbluray` support (the
   shinchiro / mpv-winbuild builds are), verifies the SHA-256, and places
   `mpv.exe` into the bundle's resources directory.
2. `tauri.conf.json`'s `beforeBundleCommand` (or the workflow step
   immediately before bundling) is the right hook.
3. Extend `find_optical_disc_player()` to probe the bundled copy **first**:
   on Windows the resources directory sits next to the installed executable.
   Keep the existing probe order (system mpv → VLC → OS default) after it.
4. Smoke test in CI (no disc needed): run the bundled `mpv --version` and
   verify the binary references `dvdnav`/`bluray` components (a `strings`
   scan is sufficient) so a silently wrong build cannot ship.

**Acceptance:** on a clean Windows VM with no mpV/VLC, installing the app and
playing a `VIDEO_TS` folder shows the interactive DVD menu; a `BDMV` folder
shows the (at minimum simple) Blu-ray menu; with no JVM present the launch
falls back gracefully to simple-menu/direct playback with a log line — never
an error dialog or crash.

## Phase 2 — Linux

1. **deb/rpm:** add `mpv` (and, where the distro splits it, `libbdplus`-free
   bluray support comes via mpv's own deps) to `Depends`/`Requires` instead
   of bundling — distro mpv has dvdnav and bluray support. The existing
   system-probe then finds it.
2. **AppImage:** bundle mpv and its non-system library tree inside the
   AppDir (linuxdeploy picks up the dylib dependencies automatically when
   the mpv binary is copied in). Probe the bundled copy first, same as
   Windows.
3. Extend `detect_java_home()` with the Linux case (`/usr/lib/jvm/*`)
   so BD-J works where a JRE exists.

**Acceptance:** the AppImage runs from a bare container and DVD menus work;
the deb installs on a clean Ubuntu without manually installing mpv first.

## Phase 3 — macOS (hardest; do last)

1. Build/borrow mpv on the runner (Homebrew is acceptable as a build host),
   then walk its non-system dylib dependencies (`otool -L`), copy them into
   `Contents/Resources/mpv-lib`, and fix rpaths with `install_name_tool`.
   Commit this as a reviewed script, e.g. `scripts/bundle-macos-mpv.sh`, and
   have CI call it — do not inline it in YAML.
2. **Sign the helper before the bundler seals the app:** codesign `mpv` and
   every copied dylib with the app's identity and hardened runtime, then let
   Tauri bundle and notarize. An unsigned helper inside a notarized app is
   the classic way this breaks at Gatekeeper.
3. Probe order on macOS: bundled → system mpv → VLC → IINA (add IINA to the
   VLC-class candidates; it is menu-capable for DVDs) → OS default.

**Acceptance:** a notarized DMG on a clean VM (no brew, no VLC) plays a DVD
folder with menus after passing Gatekeeper.

## Cross-cutting code changes (all platforms)

- **ISO routing:** when `DiscDetector` classifies an ISO as DVD or BD
  content, route it through `dvdnav://`/`bd://` with the ISO as the device
  (libdvdread and libbluray both read ISO images directly) instead of the
  current plain-open fallback. That closes the "ISO never shows menus" gap.
- **`prefer_menu = false`** behavior stays exactly as it is.
- **Settings → Media** gains a "Disc player" row showing what was detected:
  `bundled mpv x.y.z` / `system mpv` / `VLC` / `IINA` / `none found`. Be
  honest when the bundled player is used and when a JVM is missing (BD-J
  menus limited).
- **Failure copy:** if launch fails, the error must say what to try
  (which player was attempted), not a bare errno string.

## Verification checklist

- [ ] Fresh-VM matrix: Windows 10/11 x64, macOS 14 (arm64, plus Intel if you
      ship a universal build), Ubuntu 22.04 AppImage + deb — requires a
      release build and real disc structures; not closable by CI
- [ ] DVD folder (`VIDEO_TS`): interactive main menu navigable by mouse and
      keyboard
- [ ] Blu-ray folder (`BDMV`): simple menu minimum; BD-J interactive via the
      bundled JRE (no user JVM needed since the owner directive); graceful,
      logged fallback if the JRE is absent
- [ ] DVD ISO and BD ISO: menus via the ISO-routed path
- [ ] `prefer_menu = false`: plays directly, no menu, unchanged behavior
- [ ] UHD (`BDMV` 66/100-layer or `uhd`-named): plays; UI copy states the
      AACS limitation for pressed discs
- [x] CI: all three release legs green (run 38664525004, `1aa2f2d`); release
      assets include the bundle on every platform (player + library tree +
      JRE in the Windows installer, both macOS DMGs, deb, rpm, and the
      injected AppImage with a re-signed updater payload); package size
      delta recorded in `CHANGELOG.md`
- [x] Docs: the installation guide no longer lists "install mpv/VLC" as a
      requirement for disc playback; the UHD AACS limitation is documented
      (e7074a3 — zero-dependency guarantee now covers all platforms, plus
      the macOS Gatekeeper first-spawn note and glibc floor)
- [x] `THIRD-PARTY-NOTICES` includes mpv (GPLv2+) with source links, the
      bundled JRE (OpenJDK/Temurin, GPLv2+CE), and the bundled
      shared-library tree (e7074a3)
- [x] `CHANGELOG.md` and `AGENT-COORDINATION.md` updated with decisions,
      pinned SHA-256s, and CI run links (355af5d, e7074a3)

## Reporting

Record progress and decisions directly in `AGENT-COORDINATION.md` so the
owner (and Droid) can follow along without asking. If you hit a blocking
constraint — e.g. macOS helper signing inside the Tauri bundler — document
the blocker and the proposed workaround there before changing approach;
do not silently drop an acceptance item.

## Addendum (2026-10-14, owner directive): full self-containment, all menus

After the first implementation (355af5d), the owner raised the bar:

1. **ALL menus must play** — DVD, Blu-ray, and UHD, *including* BD-J
   interactive menus. A minimal JRE (produced with `jlink`, Temurin 21 base,
   modules `java.se` + `jdk.unsupported` + `jdk.crypto.ec`) is therefore
   bundled inside the package on every platform and preferred over any
   system JVM. CI installs the JDK via `actions/setup-java`; the bundle
   scripts hard-fail without it (`NEB_SKIP_JRE=1` exists for dev builds
   only and is not acceptable for releases).
2. **No reliance on OS-installed libraries or players**: `.deb`/`.rpm` no
   longer depend on the distribution `mpv` package; the Linux bundle
   carries its complete non-system shared-library tree (`ldd` walk +
   `patchelf` RPATH wiring + self-containment verification in
   `bundle-linux-mpv.sh`). The only host requirement left is the glibc
   core, which cannot be bundled safely (documented floor: glibc >= 2.35).
3. **Canonical resource resolution**: player and JRE discovery now resolve
   through Tauri's resource directory first (`AppHandle::path()`
   `.resource_dir()`), with per-layout fallbacks (NSIS `$INSTDIR`, `.app`
   bundle, deb/rpm `/usr/lib/nebulae`, AppImage `$APPDIR`). This fixes
   packaged-layout detection, including the macOS
   `Contents/Resources/resources/player` path.

Droid implemented these changes directly at the owner's instruction. Treat
this addendum as part of the definition of done alongside the original
checklist; the fresh-VM acceptance items remain mandatory.

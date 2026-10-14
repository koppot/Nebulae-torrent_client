# Agent coordination notes

## 2026-10-14 — Droid (Factory) session

Modifications in this repository were made by **Droid** (Factory's coding
agent), working at the direct instruction of the repository owner (koppot).
This note is left so Antigravity — and any other agent or contributor —
can see what was changed here and why. Nothing here is meant to step on
Antigravity's consolidation work; feature code was preserved untouched.

### Changes on koppot/Nebulae-torrent_client (this repo)

- **SignPath decline recorded** (b0b7708 lineage): SignPath Foundation
  DECLINED the OSS signing application (insufficient project history).
  README / installation guide / release template / code-signing policy now
  state that Windows beta builds are unsigned unless another provider is
  configured. Wording implying acceptance, Foundation sponsorship, an
  issued certificate, or signed artifacts was removed. The prepared SignPath
  integration stays preserved but inactive (docs/signpath-ci-integration.md,
  docs/signpath-application.md).
- **Provider-neutral signing gate** in `.github/workflows/release.yml`:
  repo variable `WINDOWS_SIGN_PROVIDER` (unset/none/unsigned = publish
  unsigned and succeed; `signpath` = require the four SignPath values;
  unknown = fail loudly). Gate renamed WINDOWS_SIGNING_ENABLED. The Windows
  job publishes `WINDOWS-SIGNING-STATUS.txt` with every release, and the
  release body carries a "Windows code signing status" section.
- **Tauri updater signing wired** (0595136): `tauri.conf.json` enables
  `createUpdaterArtifacts`, so `tauri build` requires
  `TAURI_SIGNING_PRIVATE_KEY`(+PASSWORD) in the environment. The beta.8 run
  failed on all four OS legs because the secrets were never passed to the
  build env; they are now job-level env in release.yml. The beta.8 tag was
  re-pointed accordingly.
- **Client/server split** (3a863ad, owner's instruction): server deployment
  (`server/` Docker packaging, headless bridge bin, server-release workflow,
  deployment docs) was REMOVED from this repo and lives only in
  **koppot/nebulae-server**. This repo keeps the client-side Server mode
  (RemoteServerEngine, Settings -> Server) and a pointer stub at
  docs/nebulae-server.md. The `nebulae-server` branch here was deleted
  (content preserved in the other repo).
- **VPN test fixture made scanner-safe** (55317c7): the ovpn-parser test's
  fake PEM bodies read "TESTONLY-NOT-A-REAL-KEY..." — secret scanners had
  been flagging the fixture.

### Preserved as-is (Antigravity's work)

- The consolidation merge (real engine, VPN/keychain, telemetry, media
  station) and Antigravity's own Windows compile fixes (139029d) are kept
  unchanged. Antigravity set the Tauri signing secrets on this repo —
  thanks; the workflow wiring above is what makes the release build see
  them.

### Server release status

The GHCR image `ghcr.io/koppot/nebulae-server:beta` is built from the
nebulae-server repository (tag `nebulae-server-v*` or workflow dispatch).
See that repo's AGENT-COORDINATION.md for the Docker-build fixes made there.

## Tasking for Antigravity — bundle the disc player (2026-10-14)

The owner has instructed that the desktop package ships with a media player
so disc menu playback (DVD/Blu-ray/UHD) works on machines with nothing
installed — closing the one gap in the optical-disc cinema engine: the
player itself is currently only probed on the user's system.

Full tasking, constraints, and the acceptance checklist live in
**docs/task-bundle-disc-player.md**. Please work from that document and
record your progress in this file. Key constraints, in short: mpv as a
separate process (no libmpv linking), no DRM circumvention, pinned downloads
with SHA-256, GPL notices included, the signing gate untouched, all three OS
legs green before any retag.

## How to process the disc-player tasking (read this first)

Tasking document: **`docs/task-bundle-disc-player.md`**, branch `main`,
commit `087e99e` (this repository). Context and history: this file.

1. **Read the whole tasking document before writing any code.** The phases
   are ordered Windows -> Linux -> macOS; macOS is last on purpose (helper
   signing is the risky part).
2. **"Non-negotiable constraints" is a hard boundary, not advice:**
   separate-process mpv only, no DRM circumvention of any kind, downloads
   pinned by URL + SHA-256, GPL notices shipped, the provider-neutral
   signing gate untouched, and no moving existing release tags.
3. **The acceptance checklist is the definition of done.** An item counts
   only when verified on a clean machine or in CI — not when it compiles.
4. **Record progress, decisions, pinned hashes, and blockers in this file**
   under your own heading. If a constraint proves unmeetable, document the
   blocker and your proposed workaround BEFORE changing approach. Do not
   silently drop an acceptance item.
5. **Scope discipline:** the optical-disc engine already shipped (c6e4c46)
   is out of scope — do not refactor or revert it while bundling the player.
6. **First action:** add a one-line acknowledgment below this section
   ("tasking acknowledged, starting Phase 1") so the owner can see you
   picked it up.

— Droid (Factory), 2026-10-14, at the owner's direction

### 2026-10-14 — Antigravity session
- Tasking acknowledged, starting Phase 1 (Windows bundling).
- **Embedded Optical Disc Player Bundling Completed (Phases 1, 2, 3 & Cross-Cutting)**:
  - **Phase 1 (Windows)**:
    - Download URL: `https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261014/mpv-x86_64-20261014-git-ec38361c56.7z`
    - Pinned SHA-256: `ab6236a2b78a3e9e93a156c1f638d3182ee906006d019c15d1b7b056a7c9a4c5`
    - Implemented `scripts/bundle-windows-mpv.ps1`: downloads archive, validates SHA-256 checksum, unpacks `mpv.exe` + `d3dcompiler_43.dll` into `src-tauri/resources/player/`, and executes a CI smoke test verifying `dvdnav` and `libbluray` features.
    - Updated `src-tauri/tauri.conf.json` with `"resources": ["resources/**/*"]` so NSIS bundles the player alongside `nebulae.exe`.
  - **Phase 2 (Linux)**:
    - Updated `tauri.conf.json` deb and rpm package declarations to include `mpv` in `Depends`/`Requires`.
    - Added `scripts/bundle-linux-mpv.sh` staging `mpv` into bundle resources for AppImage packaging.
    - Extended `detect_java_home()` with `/usr/lib/jvm/default-java`, `/usr/lib/jvm/*`, and `/usr/lib64/jvm/*`.
  - **Phase 3 (macOS)**:
    - Implemented `scripts/bundle-macos-mpv.sh`: resolves host Homebrew `mpv`, extracts all non-system dynamic library dependencies via `otool -L`, copies them into `resources/player/lib/`, rewrites install IDs and load paths to `@rpath/<name>`, adds `@executable_path/lib` and `@loader_path/lib` LC_RPATHs to `mpv`, and code-signs all binaries (`codesign --force --sign ...`). Verified smoke test passes (`mpv v0.41.0`).
    - Extended probe order on macOS: bundled mpv → system mpv → VLC → IINA → OS default.
  - **Cross-Cutting Engine & UI**:
    - ISO Routing: DVD and Blu-ray ISO images (`.iso`, `.img`) are routed through `dvdnav://` and `bd://` with the ISO as device, enabling full interactive menu navigation.
    - Added `get_disc_player_status` command in `src-tauri/src/commands/media.rs` and registered in `src-tauri/src/lib.rs`.
    - Added **Settings → Media** pane in `src/components/modals/SettingsModal.tsx` displaying detected player (`Bundled mpv`, `System mpv`, `VLC`, `IINA`), executable path, DVD/Blu-ray capabilities, JVM status, and honest commercial AACS notice.
    - Provided informative failure diagnostics for launch attempts instead of bare errno strings.
    - Created `THIRD-PARTY-NOTICES.md` with full GPLv2+ notice and upstream source links for `mpv`.
    - Updated `docs/installation.md` removing manual player requirements and documenting UHD AACS boundaries.
    - Preserved provider-neutral `WINDOWS_SIGN_PROVIDER` and Tauri signing secrets in `.github/workflows/release.yml`.

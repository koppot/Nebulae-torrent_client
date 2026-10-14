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

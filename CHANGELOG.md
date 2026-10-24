# Changelog

All notable changes to the **Nebulae** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Changed
- **macOS Restored to the Reference 0.2.3 Release DMG**: the macOS application is now the reference release build `Nebulae-macOS-arm64.dmg` (SHA-256 `b533c559b4afdee317b846beb1db594b522af50077f54f325f87e6bd2db5841d1`), built from git rev `7d6670d` — the last known-good macOS bundle. `/Applications/Nebulae.app` is a byte copy of `Nebulae.app` from that image (binary SHA-256 `3b54eb98f39d62c068a89164af3670857d669ba54ea215d84c69da922ba53d24`). The repository source is restored to the same revision and `src-tauri/resources/` is byte-identical to the DMG's runtime payload (`build-manifest.json`, `engine/`, `geo/`, `jre/`, `player/`, `vlc/lib/`, `vlc/plugins/`). The 66 MB static `resources/ffmpeg/ffmpeg` added later for seek-bar thumbnail extraction is removed. Verified: one visible main window at 1280×840, real UI rendered, playback starts with VideoToolbox hardware decoding and the `caopengllayer` / `glconv_cvpx` video output, clean session close, no crash reports. See `BUILD_NOTES.md` and `docs/macos-playback-repair-ledger.md`.
- **The GitHub v0.2.3 release now ships that same build**: the release assets had never been built from the `v0.2.3` tag — every release run was a `workflow_dispatch` from `main`, so the published macOS binaries came from a `main` HEAD that already contained the seek-bar thumbnail feature (`player_thumbnail_at`), and the failed 2026-10-23 21:00Z run left `latest.json` and `SHA256SUMS.txt` describing the *previous* run's binaries, so the updater manifest did not match the files it pointed at. The DMG, the `.dmg-app.zip` and the `.dmg-app.tar.gz` were all re-published from the same `Nebulae.app` and now extract to a bundle byte-identical to the installed application (`diff -rq` reports zero differences in every combination), the tarball was re-signed with the updater key, and `latest.json` / `SHA256SUMS.txt` were regenerated from the published assets. The updater signing key at `~/.tauri/nebulae-updater.key` is encrypted with an empty password, so the updater payload can be re-signed locally without a CI run. Verified end to end: `0.2.2` downloads and signature-verifies the `0.2.3` payload, stages it, and on relaunch the installed binary is the reference `3b54eb98…3d24` with working manual playback; `0.2.3` reports "Nebulae is up to date".

## [0.9.1] - 2026-10-15

### Fixed
- **UPnP Port Forwarding Was Never Enabled**: the engine session was built with `enable_upnp_port_forwarding: false` behind a comment deferring to a "Phase 4" UPnP implementation that does not exist, while the Connection settings exposed an `enable_upnp` switch (default on) and the connectivity diagnostics even reported "UPnP automatic port mapping is active" based on router probes alone. A passive client behind NAT cannot accept incoming peer connections, which starves swarms down to the few openly-connectable peers (a major contributor to "download uses only one peer" and speeds far below what the connection allows). The engine now honors the user setting (connections apply on restart, like the listen port).
- **Peer Count Underreported**: the Peers column counted only peers with an established connection and ignored peers currently being connected to, showing e.g. 1 while dozens of discovered peers sat in the connect queue. The count is now established + connecting.
- **Seeds Column Shows Real Swarm Counts (Tracker Scrape)**: librqbit discards the `complete`/`incomplete` swarm counts returned by announce responses (they are `#[allow(dead_code)]` in its tracker-comms crate), exposes no connected-seed flag in its peer API, and implements no scrape. The engine now runs its own client-side scrape loop per torrent (announce → scrape URL mapping, bencode response parser, 2-minute cadence with error backoff), and the seeds/peers figures and tracker status messages come from those live scrape results. The General tab gains a Tracker Status line showing the trackers' real announce/scrape message (including tracker failure reasons) instead of a hardcoded "Announce OK".
- **File/Folder Rename Was a Silent No-Op**: the Files tab rename actions invoked engine stubs that accepted the request and changed nothing on disk. The engine has no in-torrent path remapping (librqbit 9.0.1 exposes none, and a bare disk rename + recheck would re-download the data), so: the torrent's top-level folder now renames for real (detach without deleting files → rename the directory on disk → re-attach at the new path → forced recheck → display title and metadata preserved); actions that genuinely cannot work (individual files, nested folders) were removed from the UI with an explanatory tooltip and fail loudly with a clear error if invoked. The store no longer fakes optimistic renames when the engine rejects a request.
- **Professional Speed Display**: download/upload speeds are no longer presented from raw per-second engine reports (which burst between e.g. 4 MB/s and 0 B/s on small swarms). Matching qBittorrent (libtorrent's ~2s windowed rate) and Transmission (~2s decaying counter average), Nebulae now derives the displayed speed from cumulative byte counters over a ~3s decaying window, decays to zero when traffic stops, and snaps to an exact 0 B/s when a torrent has been idle for a few seconds.
- **Stable ETA**: the remaining-time estimate was computed from the raw spiky rate and jumped between 0, infinity and hours. It is now derived from the same smoothed speed the UI displays (remaining bytes / windowed rate), so it moves slowly and consistently, and shows infinity only for a genuinely stalled download.
- **Honest Seeds Column (superseded by tracker scrape above)**: the seeds column no longer shows a meaningless placeholder; see the tracker-scrape entry for the final behavior.
- **Progress Bar Volumes**: the torrent list progress bar now carries small monospace figures - downloaded bytes ("1.2 GiB of 4.5 GiB") while downloading, switching to uploaded bytes ("3.1 GiB uploaded") once the torrent is complete (both the manager table and the compact bar view).
- **Add-Torrent Modal Blank Screen (macOS)**: clicking Add could leave the entire window as a blank dark screen. A hook was called after the modal's early return, so opening the modal changed the number of hooks between renders and React unmounted the whole app tree (no error boundary). The effect moved above the return, an ESLint `rules-of-hooks` gate now runs inside `pnpm build` (this class of bug can no longer be built or tagged), and the full codebase was audited with the rule - this was the only instance.
- **Second Instance Fought Over the Listening Port (Recovery Screen)**:
  - Closing the window only hides Nebulae to the tray, so launching it again (for example right after installing an update) started a second process whose engine could not bind port 51413 (`Address already in use`), landing in the recovery screen. A new instance now automatically terminates the previously active Nebulae daemon (identified via an instance lock file, with PID-recycle protection) and takes over the engine port and persistence files.
  - When the takeover happens across versions, the persisted settings are re-stamped with the new version (and any newly introduced defaults), so settings are updated by the new install.

### Added
- **Honest Recovery Screen**: the recovery screen now fetches and displays the actual engine initialization error (collapsible technical details) and, for port conflicts, explains precisely that a Nebulae instance is likely still alive in the menu bar/tray or another client holds the port - instead of generic static advice only.

### Changed
- **Release Page Deduplicated**: the release workflow no longer uploads raw, unrenamed bundles through tauri-action alongside the canonical renamed artifacts (which made the release page show two copies of nearly every file). The release is created by a dedicated job with the canonical body, and only the canonical asset set is uploaded (Windows installer + standalone + updater signature, two macOS disk images + updater payloads, Linux deb/rpm/AppImage/standalone, latest.json, checksums, signing status).

## [0.9.0] - 2026-10-14

Release consolidation: all earlier `v0.1.0-beta.*` releases were removed from
the GitHub Releases page; this is the single supported release. The version
number moved to `0.9.0` so the in-app silent updater can finally resolve and
compare releases correctly (GitHub `/releases/latest` never resolves
pre-release-marked releases, and the old updater manifest pointed at stale
`v0.1.0-beta.1` assets).

### Added
- **Bundled Optical Disc Player (Zero-Prerequisite Playback)**:
  - **Windows (NSIS)**: Bundled pinned portable `mpv.exe` + `d3dcompiler_43.dll` compiled with `libdvdnav` and `libbluray` (`shinchiro/mpv-winbuild-cmake` tag `20261014`, SHA-256: `ab6236a2b78a3e9e93a156c1f638d3182ee906006d019c15d1b7b056a7c9a4c5`) into `$INSTDIR/resources/player/`.
  - **Linux (.deb, .rpm, AppImage)**: Fully self-contained on every Linux package format: the bundled `mpv` now carries its complete non-system shared-library tree (`scripts/bundle-linux-mpv.sh`: `ldd` dependency walk, `patchelf` RPATH wiring, self-containment verification); the `mpv` distribution-package dependency was REMOVED from `.deb`/`.rpm`. Requires host glibc >= 2.35 (Ubuntu 22.04+/Debian 12+ era).
  - **macOS (.dmg, .app)**: Bundled relocatable `mpv` binary and complete non-system dynamic library tree (`scripts/bundle-macos-mpv.sh`) with `@rpath` rewriting and hardened runtime codesigning.
  - **Bundled JRE (all platforms)**: A minimal Java runtime produced with `jlink` (Temurin 21 base, modules `java.se` + `jdk.unsupported` + `jdk.crypto.ec`) ships inside the package so BD-J interactive menus — including Java-based menus on 4K UHD discs — render with no user-installed Java. The bundled JRE is preferred over any system JVM.
  - **Canonical resource resolution**: Player and JRE discovery resolves through Tauri's resource directory first (`AppHandle::path().resource_dir()`), with per-layout fallbacks (NSIS `$INSTDIR`, `.app` bundle, deb/rpm `/usr/lib/nebulae`, AppImage `$APPDIR`), fixing packaged-layout detection.
  - **Package size**: The bundled player, library tree, and JRE add roughly 50-120 MB per platform before installer compression.
  - **Nebulae-Only Player Engine**: Playback uses only the bundled Nebulae player; no system mpv, VLC, IINA, OS default launcher, or system JVM is ever probed or used. A missing bundled player is reported honestly as an incomplete installation.
  - **External Disc Playback**: The Media Station offers "Play External Disc": pick the mounted DVD/Blu-ray/UHD volume and the bundled engine plays it with full menu navigation. macOS packages declare `NSRemovableVolumesUsageDescription` so removable-volume access is provisioned.
  - **ISO Menu Routing**: Routed DVD and Blu-ray ISO images (`.iso`, `.img`) through `dvdnav://` and `bd://` device paths, enabling interactive menu navigation for raw disc dumps.
  - **Settings → Media**: Added dedicated Media preferences tab displaying detected player engine, binary locations, format capabilities, BD-J Java runtime status, and commercial AACS limitations.
  - **Informative Launch Diagnostics**: Informative failure and warning reporting indicating attempted player binary and troubleshooting guidance.
  - **Third-Party GPL Compliance**: Shipped `THIRD-PARTY-NOTICES.md` documenting `mpv` (GPLv2+) license terms and upstream source repositories.
- **Optical Disc Cinema & Interactive Menu Launcher (Phase 4)**:
  - Real optical disc structure detector (`DiscDetector`) identifying DVD-Video (`VIDEO_TS`), DVD-Audio (`AUDIO_TS`), Blu-ray (`BDMV`), 4K UHD Blu-ray, and disc ISOs (`.iso`/`.img`).
  - Layer specification classification: DVD-5, DVD-9, BD-25, BD-50, UHD BD-66, and UHD BD-100.
  - Interactive menu launcher (`launch_disc_player`) with automated Java JRE environment injection (`JAVA_HOME`) for BD-J and 4K UHD menu rendering via `libbluray`.
  - Local cover art and disc scan detection (`scan_disc_artwork`) searching for authentic booklet and cover scans (`folder.jpg`, `cover.jpg`, `scans/`).
  - Video frame thumbnail generator with persistent caching under `Nebulae/internal/cache/thumbnails/`.
- **Rename Existing Content When Adding a Torrent**:
  - File and folder names are part of a torrent's info-hash, so renames happen on disk, not in the metainfo. The Add Torrent window can scan the save destination, match existing files to the torrent's expected layout by size and name, show the mapping (expected path on the left, current disk location editable on the right), and move files into the torrent's expected layout before adding, so the initial recheck finds the data and the torrent seeds with its original info-hash.
  - Path traversal and absolute paths are rejected; folders are created as needed and nothing is ever deleted.
- **Extended Tray Menu**:
  - The tray/taskbar menu now offers Pause All Torrents and Resume All Torrents (features that exist in Nebulae) alongside Open Nebulae and Quit Nebulae.
- **Multi-Disc Boxset & Series Aggregator**:
  - Clustered multi-disc releases into unified Boxset carousels in `SimpleMediaView` with individual disc selection pills.
- **Disk Space Cleaner & Completed Media Organizer**:
  - Volume disk space inspection (`get_volume_disk_space`) surfacing free headroom and usage gauges.
  - Temporary piece cache pruner (`clean_cache_files`) supporting dry-run inspection and safe cleanup.
  - Completed media organizer (`organize_completed_media`) automatically structuring finished discs into archival library paths.
- **CI Modernization**:
  - Updated `pnpm/action-setup` to `v4` across CI and release workflows, eliminating Node.js 20 runner deprecation warnings.
- **BEP 9 Magnet Link Resolver (`ut_metadata`)**:
  - Non-blocking asynchronous magnet link ingestion in `RqbitTorrentEngine` with honest `Downloading Metadata` status and live tracker resolution.
  - Added Magnet Link / URL tab in `AddTorrentModal` with instant `dn=` title preview and validation.
  - Added `pnpm.onlyBuiltDependencies: ["esbuild"]` in `package.json` matching Factory AI coordination guidance.

### Fixed
- **Windows Cross-Platform Compilation**:
  - Guarded Unix domain socket stream in `src-tauri/src/engine/rtorrent/scgi.rs` with `#[cfg(unix)]` to eliminate compilation failures on Windows.
  - Scoped `tauri::Manager` trait import in `src-tauri/src/updater.rs` under `#[cfg(windows)]` for `try_state`.
- **macOS: Adding Torrents Failed with "Read-only file system (os error 30)"**:
  - librqbit received the raw save destination, so a literal `~/Downloads` resolved against the process CWD (`/` for bundled GUI apps) onto the read-only system volume. Save destinations are now expanded (`~`), anchored to the home directory when relative, and pre-created with a clear error message when the folder cannot be written.
- **Drag-and-Drop Accepted Any File as a Torrent**:
  - Only `.torrent` and `.adc` metainfo files are accepted now (window-level drop filter plus validation in every Add Torrent path); anything else is rejected with a clear message instead of silently being treated as a torrent.
- **macOS: Window Could Not Be Restored After Close-to-Tray**:
  - Clicking the Dock icon after closing the window now shows and focuses the main window again (`RunEvent::Reopen` handling).
- **Multi-File Torrents Ignored the Torrent's Root Folder**:
  - librqbit writes multi-file contents flat into an explicitly provided output folder (its automatic sub-foldering only applies when no output folder is given), so downloads spilled their files directly into the chosen location. The engine now joins the torrent's root name itself: content lands in `<save destination>/<torrent name>/...` as expected, and the folder is created up front with a clear error if that fails.
- **Silent Updater Manifest Was Pointing at Stale Assets**:
  - `latest.json` previously reported version `v0.1.0-beta.1` with download URLs from that release (the workflow never exported the tag into the manifest step). The manifest now carries the real application version from `tauri.conf.json` and URLs of the actual release.

### Changed
- **Torrent Creator Defaults**:
  - "Start seeding when torrent is created" is now unchecked by default (opt-in).
  - A torrent file cannot be generated without at least one tracker URL; the creator shows "Please enter torrent tracker URL".

## [0.1.0-beta.8] - 2026-10-14

### Added
- **Remote Server Mode & Unraid Integration**:
  - Headless `nebulae-server` daemon and Docker container bridging rTorrent RPC/SCGI to the Nebulae desktop client.
  - End-user Unraid installation guides and template (`server/nebulae-server-unraid.xml`).
  - Remote engine adapter (`RemoteServerEngine` and `RtorrentRemoteEngine`) supporting full torrent monitoring, control, and pause/resume kill-switch flags over authenticated bearer token API.
- **Media Station & Local Title Parser (WS9)**:
  - Implemented local release title parser (`MediaTitleParser`) extracting clean titles, release years, resolutions, source formats, video codecs, audio standards, and editions without cloud mandates.
  - Enriched `TorrentItem` model with optional `media_meta` containing parsed release attributes.
  - Enhanced Simple Media Mode shelf cards with clean cinema titles, technical badges (Year, Resolution, Source, Edition), and instant default player streaming.
- **Tracker Keychain Safeguards & Private Swarm Tests (WS8)**:
  - Integrated OS Keychain storage (`keyring` crate) for personal tracker passkeys on account link (`link_adc_account`) and secure cleanup on unlink (`unlink_adc_account`).
  - Added BEP 27 private swarm integration tests confirming strict DHT/PEX isolation for Asian DVD Club private releases.
  - Added comprehensive unit tests for `SafeLogger` sanitization verifying full redaction of passkeys, query parameters, and auth tokens in application session logs.
- **Torrent Inspector, Speed Graph & Per-Torrent Controls (WS4)**:
  - Added Speed Graph tab with live SVG throughput curves powered by 500ms push telemetry.
  - Replaced hardcoded tracker status displays with honest tracker lists and per-tracker reannounce triggers.
  - Added interactive swarm toggles for sequential downloading, first/last piece header priority, and super-seeding with honest Observational banner (`docs/upstream-issues/rqbit-super-seeding.md`).
  - Added per-torrent speed limits dialog (Download/Upload KiB/s and ratio cutoffs) in General tab and context menu.
- **ADC Connect In-App Supervisor Documentation (WS7)**:
  - Updated `docs/connectivity.md` to document ADC Connect as an internal supervisor coordinating the 500ms kill-switch watcher and seeding rules loops, maintaining single signed binary compliance.
- **UI Modes, Color System & Subsystem Splash Initialization (WS2)**:
  - Applied `docs/ui-design.md` color tokens (`adc-dark`, `adc-cardDark`, `adc-cardBorderDark`, `adc-accent`, `adc-gold`, `adc-cyan`, `adc-emerald`, `adc-violet`) as single source of truth in `tailwind.config.js`.
  - Restored and wired `SimpleMediaView` with 3 horizontal shelves: Active Downloads (speeds, progress, ETA), Ready to Watch (posters, direct streaming player launch, folder jump), and Completed & Seeding (ratio, file size).
  - Implemented `CompactBarView` for low-footprint desktop seeding with aggregated transfer gauges, quick pause/resume all controls, and tray minimize.
  - Added UI Mode Switcher to `TitleBar` with deep-link parameter support (`?view=media|manager|compact`) and `localStorage` persistence.
  - Added real subsystem-init progress bar in `SplashScreen` displaying actual boot stages (engine, SQLite DB, network/VPN, and tracker readiness).
  - Exposed `pause_all_torrents` and `resume_all_torrents` IPC commands in Rust backend and wired to `useTorrentStore`.
- **Three-Tier Honesty Engine & Production Recovery (P0)**:
  - Eliminated mock engine fallback in release builds: production builds strictly disallow `MockTorrentEngine` unless explicitly forced via `NEBULAE_FORCE_MOCK_ENGINE=1` in debug mode.
  - Implemented single bounded engine startup retry with 500ms backoff on startup failure.
  - Added Tier 3 Recovery Screen (`RecoveryScreen.tsx`) presenting clear, non-alarming guidance with one-click "Restart Nebulae", "Copy Diagnostic Report", and "Quit Application" controls.
  - Production command renaming: promoted `add_torrent_stub`, `remove_torrent_stub`, `pause_torrent_stub`, and `resume_torrent_stub` to production IPC handlers `add_torrent`, `remove_torrent`, `pause_torrent`, and `resume_torrent`.
  - Honest error reporting: removed synthetic 1.5GB torrent fallback generator; added Tier 2 error banner to `AddTorrentModal`.
  - Honest connectivity diagnostics: eliminated hardcoded mock report and fake certificate fallback; displays honest neutral probing/untested states.
- **Real Engine Telemetry, Network Probing & Seeding Rules (P1)**:
  - Real `.torrent` export serialization: `export_torrent_file` extracts raw metainfo from active torrent handles or reconstructs valid bencoded torrent files, writing to selected destination with fallback to default Downloads folder.
  - Real live peer telemetry: integrated `librqbit` peer stats snapshot (`librqbit::http_api_types::PeerStatsFilter`) into `get_peer_telemetry`, surfacing live peer IP, connection protocol (uTP/TCP), fetched/uploaded metrics, error counts, and reciprocity scoring to Smart Peering.
  - Parallel asynchronous Connectivity Doctor probes: implemented real non-blocking network verification via UPnP SSDP M-SEARCH multicast (`239.255.255.250:1900`), NAT-PMP gateway probes (`5351`), dual-stack IPv4/IPv6 socket tests, DNS resolution latency probes (`1.1.1.1:53`), and tracker reachability checks (`tracker.asiandvdclub.org`).
  - Honest ADC Tracker profile statistics: removed fabricated 185.4 GB, 68.2 GB, 3420 BP placeholders; derived ratio and passkey status honestly from SQLite persistence and real torrent stats.
  - Seeding Rules & Seed Time Shield background loop: implemented `spawn_seeding_rules_loop` running periodic 5s evaluations to record active seed time in SQLite and auto-pause torrents exceeding target ratio limits.
- **Real VPN Subsystem, OS Keychain Secrets & Kill Switch (P2)**:
  - OS Keychain credential storage via `keyring` crate: WireGuard private keys and OpenVPN credentials are saved into the OS Credential Store (macOS Keychain, Windows Credential Manager, Linux Secret Service) rather than plaintext SQLite.
  - OpenVPN `.ovpn` configuration parser: implemented `ParsedOpenVpnProfile::parse_ovpn` parsing remote host/port/proto, tun/tap device, DNS options, and inline `<ca>`, `<cert>`, `<key>`, and `<tls-auth>` cryptographic blocks.
  - Real system network interface enumeration: integrated `network-interface` crate to discover real network adapters (`get_network_interfaces`), distinguishing VPN tunnel adapters from physical interfaces.
  - Honest VPN status: eliminated hardcoded `utun3`/`10.64.0.2`/`198.51.100.10:51820` values in `get_vpn_status`; surfaces genuine connected state, actual tunnel interface IP, and actual transferred data volumes.
  - 500ms fail-closed kill-switch watcher: implemented `spawn_kill_switch_watcher` background loop checking interface availability every 500ms when armed in `AdcOnlyVpn` or `SystemVpn` mode, automatically pausing all transfers upon tunnel loss.
- **Push Telemetry, Watched Folders, RSS Automation & Media Player (P3)**:
  - Real-time 500ms push telemetry: implemented `spawn_telemetry_loop` pushing live swarm states via `nebulae://telemetry` event emitter, replacing aggressive 1000ms frontend polling with instantaneous stream updates and quiet heartbeat fallback.
  - Watched Folders subsystem: implemented `spawn_watched_folder_loop` background directory monitor automatically loading dropped `.torrent` files into the engine and archiving to `.torrent.loaded`.
  - RSS feed automation engine: implemented `spawn_rss_loop` and `parse_rss_xml` parsing title, link, enclosure torrent URLs, downloading new releases matching rules, and persisting downloaded IDs in SQLite to eliminate duplicates.
  - Media Station streaming & external player launcher: implemented `launch_media_player` supporting VLC, IINA, and mpv, executing silently on Windows with `create_hidden_command` without popup terminal windows.
- **Core Architecture**:
  - Tauri v2 + React 19 + TypeScript + Tailwind CSS application foundation.
  - Rust backend with SQLite database persistence under isolated platform directory (`ADC/internal/`).
  - Cross-platform packaging configurations for macOS (Apple Silicon & Intel DMG) and Windows (NSIS 64-bit installer).
- **Silent Background Auto-Updater (Consent-First)**:
  - Cryptographically verified background update subsystem powered by `tauri-plugin-updater` with Minisign (`.sig`) signature verification against hardcoded public key.
  - Zero-telemetry opt-out model: Single "Automatically keep Nebulae updated (recommended)" toggle (ON by default, visible on first run and in Settings -> Behavior). When disabled, immediate early-return blocks all network requests and DNS lookups.
  - Quiet staged update delivery: Updates download silently in the background without dialogs, toasts, or progress popups, and stage to install strictly on natural application exit (preventing any disruption to active torrent transfers).
  - NSIS installer disclosure: Upfront notification in Windows installer notifying users that updates occur silently with opt-out available in Settings.
  - Automated release pipeline: Matrix CI builds bundle update artifacts (Windows binaries currently unsigned — see the code signing policy), with `aggregate-release` automatically generating the unified `latest.json` manifest.
- **User Interface & Visual Identity**:
  - Full-screen cinematic splash screen with hardware-accelerated starfield animations and fast startup fade.
  - Three distinct interface perspectives:
    - **Torrent Seeding Manager**: Detailed data tables, swarm metrics, ratio indicators, and inspector panel.
    - **Simple Media View**: Visual media poster gallery, metadata overlay, and single-click playback/inspection.
    - **Compact / Mini-bar Mode**: Minimal distraction mode for low-footprint desktop seeding.
  - Original Nebulae brand assets, app icons, and developer mark (*Der Sperre*).
- **Networking & Swarm Modules**:
  - **Smart Peering (Observational Tier)**: Quality scoring algorithm balancing throughput, latency, reliability, and locality tiers (LAN, ASN, Region, Country, Global).
  - **Connectivity Doctor**: Port listening verification, UPnP/NAT-PMP state handling, local listener confirmation, and verified connectability certificate generator.
  - **VPN Manager**: System network interface detection, kill-switch fail-closed state model, WireGuard and OpenVPN profile parsers.
  - **Community Tracker Profile & Seed Time Shield**: Community tracker account statistics, H&R / Seed Time Shield timer calculations, and target ratio projections.
  - **Der Sperre Advanced Diagnostics**: Internal directory paths, scrubbed log exporter, cache cleaner, and database integrity checks.
- **Testing & Tooling**:
  - Unit tests covering WireGuard configuration parsing, peer scoring algorithms, passkey masking, ratio projections, and certificate generation.
  - GitHub Actions CI workflow for multi-platform linting, building, and test execution.

### Security
- Passkey redaction pattern ensuring secrets are never persisted in plain SQLite or written to exported logs.
- Safe path isolation ensuring runtime databases and logs are segregated in internal directories.

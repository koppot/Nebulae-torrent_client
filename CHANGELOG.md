# Changelog

All notable changes to the **Nebulae** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Bundled Optical Disc Player (Zero-Prerequisite Playback)**:
  - **Windows (NSIS)**: Bundled pinned portable `mpv.exe` + `d3dcompiler_43.dll` compiled with `libdvdnav` and `libbluray` (`shinchiro/mpv-winbuild-cmake` tag `20261014`, SHA-256: `ab6236a2b78a3e9e93a156c1f638d3182ee906006d019c15d1b7b056a7c9a4c5`) into `$INSTDIR/resources/player/`.
  - **Linux (.deb, .rpm, AppImage)**: Added `mpv` to Debian and RPM package dependencies; bundled standalone `mpv` into AppImage resources directory with Linux JVM detection (`/usr/lib/jvm/default-java`, `/usr/lib/jvm/*`).
  - **macOS (.dmg, .app)**: Bundled relocatable `mpv` binary and complete non-system dynamic library tree (`scripts/bundle-macos-mpv.sh`) with `@rpath` rewriting and hardened runtime codesigning.
  - **Player Probe Order**: `find_optical_disc_player` probes bundled isolated player first, followed by system `mpv`, `VLC`, `IINA` (on macOS), and OS default launcher.
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

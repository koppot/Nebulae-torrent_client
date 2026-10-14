# Changelog

All notable changes to the **Nebulae** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
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
- **Silent Background Auto-Updater (SignPath Foundation Compliant)**:
  - Cryptographically verified background update subsystem powered by `tauri-plugin-updater` with Minisign (`.sig`) signature verification against hardcoded public key.
  - Zero-telemetry opt-out model: Single "Automatically keep Nebulae updated (recommended)" toggle (ON by default, visible on first run and in Settings -> Behavior). When disabled, immediate early-return blocks all network requests and DNS lookups.
  - Quiet staged update delivery: Updates download silently in the background without dialogs, toasts, or progress popups, and stage to install strictly on natural application exit (preventing any disruption to active torrent transfers).
  - NSIS installer disclosure: Upfront notification in Windows installer notifying users that updates occur silently with opt-out available in Settings.
  - Automated release pipeline: Matrix CI builds bundle and sign update artifacts, with `aggregate-release` automatically generating the unified `latest.json` manifest.
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

# Changelog

All notable changes to the **Nebulae** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
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

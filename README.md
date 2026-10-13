# ADC — Nebula

<p align="center">
  <img src="src/assets/app-icon.png" alt="ADC Logo" width="128" height="128" />
</p>

<p align="center">
  <strong>Torrent Client by Asian DVD Club</strong><br>
  <em>Native Desktop Application for Windows and macOS</em>
</p>

<p align="center">
  <a href="#about">About</a> •
  <a href="#core-features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#internal-storage">Internal Storage</a> •
  <a href="#development">Development</a>
</p>

---

## About

**ADC** (executable name for **Nebula Torrent Client**) is a modern peer-to-peer file-transfer client for people, creators, archives, communities, and organizations that share content with permission.

It combines a clean, media-friendly interface with advanced connectivity, privacy controls, VPN support, and powerful torrent-management tools.

### Designed for Communities

ADC is built for:
- **Creators** distributing their own work directly to audiences
- **Open-source projects** sharing large ISO images and binary releases
- **Archives and preservation communities** protecting cultural artifacts
- **Families and organizations** exchanging large media archives
- **Media communities** organizing lawful digital collections
- **Private communities** that operate with clear rules and member trust

ADC does not host, index, search, recommend, or distribute content.

---

## Core Features

- **Dual UI Modes**:
  - **Simple Media Mode**: Netflix-style shelves (*Active Downloads*, *Recently Added*, *Completed*, *Seeding*, *Ready to Watch*), poster-style media cards, and single-click playback.
  - **Torrent Manager Mode**: Power-user table view inspired by ruTorrent with advanced multi-column sorting, categories, smart filters, and a deep tabbed inspector (*Files*, *Trackers*, *Peers*, *Speed*, *Log*).
- **Smart Peering Engine**: Passive latency, throughput, and reciprocity telemetry with rolling quality scoring and 5 selectable peering modes (*Balanced*, *Speed First*, *Latency First*, *Local First*, *Privacy First*).
- **ADC Connectivity Doctor**: Detects listening port reachability, UPnP/NAT-PMP/PCP, dual-stack IPv4/IPv6 health, and tracker status with guided router port-forwarding instructions.
- **ADC-Only VPN Mode**: Supports importing WireGuard (`.conf`) and OpenVPN (`.ovpn`) profiles. Binds ADC torrent sockets directly to the VPN interface without redirecting your browser, email, or other applications. Includes an integrated Kill Switch that pauses transfers if the VPN drops.
- **AsianDVDClub Tracker Framework**: Secure OS Keychain passkey storage, ratio dashboard, Seed Time Shield, and private torrent safeguards (automatic permanent disable of DHT, PEX, and LSD on private swarms).
- **Cinematic Startup Splash**: High-polish multicolor archive splash screen displaying initialization progress and a community callout to `asiandvdclub.org`.

---

## Shipped Executables & Packages

All published binaries, installers, and protocol handlers strictly use the abbreviation **ADC**:

| Platform | Target Architecture | Package / Executable |
| :--- | :--- | :--- |
| **Windows x64** | `x86_64-pc-windows-msvc` | `ADC-Setup-x64.exe` (installs `ADC.exe`) |
| **macOS Apple Silicon** | `aarch64-apple-darwin` | `ADC-macOS-arm64.dmg` (mounts `ADC.app`) |
| **macOS Intel** | `x86_64-apple-darwin` | `ADC-macOS-x64.dmg` (mounts `ADC.app`) |
| **Protocol Handler** | Cross-platform | `adc://` and `magnet:?` |

---

## Installation

Extremely detailed, beginner-friendly installation instructions covering Windows SmartScreen, macOS Gatekeeper, firewall permissions, router port forwarding, and troubleshooting are available in:

📖 **[Full Installation Guide (docs/installation.md)](docs/installation.md)**

---

## Internal Storage & Diagnostics

Technical files are isolated from the main user interface:

- **macOS**: `~/Library/Application Support/ADC/internal/`
- **Windows**: `%APPDATA%\ADC\internal\`

```text
ADC/internal/
├── db/             # Local SQLite database (nebula.db)
├── logs/           # Redacted session logs (adc_session.log)
├── cache/          # Fast-resume state & temporary piece cache
├── engine/         # BitTorrent engine checkpoints & DHT cache
└── diagnostics/    # Crash and panic reports
```

### Developer Mode ("Der Sperre")
Developer diagnostics and internal log exports are housed in the **Show Advanced Diagnostics** section under Settings, identified by the developer mark (*Der Sperre*). Secrets, passkeys, and magnet URIs are automatically redacted.

---

## Local Development & Build

### Prerequisites
- Node.js 18+ and `pnpm`
- Rust 1.75+ with Cargo

### Running Locally
```bash
# Clone the repository
git clone https://github.com/nebula-torrent/nebula-torrent.git
cd nebula-torrent

# Install frontend dependencies
pnpm install

# Run desktop application in development mode
pnpm tauri dev
```

### Building Release Installers
```bash
# Build production bundle for the current host OS
pnpm tauri build
```

---

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE))
- MIT license ([LICENSE-MIT](LICENSE))

at your option.

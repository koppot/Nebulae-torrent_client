# Release Notes Template — Nebulae

## Nebulae — Privacy-Focused BitTorrent Client (v{{VERSION}})

Nebulae is an open-source BitTorrent client for public and private trackers, designed around user privacy, reliable seeding, and advanced torrent management. It includes optional Asian DVD Club (ADC) integration for community members.

### Windows code signing status

> **SignPath Foundation application declined due to insufficient project history. Windows beta builds are currently unsigned unless another signing provider is configured.** Windows artifacts in this release are **unsigned**; SmartScreen will warn on first run. Per-release status: `WINDOWS-SIGNING-STATUS.txt`. Policy, roles, and the provider-neutral rules for future signed releases: [Code signing policy](docs/code-signing-policy.md).

### Official Shipped Packages:
- **Windows x64 (10 / 11 / Server 2016+)**: `Nebulae-Setup-x64.exe` (NSIS Installer) / `nebulae.exe` (Standalone)
- **macOS Apple Silicon (M1/M2/M3/M4/M5)**: `Nebulae-macOS-arm64.dmg` (Disk Image) / `Nebulae-macOS-arm64.dmg-app.zip`
- **macOS Intel (x86_64)**: `Nebulae-macOS-x64.dmg` (Disk Image) / `Nebulae-macOS-x64.dmg-app.zip`
- **Linux x86_64 (glibc >= 2.35)**: `Nebulae-Linux-x86_64.deb` (.deb) / `Nebulae-Linux-x86_64.rpm` (.rpm) / `Nebulae-Linux-x86_64.AppImage` / `nebulae-linux-x86_64` (Standalone)

### What's New in this Release:
- High-performance, local-first BitTorrent engine with Tokio async scheduling.
- **Dual UI Modes**: Simple Media Mode and Advanced Torrent Manager Mode.
- Connectivity Doctor with guided router port forwarding and dual-stack IPv4/IPv6 health tests.
- Application-Only VPN mode with WireGuard / OpenVPN profile support and integrated kill switch.
- Optional Asian DVD Club (ADC) integration framework with OS Keychain passkey security, ratio dashboard, and private swarm safeguards.
- Hidden internal technical area (`Nebulae/internal/`) isolating databases and session logs.

### Installation Instructions
For beginner-friendly, step-by-step setup walkthroughs, SmartScreen notices, Gatekeeper verifications, and firewall permissions, please see our [Installation Guide](docs/installation.md).

### Checksums (SHA-256):
```text
[INSERT SHA-256 CHECKSUMS HERE]
```

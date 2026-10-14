# Release Notes Template — Nebulae

## Nebulae — Torrent Client by Asian DVD Club (v{{VERSION}})

Nebulae is a modern, general-purpose peer-to-peer file-transfer client built for creators, archives, communities, and organizations that share content with permission.

### Code signing policy

> **SignPath Foundation application pending; signing is not yet enabled.** Windows artifacts in this release are **unsigned** beta builds unless a signature is explicitly noted below. Policy and roles: [Code signing policy](docs/code-signing-policy.md). Once accepted: Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

### Official Shipped Packages:
- **Windows 10 / 11 (64-bit)**: `Nebulae-Setup-x64.exe` (NSIS Installer)
- **macOS Apple Silicon (M1/M2/M3/M4)**: `Nebulae-macOS-arm64.dmg` (Disk Image)
- **macOS Intel (x86_64)**: `Nebulae-macOS-x64.dmg` (Disk Image)

### What's New in this Release:
- High-performance, local-first BitTorrent engine with Tokio async scheduling.
- **Simple Media Mode** & **Advanced Torrent Manager Mode** with Asian DVD Club archive styling.
- Connectivity Doctor with guided router port forwarding and dual-stack IPv4/IPv6 health tests.
- Application-Only VPN mode with WireGuard / OpenVPN profile support and integrated kill switch.
- AsianDVDClub integration framework with OS Keychain passkey security and private swarm safeguards.
- Hidden internal technical area (`Nebulae/internal/`) isolating databases and session logs.

### Installation Instructions
For beginner-friendly, step-by-step setup walkthroughs, SmartScreen notices, Gatekeeper verifications, and firewall permissions, please see our [Installation Guide](docs/installation.md).

### Checksums (SHA-256):
```text
[INSERT SHA-256 CHECKSUMS HERE]
```

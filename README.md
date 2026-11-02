# Nebulae — Privacy-Focused BitTorrent Client

<p align="center">
  <img src="src/assets/app-icon.png" alt="Nebulae Crest" width="128" height="128" />
</p>

<p align="center">
  <strong>Open-source, privacy-focused BitTorrent client for public and private trackers</strong><br>
  <em>Cross-platform desktop application for Windows, macOS, and Linux</em>
</p>

<p align="center">
  <a href="https://nebulae.asiandvdclub.org"><strong>Official Website</strong></a> •
  <a href="https://github.com/koppot/Nebulae-torrent_client/releases">Downloads</a> •
  <a href="LICENSE">License (MIT)</a> •
  <a href="https://github.com/koppot/Nebulae-torrent_client/actions">CI Status</a>
</p>

<p align="center">
  <a href="#about">About</a> •
  <a href="#downloads">Downloads</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#features">Features</a> •
  <a href="#privacy-architecture">Privacy Architecture</a> •
  <a href="#compatibility">Compatibility & Onboarding</a> •
  <a href="#faq">FAQ</a> •
  <a href="#code-signing-policy">Code Signing</a> •
  <a href="#development">Development</a>
</p>

---

## About

Nebulae is an open-source, privacy-focused, general-purpose BitTorrent client with optional Asian DVD Club integration.

It is designed around user privacy, reliable long-term seeding, and advanced torrent management for public and private trackers. It is a general-purpose application—not an ADC-exclusive client. Asian DVD Club (ADC) integration is an optional capability for community members; ADC membership is not required for general torrent use.

### Built For

- **Open-source distribution**: Sharing Linux distributions, open-source software releases, and public datasets.
- **Independent creators**: Distributing digital media, music, and creative works directly to audiences.
- **Digital archivists & preservationists**: Maintaining long-term cultural and historical archives with consistent seeding tools.
- **Private & public community swarms**: Participating in standards-compatible BitTorrent swarms with fine-grained bandwidth and connection controls.

Nebulae does not host, index, search, recommend, or distribute content.

---

<a name="downloads"></a>
## Downloads & Supported Platforms

Official release packages are published on the **[GitHub Releases](https://github.com/koppot/Nebulae-torrent_client/releases)** page.

| Operating System | Architecture | Package / Installer | Description |
| :--- | :--- | :--- | :--- |
| **Windows** (10, 11, Server 2016+) | `x86_64` (64-bit) | `Nebulae-Setup-x64.exe`<br>`nebulae.exe` | Standard NSIS installer or standalone executable. |
| **macOS Apple Silicon** (M1–M5) | `aarch64` | `Nebulae-macOS-arm64.dmg`<br>`Nebulae-macOS-arm64.dmg-app.zip` | Apple Silicon disk image or direct application archive. |
| **macOS Intel** (macOS 11+) | `x86_64` | `Nebulae-macOS-x64.dmg`<br>`Nebulae-macOS-x64.dmg-app.zip` | Intel x86_64 disk image or direct application archive. |
| **Linux** (glibc ≥ 2.35) | `x86_64` | `Nebulae-Linux-x86_64.deb`<br>`Nebulae-Linux-x86_64.rpm`<br>`Nebulae-Linux-x86_64.AppImage`<br>`nebulae-linux-x86_64` | Debian/Ubuntu `.deb`, Fedora/RHEL/openSUSE `.rpm`, portable AppImage, or standalone binary. |

For installation walkthroughs, Gatekeeper/SmartScreen guidance, and runtime storage details, refer to the **[Installation Guide](docs/installation.md)**.

---

<a name="quick-start"></a>
## Quick Start (General Torrent Use)

1. **Launch Nebulae**: Open the application on your computer.
2. **Add a Torrent**:
   - **Drag and Drop**: Drag any `.torrent` file or magnet link directly into the Nebulae window.
   - **Toolbar / Menu**: Click the **Add Torrent (+)** button in the top navigation bar.
   - **File or Magnet**: Select a local `.torrent` file or paste a `magnet:?xt=urn:btih:...` link.
3. **Choose Save Destination**: Select your target download directory (defaults to your standard `Downloads` folder).
4. **Inspect Files & Priorities**: Optionally review file hierarchies, adjust individual piece priorities, or configure initial speed limits.
5. **Start Transfer**: Click **Add Torrent** to initiate transfer. Active transfers, bandwidth rates, peer counts, and swarm statistics appear immediately in the interface.

*No account creation, community registration, or passkey configuration is needed for general torrent use.*

---

<a name="features"></a>
## Features Organized by User Need

### 1. General Torrent Management
- **Dual UI Modes**:
  - **Simple Media Mode**: Clean, poster-driven shelf layout (*Active Downloads*, *Recently Added*, *Completed*, *Seeding*, *Ready to Watch*) with single-click playback.
  - **Torrent Manager Mode**: Power-user tabular interface with category filters, search, status sorting, and an in-depth inspector panel (*Files*, *Trackers*, *Peers*, *Speed*, *Log*).
- **Fine-Grained Swarm Controls**: Set per-torrent and global download/upload bandwidth limits, maximum connection ceilings, and seed ratio caps.
- **Content Inspection & Renaming**: Inspect metainfo contents prior to download, adjust file priorities, and rename torrent targets on disk while preserving swarm integrity.
- **Queue & Speed Scheduling**: Alternate speed limits (Turtle Mode), maximum active download/seed queue limits, and automatic stall management.

### 2. Privacy and Network Controls
- **Zero Analytics Architecture**: No third-party analytics trackers, advertising IDs, crash beacons, or background telemetry.
- **Safe Logging (`SafeLogger`)**: Disk logs automatically redact sensitive data, replacing passkeys, authentication tokens, and magnet URIs with redaction markers.
- **Internal Storage Isolation**: Technical runtime data (SQLite database, fast-resume state, session logs) is stored in a dedicated `internal/` folder separated from user media.
- **Application-Only VPN Socket Binding**: Bind the underlying BitTorrent engine sockets strictly to a designated VPN network interface without routing external system applications through the tunnel.
- **Fail-Closed Kill Switch**: Background watcher checks the VPN interface link state every 500ms; if the tunnel drops, transfers pause immediately and engine sockets rebind to `127.0.0.1` to prevent unencrypted traffic leaks.
- **Connectivity Diagnostics**: Built-in Connectivity Doctor evaluates listening port accessibility, UPnP / NAT-PMP port mapping health, dual-stack IPv4/IPv6 availability, and DNS response latency.

### 3. Seeding and Long-Term Preservation
- **Smart Peering Telemetry**: Passive observation of peer latency, throughput reciprocity, and connection stability to maintain healthy long-term swarms.
- **Racing Mode**: Dedicated high-priority profile that allocates expanded connection pools and elevated bandwidth priorities for time-sensitive swarms.
- **Optical Disc Cinema Engine**: Embedded media runtime capable of playing DVD (`VIDEO_TS`, ISO), Blu-ray (`BDMV`, ISO), and 4K UHD disc structures with interactive menu navigation. Zero external player installation required.

### 4. Optional Community Integrations
- **Asian DVD Club (ADC) Integration**:
  - **Optional Account Linking**: Connect an ADC username and passkey to monitor community ratio, uploaded/downloaded byte statistics, and bonus points directly in the client.
  - **Seed Time Shield**: Visual countdown and tracking against community minimum seeding obligations (e.g., 72 hours) to avoid hit-and-run penalties.
  - **Credential Security**: Passkeys are encrypted and stored in the native operating system credential vault where supported.
  - **Private Swarm Enforcement**: Automatic enforcement of private tracker protocol rules (disabling DHT, PEX, and LSD on swarms with `info.private = 1`).

---

<a name="privacy-architecture"></a>
## Privacy & Security Architecture

Nebulae is engineered around verified, concrete controls rather than unsupported marketing claims:

| Capability | What Traffic It Covers | What It Does Not Cover | Default State | Configuration Requirements | Failure Behavior |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Local Storage Isolation** | Application preferences, torrent metadata, resume data, logs, cache. | User-selected media download folder. | Enabled by default. | None. Files written to local `internal/` folders. | Corrupted SQLite databases trigger automatic schema repair. |
| **Safe Logging** | Application and engine runtime logs written to disk. | Raw network packets in transit over the wire. | Enabled by default. | None. Redaction filters run automatically at log write time. | Unmatched proprietary patterns may bypass regex filters. |
| **Client-Only VPN Binding** | Peer-to-peer data transfers and tracker announces handled by the BitTorrent engine. | Webview UI requests (e.g. poster fetches), external web links, other desktop applications. | Disabled by default ("No VPN"). | Requires selecting an active VPN network interface or importing a profile. | If interface is missing, transfers fail closed or stay paused. |
| **VPN Kill Switch** | Active torrent downloads and seeding transfers. | System OS traffic, system DNS queries, third-party software. | Armed when VPN mode is active. | Configured via VPN settings toggle. | Pauses active transfers and rebinds engine to `127.0.0.1` within 500ms of tunnel loss. |
| **Private Swarm Isolation** | Torrents marked with the standard BitTorrent private flag. | Public torrents without the private flag. | Enforced automatically. | Enabled per metainfo specification. | DHT, PEX, and local peer discovery are unconditionally disabled by the engine for private swarms while honoring user preferences for public swarms. |

### Concrete Privacy Realities & Boundaries

1. **Telemetry vs. Legitimate Network Connections**: Nebulae contains zero analytics SDKs or behavioral phone-home mechanisms. Legitimate outbound network traffic occurs only for:
   - Active peer transfers and tracker announces requested by your torrents.
   - Startup update checks against GitHub Releases (disabled with one toggle in Settings).
   - Media poster scrapes when Simple Media Mode is active.
   - Optional community status queries if an Asian DVD Club account is linked.
2. **Local Data Inventory**:
   - *Preferences & swarm metadata:* SQLite database (`internal/db/nebulae.db`).
   - *Engine resume checkpoints:* `internal/engine-state/`.
   - *Session logs:* `internal/logs/nebulae_session.log` (sanitized).
   - *Cache & posters:* `internal/cache/`.
   - *Credentials:* WireGuard private keys and OpenVPN credentials route to the OS credential vault. An optional ADC passkey is stored in the OS credential vault with a local SQLite settings record for session management.
   - *Downloaded media:* Placed strictly in your chosen destination directory.
3. **OS Credential Vault Dependencies**:
   - macOS: Apple Keychain Services.
   - Windows: Windows Credential Manager.
   - Linux: Requires an active D-Bus Secret Service provider (e.g. GNOME Keyring or KWallet). In headless or minimal Linux desktop setups lacking a secret service daemon, credentials fall back to the local SQLite database.
4. **VPN Socket Binding & Kill Switch Limits**:
   - Transmission daemon binds `bind-address-ipv4` to the active VPN interface IP.
   - Detection occurs via a 500ms polling watcher; during tunnel collapse, existing socket buffers may take a brief moment to drain.
   - This is an application-level interface binding mechanism, not a kernel packet filter (such as Windows WFP or macOS PF). It does not intercept system DNS or outside-process traffic.

For full architectural documentation, see **[Privacy & Security Architecture (docs/privacy.md)](docs/privacy.md)** and the **[VPN & Kill Switch Guide (docs/vpn.md)](docs/vpn.md)**.

---

<a name="compatibility"></a>
## Compatibility & Onboarding

### Supported Torrent Inputs
- **`.torrent` Files**: Added via drag-and-drop, open file dialog, or file-association launch.
- **Magnet Links**: Standard `magnet:?xt=urn:btih:...` URIs via clipboard, click, or manual entry.
- **HTTP / HTTPS URLs**: Direct download URLs pointing to valid `.torrent` metainfo.
- **Raw Metainfo Bytes**: In-memory inspection and addition for webview drops.

### Supported Tracker Types & Peer Discovery
- **Public & Private Trackers**: Full protocol support for standards-compliant HTTP, HTTPS, and UDP trackers.
- **Peer Discovery (DHT / PEX / LSD):** User preferences in Settings govern peer discovery for public torrents (Distributed Hash Table, Peer Exchange, and Local Service Discovery). Each control operates independently, applies dynamically to the running engine without restarting, and persists across restarts.
- **Private Swarm Protection**: Torrents marked with the standard BitTorrent private flag (`private: 1`) are restricted unconditionally by the engine. DHT, PEX, and local peer discovery remain strictly disabled for private torrents regardless of session preferences, preventing peer IP leakage while preserving public discovery.
- **Private Tracker Approval**: While Nebulae implements standard BitTorrent protocol specifications, individual private trackers enforce specific whitelists and client approval policies. Always check your tracker's client guidelines.

### Optional Community Features
- Asian DVD Club (ADC) integration is entirely optional.
- You do **not** need an ADC account or passkey to download, seed, or manage ordinary torrents.
- If you do not configure an ADC account, community-specific widgets remain inactive while all standard BitTorrent functionality operates without restriction.

---

<a name="faq"></a>
## Frequently Asked Questions (FAQ)

### Q: Is Nebulae only for Asian DVD Club?
**A:** No. Nebulae is an open-source, general-purpose BitTorrent client. It connects to any standard public or private tracker. ADC integration is an optional module that adds community-specific ratio and seed-time tracking.

### Q: Do I need an ADC account to use Nebulae?
**A:** No. General torrent operations—downloading, seeding, file management, speed capping, and VPN binding—do not require an ADC account or credentials. An ADC account is only required if you choose to link your profile for community ratio monitoring.

### Q: Can I use Nebulae with other trackers?
**A:** Yes. Nebulae works with standards-compatible public and private trackers over HTTP, HTTPS, and UDP. Note that private trackers often maintain private client approval whitelists; confirm your tracker's policy before use.

### Q: Is Nebulae a torrent tracker?
**A:** No. Nebulae is a BitTorrent client (desktop application) powered by an embedded Transmission engine backend. It connects to external trackers and swarm peers to transfer files.

### Q: What privacy protections does Nebulae provide?
**A:** Nebulae operates with zero analytics or telemetry, stores data in local isolated storage, sanitizes disk logs to redact secrets, isolates internal technical files, and supports locking BitTorrent traffic to a specific VPN interface with an automatic fail-closed kill switch. Refer to [Privacy Architecture](#privacy-architecture) for exact boundaries and limitations.

---

<a name="code-signing-policy"></a>
## Code Signing & Release Verification

> **Windows Release Notice:** Windows builds are currently unsigned beta releases. On first run, Windows SmartScreen will display an unrecognized publisher warning ("Windows protected your PC"). Click **More info** → **Run anyway** to proceed.

- **Cryptographic Minisign Verification**: All release manifests and auto-update payloads are cryptographically signed with the official public key:
  ```text
  RWRBb7y7i6osRaR3dP9YGfYFxS5tyi5NIF+Xk+c+pM11p0v+f0Vqye5X
  ```
- **Checksum Catalog**: Every release includes a verified `SHA256SUMS.txt` listing digests for all platform packages.
- Full code signing governance: **[docs/code-signing-policy.md](docs/code-signing-policy.md)**.

---

<a name="development"></a>
## Local Development & Build

### Prerequisites
- [Node.js](https://nodejs.org/) (v18+) and [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/) (1.75+) with `cargo`

### Setup and Development
```bash
# Clone the repository
git clone https://github.com/koppot/Nebulae-torrent_client.git
cd Nebulae-torrent_client

# Install frontend dependencies
pnpm install

# Run application in development mode
pnpm tauri dev
```

### Packaging & Release Builds
```bash
# Compile production desktop bundle for your current OS
pnpm tauri build
```

---

## License

Nebulae is open-source software licensed under the **[MIT License](LICENSE)**.

# Nebulae Privacy & Security Architecture

## 1. Core Privacy Philosophy

Nebulae is engineered on a strict **local-first** architecture with **zero analytics or usage tracking**:

- **Zero Analytics & Diagnostic Telemetry**: Nebulae does not embed third-party analytics SDKs (such as Google Analytics, Mixpanel, or Sentry), crash-reporting phone-home daemons, or background behavior telemetry.
- **Legitimate Outbound Network Traffic**: Outbound network connections are strictly limited to necessary protocol functions:
  1. *BitTorrent Swarm & Tracker Traffic*: Announce requests to configured HTTP/HTTPS/UDP trackers and peer-to-peer data transfers.
  2. *Software Updates*: When enabled, a single startup HTTPS check to GitHub Releases (`https://github.com/koppot/Nebulae-torrent_client/releases/latest/download/latest.json`). This can be toggled off at any time in Settings.
  3. *Media Metadata Scrapes*: When Simple Media Mode is active, public metadata and poster queries (e.g., TMDB).
  4. *Optional ADC Integration*: If an Asian DVD Club account is explicitly linked by the user, authenticated HTTPS status calls to `asiandvdclub.org`.
- **Local Data Inventory**:
  - **Settings, torrent metadata & transfer statistics**: Stored locally in SQLite (`internal/db/nebulae.db`).
  - **BitTorrent engine state & checkpoints**: Stored in `internal/engine-state/`.
  - **Session logs**: Stored in `internal/logs/nebulae_session.log` with automatic regex redaction.
  - **Temporary piece cache & media posters**: Stored in `internal/cache/`.
  - **Credentials & secrets**: WireGuard private keys and OpenVPN credentials are routed to the operating system credential vault. The optional ADC passkey is stored in the OS credential vault with a local SQLite settings record for session management.
  - **Downloaded content**: Placed strictly in the user-selected save destination.

---

## 2. Storage Isolation & Runtime Paths

Technical runtime artifacts are isolated within dedicated `internal/` folders, keeping them separate from user media:

- **macOS**: `~/Library/Application Support/Nebulae/internal/`
- **Windows**: `%APPDATA%\Nebulae\internal\` (and `%LOCALAPPDATA%\Nebulae\internal\`)
- **Linux**: `~/.local/share/Nebulae/internal/`

```text
internal/
├── db/             # Local SQLite database (nebulae.db)
├── logs/           # Redacted session logs (nebulae_session.log)
├── cache/          # Fast-resume state & temporary piece cache
├── engine-state/   # BitTorrent engine checkpoints & DHT cache
└── diagnostics/    # Crash and panic reports
```

### Path Resolution & Migration Notice
- **Current Runtime Paths**: Current releases resolve user data under the `Nebulae` directory name across all platforms (`dirs::data_dir().join("Nebulae")`).
- **Webview Runtime State**: Webview state and framework-managed caches persist under the application bundle identifier `org.asiandvdclub.nebulae` (e.g., `~/Library/Application Support/org.asiandvdclub.nebulae/` on macOS and `%LOCALAPPDATA%\org.asiandvdclub.nebulae\` on Windows).
- **Legacy Paths**: Earlier alpha/preview builds (pre-v0.2.3) stored data under legacy paths (`~/Library/Application Support/ADC/` on macOS, `%APPDATA%\ADC\` on Windows, and `~/.config/ADC/` on Linux). Fresh installations do not use these legacy paths.

---

## 3. Sanitized Logging (`SafeLogger`)

By default, internal logging scrubs sensitive values before writing to disk:
- Magnet links (`magnet:?xt=...`) are replaced with `[REDACTED_MAGNET_LINK]`.
- Private tracker announce URLs with embedded passkeys are replaced with `[REDACTED_TRACKER_URL]`.
- Authentication tokens and API parameters are stripped.
- WireGuard private keys, passwords, and disk paths are excluded from automated log outputs.

---

## 4. Developer Diagnostics ("Der Sperre")

Access to diagnostic data is guarded by the **Show Advanced Diagnostics** section in Settings, marked by the internal developer icon (*Der Sperre*):
- **Open Logs Folder**: Opens the isolated local directory in your system file manager.
- **Copy Diagnostic Summary**: Copies a sanitized JSON summary (OS version, architecture, engine status) without revealing file paths or private hashes.
- **Clear Internal Cache**: Safely purges temporary piece caches without altering your downloaded media.
- **Export Logs**: Exports redacted logs for voluntary troubleshooting on community forums.

---

## 5. Security & Isolation

- **Path Traversal Mitigation**: File paths within `.torrent` metadata (e.g. `../../Windows/System32` or `../../.bashrc`) are strictly sanitized. Any file path attempting to escape the selected download target root directory is rejected immediately.
- **Tauri 2 IPC Isolation**: The frontend webview is isolated by Tauri 2 capability definitions (`capabilities/default.json`). Arbitrary operating system command execution is blocked.
- **Credential Storage Implementation & Limitations**:
  - **macOS**: Apple Keychain Services (`Security.framework`).
  - **Windows**: Windows Credential Manager (`wincred`).
  - **Linux**: D-Bus Secret Service API (via the `keyring` crate). Requires a running Secret Service daemon (such as GNOME Keyring or KWallet). In headless or minimal desktop environments lacking a Secret Service provider, credentials fall back to the local SQLite database.

---

## 6. VPN Socket Binding & Kill Switch Boundaries

Nebulae includes application-level network controls designed to constrain BitTorrent traffic:

- **Interface Binding**: When an application VPN interface is selected, Nebulae binds Transmission daemon's listening and peer sockets (`bind-address-ipv4`) to that interface's IPv4 address.
- **Fail-Closed Kill Switch**: A background Tokio watcher polls interface link status every 500ms. If the interface disappears or loses its IPv4 address, Nebulae calls `torrent-stop` on all active torrents and rebinds `bind-address-ipv4` to `127.0.0.1`.
- **Runtime Boundaries & Demonstration**:
  - *Application-level, not kernel-level*: This mechanism operates via socket binding and daemon RPC commands; it is not a kernel firewall filter (such as Windows Filtering Platform or macOS Packet Filter).
  - *Detection timing*: The kill switch operates on a 500ms polling cycle. While the watcher responds upon detection, in-flight socket buffers may drain during that window.
  - *Network scope*: Interface binding applies to Transmission's peer-to-peer and tracker transfers. It does not control system-wide DNS resolution, third-party applications, or external web browsing. Users requiring unconditional network-wide protection should employ operating-system or router firewall rules.

---

## 7. Automatic Software Updates & Telemetry Boundaries

Nebulae incorporates an optional background update subsystem:

- **Default State**: Enabled by default (`auto_update_enabled: true`) to ensure security patches and tracker updates are received. Can be disabled at any time in **Settings → Behavior**.
- **Network Scope**: Checks once on application launch via HTTPS against GitHub Releases (`latest.json`). Only standard HTTPS transit headers are sent; no user identifiers, hardware UUIDs, or torrent histories are transmitted.
- **Zero Traffic When Disabled**: When unchecked, the update check aborts before opening any socket or issuing any DNS query.
- **Cryptographic Verification**: Every payload is verified against the compiled-in Minisign public key (`RWRBb7y7i6osRaR3dP9YGfYFxS5tyi5NIF+Xk+c+pM11p0v+f0Vqye5X`). Unsigned or mismatched payloads are rejected immediately.
- **Linux Boundary**: Native `.deb` and `.rpm` packages update via system package managers (`apt`, `dnf`); the in-app updater targets portable AppImage builds only.

---

## 8. Peer Discovery & Swarm Privacy

- **Public Swarms**: Peer discovery mechanisms (DHT - Distributed Hash Table, PEX - Peer Exchange, LSD - Local Service Discovery) follow user preferences configured in Settings. These settings can be toggled independently, apply immediately to the running engine without restart, and persist across restarts.
- **Private Swarms**: Torrents marked with the BitTorrent `private: 1` flag are strictly isolated by the engine. In accordance with standard protocol specifications (BEP 27), DHT, PEX, and Local Service Discovery are unconditionally disabled for private swarms regardless of session preferences, preventing peer IP disclosure to public networks or unauthenticated third parties.

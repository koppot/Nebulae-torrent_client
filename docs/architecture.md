# Nebulae — Technical Architecture

## 1. Overview & Identity

- **Application Brand**: Nebulae
- **Product Subtitle**: Torrent Client by Asian DVD Club
- **Binary & Executable Name**: `nebulae` (`nebulae.exe`, `Nebulae.app`)
- **Bundle Identifier**: `org.asiandvdclub.nebulae`
- **Protocol Schemes**: `adc://`, `magnet:?`
- **Primary Goals**:
  - Native desktop BitTorrent client for Windows and macOS.
  - Zero cloud dependencies or telemetry; 100% local-first data architecture.
  - Dual presentation modes: **Simple Media Mode** (casual media shelf view) and **Torrent Manager Mode** (power-user rTorrent/ruTorrent inspired table).
  - High performance, low memory footprint, safe concurrency via Rust and Tauri 2.

---

## 2. System Architecture Diagram

```
+---------------------------------------------------------------------------------------+
|                               Tauri 2 Native Desktop Shell                            |
|                                                                                       |
|  +---------------------------------------------------------------------------------+  |
|  |                         Webview Presentation Layer (React)                      |  |
|  |                                                                                 |  |
|  |   [Cinematic Startup Splash]                                                    |  |
|  |     - Multicolor Asian-cinema archive aesthetic                                 |  |
|  |     - Community callout to asiandvdclub.org                                     |  |
|  |     - Subsystem initialization progress bar                                     |  |
|  |                                                                                 |  |
|  |   [Simple Media Mode]                  [Advanced Torrent Manager Mode]          |  |
|  |     - Active Downloads shelf             - Dense, sortable data grid             |  |
|  |     - Ready to Watch shelf               - Action toolbar (pause/resume/check)  |  |
|  |     - Completed / Seeding shelf          - Tabbed inspector (files/trackers)    |  |
|  |                                                                                 |  |
|  |   [In-App Installation & Help Guide]   [User Settings & Advanced Diagnostics]   |  |
|  |     - Step-by-step Windows/macOS tabs    - User-friendly options only           |  |
|  |     - SmartScreen & Gatekeeper bypass    - Hidden Diagnostics Drawer            |  |
|  +---------------------------------------------------------------------------------+  |
|                                            |                                          |
|                               IPC Commands | Telemetry Events (500ms)                 |
|                                            v                                          |
|  +---------------------------------------------------------------------------------+  |
|  |                            Rust Core Subsystems (src-tauri)                     |  |
|  |                                                                                 |  |
|  |   +--------------------------+  +-------------------+  +--------------------+   |  |
|  |   |     Tauri IPC Router     |  | Sanitized Logger  |  | Telemetry Streamer |   |  |
|  |   | (diagnostics, settings)  |  | (no URI leaks)    |  | (rate aggregators) |   |  |
|  |   +--------------------------+  +-------------------+  +--------------------+   |  |
|  |                 |                         |                                     |  |
|  |                 v                         v                                     |  |
|  |   +-------------------------------------------------------------------------+   |  |
|  |   |                   Isolated Internal Storage Manager                     |   |  |
|  |   |                         Nebulae/internal/                               |   |  |
|  |   |   ├── db/          -> Local SQLite store (nebulae.db)                   |   |  |
|  |   |   ├── logs/        -> Redacted session logs (nebulae_session.log)       |   |  |
|  |   |   ├── cache/       -> Fast resume states & temp piece cache             |   |  |
|  |   |   ├── engine-state/-> Engine session checkpoints & DHT cache            |   |  |
|  |   |   └── diagnostics/ -> Crash & panic reports                             |   |  |
|  |   +-------------------------------------------------------------------------+   |  |
|  |                                           |                                         |  |
|  |                                           v                                         |  |
|  |   +-------------------------------------------------------------------------+   |  |
|  |   |                     BitTorrent Engine Architecture                      |   |  |
|  |   |   - Tokio 1.x async runtime                                             |   |  |
|  |   |   - BEP 3 (core), BEP 5 (DHT), BEP 9 (metadata/magnet), BEP 20 (PEX)    |   |  |
|  |   |   - Sequential piece ordering for media preview                         |   |  |
|  |   |   - Selective file download & priority trees                            |   |  |
|  |   +-------------------------------------------------------------------------+   |  |
|  +---------------------------------------------------------------------------------+  |
+---------------------------------------------------------------------------------------+
```

---

## 3. Storage Isolation: `Nebulae/internal/`

All technical data is segregated from the user experience:

- **Windows**: `%LOCALAPPDATA%\Nebulae\internal\`
- **macOS**: `~/Library/Application Support/Nebulae/internal/`

### Subdirectory Mapping:
1. `internal/db/`: Contains `nebulae.db` (SQLite 3), storing user preferences, torrent labels, tags, and RSS feeds.
2. `internal/logs/`: Contains `nebulae_session.log`. All entries pass through `SafeLogger`, which strictly sanitizes magnet URIs, passkeys, and authentication tokens.
3. `internal/cache/`: Temporary piece buffer and resume checkpoints.
4. `internal/engine-state/`: Engine state files and DHT routing checkpoints.
5. `internal/diagnostics/`: Panic hooks and crash reports.

The main UI never presents direct filesystem paths to these files. Access is restricted to the **Show Advanced Diagnostics** section in Settings.

---

## 4. BitTorrent Engine Strategy: `librqbit` Integration

### 4.1. Decision & Selection Rationale
In Phase 1, Nebulae evaluated two paths for replacing the initial transfer engine mock-up:
- **Option (a)**: Integrating an actively maintained, permissively licensed Rust engine crate (`librqbit v9.0.1`, Apache-2.0).
- **Option (b)**: Implementing the full BitTorrent wire protocol, rarest-first picker, and storage pipeline in-house.

**Decision**: **Option (a)** was selected.
1. **Single Signed Binary**: `librqbit` is embedded as a standard in-process Rust crate dependency. It requires no external background daemons or sidecars, directly satisfying SignPath Foundation's requirement that open-source certificates sign only first-party application binaries.
2. **License Compliance**: `librqbit` is licensed under **Apache-2.0**, which is fully compatible with Nebulae's top-level **MIT** license. SignPath's third-party dependency review requires permissive licenses with clear component provenance.
3. **Pure Rust & Native Tokio 1.x**: Cleanly compiles without C++/Boost dependencies across all four release targets (Windows MSVC x64, macOS Apple Silicon arm64, macOS Intel x64, Linux GNU x86_64).
4. **Standards & Private Swarm Protection**: Implements BEP 3 (core), BEP 5 (DHT), BEP 9 (metadata/magnet), BEP 10 (extension protocol), BEP 15 (UDP tracker), BEP 20 (PEX), and BEP 27 (private torrents). BEP 27 strictly disables DHT, PEX, and Local Peer Discovery on private torrents (such as AsianDVDClub), protecting user passkeys from leaking to public swarms.

### 4.2. Pinned Dependency & Feature Flags
- **Crate**: `librqbit = "9.0.1"`
- **License**: Apache-2.0
- **Feature Flags**: `default-features = false`, `features = ["rust-tls"]`
- **TLS Stack**: Configured with `rust-tls` to provide identical, memory-safe TLS handling for HTTPS trackers across Windows, macOS, and Linux, eliminating native OpenSSL linking issues in release pipelines.

### 4.3. Listen Port Configuration
The engine session does not bind to an uncoordinated ephemeral port. The listening port is initialized and updated directly from Nebulae's user settings (`listening_port`, default `51413`). This ensures that Connectivity Doctor probes, UPnP/NAT-PMP port mappings (Phase 4), and local firewall rules target the exact bound socket.

### 4.4. Two-Tier Persistence Architecture
To prevent state corruption and race conditions, storage responsibilities are cleanly bifurcated:
1. **Engine Session State (`internal/engine-state/` and `internal/cache/`)**: Handled entirely by `librqbit` native session serialization. Manages fast-resume data, piece verification bitfields, block hashes, and DHT routing caches.
2. **Application Metadata (`internal/db/nebulae.db`)**: Handled by Nebulae's SQLite database. Mirrors only UI-facing metadata: display names, custom categories, tags, user-configured bandwidth limits, and download directory paths.

### 4.5. Engine Log Sanitization & Passkey Protection
`librqbit` logs announce activity and HTTP/UDP tracker failures, which inherently contain private tracker URLs with sensitive passkeys. The application's `tracing` subscriber filters and redacts tracker query strings before log records reach `SafeLogger` on disk, upholding the zero-telemetry and credential privacy commitments in `docs/privacy.md`.

### 4.6. Architectural Risk Flag: Phase 5 VPN Socket Binding
`librqbit` does not currently provide native socket options to bind peer connections to a specific network interface (`SO_BINDTODEVICE` on Linux, `IP_BOUND_IF` on macOS, `IP_UNICAST_IF` on Windows). 
- **Phase 5 Decision Point**: Phase 5 (VPN integration) will require either upstreaming socket binding support to `librqbit` or maintaining a public, permissively licensed fork. Under SignPath Foundation guidelines, any modified dependency must exist as a public open-source repository with full build provenance rather than an opaque private patch.

### 4.7. Test & Mock Isolation
The `TorrentEngine` trait in `src-tauri/src/engine/traits.rs` is extended additively. `MockTorrentEngine` is preserved under a debug/test flag (`#[cfg(test)]`) to provide deterministic unit and regression testing for UI components, while production application wiring in `src-tauri/src/lib.rs` executes the real engine.


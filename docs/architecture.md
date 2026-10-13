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

## 4. BitTorrent Engine: Why `librqbit`?

`librqbit` was chosen as the engine for the following verified reasons:
1. **Pure Rust Implementation**: Zero C++ dependencies (no Boost or rasterbar compilation friction on Windows MSVC or Apple Silicon).
2. **Native Tokio 1.x Integration**: Seamless async task multiplexing within Tauri's thread pool.
3. **Sequential Downloading**: Built-in sequential piece selection for streaming and early video verification.
4. **Comprehensive BEP Support**: Covers BEP 3, 5, 9, 10, 15, and 20.

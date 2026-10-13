# Nebulae Privacy & Security Architecture

## 1. Core Privacy Philosophy

Nebulae (Torrent Client by Asian DVD Club) is engineered on a strict **local-first, zero-telemetry** architecture.

- **Zero Phone-Home / Analytics**: Nebulae does not include Google Analytics, Sentry, Mixpanel, telemetry pingbacks, or background usage tracking.
- **No Cloud Account Mandatory**: All core BitTorrent operations, settings, and media management function entirely on your local machine without requiring any account.
- **Your Data Remains Yours**: Download histories, torrent files, configurations, and peer statistics are stored exclusively in your local SQLite database on your own storage drive.

---

## 2. Internal Storage Isolation (`Nebulae/internal/`)

All technical files are strictly separated from user-facing document folders and normal views:

- **macOS**: `~/Library/Application Support/Nebulae/internal/`
- **Windows**: `%LOCALAPPDATA%\Nebulae\internal\`

```text
internal/
├── db/             # Local SQLite database (nebulae.db)
├── logs/           # Redacted session logs (nebulae_session.log)
├── cache/          # Fast-resume state & temporary piece cache
├── engine-state/   # BitTorrent engine checkpoints & DHT cache
└── diagnostics/    # Crash and panic reports
```

Normal settings screens present only user-friendly configurations. Technical file paths are never exposed in the main interface.

---

## 3. Sanitized Logging (`SafeLogger`)

By default, the internal logging subsystem sanitizes all logged entries:
- Magnet links (`magnet:?xt=...`) are redacted to `[REDACTED_MAGNET_LINK]`.
- Private tracker announce URLs with passkeys are redacted to `[REDACTED_TRACKER_URL]`.
- Torrent file titles and private paths are excluded from automated log strings.
- Passwords and private keys are never written to disk logs.

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
- **Keychain Storage**: All VPN credentials, passkeys, and authentication secrets are stored directly in the macOS Keychain or Windows Credential Manager.

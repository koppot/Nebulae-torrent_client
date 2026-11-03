# Import from qBittorrent (with Optional Removal) — Architectural Plan

## 1. Phase 0 Discovery Findings

### 1.1 Stack and Build System
* **Languages**: Rust (2021 edition) for backend logic and daemon RPC; TypeScript (v5.6) with React 18 and Tailwind CSS for the frontend.
* **UI Framework**: Tauri v2 (`@tauri-apps/api: ^2.0.0`, `tauri: 2`). Frameless window with custom title bar and unified dark space design language.
* **Build & Packaging**:
  * Frontend: Vite + TypeScript (`pnpm build`).
  * Backend: Cargo (`cargo build`, `cargo test`).
  * Windows Packaging: Tauri CLI NSIS bundle (`.exe`), MSIX/Wix capable.
  * macOS Packaging: Tauri CLI DMG / `.app` bundle (Universal/arm64) signed and notarized via `scripts/fix-dmg-icon.sh` and custom signing tooling.

### 1.2 Subsystem Locations
* **Manager Window & Settings UI**:
  * Manager window: `src/components/manager/TorrentManagerView.tsx`.
  * Settings modal: `src/components/modals/SettingsModal.tsx` (sidebar tabs: Behavior, Downloads, Connection, Speed, BitTorrent, RSS, Web UI, Server, Media, Advanced).
  * Main Title Bar Tools Menu: `src/components/layout/TitleBar.tsx` (features Tools dropdown: Connectivity Doctor, VPN & Kill Switch, Tracker Dashboard).
* **Torrent Engine & Persistence**:
  * Engine: Transmission sidecar daemon orchestrated via `TransmissionTorrentEngine` (`src-tauri/src/engine/transmission.rs`) implementing `TorrentEngine` (`src-tauri/src/engine/traits.rs`).
  * Database: SQLite (`nebulae.db`) managed by `Database` (`src-tauri/src/db/mod.rs`), storing `torrent_metadata`, `settings`, `vpn_profiles`, and `seed_time_records`.
* **Add Torrent & Renames Mechanics**:
  * `add_torrent` / `add_torrent_from_bytes` in `src-tauri/src/engine/transmission.rs` accepts raw metainfo bytes or magnet URLs, normalizes directories, and adds to Transmission RPC paused or active.
  * File renaming in Transmission is handled via `torrent-rename-path` RPC (`rename_torrent_file` and `rename_torrent_folder`).
* **"Last Checked / Rechecked" Timestamp**:
  * Does NOT exist in current `TorrentItem` or `torrent_metadata` table.
  * Solution: Add `last_checked` (`INTEGER`, nullable Unix timestamp) to `torrent_metadata`, add `last_checked: Option<u64>` to `TorrentItem`, and introduce a sortable "Last checked" column in `TorrentManagerView.tsx`.
* **Nebula Profile & Data Directory**:
  * Base data dir resolved via `dirs::data_dir().join("Nebulae")`:
    * macOS: `~/Library/Application Support/Nebulae`
    * Windows: `%APPDATA%\Nebulae`
  * Logs: `<data_dir>/internal/logs`
  * Backups: `<data_dir>/qbit-import-backups/<YYYYMMDD-HHMMSS>/`

---

## 2. Architectural Analysis & Least Invasive Resolutions

1. **Transmission vs. qBittorrent Feature Parity**:
   * *First/Last piece priority*: Transmission JSON-RPC does not expose piece-level priority flags.
     * *Resolution*: Persist `first_last_piece_prio` in `torrent_metadata` for metadata parity, and explicitly note in the import summary that piece priority is recorded in metadata.
   * *Sequential download*: Supported via Transmission's `sequential_download` JSON-RPC property.
   * *File priorities*: qBittorrent uses numeric priorities (0=skip, 1=normal, 7=maximum). Transmission uses `files-unwanted` for 0, `priority-normal` for 1, and `priority-high` for >1.
2. **Path and Rename Resolution (`mapped_files` & Content Layouts)**:
   * qBittorrent supports three content layouts: `Original`, `Subfolder`, `NoSubfolder`.
   * When importing, the effective disk root must be determined accurately based on the layout and save path.
   * After adding the torrent in the paused state to Transmission, each entry in `mapped_files` is applied via `torrent-rename-path` before queuing the verification check, ensuring Transmission checks the existing files on disk without duplicating or corrupting them.
3. **Recheck Scheduling & Concurrency**:
   * Imported torrents are added `PAUSED`.
   * A bounded recheck queue (concurrency limit = 2) triggers `force_recheck` (Transmission `torrent-verify`).
   * When checking completes (transition out of checking state to paused/seeding), `last_checked` is stamped in `torrent_metadata`. Torrents that were seeding in qBittorrent and verified 100% are automatically resumed; paused torrents remain paused.

---

## 3. Storage Formats & Reader Architecture

### Format A: Legacy BT_backup
* Stored in `<qbit_data_dir>/BT_backup/`.
* `<infohash>.torrent`: raw bencoded metainfo.
* `<infohash>.fastresume`: bencoded dictionary containing `save_path`, `qBt-savePath`, `mapped_files`, `qBt-category`, `qBt-tags`, `file_priority`, etc.
* Parser: Custom, robust recursive bencode reader operating on `&[u8]` that decodes byte strings with UTF-8 lossy fallbacks so corrupted strings or non-UTF-8 paths never crash scanning.

### Format B: SQLite Storage (`torrents.db`)
* Stored in `<qbit_data_dir>/torrents.db`.
* Schema is dynamically inspected via `PRAGMA table_info` and `sqlite_master` rather than hardcoding column names.
* Reads infohash, bencoded resume data BLOB, and bencoded metadata BLOB.
* Read-only connection using SQLite URI `mode=ro` (or temp copy if locks present) so active WAL locks never block reads.

### Merge & Deduplication
* If both formats exist, entries are merged by info-hash. If duplicate info-hashes appear, the entry with the higher `last_seen` / `added_time` is preferred, tracking the source (`LegacyFile` vs `SqliteRow`) for precise downstream removal.

---

## 4. Directory Locator Strategy

### Windows
1. Portable / custom profile detection (process command line `--profile`).
2. Known folder: `FOLDERID_LocalAppData\qBittorrent` (`BT_backup`, `torrents.db`).
3. Config folder: `FOLDERID_RoamingAppData\qBittorrent\qBittorrent.ini` & `categories.json`.
4. Registry install hints (`HKCU/HKLM\SOFTWARE\qBittorrent`).
5. MSIX / Store app local cache probe.

### macOS
1. Default data dir: `~/Library/Application Support/qBittorrent`.
2. Config dir: `~/Library/Preferences/qBittorrent` and `~/.config/qBittorrent`.
3. XDG fallback: `~/.local/share/qBittorrent` and `~/.local/share/data/qBittorrent`.
4. Custom `--profile` process scan.
5. Permission check: detect `EPERM` / `EACCES` and guide user to grant Full Disk Access or use the manual folder picker.

---

## 5. Safe Removal Specification (R4b & Section 5b)

Removal is destructive, so safety guarantees are strictly enforced:

1. **Preconditions**:
   * Running process check: Scan for `qbittorrent.exe` (Windows) or `qbittorrent` (macOS). If running, show blocker modal with `[I closed it - re-check]`, `[Import without removing]`, and `[Cancel]`. Never proceed with removal while running.
   * Write permission verification: Probe data directory for write access. If unwritable, disable removal option.
2. **Second Confirmation**:
   * When R4b is checked, pressing OK opens a dedicated modal:
     > "N torrents will be imported into Nebula and then REMOVED from qBittorrent. Downloaded files will NOT be deleted. A backup will be saved to <path>. Continue?"
   * Buttons: `[Import and remove]` (danger styling) vs `[Import only]` (default focus) vs `[Cancel]`.
3. **Mandatory Backup & Integrity Verification**:
   * Backup directory created: `<Nebula profile>/qbit-import-backups/<YYYYMMDD-HHMMSS>/`.
   * For legacy: Copy `<hash>.torrent` and `<hash>.fastresume`.
   * For SQLite: Consistent backup of `torrents.db` + `-wal` / `-shm` (or SQLite backup API).
   * Verification: File size checks and `PRAGMA integrity_check` on the backup DB. If verification fails, removal is ABORTED immediately while import remains intact.
   * Manifest: `manifest.json` detailing original paths, infohashes, timestamps, and manual restore instructions.
4. **Hard Safety Path Guard**:
   * Guard function strictly validates all paths before unlinking:
     * Must resolve to inside detected qBittorrent data dir (via canonical realpath, rejecting junctions/symlink traversal).
     * Must sit directly inside `BT_backup/`.
     * Filename must match `^[0-9a-fA-F]{40,64}\.(torrent|fastresume)$`.
     * Rejects any payload files or save paths unconditionally.
5. **Atomic Removal Execution**:
   * Executed only for torrents confirmed imported AND verified persisted in Nebula's database.
   * Legacy files moved to OS Trash / Recycle Bin (macOS `NSFileManager` / Trash, Windows `IFileOperation`), falling back to permanent delete only if trash is unavailable.
   * SQLite: Single transaction `DELETE FROM torrents WHERE id = ?`, commit, verify integrity. Roll back on any failure.
6. **Summary & Reporting**:
   * Full report modal: counts imported, failed, skipped, removed, not removed (with exact reasons), backup directory path with "Open Folder" button, and "Copy Log" button.

---

## 6. UI & Keyboard Accessibility
* Entry in `SettingsModal.tsx` under Downloads / Migration section.
* High-DPI and Retina scaling, dark mode aligned with Nebula theme.
* Keyboard navigation: Space toggles checkbox, Ctrl/Cmd+A selects/deselects all, Enter confirms, Esc cancels.
* Accessible ARIA labels and live selection counter ("N of M selected").
* Filter/search input to quickly find torrents by name, category, or path.

---

## 7. Exact Files to Touch and Create

### New Files
1. `src-tauri/src/qbit_import/mod.rs`: Module root and public API.
2. `src-tauri/src/qbit_import/types.rs`: Data structures for IPC and scanning.
3. `src-tauri/src/qbit_import/bencode.rs`: Resilient byte-level bencode parser.
4. `src-tauri/src/qbit_import/locator.rs`: Cross-platform qBittorrent locator.
5. `src-tauri/src/qbit_import/reader.rs`: Legacy BT_backup and SQLite `torrents.db` reader.
6. `src-tauri/src/qbit_import/mapper.rs`: Path, rename, category, layout, priority mapper.
7. `src-tauri/src/qbit_import/remover.rs`: Backup creation, process check, safety path guard, and removal logic.
8. `src-tauri/src/qbit_import/importer.rs`: Import coordinator and recheck queue manager.
9. `src-tauri/src/commands/qbit_import.rs`: Tauri command bindings.
10. `src/components/modals/QbitImportModal.tsx`: Complete UI for import workflow, selection, confirmation, removal warnings, and summary.
11. `docs/qbit-import-testing.md`: Testing matrix and manual test documentation.

### Existing Files Modified
1. `src-tauri/src/lib.rs`: Register new commands and state.
2. `src-tauri/src/commands/mod.rs`: Register `qbit_import` command module.
3. `src-tauri/src/db/schema.rs`: Add `last_checked` to `CREATE_TORRENT_METADATA_TABLE`.
4. `src-tauri/src/db/mod.rs`: Add `last_checked` column migration, update `TorrentDbMetadata` and CRUD queries.
5. `src-tauri/src/commands/torrent.rs`: Add `last_checked` field to `TorrentItem`.
6. `src-tauri/src/engine/transmission.rs`: Map `last_checked` and labels into `TorrentItem`.
7. `src/types/index.ts`: Add `last_checked` to `TorrentItem` and declare qBit import types.
8. `src/components/modals/SettingsModal.tsx`: Add "Import from qBittorrent…" entry button.
9. `src/components/manager/TorrentManagerView.tsx`: Add sortable "Last checked" table column and detail panel entry.
10. `CHANGELOG.md`: Document qBittorrent import and optional removal capability.

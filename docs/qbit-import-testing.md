# qBittorrent Migration Testing Matrix & Manual Verification Guide

This document details the test matrix, preconditions, expected behaviors, verification steps, and backup restoration instructions for the "Import from qBittorrent" feature in Nebulae.

---

## 1. Operating System & Platform Matrix

| OS / Platform | Storage Format | qB State | Removal (R4b) | Expected Result |
| :--- | :--- | :--- | :--- | :--- |
| **macOS 14+ (Apple Silicon)** | Legacy `BT_backup` (`.torrent` + `.fastresume`) | Closed | **OFF** | Torrents imported paused, recheck queued, payload preserved, qB directory untouched. |
| **macOS 14+ (Apple Silicon)** | SQLite `torrents.db` | Closed | **ON** | Torrents imported, verified backup created in `<profile>/qbit-import-backups/`, rows removed from `torrents.db`, payload preserved. |
| **macOS 13+ (Intel x86_64)** | Legacy `BT_backup` | Running | **OFF** | Stale data warning shown, import proceeds paused, recheck queued, qB files intact. |
| **macOS 13+ (Intel x86_64)** | Legacy `BT_backup` | Running | **ON** | Blocker dialog displayed ("qBittorrent is currently running. Please close it completely..."), removal blocked. |
| **Windows 11 (x64)** | Legacy `BT_backup` | Closed | **OFF** | SHGetKnownFolderPath auto-locates `%LOCALAPPDATA%\qBittorrent`, paths & renames preserved. |
| **Windows 11 (x64)** | SQLite `torrents.db` | Closed | **ON** | Second confirmation modal shown, backup verified, entries deleted atomically from SQLite. |
| **Windows 10 (x64)** | Portable / Custom (`--profile`) | Closed | **OFF** | Auto-detection via running command-line or fallback folder picker, import succeeds. |
| **Windows 10 (x64)** | Legacy `BT_backup` | Running | **ON** | Process blocker halts removal flow; user closes qBittorrent, retry proceeds. |

---

## 2. Permissions, Sandbox & macOS Full Disk Access (TCC)

On macOS 13+ (Ventura, Sonoma, Sequoia), sandboxed or hardened-runtime applications accessing `~/Library/Application Support/` or external volumes (`/Volumes/...`) may encounter system permission restrictions (`EPERM` / `EACCES`).

### Granting Full Disk Access (macOS)
1. Open **System Settings** > **Privacy & Security** > **Full Disk Access**.
2. Click the `+` button (or unlock with Touch ID / Password).
3. Select **Nebulae.app** from `/Applications` and toggle the permission switch **ON**.
4. Restart Nebulae.

### Fallback Folder Picker
If Nebulae cannot read the default directory due to missing permissions:
- Nebulae displays an informative prompt explaining the permission requirement.
- The user can click **Browse…** and select the qBittorrent directory through standard system dialogs (`NSOpenPanel`).

---

## 3. Storage Format Discovery & Verification

Nebulae supports both qBittorrent storage architectures:

### A. Legacy File-Based (`BT_backup`)
- Location: `<qBt-Data-Dir>/BT_backup/`
- Structure: Pairs of `<infohash>.torrent` (bencoded metadata) and `<infohash>.fastresume` (bencoded state dict).
- Hash Support: 40-character hex v1 SHA-1 hashes and 64-character v2 / hybrid hashes.
- Verification:
  - Check that `.fastresume` files are parsed with depth limits and lossy UTF-8 fallback.
  - Verify that file renames in `mapped_files` are recognized before hash verification.

### B. SQLite Storage (`torrents.db`)
- Location: `<qBt-Data-Dir>/torrents.db` (qBittorrent >= 4.4 with SQLite resume storage).
- Dynamic Schema Discovery: Nebulae queries `sqlite_master` and table columns dynamically (`PRAGMA table_info`), searching for:
  - Hash identifier column (e.g. `hash`, `torrent_hash`, `info_hash`, `id`)
  - Metadata BLOB column (e.g. `torrent`, `metadata`, `torrent_file`)
  - Resume data BLOB column (e.g. `fastresume`, `resume_data`, `data`)
- Read Locks & Concurrency: SQLite databases are opened with read-only URI flags (`mode=ro`) or staged from temporary snapshots with WAL/SHM files to prevent database locking conflicts.

### Merging Strategy
If both `torrents.db` and `BT_backup` exist, Nebulae deduplicates entries by info-hash, preferring the newer modification date, and tags each entry with its storage source (`legacy_file` or `sqlite`).

---

## 4. Path & Content Layout Preservation

Nebulae strictly preserves download targets and file mappings:
- **Never Moves Payloads**: The payload files on disk are never touched, moved, or deleted during import or removal.
- **Content Layouts**:
  - `Original`: Preserves the torrent structure as declared in the metainfo.
  - `Subfolder`: Always creates a containing root folder.
  - `NoSubfolder`: Strips single top-level directory nesting.
- **Renamed Files (`mapped_files`)**: Applied via engine RPC (`torrent-rename-path`) *prior* to queuing hash verification so that existing completed files on disk are verified immediately without re-downloading.
- **Cross-OS Rejection**: If Windows drive-letter paths (e.g. `D:\Downloads`) are detected on macOS, or POSIX root paths (e.g. `/home/user`) on Windows, Nebulae flags the path and offers an explicit path locator rather than failing silently.

---

## 5. Verification Queue & "Last checked" Column

1. **Paused Addition**: All imported torrents are added to the engine in **Paused** state to avoid accidental churn or re-downloading before verification.
2. **Concurrency Limit**: Nebulae runs a background verification queue with a concurrency limit of 2 simultaneous checks.
3. **Status Reporting**:
   - Before check begins: displays `Queued`.
   - While checking: displays `Checking… ({pct}%)`.
   - When verification completes: updates the persistent `last_checked` timestamp (in local timezone format).
4. **Auto-Resume**: If verification completes at 100% and the torrent was active (seeding/downloading) in qBittorrent, Nebulae automatically resumes the torrent; paused torrents remain paused.
5. **Sortable Column**: The "Last checked" header in the Manager table can be clicked to sort transfers ascending, descending, or restore default ordering.

---

## 6. Removal Safety & Rollback Verification (R4b & Section 5b)

Removal of entries from qBittorrent is strictly gated:
- **Default OFF**: Checkbox is unchecked by default and never remembered across sessions.
- **Process Check**: If `qbittorrent` (or `qbittorrent.exe`) is running, removal is aborted with a blocking modal. Nebulae never kills the process.
- **Second Confirmation**: Requires a dedicated modal confirmation before executing any deletion.
- **Hard Safety Path Guard**: The deletion engine verifies via canonical path resolution that target files are strictly located within `<qbit_data_dir>/BT_backup/` matching regex `^[0-9a-fA-F]{40,64}\.(torrent|fastresume)$`. Any attempt to delete payload files, save directories, or files outside `BT_backup` results in immediate panic and operation abort.
- **Atomic SQLite Rollback**: Deletions from `torrents.db` occur inside a single SQLite transaction (`BEGIN IMMEDIATE`). If any error occurs, the transaction rolls back cleanly.

---

## 7. Backup Structure & Disaster Recovery (Manual Restore)

Before any removal operation, Nebulae generates an uncompressed archive snapshot in:
`<profile_dir>/qbit-import-backups/<YYYYMMDD-HHMMSS>/`

### Backup Archive Contents
- `manifest.json`: Detailed backup metadata, source paths, sha256 checksums, and storage format.
- `BT_backup/`: Backed up `.torrent` and `.fastresume` file pairs (if legacy storage).
- `torrents.db`: Copy of the SQLite database along with active `-wal` and `-shm` files.

### Emergency Manual Restore Procedure
If qBittorrent data needs to be restored to its pre-removal state:

1. **Ensure qBittorrent is completely closed**.
2. Locate the backup folder in Nebulae's profile directory:
   - macOS: `~/Library/Application Support/com.asian-dvd-club.nebulae/qbit-import-backups/<TIMESTAMP>/`
   - Windows: `%APPDATA%\com.asian-dvd-club.nebulae\qbit-import-backups\<TIMESTAMP>\`
3. Inspect `manifest.json` to verify the backup timestamp and targeted files.
4. For Legacy File Storage:
   - Copy all files from `<BACKUP_DIR>/BT_backup/` back into your qBittorrent `BT_backup` directory:
     - macOS: `~/Library/Application Support/qBittorrent/BT_backup/`
     - Windows: `%LOCALAPPDATA%\qBittorrent\BT_backup\`
5. For SQLite Storage:
   - Copy `<BACKUP_DIR>/torrents.db` (and any `-wal`/`-shm` files) back into:
     - macOS: `~/Library/Application Support/qBittorrent/torrents.db`
     - Windows: `%LOCALAPPDATA%\qBittorrent\torrents.db`
6. Restart qBittorrent. All previous transfers will be restored intact.

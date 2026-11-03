# qBittorrent Import & Destructive-Operation Safety Acceptance Report

**Date:** November 2, 2026  
**Repository:** `Nebulae-torrent_client`  
**Branch:** `feature/qbit-import`  
**Base Commit:** `abc6f04` (`docs: add qBittorrent import testing matrix and changelog entry`)  
**Latest Verification Commit:** `07e0e26` (`fix(tray-speed): initialize last_active to stale on start so idle state reports None`)  
**Audit Target:** End-to-End Acceptance, Production-Engine Verification, Destructive-Operation Safety, and Native Cross-Platform Parity  
**Verification Workstations:**  
- **macOS Workstation:** Darwin `MacStudio.local` 27.2.0 (macOS 15/16 arm64 Apple Silicon)  
- **Windows Workstation:** `Supermicro.local` (Windows 10 Enterprise LTSC Build 19044.5247 x86_64, MinGW-w64 GCC 16.1.0 GNU toolchain)

---

## 1. Executive Summary & Production Readiness Verdicts

This report documents the evidence-based final acceptance, production-engine verification, and destructive-operation safety audit for the **"Import from qBittorrent"** feature (including the optional post-import removal capability) in Nebulae.

Native Rust compilation, real transmission-daemon engine execution, SQLite transactional backups, and fail-closed Recycle Bin / Trash tests have been executed on **both native macOS and native Windows platforms**.

### Production Readiness Verdicts by Platform & Mode

| Platform & Mode | Production Verdict | Verification Evidence |
| :--- | :---: | :--- |
| **macOS Import-Only** | **GO** | Fully verified on Darwin arm64. Read-only parsing of legacy `.torrent`/`.fastresume` and SQLite `torrents.db`, immutable payload mapping, and real-engine piece verification confirmed. |
| **macOS Import with Removal** | **GO** | Verified fail-closed boundary: SQLite online backup snapshotting, selective restoration, fail-closed macOS Finder Trash routing (no silent permanent unlinking), and auto-resume withholding on removal failure. |
| **Windows Import-Only** | **GO** | Fully verified on native Windows 10 LTSC (`Supermicro.local`). Common-Controls 6.0 manifest embedding, isolated `transmission-daemon.exe` execution, and genuine piece hashing verified. |
| **Windows Import with Removal** | **GO** | Verified on native Windows 10 LTSC. PowerShell `SendToRecycleBin` execution verified with verbatim UNC prefix stripping (`\\?\`), selective restoration, and running process blocker confirmed. |

---

## 2. Production-Engine End-to-End Migration & Verification (`qbit_import_real_engine.rs`)

To eliminate acceptance gaps from mock implementations, a dedicated real-engine acceptance suite was developed and executed natively on both target operating systems using:
- Genuine `QbitImporter` orchestration;
- Genuine `TransmissionTorrentEngine` client;
- Isolated, dedicated `transmission-daemon` instances (macOS native binary & Windows `transmission-daemon.exe 4.1.3 (838877323f)`);
- Ephemeral, disposable SQLite databases and directories (preventing any interaction with live user profiles or default databases);
- Synthetic multi-file torrent payloads with cryptographically valid piece hashes generated via `sha1`.

### 2.1 Test Matrix & Verified Engine Behaviors

| Test Scenario | Payload Structure | Engine Action & Verified Behavior | Result |
| :--- | :--- | :--- | :---: |
| **Complete Original Layout** | Multi-file tree (`root/file1.bin`, `root/sub/file2.bin`) | Real piece hashing completes to **100%** progress (`1.0`). `last_checked` timestamp written to SQLite. | **PASS** |
| **Renamed Hierarchy** | Root folder renamed, nested folder renamed, file renamed | Files located at existing disk locations without physical moves/renames. Real engine verifies to **100%**. | **PASS** |
| **Deliberately Corrupted Payload** | First piece modified with inverted bytes | Engine hash check detects mismatched SHA-1 pieces. Progress stops at **33.3%** (`0.333`). Auto-resume withheld; torrent kept paused. | **PASS** |
| **Missing / Offline Download Path** | Non-existent path (`/tmp/missing_...` or `C:\missing_...`) | Engine marks torrent with missing data. `last_checked` timestamp is **not** written (`None`). | **PASS** |
| **Durable Persistence Across Restart** | Database connection closed and reopened | Verified progress, effective file mappings, and `last_checked` timestamp survive full application restart. | **PASS** |
| **Payload Immutability** | Recursive SHA-1 hash inventory before & after | 100% byte-for-byte and path identity preserved across all files. Zero relocations, copies, or renames. | **PASS** |

### 2.2 Recheck Polling Logic Verification

The recheck polling logic was hardened to address transient timing where `force_recheck` (calling Transmission RPC `torrent-verify`) returns before the next status poll reflects the `"Checking"` or `"Queued for checking"` state:
- When a torrent is added paused, its initial status is `"Paused"`.
- If verification is requested, relying strictly on "status is not Checking" could cause an immediate false-positive check completion if polled before Transmission transitions to checking.
- **Hardened Verification Protocol:**
  1. The importer records the verification start timestamp.
  2. The polling loop requires either observing the active check state (`"Checking"` or `"Queued for checking"`) OR observing a monotonic advance in verified progress/state after verification initiation.
  3. A safety minimum poll window ensures the engine has received and registered the verification RPC before status evaluation completes.

---

## 3. Destructive-Operation Safety, Removal & Selective Recovery

### 3.1 Removal & Selective Recovery Round Trip

Tested on disposable qBittorrent source profiles containing **Selected Torrent A** and **Unselected Torrent B**:
1. **Pre-Import Verification:** Source files for both A and B exist in `BT_backup/` (`.torrent` and `.fastresume`).
2. **Import Execution:** Torrent A is imported with `remove_from_qbit: true`.
3. **Persistence Verification:** Torrent A is verified durably written to Nebulae SQLite database prior to any deletion operation.
4. **Transactional Backup:** SQLite online backup API / file backup creates snapshot in `qbit_backups/<timestamp>/` and generates `manifest.json`.
5. **Scoped Deletion:**
   - Only Torrent A's source metadata is moved to macOS Trash / Windows Recycle Bin.
   - Torrent B remains completely untouched in the active profile.
   - All downloaded payload files and directories for both A and B remain 100% byte-for-byte intact.
6. **Selective Restoration:**
   - `QbitRemover::restore_from_manifest_with_mode` restores only Torrent A.
   - Both Torrent A and Torrent B exist and are verified valid in the qBittorrent profile post-restoration.

### 3.2 Restore Scope: Selective vs. Whole-Profile Rollback

| Mode | Target Scope | Safety Safeguards | Verification Test |
| :--- | :--- | :--- | :--- |
| **Selective** (*Default*) | Restores only entries present in `manifest.json`. | Never overwrites existing conflicting rows. Unrelated torrents added after the backup survive intact. | `test_acceptance_sqlite_selective_restoration_preserves_unrelated_torrents` |
| **Whole-Profile Rollback** | Replaces active `torrents.db` with backup snapshot. | Requires exact confirmation phrase: `"This replaces the entire qBittorrent torrent database with the backup. Torrents added or changed since that backup may be lost from the active profile."`. Takes automated safety snapshot `torrents.db.pre_restore_<timestamp>` prior to overwrite. | `test_acceptance_whole_profile_rollback_requires_explicit_confirmation` |

### 3.3 Fail-Closed Recycle Bin / Trash Policy

Nebulae strictly enforces that **silent permanent deletion is completely prohibited**:
- **macOS:** Deletions route via AppleScript to macOS Finder Trash.
- **Windows:** Deletions route via PowerShell to Windows Recycle Bin (`Microsoft.VisualBasic.FileIO.FileSystem::DeleteFile`).
- **Fail-Closed Gate:** If Trash or Recycle Bin is unavailable or fails, deletion is aborted immediately. Files are never unlinked permanently (`fs::remove_file`).
- **Auto-Resume Withheld on Removal Failure:** If removal fails for an imported torrent, auto-resume is withheld and the torrent remains paused in Nebulae to prevent dual-client file access collisions.

---

## 4. Confirmed Defects Discovered & Resolved During Native Testing

During cross-platform compilation and native execution on Windows 10 LTSC and macOS, five distinct defects were identified and resolved:

### Defect 1: Windows GNU Manifest Embedding for Common-Controls 6.0 (Crash `0xc0000139`)
- **Symptom:** On Windows MinGW-w64 (`x86_64-pc-windows-gnu`), running any test binary failed immediately with exit code `0xc0000139` (`STATUS_ENTRYPOINT_NOT_FOUND`).
- **Root Cause:** The `rfd` (Rust File Dialog) crate imports `TaskDialogIndirect` from `comctl32.dll`. Without an application manifest requesting Common-Controls version 6.0, the Windows dynamic linker binds `comctl32.dll` to legacy version 5.82 (which lacks `TaskDialogIndirect`).
- **Resolution:** Updated `src-tauri/build.rs` to detect Windows GNU targets, generate an RT_MANIFEST resource script referencing WinSxS ComCtl32 version 6.0.0.0, compile it using `windres`, and emit `cargo:rustc-link-arg=<out_dir>/manifest.o`.

### Defect 2: Missing `last_checked` in Windows-Only `tray_speed.rs` Test Initializer
- **Symptom:** Native Windows compilation failed with `error[E0063]: missing field 'last_checked' in initializer of 'torrent::TorrentItem'`.
- **Root Cause:** `tray_speed.rs` is gated to `#[cfg(target_os = "windows")]`. When `last_checked` was added to `TorrentItem`, macOS builds did not compile `tray_speed.rs`, leaving the test struct helper uninitialized for Windows.
- **Resolution:** Added `last_checked: None` to the `item()` test helper in `src-tauri/src/tray_speed.rs`.

### Defect 3: Windows Recycle Bin Verbatim UNC Prefix Failure (`\\?\`)
- **Symptom:** Real-engine removal test on Windows failed with `summary.removed_count == 0` instead of `1`.
- **Root Cause:** `DefaultTrashProvider::delete_file` called `path.canonicalize()`, which prefixes Windows paths with the verbatim UNC designator `\\?\` (e.g. `\\?\C:\Users\...`). When passed to PowerShell's `[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile`, .NET threw `System.ArgumentException: Illegal characters in path`. Because Nebulae prohibits silent permanent unlinking, the removal safely aborted.
- **Resolution:** Updated `remover.rs` on Windows to strip `\\?\` (and normalize `\\?\UNC\` to standard network paths `\\`) before passing to PowerShell, and routed execution through `run_hidden_powershell`.

### Defect 4: Windows Tray Speed Initializer Staleness Bug
- **Symptom:** `tray_speed::tests::idle_torrents_report_none_initially` failed on Windows with `assertion failed: rates.is_none()`.
- **Root Cause:** `TraySpeedState::default()` initialized `last_active = Instant::now()`. At startup with idle torrents, `now - last_active` was 0 seconds (less than `STALE_AFTER_SECS`), causing the state machine to treat the client as recently active and return `Some((0, 0))` rather than `None`.
- **Resolution:** Initialized `last_active` to `now - Duration::from_secs(STALE_AFTER_SECS + 1)` in `TraySpeedState::default()`, ensuring idle torrents report `None` on initial poll.

### Defect 5: Service Injection Decoupling in `QbitImporter`
- **Symptom:** When automated real-engine tests were executed on a developer workstation where the real `qbittorrent.exe` was running in the background, `SystemProcessChecker` detected the running background process and safely aborted removal against the synthetic test profile.
- **Root Cause:** `QbitImporter::execute_import` hardcoded `SystemProcessChecker` and `DefaultTrashProvider`.
- **Resolution:** Added `execute_import_with_services` to allow test suites to supply an isolated profile process checker while testing the real `DefaultTrashProvider` (Recycle Bin / Trash), while retaining `execute_import` with system defaults for production commands.

---

## 5. Comprehensive Settings Fidelity Table

| qBittorrent Field | Parsed Value | Nebulae Persisted Field | Engine Operation | Test Verification | Status |
| :--- | :--- | :--- | :--- | :--- | :---: |
| `save_path` / `download_path` | `torrent.save_path` | `torrent_metadata.save_path` | Sets download path via RPC `torrent-set` | `test_real_engine_end_to_end_migration_and_verification` | **PASS** |
| `qBt-savePath` | `torrent.save_path` | `torrent_metadata.save_path` | Fallback incomplete payload path | `test_acceptance_incomplete_payloads_and_qb_extension` | **PASS** |
| `mapped_files` | `mapped_files: Vec<String>` | In-memory engine file map | Differential folder/file RPC renaming | `test_real_engine_end_to_end_migration_and_verification` | **PASS** |
| `priority` | `file_priorities: Vec<u8>` | Per-file priorities | `set_file_priority(file_idx, prio)` | `test_acceptance_skipped_files_and_per_file_priorities` | **PASS** |
| `trackers` | `trackers: Vec<String>` | Torrent metainfo tiers | Injected during metainfo registration | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `category` | `category: Option<String>` | `torrent_metadata.category` | `db.update_category(hash, cat)` | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `tags` | `tags: Vec<String>` | `torrent_metadata.labels` | Formatted as comma-separated labels | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `dl_limit` / `up_limit` | `download/upload_limit_kib` | `torrent_metadata.dl/ul_limit` | Sets bandwidth limits via RPC | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `max_ratio` | `ratio_limit: Option<f64>` | `torrent_metadata.ratio_limit` | Sets ratio limit (-1.0 = disabled) | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `sequential_download` | `sequential_download: bool` | `torrent_metadata.sequential_download`| Sets sequential flag via RPC | `test_real_engine_end_to_end_migration_and_verification` | **PASS** |
| `first_last_piece_prio`| `first_last_piece_prio: bool`| `torrent_metadata.first_last_piece_prio`| Sets piece priority flags | `test_real_engine_end_to_end_migration_and_verification` | **PASS** |
| `paused` / `state` | `paused: bool` | Engine active/paused state | Added paused; verified before auto-resume | `test_real_engine_removal_and_selective_recovery_round_trip`| **PASS** |
| `added_time` | `added_time: Option<i64>` | `torrent_metadata.added_at` | Stamped in metadata SQLite table | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `completed_time` | `completed_time: Option<i64>`| `torrent_metadata.completed_on` | Stamped in metadata SQLite table | `test_acceptance_end_to_end_import_with_unsupported_settings` | **PASS** |
| `qBt-seedingTimeLimit` | `seeding_time_limit: Option<i64>`| None (unsupported in engine) | Recorded in `report.unsupported_settings` | `test_acceptance_end_to_end_import_with_unsupported_settings` | **REPORTED** |
| `auto_managed` | `auto_managed: Option<bool>`| None (unsupported per-torrent)| Recorded in `report.unsupported_settings` | `test_acceptance_end_to_end_import_with_unsupported_settings` | **REPORTED** |

---

## 6. Full Test Suite Results Across Platforms

### macOS (Darwin arm64 — MacStudio.local)
- **Real-Engine Acceptance Suite (`tests/qbit_import_real_engine.rs`):**
  - `test_real_engine_end_to_end_migration_and_verification`: **PASSED**
  - `test_real_engine_lifecycle_smoke`: **PASSED**
  - `test_real_engine_removal_and_selective_recovery_round_trip`: **PASSED**
  - *Result: 3 passed, 0 failed (19.49s)*
- **Acceptance & Safety Suite (`tests/qbit_import_acceptance.rs`):**
  - *Result: 21 passed, 0 failed (0.24s)*
- **Library Unit Tests (`src/lib.rs`):**
  - *Result: 327 passed, 0 failed, 1 ignored (0.64s)*

### Windows (Windows 10 Enterprise LTSC 19044 — Supermicro.local)
- **Real-Engine Acceptance Suite (`tests/qbit_import_real_engine.rs`):**
  - `test_real_engine_end_to_end_migration_and_verification`: **PASSED**
  - `test_real_engine_lifecycle_smoke`: **PASSED**
  - `test_real_engine_removal_and_selective_recovery_round_trip`: **PASSED**
  - *Result: 3 passed, 0 failed (41.01s)*
- **Acceptance & Safety Suite (`tests/qbit_import_acceptance.rs`):**
  - *Result: 21 passed, 0 failed (0.72s)*
- **Library Unit Tests (`src/lib.rs`):**
  - *Result: 328 passed, 0 failed, 1 ignored (120.72s)* (including Windows-specific `tray_speed` test suite)

---

## 7. Final Acceptance Conclusion

All acceptance criteria across Phase 0 discovery, engine integration, destructive-operation safeguards, and native cross-platform execution have been satisfied.

1. **Payload Integrity:** Downloaded payload files are never physically moved, renamed, or modified during import.
2. **Verification Authenticity:** Piece verification executes through the genuine torrent engine; corrupt pieces are detected; and completion timestamps are stamped only upon successful check completion.
3. **Safety Boundaries:** Deletions require pre-existing verified backups, reject symlink traversals, enforce strict filename patterns, never touch payload files, route exclusively to the system Recycle Bin / Trash with fail-closed semantics, and withhold auto-resume if removal fails.
4. **Cross-Platform Parity:** Verified via native execution on both macOS and Windows.

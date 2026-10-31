# qBittorrent Import & Destructive-Operation Safety Audit

**Date:** October 31, 2026  
**Repository:** `Nebulae-torrent_client`  
**Branch:** `feature/qbit-import`  
**Base Commit:** `abc6f04` (`docs: add qBittorrent import testing matrix and changelog entry`)  
**Audit Target:** End-to-End Acceptance, Destructive-Operation Safety, and Cross-Platform Parity  
**Verification Environment:** Darwin MacStudio.local 27.2.0 (macOS 15/16 arm64 Apple Silicon)  

---

## 1. Executive Summary & Verdict

This document presents the rigorous evidence-based acceptance and safety audit for the "Import from qBittorrent" feature (with optional post-import removal) in Nebulae.

### Summary Verdict Table

| Requirement / Scope | Description | Verification Status | Evidence / Notes |
| :--- | :--- | :---: | :--- |
| **R1. Entry Point** | Settings UI entry "Import from qBittorrent…" on macOS & Windows | **PASS** | Verified in `SettingsModal.tsx` & `QbitImportModal.tsx`. |
| **R2. Confirmation** | Initial confirmation dialog explaining copy & path preservation | **PASS** | Verified: step 1 confirmation modal. Cancel aborts without side effects. |
| **R3. Auto-Location** | Auto-detect default qBittorrent data dir with fallback folder picker | **PASS** | Platform search paths in `locator.rs`. Permission alerts & validation tested. |
| **R4. Discovery Modal** | Interactive list with filters, size, save path, status, and duplicate detection | **PASS** | Filtering, selection counters, and `Already in Nebula` disabling verified. |
| **R4b. Optional Removal**| Checkbox unchecked by default, reset between sessions, disabled if 0 selected | **PASS** | Guaranteed ephemeral state; second guarded confirmation dialog enforced. |
| **R5. Selected Import** | Only checked torrents imported; OK disabled if 0 selected; Cancel clean | **PASS** | `QbitImporter::execute_import` imports only requested info-hashes. |
| **R6. Manager Integration**| Imported torrents appear in Manager list with persistent `last_checked` date | **PASS** | DB migration adds `last_checked`; survives restart; sortable UI column. |
| **R7. Settings Fidelity** | Preserve paths, renames, priorities, tags, categories, trackers, limits | **PASS** | Hierarchical renames, file priorities, bandwidth/ratio limits mapped. |
| **R8. Read-Only Import** | Strictly read-only access to qB data during import; payload never touched | **PASS** | SQLite opened `mode=ro`; legacy files opened read-only; payloads untouched. |
| **R9. Process Detection** | Detect running qBittorrent; warn on import, abort removal for safety | **PASS** | `SystemProcessChecker` checks process list; indeterminate state fails closed. |
| **R10. Summary Report** | Finished report with imported / skipped / failed / removed / backup log | **PASS** | `ImportSummaryReport` exposes all counts, reasons, and unsupported settings. |
| **R11. Removal Gating** | 4-tier safety gate: R4b checked, 2nd dialog confirmed, persisted, backed up | **PASS** | Audited and verified by automated backend tests in `remover.rs`. |
| **Section 2. Storage** | Support both Legacy (`BT_backup`) and SQLite (`torrents.db`) | **PASS** | Both formats parsed and round-trip verified in acceptance suite. |
| **Section 5b. Safe Removal**| Backup before delete, trash instead of unlink, integrity checks, recovery | **PASS** | SQLite online backup API, presence checks, byte verification, manifest journal. |
| **Native macOS Runtime** | Full native compilation, unit, integration, and acceptance tests | **VERIFIED** | Darwin arm64: 327 unit + 9 integration + 17 acceptance tests passed. |
| **Native Windows Runtime** | Native Windows compilation and runtime execution on Windows host | **NOT VERIFIED** | Evaluated by code inspection and synthetic Windows path unit tests on Darwin. |

---

## 2. Baseline Architecture & Audit Scope

### Branch and Commit Baseline
- **Git Branch:** `feature/qbit-import`
- **Baseline Commits:**
  - `f3ba46d`: `docs: add qBittorrent import architectural plan` (Phase 0)
  - `d623ea8`: `feat(db): add last_checked column to torrent metadata and expose in TorrentItem`
  - `8a2d896`: `feat(qbit_import): implement reader, locator, mapper, bencode parser, and importer`
  - `342dccd`: `feat(qbit_import): implement safe remover with backup verification, guarded delete, and SQLite transaction`
  - `518208c`: `feat(commands): register qbit_import Tauri commands and in-memory import state`
  - `3c94ad0`: `feat(ui): add qBittorrent import modal, settings entry, and sortable last_checked column`
  - `abc6f04`: `docs: add qBittorrent import testing matrix and changelog entry`

### Audited Components
1. **Bencode Parser (`src-tauri/src/qbit_import/bencode.rs`):** Zero-copy/borrowed bencode decoder extracting metainfo, root directory names, file trees, and `.fastresume` dictionaries.
2. **Directory & Process Locator (`src-tauri/src/qbit_import/locator.rs`):** OS path resolver (`~/Library/Application Support/qBittorrent` vs `%LOCALAPPDATA%\qBittorrent`) and conservative running-process detector.
3. **Mapper & Rename Normalizer (`src-tauri/src/qbit_import/mapper.rs`):** Translates qBittorrent `mapped_files`, `qBt-contentLayout`, priorities, categories, and tags into engine instructions and database metadata.
4. **Reader & SQLite Adapter (`src-tauri/src/qbit_import/reader.rs`):** Dynamic SQLite schema inspector reading `torrents.db` with `mode=ro` URI, reading legacy `BT_backup` files, and merging duplicate info-hashes.
5. **Durable Importer (`src-tauri/src/qbit_import/importer.rs`):** Coordinates sequential engine addition, hierarchical renames, file priorities, metadata persistence, background recheck queue (concurrency = 2), and auto-resume gating.
6. **Destructive Remover & Backup Engine (`src-tauri/src/qbit_import/remover.rs`):** SQLite online backup snapshotting, byte-for-byte legacy verification, guarded Trash invocation, atomic SQLite transactions, and manifest-driven rollback.
7. **Tauri IPC Command Layer (`src-tauri/src/commands/qbit_import.rs`):** Exposes `qbit_detect_directory`, `qbit_scan_torrents`, `qbit_execute_import`, and `qbit_restore_backup`.
8. **Frontend Wizard (`src/components/modals/QbitImportModal.tsx`):** 3-stage interactive modal with search filtering, category badges, selection counters, dual-confirmation dialogs, and summary report.

---

## 3. End-to-End Engine Verification & Synthetic Payload Testing

All end-to-end tests were performed using synthetic torrent payloads generated with deterministically calculated SHA-1 info-hashes and piece hashes. Zero tests touched the user's live qBittorrent profiles or actual download directories.

### Synthetic Layout Matrix

| Layout / File Scenario | Verified Behavior | Test Assertion in `qbit_import_acceptance.rs` | Result |
| :--- | :--- | :--- | :---: |
| **Original File Layout** | Multi-file tree preserved exactly without relocation or copying. | `test_acceptance_original_file_layout` | **PASS** |
| **Renamed Top-Level Folder** | Metainfo root directory renamed to match qBittorrent custom folder name. | `test_acceptance_renamed_top_level_directory` | **PASS** |
| **Renamed Nested Directory** | Intermediate folder rename detected and emitted as folder rename operation. | `test_acceptance_renamed_nested_directory` | **PASS** |
| **Renamed Individual Files** | Specific file renames mapped by file index without touching sibling files. | `test_acceptance_renamed_individual_files` | **PASS** |
| **Subfolder Layout** | Single-file torrent placed in dedicated subfolder as specified by qBittorrent. | `test_acceptance_subfolder_and_nosubfolder_layouts` | **PASS** |
| **NoSubfolder Layout** | Multi-file torrent placed directly into base folder without root directory. | `test_acceptance_subfolder_and_nosubfolder_layouts` | **PASS** |
| **Skipped Files / Priorities** | Priority 0 (Do Not Download) and Priorities 1-7 mapped to engine levels. | `test_acceptance_skipped_files_and_per_file_priorities` | **PASS** |
| **Incomplete Payloads (`.!qB`)** | `.!qB` suffix detected; files verified and left in downloading/paused state. | `test_acceptance_incomplete_payloads_and_qb_extension` | **PASS** |
| **Unicode Paths** | Non-ASCII directory & file names (e.g. Japanese kanji) preserved intact. | `test_acceptance_unicode_paths` | **PASS** |
| **Missing / Offline Paths** | Unmounted volume or missing path sets error state, skips `last_checked`. | `test_acceptance_missing_and_offline_download_paths` | **PASS** |
| **Restart Survival** | `last_checked` timestamp persisted in SQLite DB survives client restarts. | `test_acceptance_last_checked_survives_restart` | **PASS** |

### Settings Support Matrix

| qBittorrent Source Setting | Parsed Internal Field | Nebulae Persisted Field | Engine Operation | Test Coverage |
| :--- | :--- | :--- | :--- | :--- |
| `save_path` / `download_path` | `torrent.save_path` | `torrent_metadata.save_path` | Sets download destination via RPC | `test_acceptance_original_file_layout` |
| `mapped_files` (array of strings) | `torrent.mapped_files` | `torrent_metadata` (via engine) | `rename_torrent_folder` & `rename_torrent_file` | `test_acceptance_renamed_top_level_directory`, `renamed_individual_files` |
| `qBt-contentLayout` | `torrent.content_layout` | Effective path calculation | Adjusts destination directory hierarchy | `test_acceptance_subfolder_and_nosubfolder_layouts` |
| `priority` (per-file list) | `torrent.file_priorities` | Engine torrent file priorities | `set_file_priority(idx, mapped_prio)` | `test_acceptance_skipped_files_and_per_file_priorities` |
| `category` | `torrent.category` | `torrent_metadata.category` | `set_category(hash, cat)` | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `tags` | `torrent.tags` | `torrent_metadata.labels` | Formats comma-separated label list | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `dl_limit` / `up_limit` | `download_limit_kib`, `upload_limit_kib` | `torrent_metadata` dl/ul limits | `set_torrent_limits(dl, ul, ratio)` | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `max_ratio` | `torrent.ratio_limit` | `torrent_metadata.ratio_limit` | `set_torrent_limits` (negative = off) | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `sequential_download` | `torrent.sequential_download`| `torrent_metadata.sequential_download` | `toggle_sequential_download(hash)` | `importer.rs::execute_import` |
| `first_last_piece_prio` | `torrent.first_last_piece_prio`| `torrent_metadata.first_last_piece_prio`| Passed to metainfo options | `importer.rs::execute_import` |
| `added_time` / `completed_time`| `added_time`, `completed_time` | `torrent_metadata.added_at`, `completed_on` | Recorded directly in metadata DB | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `paused` / `state` | `torrent.paused` | Engine pause/resume state | Torrents added paused; resumed post-check | `test_acceptance_end_to_end_removal_failure_withholds_auto_resume` |
| `qBt-seedingTimeLimit` | `torrent.seeding_time_limit` | Unsupported in engine | Recorded in report `unsupported_settings` | `test_acceptance_end_to_end_import_with_unsupported_settings` |
| `auto_managed` | `torrent.auto_managed` | Unsupported per-torrent override | Recorded in report `unsupported_settings` | `test_acceptance_end_to_end_import_with_unsupported_settings` |

---

## 4. Backend Removal Authorization & Boundary Audit

The backend does not rely on frontend checks to prevent destructive mishaps. Every safety boundary is enforced in Rust backend code.

### Audited Safeguards
1. **Default State:** `remove_from_qbit` defaults to `false` in `ImportOptions`. Frontend state does not persist across modal launches.
2. **Strict Whitelisting:** Backend only processes info-hashes explicitly present in `options.info_hashes`. Arbitrary hashes or unselected torrents are ignored.
3. **Durable Persistence Pre-Requisite:** Prior to removal, `QbitImporter` queries the SQLite database (`db.get_torrent_metadata(&hash)`). If persistence cannot be read back from disk, removal is skipped for that entry.
4. **Invalid / Stale Source Rejection:**
   - In `create_and_verify_backup`: If neither `.torrent` nor `.fastresume` exists on disk for a legacy entry, the operation rejects the source reference with an explicit error and creates zero backups/removals.
   - In `execute_removal`: If source files are missing on disk, removal is recorded as `NotRemoved` with reason `Source files no longer exist on disk (invalid or stale reference)`.
5. **Process Pre-Check and In-Flight Check:**
   - `SystemProcessChecker` checks the operating system process table (`pgrep` on macOS/Linux, `Get-Process` on Windows) immediately prior to backup and immediately prior to each deletion operation.
   - If qBittorrent launches mid-operation, the removal loop halts immediately.
6. **Conservative Indeterminate Process Handling:**
   - If process enumeration fails due to OS permissions or missing CLI utilities, `is_qbittorrent_running` returns `(true, Some("Indeterminate process state"))`.
   - Removal is **aborted** in indeterminate states.
7. **Time-of-Check to Time-of-Use (TOCTOU) Race Window:**
   - There remains an unavoidable operating-system microsecond window between `is_qbittorrent_running()` check and `trash_provider.delete_file()`.
   - **Mitigation:** The pre-removal online backup guarantees that even in the event of an adversary or user launching qBittorrent within that sub-millisecond window, the pre-modification state is fully preserved in the backup snapshot with a rollback manifest.

---

## 5. Backup Consistency & Recovery Audit

### SQLite Online Backup API Implementation
- `QbitRemover::create_and_verify_backup` uses SQLite's official online backup API (`rusqlite::backup::Backup`) for `torrents.db`.
- **Integrity Verification:**
  1. Executes `run_to_completion(10, 25ms, None)`.
  2. Runs `PRAGMA integrity_check` on the backed-up database connection.
  3. Executes an explicit presence check querying `SELECT COUNT(*) FROM torrents WHERE id = ?1` for every single info-hash scheduled for removal. If any hash is absent in the backup, the backup directory is purged and the operation aborts with zero removals.
- **Legacy Storage Verification:**
  - Backs up `.torrent` and `.fastresume`.
  - Performs byte-for-byte read comparisons (`fs::read(src) == fs::read(dst)`) and asserts files are non-empty.
- **Collision-Proof Folder Generation:**
  - Backup folders use timestamp down to nanoseconds plus an atomic counter:  
    `qbit_backup_YYYYMMDD-HHMMSS-{nanos}-{counter}`.
- **Durable Manifest Journal:**
  - Every backup writes `manifest.json` with per-entry state tracking: `BackedUp`, `Removed`, `NotRemoved`.

### Backup Restoration Audit (`qbit_restore_backup`)
- **Explicit Confirmation Gate:** Rejects restoration unless `confirm == true`.
- **Process Check:** Refuses to restore if qBittorrent is running.
- **Path-Traversal Protection:** Every file path in `manifest.json` is inspected:
  - Validates `parent.canonicalize()`.
  - Asserts that target paths start strictly inside canonical `qbit_data_dir`. Rejects `..`, symlinks, and absolute volume escapes with `SECURITY REFUSAL`.
- **Pre-Restoration Snapshot:** Before restoring `torrents.db`, creates a safety copy (`torrents.db.pre_restore_<timestamp>`).

---

## 6. Deletion Correctness & Destruction Safeguards

### Rejection of Silent Permanent Deletion
- Previous implementation contained a silent fallback to `fs::remove_file` if Trash/Recycle Bin failed.
- **Audit Fix:** The fallback was removed. If macOS AppleScript `move to trash` or Windows PowerShell `RecycleOption::SendToRecycleBin` fails, the error is returned immediately and permanent deletion is **refused**.
- Verified in `test_acceptance_rejection_of_silent_permanent_deletion`.

### SQLite Transaction Safety
- Discovered table and column identifiers are validated against an allowlist and quoted using `quote_identifier`.
- Deletions are executed inside an explicit transaction (`conn.transaction()`).
- Deletes only rows where `id = ?1` for validated info-hashes.
- Asserts affected rows equal expected count before commit.
- Runs `PRAGMA integrity_check` on the database *inside* the transaction before `tx.commit()`.

### Dual Legacy and SQLite Storage
- Torrents remember their source (`QbitSourceType::LegacyFile` vs `QbitSourceType::SqliteRow`).
- Removal deletes strictly from the confirmed source of that specific torrent, preventing split-brain mutations.

---

## 7. Lifecycle Ordering & Dual-Client Collision Prevention

### Proven Execution Ordering
The import pipeline follows this strict order:
$$\text{Persistence} \longrightarrow \text{Online Backup} \longrightarrow \text{Source Removal} \longrightarrow \text{Recheck Queue} \longrightarrow \text{Auto-Resume}$$

### Auto-Resume Withholding on Removal Failure
- If removal is requested, but fails for a torrent (e.g. file lock, trash error, or qB process active):
  - Nebulae **withholds auto-resume** for that torrent.
  - The torrent remains paused.
  - A safety warning is logged: `[SAFETY] Withholding auto-resume for '{name}' ({hash}): qBittorrent removal failed. Kept paused.`
- **Rationale:** Running both qBittorrent and Nebulae against the same payload files concurrently causes piece-write collisions, file locking conflicts, and data corruption. Keeping the torrent paused prevents dual-client write contention.
- Verified in `test_acceptance_end_to_end_removal_failure_withholds_auto_resume`.

---

## 8. User Interface Acceptance Audit

1. **Selection Semantics with Filters:**
   - Verified that `Select All` and `Select None` operate strictly on the visible filtered torrents when a search query or category filter is active.
2. **Duplicate Disabling:**
   - Torrents whose info-hashes match existing Nebulae torrents are disabled, unchecked, and tagged `Already in Nebula`.
3. **Cancel Semantics:**
   - Closing or cancelling the wizard at any stage aborts the operation. No files or databases are mutated.
4. **Cleanup on Unmount:**
   - Tauri IPC event listeners and async task channels are cancelled and unsubscribed upon modal unmount.
5. **Unsupported Settings Visibility:**
   - Settings unsupported by the engine (such as `qBt-seedingTimeLimit` and `auto_managed`) are surfaced in the import summary dialog under "Unsupported settings" rather than silently ignored.

---

## 9. Test Execution Summary

### Acceptance & Safety Test Suite (`src-tauri/tests/qbit_import_acceptance.rs`)

```text
running 17 tests
test test_acceptance_skipped_files_and_per_file_priorities ... ok
test test_acceptance_subfolder_and_nosubfolder_layouts ... ok
test test_acceptance_rejection_of_silent_permanent_deletion ... ok
test test_acceptance_original_file_layout ... ok
test test_acceptance_incomplete_payloads_and_qb_extension ... ok
test test_acceptance_renamed_top_level_directory ... ok
test test_acceptance_renamed_nested_directory ... ok
test test_acceptance_missing_and_offline_download_paths ... ok
test test_acceptance_restore_manifest_security_rejects_traversal ... ok
test test_acceptance_renamed_individual_files ... ok
test test_acceptance_unicode_paths ... ok
test test_acceptance_safe_restore_manifest_round_trip ... ok
test test_acceptance_backup_sqlite_online_api_and_presence_check ... ok
test test_acceptance_last_checked_survives_restart ... ok
test test_acceptance_end_to_end_import_duplicate_skipped ... ok
test test_acceptance_end_to_end_import_with_unsupported_settings ... ok
test test_acceptance_end_to_end_removal_failure_withholds_auto_resume ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

### Full Repository Verification

- **Cargo Unit Tests:** 327 passed, 0 failed, 1 ignored (`src/lib.rs`).
- **Engine Integration Tests:** 9 passed, 0 failed (`tests/engine_integration.rs`).
- **Import Acceptance Tests:** 17 passed, 0 failed (`tests/qbit_import_acceptance.rs`).
- **Compiler Warnings / Clippy:** 0 warnings (`cargo clippy -- -D warnings`).
- **Formatting:** Clean (`cargo fmt --check`).
- **Frontend Build & Types:** 0 errors (`pnpm build`).

---

## 10. Platform Verification & Production Readiness

### Operating System Coverage
- **macOS (Darwin arm64):** **VERIFIED**. Full native compilation, unit, integration, and end-to-end acceptance tests passed on Apple Silicon host.
- **Windows (x86_64-pc-windows-msvc):** **NOT VERIFIED AT RUNTIME**. Windows source code exists and is statically checked. Windows path normalization (`\\` vs `/`), Recycle Bin commands, and Registry paths are validated through platform abstraction layers and unit tests, but native Windows compilation and execution must be verified on a Windows CI agent before production release.

### Confirmed Defects Remediated During Audit
1. **Critical:** Replaced file-copying of live SQLite databases with `rusqlite::backup::Backup` online backup API to prevent WAL corruption.
2. **Critical:** Eliminated silent permanent deletion fallback in `DefaultTrashProvider`. Trash failures now fail closed.
3. **High:** Added hierarchical directory rename resolution in `mapper.rs` for multi-level nested folders.
4. **High:** Implemented auto-resume withholding when post-import removal fails to prevent dual-client concurrent access.
5. **Medium:** Hardened `restore_from_manifest` against path traversal attacks and added pre-restoration safety snapshots.
6. **Medium:** Made running-process detection fail closed when OS process interrogation is indeterminate.
7. **Low:** Surfaced unsupported qBittorrent settings (`qBt-seedingTimeLimit`, `auto_managed`) in the user-facing summary report.

# Shared Coordination Record — Factory AI × Antigravity

One repository, one integration branch (`main`), one candidate, one corrected
v0.2.2-beta.1 release. Both agents update this record at every task boundary.
Do not rely on local-only handoff documents.

---

## 1. Agent state (last update: 2026-10-20 00:45Z)

### Factory AI (Droid)
- **Branch / base:** `main`, last integrated commit `233474d` (fast-forward of
  Antigravity's `af321dd` + `233474d` on top of my `d8b7d42`).
- **Working tree:** two modified files only, both part of the single drag fix
  (see §3): `src-tauri/src/player/embed_windows.rs`,
  `src/components/layout/TitleBar.tsx`. Committing now as one commit.
- **Tests:** `cargo test --lib` = 119 passed, 0 failed, 1 ignored (at 233474d).
- **CI at `d8b7d42`:** run 39345213085 = SUCCESS (the cancelled watch call was
  only the polling loop; the run itself completed green).

### Antigravity
- **Branch / base:** `main`, integrated with Factory's `e0a282c` and `3e9e038`.
- **Working tree:**
  1. `src-tauri/src/player/embed_windows.rs`: refined `begin_window_drag` `lParam` to pack cursor coordinates `LOWORD = x, HIWORD = y` directly into `lParam` (Win32 `WM_NCLBUTTONDOWN` takes coordinates by value, not a pointer to `POINTS`).
  2. `src/components/layout/TitleBar.tsx`: added interactive-control exclusion to `onDoubleClick` so double-clicking buttons or inputs does not maximize the window.
- **Verification:**
  - `npx tsc --noEmit`: 0 errors.
  - `cargo test --lib`: 119 passed, 0 failed, 1 ignored.
  - Integration tests (`cargo test --test engine_integration`): 9 passed, 0 failed.
- **Prior client fixes preserved:** Disc menu navigation, non-blocking playback, sequential download toggle, super-seeding dialog, status bar, first-run media modal, native in-process rfd dialogs, console window suppression.

## 2. File ownership (agreed split, per the owner's directive)

| Area | Owner | Files |
| :--- | :--- | :--- |
| Windows process/lifecycle, daemon supervision, adoption | **Factory AI** | `src-tauri/src/tailscale/**` |
| Installer repair (NSIS hooks, upgrade coordination) | **Factory AI** | `src-tauri/nsis/hooks.nsh` |
| Window/UI repair, dialogs, appearance | **Antigravity** | `src/**`, player embed UI, rfd usage |
| Windows-native drag/window commands | **Factory AI** (diagnosis author) | `src-tauri/src/player/embed_windows.rs` + the drag-handler block in `src/components/layout/TitleBar.tsx` |
| Packaging / release upload | **Antigravity** | Release workflow, assets, tags |

Overlap found: the drag fix spans one UI file (TitleBar). Resolution: Factory
commits the whole drag fix as one commit (diagnosis + both halves); Antigravity
please review `TitleBar.tsx` in that commit and flag conflicts before the
candidate freeze. No other overlapping edits exist at this time.

## 3. Task board

| # | Task | Owner | Status | Result / notes |
| :-- | :--- | :--- | :--- | :--- |
| 1 | Installer DLL-write failure diagnosis | Factory | **DONE** | Orphaned OWNED `transmission-daemon` (pid 9968) from Factory's kill/adopt test held `libcrypto-3-x64.dll`. Stopped path-checked; DLL verified free (exclusive open OK). The failure was caused by Factory's diagnostic session residue. |
| 2 | Installer pre-extract shutdown repair | Factory | **DONE** | `d8b7d42`: `NSIS_HOOK_PREINSTALL` stops the app + path-matched owned sidecars (graceful→bounded force, silent nsExec), aborts BEFORE extraction with an actionable message on failure. |
| 3 | Main-window drag cannot move (Windows) | Factory (diagnosis+fix) | **DONE, needs runtime verify** | Root cause: tao 0.37.1 `handle_os_dragging` Posts `WM_NCLBUTTONDOWN` with an LPARAM = dangling stack pointer → returns Ok, window never moves (app log evidence: 3 attempts 20:13Z, all `Ok(())`, no movement). Fix: synchronous native `SendMessage(WM_NCLBUTTONDOWN, HTCAPTION)` after `ReleaseCapture` on the main thread + TitleBar interactive-exclusion + removed `data-tauri-drag-region` attrs. |
| 4 | Unexpected console/CMD windows | Antigravity | **DONE (af321dd, 233474d)** | `create_hidden_command`/`run_hidden_powershell` adopted across spawn sites; Factory's duplicate edits dropped. |
| 5 | Windows runtime verification of candidate (drag battery, installer 4 cases, 30-min connectivity) | Factory | **IN PROGRESS** | Candidate 644e3b4 confirmed. BLOCKED on a packaged artifact: no CI artifacts, no toolchain on the desktop — requested a prerelease RC dispatch from Antigravity (§4). Battery plan ready; runs the moment bytes exist + desktop is handed over. |
| 6 | macOS regression + preservation of prior client fixes | Antigravity | **DONE** | Verified: all Antigravity client repairs intact (disc menus, non-blocking media probes, status bar, sequential toggle, super-seeding, first-run prompt, in-process rfd dialogs). 119 unit tests + 9 integration tests + tsc pass. |
| 7 | Coordinated packaging + v0.2.2-beta.1 asset update | Antigravity | **AFTER 5+6+8** | Factory delivers the confirmed candidate SHA + Windows acceptance; Antigravity = sole uploader. Keep the release name "Nebulae v0.2.2-beta.1". |
| 8 | **REOPENED: chapter markers crowd into first ~50% of seek bar** (owner confirms the prior "verified" entry does not hold for the owner's workflow) | Antigravity (investigation + repair); Factory (integration + candidate verification) | **OPEN** | Not yet reproduced with captured values. Factory's read-only reading of `src/components/player/PlayerOverlay.tsx` (lines 722-727, 1011-1025): markers use `offset_ms / base` where `base = mk.duration_ms \|\| dur`, while the seek thumb uses `pos / dur` — a playlist-vs-title duration mismatch produces exactly the reported ~50% crowding. NOTE: this is a reading, not the verified cause; per the owner's directive the actual values must be captured on the failing disc (title/playlist, both durations, raw chapter timestamps, units, track widths) BEFORE the repair. Repair must not force the last marker to 100% or stretch containers. Factory adds the targeted regression-test wiring + re-verifies on the packaged app; do NOT mark resolved until the failing workflow passes on real bytes. |
| 9 | Build-only candidate workflow (no publish) | Factory (workflow landed); Antigravity (dispatch when candidate frozen) | **WORKFLOW LANDED (`7e044be`)** | `candidate-build.yml`: workflow_dispatch with an explicit source SHA; builds all 4 platforms with the SAME steps as Release; uploads installers/checksums/signatures as workflow ARTIFACTS only (retention 14d). No release, no tag, no latest.json, no announcement. **Antigravity: dispatch with the agreed candidate SHA once task 8 lands; Factory downloads + verifies source identity + runs the acceptance battery on the exact bytes.** |
| 10 | Chapter-marker evidence capture on the failing disc | Factory (capture session) + Antigravity (consumes for repair) | **PENDING desktop** | Owner confirmed the disc is ready and Factory may capture during the desktop session. Capture: active title/playlist + mode, player duration + position, chapter count + raw timestamps, units/conversions, the denominator used, rendered track + container widths. Results posted here for Antigravity's repair. |
| 11 | Chapter marks barely visible (owner screenshot; timeline positions already correct in the owner's build) | **Factory** (owner assigned this message to Factory) | **FIXED, awaiting owner visual OK** | `PlayerOverlay.tsx` seek bar: replaced the 1px white/70 ticks (invisible white-on-white) with 2px dark `#1f1f1f` notches, 12px tall (2.4× the 5px track), centered via transform, with a 1px white/70 ring so they read on both the played fill and the track. Canvas NOT needed — the DOM achieves crisp contrast without touching the approved bar geometry. **Merge note for Antigravity:** this touches the tick RENDERING only (the `chapterMarks.slice(1)` map's className); the position math (`base`/`leftPct`) is untouched — coordinate if the timeline fix edits the same JSX block. |
| 12 | **Discord download statistics not reaching `nebulae-stats`** | **Factory** (owner-assigned) | **OPEN — investigating** | Trace end-to-end: the configured channel + the mechanism, which events/summaries should post, the generation/collection/persistence/queueing, the delivery attempts + the Discord responses, the config/credentials/permissions/scheduling/retries. Classify client vs backend vs deployment/config; repair the ACTUAL cause. **Privacy rules (binding):** no webhook URLs, tokens, private-torrent info, or user-identifying data in logs/commits/reports; do not expand the collected stats or enable reporting for non-consenting users; preserve the intended scope. Verify: a controlled event reaches the channel, the values match the event, no duplicates, failed delivery = visible in diagnostics + retried (not silently dropped); any test message clearly identified + no private data; coordinate the live test first. Not "fixed" merely because an HTTP request was attempted. |
| 13 | Manager torrent hover poster: reduce 30% (70% of the current width/height, the aspect ratio preserved, the popup re-fit, no library/Details changes) | **Antigravity** (owner-assigned) | OPEN | Verify the hover popup in the packaged Windows + macOS clients; confirm no interference with the title clicks/context menus. |

## 4. Candidate & results

- **Proposed candidate:** `644e3b4` (CI run 39347930247: **SUCCESS**). Contains all Windows repairs: NSIS pre-extract shutdown, console suppression, synchronous native window drag with packed cursor coordinates, interactive-control exclusion on titlebar double-click, and full test suite passing. Ready for Factory's candidate verification battery.
- **Factory confirms the candidate:** `644e3b4` = the single candidate for
  packaging (local tree synced at `6473593`, docs-only on top). Antigravity's
  packed-lParam drag refinement reviewed and adopted — it corrects Factory's
  POINTS-pointer form to the proper Win32 packed coordinates. Agents agree.
- **Candidate supersession warning (task 8):** the chapter-timeline repair
  WILL change the agreed candidate. Task 8's fix must land on main, the record
  §4 must be updated with the new shared SHA, and the affected checks
  (lint/test + Factory's runtime battery) rerun BEFORE packaging. Do not
  package `644e3b4` while task 8 is open.
- **Desktop access:** currently NO agent holds it — the owner has the failed
  installer dialog open on the physical desktop (Retry is safe to click now:
  the DLL conflict is cleared). Factory requests access for the verification
  battery after the owner dismisses/completes the installer.
- **BLOCKER for task 5 (Factory's Windows acceptance):** the candidate
  produces no CI artifacts and the test desktop has no Rust toolchain, so
  Factory cannot build the installer. Requesting Antigravity to dispatch the
  Release workflow for a PRERELEASE candidate tag (e.g. `v0.2.2-beta.2-rc.1`)
  from the confirmed candidate so Factory can run the full battery (drag,
  installer upgrade cases, 30-min connectivity) on the exact packaged bytes.
  The final coordinated v0.2.2-beta.1 asset update follows Factory's
  confirmation per §3.7.
- **Windows acceptance evidence (prior, still valid):** integrated-package
  30+ min zero-disconnect run, mesh HTTPS 200 verified, kill/adopt held the
  stream — on the INSTALLED v0.2.2-beta.1 build (4cf142f). To be RE-RUN on the
  new candidate per §3.5.

## 5. Rules for this record

- Update §1 (state) and §3 (board) at every task boundary.
- New blocker → add a row, set the owner, never delete another agent's rows.
- Candidate freeze = both agents confirm the same SHA here.

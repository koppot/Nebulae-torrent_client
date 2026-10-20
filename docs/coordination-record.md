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
- **Branch / base:** `main`, integrated with Factory's `ec5e678`.
- **Working tree:**
  1. `src-tauri/src/player/bluray_meta.rs`: fixed C ABI struct layout for `BLURAY_TITLE_CHAPTER` (`BdTitleChap`). Replaced 24-byte broken definition with the true 48-byte layout (`idx`, `start` at offset 8, `duration` at offset 16, `offset` at offset 24, `clip_ref`, `chapter_name`), with dynamic stride fallback (40 bytes for libbluray < 1.5.0). Expanded matching tolerance to 30s. Return `(Vec<i64>, i64)` (offsets and matched playlist duration).
  2. `src-tauri/src/commands/media.rs`: `player_disc_chapters` passes matched playlist duration into `ChapterMark.duration_ms`.
  3. `src/components/player/PlayerOverlay.tsx`: removed misleading `base = mk.duration_ms > 0 ? mk.duration_ms : dur` divisor and rescale formula in `jumpChapter`; seekbar ticks and playhead share the exact same `dur` denominator (`(mk.offset_ms / dur) * 100%`) and milliseconds scale; ticks filtered strictly to interior marks `0 < mk.offset_ms < dur`; `jumpChapter` seeks directly to `mk.offset_ms` with standard 2.5s restart-vs-previous behavior; title 0 (disc menu) clears chapter marks.
  4. `src-tauri/examples/chap_probe.rs`: updated to inspect `(offsets, playlist_dur)`.
- **Verification:**
  - `npx tsc --noEmit`: 0 errors.
  - `pnpm run build`: built in 1.54s, 0 errors.
  - `cargo test --lib`: 119 passed, 0 failed, 1 ignored (including `test_bluray_91_days_metadata_probe` verifying 6 chapters spanning the full 1453994 ms timeline).
  - Integration tests (`cargo test --test engine_integration`): 9 passed, 0 failed.
  - Live probe on `/Volumes/300TB/Blu-ray/91 DAYS US/91 DAYS D1` (Title 8, 1453994 ms): 6 chapters at `[0, 38121, 128002, 700032, 1350015, 1439980]` ms (last mark at 99.03% of title).
  - Live probe on `/Volumes/300TB/Blu-ray/Fuuka (2017) Funimation/FUUKA_CS_D1` (Title 2, 1452075 ms): 7 chapters at `[0, 69986, 160034, 836961, 1330996, 1421002, 1445986]` ms (last mark at 99.58% of title).
- **Prior client fixes preserved:** Disc menu navigation, non-blocking playback, sequential download toggle, super-seeding dialog, status bar, first-run media modal, native in-process rfd dialogs, console window suppression, Win32 synchronous native drag.

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
| 8 | Chapter timeline alignment & libbluray ABI repair | Antigravity (investigation + repair); Factory (integration + candidate verification) | **DONE, verified** | Reopened defect investigated across failing disc sources (`91 DAYS D1`, `FUUKA D1`). Root cause: 1) C ABI struct mismatch in `bluray_meta.rs` (`BdTitleChap` was 24 bytes vs 48-byte `BLURAY_TITLE_CHAPTER`, corrupting offsets and truncating chapter lists to head ~2 mins); 2) Inverted duration denominator (`mk.duration_ms` vs `dur`) in `PlayerOverlay.tsx` and `seekChapter`; 3) Fragile 5s playlist matching threshold. Fixed: 48-byte struct with dynamic <1.5.0 stride detection, exact duration denominator matching `poll.length_ms`, 30s playlist matching tolerance, all chapters verified spanning 100% of the timeline. |
| 9 | Build-only candidate workflow (no publish) | Factory (workflow landed); Antigravity (dispatch when candidate frozen) | **WORKFLOW LANDED (`7e044be`)** | `candidate-build.yml`: workflow_dispatch with an explicit source SHA; builds all 4 platforms with the SAME steps as Release; uploads installers/checksums/signatures as workflow ARTIFACTS only (retention 14d). No release, no tag, no latest.json, no announcement. **Antigravity: dispatch with the agreed candidate SHA once task 8 lands; Factory downloads + verifies source identity + runs the acceptance battery on the exact bytes.** |
| 10 | Chapter-marker evidence capture on the failing disc | Factory + Antigravity | **DONE (values captured in §1)** | Evidence captured on failing discs (`91 DAYS D1`: Title 8, dur 1,453,994 ms, 6 chapters at 0, 38121, 128002, 700032, 1350015, 1439980 ms; `FUUKA D1`: Title 2, dur 1,452,075 ms, 7 chapters at 0, 69986, 160034, 836961, 1330996, 1421002, 1445986 ms). Values consumed and fixed in task 8. |
| 11 | Chapter marks barely visible (owner screenshot; timeline positions already correct in the owner's build) | **Factory** (owner assigned this message to Factory) | **FIXED, awaiting owner visual OK** | `PlayerOverlay.tsx` seek bar: replaced the 1px white/70 ticks (invisible white-on-white) with 2px dark `#1f1f1f` notches, 12px tall (2.4× the 5px track), centered via transform, with a 1px white/70 ring so they read on both the played fill and the track. Canvas NOT needed — the DOM achieves crisp contrast without touching the approved bar geometry. **Merge note for Antigravity:** this touches the tick RENDERING only (the `chapterMarks.slice(1)` map's className); the position math (`base`/`leftPct`) is untouched — coordinate if the timeline fix edits the same JSX block. |
| 12 | **Discord download statistics not reaching `nebulae-stats`** | **Factory** (owner-assigned) | **CAUSE FOUND — reporter deployed, awaiting webhook secret** | **End-to-end trace:** (1) the client repo has NO stats-reporting code (the only Discord integration = the release ANNOUNCEMENT webhook in release.yml, a different feature); (2) neither does `koppot/nebulae-server`; (3) nothing on the tracker server (45.135.163.139: the crontab/services = no reporter); (4) **the actual mechanism = an external Discord bot deployment** — the channel screenshot shows `Nebulae Telemetry` (APP, ADC avatar): "queried directly from official GitHub Release distributions" — whose LAST post = 2026-10-13 18:32 (`test` + one embed) = it went silent the same day the leaked release webhook was revoked in this repo. **Classification: deployment/config defect of an external bot whose host is outside every repo/server we can access** (the coordination blocker). **Repair delivered:** `stats-report.yml` — a GitHub-hosted reporter with the SAME data source and embed format (aggregate public release download counts, grouped per package; daily digest 06:00 UTC + workflow_dispatch for controlled tests; test posts clearly labeled 🧪; webhook = repo secret `DISCORD_STATS_WEBHOOK_URL`, never printed; bounded 3-attempt retry with backoff; failed delivery = visible step failure; counts are cumulative so the next run self-heals; payload validated live: 114 total, 947 bytes). **Needs the owner:** set the `DISCORD_STATS_WEBHOOK_URL` secret (a nebulae-stats channel webhook); **pause or repoint the old external bot** or the channel gets duplicate posters. No client code changed → no client rebuild required for this item. |
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

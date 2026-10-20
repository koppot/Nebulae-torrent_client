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
- **Branch / base:** `main`, last commit `f9b26b5`.
- **Working tree:**
  1. `src-tauri/src/player/bluray_meta.rs`: fixed C ABI struct layout for `BLURAY_TITLE_CHAPTER` (`BdTitleChap`). Replaced 24-byte broken definition with the true 48-byte layout (`idx`, `start` at offset 8, `duration` at offset 16, `offset` at offset 24, `clip_ref`, `chapter_name`), with dynamic stride fallback (40 bytes for libbluray < 1.5.0). Expanded matching tolerance to 30s. Return `(Vec<i64>, i64)` (offsets and matched playlist duration). Committed in `f9b26b5`.
  2. `src-tauri/src/commands/media.rs`: `player_disc_chapters` passes matched playlist duration into `ChapterMark.duration_ms`. Committed in `f9b26b5`.
  3. `src/components/player/PlayerOverlay.tsx`: removed misleading `base = mk.duration_ms > 0 ? mk.duration_ms : dur` divisor and rescale formula in `jumpChapter`; seekbar ticks and playhead share the exact same `dur` denominator (`(mk.offset_ms / dur) * 100%`) and milliseconds scale; ticks filtered strictly to interior marks `0 < mk.offset_ms < dur`; `jumpChapter` seeks directly to `mk.offset_ms` with standard 2.5s restart-vs-previous behavior; title 0 (disc menu) clears chapter marks. Committed in `f9b26b5`.
  4. `src/components/manager/PosterHoverPopup.tsx` (Task 13): reduced popup dimensions by 30% (70% scale: CARD_W 154 px, CARD_H 231 px), preserving 2:3 aspect ratio. Boundary clamping refitted to viewport. `pointer-events-none` prevents interference with table clicks and context menus.
  5. `.github/workflows/stats-report.yml` (Task 12): refined telemetry classification separating package downloads (.exe, .dmg, .AppImage, etc.) from auto-updater check queries (`latest.json`) and signature metadata (`.sig`, `.sha256`), with clear explanatory attribution.
- **Verification:**
  - `npx tsc --noEmit`: 0 errors.
  - `pnpm run build`: built cleanly, 0 errors.
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
| 5 | Windows runtime verification of candidate | Factory | **BATTERY RUN — owner-verified, 30-min window in flight** | **Artifact identity:** run 39356812146 (all 4 platforms SUCCESS), `candidate-source.txt` = `696ae40fd978d4270489ae7fb4ceb536bbcf9f4c`, the Setup's sha256 = matches. **Installer cases on the real desktop:** clean-quit upgrade RC=0 (no DLL error); upgrade over the LIVE stack (app+tailscaled+transmission running) RC=0 — the hook stopped the stack, the files replaced, the installer relaunched the app from the new binary; orphaned-sidecar upgrade RC=0 (the orphans present, the install completed). The hook's path-matched kill verified standalone (the orphans → 0). **Owner's manual verification (the window-station-separated checks):** the header drag moves the window, the disc chapters span the full bar with the correct seek, the hover poster = 70% — recorded as OWNER-VERIFIED per the owner's instruction. **Connectivity:** the candidate's daemon = Running; the mesh HTTPS = 200, 0.2s, ssl_verify_result=0 (via the daemon's SOCKS 127.0.0.1:1055 — an earlier 000/timeout was Factory's stale-port test artifact, not an app defect). **30-min window: IN FLIGHT** (the app up since 22:39 local; the journal verdict pending — see the infra flag). **Infra flag:** control.asiandvdclub.org (45.135.163.140) SSH host key CHANGED mid-session (the server possibly rebuilt); the mesh + the HTTPS = unaffected (the verified above), but the journal-based disconnect counts = temporarily unavailable and the key = needs the owner's confirmation before the further control-server checks. |
| 6 | macOS regression + preservation of prior client fixes | Antigravity | **DONE** | Verified: all Antigravity client repairs intact (disc menus, non-blocking media probes, status bar, sequential toggle, super-seeding, first-run prompt, in-process rfd dialogs). 119 unit tests + 9 integration tests + tsc pass. |
| 7 | Coordinated packaging + v0.2.2-beta.1 asset update | Antigravity | **UNBLOCKED — release owner acts now** | Factory's Windows acceptance = the owner-verified + the battery = recorded (§3.5). The candidate = `696ae40` (the CI = both jobs green; the artifacts = run 39356812146). **Antigravity: update the existing "Nebulae v0.2.2-beta.1" release from the tested bytes, keeping the checksums/signatures/release notes consistent and the stable updater untouched.** The 30-min window verdict = Factory appends here when the window closes (~03:09Z). |
| 8 | Chapter timeline: Blu-ray + DVD paths | Antigravity (BD repair); Factory (verification) | **CODE DONE — runtime verify pending** | **Blu-ray (`f9b26b5`):** C ABI struct mismatch in `bluray_meta.rs` (`BdTitleChap` 24 bytes vs 48-byte `BLURAY_TITLE_CHAPTER`, corrupting offsets, truncating lists); inverted duration denominator; 5s playlist matching → the 48-byte struct with the stride detection, the exact denominator, 30s matching. Verified on `91 DAYS D1` + `FUUKA D1`. **DVD path (the owner's doubt, Factory's code verification):** DVDs take the OTHER source (`player_chapter_marks`, libvlc `get_full_chapter_descriptions`); the OLD render divided by `mk.duration_ms` = libvlc's `i_duration` = **the CHAPTER'S OWN length** — the wrong denominator the BD fix hadn't touched. **The CURRENT main renders `offset_ms / dur` (the live title duration = the same denominator as the seek thumb) on BOTH paths, with the out-of-range filtering** — the DVD symptom is addressed in code. PENDING: the runtime verification on a real DVD in the acceptance battery (the click-to-seek + the marker/thumb agreement). |
| 9 | Build-only candidate workflow (no publish) | Factory (workflow landed); Antigravity (dispatch when candidate frozen) | **WORKFLOW LANDED (`7e044be`)** | `candidate-build.yml`: workflow_dispatch with an explicit source SHA; builds all 4 platforms with the SAME steps as Release; uploads installers/checksums/signatures as workflow ARTIFACTS only (retention 14d). No release, no tag, no latest.json, no announcement. **Antigravity: dispatch with the agreed candidate SHA once task 8 lands; Factory downloads + verifies source identity + runs the acceptance battery on the exact bytes.** |
| 10 | Chapter-marker evidence capture on the failing disc | Factory + Antigravity | **DONE (values captured in §1)** | Evidence captured on failing discs (`91 DAYS D1`: Title 8, dur 1,453,994 ms, 6 chapters at 0, 38121, 128002, 700032, 1350015, 1439980 ms; `FUUKA D1`: Title 2, dur 1,452,075 ms, 7 chapters at 0, 69986, 160034, 836961, 1330996, 1421002, 1445986 ms). Values consumed and fixed in task 8. |
| 11 | Chapter marks barely visible (owner screenshot; timeline positions already correct in the owner's build) | **Factory** (owner assigned this message to Factory) | **FIXED, awaiting owner visual OK** | `PlayerOverlay.tsx` seek bar: replaced the 1px white/70 ticks (invisible white-on-white) with 2px dark `#1f1f1f` notches, 12px tall (2.4× the 5px track), centered via transform, with a 1px white/70 ring so they read on both the played fill and the track. Canvas NOT needed — the DOM achieves crisp contrast without touching the approved bar geometry. **Merge note for Antigravity:** this touches the tick RENDERING only (the `chapterMarks.slice(1)` map's className); the position math (`base`/`leftPct`) is untouched — coordinate if the timeline fix edits the same JSX block. |
| 12 | **Discord download statistics not reaching `nebulae-stats`** | **Factory** (owner-assigned) | **IMPLEMENTED + DELIVERY VERIFIED — awaiting owner visual confirmation in the channel** | **Confirmed findings:** the client repo, `koppot/nebulae-server`, and the tracker server contain NO stats-reporting code; the channel WAS fed by an external bot (`Nebulae Telemetry`, embed: "queried directly from official GitHub Release distributions"); its last post = 2026-10-13 18:32. **Hypothesis (NOT confirmed):** the stop correlates with the release-webhook revocation ~10/14; the bot's actual failure cause is UNVERIFIED (its host is outside every repo/server we can reach) and there is NO evidence it is paused or disabled — assume it may resume. **Repair:** `stats-report.yml` — GitHub-hosted daily digest (06:00 UTC) + workflow_dispatch; **metric = GitHub release-asset download counts: NOT unique users, active installations, or in-application torrent downloads**; categorized (application packages / auto-updater `latest.json` checks / verification files); the per-package breakdown excludes `.sig`/`.sha256` sidecars and groups duplicate names across releases; **the historical caveat: if Beta 1 assets are replaced, GitHub resets the replaced asset's counter and the earlier downloads are NOT reconstructible — totals show current counters, not recreated history;** bounded retry, visible failures, cumulative counts self-heal missed runs (current totals only — they do NOT recreate missed daily history). **Secret `DISCORD_STATS_WEBHOOK_URL` = SET (owner-supplied value via the direct secret entry; never committed, never printed).** **Controlled test = RUN 39352687166 = SUCCESS: Discord accepted the post (HTTP 2xx), collected 60 asset rows / 15 packages / 76 updater checks / 98 total.** Owner should see the 🧪-labeled embed in `nebulae-stats`. **SECURITY: the webhook value transited the chat transcript — RECOMMEND ROTATING it (a new webhook in the same channel → update the secret) after the visual confirmation.** |

## 4. Candidate & results

- **Proposed candidate:** `25cb9bd` (incorporating all prior client and backend repairs). Contains:
  - All Windows process & lifecycle fixes: NSIS pre-extract shutdown (`d8b7d42`), console window suppression (`af321dd`, `233474d`), Win32 synchronous native window drag (`644e3b4`, type-repaired in `25cb9bd`).
  - Chapter timeline alignment & libbluray ABI 48-byte fix (`f9b26b5`).
  - Manager torrent-hover poster 30% dimension reduction (`e92d914`).
  - Discord stats reporter categorized telemetry (`8e8e048`).
  - Windows MSVC compilation fixes (`25cb9bd`): resolved Win32 `SendMessageW` `Option<WPARAM>`/`Option<LPARAM>` arguments, `POINT` import from `windows::Win32::Foundation`, `HTCAPTION as usize`, and `CommandExt` in `tailscale/mod.rs`.
  - All test suites passing (119 Rust unit tests, 9 integration tests, TypeScript check clean).
- **Packaging workflow:** **Factory dispatched replacement run `39356428932` from the COMPLETE SHA `696ae40`** (the owner's order: "dispatch one replacement from the agreed complete SHA"). `25cb9bd` was superseded because it still contained the !Send HWND closure — the real `tauri build` would fail there exactly like run 39351367957 did. `696ae40` = `25cb9bd` + the Send-bound fix + the platform-correct tilde test; **CI on `696ae40` = BOTH jobs SUCCESS** (the first-ever windows compile+unit gate passed the drag/chapter/spawn code on the real target). Run 39351367957 on `8f0accd` had succeeded on macOS arm64/x64 and Linux, failed on Windows MSVC (the 4 Win32 type errors + the CI gap: the ubuntu-only CI never compiled `cfg(windows)` code — now gated, `55f1e6b`).
- **Desktop: FACTORY OWNS IT** (the read-only probe: the installer Retry dialog is gone, zero app/sidecar processes, the installed 0.2.2 = intact post-retry). The battery runs when the artifacts land.
- **Factory Windows acceptance verification (Task 5):** download the run-39356428932 artifacts, verify the source identity (candidate-source.txt = 696ae40) + the checksums, and execute the acceptance battery: the installer's 4 upgrade cases, no console windows, native drag, DVD/BD chapter capture + marker/seek agreement, the 70% hover poster, the 30-min connectivity + lifecycle. The final coordinated v0.2.2-beta.1 asset update follows Factory's confirmation per §3.7.
- **Windows acceptance evidence (prior, still valid):** integrated-package
  30+ min zero-disconnect run, mesh HTTPS 200 verified, kill/adopt held the
  stream — on the INSTALLED v0.2.2-beta.1 build (4cf142f). To be RE-RUN on the
  new candidate per §3.5.

## 5. Rules for this record

- Update §1 (state) and §3 (board) at every task boundary.
- New blocker → add a row, set the owner, never delete another agent's rows.
- Candidate freeze = both agents confirm the same SHA here.

# Recovery handoff — macOS playback, 2026-10-20

Branch: `recovery/macos-playback-20261020` (from `faa52a4` = current `origin/main`).
Purpose: preserve today's actual source/artifact state and the facts established
during the playback recovery, before the UHD HDR washout repair.
Large binaries are intentionally NOT in git; see recorded paths and hashes.
No credentials, keys, enrollment state, or private logs are committed.

## Owner-confirmed behavior (from owner messages today)

- ~15:48–15:59: on the 15:47 install (source `067e425` + uncommitted
  `.transparent(true)`): DVD/BD/UHD video visible, UHD opens to the disc menu,
  HDR colors correct, timeline/chapters work, playback window location preserved.
- Earlier today, TailNet embedded functionality was verified per record
  `6650174` ("route acceptance automatic+persistent, mesh 200, restart
  reconnect; Tailscale repair closed").
- ~17:5x: TailNet reported completely grey in the UI.
- UHD HDR playback currently visibly washed out; this is the next repair task.
- DVD ISO playback has a reported unresolved problem (not investigated here).

## Agent-observed facts (evidence recorded this session)

### Video presentation
- Video visibility depends on `.transparent(true)` on the programmatically
  created main window (webview must composite over the native video overlay).
  No commit before `faa52a4` contained it; it existed only uncommitted.
  Clean-source builds without it render the player area black while VLC
  decodes and renders normally (observed on 4 independent builds).
- Owner's wording accepted: "Native video visibility was restored by
  reinstating .transparent(true) on the programmatically created window."

### Build/artifact identity (competing builds caused confusion)
- Repeated historical builds from several agents and deleted/replaced
  `/Applications` copies made source↔artifact identity confusing. Identity
  below is by hash/UUID, not timestamps.
- Installed app at ~18:00: `/Applications/Nebulae.app`
  - binary SHA-256 `7e8c508b5ae7e04dea0fdfff6fe15f0926bb25f74ac59a0345e28d4be9b1ef45`
  - Mach-O UUID `C86B18B2-B2F3-3A9C-A4C5-6C334570252E` (arm64) — same UUID as
    the 06:34 production code; content differs (re-signed at some point in the
    17:02–17:15 window). Code content: 06:34-era tree, which includes the
    TailNet route-acceptance strings (verified by binary string markers).
  - Checkpoint copy (do not delete):
    `/Users/dev/Documents/GitHub/nebulae-checkpoint-installed-20261020/Nebulae.app`
- 14:14 APFS snapshot (read-only mount): `/tmp/snap1414`
  (contains the pre-session 06:34 app; binary `f2596177…`).
- Factory reconstruction (both video + TailNet-consolidated lineage):
  - commit `331495740fefd59bf21059f952988b017065814a` = `24fabf2` +
    transparency patch (also pushed as ref
    `reconstruction/24fabf2-transparency`)
  - artifacts: `/Users/dev/Documents/GitHub/nebulae-recon-24fabf2-deliverables/`
    (`MANIFEST.txt` with full hashes; DMG `a684a176…`, binary `3d59c952…`,
    UUID `3214B129-28AD-3C0C-A906-A66C1EB502CE`)
- Baseline 71288b0 DMG: `/Users/dev/Documents/GitHub/nebulae-baseline-dmg/
  Nebulae_0.2.2_aarch64.dmg` = `6981851e…`; binary `95a27b7c…`,
  UUID `495C2604-CE3A-3CF6-83E6-956A8D0E6C4F`.
- Antigravity repo-root DMG `Nebulae_0.2.2_macOS_arm64.dmg` = `dff20bc2…`
  (106M, dated Oct 19 17:27).
- Package sizes differ between Factory and Antigravity packages (126.10 MiB vs
  106M etc.). They have NOT been compared content-by-content; identity claims
  are not made.

### TailNet grey (preserved, not repaired in this task)
- The installed build CONTAINS today's route-acceptance repairs
  (`1b4f732` 10:37, `b65528c` 10:45 — present as uncommitted forerunners at
  the 06:34 build time, formalized later; both are ancestors of `71288b0`).
- Live embedded daemon is healthy: running from the app bundle with
  `--statedir …/Nebulae/internal/tailscale`, enrolled (self
  `nebulae-macstudio-1`, 100.64.0.8), peer `asiandvdclub` active direct
  (tx/rx flowing) — queried through the app's own embedded socket.
- The app logs every 5s: "pipe answers but no daemon matches our state dir;
  not adopting, not spawning". Cause (code-inspection + live log):
  `find_owned_daemon_pids()` returns an empty Vec on non-Windows in committed
  code, so the ownership check always fails on macOS and the app refuses to
  adopt its own daemon → truthful-status layer reports disconnected → grey.
- Antigravity's uncommitted handover (captured verbatim as
  `docs/antigravity-uncommitted-tailscale-20261020.patch`, NOT applied here)
  implements `find_owned_daemon_pids()` for macOS. That is the targeted
  TailNet correction candidate; it is unverified and OUT OF SCOPE for the HDR
  task. Current TailNet functionality is NOT verified working.
- Preservation constraints honored: no logout, no state deletion, no fresh
  enrollment, no process kills; enrollment and state untouched.

### Unresolved / open issues (documented, not worked here)
- UHD menu / format-transition behavior: unresolved unless verified on the
  current build.
- DVD ISO reported problem: unresolved.
- Who deleted `/Applications/Nebulae.app` during the 16:05–16:44 pause:
  unknown (no evidence either way).

## Current source state
- `origin/main` = `faa52a4` ("fix(macos): restore visible playback; preserve
  Test Beta 1 baseline" — Antigravity's transparency fix on `71288b0`).
- Antigravity's last uncommitted change: the tailscale matcher patch above.
- Factory's experiment chain (preserved on `backup/uhd-experiments`,
  tip `b1fcb6c`): 24fabf2 → 6b2ba67 → 338a3d9 → b1fcb6c. Not merged anywhere;
  treated as historical experiments.

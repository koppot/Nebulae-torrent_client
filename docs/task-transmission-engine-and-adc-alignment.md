# Tasking for Antigravity — transmission-daemon sidecar engine + ADC connectivity alignment

Issued by the owner, 2026-10-15. Written by Droid (Factory) at the owner's
direction, grounded in the current state of both repositories. Master copy
lives in **koppot/Nebulae-torrent_client → `docs/task-transmission-engine-and-adc-alignment.md`**
(this file). The tracker-side portion is mirrored into the ADC repository at
`docs/task-tracker-peer-alignment.md`.

Read this whole document before writing any code. Record progress, decisions,
and pinned hashes in `AGENT-COORDINATION.md` under your own heading.

---

## 0. Operating rules — OTHER AGENT SESSIONS ARE LIVE ON THIS MACHINE

Multiple AI clients and sessions (Droid, Antigravity, others) work in these
repositories concurrently. These rules are hard requirements:

1. **Serial, scoped work.** One task at a time. No parallel test runs, no
   background processes you do not clean up, no killing processes you did
   not start (other sessions may own dev servers, runners, container VMs).
2. **Inspect `git status` immediately before every staging step.** Stage
   only files you changed for this task. Never `git add -A`, never
   `git commit -a`.
3. **Forbidden git operations:** `reset --hard`, `clean`, `checkout`/
   `restore` over dirty paths, `stash drop/clear`, force-push, moving or
   deleting tags, rewriting shared history. If a dirty file blocks you,
   stop and record the blocker instead of discarding it.
4. **Branch, don't idle on main:** do the engine work on branch
   `ag/transmission-engine` off current `nebulae-server` main. Small,
   well-scoped commits. The owner merges and releases.
5. **No releases, tags, or version bumps on your own.** Releases are the
   owner's action (see the versioning policy note in AGENT-COORDINATION.md).
   CI verification is your job; publishing is not.
6. **No touching the provider-neutral signing gate**
   (`WINDOWS_SIGN_PROVIDER` logic in `.github/workflows/release.yml`) and
   no changes to the Tauri updater signing secrets wiring.
7. **First action:** add a one-line acknowledgment in
   `AGENT-COORDINATION.md` ("tasking acknowledged, starting Phase A0") so
   the owner can see you picked it up.

---

## 1. Non-negotiable constraints (private-tracker security boundary)

These are the security invariants of the whole task. Every design decision
below is subordinate to them.

- **NEVER enable PEX, DHT, or Local Peer Discovery for private torrents.**
  Nebulae is an Asian DVD Club (ADC) client; ADC is a private tracker. PEX
  would let banned/unauthenticated parties harvest member peer IPs. Note
  that an earlier general BitTorrent recommendation ("send PEX queries every
  60 seconds") is **explicitly rejected** for this project — the owner's
  own constraint list forbids it. PEX/DHT/LPD stay off at the session level
  as defense-in-depth, even though the `private: 1` flag already disables
  them in compliant engines.
- **Never modify anything inside the `info` dictionary** of an existing
  torrent (alters the info-hash; the tracker rejects the announce and the
  member's ratio silently stops logging). No `url-list`/web-seed injection
  into private torrents, no added trackers, ever.
- **Never announce more frequently than the tracker's advertised
  `min interval`.** "Aggressive re-announcing" below the min interval is a
  bannable offense. The only allowed acceleration is Transmission's
  built-in immediate re-announce on tracker *failure* and on events
  (started/completed), which is already compliant behavior.
- **Renames and migrations must never delete user data.** See Phase A3.

---

## 2. What is ALREADY DONE — do not redo or regress it

Ground truth as of 2026-10-15 (verify, don't re-implement):

**Nebulae client (`koppot/Nebulae-torrent_client`, this repo, engine =
librqbit 9.0.1):**
- Tracker scrape layer giving real seeds/peers + tracker status messages
  (per-torrent task, 120 s cadence, 600 s error backoff, results merged
  into `list_torrents`; hand-rolled bencode parser for binary-keyed scrape
  dicts). Keep it working in librqbit mode; under Transmission use
  `torrent-get` `trackerStats` instead but keep the layer as fallback.
- UPnP port forwarding (`UserSettings.enable_upnp`, default on, passed to
  `ListenerOptions.enable_upnp_port_forwarding`).
- Speed/ETA display smoothing: cumulative byte counters + 3 s EMA in
  `src/stores/torrentStore.ts` (`SPEED_WINDOW_SECONDS = 3.0`). Owner has
  asked for a 5 s window — see Phase A2, this is a one-line delta.
- Torrent creator sets/preserves `private: 1` (`src-tauri/src/engine/metainfo.rs`).
- Honest rename on add (disk rename + re-add + recheck, commit 22f18c7);
  the librqbit no-op rename stubs are documented dead ends.

**ADC tracker (`ADC` repo, PHP, `app/controllers/announce.php`):**
- Upload rejects non-private torrents (`app/models/takeupload.php` step 4).
- `numwant` honored: default 50, peer list query `LIMIT :max_peers`.
- Announce response carries `complete`, `incomplete`, `downloaded`.
- `/scrape` endpoint with multi-info_hash support exists
  (`announceSession`'s sibling `scrapeSession`).
- Client whitelist infrastructure exists (`agent_allowed_family` table +
  `checkUserAgent()`), but the check call is **commented out** and the
  function itself has a bug (see Phase B2).

---

## 3. Part A — Nebulae: transmission-daemon as embedded engine (sidecar)

Decision (owner, 2026-10-15): adopt **transmission-daemon as a bundled
sidecar process**, controlled over its JSON-RPC from the Rust backend,
replacing librqbit as the default engine. The comparison that drove this:
trivial Tauri integration via the proven mpv sidecar pattern, no C++
toolchain in CI, ~5–8 MB binary, mature typed RPC, crash isolation, and
20 years of private-tracker compliance (a private flag instantly locks
down PEX/DHT/LPD). libtorrent-rasterbar was rejected for interface
reasons (FFI/Boost/OpenSSL CI pain; no clean headless daemon without Qt).

### Phase A0 — Recon (half a day)

1. Branch `ag/transmission-engine` off current `main`.
2. Read `src-tauri/src/engine/traits.rs` (the `TorrentEngine` trait),
   `rqbit.rs`, `mod.rs`, `lib.rs` (engine selection), `killswitch.rs`,
   `commands/settings.rs`, `src/stores/torrentStore.ts`.
3. Read `scripts/bundle-{windows,linux,macos}-mpv.*` and the release
   workflow player-staging steps — your bundling work must follow the
   same pattern, including the **staging-dir-outside-`target/`** rule
   (rust-cache nesting ate the player once; do not repeat it).
4. Write a short design note in AGENT-COORDINATION.md: RPC mapping table
   (trait method → RPC call), process lifecycle, config-dir layout,
   migration plan. Get owner sign-off in that file before Phase A1 code.

### Phase A1 — Sidecar bundling (mirror the mpv tasking)

Bundle `transmission-daemon` per OS into
`src-tauri/resources/engine/transmission/`:

- **Pinned download URL + SHA-256 per artifact, verified at build time** —
  same discipline as the mpv scripts. Prefer official Transmission 4.x
  artifacts; on Linux, where no official static binary exists, build the
  daemon from the pinned release tag in CI with GUI/utils disabled
  (`-DENABLE_DAEMON=ON -DENABLE_GTK=OFF -DENABLE_QT=OFF -DENABLE_UTILS=OFF
  -DENABLE_MAC=OFF`) and walk `ldd` / copy non-glibc libs + RPATH
  (`patchelf`), exactly like the self-contained Linux mpv/JRE bundle.
- macOS: extract the daemon binary + dylibs (otool walk, install-name
  rewrite, ad-hoc or identity codesign) like `bundle-macos-mpv.sh`.
- Windows: extract daemon exe + required DLLs; smoke-test that the binary
  actually runs on the runner (the mpv Defender-quarantine incident is the
  precedent — exclude the staging dir from Defender in CI).
- Add GPL v2/v3 notices for Transmission to `THIRD-PARTY-NOTICES.md`.
- Smoke test per leg: spawn daemon with a temp config dir, RPC
  `session-get` succeeds, kill it. All three OS legs green in CI before
  you claim the phase done.

### Phase A2 — RPC engine + settings mapping

Implement `TransmissionTorrentEngine` behind the existing `TorrentEngine`
trait; select via a new `UserSettings.engine` (`"librqbit" |
"transmission"`), default `"transmission"` once CI is green, keep
`"librqbit"` selectable as rollback.

Process/RPC rules:
- Spawn sidecar with `--config-dir <app-data>/engine/transmission`,
  `--port <free ephemeral port>`, **RPC bound to 127.0.0.1 only**
  (`rpc-bind-address`), `rpc-whitelist-enabled` with `127.0.0.1`,
  plus the `X-Transmission-Session-Id` CSRF handshake. No LAN exposure.
- Generate a random per-launch RPC token, pass via settings.json
  (`rpc-username`/`rpc-password`), never log it.
- Handle daemon death: restart with backoff; surface status in the UI
  (the Connectivity screen pattern).

Settings mapping (Transmission session-set / settings.json):

| Nebulae setting | Transmission key | Value |
|---|---|---|
| listen port | `peer-port` | from UserSettings |
| UPnP | `port-forwarding-enabled` | true (default on, as today) |
| — (new) | `peer-limit-per-torrent` | 100 (settings range 60–100) |
| — (new) | `peer-limit-global` | 400 (settings range 250–500) |
| — (new) | `upload-slots-per-torrent` | 6 (range 4–8) |
| — (new) | `encryption` | `preferred` |
| — (new) | `utp-enabled` | true |
| — (new) | `dht-enabled` | **false** |
| — (new) | `pex-enabled` | **false** |
| — (new) | `lpd-enabled` | **false** |
| — (new) | `bind-address-ipv4` | VPN interface IP when killswitch active |

DHT/PEX/LPD are hard-off at session level (Section 1). The `private: 1`
flag would disable them per-torrent anyway; we keep both layers.

**Killswitch inheritance (security-relevant):** the sidecar is a separate
process — it does NOT automatically inherit any VPN/killswitch logic the
Tauri app applies to librqbit. Study `killswitch.rs`; the Transmission
engine must satisfy the same guarantee (bind to the tunnel interface and
pause everything if the tunnel drops). If a mechanism cannot be honored,
stop and document the gap BEFORE shipping the engine as default.

**Engine behavior notes (owner's attractiveness list, mapped honestly):**
- *Unchoke slots:* `upload-slots-per-torrent` 4–8 concentrates upload
  bandwidth so ADC seedboxes reciprocate. Done via the table above.
- *Rarest-first:* verify Transmission's default (it is rarest-first
  adapted by availability); do NOT enable sequential download globally.
  If the bundled version exposes the sequential RPC flag, wire it
  per-torrent only for Media Station streaming playback, off otherwise.
- *Fast Extension (BEP 6), uTP (BEP 29), MSE encryption:* built into
  Transmission; enabled via the table. No code needed beyond settings.
- *Half-open connection concurrency:* not directly exposed in
  Transmission; do not fake it. `peer-limit-global` covers the pool.
- *Re-announce:* use `torrent-reannounce` only on explicit user action
  ("Update tracker" button) or tracker-failure recovery, never on a timer
  below the tracker's `min interval`.

### Phase A3 — Data migration + engine-surface integration

1. **Migration (no data loss, ever):** on first launch with the
   Transmission engine, build a one-time import: for each existing librqbit
   torrent — export metainfo bytes, locate its save destination, add
   paused, verify the data layout (multi-file root-join rule from the
   v0.9.0 fix applies to Transmission's expectations too — verify), force
   recheck, then start. Keep librqbit's resume data untouched so rollback
   stays possible. Write the migration log to AGENT-COORDINATION.md.
2. **Swarm stats:** seeds/leechers per torrent from
   `torrent-get` `trackerStats` (`seederCount`/`leecherCount`, plus
   `lastAnnounceResult` for the Tracker Status cell). Retire the
   librqbit-only scrape task in Transmission mode (keep it for librqbit
   mode). The seeds column must show real values in all modes now —
   the "dash in local mode" interim state ends with this engine.
3. **Rename:** implement `rename_torrent` / `_file` / `_folder` for real
   via Transmission RPC `torrent-rename-path` (it supports paths and
   folders natively). Remove the librqbit disk-rename+re-add workaround
   path only in Transmission mode; keep it for librqbit mode. Re-enable
   the UI file/folder rename controls that were removed for librqbit.
4. **Speed display:** change `SPEED_WINDOW_SECONDS` from `3.0` to `5.0`
   in `src/stores/torrentStore.ts` (owner asked for a 5-second rolling
   window) and keep the cumulative-counter EMA approach — Transmission's
   `rateDownload` is as spiky as librqbit's; counters
   (`downloadedEver`/`uploadedEver`) remain the stable input.
5. **Piece size default (creator):** `commands/torrent.rs` defaults to
   1 MiB. For ADC-scale content raise the default: ≥ 15 GiB total →
   8 MiB, ≥ 50 GiB → 16 MiB, user-adjustable, never touching the `info`
   dict after creation. `private: 1` stays mandatory for created torrents.
6. **Connectivity screen:** wire the existing port/connectivity check to
   Transmission RPC `port-test` in Transmission mode.

### Phase A4 — Gates

- `pnpm build` (lint + tsc + vite) clean; `cargo fmt`/`clippy -D
  warnings`/`cargo test` green; no new eslint suppressions.
- Rust unit tests for the RPC client (session-id handshake, error
  mapping, settings mapping table) — mock the HTTP layer; no network.
- CI: all OS legs green. Do NOT retag v0.9.1 or cut releases; report
  here and wait for the owner.

---

## 4. Part B — ADC tracker side (mirrored in the ADC repo)

Master file in the ADC repo: `docs/task-tracker-peer-alignment.md`.
Work on a **fresh branch off current `ADC` `main`** — the existing
`ADC-antigravity` checkout is 141 commits behind and must not be
continued. ADC's `AGENTS.md` rules apply in full: tracker changes are
committed **and deployed to production** per `docs/deploy.md`; DB data
changes and operational actions need separate owner authorization; stop
and report blockers instead of bypassing.

What remains there is small; most of the owner's Part-1 list already
exists in production code:

1. **numwant cap (code, deployable):** clamp
   `$client['numwant'] = min((int)$client['numwant'], 150);` after the
   default-50 block in `announce.php` so a hostile client cannot request
   the whole swarm. Add/extend a test.
2. **Whitelist Nebulae (staged, needs owner authorization for the DB):**
   - Fix `checkUserAgent()`'s self-check bug first
     (`preg_match($allowedItem['agent_pattern'], $allowedItem['agent_start'], ...)`
     validates the pattern against the wrong subject) and its inverted
     early-returns — with unit tests, still disabled in production path.
   - Add a **log-only dry-run mode**: when enabled, run the check and
     only record rejections to a log/DB column for a full announce
     cycle. Review with the owner before enforcing; enforcement that
     misfires locks real members out of their ratio.
   - Whitelist entry: if Nebulae presents a Transmission core, its peer
     id will be Transmission's (`-TR…`), which is likely already allowed
     for existing versions — VERIFY coverage for the bundled version and
     extend the version floor if needed. If the owner instead wants a
     distinct `-NE…` prefix for Nebulae, that is an owner decision plus a
     DB row (`agent_allowed_family`) — data change, separate
     authorization, never committed as a code-side seed migration.
3. **Explicitly out of scope / forbidden on the tracker:**
   - No PEX support of any kind, no DHT-related endpoints.
   - No changes to `announce_interval` / `announce_interval_min`
     (clients must honor them; do not add "fast announce" features).
   - No metainfo rewriting at download time; `private: 1` is enforced at
     upload and must stay untouched end-to-end.
   - The per-announce connectback check (`fsockopen`, 2 s) is existing
     behavior — do not "optimize" it in the same change.

---

## 5. Acceptance checklist (the definition of done)

Nebulae:
- [ ] Bundled transmission-daemon verified on all 3 CI OS legs (spawn,
      `session-get`, clean kill), pinned URLs + SHA-256 recorded here.
- [ ] `engine` setting switches librqbit/transmission; librqbit rollback works.
- [ ] DHT/PEX/LPD confirmed off at session level in every mode (test asserts it).
- [ ] Private torrents: announce URL is the ADC passkey URL only; a
      packet/log-level check shows no tracker traffic to any other host.
- [ ] Peer limits, upload slots, encryption, uTP set per the table; user-facing.
- [ ] Seeds column real values in Transmission mode; Tracker Status cell live.
- [ ] Rename of top-level folder AND files/folders works in Transmission mode.
- [ ] Migration imports existing torrents with zero data loss (dry-run on
      a scratch profile first); rollback path documented.
- [ ] Killswitch guarantee holds with the sidecar engine or is documented
      as a blocker BEFORE defaulting to it.
- [ ] Speed window 5 s; ETA from the same smoothed series.
- [ ] Creator defaults: 8/16 MiB pieces for large content, private forced.
- [ ] `pnpm build`, fmt, clippy, tests green; no signing-gate changes;
      no tags/releases.

ADC:
- [ ] numwant cap committed and deployed per `docs/deploy.md`.
- [ ] `checkUserAgent` fixed with tests; dry-run mode reviewed with owner;
      enforcement only after the owner reviews dry-run logs.
- [ ] Whitelist coverage confirmed for the bundled engine's peer id.
- [ ] No PEX/DHT/interval/metainfo changes anywhere in the diff.

Process:
- [ ] Acknowledgment + progress notes in AGENT-COORDINATION.md (both repos
      as applicable), pinned hashes recorded, blockers raised before
      approach changes, no destructive git operations, only own files staged.

— Droid (Factory), 2026-10-15, at the owner's direction.

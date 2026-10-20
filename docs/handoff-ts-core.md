# Tailscale-core handoff — FINAL 2026-10-19 (Windows private ADC access WORKING)

## INTEGRATED RELEASE VERIFICATION (v0.2.2-beta.1, the tagged candidate)

Provenance (confirmed against the owner's `docs/acceptance-reconciliation.md`):
tag `v0.2.2-beta.1` → tag object `3fe3215719b15bd5f9367c08525b25f2437dff63`
→ peeled source `4cf142f2e423147ad48fdb7305f1f318e52aaabf` = the Release
workflow run 39330027020's checked-out build SHA (no drift). The
tailscale code in the tag = byte-identical to the Factory-tested branch
(diff empty; all fixes present: keepalive, peer selector, probe target,
webview recovery, TLS validation). Post-tag main commits are clippy/fmt
only. The published Setup's sha256 matched before install.

**Verification of the INSTALLED package (Nebulae 0.2.2 from
Nebulae-Setup-x64.exe, sha256 012acb90…38, silent install, real install
path %LOCALAPPDATA%\Nebulae):**

- The app spawned its bundled daemon AND the keepalive client (the fix
  is live in the shipped build).
- The daemon-level chain (the same path the app's probe uses): backend
  Running; ADC peer asiandvdclub 100.64.0.1 online with route
  45.135.163.139/32; HTTPS via the daemon's SOCKS to
  https://asiandvdclub.org = 200, curl ssl_verify_result=0 (certificate
  VERIFIED), ~190ms; control health via mesh = 200 verified.
- **30+ minutes continuous: ZERO disconnects, ONE login event** (the
  journal, 23:52:24Z launch → 00:22+), probes passing throughout.
- Mid-cycle hard-kill → the orphan daemon NEVER lost its stream (the
  keepalive held it through the app's death — the fix working as
  designed) → the relaunched app adopted the SAME daemon pid → the mesh
  restored.
- Known minor defect found and FIXED on main (5d3a555, CI
  39345213085): the hard-killed app's keepalive client orphans, and the
  next app instance spawned a second one. Now adopted
  (ownership-checked) instead of duplicated; adopted clients are
  cleaned up on shutdown. Queued for the next versioned beta — the
  published beta is functionally unaffected (both clients hold the
  backend; connectivity verified perfect throughout).

## RESULT: END-TO-END SUCCESS on build 243084c (the standalone acceptance)

`243084c82cce24ba99df0874cdaa5627e9487722` (CI 39316981556 ✓, all jobs)
is the final tested SHA; the desktop package `nebtestF` = the CI artifact
`nebulae-tailscale-test-243084c82cce24ba99df0874cdaa5627e9487722-win64`
(full SHA match). Windows private ADC connectivity is demonstrated
end-to-end and stable:

- **Probe (all 4 tiers TRUE)**: daemon Running; ADC peer 100.64.0.1
  advertising route 45.135.163.139; routed TCP through the daemon's
  SOCKS to 45.135.163.139:443 in 230ms; real HTTPS 200 from
  asiandvdclub.org with FULL certificate validation (tls_verified=true).
- **Stability**: one connection held >680s (the journal shows ZERO
  disconnects across the whole window; probes at ~170s/~430s/~680s all
  four tiers true).
- **Reproducibility**: hard-kill → orphan → relaunch → ADOPTION (same
  daemon pid) → the state machine settled at Running again → all four
  tiers TRUE on the second connection.
- **Lifecycle on the final build**: graceful quit after adoption stopped
  the app AND the adopted daemon; the identity state file preserved
  (node 3, machine key [DCmIu], node key [pa2YU], no re-enrollment).
- **Status truthfulness**: at every step (including all the failures
  below) the probe matched the control plane's journal exactly and never
  reported success over a failed lower tier.

## THE TWO ROOT CAUSES (both fixed in code)

1. **Windows backend teardown on client disconnect** (ff12578): the
   Windows dev-mode daemon tears its ipn backend down when the last
   client disconnects, and every CLI connection forces a full profile
   switch + control-client rebuild ("profile data directory: profile
   not found" at startup — the current-profile pointer resolves only on
   a CLI connect). The app's status poll = one CLI connection per poll =
   a perpetual teardown/relogin storm; the control stream died every
   10-310s. THE MAC IS IMMUNE (verified: 30 CLI polls in 150s, zero
   rebuilds, state held Running). FIX: the app's supervisor holds ONE
   long-lived `debug daemon-logs` client per daemon (spawned after
   readiness, adopted immediately, respawned if it dies, stopped on
   shutdown), so the backend always has a connected client.
2. **The probe/prefer-peer selector** (6eed154 + 243084c): the tailnet's
   MagicDNS suffix is asiandvdclub.org, so every peer's DNSName contains
   that string and the name-only selector matched an arbitrary client
   peer; and probe_adc_mesh_tcp still dialed the tailnet IP where
   nothing listens. FIX: prefer the peer advertising the subnet route
   (the ADC server), and dial the advertised route IP.

## What was ruled out along the way (proven NOT the cause)

LSOv2 offloads (approved experiment: the deaths persisted with LSOv2
disabled; values restored to Enabled and verified), the UPnP portmapper
endpoint churn (disabling it stopped the churn but not the storm), MTU,
firewall/AV (Defender-only, no drops logged), system proxy, DNS,
routing/teaming, elevation, supervisor churn. The ixgbt 10G driver
shows occasional link events (~22min apart) — a secondary signal, not
the storm's driver.

## Remaining limitations (documented, not blockers)

- The Windows daemon's per-connection profile-switch + last-client
  teardown is an upstream tailscale behavior (1.104.1 dev-mode); our
  keepalive client works around it inside the app. Worth an upstream
  report with the stderr evidence in this file's history.
- `TS_DEBUG_DISABLE_PORTMAPPER` was tested as a diagnosis only; NOT
  shipped (the endpoint churn was not the connectivity blocker once the
  keepalive holds the backend).
- The mac evidence: the same binary holds Running for 4+ hours with a
  single login and passed the identical private-path checks (routed TCP
  + HTTPS 200 + full TLS) in earlier acceptance runs.
- Retention: headscale 0.29.5's long-offline GC applies
  `node.ephemeral.inactivity_timeout` to ALL nodes (the interim 8760h
  stands; the permanent policy = owner decision). LSOv2 values restored.
- Stale test nodes (2/4/5) remain on the server per the earlier
  directive; the desktop's MTUs stay 1500.

## Environment release states

- **Windows desktop (supermicro)**: 0 app/daemon/webview processes;
  scheduled task deleted; identity + `ts-state-backup` preserved;
  package `nebtestF` (243084c) on disk. LSOv2 = Enabled (restored).
- **ADC server**: healthy; no server-policy changes; debug logging
  restored earlier.
- **mac test client**: daemon stopped.

## Lifecycle acceptance detail (cycles 1-3 on 262a502, cycle 4 on 243084c)

| Cycle | Fresh spawn (app/daemon) | Orphan daemon survived | Adopted pid | One app + one daemon | Respawn loop | Graceful quit after adoption |
|---|---|---|---|---|---|---|
| 1 (earlier) | — / 2460 | 2460 ✓ | 2460 ✓ | ✓ | none ✓ | ✓ daemon stopped |
| 2 | 5584 / 4612 | 4612 ✓ | 4612 ✓ | ✓ | none ✓ | ✓ daemon stopped |
| 3 | 18256 / 14636 | 14636 ✓ | 14636 ✓ | ✓ | none ✓ | ✓ daemon stopped |
| 4 (final) | 3756 / 8136 | 8136 ✓ | 8136 ✓ | ✓ | none ✓ | ✓ daemon stopped |

- No WebView profile deletion was ever needed (recovery is
  ownership-scoped; the profile on disk was never touched).
- Identity preserved across every cycle: node 3 = 100.64.0.3, machine
  key [DCmIu], node key [pa2YU]; no re-enrollment, no node churn.

---

# Historical checkpoint (superseded by the RESULT above)

## FINAL LIFECYCLE ACCEPTANCE (completed; build 262a502 on the desktop)

The 262a502→26ff30e delta is log-gating only (14+/4- in one file; no
functional change), so the deployed 262a502 package remained the final
test build — no redeploy required.

Three full hard-kill/relaunch/adoption/graceful-quit cycles, ALL PASS:

| Cycle | Fresh spawn (app/daemon) | Orphan daemon survived | Adopted pid | One app + one daemon | Respawn loop | Graceful quit after adoption |
|---|---|---|---|---|---|---|
| 1 (earlier) | — / 2460 | 2460 ✓ | 2460 ✓ | ✓ | none ✓ | ✓ daemon stopped |
| 2 | 5584 / 4612 | 4612 ✓ | 4612 ✓ | ✓ | none ✓ | ✓ daemon stopped |
| 3 | 18256 / 14636 | 14636 ✓ | 14636 ✓ | ✓ | none ✓ | ✓ daemon stopped |

- No WebView profile deletion was ever needed (recovery is
  ownership-scoped; the profile on disk was never touched).
- Identity preserved across every cycle: node 3 = 100.64.0.3, machine
  key [DCmIu], node key [pa2YU]; no re-enrollment, no node churn.
- Known cosmetic (fixed in 26ff30e, not in the deployed build): the
  "adopting already-running daemon" line repeats per supervisor tick
  (8 lines ≈ 40s of supervision). Behavior is unaffected.

## Deliverables (CI-green)

| SHA (full) | What | CI run |
|---|---|---|
| `262a5023ab78619dbb67738eda38a05eed3ec274` | recovery + 4-tier probe + cert fix (deployed as desktop package `nebtestC`) | 39268460544 ✓ (incl. `test-package-windows`, `windows-pipe-smoke`) |
| `26ff30e8749de2c8e7c0ac54f33e124ed9d633ab` | branch head: adoption logged once per orphan | 39270992651 ✓ |

The deployed desktop package was the CI artifact
`nebulae-tailscale-test-262a5023ab78619dbb67738eda38a05eed3ec274-win64`
(full SHA matches `git rev-parse 262a502`; the tested build is the
deployed build).

## Acceptance results

1. **Hard-kill → relaunch adoption: 1 of 3 planned iterations executed.**
   App hard-killed at 14:37:59Z; daemon pid 2460 survived as orphan;
   relaunch adopted the SAME pid 2460 (single app process 5420, no
   respawn loop, single resume). Iterations 2-3 were not run before this
   checkpoint (mechanism proven once; repeats pending).
2. **Graceful quit after adoption: PASSED.** `quit_app` via CDP; app
   exited; the ADOPTED daemon (2460, no child handle) was stopped by the
   new ownership-checked shutdown path ("ADOPTED DAEMON STOPPED").
3. **Fresh relaunch after graceful quit: clean spawn** (app 3304, new
   daemon 15180) — no interference from the previous cycle.
4. **Fail-closed probe: honest in every observed state.** Three live
   reads during a dead-stream window: `daemon_ready=true`,
   `peer_available=false`, tiers 3-4 false, exact reason surfaced
   ("ADC node is not in the tailnet map…"). CI unit test pins the
   fail-closed structure (no tier true over a failed lower tier;
   aliases `mesh_tcp_ok`/`https_public_name_ok` mirror tiers 3/4).

## Windows control-stream failure — CONFIRMED OBSERVATIONS (unresolved)

- The desktop node's control stream dies 10-310s after each connect
  (observed lifetimes: 11s, 180s, 310s). The client logs NOTHING at the
  death (stays nominally up, re-logins after backoff); the server FINs
  the stream when its keepalive write fails. Headscale's "node has
  disconnected" log lags ~10s (its reconnect-grace loop) — earlier "10s
  lifetime" readings were log latency, not the real lifetime.
- Exonerated read-only on the desktop host: third-party AV/EDR (Windows
  Defender only), Windows Firewall (outbound Allow, no per-program
  rules, no logged drops), system proxy (WinHTTP/WinINET/WPAD), DNS
  (resolves the origin IP), routing (single gateway, no teaming/weak-host),
  MTU 1500 (no behavior change), elevation, supervisor churn (one app,
  one daemon).
- Control group: the mac test client (same 1.104.1 binary, same
  userspace-networking flags, same server, same LAN) held its stream
  4+ HOURS across a server restart.
- Remaining suspects (require owner-side change or on-host capture):
  1. **LSOv2 enabled** on "OnBoard LAN 2" (Large Send Offload V2 IPv4+
     IPv6) — known to stall long-lived idle TCP on some Intel NICs.
     PROPOSED (not applied; a NIC change per directive): disable LSOv2,
     rollback = re-enable.
  2. Switch/router path for host 10.0.0.4 (conntrack/per-port handling).
  3. NIC driver-level pathology. Client-side pktmon was inconclusive
     (counter-only output; could not show whether the client stack SEES
     the server FIN or the path drops silently).
- State at checkpoint: node offline since 14:40:13Z; daemon up but
  NoState (login backoff); identity intact (machine key [DCmIu], node
  key [pa2YU]).

## Retention root cause (investigation result)

Headscale 0.29.5's "cleanup of long-offline nodes" applies
`node.ephemeral.inactivity_timeout` (default 900000 ms = 15 min) to ALL
nodes REGARDLESS of the Ephemeral flag. Journal evidence:
`cleaning up node that has been offline for too long node.id=2
offline_duration=900000` (node 2 was Ephemeral=false; node 3 hit the
same path at 00:13:00Z). The interim `8760h` setting (backed up as
`config.yaml.pre-ephemeral-fix`) therefore protects every node for a
year. Permanent policy = owner decision (keep 8760h / pin a headscale
version that scopes the GC to ephemeral nodes / accept GC of dead
nodes).

## TLS truthfulness

`ssl_verify=0` is curl's `%{ssl_verify_result}` SUCCESS code (0 =
certificate verified), not a bypass. Additionally, the tracker client's
`danger_accept_invalid_certs(true)` was REMOVED in 262a502 — full
certificate/hostname validation everywhere (fail-closed).

## Environment release states

- **Windows desktop (supermicro)**: 0 app/daemon/webview processes;
  scheduled task `NebulaeTestRun` deleted; enrolled identity
  (`tailscaled.state`) and `ts-state-backup` preserved; NIC MTUs remain
  1500. Package `nebtestC` (262a502) remains on disk for Antigravity.
- **ADC server (control.asiandvdclub.org)**: healthy; debug logging was
  enabled for one bounded capture and RESTORED (config back to
  `config.yaml.pre-debug`, service active); no other server changes.
- **mac test client**: node 5 daemon stopped.

## NEXT EXPERIMENT (LSOv2) — EXECUTED, HYPOTHESIS REFUTED

With owner approval, LSOv2 (IPv4+IPv6) was disabled on "OnBoard LAN 2"
only (RegistryKeywords `*LsoV2IPv4`/`*LsoV2IPv6` → 0). Result: the
short-lifetime deaths PERSISTED (connect 18:57:26Z → disconnect
18:57:37Z, 11s) and the login-cycle pattern continued. LSOv2 is NOT the
cause. The values were RESTORED to Enabled (both verified) the same
session.

## CONTROL-STREAM ROOT CAUSE (evidence from the follow-up capture)

A client-side daemon-log capture across a cycle boundary shows the
daemon's ipn state machine FULL-CYCLING every ~40-70s:
NoState → Starting → Running → NoState → … with one doLogin per cycle,
while the process (and machine key [DCmIu], node key [pa2YU]) stays
stable. The cycle boundary carries:

    health(warnable=login-state): error: You are logged out. The last
    login error was: fetch control key: Get
    "https://control.asiandvdclub.org/key?v=148": context canceled

i.e. a SELF-CANCELING LOGIN RETRY STORM: a pending control-key fetch is
canceled by the next login attempt; the recorded error marks the client
logged-out; that error triggers the next attempt. The same binary on
the mac does none of this (single login, stream holds for hours).

Additional experiment the same session: the daemon was relaunched with
TS_DEBUG_DISABLE_PORTMAPPER=1 (manually, then ADOPTED by the app —
adoption verified). The UDP endpoint churn STOPPED (0 "endpoints
changed" in 150s vs constant churn before) but the LOGIN STORM
CONTINUED. So there are TWO independent client-side behaviors:
(a) UPnP portmapper endpoint churn (stopped by the debug env var; a
debug knob, NOT a supported remedy), and (b) the login retry storm
(still open).

### Remaining precise hypothesis + needed access

The storm's fuel is the control-key fetch (GET /key) being slow enough
on this Windows host to be canceled by the client's own retry. The
discriminating test is a packet-level timeline of ONE login cycle on
the desktop (DNS + TCP + TLS to 45.135.163.140:443, /key fetch latency)
compared with the mac. Windows pktmon on this host yields counters, not
packet timelines; a real capture needs `netsh trace`/Wireshark on the
desktop, which needs an owner-approved tool/install. Everything else is
exonerated; the switch/router path remains a fallback suspect only if
the capture shows the host's packets are fine.


# Tailscale-core handoff — checkpoint 2026-10-19 (branch head 26ff30e)

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

## NEXT EXPERIMENT (requires owner approval before execution)

Single most discriminating test for the control-stream death:

- **Target**: ONLY the "OnBoard LAN 2" adapter on the Windows desktop
  (10.0.0.4). Nothing else changes — no MTU, firewall, AV, DNS, routing.
- **Change**: `Set-NetAdapterAdvancedProperty -Name 'OnBoard LAN 2' -DisplayName 'Large Send Offload V2 (IPv4)' -DisplayValue Disabled` and
  the same for `'Large Send Offload V2 (IPv6)'`.
- **Hypothesis**: LSOv2 on this Intel NIC is stalling ACK delivery on
  long-lived idle TCP flows, which is why every outbound connection of
  the embedded daemon dies silently in 10-310s while the identical mac
  client on the same LAN holds for hours.
- **Expected if the hypothesis is right**: headscale's journal shows
  node 3 connect with NO following disconnect for >600s, and the app
  holds `Running` continuously across multiple probe reads.
- **Rollback** (exact): set both properties back to `Enabled`
  (current values, recorded here for restore).
- **Disruption**: ~3-5s NIC reset when the property applies (any traffic
  on that adapter blips); offload removed may slightly raise CPU and
  lower throughput on that host. Scoped to the one interface.

If the stream STILL dies with LSOv2 disabled, the remaining suspect is
the switch/router path for host 10.0.0.4, and the investigation hands
off to whoever owns that network path.

# Tailscale-core handoff — checkpoint 2026-10-19 (branch head 26ff30e)

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

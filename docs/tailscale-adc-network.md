# Embedded Tailscale ("ADC Network") — Client Behavior

## What ships
`resources/engine/tailscale/{tailscaled, tailscale}` (per-platform, pinned
1.104.1). The daemon runs ONLY in userspace mode (`--tun=userspace-networking`,
`--port=0`), with ALL state under `Nebulae/internal/tailscale/`:
node identity, the CLI socket, and nothing in system locations. An existing
system Tailscale install is never touched, configured, or unconfigured; the
CLI always passes `--socket <our socket>`.

## Enrollment (one-time per machine)
- Headscale endpoint (default, no Tailscale Inc. account):
  `tailscale up --authkey <SINGLE-USE key from the ADC site> --login-server
  https://control.asiandvdclub.org --hostname nebulae-<hostname>`
- Own-account alternative: same call without `--login-server`.
- State persists in the state dir: app restarts come back Running without
  re-enrollment (verified locally against a live Headscale).

## What traffic uses it (truthful scope)
- App HTTPS to ADC can be routed via the daemon's local SOCKS5 proxy
  (127.0.0.1:1055) — `adc_private_probe` does a real HTTPS request there and
  reports reachable only on a real response.
- Torrent peer transport over the tailnet is NOT supported yet. Userspace
  networking is not a bindable OS tunnel; binding Transmission to the
  tailnet IP requires a TUN-based daemon (root/admin) and must be proven
  with a real two-member transfer before being claimed. The bind-decision
  code in the engine is UNCHANGED in this branch.

## Failure behavior
- Daemon not running / not enrolled: status reports it; probe reports
  `via: unavailable`, never fake success.
- If a restricted VPN mode is selected by the user, that mode's rules win;
  the ADC-network layer never overrides or bypasses them.

## Commands (Tauri)
- `tailscale_status` — daemon + backend state + tailnet IP + control server + ADC node seen
- `tailscale_enroll(auth_key, control_url?)` — single-use key enrollment
- `tailscale_disconnect` — `tailscale logout`
- `adc_private_probe` — real HTTPS via SOCKS5 through the embedded daemon

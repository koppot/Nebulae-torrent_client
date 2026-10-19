# ADC Network Server Setup — Headscale Control Plane (DRAFT — awaiting deployment approval)

This document describes changes to the **ADC Linux server** required by
Nebulae's embedded Tailscale ("ADC Network") mode. **Nothing here has been
executed against production.** Deployment requires explicit owner approval.

## What gets installed
| Component | Version | Purpose |
|---|---|---|
| headscale | pinned (see scripts/adc-headscale-setup.sh) | Open-source Tailscale control plane — members enroll WITHOUT Tailscale Inc. accounts |
| tailscale + tailscaled | same pinned version | The server's own node (`asiandvdclub`), also acts as subnet router |

## Control endpoint + TLS
- New vhost `control.asiandvdclub.org` on the existing reverse proxy, TLS via
  the existing certbot/Let's Encrypt flow, proxying to headscale on
  127.0.0.1:8080.
- `server_url: https://control.asiandvdclub.org`.

## Service
- `headscale.service` (systemd, Restart=always), state in /var/lib/headscale
  (SQLite), in the existing backup job.
- Server-side `tailscaled`: `tailscale up --login-server
  https://control.asiandvdclub.org --hostname=asiandvdclub
  --advertise-routes=<server public service IPs>` — routes approved in
  headscale so members reach ADC services with valid TLS (SNI stays
  asiandvdclub.org).

## DERP (NAT traversal fallback)
- Members behind hard NATs need a DERP relay. Start with Tailscale's public
  DERP map (`derp.urls` default); if privacy requires it later, run a private
  DERP server on the ADC box and point `derp.server_enabled: true` +
  custom DERP map. Decision NOT required for day one.

## Private admin interface
- headscale CLI over its unix socket, root/admin only (socket dir
  /var/run/headscale, 0700). Web admin UI deliberately not exposed.
- Never expose the gRPC listener publicly.

## Enrollment (short-lived, single-use)
- Initial implementation: SINGLE-USE keys: `headscale preauthkeys create
  --user club --expiration 30m` (no --reusable). One key = one device.
  Key delivered to the member through the ADC site (logged-in download).
- No shared long-lived club key is ever created or embedded in the client.
- Rotation: keys expire on their own; stale nodes pruned via
  `headscale nodes` (monthly cron: expire + delete offline >90d).

## Access rules (ACL)
- member -> adc node : tcp/443
- member -> member   : tcp/41413 (BitTorrent peering over the tailnet —
  client support for peering is a SEPARATE follow-up; see handoff notes)
- member -> server ssh/lan : deny
- Write as /etc/headscale/acl.hujson; `headscale policy` reload.

## Backups
- /var/lib/headscale (device registry + keys) added to existing backup job.

## Firewall / DNS impact
- ONE new inbound: HTTPS for control.<host>. Member WireGuard traffic is
  peer-to-peer (or via DERP) and needs no new server ports.
- No production tracker/DNS/membership changes.

## Rollback
- systemctl disable --now headscale; remove vhost. Members' embedded
  daemons simply report "not connected". No client-side data is lost.

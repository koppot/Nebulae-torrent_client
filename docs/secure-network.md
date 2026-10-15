# Nebulae Secure Network — Detailed Guide

Status: rolling out with an upcoming release. Final UI wording may differ
slightly from this document during testing.

## What this feature is

Nebulae can connect your computer to Asian DVD Club's **Secure Network** —
a private, member-only network layer between you and the club's servers
(and, over time, between members). Under the hood it uses Tailscale's
technology, which is built on WireGuard, one of the most trusted encryption
protocols on the internet.

You do not need a Tailscale account, you never visit the Tailscale website,
and you never see a command line. Enrollment happens inside Nebulae with
one click.

## Why the club offers it

1. **You become reachable.** Normally, members behind strict routers cannot
   accept incoming connections, which slows downloads. The Secure Network
   creates direct encrypted connections with no router configuration and
   no port forwarding.
2. **Your passkey traffic is protected.** While enrolled, your announce
   (the request that carries your member key) travels through the
   encrypted private network instead of the public internet.
3. **Membership-based access.** Your enrollment is created by your ADC
   account. If a membership ends, network access ends with it — precisely
   and immediately.

## How to enroll

1. Open **Settings → ADC Secure Network** in Nebulae.
2. Click **Enroll**. Nebulae authenticates with your existing ADC
   credentials and receives a one-time, single-use key from the club's
   server. The key is issued specifically for your account.
3. Nebulae installs the network helper on your computer:
   - **Windows:** one permission prompt (UAC). Nebulae runs the installer
     for you.
   - **macOS:** one permission prompt, plus a one-time "Allow" in System
     Settings for the network extension (macOS requires this for any
     network software; it cannot be skipped).
   - **Linux:** a short package installation.
4. When the status badge shows **Connected**, you are enrolled. The status
   screen shows your Secure Network address (an address in the range
   `100.x.y.z`).

Enrollment is one-time. After the first setup, Nebulae reconnects the
network automatically whenever it starts.

## What gets routed where

The Secure Network is **split-routed**. That means it only carries traffic
that belongs to it:

| Traffic | Route |
|---|---|
| ADC announce and ADC peers on the Secure Network | Encrypted tunnel |
| ADC peers without a Secure Network address | Public internet (as today) |
| Every other tracker, and all other internet traffic | Public internet (untouched) |

Nebulae can run an ADC torrent and any other tracker's torrents at the
same time; only the ADC side uses the tunnel. If the tunnel drops, Nebulae
pauses ADC transfers until the network returns. Other trackers keep
downloading.

## Privacy and security model

- **Your secret keys never leave your computer.** The club's server only
  ever sees the public half of the encryption keys — the same way a padlock
  can be public while the key stays in your pocket.
- **No DHT for club content.** Torrents marked private (all ADC torrents)
  never participate in the public DHT or peer exchange. Public trackers'
  torrents keep normal DHT behavior.
- **Revocation.** Club staff can remove a device's network access
  instantly, for example when a membership ends.
- **Killswitch.** While enrolled, if the Secure Network drops, Nebulae
  pauses ADC transfers rather than letting them fall back to the public
  internet mid-transfer.

## Turning it off

Settings → ADC Secure Network → **Disconnect**. Everything in Nebulae
works normally without the Secure Network; it is an optional layer. You
can also leave it disconnected permanently.

## Troubleshooting

- **The permission prompt did not appear.** Reinstall from the Enroll
  screen; Nebulae will re-run the installer.
- **macOS: "System Extension Blocked".** Open System Settings → Privacy &
  Security → click **Allow**, then return to Nebulae.
- **Connected, but slow.** When a direct connection is not possible, traffic
  is relayed (still fully encrypted, just slower). This is uncommon.
- **Status shows Paused after a network change.** The killswitch paused
  ADC transfers because the tunnel dropped. Reconnect and resume from the
  torrent manager.

## Technical appendix (for the curious)

- The coordination service is **Headscale**, an open-source implementation
  of the Tailscale control protocol, operated by the club on its own
  server. It never carries torrent traffic; it only handles enrollment,
  addresses, and key exchange.
- The data plane is standard **WireGuard** between your device and the
  club's server (and, later, between members), with direct connections
  where NAT allows and an encrypted relay fallback where it does not.
- Private torrents are excluded from DHT/PEX/LPD by the engine itself;
  Nebulae additionally refuses to add an ADC-domain torrent that is not
  marked private.
- Enrollment keys are single-use, expire, and are bound to your ADC
  account. The mapping between your account and your device exists only
  on the club's server.

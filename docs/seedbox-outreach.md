# Seedbox Provider Outreach — Preparation Document

> **Status:** DRAFT — Requires user approval before any contact is initiated.
> **Last updated:** 2026-11-17

## 1. Compatible Connection Methods

Nebulae supports three remote engine backends:

| Backend | Protocol | Port | Auth | Notes |
|---|---|---|---|---|
| rTorrent SCGI | XML-RPC over TCP/Unix socket | Configurable (typically 5000) | None / SSH tunnel | Native support, most seedbox-compatible |
| nebulae-server | HTTP REST API | 9174 (default) | Bearer token | Docker-deployable, wraps Transmission |
| Transmission RPC | JSON-RPC over HTTP | 9091 (default) | Username/password | Used by nebulae-server bridge internally |

**Primary compatibility target:** rTorrent via SCGI over SSH tunnel — this matches the most common seedbox configuration.

## 2. Provider Shortlist

Based on established providers offering rTorrent or Transmission with SSH access:

| Provider | Engine | SSH Access | Direct SCGI/RPC | Pricing | Notes |
|---|---|---|---|---|---|
| Feral Hosting | rTorrent | ✓ | SCGI via SSH tunnel | From £10/mo | Well-known, UK-based, SSH standard |
| Whatbox | rTorrent | ✓ | SCGI socket exposed | From $15/mo | Good documentation, US/EU |
| Seedhost.eu | rTorrent | ✓ | SCGI via SSH | From €6/mo | EU-based, budget options |
| Ultra.cc | rTorrent | ✓ | SCGI + ruTorrent | From $5.50/mo | SSH standard, modern panel |
| Seedbox.io | rTorrent | ✓ | SCGI socket | From €5/mo | EU-based |
| Hetzner (dedicated) | User choice | ✓ | Full root access | From €39/mo | Self-managed, Docker compatible |
| OVH/SoYouStart | User choice | ✓ | Full root access | From €16/mo | Self-managed, Docker compatible |

**Note:** Dedicated/VPS providers (Hetzner, OVH) would use the nebulae-server Docker image directly. Shared seedbox providers would use the existing rTorrent SCGI integration.

## 3. Compatibility Checklist

For each provider, verify:

- [ ] SSH access with TCP port forwarding (for SCGI tunnel)
- [ ] rTorrent or Transmission daemon available
- [ ] SCGI socket or TCP port accessible
- [ ] Sufficient permissions to configure torrent client settings
- [ ] Persistent storage for torrent state across reboots
- [ ] API documentation or wiki presence for client integration

## 4. Outreach Email Template

```
Subject: Nebulae Client — Integration Testing Request

Dear [Provider] Team,

Nebulae is a desktop torrent client with built-in support for remote
server management. We have implemented native rTorrent SCGI integration
and are testing compatibility with established seedbox providers.

We would appreciate:

1. A temporary test account (or trial period) to verify our SCGI
   integration with your rTorrent configuration.

2. Any technical documentation about your SCGI socket path, SSH tunnel
   setup, or API access that would help us ensure seamless connectivity.

3. If interested, we would be happy to provide a compatibility page or
   setup guide for your users on our documentation site.

This is a client-side integration — no changes to your infrastructure
are needed. We simply connect to the existing rTorrent instance via
SSH-tunneled SCGI.

Thank you for your time.

Best regards,
[User name]
Nebulae — https://nebulae.asiandvdclub.org
```

## 5. Provider Documentation/Wiki Listing

> **Important distinction:** Provider documentation/wiki pages are maintained by the providers themselves. Any listing of Nebulae as a compatible client on a provider's wiki is at the provider's discretion. This is distinct from public encyclopedia listings (e.g., Wikipedia), which have separate content and sourcing policies. We do NOT create or edit Wikipedia articles.

Requested listing venue clarification from user: provider-maintained documentation and wiki pages only? Or additional venues?

## 6. Approval Gate

**The following actions require explicit user approval:**
- [ ] Sending outreach emails to any provider
- [ ] Creating accounts on any provider platform
- [ ] Submitting content to any external wiki or documentation
- [ ] Any public representation of Nebulae's capabilities to third parties

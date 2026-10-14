# Upstream Issue Draft: `ikatson/rqbit`

**Target Repository**: `ikatson/rqbit` (GitHub Issues)  
**Issue Type**: Feature Request  
**Title**: `Feature Request: Super-Seeding (Initial Seeding / BEP 16) support for ManagedTorrent`

---

## Problem Statement

When a content publisher or community archivist introduces a new, large release (such as a 30–50 GB optical disc preservation ISO or remux), there is initially only a single seed in the entire swarm.

Under standard BitTorrent rarest-first piece sharing, the initial seed frequently sends duplicate pieces to multiple connected leechers before any single leecher completes enough blocks to re-distribute them. For seeders with constrained residential or asymmetric upstream bandwidth (e.g. 20–50 Mbps upload vs. gigabit download), this redundancy severely delays the point at which the swarm becomes self-sustaining (swarm health > 1.0).

## Feature Description: Super-Seeding (BEP 16)

Super-seeding (also known as initial seeding mode) changes the seeder's behavior to maximize piece propagation while minimizing redundant upload:

1. The seed disguises itself as an ordinary peer with 0 pieces initially.
2. When a peer connects, the seed informs the peer of only a single piece that no other peer has received yet (via a synthetic `HAVE` message).
3. The seed will not send that peer another piece until it observes that the previously sent piece has been distributed to at least one other peer in the swarm (observed via `HAVE` announcements from third-party peers).
4. This ensures that every uploaded byte contributes directly to the total availability of the swarm, allowing the swarm to reach 100% completion while uploading approximately 1.05–1.2x the torrent size rather than 2.0–3.0x.

## Proposed API / Architecture Integration

Within `librqbit`:
- Add a configuration flag `super_seeding: bool` on `ManagedTorrentOptions` and `AddTorrentOptions`.
- Expose runtime toggle: `session.toggle_super_seeding(&handle, bool) -> anyhow::Result<()>`.
- Extend the `LivePeerState` / `Peer` message scheduler to track advertised virtual pieces per connected peer.
- Deactivate super-seeding automatically once a secondary complete seed appears in the swarm (or when the user toggles it off).

## Community Context & Willingness to Contribute

This feature is requested on behalf of the [Nebulae](https://github.com/koppot/Nebulae-torrent_client) open-source desktop client (Asian DVD Club community), which embeds `librqbit` as its core transfer engine.

We would be pleased to collaborate on the implementation, assist with protocol testing against real swarms, and provide test vectors or an initial PR if this feature aligns with `librqbit`'s roadmap.

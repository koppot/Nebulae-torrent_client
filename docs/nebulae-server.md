# Nebulae Server — rtorrent remote engine

The Nebulae **desktop application stays exactly as it is**: one native window, no web interface. This branch adds the ability for that same application to drive a torrent engine that runs on a server (Unraid, TrueNAS, Synology, any Docker host) instead of locally.

## Architecture

```
Desktop (Windows / macOS / Linux)              Server (Unraid etc.)
┌────────────────────────────────┐             ┌─────────────────────────────┐
│ Nebulae app — original UI      │             │ rtorrent daemon (GPL)       │
│ TorrentEngine trait ───────────┼── SCGI ────►│   unix socket, local only   │
│ RtorrentRemoteEngine (this)    │   XML-RPC   │ Nebulae bridge (headless)   │
└────────────────────────────────┘             └─────────────────────────────┘
```

- `src-tauri/src/engine/rtorrent/scgi.rs` — minimal SCGI transport (netstring framing over a Unix socket or TCP for testing).
- `src-tauri/src/engine/rtorrent/rpc.rs` — minimal XML-RPC layer for rtorrent's `d.*` command set (method building + response parsing + fault handling).
- `src-tauri/src/engine/rtorrent/mod.rs` — `RtorrentRemoteEngine`, implementing the shared `TorrentEngine` trait.

Licensing: rtorrent (GPL-2.0) is a **separate process** in its own container. Aggregation, not linking — the MIT Nebulae code never links GPL code. The same model every seedbox image uses.

## Implemented over RPC (this branch)

- `list_torrents` via `d.multicall2` (name, size, progress, rates, ratio per-mille conversion, seed/peer counts, state mapping to Nebulae statuses)
- `add_torrent` (magnet / URL via `load.start`, local `.torrent` via `load.raw_start`)
- `remove_torrent` (erase only — data deletion over RPC is not possible and errors honestly)
- pause / resume / pause_all / resume_all (`d.stop` / `d.start`)
- kill switch: Nebulae-owned flag + stop-all enforcement (rtorrent has no native switch)
- `force_recheck` (`d.check_hash`), `set_location` (`d.directory.set`), download/upload throttles (`d.throttle.*.set`)

## Not wired yet (honest preview errors, no silent stubs)

- `reannounce`, per-file priority, file/folder rename (server-side filesystem), categories (no rtorrent label mapping), sequential download, first/last piece priority, super-seeding (rtorrent has none)
- Torrent `files`/`trackers` details and ratio-projection inputs require per-torrent `d.f.*`/`t.*` multicalls — next milestone
- Settings UI for connection profiles (unix socket path / TCP target), engine picker (local librqbit vs rtorrent server)

## Unraid packaging (planned, this branch)

`server/docker/` will ship a container with rtorrent + the SCGI socket, exposed only through Nebulae's authenticated API; a Community Applications template follows. The desktop app never talks to SCGI directly over the network.

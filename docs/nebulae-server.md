# Nebulae Server — rtorrent remote engine (beta)

The Nebulae **desktop application stays exactly as it is**: one native window, no web interface. Nebulae Server lets that same application drive a torrent engine that runs on a server (Unraid, TrueNAS, Synology, any Docker host) instead of locally.

## Architecture

```
Desktop (Windows / macOS / Linux)                Server (Unraid etc.)
┌──────────────────────────────────┐             ┌──────────────────────────────┐
│ Nebulae app — original UI        │             │ rtorrent daemon (GPL)        │
│ TorrentEngine trait ─────────────┼── HTTP+token│   SCGI unix socket, local    │
│ RemoteServerEngine ──────────────┼────────────►│ nebulae-server bridge        │
└──────────────────────────────────┘  (v1 JSON)  │   (headless, no web UI)      │
                                                 └──────────────────────────────┘
```

The desktop app never talks to SCGI over the network. It talks to the bridge's small authenticated HTTP API; the bridge lives in the same container as rtorrent and reaches rtorrent over a local Unix socket.

Licensing: rtorrent (GPL-2.0) is a **separate process** in its own container. Aggregation, not linking — the MIT Nebulae code never links GPL code. The same model every seedbox image uses.

## Deploy with Docker (Unraid / any Linux host)

The container image `ghcr.io/koppot/nebulae-server:beta` contains rtorrent 0.9.8 + the headless bridge.

```bash
# docker compose (see server/docker-compose.yml)
docker compose up -d
```

Or plain `docker run`:

```bash
docker run -d --name nebulae-server \
  -p 9174:9174 -p 6881:6881 -p 6881:6881/udp \
  -v /mnt/user/appdata/nebulae-server:/data \
  -e NEBULAE_SERVER_TOKEN='<long random string>' \
  ghcr.io/koppot/nebulae-server:beta
```

### Unraid (Community Applications)

See the step-by-step end-user guide: **[docs/unraid-install.md](unraid-install.md)**.

In short: install the `nebulae-server` template (`server/nebulae-server-unraid.xml`). Set:
- **API Token** — a long random string; the desktop app must present it
- **Data directory** — where downloads, session state and the watch folder live

### Connecting the desktop app

1. Nebulae desktop → Settings → **Server**
2. Enable *Drive torrents on a Nebulae Server (rtorrent)*
3. Enter `http://<server-ip>:9174` and the API token, click **Test connection**
4. Restart Nebulae — the engine choice applies on start; the UI is unchanged

Switch back any time: untick the box and restart.

## The bridge API (v1)

Bearer-token authenticated (401 otherwise). Plain HTTP in this beta — keep it on the trusted LAN or put a TLS-terminating reverse proxy in front.

| Method & path | Purpose |
|---|---|
| `GET /api/v1/status` | name, version, engine, torrent counts, kill-switch state |
| `GET /api/v1/torrents` | full torrent list incl. per-torrent files + trackers (same `TorrentItem` JSON as the desktop app) |
| `POST /api/v1/torrents` | add — body `{"source": "magnet:…"}` or `{"source_b64": "…"}` for uploaded `.torrent` bytes, optional `download_dir` (applied at load time via rtorrent keylines) |
| `DELETE /api/v1/torrents/{id}?delete_files=` | remove (erase only; data deletion is refused honestly) |
| `POST /api/v1/torrents/{id}/pause` / `…/resume` / `…/recheck` / `…/reannounce` | per-torrent control |
| `POST /api/v1/torrents/pause-all` / `…/resume-all` | bulk control |
| `POST /api/v1/torrents/{id}/limits` | `{"dl_kib":0,"ul_kib":0,"ratio_limit":-1}` — named throttle groups (rtorrent 0.9.x; honest error on 0.16+) |
| `POST /api/v1/torrents/{id}/location` | `{"path": "/data/downloads/…"}` |
| `POST /api/v1/torrents/{id}/category` | `{"category": "…"}` — rtorrent `custom1` label (ruTorrent-compatible) |
| `POST /api/v1/torrents/{id}/firstlast` | toggle first/last-piece priority (rtorrent `file.prioritize_toc`) |
| `POST /api/v1/torrents/{id}/file-priority` | `{"file_index":0,"priority":2}` (rtorrent 0.9.x; 0.16.25 currently faults upstream) |
| `POST /api/v1/kill-switch` · `GET /api/v1/kill-switch` | `{"enable":true|false}` / current state |

## RPC compatibility (verified against a live rtorrent 0.16.25)

The engine probes `system.client_version` once and gates call shapes:
- **rtorrent 0.16+**: `d.multicall2` takes `(target, view, columns…)`; `f.multicall` takes a filter argument; bare `t.*`/`f.*` command names; SCGI responses carry HTTP-style headers (stripped by the transport).
- **rtorrent 0.9.x** (the container image ships 0.9.8): classic `(view, columns…)` / `t.get_*` forms.

Verified working end-to-end over the bridge: list with live categories + file + tracker details, add via magnet (with per-add `download_dir`), `.torrent` upload as XML-RPC `<base64>`, erase, pause/resume (single + all), kill switch, recheck, reannounce, category, first/last-piece priority, location change.

Known limitations (each fails loudly, never silently):
- per-torrent rate limits: rtorrent 0.16 removed the throttle-group RPC commands — works on 0.9.x (container default), honest error on 0.16+
- per-file priority: rtorrent 0.16.25 faults the `(hash, index, value)` target shape upstream — works on 0.9.x, fault passes through on 0.16+
- `ratio_limit` is accepted but not applied over rtorrent yet (view-based ratio scheduling is future work)
- sequential download and super-seeding have no rtorrent equivalent
- renames require server-side filesystem access and are unavailable in remote mode

## Building from source

```bash
# frontend is embedded by the Tauri build
pnpm install && pnpm build
cd src-tauri && cargo build --release --bin nebulae-server
./target/release/nebulae-server --engine rtorrent --scgi /run/rtorrent/scgi.sock --listen 0.0.0.0:9174 --token '…'
```

CLI: `--engine <rtorrent|mock>` (mock is for development only), `--scgi <PATH>` or `--scgi-tcp <HOST:PORT>`, `--listen <ADDR:PORT>` (default `0.0.0.0:9174`), `--token <TOKEN>` (falls back to `$NEBULAE_SERVER_TOKEN`, else generates a one-time token and prints it).

For a systemd unit, start rtorrent under `dtach` (or your service manager) and then the bridge against its socket — the container entrypoint (`server/entrypoint.sh`) shows the exact sequence.

## Release automation

Tag `nebulae-server-v0.1.0-beta.1` (or any `nebulae-server-v*`) triggers `.github/workflows/server-release.yml`: builds the Linux x86_64 binary, smoke-tests the API (401 without token, status shape with token), attaches it to a GitHub release, and pushes the container image to GHCR (`:beta` + `:v<version>` tags).

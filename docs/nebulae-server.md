# Nebulae Server mode (`nebulae-server`)

The Nebulae desktop app can drive a headless torrent engine running on a remote host or NAS instead of locally:

1. **Settings → Server**
2. Enable *Drive torrents on a Nebulae Server instead of locally*
3. Enter the server address (`http://<server-ip>:9174`) and the bearer API token (`NEBULAE_TOKEN`)
4. Click **Test connection**, then restart the app

## Architecture & Bridge Repositories

- **In-tree bridge (`server/`)**: This repository includes the `nebulae-server` crate (`server/`), an Actix-web REST bridge that translates authenticated `/api/v1/*` requests to a local **Transmission** JSON-RPC daemon (`TRANSMISSION_RPC_URL`, default `http://127.0.0.1:9091/transmission/rpc`).
- **Standalone deployment repository**: Container packaging, Docker Compose/Unraid templates, and release automation are maintained in **<https://github.com/koppot/nebulae-server>**.

## Supported Bridge API Endpoints (`/api/v1/*`)

All endpoints require `Authorization: Bearer <NEBULAE_TOKEN>`:

- `GET /api/v1/status` — Engine identity, `active_torrents`, `paused_torrents`, `kill_switch_engaged`, `bound_interface`
- `GET /api/v1/torrents` — List torrents with files, trackers, limits, and labels
- `POST /api/v1/torrents` — Add torrent by `{ source, download_dir }` (magnet/URL) or `{ source_b64, download_dir }` (base64 `.torrent`)
- `POST /api/v1/torrents/{id}/pause` / `resume` — Pause or resume a torrent
- `DELETE /api/v1/torrents/{id}?delete_files=true|false` — Remove torrent
- `POST /api/v1/torrents/{id}/reannounce` / `recheck` — Force tracker announce or piece verification
- `POST /api/v1/torrents/{id}/limits` — Set per-torrent `dl_kib`, `ul_kib`, and `ratio_limit`
- `POST /api/v1/torrents/{id}/location` — Move torrent storage (`{ path }`)
- `POST /api/v1/torrents/{id}/category` — Set category label (`{ category }`)
- `POST /api/v1/torrents/{id}/file-priority` — Set per-file priority (`{ file_index, priority }`)
- `POST /api/v1/torrents/{id}/sequential` — Toggle sequential download
- `POST /api/v1/torrents/pause-all` / `resume-all` — Pause or resume all torrents
- `POST /api/v1/kill-switch` — Engage or disengage kill switch (`{ engaged }` or `{ enable }`)

## Docker Deployment

Build and run from the repository root:

```bash
export NEBULAE_TOKEN="your-secret-token"
docker compose -f server/docker/docker-compose.yml up -d
```

Or build manually:

```bash
docker build -f server/docker/Dockerfile -t nebulae-server .
docker run -d \
  -e NEBULAE_TOKEN="your-secret-token" \
  -p 9174:9174 -p 51413:51413 -p 51413:51413/udp \
  -v nebulae-downloads:/data/downloads \
  -v nebulae-config:/data/config \
  nebulae-server
```

Volumes:

| Volume | Path in container | Purpose |
|---|---|---|
| `nebulae-downloads` | `/data/downloads` | Completed and in-progress downloads |
| `nebulae-config` | `/data/config` | Transmission `settings.json` and resume files |
| `nebulae-watch` | `/data/watch` | Drop `.torrent` files here to auto-add |

---

## Migrating from rTorrent (SCGI) to Transmission (nebulae-server)

Previous Nebulae releases supported rTorrent over SCGI as a remote engine. The current release replaces that path with `nebulae-server` backed by Transmission. The client Settings UI fields are unchanged (URL and token); only the server side changes.

### Key differences

| Behaviour | rTorrent / SCGI | nebulae-server / Transmission |
|---|---|---|
| Protocol | XML-RPC over SCGI | JSON-RPC (`POST /transmission/rpc`) via authenticated REST bridge |
| Auth | None / SSH tunnel | Bearer token (`NEBULAE_TOKEN`) |
| Sequential download | Not supported | Supported — `torrent-set sequentialDownload` |
| File priority values | 0 / 1 / 2 | 0 (skip) / 1 (normal) / 2 (high) — same semantics |
| Category/label | XMLRPC custom tag | Transmission label field |
| Peer port | Configured in rTorrent config | `peer-port` in `settings.json` or `PEER_PORT` env var |

### Migration steps

1. **Export active torrents from rTorrent.**  
   The simplest path is to copy the `.torrent` files and resume data from your rTorrent session directory. Transmission reads standard `.torrent` files but does not import rTorrent's `.rtorrent` resume metadata — torrents will recheck on first start.

2. **Install and start `nebulae-server`** using the Docker steps above.

3. **Place your `.torrent` files in `/data/watch`** (or add them through Nebulae after connecting). Transmission will begin rechecking them against the files in `/data/downloads`.

4. **In Nebulae Settings → Server:**
   - Disable any existing rTorrent SCGI URL.
   - Enable *Drive torrents on a Nebulae Server instead of locally*.
   - Enter `http://<server-ip>:9174` and your `NEBULAE_TOKEN`.
   - Click **Test connection**, then restart the app.

5. **Peer port:** Open port `51413` (TCP + UDP) on your firewall/router, or set `PEER_PORT` to your preferred port in the Docker environment and update your firewall accordingly.

### Not supported after migration

- rTorrent-specific per-tracker scrape intervals and custom XMLRPC commands are not exposed by `nebulae-server`.
- Super-seeding toggle — Transmission does not support super-seeding; the UI control returns an error when connected to a remote server.

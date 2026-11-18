# Nebulae Server mode (`nebulae-server`)

The Nebulae desktop app can drive a headless torrent engine running on a remote host or NAS instead of locally:

1. **Settings → Server**
2. Enable *Connect to a Nebulae Server (Transmission)*
3. Enter the server address (`http://<server-ip>:9174`) and the bearer API token (`NEBULAE_TOKEN`)
4. Click **Test connection**. A **Server** tab appears next to Manager; no restart is needed.

The **Manager** tab always shows this computer's own torrents. The **Server** tab shows the same interface bound to the server, with a connection banner above it. Adding a torrent there uses the server's default download folder (or a server path you type), never a folder from this computer.

## Architecture & Bridge Repositories

- **In-tree bridge (`server/`)**: This repository includes the `nebulae-server` crate (`server/`), an Actix-web REST bridge that translates authenticated `/api/v1/*` requests to a local **Transmission** JSON-RPC daemon (`TRANSMISSION_RPC_URL`, default `http://127.0.0.1:9091/transmission/rpc`).
- **Standalone distribution repository**: <https://github.com/koppot/nebulae-server> remains as the Unraid-template/distribution repository; the Docker sources for 0.2.8 live in `server/docker/` here (see *Docker Deployment*).

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

### Source of truth

The authoritative 0.2.8 source for the server image is **this repository**: `server/` (Rust bridge) and `server/docker/` (Dockerfile, `entrypoint.sh`, `docker-compose.yml`, `.env.example`, and `transmission-4.1.3-identity-override.patch`). The image bundles stock Transmission **4.1.3** (SHA-256 pinned at build) with the identity-override patch, and the `nebulae-server` bridge built from the same commit. The separate repository <https://github.com/koppot/nebulae-server> remains the distribution/Unraid-template repository; it is not removed, and the tagged image published from this repository is what the Unraid template and Compose file should point to.

### Start (published image)

```bash
cd server/docker
cp .env.example .env
# edit .env: set NEBULAE_TOKEN (generate with: openssl rand -hex 32)
docker compose pull
docker compose up -d
docker compose ps          # wait for "healthy"
```

`.env` holds the token and must never be committed (it is git-ignored; only `.env.example`, which contains placeholders, is tracked). Compose refuses to start when `NEBULAE_TOKEN` is unset.

### Build from source

Build from the **repository root** (the build context needs `server/Cargo.*`, `server/src` and `server/docker/*`):

```bash
docker build -f server/docker/Dockerfile \
  --build-arg BUILD_ID="$(git rev-parse HEAD)" \
  -t ghcr.io/koppot/nebulae-server:0.2.8 .
# or, from server/docker:  BUILD_ID=$(git rev-parse HEAD) docker compose build
```

Plain `docker run` equivalent:

```bash
docker run -d --name nebulae-server --restart unless-stopped \
  -e NEBULAE_TOKEN="$NEBULAE_TOKEN" \
  -p 9174:9174 -p 51413:51413 -p 51413:51413/udp \
  -v nebulae-downloads:/data/downloads \
  -v nebulae-config:/data/config \
  -v nebulae-watch:/data/watch \
  ghcr.io/koppot/nebulae-server:0.2.8
```

### Version and build identity

`GET /api/v1/status` (authenticated) reports the bridge `version` (0.2.8), the running Transmission `engine_version` (4.1.3, read live from the daemon) and the `build_id` (git commit baked in at build time). The image also carries OCI labels (`org.opencontainers.image.version`, `.revision`, `io.nebulae.transmission.version`, `io.nebulae.transmission.patch`); inspect with `docker image inspect <image> --format '{{json .Config.Labels}}'`.

### Authentication and safe exposure

- Every `/api/v1/*` call needs `Authorization: Bearer <NEBULAE_TOKEN>`; missing or wrong tokens get `401`.
- The Transmission RPC port (9091) listens on `127.0.0.1` inside the container and is never published.
- The API is plain HTTP. Do not expose port 9174 directly to the internet: put it behind a TLS reverse proxy, a VPN, or a Tailscale network, and forward only the peer port (51413 TCP+UDP) on your router.
- Rotate the token by editing `.env` and running `docker compose up -d` (the container is recreated; torrents are kept), then update the token in the desktop Server tab.

### Volumes, permissions and ports

| Mount | Path in container | Purpose |
|---|---|---|
| `DOWNLOAD_DIR` (default named volume `nebulae-downloads`) | `/data/downloads` | Completed and in-progress downloads |
| `nebulae-config` | `/data/config` | Transmission `settings.json`, torrents and resume state |
| `WATCH_DIR` (default named volume `nebulae-watch`) | `/data/watch` | Drop `.torrent` files here to auto-add |

For host paths set `DOWNLOAD_DIR` / `WATCH_DIR` and `PUID` / `PGID` to the owning user of those folders; the entrypoint switches the container user to match and fixes ownership of `/data`.

| Port | Purpose |
|---|---|
| `9174/tcp` (`NEBULAE_HOST_PORT`) | Authenticated REST API used by the desktop client |
| `51413/tcp+udp` | BitTorrent peer port (forward on the router) |

### Connect the desktop client

Settings → **Server** → enable *Connect to a Nebulae Server (Transmission)* → URL `http://<host>:9174` and the token → **Test connection**. The **Server** tab then lists, adds, pauses/resumes, re-prioritizes files, relocates and sets sequential mode on the remote engine, using the same layout as Manager.

### Update, stop, back up, roll back

```bash
cd server/docker
docker compose stop                     # graceful: Transmission writes resume data first
docker compose down                     # remove the container, keep the volumes
NEBULAE_TAG=0.2.8 docker compose pull && docker compose up -d     # update to a tag
# Back up configuration/resume state (the downloads volume is your data; copy it separately):
docker run --rm -v nebulae-config:/c -v "$PWD":/b debian:bookworm-slim tar czf /b/nebulae-config-backup.tgz -C /c .
# Roll back: set NEBULAE_TAG to the previous version and run `docker compose up -d`;
# to restore configuration, extract the backup into the nebulae-config volume while the container is stopped.
```

Never run `docker compose down -v` unless you intend to delete the named volumes (config, resume state and, by default, downloads).

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
   - Enable *Connect to a Nebulae Server (Transmission)*.
   - Enter `http://<server-ip>:9174` and your `NEBULAE_TOKEN`.
   - Click **Test connection**.

5. **Peer port:** Open port `51413` (TCP + UDP) on your firewall/router, or change `peer-port` in `/data/config/settings.json` and the published ports to match.

### Not supported after migration

- rTorrent-specific per-tracker scrape intervals and custom XMLRPC commands are not exposed by `nebulae-server`.
- Super-seeding toggle — Transmission does not support super-seeding; the UI control returns an error when connected to a remote server.

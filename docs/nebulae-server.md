# Nebulae Server (Docker) and the desktop client

Nebulae Server is a complete headless deployment powered by Nebulae Core, Nebulae’s bundled torrent engine. The Docker image includes the required server components; no separate torrent-engine installation is needed. Control it from the Nebulae desktop app’s Server tab. Shorter summaries elsewhere (README, GitHub Wiki, release notes, website) link here.

Current published image: **`ghcr.io/koppot/nebulae-server:0.2.8`** (also `:latest`), index digest `sha256:4ae899a02f5c6be283d91d7d717f7eee3feaa4c54322c8757b28b1e96009ab63`, platform **linux/amd64 only**.

> **"Activation"** means only turning on and filling in *Settings → Server* in the desktop app. There is no license, account or key to activate.

## Contents

1. [Architecture: what runs where](#1-architecture-what-runs-where)
2. [Install with Docker](#2-install-with-docker)
3. [Platform notes (Linux, Windows, macOS, Unraid/NAS)](#3-platform-notes)
4. [Connect the desktop app](#4-connect-the-desktop-app)
5. [Network and security](#5-network-and-security)
6. [Trackers and the AsianDVDClub (ADC) one-click flow](#6-trackers-and-the-asiandvdclub-adc-one-click-flow)
7. [Operate: logs, stop, update, back up, roll back, remove](#7-operate)
8. [Troubleshooting](#8-troubleshooting)
9. [What was verified, and what was not](#9-what-was-verified-and-what-was-not)
10. [Bridge API reference](#supported-bridge-api-endpoints-apiv1)
11. [Migrating from rTorrent](#migrating-from-rtorrent-to-nebulae-server)
12. [Advanced Implementation Reference](#advanced-implementation-reference)

## 1. Architecture: what runs where

Inside the container, Nebulae Core handles downloads and seeding. Nebulae Server provides the authenticated API used by the desktop client. Both components are included and started automatically inside the container.

- **Nebulae Core:** The bundled torrent engine. Its internal engine RPC port 9091 listens on `127.0.0.1` inside the container only and is container loopback only; do not expose.
- **Nebulae Server:** An authenticated REST API on port **9174** that interfaces with the engine. Every call requires `Authorization: Bearer <NEBULAE_TOKEN>`.

There is no web UI. The only client is the Nebulae desktop app.

**On the desktop** the app has two engines and two tabs:

| | **Manager** tab | **Server** tab |
|---|---|---|
| Engine | Nebulae Core on this computer | Nebulae Core inside the server container |
| Who downloads, seeds and stores | This computer | **The server.** Data lands in the container's `/data/downloads` (your host folder or volume) |
| Shows | This computer's torrents | The server's torrents, in the same layout, under a connection banner |
| Appears when | Always | After you enable and configure *Settings → Server* |

The desktop app is a remote control for the Server tab. It does not download or seed those torrents, so it can be closed or offline while the server keeps working. The token is kept in the app's Rust backend and is never handed to the WebView.

**Supported from the Server tab:** list and live progress; add a magnet link, URL or `.torrent` file; start/pause one or all; remove (with or without data); re-announce; recheck; per-torrent speed/ratio limits; move storage location; category; per-file priority; sequential download; kill switch.

**Not available in Server mode** (the app says so, or hides the control): renaming torrents/files/folders, exporting the `.torrent` file, super-seeding, and **playback / "open file" for server files**. The server's files live on the server, and there is **no path mapping** between server and desktop paths. To watch or open a file, reach the server's download folder yourself (for example a network share) and open it with any player.

**Which engine receives a new torrent?** Whichever tab you add it from: the Add Torrent dialog follows the active tab. In the Server tab the destination starts empty (the server default, `/data/downloads`) and is sent only if you type a path; type a **path inside the container**, such as `/data/downloads/movies`, never a folder from your computer. Website "one-click" deliveries are routed by a separate priority list, see [section 6](#6-trackers-and-the-asiandvdclub-adc-one-click-flow).

## 2. Install with Docker

### Prerequisites

- Docker Engine or Docker Desktop with Compose v2 (`docker compose`; verified with Docker 29.9.3).
- A **linux/amd64** host (see [platform notes](#3-platform-notes) for Apple Silicon and ARM).
- Ability to reach ports 9174 (desktop → server) and 51413 TCP+UDP (BitTorrent peers) on the host.

### Choose an image reference

| Reference | Use when |
|---|---|
| `ghcr.io/koppot/nebulae-server:0.2.8` | **Recommended.** Pinned release; changes only when you change the tag |
| `ghcr.io/koppot/nebulae-server:latest` | You want the newest release on every pull (currently identical to 0.2.8) |
| `ghcr.io/koppot/nebulae-server@sha256:4ae899a02f5c6be283d91d7d717f7eee3feaa4c54322c8757b28b1e96009ab63` | Immutable: byte-for-byte this build |

The tag `:beta` is the **old rTorrent-based beta**. It uses different environment variables and layout; do not use it with this guide.

### Generate a token

```bash
openssl rand -hex 32
```

Use the output as `NEBULAE_TOKEN`. Treat it like a password. The container refuses to start without it.

### Option A: Docker Compose (recommended)

```bash
mkdir nebulae-server && cd nebulae-server
curl -fsSLO https://raw.githubusercontent.com/koppot/Nebulae-torrent_client/main/server/docker/docker-compose.yml
curl -fsSL  https://raw.githubusercontent.com/koppot/Nebulae-torrent_client/main/server/docker/.env.example -o .env
# edit .env: set NEBULAE_TOKEN=<your token>; optionally DOWNLOAD_DIR, WATCH_DIR, PUID, PGID
docker compose pull
docker compose up -d
docker compose ps        # wait for "healthy" (about 20 s)
```

(Or clone the repository and use `server/docker/`.) `.env` holds the token and must not be committed or shared.

`.env` variables:

| Variable | Default | Meaning |
|---|---|---|
| `NEBULAE_TOKEN` | **required** | Bearer token the desktop app presents |
| `NEBULAE_TAG` | `0.2.8` | Image tag |
| `NEBULAE_IMAGE` | `ghcr.io/koppot/nebulae-server` | Image repository |
| `NEBULAE_HOST_PORT` | `9174` | Host port for the API (container side is always 9174) |
| `DOWNLOAD_DIR` | named volume `nebulae-downloads` | Host folder mounted at `/data/downloads` |
| `WATCH_DIR` | named volume `nebulae-watch` | Host folder mounted at `/data/watch`; drop `.torrent` files here to auto-add |
| `PUID` / `PGID` | `1000` / `1000` | User/group the daemon runs as; set to the owner of your host folders |

Container-level environment (set by the Compose file; change only if you know why): `NEBULAE_PORT=9174`, `NEBULAE_BIND=0.0.0.0`, `TRANSMISSION_RPC_URL=http://127.0.0.1:9091/transmission/rpc`. The bridge also reads `TRANSMISSION_USER` / `TRANSMISSION_PASS` for a password-protected Transmission (not used by the bundled daemon).

### Option B: `docker run`

```bash
export NEBULAE_TOKEN="$(openssl rand -hex 32)"; echo "$NEBULAE_TOKEN"   # save this
docker run -d --name nebulae-server --restart unless-stopped \
  --platform linux/amd64 \
  -e NEBULAE_TOKEN="$NEBULAE_TOKEN" -e PUID=1000 -e PGID=1000 \
  -p 9174:9174 -p 51413:51413 -p 51413:51413/udp \
  -v nebulae-downloads:/data/downloads \
  -v nebulae-config:/data/config \
  -v nebulae-watch:/data/watch \
  ghcr.io/koppot/nebulae-server:0.2.8
```

The image itself defines no Docker health check; the Compose file adds one. With `docker run`, check health with the `curl` commands below.

### Ports, volumes, ownership

| Port | Purpose | Expose to |
|---|---|---|
| `9174/tcp` | Authenticated REST API (desktop app) | Your LAN/VPN only, see [section 5](#5-network-and-security) |
| `51413/tcp` + `51413/udp` | BitTorrent peers | Forward on your router if you want inbound peers |
| `9091` | Internal engine RPC | **Never.** Container loopback only; do not expose |

| Container path | Holds | Back up? |
|---|---|---|
| `/data/config` | Nebulae Core settings and torrent state (`settings.json`, `torrents/`, `resume/`, `peer-id-suffix`) | **Yes** |
| `/data/downloads` | Downloads and `.incomplete` | Your data; back up separately |
| `/data/watch` | Optional drop folder for `.torrent` files | No |

On first start the entrypoint writes a default `settings.json` (download dir `/data/downloads`, peer port 51413, port forwarding on, DHT/PEX/LPD **off**, encryption preferred) and **never overwrites an existing one**. It remaps the `nebulae` user to `PUID`/`PGID` and fixes ownership of `/data`. It also stores a stable `peer-id-suffix` in the config volume, so a recreated container is still the same BitTorrent client to trackers.

Because DHT, PEX and local peer discovery are off by default, the server finds peers through **trackers**. A bare magnet link with no tracker has no way to find peers; use magnets that carry tracker URLs, or `.torrent` files.

### Verify it works

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:9174/api/v1/status                       # 401 (no token)
curl -s -H "Authorization: Bearer $NEBULAE_TOKEN" http://localhost:9174/api/v1/status              # 200, JSON
```

Expected JSON (`build_id` is the git commit the image was built from):

```json
{"name":"nebulae-server","version":"0.2.8","engine":"transmission","active_torrents":0,"paused_torrents":0,"kill_switch_engaged":false,"bound_interface":null,"engine_version":"4.1.3 (838877323f)","build_id":"c4361e02d2d7a36c32f76586076276785c611870"}
```

Harmless log lines: Transmission warns that its RPC "has no password" (it is loopback-only inside the container) and that `queue.json` does not exist on first start.

## 3. Platform notes

All platforms use the same image and the same commands. Behavior differs in how Docker reaches storage and networks.

### Linux (native amd64)

Use Compose or `docker run` as above. For host folders, set `DOWNLOAD_DIR` and make `PUID`/`PGID` match their owner (`id -u`, `id -g`). Allow `9174` (LAN only) and `51413` (TCP+UDP) through the host firewall. Not verified on a native Linux host in this pass; the image is a standard linux/amd64 Debian bookworm image.

### Windows (Docker Desktop)

Use Docker Desktop with the WSL 2 backend and run the same `docker compose` commands in PowerShell. Prefer the default **named volumes** (or a folder inside WSL); `PUID`/`PGID` have no meaning for Windows drive folders, and Windows bind mounts may not honor Linux ownership. Allow Docker Desktop through Windows Firewall for 9174 and 51413 if other computers must reach it. Not verified on Windows Docker Desktop in this pass.

### macOS (Docker Desktop)

Same commands (verified on Docker Desktop with Compose). On Apple Silicon the image runs under emulation: Docker prints a "requested image's platform (linux/amd64) does not match the detected host platform" warning, which is harmless (or add `--platform linux/amd64` / `platform: linux/amd64`). Host folders work; set `PUID`/`PGID` to your macOS user (`id -u`, usually 501, and `id -g`). Named volumes live in Docker Desktop's virtual disk, which has a size limit: **when that disk is full, the engine fails with "No space left on device"** (see troubleshooting).

### Unraid and other NAS

The old Unraid template in the `koppot/nebulae-server` repository points at the retired `:beta` rTorrent image (variable `NEBULAE_SERVER_TOKEN`, port 6881, single `/data`). **Do not use it with 0.2.8.** On Unraid use *Docker → Add Container* manually (untested on Unraid hardware):

| Field | Value |
|---|---|
| Repository | `ghcr.io/koppot/nebulae-server:0.2.8` |
| Network | bridge |
| Variable | `NEBULAE_TOKEN` = your token (masked) |
| Variables | `PUID=99`, `PGID=100` (Unraid's `nobody:users`) |
| Ports | `9174` TCP, `51413` TCP, `51413` UDP |
| Paths | `/data/downloads` → e.g. `/mnt/user/downloads`; `/data/config` → `/mnt/user/appdata/nebulae-server/config`; `/data/watch` → `/mnt/user/appdata/nebulae-server/watch` |

Synology, QNAP, TrueNAS and ARM NAS devices are not tested. A native ARM host would need linux/amd64 emulation (qemu/binfmt); no arm64 image is published.

## 4. Connect the desktop app

Works the same on Windows, macOS and Linux (release **v0.2.8** or newer).

1. Open **Settings → Server**.
2. Tick **Connect to a Nebulae Server (Transmission)** (the current UI label in the released application, connecting to the bundled Nebulae Core server).
3. **Server address:** `http://<server-ip>:9174` (for example `http://192.168.1.50:9174` or `http://tower.local:9174`). `https://…` also works behind a reverse proxy. If you changed `NEBULAE_HOST_PORT`, use that port.
4. **API token:** the `NEBULAE_TOKEN` value. (The field's grey placeholder still reads `NEBULAE_SERVER_TOKEN` from the old beta; it is the same field.)
5. Click **Test connection**. Success reads "Connected…", and a **Server** tab appears next to **Manager**. No restart is needed.

Using it:

- The banner above the Server tab shows *Connected · nebulae-server 0.2.8 · engine: transmission*, or the connection/authentication error, with a refresh button.
- **Add Torrent** from the Server tab. Leave the destination blank for `/data/downloads`, or type a path inside the container. Magnet, URL and `.torrent` file all work.
- Start/pause from the toolbar or row menu; progress updates live. Torrents survive container restarts and recreation as long as `/data/config` persists.
- **Disconnect safely:** untick *Connect to a Nebulae Server* (or just quit the app). The Server tab disappears; the server **keeps downloading and seeding** on its own. The desktop app deletes server data only when you remove a torrent in the Server tab and choose to delete its data.

The Manager tab never changes: it keeps showing this computer's torrents.

## 5. Network and security

- **Default, trusted LAN:** plain HTTP to `http://<server-ip>:9174` with the bearer token. The token travels unencrypted on that connection, so keep it on a network you trust.
- **Remote access, preferred:** put the server and your desktop on a VPN or tailnet and use the server's tailnet address (for example `http://100.x.y.z:9174`). This changes nothing in an existing TailNet setup; Nebulae Server is just another reachable host.
- **Remote access over the internet:** terminate TLS in a reverse proxy in front of 9174 and bind the container's API to localhost (`127.0.0.1:9174:9174` in the ports mapping). Illustrative Caddy example (not verified here): `nebulae.example.com { reverse_proxy 127.0.0.1:9174 }`. Then use `https://nebulae.example.com` in the app.
- **Never** publish port 9174 to the internet unprotected, and **never** publish or proxy internal engine RPC port 9091 (it is container loopback only; do not expose).
- Forward only the peer port (51413 TCP+UDP) on your router.
- Rotate the token by editing `.env` and running `docker compose up -d` (the container is recreated, torrents are kept), then update the token in Settings → Server.

## 6. Trackers and the AsianDVDClub (ADC) one-click flow

- **Public trackers / magnets with tracker URLs:** add them in the Server tab. DHT/PEX/LPD are off by default on the server, so the torrent needs working trackers.
- **Private trackers:** use the personalized `.torrent` from the tracker. The passkey inside it is a **secret**: it is stored in the server's `/data/config/torrents/`, so protect the config volume and its backups, and never post screenshots or logs containing announce URLs.
- **Tracker client rules apply.** Many private trackers restrict which clients, IPs or simultaneous sessions are allowed. The server is a different client and IP from your desktop. Do not run the **same private torrent** on the desktop (Manager tab) and on the server at the same time. Nebulae Server keeps a stable peer-id suffix across container recreation so a restart is not seen as a second client.
- **ADC website one-click delivery:** the desktop app redeems the website's delivery request and sends the torrent to the destination selected in ADC One-Click Client Routing: local Nebulae Core or a configured Nebulae Server. The list contains **Local Nebulae Engine** and **Remote server / seedbox (Transmission)**, in priority order (the remote entry's label in the released application includes '(Transmission)' to identify the bundled engine server; it connects to your configured Nebulae Server). If the first is unavailable the next enabled one is used (the remote entry needs *Settings → Server* configured and reachable). Local delivery is skipped while the app is in Server mode.
- **A connected server does not by itself prove website routing.** After a one-click request, confirm the torrent appeared in the Server tab and not in Manager. This guide does not claim that any specific tracker has been tested with the published image; try one small torrent first.

## 7. Operate

```bash
docker compose ps                         # status and health
docker compose logs -f --tail=100         # logs (docker logs -f nebulae-server for docker run)
docker inspect -f '{{.State.Health.Status}}' nebulae-server     # Compose only
docker compose stop                       # graceful: Nebulae Core saves torrent state before stopping
docker compose down                       # remove container + network, KEEP volumes and folders
docker compose up -d                      # start again; torrents are still there
```

**Update / pin / roll back:** edit `NEBULAE_TAG` in `.env`, then `docker compose pull && docker compose up -d`. To roll back, set the previous tag and run `docker compose up -d`. Take a config backup before upgrading.

**Back up the configuration (named-volume setup):**

```bash
docker compose stop
docker run --rm -v nebulae-config:/c -v "$PWD":/b debian:bookworm-slim tar czf /b/nebulae-config-backup.tgz -C /c .
docker compose start
```

Compose prefixes volume names with the project (folder) name, for example `nebulae-server_nebulae-config`; list them with `docker volume ls`. If `/data/config` is a host folder, copy that folder while the container is stopped. Restore by extracting into the config location while the container is stopped. The backup contains passkeys (section 6). Downloads are separate data; back them up as you do any files.

**Remove without losing data:** `docker compose down` keeps named volumes and host folders. `docker compose down -v` **deletes the named volumes** (config, resume state and, by default, downloads); only run it when you mean it. Removing the image (`docker rmi`) never deletes volumes.

**Version and build identity:** `GET /api/v1/status` reports the bridge `version`, the live bundled engine version (`engine_version`), and the `build_id` commit. Image labels: `docker image inspect <image> --format '{{json .Config.Labels}}'` (source repository, version, revision, engine version and patch).

**Build from source** (repository root):

```bash
docker build -f server/docker/Dockerfile --build-arg BUILD_ID="$(git rev-parse HEAD)" -t nebulae-server:local .
```

Source of truth: `server/` (bridge) and `server/docker/` (Dockerfile, entrypoint, Compose, Transmission identity patch) in this repository. The separate repository <https://github.com/koppot/nebulae-server> is a legacy mirror of the old rTorrent-based beta; see its README.

## 8. Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| **Test connection: connection refused** | Container not running or wrong port. Run `docker compose ps`; check `NEBULAE_HOST_PORT` and that the URL uses the host's port. |
| **Timeout / unreachable** | Wrong IP, firewall, or different network/VLAN. From the desktop machine run `curl -i http://<ip>:9174/api/v1/status` (expect `401`). On Windows/macOS allow Docker Desktop in the OS firewall. |
| **Wrong URL** | Must start with `http://` or `https://`, and include `:9174` unless behind a proxy. |
| **Authentication failed / 401** | Token mismatch. Re-enter it in Settings → Server. Token changes need `docker compose up -d`. |
| **Container exits: "NEBULAE_TOKEN environment variable is required"** | `.env` missing or empty. |
| **Downloads land somewhere unexpected** | Server paths are **container paths**. Anything outside `/data/…` lives inside the container and disappears with it. Map host folders to `/data/downloads` and leave the destination blank. |
| **Permission denied / files owned by the wrong user** | Set `PUID`/`PGID` to the folder owner and run `docker compose up -d` (ownership of `/data` is fixed on start). |
| **Torrents vanished after recreating** | `/data/config` was not on a persistent volume, or was removed with `down -v`. Restore a config backup; the downloaded files are still there, so re-add the torrents and let them recheck. |
| **"No space left on device" in logs; state not saved** | The disk backing Docker is full (on macOS/Windows, Docker Desktop's virtual disk). Free space or raise Docker Desktop's disk limit; `docker system df` shows usage. |
| **Torrent shows Downloading with 0 peers / no metadata** | Queued or stalled, not paused (**Paused** is shown explicitly): it has no peers. DHT/PEX/LPD are off, so it needs working tracker URLs; check the Trackers tab and port reachability. |
| **Private tracker rejects or limits the client** | Multi-client or session rules: stop the same torrent on the desktop, use one client/IP per torrent, keep the container's peer id (do not delete `/data/config/peer-id-suffix`), and read the tracker's client policy. |
| **No incoming peers / port closed, partial or unknown** | Forward 51413 TCP+UDP to the Docker host and allow it in its firewall. IPv4 and IPv6 reachability can differ (the app reports them separately); an IPv6-only open port is not fully open. "Unknown" means the test could not decide, not that the port is closed. |
| **Torrent went to the wrong machine** | Check which tab you added it from, and the ADC routing priority list (section 6). A torrent in Manager is on your computer. |
| **Server tab missing** | The tab appears only while *Connect to a Nebulae Server* is ticked and an address is saved. |

**Redacted diagnostics for a bug report:**

```bash
docker version --format '{{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}'
docker compose ps
docker image inspect ghcr.io/koppot/nebulae-server:0.2.8 --format '{{index .RepoDigests 0}}'
docker compose logs --tail=200 | sed -E 's#(passkey=|announce/)[A-Za-z0-9]+#\1<redacted>#g'
curl -s -H "Authorization: Bearer $NEBULAE_TOKEN" http://localhost:9174/api/v1/status
```

Never include the token, a `.env` file, a `.torrent` from a private tracker, or an announce URL.

## 9. What was verified, and what was not

**Verified** against the published image `sha256:4ae899a0…` (Docker Desktop 29.9.3 on macOS; Compose and `docker run`; linux/amd64 image under emulation): image platform and OCI labels; Compose file validity; container reaches *healthy*; no token or wrong token returns 401 and the correct token returns 200 with the JSON above; a missing `NEBULAE_TOKEN` makes the container exit with a clear message; torrent add with a server path, pause and resume through the API; the torrent, its paused state and the same `peer-id-suffix` preserved across `stop`, `down` and `up` (config on a host folder); host-folder ownership follows `PUID`/`PGID`; graceful stop completes in under a second; `:latest` resolves to the same digest as `:0.2.8`. Desktop labels and steps in section 4 were checked against the 0.2.8 source.

**Not verified in this pass:** native Linux, Windows Docker Desktop, Unraid and other NAS hosts; native ARM; named-volume recreation (the local Docker disk was full, so persistence was verified with a host-folder config); an end-to-end desktop-to-container session with the released installers; ADC website one-click delivery and any specific private tracker against the published image; the reverse-proxy example. No Server-tab screenshots are included.

---

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

---

## Migrating from rTorrent to Nebulae Server

The current server deployment uses Nebulae Core through Nebulae Server’s authenticated API. Previous Nebulae releases supported rTorrent over SCGI as a remote engine. The client Settings UI fields are unchanged (URL and token); only the server side changes.

### Key differences

| Behaviour | rTorrent / SCGI | Nebulae Server / Nebulae Core |
|---|---|---|
| Protocol | XML-RPC over SCGI | JSON-RPC (`POST /transmission/rpc`) via authenticated REST bridge |
| Auth | None / SSH tunnel | Bearer token (`NEBULAE_TOKEN`) |
| Sequential download | Not supported | Supported — `torrent-set sequentialDownload` |
| File priority values | 0 / 1 / 2 | 0 (skip) / 1 (normal) / 2 (high) — same semantics |
| Category/label | XMLRPC custom tag | Engine label field |
| Peer port | Configured in rTorrent config | `peer-port` in `/data/config/settings.json` (default 51413) |

### Migration steps

1. **Export active torrents from rTorrent.**  
   The simplest path is to copy the `.torrent` files and resume data from your rTorrent session directory. The engine reads standard `.torrent` files but does not import rTorrent's `.rtorrent` resume metadata — torrents will recheck on first start.

2. **Install and start `nebulae-server`** using the Docker steps above.

3. **Place your `.torrent` files in `/data/watch`** (or add them through Nebulae after connecting). The engine will begin rechecking them against the files in `/data/downloads`.

4. **In Nebulae Settings → Server:**
   - Disable any existing rTorrent SCGI URL.
   - Enable *Connect to a Nebulae Server (Transmission)* (the UI label in the released application, connecting to the bundled Nebulae Core server).
   - Enter `http://<server-ip>:9174` and your `NEBULAE_TOKEN`.
   - Click **Test connection**.

5. **Peer port:** Open port `51413` (TCP + UDP) on your firewall/router, or change `peer-port` in `/data/config/settings.json` (container stopped) and the published ports to match.

### Not supported after migration

- rTorrent-specific per-tracker scrape intervals and custom XMLRPC commands are not exposed by `nebulae-server`.
- Super-seeding toggle — the bundled engine does not support super-seeding; the UI control returns an error when connected to a remote server.

---

## Advanced Implementation Reference

> [!NOTE]
> **Technical Identifiers & Attribution**
> Some internal identifiers retain upstream names for protocol and file-format compatibility. They refer to components included with Nebulae, not separate installations:
> - The underlying bundled torrent engine executable is based on patched Transmission 4.1.3 (`transmission-daemon`), compiled with Nebulae's identity and keepalive patches.
> - Internal RPC communication uses the standard `/transmission/rpc` endpoint on container loopback (`127.0.0.1:9091`).
> - Environment variables `TRANSMISSION_RPC_URL`, `TRANSMISSION_USER`, and `TRANSMISSION_PASS` are read by the bridge for engine connection configuration.
> - Status JSON response reports `"engine": "transmission"` for client compatibility.
> - Source licenses, patch disclosures, and third-party notices are maintained in [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md) and `patches/`.

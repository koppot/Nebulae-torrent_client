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

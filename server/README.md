# nebulae-server — Headless Server

A standalone REST API bridge that wraps a local Transmission daemon, designed to run on a NAS, VPS, or seedbox. The Nebulae desktop client connects to it via the **Server** tab in Settings.

## Quick Start (Docker)

```bash
# 1. Clone the repository
git clone https://github.com/koppot/Nebulae-torrent_client.git
cd Nebulae-torrent_client

# 2. Set your API token
export NEBULAE_TOKEN="your-secret-token"

# 3. Start the server
docker compose up -d

# 4. Configure the Nebulae desktop client
#    Settings → Server → Enable remote server
#    URL: http://<server-ip>:9174
#    Token: your-secret-token
```

## Docker Compose Configuration

| Variable | Default | Description |
|---|---|---|
| `NEBULAE_TOKEN` | **(required)** | Bearer token for API authentication |
| `NEBULAE_PORT` | `9174` | REST API port |
| `NEBULAE_BIND` | `0.0.0.0` | Listen address |
| `DOWNLOAD_DIR` | `./downloads` | Host path for downloads |
| `WATCH_DIR` | `./watch` | Host path for .torrent watch folder |
| `PUID` | `1000` | Container user ID |
| `PGID` | `1000` | Container group ID |

## Ports

| Port | Protocol | Purpose |
|---|---|---|
| `9174` | TCP | nebulae-server REST API |
| `51413` | TCP + UDP | BitTorrent peer connections |

## API Reference

All endpoints require `Authorization: Bearer <token>` header.

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/api/v1/status` | Server name, version, engine, kill switch |
| `GET` | `/api/v1/torrents` | List all torrents with full metadata |
| `POST` | `/api/v1/torrents` | Add torrent (magnet/URL/base64 metainfo) |
| `POST` | `/api/v1/torrents/{id}/pause` | Pause torrent |
| `POST` | `/api/v1/torrents/{id}/resume` | Resume torrent |
| `DELETE` | `/api/v1/torrents/{id}?delete_files=true` | Remove torrent |
| `POST` | `/api/v1/torrents/{id}/reannounce` | Force reannounce |
| `POST` | `/api/v1/torrents/{id}/recheck` | Force recheck |
| `POST` | `/api/v1/torrents/{id}/limits` | Set speed/ratio limits |
| `POST` | `/api/v1/torrents/pause-all` | Pause all torrents |
| `POST` | `/api/v1/torrents/resume-all` | Resume all torrents |
| `POST` | `/api/v1/kill-switch` | Engage/reset kill switch |

## Standalone (without Docker)

```bash
# Build
cd server
cargo build --release

# Run (requires Transmission daemon running separately)
./target/release/nebulae-server \
    --token "your-secret-token" \
    --transmission-url "http://127.0.0.1:9091/transmission/rpc" \
    --port 9174
```

## Migration from Existing Transmission

If you have an existing Transmission installation with data you want to manage through Nebulae:

1. **Mount existing download directories** into the Docker container:
   ```yaml
   volumes:
     - /path/to/existing/downloads:/data/downloads
     - /path/to/existing/config:/data/config
   ```

2. **Copy existing Transmission settings**: If you have a `settings.json`, place it at the config volume mount. The container will use it instead of generating defaults.

3. **Preserve torrents**: Transmission stores its torrent state in `config/torrents/` and `config/resume/`. Mount the entire config directory to preserve all active torrents.

## Security

- The API token is the sole authentication mechanism. Use a strong random value.
- The token is never sent to the WebView/frontend — all API calls are proxied through Rust IPC commands.
- The Transmission RPC interface binds to `127.0.0.1` inside the container and is not exposed externally.
- Use HTTPS (reverse proxy with TLS) for production deployments over untrusted networks.

## Health Check

```bash
curl -sf http://localhost:9174/api/v1/status \
    -H "Authorization: Bearer your-token"
```

Expected response:
```json
{
    "name": "nebulae-server",
    "version": "0.1.0",
    "engine": "transmission",
    "kill_switch_engaged": false
}
```

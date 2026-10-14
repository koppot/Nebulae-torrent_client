# Nebulae Server mode (client side)

The Nebulae desktop app can drive a torrent engine running on a server instead of locally:

1. **Settings → Server**
2. Enable *Drive torrents on a Nebulae Server (rtorrent) instead of locally*
3. Enter the server address (`http://<server-ip>:9174`) and the API token
4. Click **Test connection**, then restart the app

The original native interface is unchanged. Options the server engine cannot
fulfill (sequential download, super-seeding, renames, delete-with-data) are
hidden automatically while in server mode.

The server itself — the headless `nebulae-server` bridge, rtorrent packaging,
Docker/Unraid deployment, and release automation — is a separate repository:

**<https://github.com/koppot/nebulae-server>**

See that repository's `docs/` for the deployment guides (including the Unraid
install guide) and the bridge API reference.

# Third-Party Notices and Open-Source Software Licenses

Nebulae does not bundle media players. Playback uses the user's installed VLC, loaded at runtime as a library (libVLC) and embedded inside the app's own window; VLC is not redistributed. The notices below cover the components Nebulae does distribute.

---

---

---

## 1. Transmission BitTorrent Client (transmission-daemon)

- **Component:** `transmission-daemon` (and bundled support libraries)
- **Version:** 4.1.3
- **License:** GNU General Public License version 2 (GPLv2)
- **Upstream Project & Source Code:** [https://github.com/transmission/transmission](https://github.com/transmission/transmission)
- **Official Release Download:** [https://github.com/transmission/transmission/releases/tag/4.1.3](https://github.com/transmission/transmission/releases/tag/4.1.3)
- **Pinned Artifacts (Phase A1):**
  - Windows x64 MSI SHA-256: `c8ea492d8f46fadac26e0c05b244cabba556201d5fe348dfcf1cf036621741f8`
  - macOS DMG SHA-256: `d622736c19990262f2ab12183e1acaa5b0dad5777ddc7db0a187c06e035a7e1a`
  - Source tarball SHA-256: `ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da`
- **Peer-id prefix:** `-TR4130-`

### Architecture & Aggregation Note

Nebulae executes `transmission-daemon` strictly as an independent, detached sidecar process via standard operating system process launch APIs (`std::process::Command` / `Child`). Nebulae communicates with it exclusively over its JSON-RPC HTTP interface bound to `127.0.0.1` only. Nebulae does not statically or dynamically link to any Transmission library or call Transmission C/C++ runtime internals from the main `nebulae` binary. The `transmission-daemon` binary and its runtime libraries are distributed alongside Nebulae as an independent aggregation.

Source code corresponding to the distributed `transmission-daemon` binary is available at the upstream repository linked above. The pinned source tarball SHA-256 is recorded in this file and in `AGENT-COORDINATION.md`.

### Private-tracker compliance

The bundled `transmission-daemon` is configured at launch with `dht-enabled: false`, `pex-enabled: false`, and `lpd-enabled: false` at the session level. These settings are written by Nebulae's `TransmissionTorrentEngine` and are non-user-overridable. They are defense-in-depth: private torrents with `private: 1` already disable DHT/PEX/LPD at the per-torrent level in any compliant engine.

### GNU GENERAL PUBLIC LICENSE (Version 2, June 1991)

```
                    GNU GENERAL PUBLIC LICENSE
                       Version 2, June 1991

 Copyright (C) 1989, 1991 Free Software Foundation, Inc.,
 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 Everyone is permitted to copy and distribute verbatim copies
 of this license document, but changing it is not allowed.

[Full license text available at: https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt]
```

### Bundled Libraries (platform-specific dylib/so tree)

On macOS and Linux, the bundle scripts (`bundle-{macos,linux}-transmission.sh`) copy
the non-system dynamic libraries required by `transmission-daemon` into
`resources/engine/transmission/lib/`. These libraries may include:

| Library family | Typical license | Upstream |
|---|---|---|
| OpenSSL (libssl, libcrypto) | Apache-2.0 (3.x) / OpenSSL (1.x) | https://openssl.org |
| libminiupnpc | BSD-3-Clause | https://miniupnp.tuxfamily.org |
| libnatpmp | BSD-2-Clause | https://miniupnp.tuxfamily.org |
| libdeflate | MIT | https://github.com/ebiggers/libdeflate |
| libz (zlib) | zlib | https://zlib.net |
| libcurl | curl (MIT-like) | https://curl.se |

The exact set varies by platform and pinned build. Each bundled library's source is obtainable from its upstream project as linked above. No bundled library is modified by Nebulae beyond binary load-path rewriting (`install_name_tool` / `patchelf`), which does not constitute a modification of the work under the applicable licenses.

Nebulae's `transmission-daemon` integration does not enable PEX, DHT, Local Peer Discovery, web-seeding, or any metainfo rewriting capability. It is used exclusively as a private-tracker BitTorrent client.

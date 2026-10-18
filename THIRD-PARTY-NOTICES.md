# Third-Party Notices and Open-Source Software Licenses

Nebulae bundles the components listed below to deliver self-contained playback with no additional user installations required. libVLC (LGPL 2.1+), libbluray (LGPL 2.1), and Eclipse Temurin JRE 21 (GPLv2 + Classpath Exception) are included in the installer. Transmission-daemon (GPLv2) is also bundled as an independent sidecar process. The corresponding source code for each component is available at the upstream projects linked below.

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


---

## 2. VLC media player — libVLC (libvlc, libvlccore, plugins)

- **Component:** libVLC shared libraries and bundled codec/format/output plugins
- **Version:** 3.0.21
- **License:** GNU Lesser General Public License version 2.1 or later (LGPL 2.1+)
- **Upstream Project & Source Code:** [https://www.videolan.org/vlc/](https://www.videolan.org/vlc/)
- **Release Source Tarball:** [https://download.videolan.org/pub/videolan/vlc/3.0.21/](https://download.videolan.org/pub/videolan/vlc/3.0.21/)
- **Bundled Location:** `resources/vlc/` inside the installed package (lib/, plugins/, Frameworks/ on macOS)

### Bundling Method

The CI release workflow (`release.yml`) stages libVLC from the official VLC 3.0.21 distribution for each platform:

- **macOS**: downloaded from `download.videolan.org` (arm64/intel64 DMG), VLC framework extracted into `resources/vlc/`.
- **Windows**: downloaded from `download.videolan.org` (win64 ZIP), `libvlc.dll`, `libvlccore.dll`, and `plugins/` extracted into `resources/vlc/`.
- **Linux**: the runner's system `libvlc5` and `vlc-plugin-*` packages are copied into `resources/vlc/` for inclusion in the AppImage.

Nebulae loads libVLC at runtime via `libloading`. The libraries are not statically linked. No modifications are made to libVLC source or binaries beyond path relocation.

### LGPL 2.1 Compliance Notes

The LGPL 2.1 requires that users be able to replace the bundled LGPL library with a modified version. Nebulae ships the unmodified official libVLC binary. Users may substitute an alternative `libvlc.so.5` / `libvlc.dll` / `libvlc.dylib` in the `resources/vlc/` location within the installation. The `engine.rs` VLC loader checks `resources/vlc/` first and falls back to system paths, making substitution practical.

Corresponding source code is available from [https://download.videolan.org/pub/videolan/vlc/3.0.21/](https://download.videolan.org/pub/videolan/vlc/3.0.21/).

Note: many VLC *plugins* are licensed under the GPL (version 2 or later), not the LGPL. Nebulae bundles VLC's standard plugin set. Users who need to audit individual plugin licenses should refer to the VLC source tree (`modules/`) at the upstream URL above.

### GNU LESSER GENERAL PUBLIC LICENSE (Version 2.1, February 1999)

```
[Full license text available at: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt]
```

---

## 3. libbluray

- **Component:** libbluray — Blu-ray disc access library
- **Version:** 1.3.x (system-provided on Linux; bundled via the macOS VLC framework and Windows VLC distribution on those platforms)
- **License:** GNU Lesser General Public License version 2.1 (LGPL 2.1)
- **Upstream Project & Source Code:** [https://www.videolan.org/developers/libbluray.html](https://www.videolan.org/developers/libbluray.html)
- **Bundled Location:** included as part of the VLC distribution under `resources/vlc/` or loaded from the system library path on Linux

Nebulae loads libbluray dynamically at runtime to read disc metadata (chapter structure, language streams, titles). No modifications are made to libbluray source.

### GNU LESSER GENERAL PUBLIC LICENSE (Version 2.1)

```
[Full license text available at: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt]
```

---

## 4. Eclipse Temurin JRE (OpenJDK 21)

- **Component:** Eclipse Temurin JRE 21 (OpenJDK HotSpot runtime)
- **Version:** 21 GA (latest at release time, fetched from Adoptium)
- **License:** GNU General Public License version 2 with the Classpath Exception (GPLv2 + CE)
- **Upstream Project:** [https://adoptium.net/](https://adoptium.net/)
- **Source Code:** [https://github.com/adoptium/temurin-build](https://github.com/adoptium/temurin-build)
- **Bundled Location:** `resources/jre/` inside the installed package

### Bundling Purpose

libbluray uses BD-J (Blu-ray Disc Java) for UHD interactive menus. A JRE is required to execute BD-J content. The bundled JRE is private to Nebulae's installation and does not affect the user's system Java configuration.

### GNU GENERAL PUBLIC LICENSE (Version 2) + Classpath Exception

```
[Full license text available at: https://openjdk.org/legal/gplv2+ce.html]

The Classpath Exception grants permission to link the JDK class libraries
with independent modules that are not themselves derived works of the JDK.
```


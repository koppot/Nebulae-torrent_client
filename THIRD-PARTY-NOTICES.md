# Third-Party Notices and Open-Source Software Licenses

Nebulae bundles the components listed below to deliver self-contained playback and torrenting with no additional user installations required. This file describes what the release builds contain. The macOS (Apple silicon) build contains:

- libVLC and the VLC plugin set (LGPL 2.1+, with a number of GPL 2+ plugins), five files of which carry Nebulae's patches;
- libbluray with its BD-J jars (LGPL 2.1), libudfread (LGPL 2.1+), FreeType (FreeType License), fontconfig, libpng and the libintl library of GNU gettext (LGPL 2.1+);
- Eclipse Temurin JRE 21 (GPLv2 + Classpath Exception);
- Transmission's `transmission-daemon` (GPLv2 or GPLv3) as an independent sidecar process, with libevent, libdeflate, libpsl, miniupnpc and libintl;
- Tailscale (BSD-3-Clause).

The corresponding source code of each component is available from the upstream projects linked below. For what Nebulae builds or patches itself, the source version, its SHA-256 and the patches are recorded in this repository and are copied into the app.

### Where the notices are inside the macOS app

All paths are relative to `Nebulae.app/Contents/Resources/resources/`.

| What | Where |
|---|---|
| VLC runtime, libbluray and the libraries it needs: license texts, source and patch information | `vlc/licenses/` (one directory per component; `README.txt` is the index) |
| VLC runtime: version and SHA-256 of every file | `build-manifest.json`, `vlc/bundled-libraries.json` |
| Transmission and its libraries: license texts, source and patch information | `engine/transmission/licenses/` |
| Transmission libraries: versions and SHA-256 | `engine/transmission/bundled-libraries.json` |
| Java runtime | `jre/legal/`, `jre/NOTICE` |

---

## 1. Transmission BitTorrent Client (transmission-daemon)

- **Component:** `transmission-daemon` (and bundled support libraries)
- **Version:** 4.1.3
- **License:** GNU General Public License version 2 or version 3, or any future license endorsed by Mnemosyne LLC, at the recipient's choice; some source files carry more permissive licenses; linking to and using OpenSSL is allowed (see `COPYING` in the shipped license directory)
- **Upstream Project & Source Code:** [https://github.com/transmission/transmission](https://github.com/transmission/transmission)
- **Official Release Download:** [https://github.com/transmission/transmission/releases/tag/4.1.3](https://github.com/transmission/transmission/releases/tag/4.1.3)
- **Pinned Artifacts (Phase A1):**
  - Windows x64 MSI SHA-256: `c8ea492d8f46fadac26e0c05b244cabba556201d5fe348dfcf1cf036621741f8`
  - macOS DMG SHA-256: `d622736c19990262f2ab12183e1acaa5b0dad5777ddc7db0a187c06e035a7e1a` (recorded in Phase A1; the macOS build scripts do not use it, see below)
  - Source tarball SHA-256: `ce7d2d8b101f7eb54bc3cf0bc55f52f7ebd4a25fa48e00bdca9a7e0fc02617da`
- **Peer-id prefix:** `-TR4130-`

### Source of the macOS daemon

`scripts/bundle-macos-transmission.sh` builds the macOS daemon from the pinned source tarball above (SHA-256 verified) after applying `scripts/patches/transmission-4.1.3-identity-override.patch`. The patch lets the host application set the peer-id prefix and the user agent through the environment variables `TRANSMISSION_PEER_ID_PREFIX` and `TRANSMISSION_USER_AGENT`; it changes nothing else. The patch, `COPYING`, the license texts of Transmission and of the code in its `third-party/` tree, and a `SOURCE.txt` that gives the source URL and hash are shipped in `engine/transmission/licenses/transmission-4.1.3/`.

### Architecture & Aggregation Note

Nebulae executes `transmission-daemon` strictly as an independent, detached sidecar process via standard operating system process launch APIs (`std::process::Command` / `Child`). Nebulae communicates with it exclusively over its JSON-RPC HTTP interface bound to `127.0.0.1` only. Nebulae does not statically or dynamically link to any Transmission library or call Transmission C/C++ runtime internals from the main `nebulae` binary. The `transmission-daemon` binary and its runtime libraries are distributed alongside Nebulae as an independent aggregation.

Source code corresponding to the distributed `transmission-daemon` binary is the upstream tarball named above plus the patch above. The pinned source tarball SHA-256 is recorded in this file and in `AGENT-COORDINATION.md`.

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

### Bundled Libraries

**macOS (Apple silicon).** `scripts/bundle-macos-transmission.sh` copies the non-system dynamic libraries that `transmission-daemon` needs into `resources/engine/transmission/lib/`, rewrites every load command to a path inside the bundle (`scripts/relocate-macos-dylibs.py`), signs them ad hoc and then fails the build if `scripts/audit-macos-bundle.py --strict --strict-strings` finds a reference to anything outside the bundle. The libraries are Homebrew builds of the upstream releases. Nebulae changes only their load commands and signature; `sha256_source` and `sha256_bundled` in `bundled-libraries.json` record the file before and after. The license files of each library are in `engine/transmission/licenses/<name>-<version>/`.

| Library (file) | Version | License | Upstream |
|---|---|---|---|
| libevent (`libevent-2.1.7.dylib`) | 2.1.13_1 | BSD-3-Clause, plus the permissive licenses of small components listed in its `LICENSE` | https://libevent.org |
| libdeflate (`libdeflate.0.dylib`) | 1.26 | MIT | https://github.com/ebiggers/libdeflate |
| libpsl (`libpsl.5.dylib`) | 0.23.3 | MIT | https://github.com/rockdaboot/libpsl |
| miniupnpc (`libminiupnpc.21.dylib`) | 2.3.3 | BSD-3-Clause | https://miniupnp.tuxfamily.org |
| GNU gettext, libintl (`libintl.8.dylib`) | 1.0 | LGPL-2.1-or-later for libintl; the GPL-3.0 text in the shipped `COPYING` covers the gettext tools of the same package | https://www.gnu.org/software/gettext/ |

**Linux.** `bundle-linux-transmission.sh` copies the non-system dynamic libraries into `resources/engine/transmission/lib/`. These libraries may include:

| Library family | Typical license | Upstream |
|---|---|---|
| OpenSSL (libssl, libcrypto) | Apache-2.0 (3.x) / OpenSSL (1.x) | https://openssl.org |
| libminiupnpc | BSD-3-Clause | https://miniupnp.tuxfamily.org |
| libnatpmp | BSD-2-Clause | https://miniupnp.tuxfamily.org |
| libdeflate | MIT | https://github.com/ebiggers/libdeflate |
| libz (zlib) | zlib | https://zlib.net |
| libcurl | curl (MIT-like) | https://curl.se |

The exact set on Linux varies by pinned build. Each bundled library's source is obtainable from its upstream project as linked above. No Linux library is modified by Nebulae beyond binary load-path rewriting (`patchelf`), which does not constitute a modification of the work under the applicable licenses.

Nebulae's `transmission-daemon` integration does not enable PEX, DHT, Local Peer Discovery, web-seeding, or any metainfo rewriting capability. It is used exclusively as a private-tracker BitTorrent client.


---

## 2. VLC media player — libVLC (libvlc, libvlccore, plugins)

- **Component:** libVLC shared libraries and bundled codec/format/output plugins
- **Version:** 3.0.23 in the macOS and Windows release builds. Linux uses the `libvlc5` and `vlc-plugin-*` packages of the build runner. (The candidate and test workflows still fetch the official 3.0.21 Windows zip.)
- **License:** GNU Lesser General Public License version 2.1 or later (LGPL 2.1+) for libvlc and libvlccore. Each plugin carries the license named in its source files: most are LGPL 2.1+, some are GPL 2+.
- **Upstream Project & Source Code:** [https://www.videolan.org/vlc/](https://www.videolan.org/vlc/)
- **Release Source Tarball:** [https://download.videolan.org/pub/videolan/vlc/3.0.23/vlc-3.0.23.tar.xz](https://download.videolan.org/pub/videolan/vlc/3.0.23/vlc-3.0.23.tar.xz), SHA-256 `e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0`
- **Bundled Location:** `resources/vlc/` inside the installed package (`lib/` and `plugins/`)

### Bundling Method

All platforms stage libVLC from the CI release workflow (`release.yml`):

- **macOS, Apple silicon** (`scripts/bundle-macos-vlc.sh aarch64-apple-darwin`): `libvlccore.9.dylib` and every plugin except the five files below are VideoLAN's own, unmodified binaries from the official VLC 3.0.23 disk image (its SHA-256 is pinned in the script). Five files are built by Nebulae from the VLC 3.0.23 source tarball with the four patches in `patches/vlc-3.0.23/`, applied in this order by `scripts/apply-vlc-patches.sh`: `vt-305`, `navigate-menu`, `hdr-tonemap`, `bluray-playlist-id`.
  - `lib/libvlc.5.dylib` (`navigate-menu`, `bluray-playlist-id`)
  - `plugins/libvideotoolbox_plugin.dylib` (`vt-305`)
  - `plugins/libcaopengllayer_plugin.dylib` (`hdr-tonemap`)
  - `plugins/libglconv_cvpx_plugin.dylib` (built from the source patched by `hdr-tonemap`)
  - `plugins/liblibbluray_plugin.dylib` (`bluray-playlist-id`; linked to the libbluray of section 3)

  Left out of the official set: `plugins.dat` (VLC rebuilds it on first launch), the `libmacosx_plugin` and `libosx_notifications_plugin` plugins of VLC.app (they need the Sparkle and Growl frameworks, which are not part of Nebulae), and the libbluray 1.4.0 jars of the official Blu-ray plugin (replaced by the 1.5.0 jars of section 3).
- **macOS, Intel** (`scripts/bundle-macos-vlc.sh x86_64-apple-darwin`): the unmodified official Intel VLC 3.0.23 (SHA-256 pinned). Nebulae's patches are not in this runtime.
- **Windows**: downloaded from `download.videolan.org` (win64 ZIP), `libvlc.dll`, `libvlccore.dll`, and `plugins/` extracted into `resources/vlc/`.
- **Linux**: the runner's system `libvlc5` and `vlc-plugin-*` packages are copied into `resources/vlc/` for inclusion in the AppImage.

Nebulae loads libVLC at runtime via `libloading`. The libraries are not statically linked. Except for the five macOS Apple-silicon files above, Nebulae makes no modification to libVLC source or binaries beyond path relocation.

The official VLC plugins statically include codec and format libraries that VideoLAN builds from the "contrib" sources named in the VLC source tree. They are VideoLAN's binaries; their licenses are those of the VLC source tree.

### LGPL 2.1 Compliance Notes

The LGPL 2.1 requires that users be able to replace the bundled LGPL library with a modified version. The libraries are separate files that Nebulae loads at run time. Users may substitute an alternative `libvlc.so.5` / `libvlc.dll` / `libvlc.dylib` in the `resources/vlc/` location within the installation. The `engine.rs` VLC loader checks `resources/vlc/` first and falls back to system paths, making substitution practical. On macOS the app's signature must be renewed after a file inside the app is replaced (`codesign --force --deep --sign - Nebulae.app`); macOS refuses an app whose signed contents have changed.

Corresponding source code: the release tarball above. For the five modified macOS files, the tarball plus the four patches, which are in this repository (`patches/vlc-3.0.23/`) and in the app (`vlc/licenses/vlc-3.0.23/`, together with `SOURCE.txt`, `COPYING` and `COPYING.LIB`). `scripts/build-vlc-runtime.sh` is the build recipe (pinned versions, checksums and configure flags).

Note: many VLC *plugins* are licensed under the GPL (version 2 or later), not the LGPL. Nebulae bundles VLC's standard plugin set. Users who need to audit individual plugin licenses should refer to the VLC source tree (`modules/`) at the upstream URL above.

### GNU LESSER GENERAL PUBLIC LICENSE (Version 2.1, February 1999)

```
[Full license text available at: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt]
```

---

## 3. libbluray and the libraries it needs on macOS

- **Component:** libbluray — Blu-ray disc access library, with its BD-J (Blu-ray Disc Java) jars
- **Version:** 1.5.0 in the macOS Apple-silicon build. Windows: the version inside the official VLC win64 distribution. Linux: the system library. macOS Intel: inside the official Intel VLC.
- **License:** GNU Lesser General Public License version 2.1 (LGPL 2.1); the source files name "version 2.1 or (at your option) any later version"
- **Upstream Project & Source Code:** [https://www.videolan.org/developers/libbluray.html](https://www.videolan.org/developers/libbluray.html)
- **Source Tarball (macOS):** [https://download.videolan.org/pub/videolan/libbluray/1.5.0/libbluray-1.5.0.tar.xz](https://download.videolan.org/pub/videolan/libbluray/1.5.0/libbluray-1.5.0.tar.xz), SHA-256 `f676408e91a5d321abf8b8d4dfdae36205c297dab5c54c3ec519639025f474a2`
- **Bundled Location (macOS):** `resources/vlc/lib/` (`libbluray.4.dylib`, `libbluray-j2se-1.5.0.jar`, `libbluray-awt-j2se-1.5.0.jar`); loaded by the VLC Blu-ray plugin `resources/vlc/plugins/liblibbluray_plugin.dylib`

On macOS Apple silicon, `scripts/build-vlc-runtime.sh` builds libbluray 1.5.0 unmodified, with Meson, together with the BD-J jars of the same version (libbluray only accepts jars of its own version). Apache Ant 1.10.18 (Apache License 2.0, [https://ant.apache.org](https://ant.apache.org)) compiles the jars at build time; it is a build tool and is not shipped.

Nebulae loads libbluray dynamically at runtime to read disc metadata (chapter structure, language streams, titles). No modifications are made to libbluray source.

### Libraries copied from Homebrew (macOS Apple silicon)

libbluray needs the libraries below. They are Homebrew builds of the upstream releases that are staged by `scripts/stage-vlc-runtime.sh`, which rewrites their load commands to paths inside the bundle and signs them ad hoc. `vlc/bundled-libraries.json` records versions, hashes before and after, and the minimum macOS version of each. Their license files are in `vlc/licenses/<name>-<version>/`.

| Library (file) | Version | License | Upstream |
|---|---|---|---|
| libudfread (`libudfread.3.dylib`) | 1.2.0 | LGPL-2.1-or-later | https://code.videolan.org/videolan/libudfread |
| FreeType (`libfreetype.6.dylib`) | 2.14.3 | FreeType License (FTL); FreeType is dual-licensed, the alternative is GPL-2.0 | https://freetype.org |
| fontconfig (`libfontconfig.1.dylib`) | 2.18.3 | HPND-sell-variant, Unicode-3.0, MIT-Modern-Variant and MIT, plus some public-domain code (see the shipped `COPYING`) | https://www.freedesktop.org/wiki/Software/fontconfig/ |
| libpng (`libpng16.16.dylib`) | 1.6.59 | PNG Reference Library License version 2 | https://github.com/pnggroup/libpng |
| GNU gettext, libintl (`libintl.8.dylib`) | 1.0 | LGPL-2.1-or-later for libintl; the GPL-3.0 text in the shipped `COPYING` covers the gettext tools of the same package | https://www.gnu.org/software/gettext/ |

The versions are those of the Homebrew packages that were staged (Homebrew's own metadata for freetype can name a newer formula than the staged keg; `bundled-libraries.json` names the keg that was copied).

**FreeType credit.** This software is based in part on the work of the FreeType Team (https://www.freetype.org). Portions of this software are copyright © 2026 The FreeType Project (www.freetype.org). All rights reserved.

The LGPL libraries here (libbluray, libudfread, libintl) are separate files that the VLC Blu-ray plugin loads at run time and can be replaced in the same way as libVLC (section 2).

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

---

## 5. Tailscale

- **Component:** `tailscale` and `tailscaled`
- **Version:** 1.104.1 (pinned in `scripts/bundle-tailscale.sh`)
- **License:** BSD 3-Clause License
- **Upstream Project & Source Code:** [https://github.com/tailscale/tailscale](https://github.com/tailscale/tailscale)
- **Bundled Location:** `resources/engine/tailscale/` inside the installed package

On macOS the binaries are built by `scripts/bundle-tailscale.sh` from the upstream tag `v1.104.1` with the Go toolchain. On Linux and Windows the script downloads the official release archive from `pkgs.tailscale.com` and verifies its pinned SHA-256. Nebulae starts them as separate processes. They are Go programs: the Go standard library and the Go modules they are built from are linked into each binary under their own licenses, which are listed with the upstream source at the tag above (`go.mod` and `licenses/tailscale.md`).

```
BSD 3-Clause License

Copyright (c) 2020 Tailscale Inc & contributors.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

---

## 6. MaxMind GeoLite2 Databases

- **Component:** GeoLite2-Country.mmdb, GeoLite2-ASN.mmdb
- **Provider:** MaxMind, Inc.
- **License:** MaxMind GeoLite2 End User License Agreement
- **License URL:** https://www.maxmind.com/en/geolite/eula
- **Location when present:** `resources/geo/` inside the installed package (on macOS, `Nebulae.app/Contents/Resources/resources/geo/`)

The databases are not part of the repository, and a build includes them only when its maintainer supplies them (`scripts/download_geo_db.sh` fetches them with a MaxMind license key). When they are absent, Nebulae shows no peer country or ASN information and writes one line about it to its log. Release builds made without them contain no MaxMind data.

When present, Nebulae uses the GeoLite2 databases for offline IP geolocation (peer country and ASN classification in the torrent peer list). The databases are loaded at runtime via the `maxminddb` Rust crate. No modifications are made to the database files.

Use of this MaxMind product is governed by MaxMind's GeoLite End User License Agreement, which can be viewed at https://www.maxmind.com/en/geolite/eula.

### Incorporated Geographical Data — GeoNames

This database incorporates GeoNames (https://www.geonames.org) geographical data, which is made available under the Creative Commons Attribution 4.0 License. To view a copy of this license, visit https://creativecommons.org/licenses/by/4.0/.

**Attribution:** GeoNames — https://www.geonames.org  
**License:** Creative Commons Attribution 4.0 International (CC BY 4.0)  
**License URL:** https://creativecommons.org/licenses/by/4.0/

---

## 7. Build-time tools that are not shipped

The macOS runtime is built with Meson, Ninja, pkgconf, bison, CMake, the Xcode command line tools, a JDK and Apache Ant 1.10.18 (Apache License 2.0). None of them is part of the installed app. Ant is fetched as a checksum-verified upstream release by `scripts/build-vlc-runtime.sh`.

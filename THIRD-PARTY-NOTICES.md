# Third-Party Notices and Open-Source Software Licenses

Nebulae bundles or redistributes certain third-party software and components subject to their respective open-source licenses. This document contains required notices, source code links, and licensing terms for distributed components.

---

## 1. mpv media player

- **Component:** `mpv` (and bundled support libraries `libbluray`, `libdvdnav`, `libdvdread`)
- **Version:** 0.39+ / 0.41+
- **License:** GNU General Public License version 2 or later (GPLv2+) / GNU Lesser General Public License version 2.1 or later (LGPLv2.1+)
- **Upstream Project & Source Code:** [https://github.com/mpv-player/mpv](https://github.com/mpv-player/mpv)
- **Windows Packaging & Build Recipe:** [https://github.com/shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake)
- **libbluray Project:** [https://code.videolan.org/videolan/libbluray](https://code.videolan.org/videolan/libbluray)
- **libdvdnav Project:** [https://code.videolan.org/videolan/libdvdnav](https://code.videolan.org/videolan/libdvdnav)

### Architecture & Aggregation Note

Nebulae executes `mpv` strictly as an independent, detached subprocess via standard operating system process launch APIs (`std::process::Command`). Nebulae does not statically or dynamically link to `libmpv` or call mpv C/C++ runtime internals from the main `nebulae` binary. The mpv binary and its runtime libraries are distributed alongside Nebulae as an independent aggregation.

Source code corresponding to the distributed mpv binary and build configurations is available at the upstream repositories linked above.

---

### GNU GENERAL PUBLIC LICENSE (Version 2, June 1991)

```
                    GNU GENERAL PUBLIC LICENSE
                       Version 2, June 1991

 Copyright (C) 1989, 1991 Free Software Foundation, Inc.
 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301  USA
 Everyone is permitted to copy and distribute verbatim copies
 of this license document, but changing it is not allowed.

                            Preamble

  The licenses for most software are designed to take away your
freedom to share and change it.  By contrast, the GNU General Public
License is intended to guarantee your freedom to share and change free
software--to make sure the software is free for all its users.  This
General Public License applies to most of the Free Software
Foundation's software and to any other program whose authors commit to
using it.  (Some other Free Software Foundation software is covered by
the GNU Lesser General Public License instead.)  You can apply it to
your programs, too.

  When we speak of free software, we are referring to freedom, not
price.  Our General Public Licenses are designed to make sure that you
have the freedom to distribute copies of free software (and charge for
this service if you wish), that you receive source code or can get it
if you want it, that you can change the software or use pieces of it
in new free programs; and that you know you can do these things.

  To protect your rights, we need to make restrictions that forbid
anyone to deny you these rights or to ask you to surrender the rights.
These restrictions translate to certain responsibilities for you if you
distribute copies of the software, or if you modify it.

  For example, if you distribute copies of such a program, whether
gratis or for a fee, you must give the recipients all the rights that
you have.  You must make sure that they, too, receive or can get the
source code.  And you must show them these terms so they know their
rights.

  We protect your rights with two steps: (1) copyright the software, and
(2) offer you this license which gives you legal permission to copy,
distribute and/or modify the software.

  Also, for each author's protection and ours, we want to make certain
that everyone understands that there is no warranty for this free
software.  If the software is modified by someone else and passed on, we
want its recipients to know that what they have is not the original, so
that any problems introduced by others will not reflect on the original
authors' reputations.

  Finally, any free program is threatened constantly by software
patents.  We wish to avoid the danger that redistributors of a free
program will individually obtain patent licenses, in effect making the
program proprietary.  To prevent this, we have made it clear that any
patent must be licensed for everyone's free use or not licensed at all.

  The precise terms and conditions for copying, distribution and
modification follow.

                    GNU GENERAL PUBLIC LICENSE
   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION

  0. This License applies to any program or other work which contains
a notice placed by the copyright holder saying it may be distributed
under the terms of this General Public License.  The "Program", below,
refers to any such program or work, and a "work based on the Program"
means either the Program or any derivative work under copyright law:
that is to say, a work containing the Program or a portion of it,
either verbatim or with modifications and/or translated into another
language.  (Hereinafter, translation is included without limitation in
the term "modification".)  Each licensee is addressed as "you".

Activities other than copying, distribution and modification are not
covered by this License; they are outside its scope.  The act of
running the Program is not restricted, and the output from the Program
is covered only if its contents constitute a work based on the
Program (independent of having been made by running the Program).
Whether that is true depends on what the Program does.

  1. You may copy and distribute verbatim copies of the Program's
source code as you receive it, in any medium, provided that you
conspicuously and appropriately publish on each copy an appropriate
copyright notice and disclaimer of warranty; keep intact all the
notices that refer to this License and to the absence of any warranty;
and give any other recipients of the Program a copy of this License
along with the Program.

You may charge a fee for the physical act of transferring a copy, and
you may at your option offer warranty protection in exchange for a fee.

  2. You may modify your copy or copies of the Program or any portion
of it, thus forming a work based on the Program, and copy and
distribute such modifications or work under the terms of Section 1
above, provided that you also meet all of these conditions:

    a) You must cause the modified files to carry prominent notices
    stating that you changed the files and the date of any change.

    b) You must cause any work that you distribute or publish, that in
    whole or in part contains or is derived from the Program or any
    part thereof, to be licensed as a whole at no charge to all third
    parties under the terms of this License.

    c) If the modified program normally reads commands interactively
    when run, you must cause it, when started running for such
    interactive use in the most ordinary way, to print or display an
    announcement including an appropriate copyright notice and a
    notice that there is no warranty (or else, saying that you provide
    a warranty) and that users may redistribute the program under
    these conditions, and telling the user how to view a copy of this
    License.  (Exception: if the Program itself is interactive but
    does not normally print such an announcement, your work based on
    the Program is not required to print an announcement.)

These requirements apply to the modified work as a whole.  If
identifiable sections of that work are not derived from the Program,
and can be reasonably considered independent and separate works in
themselves, then this License, and its terms, do not apply to those
sections when you distribute them as separate works.  But when you
distribute the same sections as part of a whole which is a work based
on the Program, the distribution of the whole must be on the terms of
this License, whose permissions for other licensees extend to the
entire whole, and thus to each and every part regardless of who wrote it.

Thus, it is not the intent of this section to claim rights or contest
your rights to work written entirely by you; rather, the intent is to
exercise the right to control the distribution of derivative or
collective works based on the Program.

In addition, mere aggregation of another work not based on the Program
with the Program (or with a work based on the Program) on a volume of
a storage or distribution medium does not bring the other work under
the scope of this License.

  3. You may copy and distribute the Program (or a work based on it,
under Section 2) in object code or executable form under the terms of
Sections 1 and 2 above provided that you also do one of the following:

    a) Accompany it with the complete corresponding machine-readable
    source code, which must be distributed under the terms of Sections
    1 and 2 above on a medium customarily used for software interchange; or,

    b) Accompany it with a written offer, valid for at least three
    years, to give any third party, for a charge no more than your
    cost of physically performing source distribution, a complete
    machine-readable copy of the corresponding source code, to be
    distributed under the terms of Sections 1 and 2 above on a medium
    customarily used for software interchange; or,

    c) Accompany it with the information you received as to the offer
    to distribute corresponding source code.  (This alternative is
    allowed only for noncommercial distribution and only if you
    received the program in object code or executable form with such
    an offer, in accord with Subsection b above.)

The source code for a work means the preferred form of the work for
making modifications to it.  For an executable work, complete source
code means all the source code for all modules it contains, plus any
associated interface definition files, plus the scripts used to
control compilation and installation of the executable.  However, as a
special exception, the source code distributed need not include
anything that is normally distributed (in either source or binary
form) with the major components (compiler, kernel, and so on) of the
operating system on which the executable runs, unless that component
itself accompanies the executable.

If distribution of executable or object code is made by offering
access to copy from a designated place, then offering equivalent
access to copy the source code from the same place counts as
distribution of the source code, even though third parties are not
compelled to copy the source along with the object code.

  4. You may not copy, modify, sublicense, or distribute the Program
except as expressly provided under this License.  Any attempt
otherwise to copy, modify, sublicense or distribute the Program is
void, and will automatically terminate your rights under this License.
However, parties who have received copies, or rights, from you under
this License will not have their licenses terminated so long as such
parties remain in full compliance.

  5. You are not required to accept this License, since you have not
signed it.  However, nothing else grants you permission to modify or
distribute the Program or its derivative works.  These actions are
prohibited by law if you do not accept this License.  Therefore, by
modifying or distributing the Program (or any work based on the
Program), you indicate your acceptance of this License to do so, and
all its terms and conditions for copying, distributing or modifying
the Program or works based on it.

  6. Each time you redistribute the Program (or any work based on the
Program), the recipient automatically receives a license from the
original licensor to copy, distribute or modify the Program subject to
these terms and conditions.  You may not impose any further
restrictions on the recipients' exercise of the rights granted herein.
You are not responsible for enforcing compliance by third parties to
this License.

  7. If, as a consequence of a court judgment or allegation of patent
infringement or for any other reason (not limited to patent issues),
conditions are imposed on you (whether by court order, agreement or
otherwise) that contradict the conditions of this License, they do not
excuse you from the conditions of this License.  If you cannot
distribute so as to satisfy simultaneously your obligations under this
License and any other pertinent obligations, then as a consequence you
may not distribute the Program at all.  For example, if a patent
license would not permit royalty-free redistribution of the Program by
all those who receive copies directly or indirectly through you, then
the only way you could satisfy both it and this License would be to
refrain entirely from distribution of the Program.

If any portion of this section is held invalid or unenforceable under
any particular circumstance, the balance of the section is intended to
apply and the section as a whole is intended to apply in other
circumstances.

It is not the purpose of this section to induce you to infringe any
patents or other property right claims or to contest validity of any
such claims; this section has the sole purpose of protecting the
integrity of the free software distribution system, which is
implemented by public license practices.  Many people have made
generous contributions to the wide range of software distributed
through that system in reliance on consistent application of that
system; it is up to the author/donor to decide if he or she is willing
to distribute software through any other system and a licensee cannot
impose that choice.

This section is intended to make thoroughly clear what is believed to
be a consequence of the rest of this License.

  8. If the distribution and/or use of the Program is restricted in
certain countries either by patents or by copyrighted interfaces, the
original copyright holder who places the Program under this License
may add an explicit geographical distribution limitation excluding
those countries, so that distribution is permitted only in or among
countries not thus excluded.  In such case, this License incorporates
the limitation as if written in the body of this License.

  9. The Free Software Foundation may publish revised and/or new versions
of the General Public License from time to time.  Such new versions will
be similar in spirit to the present version, but may differ in detail to
address new problems or concerns.

Each version is given a distinguishing version number.  If the Program
specifies a version number of this License which applies to it and "any
later version", you have the option of following the terms and conditions
either of that version or of any later version published by the Free
Software Foundation.  If the Program does not specify a version number of
this License, you may choose any version ever published by the Free Software
Foundation.

  10. If you wish to incorporate parts of the Program into other free
programs whose distribution conditions are different, write to the author
to ask for permission.  For software which is copyrighted by the Free
Software Foundation, write to the Free Software Foundation; we sometimes
make exceptions for this.  Our decision will be guided by the two goals
of preserving the free status of all derivatives of our free software and
of promoting the sharing and reuse of software generally.

                            NO WARRANTY

  11. BECAUSE THE PROGRAM IS LICENSED FREE OF CHARGE, THERE IS NO WARRANTY
FOR THE PROGRAM, TO THE EXTENT PERMITTED BY APPLICABLE LAW.  EXCEPT WHEN
OTHERWISE STATED IN WRITING THE COPYRIGHT HOLDERS AND/OR OTHER PARTIES
PROVIDE THE PROGRAM "AS IS" WITHOUT WARRANTY OF ANY KIND, EITHER EXPRESSED
OR IMPLIED, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE.  THE ENTIRE RISK AS
TO THE QUALITY AND PERFORMANCE OF THE PROGRAM IS WITH YOU.  SHOULD THE
PROGRAM PROVE DEFECTIVE, YOU ASSUME THE COST OF ALL NECESSARY SERVICING,
REPAIR OR CORRECTION.

  12. IN NO EVENT UNLESS REQUIRED BY APPLICABLE LAW OR AGREED TO IN WRITING
WILL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY WHO MAY MODIFY AND/OR
REDISTRIBUTE THE PROGRAM AS PERMITTED ABOVE, BE LIABLE TO YOU FOR DAMAGES,
INCLUDING ANY GENERAL, SPECIAL, INCIDENTAL OR CONSEQUENTIAL DAMAGES ARISING
OUT OF THE USE OR INABILITY TO USE THE PROGRAM (INCLUDING BUT NOT LIMITED
TO LOSS OF DATA OR DATA BEING RENDERED INACCURATE OR LOSSES SUSTAINED BY
YOU OR THIRD PARTIES OR A FAILURE OF THE PROGRAM TO OPERATE WITH ANY OTHER
PROGRAMS), EVEN IF SUCH HOLDER OR OTHER PARTY HAS BEEN ADVISED OF THE
POSSIBILITY OF SUCH DAMAGES.

                     END OF TERMS AND CONDITIONS
```

---

## 2. Bundled Java Runtime (BD-J / UHD menu support)

- **Component:** minimal Java runtime image produced with `jlink` from **Eclipse Temurin** (OpenJDK 21), shipped inside the package as `resources/player/jre` on Windows, macOS, and Linux
- **License:** GNU General Public License version 2 with the Classpath Exception (GPLv2+CE)
- **Upstream Project & Source Code:** [https://openjdk.org/](https://openjdk.org/) / [https://github.com/adoptium/temurin21-binaries](https://github.com/adoptium/temurin21-binaries)
- **Purpose:** libbluray requires a JVM to run BD-J interactive menus, including Java-based menus on 4K UHD discs. The bundled runtime is used by the bundled mpv player only; Nebulae itself does not use Java.

The OpenJDK source code corresponding to the bundled runtime is available at the Adoptium repository linked above.

---

## 3. Bundled shared-library tree (macOS and Linux player bundle)

The bundled player is self-contained: on macOS and Linux the package carries the complete set of non-system shared libraries that the bundled mpv links against (discovered at build time from the pinned build host and copied into the package with their load paths rewritten). These libraries originate from their respective upstream projects and are redistributed under their own licenses. The principal components and licenses include:

| Library | Typical license | Upstream |
| --- | --- | --- |
| FFmpeg (libav*) | LGPLv2.1+ / GPLv2+ depending on build configuration | https://ffmpeg.org/ |
| libass | ISC | https://github.com/libass/libass |
| libplacebo | LGPLv2.1+ | https://code.videolan.org/videolan/libplacebo |
| fontconfig, freetype | MIT / FTL | https://www.freedesktop.org/wiki/Software/fontconfig/ , https://www.freetype.org/ |
| HarfBuzz, fribidi | MIT / LGPLv2.1+ | https://harfbuzz.github.io/ , https://github.com/fribidi/fribidi |
| zlib, zstd, brotli, xz, lz4 | zlib / BSD / MIT / public domain | upstream project sites |
| expat, libxml2, libxslt | MIT / MIT | https://github.com/libexpat/ |
| OpenSSL / GnuTLS / mbedtls (as linked) | Apache-2.0 / LGPLv2.1+ / Apache-2.0 | upstream project sites |

The exact set varies with the pinned build; the corresponding source for every bundled library is obtainable from the upstream project of each library as linked by the build recipes above (shinchiro/mpv-winbuild-cmake for Windows; the distribution source packages of the build host for macOS/Linux trees). No library bundled this way is modified by Nebulae beyond binary load-path rewriting (`install_name_tool` / `patchelf`), which does not constitute a modification of the work under the applicable licenses. As above, the player is executed as an independent subprocess; nothing in this tree is linked into the Nebulae binary.

Nebulae does not bundle VLC, IINA, or any DRM/circumvention component (no libaacs, no key databases).

---

## 2. Transmission BitTorrent Client (transmission-daemon)

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

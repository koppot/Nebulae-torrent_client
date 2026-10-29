#!/usr/bin/env python3
"""Stage the Windows x64 runtime tree from the pinned inputs of inputs.lock.json.

Produces the directory that becomes `{install dir}\\resources` plus, when --exe is
given, the app executables, and proves before anything is bundled:

- every PE import resolves inside the tree or against Windows itself
  (scripts/windows/pe_audit.py, layout "nebulae");
- the Visual C++ runtime the Transmission daemon needs sits next to it
  (extracted from the pinned VC_redist.x64.exe, never executed);
- a patched VLC overlay really carries the three Nebulae libVLC entry points,
  and a stock staging provably does not;
- licence texts of every bundled component are present;
- a build manifest with the SHA-256 of every staged file is written.

The WebView2 offline installer is an installer-time input, not part of this
tree; the candidate workflow pre-seeds it for the bundler from the same lock.

    stage_runtime.py --cache DIR --out DIR [--patched-vlc DIR] [--require-patched]
                     [--exe FILE ...] [--manifest PATH] [--cab-tool PATH]
                     [--offline] [--no-audit]

Nothing outside --out (and the cache when not --offline) is written.
"""
from __future__ import annotations

import argparse
import datetime
import io
import json
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fetch_inputs  # noqa: E402
import msi_reader  # noqa: E402
import pe_audit  # noqa: E402

VLC_PREFIX = "vlc-3.0.23/"
# Parts of the official win64 zip the app never loads: the msi wrapper bits,
# the skins2 interface assets, the ActiveX and Netscape plugins and the two
# stand-alone executables. The plugin tree itself is left untouched.
VLC_DENY_DIRS = ("msi/", "skins/")
VLC_DENY_FILES = {"axvlc.dll", "npvlc.dll", "vlc.exe", "vlc-cache-gen.exe", "vlc.ico"}

STOCK_BD_JARS = ("libbluray-j2se-1.4.0.jar", "libbluray-awt-j2se-1.4.0.jar")
PATCHED_OVERLAY = (
    "libvlc.dll",
    "liblibbluray_plugin.dll",
    "libbluray-j2se-1.5.0.jar",
    "libbluray-awt-j2se-1.5.0.jar",
)
CUSTOM_EXPORTS = (
    "libvlc_media_player_navigate_menu",
    "libvlc_media_player_get_bluray_menu_open",
    "libvlc_media_player_get_bluray_playlist_id",
)

TRANSMISSION_FILES = (
    "transmission-daemon.exe",
    "libcrypto-3-x64.dll",
    "libssl-3-x64.dll",
    "libcurl.dll",
    "zlib.dll",
)
TAILSCALE_FILES = ("tailscale.exe", "tailscaled.exe")

# App-local Visual C++ runtime placed next to the Transmission daemon. Names
# inside the redistributable's payload cab carry an "_amd64" suffix.
VCRT_DLLS = (
    "concrt140.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll",
    "msvcp140_atomic_wait.dll", "msvcp140_codecvt_ids.dll", "vcamp140.dll",
    "vccorlib140.dll", "vcomp140.dll", "vcruntime140.dll", "vcruntime140_1.dll",
    "vcruntime140_threads.dll",
)


class StageError(RuntimeError):
    pass


def fail(msg: str) -> None:
    raise StageError(msg)


def sha256_file(path: Path) -> str:
    return fetch_inputs.sha256_file(path)


class Recorder:
    """Every file written under the staging root, with provenance."""

    def __init__(self, root: Path):
        self.root = root
        self.files: list[dict] = []
        self.seen: set[str] = set()

    def rel(self, path: Path) -> str:
        return path.relative_to(self.root).as_posix()

    def add(self, path: Path, source: str) -> None:
        rel = self.rel(path)
        if rel in self.seen:
            fail(f"{rel}: staged twice")
        self.seen.add(rel)
        self.files.append({
            "path": rel,
            "sha256": sha256_file(path),
            "bytes": path.stat().st_size,
            "source": source,
        })

    def copy(self, src: Path, dst: Path, source: str) -> None:
        if dst.exists():
            fail(f"{self.rel(dst)}: already exists")
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
        self.add(dst, source)

    def write(self, rel: str, data: bytes, source: str) -> Path:
        dst = self.root / msi_reader.safe_rel(rel)
        if dst.exists():
            fail(f"{rel}: already exists")
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(data)
        self.add(dst, source)
        return dst


# --- safe archive extraction --------------------------------------------------

def _check_member_name(name: str) -> PurePosixPath:
    p = PurePosixPath(name)
    if (p.is_absolute() or ".." in p.parts or any("\\" in part or ":" in part for part in p.parts)):
        fail(f"archive member {name!r} has an unsafe path")
    return p


def extract_zip(zip_path: Path, dest: Path, strip_prefix: str, rec: Recorder, source: str,
                deny_dirs: tuple[str, ...] = (), deny_files: frozenset[str] | set = frozenset()) -> int:
    """Extract members under STRIP_PREFIX into DEST. Returns the file count."""
    count = 0
    with zipfile.ZipFile(zip_path) as zf:
        for info in zf.infolist():
            if info.is_dir():
                continue
            if (info.external_attr >> 16) & 0o170000 == 0o120000:
                fail(f"{zip_path.name}: symlink member {info.filename!r} is not allowed")
            rel = _check_member_name(info.filename)
            if not str(rel).startswith(strip_prefix):
                continue
            sub = str(rel)[len(strip_prefix):]
            if not sub or sub.startswith(deny_dirs):
                continue
            if "/" not in sub and sub in deny_files:  # top-level files only
                continue
            target = dest / sub
            target.parent.mkdir(parents=True, exist_ok=True)
            with zf.open(info) as fin, open(target, "wb") as fout:
                shutil.copyfileobj(fin, fout, 1 << 20)
            rec.add(target, source)
            count += 1
    return count


def extract_tar_members(tar_path: Path, wanted: dict[str, str], rec: Recorder, source: str) -> None:
    """Copy selected members of a tarball to recorded files.

    WANTED maps a member suffix ('vlc-3.0.23/COPYING') to the staged relative path.
    """
    remaining = dict(wanted)
    with tarfile.open(tar_path) as tf:
        for member in tf:
            if not member.isfile():
                continue
            rel = str(_check_member_name(member.name))
            for suffix, dest_rel in list(remaining.items()):
                if rel == suffix or rel.endswith("/" + suffix):
                    f = tf.extractfile(member)
                    assert f is not None
                    rec.write(dest_rel, f.read(), source)
                    del remaining[suffix]
                    break
    if remaining:
        fail(f"{tar_path.name}: members not found: {', '.join(sorted(remaining))}")


# --- component staging ---------------------------------------------------------

def stage_vlc(cache: Path, inputs: dict, rec: Recorder, patched: Path | None, patches_dir: Path | None) -> bool:
    src = fetch_inputs.ensure("vlc-win64", inputs["vlc-win64"], cache)
    dest = rec.root / "resources" / "vlc"
    n = extract_zip(src, dest, VLC_PREFIX, rec, "vlc-3.0.23-win64.zip (official, SHA-256 pinned)",
                    VLC_DENY_DIRS, VLC_DENY_FILES)
    if n < 500:
        fail(f"suspiciously few VLC files staged: {n}")
    for need in ("libvlc.dll", "libvlccore.dll", "plugins/access/liblibbluray_plugin.dll", "COPYING.txt"):
        if not (dest / need).is_file():
            fail(f"staged VLC tree lacks {need}")
    if not patched:
        return False

    for name in PATCHED_OVERLAY:
        if not (patched / name).is_file():
            fail(f"patched VLC overlay {patched} lacks {name}")
    access = dest / "plugins" / "access"

    def unstage(path: Path) -> None:
        rel = rec.rel(path)
        rec.seen.discard(rel)
        rec.files = [f for f in rec.files if f["path"] != rel]
        path.unlink()

    for jar in STOCK_BD_JARS:
        if not (access / jar).is_file():
            fail(f"stock jar {jar} not found; the official zip layout changed")
    unstage(access / STOCK_BD_JARS[0])
    unstage(access / STOCK_BD_JARS[1])
    unstage(dest / "libvlc.dll")
    unstage(access / "liblibbluray_plugin.dll")
    replaced = dest / "libvlc.dll"
    plugin = access / "liblibbluray_plugin.dll"

    provenance = _overlay_provenance(patched)
    rec.copy(patched / "libvlc.dll", replaced, provenance)
    rec.copy(patched / "liblibbluray_plugin.dll", plugin, provenance)
    for jar in PATCHED_OVERLAY[2:]:
        rec.copy(patched / jar, access / jar, provenance)

    if patches_dir and patches_dir.is_dir():
        lic = "resources/licenses/vlc-3.0.23/patches"
        for patch in sorted(patches_dir.glob("*.patch")):
            rec.copy(patch, rec.root / lic / patch.name, "Nebulae patch (this repository)")
        readme = patches_dir / "README.md"
        if readme.is_file():
            rec.copy(readme, rec.root / lic / readme.name, "Nebulae patch (this repository)")
    return True


def _overlay_provenance(patched: Path) -> str:
    note = patched / "patched-vlc-manifest.json"
    if note.is_file():
        try:
            meta = json.loads(note.read_text(encoding="utf-8"))
            return (f"custom-build: VLC {meta.get('vlc_version', '?')} + patches "
                    f"{', '.join(meta.get('patches', []))} ({meta.get('toolchain', 'toolchain unknown')})")
        except (json.JSONDecodeError, OSError):
            pass
    return f"custom-build from {patched} (no patched-vlc-manifest.json; provenance incomplete)"


def stage_jre(cache: Path, inputs: dict, rec: Recorder) -> None:
    src = fetch_inputs.ensure("temurin-jre", inputs["temurin-jre"], cache)
    dest = rec.root / "resources" / "jre"
    with zipfile.ZipFile(src) as zf:
        tops = {PurePosixPath(n).parts[0] for n in zf.namelist() if not n.endswith("/")}
    if len(tops) != 1:
        fail(f"Temurin zip has {len(tops)} top-level entries, expected exactly one")
    n = extract_zip(src, dest, next(iter(tops)) + "/", rec,
                    "Eclipse Temurin JRE 21.0.12+8 (official, SHA-256 pinned)")
    for need in ("bin/java.exe", "bin/server/jvm.dll", "NOTICE", "legal", "release"):
        if not (dest / need).exists():
            fail(f"staged JRE lacks {need}")
    if n < 250:
        fail(f"suspiciously few JRE files staged: {n}")


def stage_transmission(cache: Path, inputs: dict, rec: Recorder, cab_tool: list[str]) -> None:
    src = fetch_inputs.ensure("transmission-msi", inputs["transmission-msi"], cache)
    dest = rec.root / "resources" / "engine" / "transmission"
    with tempfile.TemporaryDirectory(prefix="nebulae-msi-") as tmp:
        msi_reader.extract_msi(src, Path(tmp), cab_tool)
        base = Path(tmp) / "Transmission"
        for name in TRANSMISSION_FILES:
            if not (base / name).is_file():
                fail(f"{name} not found in the Transmission MSI")
            rec.copy(base / name, dest / name,
                     "transmission-4.1.3-x64.msi (official, SHA-256 pinned)")


def stage_tailscale(cache: Path, inputs: dict, rec: Recorder, cab_tool: list[str]) -> None:
    src = fetch_inputs.ensure("tailscale-msi", inputs["tailscale-msi"], cache)
    dest = rec.root / "resources" / "engine" / "tailscale"
    with tempfile.TemporaryDirectory(prefix="nebulae-msi-") as tmp:
        msi_reader.extract_msi(src, Path(tmp), cab_tool)
        base = Path(tmp) / "Tailscale"
        for name in TAILSCALE_FILES:
            if not (base / name).is_file():
                fail(f"{name} not found in the Tailscale MSI")
            rec.copy(base / name, dest / name,
                     "tailscale-setup-1.104.1-amd64.msi (official, SHA-256 pinned)")


# --- Visual C++ runtime --------------------------------------------------------

def _find_cabs(data: bytes) -> list[tuple[int, int]]:
    """(offset, size) of every plausible cabinet inside a Burn bundle .exe."""
    out = []
    pos = 0
    while True:
        pos = data.find(b"MSCF", pos)
        if pos < 0:
            return out
        if pos + 36 <= len(data):
            (_sig, _r1, size, _r2, _coff, _r3,
             minor, major, _nfolders, _nfiles) = struct.unpack_from("<IIIIIIBBHH", data, pos)
            if (major, minor) == (1, 3) and 44 < size <= len(data) - pos:
                out.append((pos, size))
        pos += 4


def stage_vcredist(cache: Path, inputs: dict, rec: Recorder, cab_tool: list[str]) -> None:
    src = fetch_inputs.ensure("vc-redist-x64", inputs["vc-redist-x64"], cache)
    dest = rec.root / "resources" / "engine" / "transmission"
    cabs = _find_cabs(src.read_bytes())
    if len(cabs) < 2:
        fail(f"VC_redist.x64.exe: expected at least 2 embedded cabinets, found {len(cabs)}")
    source = "VC_redist.x64.exe 14.44.35211.0 (official, SHA-256 pinned; extracted, never executed)"
    license_done = False
    runtime_done = False
    with tempfile.TemporaryDirectory(prefix="nebulae-vcrt-") as tmp_s:
        tmp = Path(tmp_s)
        # The Burn bundle nests its payload cabs: the outer cabs hold the UX
        # files (the licence text) and numbered payload files, one of which is
        # itself the cab with the runtime DLLs. Walk a worklist, depth-limited.
        worklist: list[tuple[Path, int]] = []
        for i, (off, size) in enumerate(cabs):
            cab = tmp / f"attached-{i}.cab"
            with open(src, "rb") as fh:
                fh.seek(off)
                cab.write_bytes(fh.read(size))
            worklist.append((cab, 0))
        seen: set[int] = set()
        n = 0
        while worklist and not (runtime_done and license_done):
            cab, depth = worklist.pop(0)
            digest = hash(cab.stat().st_size)
            if digest in seen or depth > 3:
                continue
            seen.add(digest)
            out = tmp / f"cab-{n}"
            n += 1
            try:
                msi_reader.extract_cab(cab, out, cab_tool)
            except msi_reader.MsiError:
                continue  # not a plain cabinet payload; try the next one
            entries = {p.name: p for p in out.iterdir() if p.is_file()}
            if "vcruntime140.dll_amd64" in entries and not runtime_done:
                missing = [d for d in VCRT_DLLS if d + "_amd64" not in entries]
                if missing:
                    fail(f"VC runtime payload cab lacks: {', '.join(missing)}")
                for name in VCRT_DLLS:
                    target = dest / name
                    rec.copy(entries[name + "_amd64"], target, source)
                    if pe_audit.parse_pe(target.read_bytes()).arch != "x64":
                        fail(f"{name}: not an x64 PE")
                runtime_done = True
                continue
            for p in entries.values():
                head = p.read_bytes()[:65536]
                if (not license_done and head.startswith(b"{\\rtf")
                        and b"MICROSOFT SOFTWARE LICENSE TERMS" in head and b"VISUAL C++" in head.upper()):
                    rec.copy(p, rec.root / "resources" / "licenses" / "microsoft-vc-redist-license.rtf", source)
                    license_done = True
                elif head.startswith(b"MSCF"):
                    worklist.append((p, depth + 1))
    if not runtime_done:
        fail("no cabinet with the x64 VC runtime DLLs was found inside VC_redist.x64.exe")
    if not license_done:
        fail("the Microsoft software licence text was not found inside VC_redist.x64.exe")


# --- licences ------------------------------------------------------------------

LICENSE_README = """\
Nebulae for Windows - third-party licences
==========================================

Every directory here holds the licence text of one bundled component, copied
from the pinned, hash-verified upstream input named in build-manifest.json
(one level up). The index of components, versions, upstream projects and
licence identifiers is THIRD-PARTY-NOTICES.md at
https://github.com/koppot/Nebulae-torrent_client (and docs/windows in the
source tree of this build).

- vlc-3.0.23/            VLC media player: GPL-2.0+ (COPYING) and LGPL-2.1+
                         (COPYING.LIB, covers libvlc/libvlccore). patches/
                         holds the Nebulae patches when this build's
                         libvlc.dll and Blu-ray plugin are the patched ones.
- libbluray-1.5.0/       Blu-ray access library of the patched Blu-ray plugin,
                         LGPL-2.1+. The stock VLC Blu-ray plugin embeds
                         libbluray 1.4.0 under the same licence family.
- transmission-4.1.3/    transmission-daemon: GPL-2.0 or GPL-3.0 with an
                         OpenSSL linking exception.
- curl-8.16.0/           libcurl.dll shipped by the Transmission MSI.
- openssl-3.5.4/         libcrypto-3-x64.dll / libssl-3-x64.dll (Apache-2.0).
- zlib-1.3.1/            zlib.dll shipped by the Transmission MSI.
- tailscale-1.104.1/     tailscale.exe / tailscaled.exe (BSD-3-Clause).
- microsoft-vc-redist-license.rtf
                         Microsoft Visual C++ 2015-2022 runtime DLLs placed
                         next to the Transmission daemon, extracted verbatim
                         from Microsoft's own installer.
- Eclipse Temurin JRE 21.0.12+8 keeps its own notices in resources/jre/NOTICE
  and resources/jre/legal/ (GPL-2.0 with the Classpath Exception).

Source code for the LGPL/GPL components is available from the upstream
projects; exact source URLs and SHA-256 digests are recorded in
build-manifest.json and scripts/windows/inputs.lock.json of the source tree.
"""


def stage_licenses(cache: Path, inputs: dict, rec: Recorder, patched: bool) -> None:
    lic = rec.root / "resources" / "licenses"
    tar_src = fetch_inputs.ensure("vlc-src", inputs["vlc-src"], cache)
    extract_tar_members(tar_src, {
        "vlc-3.0.23/COPYING": "resources/licenses/vlc-3.0.23/COPYING",
        "vlc-3.0.23/COPYING.LIB": "resources/licenses/vlc-3.0.23/COPYING.LIB",
    }, rec, "vlc-3.0.23.tar.xz (official, SHA-256 pinned)")
    lb_src = fetch_inputs.ensure("libbluray-src", inputs["libbluray-src"], cache)
    extract_tar_members(lb_src, {
        "libbluray-1.5.0/COPYING": "resources/licenses/libbluray-1.5.0/COPYING",
    }, rec, "libbluray-1.5.0.tar.xz (official, SHA-256 pinned)")

    small = (
        ("license-transmission", "transmission-4.1.3/COPYING"),
        ("license-curl", "curl-8.16.0/COPYING"),
        ("license-openssl", "openssl-3.5.4/LICENSE.txt"),
        ("license-zlib", "zlib-1.3.1/LICENSE"),
        ("license-tailscale", "tailscale-1.104.1/LICENSE"),
    )
    for name, dest_rel in small:
        entry = inputs[name]
        rec.copy(fetch_inputs.ensure(name, entry, cache), lic / dest_rel,
                 f"{entry['file']} (upstream licence text, SHA-256 pinned)")
    rec.write("resources/licenses/README.txt", LICENSE_README.encode(), "Nebulae (this build)")
    if not patched:
        rec.write("resources/licenses/vlc-3.0.23/STOCK-RUNTIME.txt",
                  b"This build ships the unmodified official VLC 3.0.23 win64 runtime.\n"
                  b"The Nebulae Blu-ray menu patches are NOT part of it; the Blu-ray plugin\n"
                  b"embeds libbluray 1.4.0 (LGPL-2.1+), whose licence text is alongside.\n",
                  "Nebulae (this build)")


# --- audit + manifest ------------------------------------------------------------

def audit_tree(rec: Recorder, patched: bool) -> dict:
    report = pe_audit.audit(rec.root, [], "x64", pe_audit.NEBULAE_LAYOUT, {})
    problems = []
    if report["missing"]:
        problems.append(f"unresolved imports: {sorted(report['missing'])}")
    if report["misplaced"]:
        problems.append(f"misplaced DLLs: {sorted(report['misplaced'])}")
    if report["vcredist"]:
        problems.append(f"Visual C++ runtime still unresolved: {sorted(report['vcredist'])}")
    if report["wrong_arch"]:
        problems.append("not x64: " + ", ".join(w["path"] for w in report["wrong_arch"]))
    if report["unreadable"]:
        problems.append("unreadable PE: " + ", ".join(u["path"] for u in report["unreadable"]))

    libvlc = rec.root / "resources" / "vlc" / "libvlc.dll"
    exports = pe_audit.parse_pe(libvlc.read_bytes()).exports if libvlc.is_file() else []
    if "libvlc_media_player_navigate" not in exports:
        problems.append("libvlc.dll does not export libvlc_media_player_navigate; wrong file?")
    missing_custom = [s for s in CUSTOM_EXPORTS if s not in exports]
    if patched and missing_custom:
        problems.append(f"patched libvlc.dll lacks the Nebulae entry points: {missing_custom}")
    if not patched and not missing_custom:
        problems.append("stock libvlc.dll unexpectedly exports the Nebulae entry points")
    if problems:
        fail("audit failed; manifest not written:\n" + "\n".join(f"  - {p}" for p in problems))
    print(pe_audit._summary(report))
    return report


def write_manifest(rec: Recorder, inputs: dict, patched: bool, report: dict | None,
                   warnings: list[str], path: Path) -> None:
    try:
        rev = subprocess.run(["git", "-C", str(Path(__file__).resolve().parents[2]),
                              "rev-parse", "HEAD"], capture_output=True, text=True, timeout=15)
        git_rev = rev.stdout.strip() if rev.returncode == 0 else "unknown"
    except (OSError, subprocess.SubprocessError):
        git_rev = "unknown"
    manifest = {
        "schema": 1,
        "platform": "windows-x86_64",
        "staged_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "git_rev": git_rev,
        "vlc_version": "3.0.23",
        "patched_vlc": patched,
        "menu_repairs": "active" if patched else "inactive (stock libvlc)",
        "inputs": {n: {k: e[k] for k in ("file", "url", "sha256", "bytes", "version", "license")}
                   for n, e in sorted(inputs.items())},
        "files": sorted(rec.files, key=lambda f: f["path"]),
        "pe_audit": None if report is None else {
            "pe_files": report["pe_files"],
            "missing": sorted(report["missing"]),
            "misplaced": sorted(report["misplaced"]),
            "vcredist": sorted(report["vcredist"]),
            "wrong_arch": report["wrong_arch"],
        },
        "warnings": warnings,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, indent=1, sort_keys=False) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache", type=Path, required=True, help="pinned-input cache (see fetch_inputs.py)")
    ap.add_argument("--out", type=Path, required=True, help="staging root; must not exist or be empty")
    ap.add_argument("--lock", type=Path, default=fetch_inputs.DEFAULT_LOCK)
    ap.add_argument("--patched-vlc", type=Path, help="directory with the patched libvlc.dll, "
                    "liblibbluray_plugin.dll and the two libbluray 1.5.0 jars")
    ap.add_argument("--require-patched", action="store_true", help="fail instead of warning on a stock runtime")
    ap.add_argument("--patches-dir", type=Path,
                    default=Path(__file__).resolve().parents[2] / "patches" / "vlc-3.0.23",
                    help="copied into the licence tree when a patched overlay is staged")
    ap.add_argument("--exe", type=Path, action="append", default=[],
                    help="app executable to include in the tree and the audit (e.g. nebulae.exe)")
    ap.add_argument("--manifest", type=Path, help="defaults to <out>/resources/build-manifest.json")
    ap.add_argument("--cab-tool", help="7-Zip executable; auto-detected otherwise")
    ap.add_argument("--offline", action="store_true", help="use only what is already in the cache")
    ap.add_argument("--no-audit", action="store_true", help="skip the PE audit (experiments only)")
    args = ap.parse_args(argv)

    warnings: list[str] = []
    inputs = fetch_inputs.load_lock(args.lock)
    cab_tool = msi_reader.find_cab_tool(args.cab_tool)
    out: Path = args.out
    if out.exists() and any(out.iterdir()):
        fail(f"--out {out} is not empty; refusing to overlay into it")
    out.mkdir(parents=True, exist_ok=True)
    rec = Recorder(out)

    if args.patched_vlc is None and args.require_patched:
        fail("--require-patched was given but no --patched-vlc overlay")
    for exe in args.exe:
        if not exe.is_file():
            fail(f"--exe {exe} not found")
        rec.copy(exe, out / exe.name, "Nebulae build output (this repository)")

    patched = stage_vlc(args.cache, inputs, rec, args.patched_vlc, args.patches_dir)
    if not patched:
        warnings.append("stock VLC staged: the Blu-ray menu repairs (M-01..M-03) are INACTIVE in this tree")
    stage_jre(args.cache, inputs, rec)
    stage_transmission(args.cache, inputs, rec, cab_tool)
    stage_tailscale(args.cache, inputs, rec, cab_tool)
    stage_vcredist(args.cache, inputs, rec, cab_tool)
    stage_licenses(args.cache, inputs, rec, patched)
    (out / "resources" / "geo").mkdir(exist_ok=True)
    warnings.append("resources/geo holds no GeoLite2 data (needs a MaxMind licence key at build time)")
    if not args.exe:
        warnings.append("no --exe given: the audit covers resources only, not nebulae.exe")

    report = None if args.no_audit else audit_tree(rec, patched)
    manifest_path = args.manifest or (out / "resources" / "build-manifest.json")
    write_manifest(rec, inputs, patched, report, warnings, manifest_path)
    total = sum(f["bytes"] for f in rec.files)
    print(f"staged {len(rec.files)} files, {total / 1e6:.1f} MB, patched_vlc={patched}")
    print(f"manifest: {manifest_path}")
    for w in warnings:
        print(f"warning: {w}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (StageError, fetch_inputs.FetchError, msi_reader.MsiError, pe_audit.PeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Bundle the non-system dependencies of Mach-O files and rewrite every reference.

For each TARGET (an executable, plugin or dylib already staged inside a bundle):

  * every dependency that is not a system library is copied, together with its
    own dependencies, into --lib-dir under the file name the dependent used;
  * every load command that referenced such a library is rewritten with
    install_name_tool -change, using the exact string stored in the binary. A
    path that went through realpath() first never matches what the binary holds
    (Homebrew records .../opt/<formula>/lib/..., a symlink), which is how a
    bundled daemon kept loading the build machine's libraries;
  * every bundled library gets the install name @rpath/<name> and an
    @loader_path rpath, so its own dependencies resolve next to it;
  * absolute (build machine) rpaths are removed, and the --add-rpath entries
    are added to the targets;
  * everything that was modified is signed again (install_name_tool invalidates
    the signature and arm64 refuses to run an image with an invalid one);
  * the result is verified: no external absolute path may remain.

Only copies are modified. The source libraries (for example the Homebrew
cellar) are never touched. Running the tool again on an already relocated tree
is harmless.

Usage:
  relocate-macos-dylibs.py --lib-dir DIR [--add-rpath RPATH]... [--sign ID]
                           [--manifest FILE] [--licenses-dir DIR] TARGET...
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
from collections import OrderedDict, deque

from macho_tools import SYSTEM_PREFIXES, archs_of, is_macho, parse_load_commands, run_checked

CELLAR_RE = re.compile(r"/Cellar/([^/]+)/([^/]+)/")
LICENSE_RE = re.compile(r"^(COPYING|LICEN[CS]E|COPYRIGHT|NOTICE|AUTHORS|UNLICENSE)([._-].*)?$", re.I)


class RelocationError(Exception):
    pass


class Image:
    """One file that gets modified: a target or a bundled library."""

    def __init__(self, path: str, src: str, is_lib: bool, root: "Image | None"):
        self.path = path  # file that will be modified (a copy inside the bundle)
        self.src = src  # file its load commands are read from
        self.is_lib = is_lib
        self.root = root or self
        self.info: dict = {}
        self.changes: list[tuple[str, str]] = []  # (exact old string, new string)

    @property
    def name(self) -> str:
        return os.path.basename(self.path)


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def is_system(ref: str) -> bool:
    return ref.startswith(SYSTEM_PREFIXES)


def expand(entry: str, loader_dir: str, exec_dir: str) -> str:
    if entry.startswith("@loader_path"):
        return os.path.normpath(loader_dir + entry[len("@loader_path"):])
    if entry.startswith("@executable_path"):
        return os.path.normpath(exec_dir + entry[len("@executable_path"):])
    return os.path.normpath(entry)


def resolve(ref: str, img: Image, lib_dir: str) -> "str | None":
    """Return the real file a load-command string refers to, or None."""
    src_dir = os.path.dirname(img.src)
    root_dir = os.path.dirname(img.root.src)
    if ref.startswith("@rpath/"):
        tail = ref[len("@rpath/"):]
        dirs = [expand(rp, src_dir, root_dir) for rp in img.info["rpaths"]]
        dirs += [expand(rp, root_dir, root_dir) for rp in img.root.info["rpaths"]]
        dirs.append(lib_dir)
        for d in dirs:
            cand = os.path.join(d, tail)
            if os.path.isfile(cand):
                return os.path.realpath(cand)
        return None
    if ref.startswith("@loader_path/") or ref.startswith("@executable_path/"):
        cand = expand(ref, src_dir, root_dir)
        return os.path.realpath(cand) if os.path.isfile(cand) else None
    if ref.startswith("/"):
        return os.path.realpath(ref) if os.path.isfile(ref) else None
    return None


def discover(targets: list[str], lib_dir: str, arch: "str | None") -> tuple[list[Image], "OrderedDict[str, Image]"]:
    roots: list[Image] = []
    libs: "OrderedDict[str, Image]" = OrderedDict()
    queue: deque = deque()
    for t in targets:
        if not is_macho(t):
            raise RelocationError(f"not a Mach-O file: {t}")
        archs = archs_of(t)
        if len(archs) != 1:
            raise RelocationError(f"{t}: expected a single-architecture image, found {archs or '?'}")
        if arch and archs[0] != arch:
            raise RelocationError(f"{t}: architecture {archs[0]}, expected {arch}")
        img = Image(t, t, False, None)
        roots.append(img)
        queue.append(img)

    while queue:
        img = queue.popleft()
        img.info = parse_load_commands(img.src)
        for _cmd, ref in img.info["deps"]:
            if is_system(ref):
                continue
            if ".framework/" in ref:
                raise RelocationError(f"{img.src}: framework dependency not supported: {ref}")
            src = resolve(ref, img, lib_dir)
            if src is None:
                raise RelocationError(f"{img.src}: cannot find {ref}")
            name = os.path.basename(ref)
            known = libs.get(name)
            if known is not None:
                if os.path.realpath(known.src) != src:
                    raise RelocationError(
                        f"two different libraries want the name {name}: {known.src} and {src}"
                    )
            else:
                lib_arch = archs_of(src)
                if len(lib_arch) != 1 or (arch and lib_arch[0] != arch):
                    raise RelocationError(f"{src}: architectures {lib_arch or '?'}, expected {arch or 'one'}")
                known = Image(os.path.join(lib_dir, name), src, True, img.root)
                libs[name] = known
                queue.append(known)
            new = "@rpath/" + name
            if ref != new:
                img.changes.append((ref, new))
    return roots, libs


def copy_libs(libs: "OrderedDict[str, Image]") -> None:
    for lib in libs.values():
        os.makedirs(os.path.dirname(lib.path), exist_ok=True)
        if os.path.lexists(lib.path) and os.path.realpath(lib.path) == os.path.realpath(lib.src):
            os.chmod(lib.path, 0o755)
            continue
        if os.path.lexists(lib.path):
            os.unlink(lib.path)
        shutil.copy2(lib.src, lib.path)
        os.chmod(lib.path, 0o755)


def rewrite(img: Image, add_rpaths: list[str]) -> bool:
    """Apply the recorded changes to img. Returns True when the file changed."""
    argv = ["install_name_tool"]
    for old, new in img.changes:
        argv += ["-change", old, new]
    existing = list(img.info["rpaths"])
    for rp in existing:
        if rp.startswith("/") and not rp.startswith(SYSTEM_PREFIXES):
            argv += ["-delete_rpath", rp]
    kept = [rp for rp in existing if not (rp.startswith("/") and not rp.startswith(SYSTEM_PREFIXES))]
    wanted = ["@loader_path"] if img.is_lib else list(add_rpaths)
    for rp in wanted:
        if rp not in kept:
            argv += ["-add_rpath", rp]
            kept.append(rp)
    ident = img.info["id"]
    if img.is_lib:
        if ident != "@rpath/" + img.name:
            argv += ["-id", "@rpath/" + img.name]
    elif ident and ident.startswith("/") and not is_system(ident):
        argv += ["-id", "@rpath/" + img.name]
    if len(argv) == 1:
        return False
    argv.append(img.path)
    run_checked(argv)
    return True


def sign(path: str, identity: str) -> None:
    argv = ["codesign", "--force", "--sign", identity]
    if identity != "-":
        argv += ["--options", "runtime", "--timestamp"]
    argv.append(path)
    run_checked(argv)


def verify(images: list[Image], lib_dir: str) -> list[str]:
    problems: list[str] = []
    for img in images:
        info = parse_load_commands(img.path)
        for rp in info["rpaths"]:
            if rp.startswith("/") and not is_system(rp):
                problems.append(f"{img.path}: absolute rpath {rp}")
        if info["id"] and info["id"].startswith("/") and not is_system(info["id"]):
            problems.append(f"{img.path}: absolute install name {info['id']}")
        for _cmd, ref in info["deps"]:
            if is_system(ref):
                continue
            if ref.startswith("@rpath/"):
                if not os.path.isfile(os.path.join(lib_dir, ref[len("@rpath/"):])):
                    problems.append(f"{img.path}: {ref} is not in {lib_dir}")
            else:
                problems.append(f"{img.path}: unexpected reference {ref}")
    return problems


def provenance(lib: Image) -> dict:
    entry = {
        "name": lib.name,
        "source": lib.src,
        "original_install_name": lib.info["id"],
        "current_version": lib.info["current_version"],
        "compatibility_version": lib.info["compat_version"],
        "min_macos": lib.info.get("min_os"),
        "sha256_source": sha256(lib.src),
        "sha256_bundled": sha256(lib.path),
    }
    m = CELLAR_RE.search(lib.src)
    if m:
        entry["homebrew_formula"] = m.group(1)
        entry["homebrew_version"] = m.group(2)
        entry["keg"] = lib.src[: m.end() - 1]
    return entry


def copy_licenses(entries: list[dict], licenses_dir: str) -> list[str]:
    """Copy the license files of every Homebrew keg that supplied a library."""
    os.makedirs(licenses_dir, exist_ok=True)
    missing: list[str] = []
    done: set[str] = set()
    for e in entries:
        keg = e.get("keg")
        if not keg:
            continue
        key = f"{e['homebrew_formula']}-{e['homebrew_version']}"
        if key in done:
            continue
        done.add(key)
        found = [f for f in sorted(os.listdir(keg)) if LICENSE_RE.match(f) and os.path.isfile(os.path.join(keg, f))]
        # Prefer a real license text over an AUTHORS file when deciding completeness.
        if not any(not f.upper().startswith("AUTHORS") for f in found):
            missing.append(key)
            continue
        dest = os.path.join(licenses_dir, key)
        os.makedirs(dest, exist_ok=True)
        for f in found:
            shutil.copy2(os.path.join(keg, f), os.path.join(dest, f))
            os.chmod(os.path.join(dest, f), 0o644)
    return missing


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("targets", nargs="+", help="Mach-O files to relocate (modified in place)")
    ap.add_argument("--lib-dir", required=True, help="directory that receives the bundled libraries")
    ap.add_argument("--add-rpath", action="append", default=[], help="rpath to add to the targets (repeatable)")
    ap.add_argument("--sign", default="-", help="codesign identity ('-' for ad-hoc, the default)")
    ap.add_argument("--arch", default=None, help="require this architecture (for example arm64)")
    ap.add_argument("--manifest", help="write the list of bundled libraries as JSON")
    ap.add_argument("--licenses-dir", help="copy the license files of the Homebrew kegs used")
    args = ap.parse_args()

    lib_dir = os.path.abspath(args.lib_dir)
    targets = [os.path.abspath(t) for t in args.targets]
    try:
        roots, libs = discover(targets, lib_dir, args.arch)
        copy_libs(libs)
        images = list(libs.values()) + roots
        changed = [img for img in images if rewrite(img, args.add_rpath)]
        for img in changed:
            sign(img.path, args.sign)
        # An unchanged file whose signature was already valid stays as it is.
        problems = verify(images, lib_dir)
    except RelocationError as exc:
        print(f"relocate: ERROR: {exc}", file=sys.stderr)
        return 1
    except RuntimeError as exc:
        print(f"relocate: ERROR: {exc}", file=sys.stderr)
        return 1

    print(f"relocate: {len(libs)} librar{'y' if len(libs) == 1 else 'ies'} bundled into {lib_dir}")
    for lib in libs.values():
        m = CELLAR_RE.search(lib.src)
        origin = f"{m.group(1)} {m.group(2)}" if m else lib.src
        print(f"  {lib.name:32s} {origin}")
    for img in roots:
        for old, new in img.changes:
            print(f"  {img.name}: {old} -> {new}")

    entries = [provenance(lib) for lib in libs.values()]
    if args.manifest:
        os.makedirs(os.path.dirname(os.path.abspath(args.manifest)), exist_ok=True)
        with open(args.manifest, "w", encoding="utf-8") as fh:
            json.dump({"libraries": entries}, fh, indent=2)
            fh.write("\n")
    if args.licenses_dir:
        missing = copy_licenses(entries, os.path.abspath(args.licenses_dir))
        for key in missing:
            problems.append(f"no license file found in the Homebrew keg of {key}")

    if problems:
        for p in problems:
            print(f"relocate: PROBLEM: {p}", file=sys.stderr)
        return 1
    print("relocate: verified, no external path remains in the processed images")
    return 0


if __name__ == "__main__":
    sys.exit(main())

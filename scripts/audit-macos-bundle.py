#!/usr/bin/env python3
"""Recursive Mach-O audit of a macOS app bundle or resource directory.

Walks every file under ROOT, finds Mach-O images by magic number (not by
extension), and for each one reads the architectures, the install name, every
dependency load command (including weak, re-exported, lazy and upward loads),
and every LC_RPATH. Each dependency is resolved the way dyld would resolve it:

  /System/Library/..., /usr/lib/...  system, always allowed
  @rpath/x                           the image's own rpaths, then the rpaths of
                                     the bundle's executables (dyld's rpath
                                     stack), then an already-loaded image whose
                                     install name is exactly that string
  @loader_path/x                     relative to the image
  @executable_path/x                 relative to each executable in the bundle
  any other absolute path            external; always a finding (Homebrew,
                                     /usr/local, /Users, /tmp, ...)

Findings:
  EXTERNAL     an absolute dependency or rpath outside the system locations
  UNRESOLVED   an @-reference no search location satisfies inside ROOT
  ARCH         an image without the required architecture slice
  ID           an install name that is an absolute path outside the bundle
  DSSTORE      a Finder metadata file inside the bundle
  SYMLINK      a symlink that is absolute, dangling, or leaves ROOT
  STRING       an absolute external .dylib path embedded in an executable
               (a dlopen fallback, reported separately; --strict-strings fails)
  MINOS        an image that needs a newer macOS than --max-minos allows.
               Only checked when --max-minos is given. A bundled library keeps
               the deployment target it was built with (a Homebrew bottle
               targets the macOS of the machine that built it), so it can raise
               the real minimum above what the app declares.

The report always lists the minimum-macOS versions found.

Exit status: 0 when clean (or without --strict), 1 when --strict and there are
findings, 2 on usage errors.

Usage:
  scripts/audit-macos-bundle.py ROOT [--arch arm64] [--max-minos 11.0]
                                [--strict] [--strict-strings] [--json FILE] [-v]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import OrderedDict

from macho_tools import SYSTEM_PREFIXES, archs_of, is_macho, parse_load_commands, version_tuple

EXTERNAL_HINTS = ("/opt/homebrew", "/usr/local", "/Users/", "/tmp/", "/private/", "/Applications/", "/Library/")
STRING_RE = re.compile(rb"(?:/opt/homebrew|/usr/local)[\x21-\x7e]*?\.dylib")
KINDS = ("EXTERNAL", "UNRESOLVED", "ARCH", "ID", "DSSTORE", "SYMLINK", "STRING", "MINOS")


class Image:
    def __init__(self, path: str, rel: str):
        self.path = path
        self.rel = rel
        self.archs: list[str] = []
        self.info: dict = {}
        self.findings: list[tuple[str, str]] = []
        self.resolved: list[tuple[str, str, str]] = []  # (cmd, ref, how)


def inside(root: str, path: str) -> bool:
    root = os.path.realpath(root)
    path = os.path.realpath(path)
    return path == root or path.startswith(root + os.sep)


def collect(root: str) -> tuple[list[Image], list[str], list[tuple[str, str]]]:
    images: list[Image] = []
    stray: list[str] = []
    links: list[tuple[str, str]] = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        dirnames.sort()
        for name in sorted(dirnames + filenames):
            full = os.path.join(dirpath, name)
            if os.path.islink(full):
                target = os.readlink(full)
                rel = os.path.relpath(full, root)
                if os.path.isabs(target):
                    links.append((rel, "absolute target %s" % target))
                elif not os.path.exists(full):
                    links.append((rel, "dangling -> %s" % target))
                elif not inside(root, full):
                    links.append((rel, "leaves the bundle -> %s" % target))
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            rel = os.path.relpath(full, root)
            if name == ".DS_Store":
                stray.append(rel)
                continue
            if is_macho(full):
                images.append(Image(full, rel))
    return images, stray, links


def expand(entry: str, loader_dir: str, exec_dirs: list[str]) -> list[str]:
    if entry.startswith("@loader_path"):
        return [os.path.normpath(loader_dir + entry[len("@loader_path"):])]
    if entry.startswith("@executable_path"):
        return [os.path.normpath(d + entry[len("@executable_path"):]) for d in exec_dirs]
    return [os.path.normpath(entry)]


def main() -> int:
    ap = argparse.ArgumentParser(description="Recursive Mach-O audit of a macOS bundle.")
    ap.add_argument("root")
    ap.add_argument("--arch", default="arm64", help="required architecture slice (default arm64; '' disables)")
    ap.add_argument("--strict", action="store_true", help="exit 1 on any finding")
    ap.add_argument("--strict-strings", action="store_true", help="also fail on embedded external .dylib paths")
    ap.add_argument(
        "--max-minos",
        default=None,
        help="report images that need a newer macOS than this (for example 11.0); counted by --strict",
    )
    ap.add_argument("--json", dest="json_out", help="write the full result as JSON")
    ap.add_argument("-v", "--verbose", action="store_true", help="print every image and every dependency")
    args = ap.parse_args()

    root = os.path.abspath(args.root)
    if not os.path.isdir(root):
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    images, stray, links = collect(root)
    by_install_name: dict[str, str] = {}
    for img in images:
        img.archs = archs_of(img.path)
        arch_for_otool = args.arch if (args.arch and len(img.archs) > 1 and args.arch in img.archs) else None
        img.info = parse_load_commands(img.path, arch_for_otool)
        if img.info["id"]:
            by_install_name.setdefault(img.info["id"], img.rel)

    exec_dirs = sorted(
        {os.path.dirname(i.path) for i in images if i.info.get("filetype") == "EXECUTE"}
    )
    exec_rpaths: list[str] = []
    for img in images:
        if img.info.get("filetype") == "EXECUTE":
            for rp in img.info["rpaths"]:
                exec_rpaths.append(rp)

    counts: "OrderedDict[str, int]" = OrderedDict((k, 0) for k in KINDS)
    string_hits: list[tuple[str, str]] = []

    for img in images:
        loader_dir = os.path.dirname(img.path)
        if args.arch and args.arch not in img.archs:
            img.findings.append(("ARCH", f"slices={','.join(img.archs) or '?'} (need {args.arch})"))
        ident = img.info["id"]
        if ident and ident.startswith("/") and not ident.startswith(SYSTEM_PREFIXES):
            img.findings.append(("ID", f"install name is an absolute path: {ident}"))

        for rp in img.info["rpaths"]:
            if rp.startswith("/") and not rp.startswith(SYSTEM_PREFIXES):
                img.findings.append(("EXTERNAL", f"LC_RPATH {rp}"))

        min_os = img.info.get("min_os")
        if args.max_minos and min_os and version_tuple(min_os) > version_tuple(args.max_minos):
            img.findings.append(("MINOS", f"needs macOS {min_os} (limit {args.max_minos})"))

        search: list[str] = []
        for rp in img.info["rpaths"]:
            search += expand(rp, loader_dir, exec_dirs)
        for rp in exec_rpaths:
            search += expand(rp, loader_dir, exec_dirs)

        for cmd, ref in img.info["deps"]:
            if ref.startswith(SYSTEM_PREFIXES):
                img.resolved.append((cmd, ref, "system"))
                continue
            if ref.startswith("@rpath/"):
                tail = ref[len("@rpath/"):]
                hit = next(
                    (os.path.join(d, tail) for d in search if os.path.exists(os.path.join(d, tail))),
                    None,
                )
                if hit and inside(root, hit):
                    img.resolved.append((cmd, ref, "rpath:" + os.path.relpath(hit, root)))
                elif ref in by_install_name:
                    img.resolved.append((cmd, ref, "install-name:" + by_install_name[ref]))
                else:
                    img.findings.append(("UNRESOLVED", f"{cmd} {ref}"))
                continue
            if ref.startswith("@loader_path/") or ref.startswith("@executable_path/"):
                cands = expand(ref, loader_dir, exec_dirs)
                hit = next((c for c in cands if os.path.exists(c) and inside(root, c)), None)
                if hit:
                    img.resolved.append((cmd, ref, "path:" + os.path.relpath(hit, root)))
                elif ref in by_install_name:
                    img.resolved.append((cmd, ref, "install-name:" + by_install_name[ref]))
                else:
                    img.findings.append(("UNRESOLVED", f"{cmd} {ref}"))
                continue
            if ref.startswith("/"):
                note = "build-machine path" if ref.startswith(EXTERNAL_HINTS) else "absolute path"
                img.findings.append(("EXTERNAL", f"{cmd} {ref} ({note})"))
                continue
            img.findings.append(("UNRESOLVED", f"{cmd} {ref} (relative reference)"))

        if img.info.get("filetype") == "EXECUTE" or os.access(img.path, os.X_OK):
            try:
                with open(img.path, "rb") as fh:
                    data = fh.read()
                for m in sorted(set(STRING_RE.findall(data))):
                    string_hits.append((img.rel, m.decode("ascii", "replace")))
            except OSError:
                pass

    counts["DSSTORE"] = len(stray)
    counts["SYMLINK"] = len(links)
    for img in images:
        for kind, _ in img.findings:
            counts[kind] += 1
    counts["STRING"] = len(string_hits)

    # ---- report ---------------------------------------------------------
    n_exec = sum(1 for i in images if i.info.get("filetype") == "EXECUTE")
    print(f"Mach-O audit of {root}")
    print(f"  images: {len(images)}   executables: {n_exec}")
    slices: dict[str, int] = {}
    for img in images:
        key = ",".join(img.archs) or "?"
        slices[key] = slices.get(key, 0) + 1
    print("  slices: " + ", ".join(f"{k}={v}" for k, v in sorted(slices.items())))
    minos: dict[str, int] = {}
    for img in images:
        key = img.info.get("min_os") or "?"
        minos[key] = minos.get(key, 0) + 1
    print(
        "  minimum macOS: "
        + ", ".join(f"{k}={v}" for k, v in sorted(minos.items(), key=lambda kv: version_tuple(kv[0])))
    )

    for img in images:
        if args.verbose:
            print(
                f"\n{img.rel}  [{img.info.get('filetype')}] archs={','.join(img.archs)} "
                f"minos={img.info.get('min_os')} id={img.info['id']}"
            )
            if img.info["rpaths"]:
                print("    rpaths: " + ", ".join(img.info["rpaths"]))
            for cmd, ref, how in img.resolved:
                print(f"    {ref}  -> {how}")
        for kind, msg in img.findings:
            print(f"{kind:10s} {img.rel}: {msg}")
    for rel in stray:
        print(f"{'DSSTORE':10s} {rel}")
    for rel, why in links:
        print(f"{'SYMLINK':10s} {rel}: {why}")
    for rel, hit in string_hits:
        print(f"{'STRING':10s} {rel}: {hit}")

    print("\nSummary: " + "  ".join(f"{k}={v}" for k, v in counts.items()))

    if args.json_out:
        payload = {
            "root": root,
            "arch_required": args.arch,
            "counts": counts,
            "images": [
                {
                    "path": img.rel,
                    "type": img.info.get("filetype"),
                    "archs": img.archs,
                    "install_name": img.info["id"],
                    "min_os": img.info.get("min_os"),
                    "rpaths": img.info["rpaths"],
                    "dependencies": [
                        {"cmd": c, "ref": r, "resolved": h} for c, r, h in img.resolved
                    ],
                    "findings": [{"kind": k, "detail": m} for k, m in img.findings],
                }
                for img in images
            ],
            "ds_store": stray,
            "symlinks": [{"path": p, "problem": w} for p, w in links],
            "embedded_external_dylib_strings": [{"path": p, "string": s} for p, s in string_hits],
        }
        with open(args.json_out, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, indent=2)

    hard = sum(v for k, v in counts.items() if k != "STRING")
    if args.strict_strings:
        hard += counts["STRING"]
    if args.strict and hard:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

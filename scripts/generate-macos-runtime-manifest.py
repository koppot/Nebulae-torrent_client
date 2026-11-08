#!/usr/bin/env python3
"""Write build-manifest.json for the macOS payload that is actually staged.

stage-vlc-runtime.sh writes this file as part of staging. This script exists for
the case where the runtime under <resources>/vlc was not produced by that script
(for example an accepted runtime that is carried forward), so the manifest still
describes the files that will ship and not the files of another platform.

Every file and symlink under <resources>/vlc is hashed as it is on disk now.
scripts/verify-macos-app.sh compares the built app against this file.

Usage:
  generate-macos-runtime-manifest.py --resources src-tauri/resources \
      --arch aarch64 [--excluded-plugin libfoo_plugin.dylib ...] [--note TEXT ...]
"""

import argparse
import datetime
import hashlib
import json
import os
import subprocess
import sys

CUSTOM = {
    "lib/libvlc.5.dylib",
    "plugins/libvideotoolbox_plugin.dylib",
    "plugins/libcaopengllayer_plugin.dylib",
    "plugins/libglconv_cvpx_plugin.dylib",
    "plugins/liblibbluray_plugin.dylib",
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--resources", required=True)
    ap.add_argument("--arch", required=True, help="aarch64 or x86_64")
    ap.add_argument("--vlc-version", default="3.0.23")
    ap.add_argument("--excluded-plugin", action="append", default=[])
    ap.add_argument("--note", action="append", default=[])
    ap.add_argument("--out", help="defaults to <resources>/build-manifest.json")
    args = ap.parse_args()

    dest = os.path.join(args.resources, "vlc")
    if not os.path.isdir(dest):
        sys.exit("no vlc directory under %s" % args.resources)
    for name in args.excluded_plugin:
        if os.path.exists(os.path.join(dest, "plugins", name)):
            sys.exit("%s is listed as excluded but is still staged" % name)

    bundled_path = os.path.join(dest, "bundled-libraries.json")
    bundled = json.load(open(bundled_path))["libraries"] if os.path.isfile(bundled_path) else []
    relocated = {e["name"] for e in bundled}
    lb_files = {"lib/libbluray.4.dylib"} | {
        "lib/" + n for n in os.listdir(os.path.join(dest, "lib")) if n.endswith(".jar")
    }

    def source_of(rel):
        if rel in CUSTOM:
            return "custom-build"
        if rel in lb_files:
            return "libbluray-build"
        if rel.startswith("lib/") and os.path.basename(rel) in relocated:
            return "homebrew-relocated"
        if rel.startswith("licenses/") or rel == "bundled-libraries.json":
            return "nebulae-notice"
        return "official-vlc-app"

    files, links = [], []
    for root, dirs, names in os.walk(dest):
        for name in names + dirs:
            path = os.path.join(root, name)
            rel = os.path.relpath(path, dest)
            if os.path.islink(path):
                links.append({"path": "vlc/" + rel, "target": os.readlink(path)})
            elif os.path.isfile(path):
                files.append({"path": "vlc/" + rel, "sha256": sha256(path), "source": source_of(rel)})
    files.sort(key=lambda e: e["path"])
    links.sort(key=lambda e: e["path"])

    try:
        rev = subprocess.run(
            ["git", "-C", os.path.dirname(os.path.abspath(__file__)), "rev-parse", "--short", "HEAD"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True,
        ).stdout.decode().strip()
    except Exception:
        rev = "unknown"

    doc = {
        "schema": 1,
        "platform": "macos-" + args.arch,
        "staged_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "git_rev": rev,
        "vlc_version": args.vlc_version,
        "target_arch": args.arch,
        "generated_by": "scripts/generate-macos-runtime-manifest.py",
        "excluded_plugins": sorted(args.excluded_plugin),
        "notes": args.note,
        "files": files,
        "links": links,
    }
    out = args.out or os.path.join(args.resources, "build-manifest.json")
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2)
        fh.write("\n")
    print("%s: %d files, %d links, git_rev %s" % (out, len(files), len(links), rev))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Check that every Mach-O image inside an app bundle carries a valid signature.

    check-macos-signatures.py APP

Sealing a bundle records a hash for each file under Contents/Resources. It does
not validate, and cannot repair, the signature of a Mach-O file in there, so a
VLC plugin or a Homebrew library that was edited after it was signed stays
broken inside a bundle whose seal verifies. This script checks each image on its
own with `codesign --verify --strict`.

The main executable is left out: while the bundle is unsealed its signature
cannot be verified in isolation, and once the bundle is sealed
`codesign --verify --deep --strict APP` covers it.

Exit status 0 when every image verifies, 1 otherwise.
"""

import concurrent.futures
import os
import plistlib
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import macho_tools  # noqa: E402


def main_executable(app):
    with open(os.path.join(app, "Contents", "Info.plist"), "rb") as fh:
        name = plistlib.load(fh)["CFBundleExecutable"]
    return os.path.join(app, "Contents", "MacOS", name)


def images_in(app, skip):
    found = []
    for root, _dirs, names in os.walk(app):
        for name in names:
            path = os.path.join(root, name)
            if path == skip or os.path.islink(path) or not os.path.isfile(path):
                continue
            if macho_tools.is_macho(path):
                found.append(path)
    return sorted(found)


def verify(path):
    done = subprocess.run(["codesign", "--verify", "--strict", path], capture_output=True, text=True)
    lines = (done.stderr or done.stdout).strip().splitlines()
    detail = lines[0] if lines else "invalid signature"
    return path, done.returncode, detail.replace(path + ": ", "", 1)


def main():
    if len(sys.argv) != 2 or not os.path.isdir(os.path.join(sys.argv[1], "Contents", "MacOS")):
        print("usage: check-macos-signatures.py /path/to/Nebulae.app", file=sys.stderr)
        return 2
    app = os.path.abspath(sys.argv[1])
    images = images_in(app, main_executable(app))
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        results = list(pool.map(verify, images))
    bad = [(path, detail) for path, code, detail in results if code != 0]
    for path, detail in bad:
        print("FAIL: %s: %s" % (os.path.relpath(path, app), detail))
    print("%d nested images checked, %d without a valid signature" % (len(images), len(bad)))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Kit test: the committed dmg window backgrounds match the approved spec.

Checks both assets are PNGs with the exact approved dimensions (660x400 1x,
1320x800 2x) and the SHA-256 values recorded in the Task 5 report (user
supplied, 2026-11-27 amendment).

Run: python3 scripts/tests/test_dmg_background_assets.py
"""
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..")

EXPECTED = [
    ("assets/dmg/dmg-background.png", (660, 400),
     "318d9744ab04b923396e945d569957dff1f54cd05f4a05d278b6a1793ccfc5d9"),
    ("assets/dmg/dmg-background@2x.png", (1320, 800),
     "313ff4222c6cf6f140de28fdaae6238e76815da33b7d5e975a5ec20da23aadf2"),
]


def png_size(path):
    with open(path, "rb") as fh:
        data = fh.read(33)
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    w, h = struct.unpack(">II", data[16:24])
    return (w, h)


def main():
    failures = []
    for rel, want_dims, want_sha in EXPECTED:
        p = os.path.join(REPO, rel)
        if not os.path.isfile(p):
            failures.append(f"missing: {rel}")
            continue
        with open(p, "rb") as fh:
            sha = hashlib.sha256(fh.read()).hexdigest()
        if sha != want_sha:
            failures.append(f"{rel}: sha {sha} != expected {want_sha}")
        dims = png_size(p)
        if dims != want_dims:
            failures.append(f"{rel}: dimensions {dims} != {want_dims}")
    if failures:
        print("FAIL:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("test_dmg_background_assets: OK (2 assets, hashes and dimensions match)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

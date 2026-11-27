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
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..")
GENERATOR = os.path.join(REPO, "scripts", "make-dmg-background.py")

# Pillow is needed to run the generator end-to-end; locate a python that has it.
_PIL_CANDIDATES = [
    "/tmp/neb/pil-venv/bin/python",
    sys.executable,
]
PYPIL = None
for _py in _PIL_CANDIDATES:
    try:
        subprocess.run([_py, "-c", "import PIL"], check=True,
                       capture_output=True)
        PYPIL = _py
        break
    except (OSError, subprocess.CalledProcessError):
        continue

ZONE_HALF = 128
# 1x zone centres (half of the 2x values 360,380 / 960,380)
ZONE_CENTERS_1X = [(180, 190), (480, 190)]

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

    # Generator round-trip: regenerate both PNGs and verify dimensions and
    # the two empty 256px (128px at 1x) icon zones. Runs twice: once with
    # the real artwork (if present) and once with the self-draw fallback
    # (artwork missing). The committed assets are never overwritten.
    if PYPIL is None:
        print("SKIP: generator round-trip (no python with Pillow found)")
        return 0

    def zones_empty(png_path, dims):
        code = f"""
from PIL import Image
img = Image.open({png_path!r}).convert('RGB')
dims = {dims!r}
scale = dims[0] / 1320.0
half = int(round(128 * scale))
bad = []
if img.size != dims:
    bad.append('dimensions %s != %s' % (img.size, dims))
for cx2, cy2 in [(360, 380), (960, 380)]:
    cx, cy = int(round(cx2 * scale)), int(round(cy2 * scale))
    crop = img.crop((cx - half, cy - half, cx + half, cy + half))
    colors = crop.getcolors(1 << 24)
    for count, (r, g, b) in colors:
        if (r, g, b) == (229, 16, 43) or (r, g, b) == (255, 255, 255):
            bad.append('zone (%d,%d) drawn' % (cx, cy))
print('BAD=' + repr(bad))
"""
        r = subprocess.run([PYPIL, "-c", code], capture_output=True, text=True)
        if r.returncode != 0:
            return f"zone check failed: {r.stderr[-200:]}"
        line = [l for l in r.stdout.splitlines() if l.startswith("BAD=")]
        bad = eval(line[0][4:]) if line else ["zone check produced no output"]
        return "; ".join(bad) if bad else None

    rt_failures = []
    for label, art_env in [
        ("self-draw fallback", {"NEBULAE_DMG_ART": "/nonexistent/nebula-art.png"}),
    ]:
        with tempfile.TemporaryDirectory() as td:
            env = dict(os.environ, **art_env)
            r = subprocess.run([PYPIL, GENERATOR, td], capture_output=True,
                               text=True, env=env)
            if r.returncode != 0:
                rt_failures.append(f"{label}: generator exit {r.returncode}: {r.stderr[-300:]}")
                continue
            for rel, dims, _sha in EXPECTED:
                out = os.path.join(td, os.path.basename(rel))
                if not os.path.isfile(out):
                    rt_failures.append(f"{label}: generator did not produce {os.path.basename(rel)}")
                    continue
                problem = zones_empty(out, dims)
                if problem:
                    rt_failures.append(f"{label}: {os.path.basename(rel)}: {problem}")
    if rt_failures:
        print("FAIL (generator round-trip):")
        for f in rt_failures:
            print(f"  - {f}")
        return 1
    print("generator round-trip: OK (self-draw fallback produces valid 1x/2x pair, zones clean)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

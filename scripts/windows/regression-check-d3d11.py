#!/usr/bin/env python3
"""Regression check for the accepted Windows D3D11 HDR-to-SDR plugin.

Exits nonzero if the installed/pinned runtime identity drifts from the
user-accepted baseline. Safe to run in CI; it only reads files.

usage: regression-check-d3d11.py [PLUGIN_PATH]

Without PLUGIN_PATH it checks the lock file identity only. With a path it also
hashes, sizes, PE-machine, and tone-mapping-marker checks the built/staged
plugin against the accepted baseline.
"""

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pe_audit  # noqa: E402

SCRIPT = Path(__file__).resolve().parent
LOCK_PATH = SCRIPT / "d3d11-plugin.lock.json"
PLUGIN_REL = "plugins/video_output/libdirect3d11_plugin.dll"

# Accepted baseline (user-verified candidate, delivered 2026-11-09).
ACCEPTED_SHA256 = "b825a0f6dbef7d3f8a223bd934091b764f7b1574cd5654d2685256b5512f6725"
ACCEPTED_BYTES = 401920
STOCK_SHA256 = "c1ba837963b15835645ab157dc8efb63e1083d211f4d74010237a3e87d7f0ce8"
STOCK_BYTES = 408472
MARKER = b"HDR to SDR tone mapping:"

REFERENCE_TOLERANCES = (
    "Synthetic PQ vs BT.2390 reference: max error 2.75 / 255 codes, monotonic, no clip\n"
    "Synthetic HLG vs reference: max error 1.63 / 255 codes\n"
    "SDR captures (synthetic + Fuuka): bit-identical to stock/0.2.5\n"
    "Mary UHD 5 scenes vs reference: mean abs error 0.8-2.0 codes; luma p50 "
    "0.610 / 0.625 / 0.144 / 0.238 / 0.672\n"
)


def fail(msg):
    print(f"REGRESSION CHECK FAILED: {msg}")
    return 1


def ok(msg):
    print(f"ok: {msg}")
    return 0


def check_lock():
    try:
        lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return fail(f"cannot read {LOCK_PATH}: {exc}")
    got = lock.get("sha256", "").lower()
    want = ACCEPTED_SHA256.lower()
    if got != want:
        return fail(f"lock sha256 {got} != accepted {want}")
    if lock.get("bytes") != ACCEPTED_BYTES:
        return fail(f"lock bytes {lock.get('bytes')} != accepted {ACCEPTED_BYTES}")
    stock = lock.get("stock_sha256", "").lower()
    if stock != STOCK_SHA256.lower():
        return fail(f"lock stock_sha256 {stock} != expected {STOCK_SHA256}")
    if got == stock:
        return fail("lock sha256 equals the stock plugin sha256")
    print(f"ok: lock identity pinned to {want} ({lock.get('bytes')} bytes; stock {stock})")
    return 0


def check_plugin(path):
    data = Path(path).read_bytes()
    got = hashlib.sha256(data).hexdigest()
    if got != ACCEPTED_SHA256:
        return fail(f"plugin sha256 {got} != accepted {ACCEPTED_SHA256}")
    if len(data) != ACCEPTED_BYTES:
        return fail(f"plugin bytes {len(data)} != accepted {ACCEPTED_BYTES}")
    try:
        machine = pe_audit.parse_pe(data).machine
    except Exception as exc:
        return fail(f"cannot parse plugin PE: {exc}")
    if machine != 0x8664:
        return fail(f"plugin PE machine {machine:#x} != x64 (0x8664)")
    if MARKER not in data:
        return fail("plugin lacks HDR-to-SDR tone mapping marker")
    if got == STOCK_SHA256:
        return fail("plugin is the stock VLC D3D11 plugin (fix reverted)")
    print(f"ok: plugin {path} is the accepted build ({got}, {len(data)} bytes, x64, tone-mapping marker present)")
    return 0


def main(argv):
    rc = check_lock()
    if len(argv) > 1:
        rc |= check_plugin(argv[1])
    print("\nAccepted reference tolerances (validation evidence, not re-executed here):")
    for line in REFERENCE_TOLERANCES.splitlines():
        print("  " + line)
    return 1 if rc else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

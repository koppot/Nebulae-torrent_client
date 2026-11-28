#!/usr/bin/env python3
"""Structural kit test: in the Linux and macOS entry scripts the frozen
frontend install must come before the first command that needs node_modules
(tsc), and dist/ must exist before cargo check (generate_context!).

build-entry-linux.sh ran 'pnpm exec tsc' three lines BEFORE
'pnpm install --frozen-lockfile', so a fresh clone could not pass its own
compile gate.

Run: python3 -m pytest scripts/tests/test_entry_gate_order.py
"""
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def line_of(lines, needle):
    for i, ln in enumerate(lines):
        if not ln.lstrip().startswith("#") and needle in ln:
            return i
    raise AssertionError(f"{needle!r} not found")


def test_install_and_dist_precede_the_compile_gates():
    for name in ("build-entry-linux.sh", "build-entry-macos.sh"):
        with open(os.path.join(ROOT, "scripts", name)) as fh:
            lines = fh.read().splitlines()
        install = line_of(lines, "pnpm install --frozen-lockfile")
        mkdist = line_of(lines, "mkdir -p dist")
        cargo = line_of(lines, "cargo check")
        tsc = line_of(lines, "tsc --noEmit")
        assert install < tsc, f"{name}: pnpm install must come before tsc"
        assert mkdist < cargo, f"{name}: mkdir -p dist must come before cargo check"


if __name__ == "__main__":
    test_install_and_dist_precede_the_compile_gates()
    print("ok")

#!/usr/bin/env python3
"""Kit test: the Windows-only Rust code must type-check and pass clippy
(-D warnings) for the Windows target, run from this machine.

Regression for f1ff870, where macOS gates were green and the Windows build
failed with E0308 (&Arc<VlcLib> passed where Arc<VlcLib> is taken), an
unused import and an unreachable statement, all inside cfg(windows) code that
the macOS compiler never sees. Skipped when the Windows target's std is not
installed (the script exits 3); it never installs anything.

Run: python3 -m pytest scripts/tests/test_windows_target_check.py
"""
import os
import subprocess
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SCRIPT = os.path.join(ROOT, "scripts", "check-windows-target.sh")


class WindowsTargetCheck(unittest.TestCase):
    def test_windows_target_typechecks_and_lints(self):
        env = dict(os.environ)
        cargo_bin = os.path.expanduser("~/.rustup/toolchains/stable-aarch64-apple-darwin/bin")
        if os.path.isdir(cargo_bin):
            env["PATH"] = cargo_bin + os.pathsep + env["PATH"]
        r = subprocess.run([SCRIPT], capture_output=True, text=True, env=env, timeout=1800)
        if r.returncode == 3:
            self.skipTest(r.stderr.strip())
        self.assertEqual(r.returncode, 0, (r.stdout + r.stderr)[-4000:])


if __name__ == "__main__":
    unittest.main()

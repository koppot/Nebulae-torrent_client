"""Behavior test for scripts/windows/verify-clean-build.ps1 (run under pwsh).

The script used to read the embedded build_id with Sysinternals strings.exe and
only fell back to a raw scan "if strings.exe is absent", but under
$ErrorActionPreference = "Stop" a missing strings.exe is a terminating
CommandNotFoundException, so the fallback never ran (reproduced under pwsh on
macOS, where strings.exe does not exist). The script now scans the bytes only.

Skipped when pwsh is not installed.
"""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SCRIPT = REPO / "scripts" / "windows" / "verify-clean-build.ps1"
PWSH = shutil.which("pwsh") or "/opt/homebrew/bin/pwsh"
STALE = "1" * 40


def git(cwd, *args):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()


@unittest.skipUnless(Path(PWSH).is_file(), "pwsh not available on this machine")
class VerifyCleanBuild(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="nebulae-vcb-")
        self.repo = os.path.join(self.tmp, "repo")
        os.makedirs(self.repo)
        git(self.repo, "init", "-q")
        git(self.repo, "config", "user.name", "kit-test")
        git(self.repo, "config", "user.email", "kit-test@example.invalid")
        Path(self.repo, "a").write_text("a")
        git(self.repo, "add", "a")
        git(self.repo, "commit", "-q", "-m", "x")
        self.head = git(self.repo, "rev-parse", "HEAD")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_gate(self, payload, name="nebulae.exe"):
        exe = os.path.join(self.tmp, name)  # outside the repo: the tree stays clean
        Path(exe).write_bytes(payload)
        return subprocess.run([PWSH, "-NoProfile", "-File", str(SCRIPT), "-BuiltExe", exe],
                              cwd=self.repo, capture_output=True, text=True)

    def test_expected_id_wins_over_a_stale_first_match(self):
        r = self.run_gate(f"x {STALE}-clean y {self.head}-clean z".encode())
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("clean provenance OK", r.stdout)

    def test_path_with_brackets_and_space(self):
        r = self.run_gate(f"{self.head}-clean".encode(), name="my [x] build.exe")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_dirty_other_commit_and_missing_id_fail(self):
        for payload, want in ((f"{self.head}-dirty", "-dirty"),
                              (f"{STALE}-clean", "does not match"),
                              ("nothing here", "could not read")):
            r = self.run_gate(payload.encode())
            self.assertNotEqual(r.returncode, 0, payload)
            self.assertIn(want, r.stdout + r.stderr)

    def test_dirty_worktree_fails_even_with_a_clean_id(self):
        Path(self.repo, "untracked").write_text("x")
        r = self.run_gate(f"{self.head}-clean".encode())
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("dirty", r.stdout + r.stderr)


if __name__ == "__main__":
    unittest.main()

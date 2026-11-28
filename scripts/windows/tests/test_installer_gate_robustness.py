"""verify_installer.py must not leave its temp extraction dir behind when 7z is
missing or fails (a partial extraction of a 100+ MB installer was left in
%TEMP% on every failed run).
"""
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VERIFY = REPO / "scripts" / "windows" / "verify_installer.py"


@unittest.skipIf(sys.platform == "win32", "uses a POSIX shell stub for 7z")
class VerifyInstallerTempCleanup(unittest.TestCase):
    def run_with_path(self, path_dirs, tmp):
        fake = os.path.join(tmp, "fake-setup.exe")
        Path(fake).write_bytes(b"x")
        env = dict(os.environ, PATH=os.pathsep.join(path_dirs), TMPDIR=tmp, TEMP=tmp, TMP=tmp)
        r = subprocess.run([sys.executable, str(VERIFY), "--installer", fake],
                           capture_output=True, text=True, env=env)
        left = [n for n in os.listdir(tmp) if n.startswith("nebulae-verify-")]
        return r, left

    def test_no_7z_leaves_no_temp_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            empty = os.path.join(tmp, "empty-bin")
            os.makedirs(empty)
            r, left = self.run_with_path([empty], tmp)
            self.assertNotEqual(r.returncode, 0)
            self.assertEqual(left, [])

    def test_failing_7z_leaves_no_temp_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            bindir = os.path.join(tmp, "bin")
            os.makedirs(bindir)
            stub = os.path.join(bindir, "7z")
            Path(stub).write_text("#!/bin/sh\necho boom >&2\nexit 2\n")
            os.chmod(stub, os.stat(stub).st_mode | stat.S_IXUSR)
            r, left = self.run_with_path([bindir], tmp)
            self.assertEqual(r.returncode, 2)
            self.assertEqual(left, [])


if __name__ == "__main__":
    unittest.main()

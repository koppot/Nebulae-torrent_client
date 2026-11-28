#!/usr/bin/env python3
"""Kit test: build-entry-macos.sh must detach the dmg it mounted in step 8
before step 9 (baseline_diff.py) mounts the same image.

Background (attempt 2, 2026-11): step 8 mounted the delivered dmg and only
detached it in the EXIT trap; step 9 then ran "hdiutil attach" on the same
image and got "Resource busy", so the gate reported COULD NOT RUN after every
other gate had passed. No unit test of baseline_diff.py can see this, because
the bug is in the ORDER of the entry script, so this test runs the real entry
script (--verify-only, no rebuild) in a throwaway git repo with a mock
hdiutil that behaves like the real one: a second attach of an image that is
already mounted fails with "Resource busy".

Cases:
  1. normal run: exit 0, BUILD-RECORD-VERIFY-ONLY.txt written, no mount left;
  2. the same run with the detach block removed from a copy of the script must
     FAIL (proves the mock reproduces the bug and the test has teeth);
  3. a failing verify-macos-app.sh on the mounted app exits non-zero and the
     EXIT trap still leaves no mount behind.
The real hdiutil is never called. Runs on macOS only (the script uses
macOS shasum/stat/strings).

Run: python3 -m pytest scripts/tests/test_entry_mount_order.py
"""
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
ENTRY = os.path.join(ROOT, "scripts", "build-entry-macos.sh")
SHA40 = "0123456789abcdef0123456789abcdef01234567"

MOCK_HDIUTIL = r'''#!/usr/bin/env bash
# state: one line per mounted "image<TAB>mountpoint" in $MOCK_STATE
set -u
cmd="$1"; shift
case "$cmd" in
attach)
    mp=""; img=""
    while [ $# -gt 0 ]; do
        case "$1" in
            -mountpoint) mp="$2"; shift 2 ;;
            -*) shift ;;
            *) img="$1"; shift ;;
        esac
    done
    if grep -q "^$img	" "$MOCK_STATE" 2>/dev/null; then
        echo "hdiutil: attach failed - Resource busy" >&2; exit 1
    fi
    printf '%s	%s
' "$img" "$mp" >> "$MOCK_STATE"
    mkdir -p "$mp/Nebulae.app/Contents/MacOS"
    printf 'xx %s-clean xx' "$MOCK_SHA" > "$mp/Nebulae.app/Contents/MacOS/nebulae"
    ;;
detach)
    while [ $# -gt 0 ]; do case "$1" in -*) shift ;; *) mp="$1"; shift ;; esac; done
    grep -q "	$mp$" "$MOCK_STATE" || { echo "hdiutil: detach failed - No such file or directory" >&2; exit 1; }
    grep -v "	$mp$" "$MOCK_STATE" > "$MOCK_STATE.new" || true
    mv "$MOCK_STATE.new" "$MOCK_STATE"
    rm -rf "$mp/Nebulae.app"
    ;;
*) echo "mock hdiutil: unsupported $cmd" >&2; exit 64 ;;
esac
'''

# Mounts both images the way scripts/baseline_diff.py does (attach, detach).
MOCK_BASELINE_DIFF = r'''import subprocess, sys, os, tempfile
a = sys.argv[1:]
cand = a[a.index("--candidate") + 1]
base = a[a.index("--baseline") + 1]
for img in (base, cand):
    mp = tempfile.mkdtemp(prefix="mockdiff.")
    r = subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mp, img],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print("GATE COULD NOT RUN: cannot mount %s: %s" % (img, r.stderr.strip()))
        sys.exit(4)
    subprocess.run(["hdiutil", "detach", mp], check=True)
    os.rmdir(mp)
open(a[a.index("--report") + 1], "w").write("mock report\n")
print("GATE PASS: mock")
'''


def sh(cmd, cwd, env=None, check=True):
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, check=check)


@unittest.skipUnless(sys.platform == "darwin", "the entry script uses macOS shasum/stat/strings")
class EntryMountOrder(unittest.TestCase):
    def make_env(self, tmp, entry_src, verify_rc=0):
        repo = os.path.join(tmp, "repo")
        os.makedirs(os.path.join(repo, "scripts", "baseline-allow"))
        os.makedirs(os.path.join(repo, "src-tauri"))
        shutil.copy(entry_src, os.path.join(repo, "scripts", "build-entry-macos.sh"))
        with open(os.path.join(repo, "scripts", "verify-macos-app.sh"), "w") as f:
            f.write("#!/usr/bin/env bash\nexit %d\n" % verify_rc)
        with open(os.path.join(repo, "scripts", "baseline_diff.py"), "w") as f:
            f.write(MOCK_BASELINE_DIFF)
        open(os.path.join(repo, "scripts", "baseline-allow", "macos.tsv"), "w").close()
        with open(os.path.join(repo, "src-tauri", "tauri.conf.json"), "w") as f:
            f.write('{"version": "9.9.9"}')
        for n in ("verify-macos-app.sh", "build-entry-macos.sh"):
            os.chmod(os.path.join(repo, "scripts", n), 0o755)
        sh(["git", "init", "-q"], repo)
        sh(["git", "config", "user.name", "kit-test"], repo)
        sh(["git", "config", "user.email", "kit-test@example.invalid"], repo)
        sh(["git", "add", "-A"], repo)
        sh(["git", "commit", "-q", "-m", "fixture"], repo)
        head = sh(["git", "rev-parse", "HEAD"], repo).stdout.strip()

        out = os.path.join(tmp, "out dir")  # a space on purpose
        os.makedirs(out)
        for name in ("Nebulae-macOS-arm64.dmg", "Nebulae-macOS-arm64.dmg-app.zip"):
            with open(os.path.join(out, name), "wb") as f:
                f.write(name.encode())
            sums = sh(["shasum", "-a", "256", name], out).stdout
            with open(os.path.join(out, name + ".sha256"), "w") as f:
                f.write(sums)
        baseline = os.path.join(tmp, "baseline.dmg")
        open(baseline, "wb").write(b"baseline")

        bindir = os.path.join(tmp, "bin")
        os.makedirs(bindir)
        p = os.path.join(bindir, "hdiutil")
        open(p, "w").write(MOCK_HDIUTIL)
        os.chmod(p, 0o755)
        state = os.path.join(tmp, "mounts.txt")
        open(state, "w").close()
        env = dict(os.environ, PATH=bindir + os.pathsep + os.environ["PATH"],
                   MOCK_STATE=state, MOCK_SHA=SHA40, TMPDIR=tmp)
        return repo, head, out, baseline, env, state

    def run_entry(self, tmp, entry_src, verify_rc=0):
        repo, head, out, baseline, env, state = self.make_env(tmp, entry_src, verify_rc)
        r = sh(["./scripts/build-entry-macos.sh", "--commit", head, "--version", "9.9.9",
                "--target", "aarch64-apple-darwin", "--out", out, "--baseline", baseline,
                "--verify-only"], repo, env=env, check=False)
        return r, out, state

    def test_detaches_before_baseline_gate(self):
        with tempfile.TemporaryDirectory() as tmp:
            r, out, state = self.run_entry(tmp, ENTRY)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn("GATE PASS", r.stdout)
            self.assertTrue(os.path.isfile(os.path.join(out, "BUILD-RECORD-VERIFY-ONLY.txt")))
            self.assertFalse(os.path.exists(os.path.join(out, "BUILD-RECORD.txt")))
            record = open(os.path.join(out, "BUILD-RECORD-VERIFY-ONLY.txt")).read()
            self.assertIn("VERIFY-ONLY", record)
            self.assertIn(SHA40 + "-clean", record)
            self.assertEqual(open(state).read(), "", "a mount was left attached")

    def test_without_detach_the_mock_reproduces_the_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = open(ENTRY).read()
            stripped = re.sub(r'if ! hdiutil detach "\$MNT".*?\ntrap - EXIT\n', "", src, flags=re.S)
            self.assertNotEqual(src, stripped, "detach block not found; update this test")
            alt = os.path.join(tmp, "entry-no-detach.sh")
            open(alt, "w").write(stripped)
            r, out, state = self.run_entry(tmp, alt)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("Resource busy", r.stdout + r.stderr)
            self.assertEqual(open(state).read(), "", "the EXIT trap must still detach")

    def test_failure_path_leaves_no_mount(self):
        with tempfile.TemporaryDirectory() as tmp:
            r, out, state = self.run_entry(tmp, ENTRY, verify_rc=1)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("verify-macos-app.sh on the mounted dmg", r.stderr)
            self.assertEqual(open(state).read(), "", "the EXIT trap must detach on failure")


if __name__ == "__main__":
    unittest.main()

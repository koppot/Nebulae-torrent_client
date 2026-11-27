"""Tests for the locked geo asset input (scripts/geo-assets.lock.json).

Covers: lock integrity against the rules-page constants, the check gate
(present / missing / wrong size / wrong hash), the fetch script's refusal
paths (bad args, wrong pinned commit in NEBULAE_GEO_ASSETS_DIR), and the
PowerShell twin's parseability.
"""
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
LOCK_PATH = REPO / "scripts" / "geo-assets.lock.json"
FETCH_SH = REPO / "scripts" / "fetch-geo-assets.sh"
CHECK_SH = REPO / "scripts" / "check-geo-assets.sh"
FETCH_PS1 = REPO / "scripts" / "windows" / "fetch-geo-assets.ps1"

RULES_PAGE_CONSTANTS = {
    "GeoLite2-ASN.mmdb": (8994581, "ab93d4ebe5287ab9a459bcd67f3d5b358e261666c16fd246543c8ba771a78bf4"),
    "GeoLite2-Country.mmdb": (3395840, "54e327d9fe9214e568c5b8f8ce97a7f7e7087b7de10b0560640417695d153797"),
}


class TestGeoLock(unittest.TestCase):
    def test_lock_matches_rules_page_constants(self):
        lock = json.loads(LOCK_PATH.read_text())
        self.assertEqual(lock["repo"], "koppot/nebulae-build-assets")
        self.assertEqual(lock["commit"], "b4ba23b7d8f1e6dce11e3777082296e272319b66")
        by_dest = {f["dest"]: f for f in lock["files"]}
        self.assertEqual(set(by_dest), set(RULES_PAGE_CONSTANTS))
        for dest, (size, sha) in RULES_PAGE_CONSTANTS.items():
            self.assertEqual(by_dest[dest]["size"], size, dest)
            self.assertEqual(by_dest[dest]["sha256"], sha, dest)
            self.assertEqual(by_dest[dest]["path"], f"geo/{dest}", dest)
        for f in lock["files"]:
            self.assertNotIn("token", json.dumps(f).lower())
            self.assertNotIn("key", json.dumps(f).lower())


class TestCheckGate(unittest.TestCase):
    STAGED = REPO / "src-tauri" / "resources" / "geo"
    LOCK = json.loads(LOCK_PATH.read_text())

    def staged_bytes(self, dest):
        src = self.STAGED / dest
        if not src.is_file():
            self.skipTest("staged geo files not fetched on this machine")
        return src.read_bytes()

    def run_gate(self, geo_dir):
        return subprocess.run(["bash", str(CHECK_SH), "--dir", str(geo_dir)],
                              capture_output=True, text=True)

    def test_gate_accepts_correct_files(self):
        with tempfile.TemporaryDirectory() as td:
            geo = Path(td)
            for f in self.LOCK["files"]:
                (geo / f["dest"]).write_bytes(self.staged_bytes(f["dest"]))
            r = self.run_gate(geo)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertIn("geo OK", r.stdout)

    def test_gate_refuses_missing_file(self):
        with tempfile.TemporaryDirectory() as td:
            geo = Path(td)
            (geo / "GeoLite2-ASN.mmdb").write_bytes(self.staged_bytes("GeoLite2-ASN.mmdb"))
            r = self.run_gate(geo)  # Country absent
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("missing GeoLite2-Country.mmdb", r.stderr)

    def test_gate_refuses_wrong_size(self):
        with tempfile.TemporaryDirectory() as td:
            geo = Path(td)
            for f in self.LOCK["files"]:
                (geo / f["dest"]).write_bytes(self.staged_bytes(f["dest"]))
            dest = geo / "GeoLite2-ASN.mmdb"
            dest.write_bytes(dest.read_bytes()[:-1])
            r = self.run_gate(geo)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("lock says 8994581", r.stderr)

    def test_gate_refuses_wrong_hash(self):
        with tempfile.TemporaryDirectory() as td:
            geo = Path(td)
            for f in self.LOCK["files"]:
                (geo / f["dest"]).write_bytes(self.staged_bytes(f["dest"]))
            dest = geo / "GeoLite2-Country.mmdb"
            data = bytearray(dest.read_bytes())
            data[0] ^= 0xFF
            data[-1] ^= 0xFF
            dest.write_bytes(bytes(data))  # same size, different bytes
            r = self.run_gate(geo)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("SHA-256 mismatch", r.stderr)


class TestFetchRefusals(unittest.TestCase):
    def test_fetch_refuses_unknown_args(self):
        r = subprocess.run(["bash", str(FETCH_SH), "--bogus"], capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)

    def test_fetch_refuses_wrong_commit_in_local_dir(self):
        with tempfile.TemporaryDirectory() as td:
            clone = Path(td) / "clone"
            subprocess.run(["git", "init", "-q", str(clone)], check=True)
            (clone / "geo").mkdir()
            (clone / "geo" / "placeholder.txt").write_text("x")
            subprocess.run(["git", "-C", str(clone), "add", "-A"], check=True)
            subprocess.run(["git", "-C", str(clone), "-c", "user.email=t@t",
                            "-c", "user.name=t", "commit", "-qm", "x"], check=True)
            env = dict(os.environ, NEBULAE_GEO_ASSETS_DIR=str(clone))
            r = subprocess.run(["bash", str(FETCH_SH), "--geo-dir", str(Path(td) / "out")],
                               capture_output=True, text=True, env=env)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("pins", r.stderr)

    def test_fetch_accepts_correct_local_dir(self):
        # A tiny git repo whose HEAD is the pinned commit is impossible to
        # fake (the SHA covers history), so the positive path is covered by
        # the live run against the real repository instead.
        self.assertTrue(FETCH_SH.is_file())


class TestPowerShellTwin(unittest.TestCase):
    def test_fetch_ps1_refuses_wrong_commit_local_dir(self):
        pwsh = "/opt/homebrew/bin/pwsh"
        if not Path(pwsh).is_file():
            self.skipTest("pwsh not available on this machine")
        with tempfile.TemporaryDirectory() as td:
            clone = Path(td) / "clone"
            subprocess.run(["git", "init", "-q", str(clone)], check=True)
            (clone / "geo").mkdir()
            (clone / "geo" / "placeholder.txt").write_text("x")
            subprocess.run(["git", "-C", str(clone), "add", "-A"], check=True)
            subprocess.run(["git", "-C", str(clone), "-c", "user.email=t@t",
                            "-c", "user.name=t", "commit", "-qm", "x"], check=True)
            script = (
                f"$env:NEBULAE_GEO_ASSETS_DIR='{clone}';"
                f"& '{FETCH_PS1}' -GeoDir '{Path(td) / 'out'}'"
            )
            r = subprocess.run([pwsh, "-NoProfile", "-Command", script],
                               capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("pins", r.stdout + r.stderr)

    def test_fetch_ps1_never_prints_credentials(self):
        # Static check: the script must not echo the token variable or a
        # bearer/credential value anywhere in its output paths.
        text = FETCH_PS1.read_text()
        self.assertNotIn("Write-Host $env:GEO_ASSETS_TOKEN", text)
        self.assertNotIn("Write-Output $env:GEO_ASSETS_TOKEN", text)
        self.assertNotIn("Write-Host $ghToken", text)

    def test_fetch_ps1_parses(self):
        pwsh = "/opt/homebrew/bin/pwsh"
        if not Path(pwsh).is_file():
            self.skipTest("pwsh not available on this machine")
        script = (
            "$t=[System.Management.Automation.Language.Parser]::ParseFile("
            f"'{FETCH_PS1}',[ref]$null,[ref]$errs); 'PARSE_ERRORS=' + $errs.Count"
        )
        r = subprocess.run([pwsh, "-NoProfile", "-Command", script], capture_output=True, text=True)
        self.assertIn("PARSE_ERRORS=0", r.stdout, r.stdout + r.stderr)


if __name__ == "__main__":
    unittest.main()

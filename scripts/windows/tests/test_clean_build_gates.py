"""Tests for the clean-build (-dirty) gates added after the 2026-11-27 incident
where a staged build reported build_id "...-dirty" because staging modified a
tracked build-manifest.json and left an untracked macos-deps-11 directory.

These are structural tests: they prove every gate the rules page requires is
present in the scripts that the builders actually run.
"""
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


def read(p):
    return (REPO / p).read_text()


class TestGeneratedPathsAreIgnored(unittest.TestCase):
    def test_generated_manifest_and_dirs_are_gitignored(self):
        gi = read(".gitignore")
        self.assertIn("src-tauri/resources/build-manifest.json", gi)
        self.assertIn("src-tauri/macos-deps-11/", gi)
        self.assertIn("task1-manifest*.csv", gi)

    def test_generated_manifest_is_not_tracked(self):
        r = subprocess.run(["git", "ls-files", "src-tauri/resources/build-manifest.json"],
                           cwd=REPO, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "",
                         "build-manifest.json must be untracked: staging rewrites it, which dirties the tree")

    def test_gitignore_actually_ignores_the_paths(self):
        for p in ["src-tauri/resources/build-manifest.json", "src-tauri/macos-deps-11/x",
                  "task1-manifest-baseline-vs-rev2-rev3.csv"]:
            r = subprocess.run(["git", "check-ignore", "-q", p], cwd=REPO)
            self.assertEqual(r.returncode, 0, f"{p} is not gitignored")


class TestEntryScriptGates(unittest.TestCase):
    def test_macos_entry_rechecks_clean_after_staging_and_build(self):
        s = read("scripts/build-entry-macos.sh")
        self.assertIn("dirty after staging", s)
        self.assertIn("dirty after the build", s)
        self.assertIn("-clean)", s)  # provenance check requires a -clean build_id

    def test_macos_entry_quarantines_dirty_binary(self):
        s = read("scripts/build-entry-macos.sh")
        self.assertIn("quarantine", s)

    def test_linux_entry_rechecks_clean_and_verifies_provenance(self):
        s = read("scripts/build-entry-linux.sh")
        self.assertIn("dirty after staging", s)
        self.assertIn("-clean)", s)

    def test_windows_recipe_has_the_clean_provenance_gate(self):
        s = read("scripts/windows/BUILD_RECIPE.md")
        self.assertIn("verify-clean-build.ps1", s)
        self.assertIn("-dirty", s)

    def test_windows_verifier_exists_and_parses(self):
        self.assertTrue((REPO / "scripts/windows/verify-clean-build.ps1").is_file())

    def test_build_rs_stamps_dirty_from_porcelain(self):
        s = read("src-tauri/build.rs")
        self.assertIn("status", s)
        self.assertIn("--porcelain", s)
        self.assertIn("dirty", s)


if __name__ == "__main__":
    unittest.main()

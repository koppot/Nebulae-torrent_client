"""scripts/jre-pins.json (mac/linux/CI) and scripts/windows/inputs.lock.json (Windows kit) must pin the same JRE."""
import json
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[2]


class JrePinConsistency(unittest.TestCase):
    def setUp(self):
        self.pins = json.loads((SCRIPTS / "jre-pins.json").read_text())
        self.lock = json.loads((SCRIPTS / "windows" / "inputs.lock.json").read_text())["inputs"]

    def test_windows_pin_matches_the_kit_lock(self):
        win = self.pins["inputs"]["windows-x64"]
        lock = self.lock["temurin-jre-21013"]
        self.assertEqual((win["file"], win["bytes"], win["sha256"]), (lock["file"], lock["bytes"], lock["sha256"]))
        self.assertEqual(self.pins["version"], lock["version"])

    def test_every_pin_is_complete_and_the_same_version(self):
        for key, e in self.pins["inputs"].items():
            self.assertEqual(len(e["sha256"]), 64, key)
            self.assertGreater(e["bytes"], 1_000_000, key)
            self.assertIn("21.0.13_9", e["file"], key)

    def test_expected_keys(self):
        self.assertEqual(sorted(self.pins["inputs"]), ["linux-x64", "mac-aarch64", "mac-x64", "windows-x64"])


if __name__ == "__main__":
    unittest.main()

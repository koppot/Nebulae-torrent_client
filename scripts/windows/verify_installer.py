#!/usr/bin/env python3
"""Post-build verify gate for the Nebulae Windows installer.

Extracts a finished NSIS installer WITHOUT installing it (7z) — or takes an
already-extracted tree — and compares it against scripts/windows/expected-values.json
and, when provided, the Task 1 manifest CSV. Prints a PASS/FAIL table and
exits non-zero on any FAIL.

    verify_installer.py --installer PATH [--expected JSON] [--manifest CSV]
    verify_installer.py --dir PATH      [--expected JSON] [--manifest CSV]
                        [--report OUT.json] [--keep]

Standard library only; 7z (7-Zip) is required for --installer. Never installs,
never runs a binary, never writes outside the extraction directory (a /tmp
scratch path unless --keep is given) and the optional --report file.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_EXPECTED = Path(__file__).resolve().parent / "expected-values.json"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def find_7z() -> str:
    for cand in ("7z", "7zz", r"C:\Program Files\7-Zip\7z.exe"):
        if shutil.which(cand):
            return shutil.which(cand)  # type: ignore[return-value]
    raise SystemExit("FAIL setup: 7z not found (install 7-Zip or add it to PATH)")


class Report:
    def __init__(self) -> None:
        self.rows: list[dict] = []
        self.fail = 0

    def add(self, check: str, ok: bool, detail: str) -> None:
        self.rows.append({"check": check, "result": "PASS" if ok else "FAIL", "detail": detail})
        if not ok:
            self.fail += 1
        print(f"[{'PASS' if ok else 'FAIL'}] {check}: {detail}")

    def finish(self, out: Path | None) -> int:
        if out:
            out.write_text(json.dumps(self.rows, indent=2) + "\n")
        print(f"\n== verify: {len(self.rows) - self.fail} passed, {self.fail} failed ==")
        return 1 if self.fail else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--installer", type=Path)
    g.add_argument("--dir", type=Path, help="already-extracted installer tree")
    ap.add_argument("--expected", type=Path, default=DEFAULT_EXPECTED)
    ap.add_argument("--manifest", type=Path, help="Task 1 manifest CSV (optional)")
    ap.add_argument("--report", type=Path)
    ap.add_argument("--keep", action="store_true", help="keep the temp extraction")
    args = ap.parse_args()

    exp = json.loads(args.expected.read_text())
    rep = Report()

    if args.installer:
        tmp = Path(tempfile.mkdtemp(prefix="nebulae-verify-"))
        SevenZ = find_7z()
        r = subprocess.run([SevenZ, "x", "-y", f"-o{tmp}", str(args.installer)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(r.stdout[-2000:], r.stderr[-2000:])
            return 2
        root = tmp
        n_entries = sum(1 for _ in root.rglob("*"))
    else:
        tmp = None
        root = args.dir
        n_entries = sum(1 for _ in root.rglob("*"))

    try:
        size = args.installer.stat().st_size if args.installer else None
        lo, hi = exp["installer_size_range_bytes"]
        if size is None:
            rep.add("installer size", True, "skipped (--dir mode)")
        else:
            rep.add("installer size", lo <= size <= hi, f"{size} B in [{lo}, {hi}]")

        # --- VLC tree ---
        vlc = root / "resources" / "vlc"
        vlc_files = sorted(p.relative_to(vlc).as_posix() for p in vlc.rglob("*") if p.is_file())
        want = exp["expected_vlc_tree_files"]
        rep.add("vlc file count", len(vlc_files) == want, f"{len(vlc_files)} files (expected {want})")

        absent = [rel for rel in exp["deny_absent"] if not (vlc / rel).exists()]
        rep.add("deny-list absent", len(absent) == len(exp["deny_absent"]),
                f"{len(absent)}/{len(exp['deny_absent'])} absent"
                + ("" if not absent else f"; PRESENT: {absent}"))

        forbidden = [f for f in vlc_files if Path(f).name in exp["forbidden"]["basename_any_depth"]]
        rep.add("forbidden BD-J jars", not forbidden, "none present" if not forbidden else str(forbidden))
        stale = [f for f in vlc_files if f == "plugins.dat"]
        rep.add("no plugins.dat", not stale, "absent" if not stale else "PRESENT (stale plugin cache)")
        # 1.5.0 jars: the rc stages them under plugins/access/; the 0.2.8
        # baseline kept them at the VLC root. Accept either location.
        need = [rel for rel in exp["required_present"]
                if (vlc / rel).is_file() or (vlc / Path(rel).name).is_file()]
        rep.add("1.5.0 BD-J jars", len(need) == len(exp["required_present"]),
                f"{len(need)}/{len(exp['required_present'])} present")

        # --- Pinned hashes (VLC, engine, WebView2; JRE anchors verified per-input below) ---
        for rel, meta in exp["pinned"].items():
            if rel.startswith("resources/jre/"):
                continue  # A12: verified against the identified JRE input
            p = root / rel
            if not p.is_file():
                rep.add(f"pin {rel}", False, "MISSING")
                continue
            got = sha256(p)
            ok = got == meta["sha256"] and p.stat().st_size == meta["size"]
            detail = f"{p.stat().st_size} B {got[:16]}…"
            if not ok and "baseline_028_sha256" in meta and got == meta["baseline_028_sha256"]:
                rep.add(f"pin {rel}", False, detail + " = 0.2.8 baseline binary (documented deviation, needs approval to ship)")
            else:
                rep.add(f"pin {rel}", ok, detail)

        # --- Daemon patch markers (raw byte search, Latin-1 equivalent) ---
        daemon = root / "resources" / "engine" / "transmission" / "transmission-daemon.exe"
        if daemon.is_file():
            blob = daemon.read_bytes()
            markers = ["NEBULAE_INTEREST_KEEPALIVE", "TRANSMISSION_PEER_ID_PREFIX",
                       "TRANSMISSION_PEER_ID_SUFFIX", "TRANSMISSION_USER_AGENT"]
            missing = [m for m in markers if m.encode("ascii") not in blob]
            rep.add("daemon patch markers", not missing,
                    "all 4 present" if not missing else f"MISSING: {missing} (stock MSI daemon)")
        else:
            rep.add("daemon patch markers", False, "transmission-daemon.exe not found")

        # --- JRE tree: identify the input, then verify per-input (A12) ---
        jre = root / "resources" / "jre"
        jre_files = sum(1 for p in jre.rglob("*") if p.is_file())
        release_path = jre / "release"
        jre_inputs = exp.get("jre_inputs", {})
        identified = None
        if release_path.is_file():
            release_text = release_path.read_text(encoding="utf-8", errors="replace")
            for impl in re.findall(r'IMPLEMENTOR_VERSION="([^"]+)"', release_text):
                m = re.search(r"Temurin-(\d+\.\d+\.\d+\+\d+)", impl)
                if m:
                    identified = m.group(1)
                    break
        entry = next(
            (
                v
                for v in jre_inputs.values()
                if isinstance(v, dict) and v.get("version") == identified
            ),
            None,
        ) if identified else None
        if entry is None:
            known = ", ".join(
                str(v.get("version", "?"))
                for v in jre_inputs.values()
                if isinstance(v, dict)
            )
            rep.add(
                "jre input identified",
                False,
                "IMPLEMENTOR_VERSION in resources/jre/release does not match any expected"
                f" JRE input ({known or 'none defined'})",
            )
        else:
            rep.add("jre input identified", True, f"Temurin {entry['version']}")
            rep.add(
                "jre file count",
                jre_files == entry["files"],
                f"{jre_files} (expected {entry['files']} for {entry['version']})",
            )
            for label, path_name, size_key, hash_key in (
                ("jre release file", "release", "release_size", "release_sha256"),
                ("jre jvm.dll", "bin/server/jvm.dll", "jvm_dll_size", "jvm_dll_sha256"),
            ):
                p = jre / path_name
                if not p.is_file():
                    rep.add(label, False, "MISSING")
                    continue
                got = sha256(p)
                ok = p.stat().st_size == entry[size_key] and got == entry[hash_key]
                rep.add(label, ok, f"{p.stat().st_size} B {got[:16]}…")

        # --- GNU build: nebulae.exe must not import vcruntime140 (MSVC) ---
        exe = root / "nebulae.exe"
        if exe.is_file():
            sys.path.insert(0, str(REPO / "scripts" / "windows"))
            try:
                import pe_audit  # noqa: E402
                info = pe_audit.parse_pe(exe.read_bytes())
                bad = [d for d in info.imports + info.delay_imports
                       if d.lower().startswith(("vcruntime140", "msvcp140"))]
                rep.add("no MSVC CRT import in nebulae.exe", not bad,
                        "clean (GNU)" if not bad else f"imports {bad} (MSVC build!)")
            except Exception as e:  # noqa: BLE001
                rep.add("no MSVC CRT import in nebulae.exe", False, f"pe_audit error: {e}")
        else:
            rep.add("no MSVC CRT import in nebulae.exe", False, "nebulae.exe not found")

        # --- Optional: Task 1 manifest CSV cross-check ---
        if args.manifest:
            with open(args.manifest, newline="", encoding="utf-8") as f:
                rows = list(csv.DictReader(f))
            checked = missing = wrong = 0
            for row in rows:
                rel = (row.get("path") or "").replace("\\", "/")
                if not rel or rel.startswith("$PLUGINSDIR") or rel == "uninstall.exe":
                    continue  # per-build NSIS runtime
                base = row.get("sha256_pub") or ""
                p = root / rel
                if not p.is_file():
                    # present-in-0.2.8 app files must still exist (deny-list rows excepted)
                    if base and rel.startswith("resources/vlc/") and Path(rel).name in (
                        [Path(x).name for x in exp["deny_absent"]]
                        + [Path(x).name for x in exp["forbidden"]["basename_any_depth"]]
                    ):
                        continue
                    if base:
                        missing += 1
                        rep.add(f"manifest {rel}", False, "present in 0.2.8, MISSING now")
                    continue
                if base and rel.startswith("resources/") and not rel.startswith(
                        ("resources/vlc/plugins/", "resources/vlc/locale/")):
                    checked += 1
                    if sha256(p) != base:
                        wrong += 1
                        rep.add(f"manifest {rel}", False, "hash differs from 0.2.8 baseline")
            rep.add("manifest cross-check", missing == 0 and wrong == 0,
                    f"{checked} hashes compared, {missing} missing, {wrong} differing")

        # --- Version string (Windows only; PowerShell reads the PE resource) ---
        if args.installer and sys.platform == "win32":
            r = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 f'(Get-Item "{args.installer}").VersionInfo.FileVersion'],
                capture_output=True, text=True)
            ver = r.stdout.strip()
            rep.add("app version", ver == exp["version"], f"{ver!r} (expected {exp['version']!r})")
        else:
            rep.add("app version", True,
                    f"skipped on this OS (pre-build gate asserts tauri.conf.json = {exp['version']})")

        rep.add("entry count (informational)", True, f"{n_entries} extracted entries")
    finally:
        if tmp and not args.keep:
            shutil.rmtree(tmp, ignore_errors=True)

    if rep.finish(args.report):
        print("RESULT: FAIL — do not hand this installer to the user.")
        return 1
    print("RESULT: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Kit test for scripts/baseline_diff.py on a synthetic tree.

Covers: relative-path comparison, only-in-baseline / only-in-candidate /
changed detection, component totals, the --allow gate (exit 1 on any
unclassified difference, exit 0 when all are covered, inspection run exits 0
without --allow), .app-root normalization (Resources/ stripped), and a guard
on the shipped windows.tsv: no broad directory prefix (geo/vlc/jre/engine)
may hide a changed file, and a changed file under a protected prefix must
fail the gate by exact path.

Run: python3 scripts/tests/test_baseline_diff.py
"""
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "..", "baseline_diff.py")

BASELINE = {
    "MacOS/nebulae": b"main binary v1" + b"\0" * 100,
    "Resources/resources/jre/bin/java": b"jre v1" + b"\0" * 50,
    "Resources/resources/geo/GeoLite2-ASN.mmdb": b"geo asn v1" + b"\0" * 40,
    "Resources/resources/vlc/lib/libvlc.5.dylib": b"vlc v1" + b"\0" * 30,
}

# Candidate: same tree plus a new file, a changed file, a missing file,
# and a re-sized file.
CANDIDATE = {
    "MacOS/nebulae": b"main binary v2" + b"\0" * 100,  # changed (same size? no)
    "Resources/resources/jre/bin/java": b"jre v1" + b"\0" * 50,
    "Resources/resources/geo/GeoLite2-ASN.mmdb": b"geo asn v1" + b"\0" * 40,
    # resources/vlc/lib/libvlc.5.dylib REMOVED
    "Resources/resources/engine/tailscale/tailscaled": b"ts new" + b"\0" * 20,
    "READ ME FIRST.txt": b"install steps\n",
}

CHANGED_EXPECTED = "Resources/resources/MacOS-nope-placeholder"


def build(root, tree):
    for rel, data in tree.items():
        p = os.path.join(root, *rel.split("/"))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as fh:
            fh.write(data)


def run(args):
    return subprocess.run([sys.executable, SCRIPT] + args,
                          capture_output=True, text=True)


def main():
    tmp = tempfile.mkdtemp(prefix="nebulae-baseline-test.")
    failures = []
    try:
        base_app = os.path.join(tmp, "good", "Nebulae.app", "Contents")
        cand_app = os.path.join(tmp, "cand", "Nebulae.app", "Contents")
        build(base_app, BASELINE)
        build(cand_app, CANDIDATE)
        base_root = os.path.join(tmp, "good", "Nebulae.app")
        cand_root = os.path.join(tmp, "cand", "Nebulae.app")

        # 1. Inspection run: exit 0, finds all three difference categories.
        r = run(["--baseline", base_root, "--candidate", cand_root])
        if r.returncode != 0:
            failures.append(f"inspection run exit {r.returncode}")
        for needle in ("only in BASELINE (1)", "only in CANDIDATE (2)",
                       "same path, different content (1)",
                       "MacOS/nebulae",
                       "Resources/resources/vlc/lib/libvlc.5.dylib",
                       "Resources/resources/engine/tailscale/tailscaled",
                       "READ ME FIRST.txt"):
            if needle not in r.stdout:
                failures.append(f"inspection output missing: {needle!r}")

        # 2. Gate run without allow: exit 1.
        r = run(["--baseline", base_root, "--candidate", cand_root,
                 "--allow", os.path.join(tmp, "empty.tsv")])
        if r.returncode != 1:
            failures.append(f"gate with empty allow exited {r.returncode} (want 1)")

        # 3. Gate run with a PARTIAL allow: exit 1, names the rest.
        allow = os.path.join(tmp, "partial.tsv")
        with open(allow, "w") as fh:
            fh.write("READ ME FIRST.txt\trelease adds install steps\n")
        r = run(["--baseline", base_root, "--candidate", cand_root,
                 "--allow", allow])
        if r.returncode != 1:
            failures.append(f"gate with partial allow exited {r.returncode} (want 1)")
        if "GATE FAIL: 3" not in r.stderr:
            failures.append("partial-allow stderr should count 3 unclassified")

        # 4. Gate run with a COMPLETE allow: exit 0.
        allow = os.path.join(tmp, "full.tsv")
        with open(allow, "w") as fh:
            fh.write("READ ME FIRST.txt\trelease adds install steps\n")
            fh.write("Resources/resources/engine/tailscale/tailscaled\tnewer tailscale\n")
            fh.write("Resources/resources/vlc/lib/libvlc.5.dylib\tknown removal\n")
            fh.write("MacOS/nebulae\tversion bump\n")
        r = run(["--baseline", base_root, "--candidate", cand_root,
                 "--allow", allow])
        if r.returncode != 0:
            failures.append(f"gate with full allow exited {r.returncode} "
                            f"(want 0): {r.stderr[:200]}")
        if "GATE PASS: all 4" not in r.stdout:
            failures.append("full-allow run should print GATE PASS: all 4")

        # 4b. Prefix allow entry: "Resources/resources/vlc/" covers the
        # removed dylib without listing it.
        allow = os.path.join(tmp, "prefix.tsv")
        with open(allow, "w") as fh:
            fh.write("READ ME FIRST.txt\trelease adds install steps\n")
            fh.write("Resources/resources/engine/\tnewer tailscale\n")
            fh.write("Resources/resources/vlc/\tknown vlc churn\n")
            fh.write("MacOS/\tversion bump\n")
        r = run(["--baseline", base_root, "--candidate", cand_root,
                 "--allow", allow])
        if r.returncode != 0:
            failures.append(f"gate with prefix allow exited {r.returncode} "
                            f"(want 0): {r.stderr[:200]}")

        # 5. Report file.
        report = os.path.join(tmp, "report.md")
        run(["--baseline", base_root, "--candidate", cand_root,
             "--report", report])
        with open(report) as fh:
            rep = fh.read()
        if "| TOTAL |" not in rep or "Component byte totals" not in rep:
            failures.append("report.md missing totals table")

        # 6. The SHIPPED windows.tsv must not hide a changed file behind a
        # broad prefix. Measured derivation (2026-11-27): geo matches the
        # pinned hashes, the JRE delta is listed file-by-file, and the
        # transmission tree has no entry on purpose. Build synthetic trees
        # that change one file under each protected prefix; the real allow
        # list must FAIL the gate and name the file.
        windows_tsv = os.path.join(HERE, "..", "baseline-allow", "windows.tsv")
        with open(windows_tsv) as fh:
            shipped = [ln.split("\t")[0].strip() for ln in fh
                       if ln.strip() and not ln.startswith("#")]
        for banned in ("resources/geo/", "resources/vlc/", "resources/jre/",
                       "resources/engine/"):
            if banned in shipped:
                failures.append(f"windows.tsv re-added broad prefix: {banned}")
        for prefix in ("resources/geo/", "resources/jre/"):
            b2 = os.path.join(tmp, "b2")
            c2 = os.path.join(tmp, "c2")
            probe = prefix + "probe.bin"
            build(b2, {probe: b"original content" + b"\0" * 32})
            build(c2, {probe: b"TAMPERED CONTENT" + b"\0" * 32})
            r = run(["--baseline", b2, "--candidate", c2,
                     "--allow", windows_tsv])
            if r.returncode != 1:
                failures.append(f"changed file under {prefix} was hidden by "
                                "windows.tsv (gate exited 0)")
            elif probe not in r.stderr:
                failures.append(f"gate under {prefix} did not name {probe}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if failures:
        print("FAIL:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("test_baseline_diff: OK (7 scenarios)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

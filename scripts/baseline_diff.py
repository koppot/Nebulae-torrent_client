#!/usr/bin/env python3
"""baseline_diff.py — compare a built package against an accepted baseline.

Baseline-diff gate, rules page "Nebulae release package completeness
manifest and gate" item 1 (mandatory for every candidate on every platform).

Both roots are compared by RELATIVE path, byte size and SHA-256 (an earlier
manual comparison failed by mixing mount-prefixed paths; this script takes
two roots and only ever looks below them).

    baseline_diff.py --baseline <GOOD-root> --candidate <new-root> \
                     [--allow allow.tsv] [--report report.md]

Each root is either a directory (an extracted package, an app bundle root,
an installer's payload) or a .dmg file (mounted read-only, detached at exit).
A root that is an .app bundle is compared from inside Contents, so app and
payload trees line up.

Output: only-in-baseline, only-in-candidate, same-path-different-hash, and
byte totals per component (jre, vlc, transmission, tailscale, geo, licenses,
bdj, bluray, main binary, other). Exit 0 only when EVERY difference is
covered by the --allow file (TSV: relative path TAB reason). --allow is
optional for inspection runs; the gate run REQUIRES it.
"""
import argparse
import hashlib
import os
import subprocess
import sys
import tempfile

COMPONENTS = [
    ("jre", "resources/jre/"),
    ("vlc", "resources/vlc/"),
    ("transmission", "resources/engine/transmission/"),
    ("tailscale", "resources/engine/tailscale/"),
    ("geo", "resources/geo/"),
    ("licenses", "resources/licenses/"),
    ("bdj", "resources/vlc/lib/libbluray-j2se"),
    ("bluray", "resources/vlc/lib/libbluray"),
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def component(rel):
    r = rel.replace(os.sep, "/")
    # .app roots are walked below Contents: "Resources/resources/..." must
    # classify the same as "resources/..." in a payload tree.
    if r.startswith("Resources/"):
        r = r[len("Resources/"):]
    for name, prefix in COMPONENTS:
        if r.startswith(prefix) or r == prefix.rstrip("/"):
            return name
    if r.startswith("MacOS/") or r == "nebulae.exe" or r == "nebulae":
        return "main-binary"
    return "other"


def normalize_root(root):
    """Return (walk_root, mount_to_cleanup). An .app root is compared below
    Contents so 'Contents/Resources/...' aligns with payload trees."""
    mount = None
    if root.lower().endswith(".dmg"):
        mount = tempfile.mkdtemp(prefix="nebulae-baseline-")
        subprocess.run(["hdiutil", "attach", root, "-readonly", "-nobrowse",
                        "-mountpoint", mount], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        apps = [d for d in os.listdir(mount) if d.endswith(".app")]
        if not apps:
            cleanup(mount)
            sys.exit(f"ERROR: no .app inside {root}")
        root = os.path.join(mount, apps[0])
        mount = ("app", mount)
    if os.path.basename(root).endswith(".app"):
        contents = os.path.join(root, "Contents")
        if os.path.isdir(contents):
            walk = contents
        else:
            walk = root
    else:
        walk = root
    return walk, mount


def cleanup(mount):
    if mount and mount[0] == "app":
        subprocess.run(["hdiutil", "detach", mount[1], "-quiet"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def scan(walk):
    out = {}
    base = os.path.abspath(walk)
    for dirpath, _dirs, names in os.walk(base):
        for name in names:
            p = os.path.join(dirpath, name)
            if os.path.islink(p):
                out[os.path.relpath(p, base) + " -> " + os.readlink(p)] = ("link", 0, "")
                continue
            rel = os.path.relpath(p, base)
            out[rel] = ("file", os.path.getsize(p), sha256(p))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--candidate", required=True)
    ap.add_argument("--allow", help="TSV: relative path TAB reason; every difference must be listed")
    ap.add_argument("--report", help="write the full markdown table here")
    ap.add_argument("--max-list", type=int, default=40, help="max rows printed per category")
    args = ap.parse_args()

    base_walk, base_mnt = normalize_root(args.baseline)
    cand_walk, cand_mnt = normalize_root(args.candidate)
    try:
        base = scan(base_walk)
        cand = scan(cand_walk)
    finally:
        cleanup(base_mnt)
        cleanup(cand_mnt)

    only_base = sorted(set(base) - set(cand))
    only_cand = sorted(set(cand) - set(base))
    changed = sorted(k for k in set(base) & set(cand) if base[k] != cand[k])

    # Byte totals per component for both sides.
    totals = {}
    for label, tree in (("baseline", base), ("candidate", cand)):
        t = {}
        for rel, (_kind, size, _sha) in tree.items():
            t[component(rel)] = t.get(component(rel), 0) + size
        totals[label] = t

    lines = ["# Baseline diff", ""]
    lines.append(f"baseline: {args.baseline}  candidate: {args.candidate}")
    lines.append("")
    for title, items, tree_a, tree_b in (
            ("only in BASELINE", only_base, base, None),
            ("only in CANDIDATE", only_cand, None, cand),
            ("same path, different content", changed, base, cand)):
        lines.append(f"## {title} ({len(items)})")
        for rel in items[:args.max_list]:
            if tree_a is not None:
                lines.append(f"- {rel}  ({tree_a[rel][1]} B)")
            else:
                lines.append(f"- {rel}  ({tree_b[rel][1]} B)")
        if len(items) > args.max_list:
            lines.append(f"- ... {len(items) - args.max_list} more")
        lines.append("")

    lines.append("## Component byte totals")
    lines.append("")
    lines.append("| component | baseline B | candidate B | delta B |")
    lines.append("|---|---|---|---|")
    names = sorted(set(totals["baseline"]) | set(totals["candidate"]))
    for name in names:
        b = totals["baseline"].get(name, 0)
        c = totals["candidate"].get(name, 0)
        lines.append(f"| {name} | {b} | {c} | {c - b:+d} |")
    tb = sum(totals["baseline"].values())
    tc = sum(totals["candidate"].values())
    lines.append(f"| TOTAL | {tb} | {tc} | {tc - tb:+d} |")
    text = "\n".join(lines)
    print(text)
    if args.report:
        with open(args.report, "w") as fh:
            fh.write(text + "\n")

    allowed = set()
    if args.allow:
        with open(args.allow) as fh:
            for line in fh:
                line = line.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                allowed.add(line.split("\t")[0].strip())

    unclassified = [r for r in only_base + only_cand + changed if r not in allowed]
    if args.allow is None:
        print(f"\nINSPECTION RUN: {len(unclassified)} differences not classified "
              "(pass --allow for a gate run)")
        return 0
    if unclassified:
        print(f"\nGATE FAIL: {len(unclassified)} differences are not in the allow file:", file=sys.stderr)
        for r in unclassified[:args.max_list]:
            print(f"  {r}", file=sys.stderr)
        if len(unclassified) > args.max_list:
            print(f"  ... {len(unclassified) - args.max_list} more", file=sys.stderr)
        return 1
    print(f"\nGATE PASS: all {len(only_base) + len(only_cand) + len(changed)} "
          "differences are classified in the allow file")
    return 0


if __name__ == "__main__":
    sys.exit(main())

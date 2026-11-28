#!/usr/bin/env python3
"""Kit test: scripts that are run directly must be executable in git, and
Python gates must be started through an interpreter.

Background: build-entry-macos.sh ran ./scripts/baseline_diff.py by path while
git stored it as mode 100644, so the build died with "Permission denied" at the
last gate. Rules enforced here (git index modes, not the working tree, because
a fresh clone gets the modes from the index):

  1. every tracked *.sh file is 100755;
  2. every tracked file with a shebang line under scripts/ that is a shell
     script is 100755 (python files are always started with an interpreter,
     see 3, so their mode does not matter);
  3. no shell script, workflow or PowerShell file starts a tracked .py file by
     path: it must be 'python3 <path>', 'python <path>' or 'py <path>';
  4. build-entry-macos.sh and build-entry-linux.sh run baseline_diff.py as
     'python3 scripts/baseline_diff.py' and keep the "COULD NOT RUN" branch
     separate from "differences not classified".

Run: python3 scripts/tests/test_exec_modes.py
"""
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
INTERPRETERS = ("python3", "python", "py", "$python", "$py", "$python.source")
PY_REF = re.compile(r'(?<![\w.-])((?:\./)?(?:scripts|\$SCRIPT_DIR|\$\{SCRIPT_DIR\}|"\$SCRIPT_DIR|\$REPO_ROOT)/[\w./-]*\.py)\b')


def git(*args):
    return subprocess.run(["git", "-C", ROOT, *args], capture_output=True,
                          text=True, check=True).stdout


def main():
    failures = []
    modes = {}
    for line in git("ls-files", "-s").splitlines():
        meta, path = line.split("\t", 1)
        modes[path] = meta.split()[0]

    for path, mode in sorted(modes.items()):
        if path.endswith(".sh") and mode != "100755":
            failures.append(f"{path} is {mode}, must be 100755 (shell script)")

    scan = [p for p in modes if p.endswith((".sh", ".ps1", ".yml", ".yaml"))
            and (p.startswith("scripts/") or p.startswith(".github/workflows/"))]
    py_files = {p for p in modes if p.endswith(".py")}
    for path in sorted(scan):
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
            for n, line in enumerate(fh, 1):
                code = line.strip()
                if (not code or code.startswith("#")
                        or code.startswith(("echo ", "printf ", "Write-Host", "Write-Output"))):
                    continue
                for m in PY_REF.finditer(line):
                    ref = m.group(1).replace('"', "")
                    ref = re.sub(r"^\$\{?SCRIPT_DIR\}?", "scripts", ref)
                    ref = ref[2:] if ref.startswith("./") else ref
                    if ref not in py_files:
                        continue
                    before = line[:m.start()].rstrip().rstrip('"').lower()
                    words = [w for w in before.split() if not w.startswith("-")]
                    last = re.split(r"[\s(=`$\"]+", words[-1])[-1] if words else ""
                    if last in INTERPRETERS or before.endswith("python.source"):
                        continue
                    failures.append(f"{path}:{n} runs {ref} without an interpreter")

    for entry, allow in (("scripts/build-entry-macos.sh", "macos.tsv"),
                         ("scripts/build-entry-linux.sh", "linux.tsv")):
        with open(os.path.join(ROOT, entry)) as fh:
            body = fh.read()
        if "python3 scripts/baseline_diff.py" not in body:
            failures.append(f"{entry} must call 'python3 scripts/baseline_diff.py'")
        if "COULD NOT RUN" not in body or "differences not classified" not in body:
            failures.append(f"{entry} must report 'could not run' separately from "
                            "'differences not classified'")
        if not re.search(r"^\s*3\)", body, re.M):
            failures.append(f"{entry} must map baseline_diff.py exit 3 to 'not classified'")

    if failures:
        print("FAIL:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("test_exec_modes: OK")
    return 0


def test_exec_modes():
    assert main() == 0


if __name__ == "__main__":
    sys.exit(main())

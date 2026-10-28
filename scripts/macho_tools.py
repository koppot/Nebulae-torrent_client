#!/usr/bin/env python3
"""Mach-O helpers shared by the macOS packaging scripts (standard library only).

Imported by audit-macos-bundle.py and relocate-macos-dylibs.py. Works with the
Python 3.9 that ships with the Xcode command line tools.
"""

from __future__ import annotations

import os
import re
import subprocess

SYSTEM_PREFIXES = ("/System/Library/", "/usr/lib/")

MACHO_MAGICS = {
    b"\xcf\xfa\xed\xfe",  # MH_MAGIC_64 (little endian)
    b"\xce\xfa\xed\xfe",  # MH_MAGIC
}
FAT_MAGICS = {
    b"\xca\xfe\xba\xbe",  # FAT_MAGIC (also the first bytes of a Java class file)
    b"\xca\xfe\xba\xbf",  # FAT_MAGIC_64
}

DEP_CMDS = {
    "LC_LOAD_DYLIB",
    "LC_LOAD_WEAK_DYLIB",
    "LC_REEXPORT_DYLIB",
    "LC_LAZY_LOAD_DYLIB",
    "LC_LOAD_UPWARD_DYLIB",
}

FILETYPES = {"1": "OBJECT", "2": "EXECUTE", "6": "DYLIB", "8": "BUNDLE"}

_VERSION_RE = re.compile(r"(?:current|compatibility) version ([0-9.]+)")


def version_tuple(text: str | None) -> tuple[int, ...]:
    """'26.0' -> (26, 0), for comparing minimum-OS versions."""
    return tuple(int(part) for part in re.findall(r"\d+", text or ""))


def run(cmd: list[str]) -> str:
    """Run a tool and return stdout; failures yield whatever was printed."""
    return subprocess.run(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False
    ).stdout.decode("utf-8", "replace")


def run_checked(cmd: list[str]) -> str:
    """Run a tool and raise RuntimeError carrying its stderr on failure."""
    proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if proc.returncode != 0:
        raise RuntimeError(
            "command failed (%d): %s\n%s"
            % (proc.returncode, " ".join(cmd), proc.stderr.decode("utf-8", "replace").strip())
        )
    return proc.stdout.decode("utf-8", "replace")


def is_macho(path: str) -> bool:
    try:
        if os.path.islink(path) or not os.path.isfile(path):
            return False
        with open(path, "rb") as fh:
            head = fh.read(8)
    except OSError:
        return False
    magic = head[:4]
    if magic in MACHO_MAGICS:
        return True
    if magic in FAT_MAGICS and len(head) == 8:
        # A fat header stores a small architecture count; a Java class file
        # stores minor/major version numbers (>= 45) in the same position.
        return int.from_bytes(head[4:8], "big") < 30
    return False


def archs_of(path: str) -> list[str]:
    return run(["lipo", "-archs", path]).split()


def parse_load_commands(path: str, arch: str | None = None) -> dict:
    """Return the exact load-command strings of one Mach-O image.

    Keys: deps [(cmd, name)], rpaths, id, filetype, current_version,
    compat_version, min_os (the macOS version the image was built to need).
    Names are the literal strings stored in the binary, which is what
    install_name_tool -change has to be given.
    """
    cmd = ["otool"]
    if arch:
        cmd += ["-arch", arch]
    cmd += ["-l", path]
    text = run(cmd)
    deps: list[tuple[str, str]] = []
    rpaths: list[str] = []
    install_name = None
    current_version = None
    compat_version = None
    min_os = None
    current = None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("cmd "):
            current = line.split()[1]
            continue
        if line.startswith("Load command"):
            current = None
            continue
        if current in DEP_CMDS and line.startswith("name "):
            deps.append((current, line[5:].rsplit(" (offset", 1)[0]))
        elif current == "LC_ID_DYLIB":
            if line.startswith("name "):
                install_name = line[5:].rsplit(" (offset", 1)[0]
            else:
                m = _VERSION_RE.match(line)
                if m and line.startswith("current"):
                    current_version = m.group(1)
                elif m and line.startswith("compatibility"):
                    compat_version = m.group(1)
        elif current == "LC_RPATH" and line.startswith("path "):
            rpaths.append(line[5:].rsplit(" (offset", 1)[0])
        elif current == "LC_BUILD_VERSION" and line.startswith("minos "):
            min_os = line.split()[1]
        elif current == "LC_VERSION_MIN_MACOSX" and line.startswith("version "):
            min_os = line.split()[1]
    filetype = None
    header = run(["otool", "-h"] + (["-arch", arch] if arch else []) + [path])
    for hl in header.splitlines():
        parts = hl.split()
        if len(parts) >= 5 and parts[0].startswith("0x") and parts[4].isdigit():
            filetype = FILETYPES.get(parts[4], parts[4])
            break
    return {
        "deps": deps,
        "rpaths": rpaths,
        "id": install_name,
        "filetype": filetype,
        "current_version": current_version,
        "compat_version": compat_version,
        "min_os": min_os,
    }

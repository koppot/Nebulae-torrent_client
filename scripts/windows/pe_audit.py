#!/usr/bin/env python3
"""Static audit of the PE files in a Windows runtime tree.

Reads headers only (imports, delay-load imports, exports, machine type); it
never loads or runs a binary. Standard library only, so the same file runs on
the Linux cross-build job and on the Windows installer job.

    pe_audit.py audit ROOT [--layout nebulae] [--resolve PREFIX=DIR ...]
                           [--extra DIR ...] [--json OUT] [--allow-arch REL=ARCH ...]
                           [--fail-on-missing] [--fail-on-vcredist]
                           [--require-export REL:SYMBOL ...]
                           [--forbid-export REL:SYMBOL ...]
    pe_audit.py exports FILE [--grep TEXT]
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

MACHINES = {0x8664: "x64", 0x14C: "x86", 0xAA64: "arm64"}
PE_EXTENSIONS = {".dll", ".exe", ".ocx", ".pyd", ".node", ".drv", ".sys"}

# Present on every supported Windows 10/11 x64 installation. Anything that is
# neither bundled nor listed here or in VCREDIST_DLLS is reported as missing.
SYSTEM_DLLS = frozenset(
    """
    advapi32.dll authz.dll avrt.dll bcrypt.dll cabinet.dll cfgmgr32.dll
    comctl32.dll comdlg32.dll credui.dll crypt32.dll cryptbase.dll cryptsp.dll
    d2d1.dll d3d11.dll d3d12.dll d3d9.dll d3dcompiler_47.dll dbghelp.dll
    dcomp.dll dnsapi.dll dsound.dll dwmapi.dll dwrite.dll dxgi.dll dxva2.dll
    gdi32.dll gdi32full.dll gdiplus.dll glu32.dll hid.dll imm32.dll
    iphlpapi.dll kernel32.dll kernelbase.dll ktmw32.dll mf.dll mfplat.dll mfreadwrite.dll
    mmdevapi.dll mpr.dll msacm32.dll msi.dll msimg32.dll msvcrt.dll mswsock.dll
    ncrypt.dll netapi32.dll normaliz.dll ntdll.dll ole32.dll oleacc.dll
    oleaut32.dll opengl32.dll pdh.dll powrprof.dll propsys.dll psapi.dll
    rpcrt4.dll secur32.dll setupapi.dll shcore.dll shell32.dll shlwapi.dll
    sspicli.dll ucrtbase.dll urlmon.dll usp10.dll user32.dll userenv.dll uxtheme.dll
    version.dll wevtapi.dll windowscodecs.dll winhttp.dll wininet.dll winmm.dll
    winscard.dll winspool.drv wldap32.dll ws2_32.dll wsock32.dll wtsapi32.dll
    xinput1_4.dll xmllite.dll
    bcryptprimitives.dll combase.dll
    """.split()
)
# icuuc.dll is deliberately not listed: it is an OS component only from Windows
# 10 1903, so a file that imports it is reported as unresolved and raising the
# supported-OS floor stays a conscious choice.

# Not part of Windows: the Visual C++ runtime redistributable provides these.
VCREDIST_DLLS = frozenset(
    """
    concrt140.dll msvcp140.dll msvcp140_1.dll msvcp140_2.dll
    msvcp140_atomic_wait.dll msvcp140_codecvt_ids.dll vcamp140.dll vccorlib140.dll
    vcomp140.dll vcruntime140.dll vcruntime140_1.dll vcruntime140_threads.dll
    """.split()
)


class PeError(ValueError):
    pass


@dataclass
class PeInfo:
    machine: int
    is_dll: bool
    subsystem: int
    managed: bool
    imports: list[str] = field(default_factory=list)
    delay_imports: list[str] = field(default_factory=list)
    exports: list[str] = field(default_factory=list)

    @property
    def arch(self) -> str:
        return MACHINES.get(self.machine, hex(self.machine))


def _cstr(data: bytes, off: int) -> str:
    end = data.index(b"\0", off)
    return data[off:end].decode("latin-1")


def parse_pe(data: bytes) -> PeInfo:
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise PeError("no MZ header")
    (lfanew,) = struct.unpack_from("<I", data, 0x3C)
    if data[lfanew : lfanew + 4] != b"PE\0\0":
        raise PeError("no PE signature")
    coff = lfanew + 4
    machine, nsec, _ts, _sym, _nsym, opt_size, chars = struct.unpack_from("<HHIIIHH", data, coff)
    opt = coff + 20
    (magic,) = struct.unpack_from("<H", data, opt)
    if magic == 0x20B:
        count_off, dir_off = opt + 108, opt + 112
    elif magic == 0x10B:
        count_off, dir_off = opt + 92, opt + 96
    else:
        raise PeError(f"unknown optional header magic {magic:#x}")
    (ndirs,) = struct.unpack_from("<I", data, count_off)
    (subsystem,) = struct.unpack_from("<H", data, opt + 68)
    dirs = []
    for i in range(min(ndirs, 16)):
        if dir_off + 8 * (i + 1) > opt + opt_size:
            break
        dirs.append(struct.unpack_from("<II", data, dir_off + 8 * i))
    dirs += [(0, 0)] * (16 - len(dirs))

    sections = []
    sec_off = opt + opt_size
    for i in range(nsec):
        _name, vsize, vaddr, rawsize, rawptr = struct.unpack_from("<8sIIII", data, sec_off + 40 * i)
        sections.append((vaddr, max(vsize, rawsize), rawptr))

    def to_off(rva: int) -> int:
        for vaddr, size, rawptr in sections:
            if vaddr <= rva < vaddr + size:
                return rawptr + (rva - vaddr)
        if sections and rva < min(s[0] for s in sections):
            return rva
        raise PeError(f"RVA {rva:#x} is outside every section")

    info = PeInfo(machine=machine, is_dll=bool(chars & 0x2000), subsystem=subsystem, managed=dirs[14][0] != 0)

    rva, _size = dirs[1]
    if rva:
        off = to_off(rva)
        while True:
            oft, _t, _f, name_rva, ft = struct.unpack_from("<IIIII", data, off)
            if not (oft or name_rva or ft):
                break
            info.imports.append(_cstr(data, to_off(name_rva)))
            off += 20

    rva, _size = dirs[13]
    if rva:
        off = to_off(rva)
        while True:
            attrs, name_rva = struct.unpack_from("<II", data, off)
            if not (attrs or name_rva):
                break
            info.delay_imports.append(_cstr(data, to_off(name_rva)))
            off += 32

    rva, size = dirs[0]
    if rva:
        off = to_off(rva)
        nnames, _funcs, names_rva = struct.unpack_from("<III", data, off + 24)
        if nnames:
            names_off = to_off(names_rva)
            for i in range(nnames):
                (name_rva,) = struct.unpack_from("<I", data, names_off + 4 * i)
                info.exports.append(_cstr(data, to_off(name_rva)))
    return info


# Where the Nebulae runtime really resolves a DLL from, beyond the importing
# file's own directory. The app preloads every DLL in the VLC root by absolute
# path before libvlc, and VLC loads plugins with an altered search path; the
# Java launcher loads jvm.dll from a subdirectory of the directory that holds
# java.exe. Prefix and directories are relative to the audited root.
NEBULAE_LAYOUT = (
    ("resources/vlc/plugins", ("resources/vlc",)),
    # jvm.dll lives in jre/bin/server and finds the JRE's own VC++ runtime in
    # jre/bin: the launcher starts from bin, and lib.rs prepends jre/bin to PATH.
    ("resources/jre", ("resources/jre/bin", "resources/jre/bin/server")),
)


def classify(dll: str, local: set[str], elsewhere: set[str]) -> str:
    """local: names resolvable from the importing file's search directories.
    elsewhere: names that exist somewhere else in the tree (not resolvable)."""
    low = dll.lower()
    if low.startswith(("api-ms-win-", "ext-ms-win-")):
        return "system"
    if low in local:
        return "bundled"
    if low in elsewhere:
        return "misplaced"
    if low in SYSTEM_DLLS:
        return "system"
    if low in VCREDIST_DLLS:
        return "vcredist"
    return "missing"


def iter_pe_files(root: Path):
    for path in sorted(root.rglob("*")):
        if path.is_file() and path.suffix.lower() in PE_EXTENSIONS:
            yield path


def _search_dirs(rel_dir: str, rules) -> list[str]:
    dirs = [rel_dir]
    for prefix, more in rules:
        if rel_dir == prefix or rel_dir.startswith(prefix + "/"):
            dirs.extend(more)
    return dirs


def audit(root: Path, extra: list[Path], expect_arch: str = "x64", rules=(), arch_allow=None) -> dict:
    arch_allow = arch_allow or {}
    by_dir: dict[str, set[str]] = {}
    everywhere: set[str] = set()
    for p in iter_pe_files(root):
        rel_dir = p.parent.relative_to(root).as_posix()
        by_dir.setdefault("" if rel_dir == "." else rel_dir, set()).add(p.name.lower())
        everywhere.add(p.name.lower())
    assumed: set[str] = set()
    for base in extra:
        assumed.update(p.name.lower() for p in iter_pe_files(base))

    files = []
    wrong_arch, unreadable = [], []
    missing: dict[str, list[str]] = {}
    vcredist: dict[str, list[str]] = {}
    misplaced: dict[str, list[str]] = {}
    for path in iter_pe_files(root):
        rel = path.relative_to(root).as_posix()
        rel_dir = path.parent.relative_to(root).as_posix()
        rel_dir = "" if rel_dir == "." else rel_dir
        try:
            info = parse_pe(path.read_bytes())
        except (PeError, struct.error, ValueError) as exc:
            unreadable.append({"path": rel, "error": str(exc)})
            continue
        local = set(assumed)
        for d in _search_dirs(rel_dir, rules):
            local |= by_dir.get(d, set())
        elsewhere = everywhere - local
        entry = {
            "path": rel,
            "arch": info.arch,
            "dll": info.is_dll,
            "managed": info.managed,
            "imports": {d: classify(d, local, elsewhere) for d in info.imports},
            "delay_imports": {d: classify(d, local, elsewhere) for d in info.delay_imports},
        }
        files.append(entry)
        if info.arch != arch_allow.get(rel, expect_arch) and not info.managed:
            wrong_arch.append({"path": rel, "arch": info.arch})
        for table in ("imports", "delay_imports"):
            for dll, kind in entry[table].items():
                bucket = {"missing": missing, "vcredist": vcredist, "misplaced": misplaced}.get(kind)
                if bucket is not None:
                    bucket.setdefault(dll.lower(), []).append(rel)
    return {
        "root": str(root),
        "pe_files": len(files),
        "wrong_arch": wrong_arch,
        "unreadable": unreadable,
        "missing": missing,
        "misplaced": misplaced,
        "vcredist": vcredist,
        "files": files,
    }


def _export_check(root: Path, specs: list[str], must_exist: bool) -> list[str]:
    problems = []
    for spec in specs:
        rel, _, symbol = spec.rpartition(":")
        if not rel:
            problems.append(f"bad spec {spec!r}, expected REL:SYMBOL")
            continue
        target = root / rel
        if not target.is_file():
            problems.append(f"{rel}: file not found")
            continue
        names = parse_pe(target.read_bytes()).exports
        present = symbol in names
        if must_exist and not present:
            problems.append(f"{rel}: required export {symbol} is missing")
        if not must_exist and present:
            problems.append(f"{rel}: export {symbol} must not be present")
    return problems


def _summary(report: dict) -> str:
    lines = [f"PE files: {report['pe_files']} under {report['root']}"]
    if report["wrong_arch"]:
        lines.append(f"NOT x64: {len(report['wrong_arch'])}")
        lines += [f"  {w['path']} ({w['arch']})" for w in report["wrong_arch"][:20]]
    if report["unreadable"]:
        lines.append(f"unreadable: {len(report['unreadable'])}")
        lines += [f"  {u['path']}: {u['error']}" for u in report["unreadable"][:20]]
    if report["misplaced"]:
        lines.append("MISPLACED: present in the tree, but not where the importing file looks:")
        lines += [f"  {d} <- {len(f)} file(s), e.g. {f[0]}" for d, f in sorted(report["misplaced"].items())]
    if report["vcredist"]:
        lines.append("needs the Visual C++ runtime (not bundled anywhere):")
        lines += [f"  {d} <- {len(f)} file(s), e.g. {f[0]}" for d, f in sorted(report["vcredist"].items())]
    if report["missing"]:
        lines.append("UNRESOLVED imports (not bundled, not a Windows system DLL):")
        lines += [f"  {d} <- {len(f)} file(s), e.g. {f[0]}" for d, f in sorted(report["missing"].items())]
    if not (report["missing"] or report["misplaced"] or report["vcredist"]):
        lines.append("every import resolves")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("audit")
    a.add_argument("root", type=Path)
    a.add_argument("--extra", type=Path, action="append", default=[], help="outside directories whose DLLs count as present for every file")
    a.add_argument("--layout", choices=["nebulae"], help="apply the known search-path rules of the Nebulae runtime layout")
    a.add_argument("--resolve", action="append", default=[], metavar="PREFIX=DIR[;DIR]", help="files under PREFIX also resolve DLLs from DIR (relative to ROOT)")
    a.add_argument("--json", type=Path)
    a.add_argument("--fail-on-missing", action="store_true", help="fail on unresolved or misplaced imports")
    a.add_argument("--fail-on-vcredist", action="store_true", help="also fail when the Visual C++ runtime is needed but not bundled")
    a.add_argument("--arch", default="x64")
    a.add_argument("--allow-arch", action="append", default=[], metavar="REL=ARCH", help="expected architecture for one file, e.g. uninstall.exe=x86")
    a.add_argument("--require-export", action="append", default=[], metavar="REL:SYMBOL")
    a.add_argument("--forbid-export", action="append", default=[], metavar="REL:SYMBOL")
    e = sub.add_parser("exports")
    e.add_argument("file", type=Path)
    e.add_argument("--grep", default="")
    args = ap.parse_args(argv)

    if args.cmd == "exports":
        names = parse_pe(args.file.read_bytes()).exports
        for n in names:
            if args.grep in n:
                print(n)
        print(f"{len(names)} exports", file=sys.stderr)
        return 0

    rules = list(NEBULAE_LAYOUT) if args.layout == "nebulae" else []
    for spec in args.resolve:
        prefix, sep, dirs = spec.partition("=")
        if not sep or not prefix:
            ap.error(f"--resolve expects PREFIX=DIR[;DIR], got {spec!r}")
        rules.append((prefix.strip("/"), tuple("" if d == "." else d.strip("/") for d in dirs.split(";") if d)))
    arch_allow = {}
    for spec in args.allow_arch:
        rel, sep, arch = spec.partition("=")
        if not sep or not rel or not arch:
            ap.error(f"--allow-arch expects REL=ARCH, got {spec!r}")
        arch_allow[rel] = arch
    report = audit(args.root, args.extra, args.arch, rules, arch_allow)
    if args.json:
        args.json.write_text(json.dumps(report, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(_summary(report))
    problems = _export_check(args.root, args.require_export, True) + _export_check(args.root, args.forbid_export, False)
    for p in problems:
        print(f"EXPORT CHECK FAILED: {p}")
    failed = bool(problems or report["wrong_arch"] or report["unreadable"])
    if args.fail_on_missing and (report["missing"] or report["misplaced"]):
        failed = True
    if args.fail_on_vcredist and report["vcredist"]:
        failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Read the file tables of a Windows Installer (.msi) package without Windows.

The packages we bundle (Transmission, Tailscale, the Visual C++ redistributable)
ship their files in a cabinet whose entries are named by the MSI `File` key, not
by the real file name. This module reads the compound-file container and the
`File`/`Component`/`Directory`/`Media` tables, so the real install path of every
cabinet entry is known. Cabinet extraction itself is delegated to 7-Zip (or
expand.exe on Windows): LZX is not worth reimplementing.

Read-only and standard library only; it never executes anything from a package.

    msi_reader.py list PKG.msi
    msi_reader.py extract PKG.msi DEST [--cab-tool PATH]
"""
from __future__ import annotations

import argparse
import dataclasses
import io
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath

CFB_MAGIC = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"
_FREE, _END, _FATSECT, _DIFSECT = 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD, 0xFFFFFFFC
_MIME = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz._"
# Directory table roots. Everything below one of these is a path inside the
# package; the root's own name is not part of that path.
_ROOT_DIRS = {"TARGETDIR", "SourceDir", "ProgramFiles64Folder", "ProgramFilesFolder",
              "CommonFiles64Folder", "CommonFilesFolder", "System64Folder", "SystemFolder",
              "WindowsFolder", "LocalAppDataFolder", "AppDataFolder", "ProgramMenuFolder",
              "DesktopFolder", "StartupFolder", "TempFolder"}

# Column type flags (Windows Installer SDK, "Column Definition Format").
_T_STRING = 0x0800
_T_SIZE_MASK = 0x00FF


class MsiError(ValueError):
    pass


# --- compound file (OLE2) ---------------------------------------------------

class Cfb:
    """Minimal read-only compound file reader (MS-CFB v3 and v4)."""

    def __init__(self, data: bytes):
        if data[:8] != CFB_MAGIC:
            raise MsiError("not an OLE compound file")
        self.data = data
        (_minor, major, order, sector_shift, mini_shift) = struct.unpack_from("<HHHHH", data, 0x18)
        if order != 0xFFFE:
            raise MsiError("unsupported byte order")
        if major not in (3, 4) or sector_shift not in (9, 12):
            raise MsiError(f"unsupported compound file version {major}/{sector_shift}")
        self.sector_size = 1 << sector_shift
        self.mini_size = 1 << mini_shift
        (n_fat, first_dir, _tx, self.mini_cutoff, first_minifat, n_minifat, first_difat, n_difat) = \
            struct.unpack_from("<IIIIIIII", data, 0x2C)
        self._n_sectors = (len(data) - self.sector_size) // self.sector_size + 1

        difat = list(struct.unpack_from("<109I", data, 0x4C))
        sec = first_difat
        per = self.sector_size // 4 - 1
        for _ in range(n_difat):
            if sec in (_END, _FREE):
                break
            vals = struct.unpack_from(f"<{per + 1}I", data, self._off(sec))
            difat.extend(vals[:per])
            sec = vals[per]
        fat_sectors = [s for s in difat if s < _DIFSECT][:n_fat]
        self.fat: list[int] = []
        for s in fat_sectors:
            self.fat.extend(struct.unpack_from(f"<{self.sector_size // 4}I", data, self._off(s)))

        self.entries = self._read_directory(first_dir)
        root = self.entries[0]
        self._ministream = self._chain(root["start"], root["size"]) if root["size"] else b""
        self.minifat: list[int] = []
        if n_minifat:
            raw = self._chain_raw(first_minifat)
            self.minifat = list(struct.unpack_from(f"<{len(raw) // 4}I", raw))

    def _off(self, sector: int) -> int:
        if sector >= self._n_sectors:
            raise MsiError(f"sector {sector} is outside the file")
        return (sector + 1) * self.sector_size

    def _chain_raw(self, start: int) -> bytes:
        out = io.BytesIO()
        seen = 0
        sec = start
        while sec != _END:
            if sec >= len(self.fat) or seen > len(self.fat):
                raise MsiError("broken FAT chain")
            o = self._off(sec)
            out.write(self.data[o:o + self.sector_size])
            sec = self.fat[sec]
            seen += 1
        return out.getvalue()

    def _chain(self, start: int, size: int) -> bytes:
        return self._chain_raw(start)[:size]

    def _mini_chain(self, start: int, size: int) -> bytes:
        out = io.BytesIO()
        sec = start
        seen = 0
        while sec != _END:
            if sec >= len(self.minifat) or seen > len(self.minifat):
                raise MsiError("broken mini FAT chain")
            o = sec * self.mini_size
            out.write(self._ministream[o:o + self.mini_size])
            sec = self.minifat[sec]
            seen += 1
        return out.getvalue()[:size]

    def _read_directory(self, first_dir: int) -> list[dict]:
        raw = self._chain_raw(first_dir)
        entries = []
        for i in range(0, len(raw) - 127, 128):
            e = raw[i:i + 128]
            name_len = struct.unpack_from("<H", e, 64)[0]
            kind = e[66]
            name = e[:max(name_len - 2, 0)].decode("utf-16-le", errors="replace") if kind else ""
            start = struct.unpack_from("<I", e, 116)[0]
            size = struct.unpack_from("<Q", e, 120)[0]
            if self.sector_size == 512:
                size &= 0xFFFFFFFF
            entries.append({"name": name, "type": kind, "start": start, "size": size})
        if not entries or entries[0]["type"] != 5:
            raise MsiError("missing root directory entry")
        return entries

    def streams(self) -> dict[str, dict]:
        return {e["name"]: e for e in self.entries if e["type"] == 2}

    def read(self, entry: dict) -> bytes:
        if entry["size"] < self.mini_cutoff:
            return self._mini_chain(entry["start"], entry["size"])
        return self._chain(entry["start"], entry["size"])


# --- MSI tables --------------------------------------------------------------

def decode_stream_name(name: str) -> tuple[str, bool]:
    """Return (decoded name, is_table). Table streams carry a U+4840 prefix."""
    is_table = name[:1] == "\u4840"
    out = []
    for ch in name[1:] if is_table else name:
        c = ord(ch)
        if 0x3800 <= c < 0x4800:
            c -= 0x3800
            out.append(_MIME[c & 0x3F])
            out.append(_MIME[(c >> 6) & 0x3F])
        elif 0x4800 <= c < 0x4840:
            out.append(_MIME[c - 0x4800])
        else:
            out.append(ch)
    return "".join(out), is_table


@dataclasses.dataclass
class Column:
    name: str
    type: int

    @property
    def is_string(self) -> bool:
        return bool(self.type & _T_STRING)


class MsiPackage:
    def __init__(self, data: bytes):
        self.cfb = Cfb(data)
        self._streams: dict[str, dict] = {}
        self._tables: dict[str, dict] = {}
        for raw_name, entry in self.cfb.streams().items():
            name, is_table = decode_stream_name(raw_name)
            (self._tables if is_table else self._streams)[name] = entry
        self._load_strings()
        self._columns = self._load_columns()

    @classmethod
    def open(cls, path: Path) -> "MsiPackage":
        return cls(Path(path).read_bytes())

    # string pool -------------------------------------------------------
    def _load_strings(self) -> None:
        try:
            pool = self.cfb.read(self._tables["_StringPool"])
            blob = self.cfb.read(self._tables["_StringData"])
        except KeyError as exc:
            raise MsiError("string pool missing: not a Windows Installer database") from exc
        header = struct.unpack_from("<I", pool, 0)[0]
        self.strref = 3 if header & 0x80000000 else 2
        codepage = header & 0x7FFFFFFF
        encoding = "utf-8" if codepage in (0, 65001) else f"cp{codepage}"
        self.strings: list[str] = [""]
        pos = 0
        for i in range(4, len(pool) - 3, 4):
            length, refs = struct.unpack_from("<HH", pool, i)
            if length == 0 and refs != 0:
                raise MsiError("strings longer than 64 KiB are not supported")
            raw = blob[pos:pos + length]
            pos += length
            try:
                self.strings.append(raw.decode(encoding))
            except (UnicodeDecodeError, LookupError):
                self.strings.append(raw.decode("latin-1"))

    def _str(self, index: int) -> str:
        if index >= len(self.strings):
            raise MsiError(f"string index {index} is outside the pool")
        return self.strings[index]

    # schema -------------------------------------------------------------
    def _read_cols(self, data: bytes, widths: list[int], nrows: int) -> list[list[int]]:
        cols, pos = [], 0
        for w in widths:
            col = []
            for r in range(nrows):
                col.append(int.from_bytes(data[pos + r * w:pos + (r + 1) * w], "little"))
            pos += w * nrows
            cols.append(col)
        return cols

    def _load_columns(self) -> dict[str, list[Column]]:
        data = self.cfb.read(self._tables["_Columns"])
        row = self.strref + 2 + self.strref + 2
        nrows = len(data) // row
        tbl, num, name, typ = self._read_cols(data, [self.strref, 2, self.strref, 2], nrows)
        by_table: dict[str, dict[int, Column]] = {}
        for t, n, nm, ty in zip(tbl, num, name, typ):
            by_table.setdefault(self._str(t), {})[(n ^ 0x8000) & 0x7FFF] = Column(self._str(nm), ty ^ 0x8000)
        return {t: [cols[k] for k in sorted(cols)] for t, cols in by_table.items()}

    def table_names(self) -> list[str]:
        return sorted(self._columns)

    def rows(self, table: str) -> list[dict]:
        """All rows of a table as {column: str | int | None}."""
        if table not in self._columns or table not in self._tables:
            return []
        cols = self._columns[table]
        widths = [self.strref if c.is_string else (c.type & _T_SIZE_MASK) for c in cols]
        if any(w not in (2, 3, 4) for w in widths):
            raise MsiError(f"unsupported column widths in table {table}")
        data = self.cfb.read(self._tables[table])
        row_size = sum(widths)
        nrows = len(data) // row_size if row_size else 0
        values = self._read_cols(data, widths, nrows)
        out = []
        for r in range(nrows):
            rec = {}
            for c, w, v in zip(cols, widths, values):
                raw = v[r]
                if c.is_string:
                    rec[c.name] = self._str(raw)
                elif raw == 0:
                    rec[c.name] = None
                else:
                    rec[c.name] = raw ^ (0x8000 if w == 2 else 0x80000000)
            out.append(rec)
        return out

    def stream(self, name: str) -> bytes:
        if name not in self._streams:
            raise MsiError(f"embedded stream {name!r} not found")
        return self.cfb.read(self._streams[name])


# --- file mapping ------------------------------------------------------------

@dataclasses.dataclass(frozen=True)
class MsiFile:
    key: str          # name of the entry inside the cabinet
    path: str         # install path relative to the package root, '/'-separated
    size: int | None
    sequence: int | None
    cabinet: str | None   # Media cabinet, '#name' = embedded stream, None = uncompressed


def _long_name(spec: str) -> str:
    """'SHORT~1.EXT|Long Name.ext' -> 'Long Name.ext'."""
    return spec.split("|", 1)[1] if "|" in spec else spec


def _dir_paths(pkg: MsiPackage) -> dict[str, str]:
    rows = {r["Directory"]: r for r in pkg.rows("Directory")}
    cache: dict[str, str] = {}

    def resolve(key: str, depth: int = 0) -> str:
        if key in cache:
            return cache[key]
        if depth > 64 or key not in rows:
            raise MsiError(f"directory {key!r} has no resolvable parent")
        row = rows[key]
        parent = row["Directory_Parent"]
        name = _long_name(row["DefaultDir"] or "")
        if name.split(":")[-1] in (".", "SourceDir", ""):
            name = ""
        base = "" if (not parent or parent == key or key in _ROOT_DIRS and not name) else resolve(parent, depth + 1)
        if key in _ROOT_DIRS:
            path = base
        else:
            path = "/".join(p for p in (base, name) if p)
        cache[key] = path
        return path

    for key in rows:
        resolve(key)
    return cache


def list_files(pkg: MsiPackage) -> list[MsiFile]:
    dirs = _dir_paths(pkg)
    comps = {r["Component"]: r["Directory_"] for r in pkg.rows("Component")}
    media = sorted(pkg.rows("Media"), key=lambda r: r["LastSequence"] or 0)
    files = []
    for r in pkg.rows("File"):
        comp_dir = comps.get(r["Component_"])
        if comp_dir is None or comp_dir not in dirs:
            raise MsiError(f"file {r['File']!r} has no resolvable directory")
        name = _long_name(r["FileName"] or r["File"])
        rel = "/".join(p for p in (dirs[comp_dir], name) if p)
        cab = None
        seq = r.get("Sequence")
        for m in media:
            if seq is not None and seq <= (m["LastSequence"] or 0):
                cab = m["Cabinet"] or None
                break
        files.append(MsiFile(r["File"], rel, r.get("FileSize"), seq, cab))
    return files


def safe_rel(path: str) -> PurePosixPath:
    p = PurePosixPath(path)
    if p.is_absolute() or ".." in p.parts or any(":" in part or "\\" in part for part in p.parts):
        raise MsiError(f"refusing unsafe path {path!r}")
    return p


# --- extraction --------------------------------------------------------------

def find_cab_tool(explicit: str | None = None) -> list[str]:
    """Command prefix that extracts a cabinet: 7-Zip when present, expand.exe on Windows."""
    if explicit:
        return [explicit]
    for exe in ("7zz", "7z", "7za"):
        found = shutil.which(exe)
        if found:
            return [found]
    if sys.platform == "win32":
        for cand in (Path(r"C:\Program Files\7-Zip\7z.exe"), Path(r"C:\Program Files (x86)\7-Zip\7z.exe")):
            if cand.is_file():
                return [str(cand)]
        return ["expand.exe"]
    raise MsiError("no cabinet extractor found (install 7-Zip or pass --cab-tool)")


def extract_cab(cab: Path, dest: Path, tool: list[str]) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    if Path(tool[0]).name.lower().startswith("expand"):
        cmd = [*tool, "-F:*", str(cab), str(dest)]
    else:
        cmd = [*tool, "x", "-y", "-bd", f"-o{dest}", str(cab)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise MsiError(f"cabinet extraction failed ({cmd[0]}): {proc.stdout[-400:]}{proc.stderr[-400:]}")


def extract_msi(msi: Path, dest: Path, tool: list[str] | None = None,
                external_cabs: dict[str, Path] | None = None) -> list[MsiFile]:
    """Extract every file of the package to DEST/<install path>. Returns the file list.

    Embedded cabinets are read from the package; cabinets stored next to the MSI
    (Media.Cabinet without '#') come from EXTERNAL_CABS or the MSI's own folder.
    """
    pkg = MsiPackage.open(msi)
    files = list_files(pkg)
    tool = tool or find_cab_tool()
    external_cabs = external_cabs or {}
    dest.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="msi-cab-") as tmp:
        tmp_dir = Path(tmp)
        extracted: dict[str, Path] = {}
        for cab_name in sorted({f.cabinet for f in files if f.cabinet}):
            if cab_name.startswith("#"):
                cab_path = tmp_dir / f"{len(extracted)}.cab"
                cab_path.write_bytes(pkg.stream(cab_name[1:]))
            else:
                cab_path = external_cabs.get(cab_name) or Path(msi).parent / cab_name
                if not cab_path.is_file():
                    raise MsiError(f"external cabinet {cab_name!r} not found")
            out = tmp_dir / f"cab{len(extracted)}"
            extract_cab(cab_path, out, tool)
            extracted[cab_name] = out
        for f in files:
            if f.cabinet is None:
                raise MsiError(f"{f.key}: file is not in a cabinet; not supported")
            src = extracted[f.cabinet] / f.key
            if not src.is_file():
                raise MsiError(f"{f.key}: missing from cabinet {f.cabinet}")
            target = dest / safe_rel(f.path)
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists():
                raise MsiError(f"two files map to {f.path}")
            shutil.move(str(src), str(target))
    return files


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    ls = sub.add_parser("list")
    ls.add_argument("msi", type=Path)
    ex = sub.add_parser("extract")
    ex.add_argument("msi", type=Path)
    ex.add_argument("dest", type=Path)
    ex.add_argument("--cab-tool")
    args = ap.parse_args(argv)
    if args.cmd == "list":
        for f in list_files(MsiPackage.open(args.msi)):
            print(f"{f.size if f.size is not None else '?':>10}  {f.path}   <- {f.key}")
        return 0
    files = extract_msi(args.msi, args.dest, find_cab_tool(args.cab_tool))
    print(f"extracted {len(files)} files to {args.dest}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except MsiError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)

"""Tests for pe_audit.py. They build tiny PE32+ files in memory, so no real
binaries are needed and the suite runs on any OS:

    python3 -m unittest discover -s scripts/windows/tests
"""
import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pe_audit  # noqa: E402

SECTION_RVA = 0x1000
SECTION_OFF = 0x200


def make_pe(imports=(), delay=(), exports=(), machine=0x8664, dll=True) -> bytes:
    """A minimal PE32+ whose only section holds import, delay-import and export tables."""
    body = bytearray()

    def rva() -> int:
        return SECTION_RVA + len(body)

    def cstr(text: str) -> int:
        at = rva()
        body.extend(text.encode("latin-1") + b"\0")
        return at

    # Strings first so every table can point back at them.
    import_names = [cstr(n) for n in imports]
    delay_names = [cstr(n) for n in delay]
    export_names = [cstr(n) for n in exports]
    dll_name = cstr("test.dll")

    import_dir = rva() if imports else 0
    for name_rva in import_names:
        body.extend(struct.pack("<IIIII", 1, 0, 0, name_rva, 1))
    if imports:
        body.extend(b"\0" * 20)

    delay_dir = rva() if delay else 0
    for name_rva in delay_names:
        body.extend(struct.pack("<II", 1, name_rva) + b"\0" * 24)
    if delay:
        body.extend(b"\0" * 32)

    export_dir = 0
    if exports:
        names_table = rva()
        for name_rva in export_names:
            body.extend(struct.pack("<I", name_rva))
        export_dir = rva()
        body.extend(struct.pack("<IIHHIIIIIII", 0, 0, 0, 0, dll_name, 1, len(exports), len(exports), names_table, names_table, names_table))

    dirs = [(0, 0)] * 16
    if exports:
        dirs[0] = (export_dir, 40)
    if imports:
        dirs[1] = (import_dir, 20 * (len(imports) + 1))
    if delay:
        dirs[13] = (delay_dir, 32 * (len(delay) + 1))

    opt = bytearray(240)
    struct.pack_into("<H", opt, 0, 0x20B)
    struct.pack_into("<H", opt, 68, 3)  # console subsystem
    struct.pack_into("<I", opt, 108, 16)
    for i, (r, s) in enumerate(dirs):
        struct.pack_into("<II", opt, 112 + 8 * i, r, s)

    chars = 0x2022 if dll else 0x0022
    coff = struct.pack("<HHIIIHH", machine, 1, 0, 0, 0, len(opt), chars)
    section = struct.pack("<8sIIIIIIHHI", b".rdata", len(body), SECTION_RVA, len(body), SECTION_OFF, 0, 0, 0, 0, 0x40000040)
    header = bytearray(0x40)
    header[:2] = b"MZ"
    struct.pack_into("<I", header, 0x3C, 0x40)
    blob = bytes(header) + b"PE\0\0" + coff + bytes(opt) + section
    blob += b"\0" * (SECTION_OFF - len(blob)) + bytes(body)
    return blob


class PeFixture(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._scratch = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.scratch = Path(self._scratch.name)

    def tearDown(self):
        self._tmp.cleanup()
        self._scratch.cleanup()

    def put(self, rel: str, **kw) -> Path:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(make_pe(**kw))
        return path

    def run_cli(self, *args) -> int:
        return pe_audit.main(["audit", str(self.root), *args])


class ParseTests(PeFixture):
    def test_reads_imports_delay_imports_exports_and_arch(self):
        info = pe_audit.parse_pe(make_pe(imports=["A.dll", "B.dll"], delay=["C.dll"], exports=["alpha", "beta"]))
        self.assertEqual(info.imports, ["A.dll", "B.dll"])
        self.assertEqual(info.delay_imports, ["C.dll"])
        self.assertEqual(info.exports, ["alpha", "beta"])
        self.assertEqual(info.arch, "x64")
        self.assertTrue(info.is_dll)

    def test_rejects_non_pe(self):
        with self.assertRaises(pe_audit.PeError):
            pe_audit.parse_pe(b"not a pe file at all" * 8)


class ResolutionTests(PeFixture):
    def kinds(self, report, rel):
        entry = next(f for f in report["files"] if f["path"] == rel)
        return {**entry["imports"], **entry["delay_imports"]}

    def test_system_dlls_and_api_sets_resolve(self):
        self.put("app.exe", imports=["KERNEL32.dll", "api-ms-win-crt-runtime-l1-1-0.dll"], dll=False)
        kinds = self.kinds(pe_audit.audit(self.root, []), "app.exe")
        self.assertEqual(set(kinds.values()), {"system"})

    def test_same_directory_dll_is_bundled_case_insensitively(self):
        self.put("app.exe", imports=["Helper.DLL"], dll=False)
        self.put("helper.dll")
        self.assertEqual(self.kinds(pe_audit.audit(self.root, []), "app.exe"), {"Helper.DLL": "bundled"})

    def test_dll_in_another_directory_is_misplaced_not_bundled(self):
        self.put("engine/daemon.exe", imports=["VCRUNTIME140.dll"], dll=False)
        self.put("jre/bin/vcruntime140.dll")
        report = pe_audit.audit(self.root, [])
        self.assertEqual(self.kinds(report, "engine/daemon.exe"), {"VCRUNTIME140.dll": "misplaced"})
        self.assertEqual(list(report["misplaced"]), ["vcruntime140.dll"])

    def test_missing_visual_cpp_runtime_is_reported_as_vcredist(self):
        self.put("daemon.exe", imports=["MSVCP140.dll"], dll=False)
        report = pe_audit.audit(self.root, [])
        self.assertEqual(list(report["vcredist"]), ["msvcp140.dll"])
        self.assertEqual(report["missing"], {})

    def test_unknown_dll_is_missing(self):
        self.put("app.exe", imports=["mystery.dll"], dll=False)
        self.assertEqual(list(pe_audit.audit(self.root, [])["missing"]), ["mystery.dll"])

    def test_delay_imports_are_checked_too(self):
        self.put("app.exe", delay=["mystery.dll"], dll=False)
        self.assertEqual(list(pe_audit.audit(self.root, [])["missing"]), ["mystery.dll"])

    def test_resolve_rule_lets_plugins_use_the_parent_directory(self):
        self.put("plugins/p.dll", imports=["core.dll"])
        self.put("core.dll")
        self.assertEqual(list(pe_audit.audit(self.root, [])["misplaced"]), ["core.dll"])
        report = pe_audit.audit(self.root, [], rules=[("plugins", ("",))])
        self.assertEqual(report["misplaced"], {})
        self.assertEqual(self.kinds(report, "plugins/p.dll"), {"core.dll": "bundled"})

    def test_extra_directories_count_as_present_everywhere(self):
        (self.scratch / "redist.dll").write_bytes(make_pe())
        self.put("app.exe", imports=["redist.dll"], dll=False)
        report = pe_audit.audit(self.root, [self.scratch])
        self.assertEqual(report["missing"], {})
        self.assertEqual(self.kinds(report, "app.exe"), {"redist.dll": "bundled"})


class ArchitectureAndExportTests(PeFixture):
    def test_wrong_architecture_is_reported_and_can_be_allowed(self):
        self.put("uninstall.exe", machine=0x14C, dll=False)
        self.assertEqual(pe_audit.audit(self.root, [])["wrong_arch"], [{"path": "uninstall.exe", "arch": "x86"}])
        report = pe_audit.audit(self.root, [], arch_allow={"uninstall.exe": "x86"})
        self.assertEqual(report["wrong_arch"], [])

    def test_export_requirements(self):
        self.put("libvlc.dll", exports=["libvlc_new", "libvlc_media_player_navigate_menu"])
        self.assertEqual(pe_audit._export_check(self.root, ["libvlc.dll:libvlc_new"], True), [])
        self.assertTrue(pe_audit._export_check(self.root, ["libvlc.dll:libvlc_missing"], True))
        self.assertTrue(pe_audit._export_check(self.root, ["libvlc.dll:libvlc_new"], False))
        self.assertTrue(pe_audit._export_check(self.root, ["absent.dll:libvlc_new"], True))


class CommandLineTests(PeFixture):
    def test_exit_codes_follow_the_strictness_flags(self):
        self.put("daemon.exe", imports=["VCRUNTIME140.dll"], dll=False)
        self.assertEqual(self.run_cli(), 0)
        self.assertEqual(self.run_cli("--fail-on-missing"), 0)
        self.assertEqual(self.run_cli("--fail-on-vcredist"), 1)

    def test_misplaced_import_fails_fail_on_missing(self):
        self.put("engine/daemon.exe", imports=["helper.dll"], dll=False)
        self.put("elsewhere/helper.dll")
        self.assertEqual(self.run_cli("--fail-on-missing"), 1)
        self.assertEqual(self.run_cli("--fail-on-missing", "--resolve", "engine=elsewhere"), 0)

    def test_json_report_is_written(self):
        self.put("app.exe", imports=["KERNEL32.dll"], dll=False)
        out = self.scratch / "report.json"
        self.assertEqual(self.run_cli("--json", str(out)), 0)
        self.assertEqual(json.loads(out.read_text())["pe_files"], 1)

    def test_wrong_architecture_fails(self):
        self.put("app.exe", machine=0xAA64, dll=False)
        self.assertEqual(self.run_cli(), 1)
        self.assertEqual(self.run_cli("--allow-arch", "app.exe=arm64"), 0)


if __name__ == "__main__":
    unittest.main()

"""Tests for stage_runtime.py, fetch_inputs.py and the msi_reader helpers.

Synthetic zips, tarballs, PE files (test_pe_audit.make_pe) and a stubbed
downloader, so the suite needs no network and no real binaries:

    python3 -m unittest discover -s scripts/windows/tests
"""
import io
import json
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import fetch_inputs  # noqa: E402
import msi_reader  # noqa: E402
import pe_audit  # noqa: E402
import stage_runtime  # noqa: E402
from test_pe_audit import make_pe  # noqa: E402

LOCK = {
    "schema": 1,
    "inputs": {
        "thing": {
            "file": "thing.bin",
            "url": "https://example.invalid/thing.bin",
            "sha256": "3a6eb0790f39ac87c94f3856b2dd2c5d110e6811602261a9a923d3bb05adc8b7",
            "bytes": 5,
        }
    },
}


class Fixture(unittest.TestCase):
    def setUp(self):
        self._tmps = [tempfile.TemporaryDirectory() for _ in range(3)]
        self.root, self.cache, self.scratch = (Path(t.name) for t in self._tmps)

    def tearDown(self):
        for t in self._tmps:
            t.cleanup()

    def write_lock(self, data=None) -> Path:
        path = self.scratch / "lock.json"
        path.write_text(json.dumps(data if data is not None else LOCK))
        return path


class LockValidationTests(Fixture):
    def load(self, mutate):
        data = json.loads(json.dumps(LOCK))
        mutate(data)
        return fetch_inputs.load_lock(self.write_lock(data))

    def test_roundtrip_ok(self):
        self.assertIn("thing", fetch_inputs.load_lock(self.write_lock()))

    def test_rejects_bad_hash(self):
        for bad in ("ZZ" + "0" * 62, "0" * 63, "A" * 64):
            with self.assertRaises(fetch_inputs.FetchError):
                self.load(lambda d: d["inputs"]["thing"].update(sha256=bad))

    def test_rejects_non_https(self):
        with self.assertRaises(fetch_inputs.FetchError):
            self.load(lambda d: d["inputs"]["thing"].update(url="http://example.invalid/x"))

    def test_rejects_unsafe_filename(self):
        for bad in ("../x", "a/b", "", ".."):
            with self.assertRaises(fetch_inputs.FetchError):
                self.load(lambda d: d["inputs"]["thing"].update(file=bad))

    def test_rejects_unknown_schema(self):
        with self.assertRaises(fetch_inputs.FetchError):
            self.load(lambda d: d.update(schema=99))


class FetchTests(Fixture):
    def entry(self):
        return LOCK["inputs"]["thing"]

    def test_offline_missing_fails(self):
        with self.assertRaises(fetch_inputs.FetchError):
            fetch_inputs.ensure("thing", self.entry(), self.cache, offline=True)

    def test_existing_matching_file_is_reused(self):
        (self.cache / "thing.bin").write_bytes(b"\x00" * 5)
        # sha256 does not match, so a reuse must not happen; offline turns that into an error
        with self.assertRaises(fetch_inputs.FetchError):
            fetch_inputs.ensure("thing", self.entry(), self.cache, offline=True)

    def test_download_verifies_hash(self):
        payload = b"01234"
        good = {"sha256": fetch_inputs.hashlib.sha256(payload).hexdigest(), "bytes": 5}

        class Resp:
            sent = False

            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

            def read(self, n=-1):
                if self.sent:
                    return b""
                self.sent = True
                return payload

        import unittest.mock as mock
        entry = dict(self.entry(), **good)
        with mock.patch.object(fetch_inputs, "_open", return_value=Resp()):
            out = fetch_inputs.ensure("thing", entry, self.cache)
        self.assertEqual(out.read_bytes(), payload)
        # a second call reuses the verified cache file without downloading
        with mock.patch.object(fetch_inputs, "_open", side_effect=AssertionError("network used")):
            fetch_inputs.ensure("thing", entry, self.cache)

    def test_download_mismatch_leaves_no_file(self):
        class Resp:
            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

            def read(self, n=-1):
                return b"wrong"

        import unittest.mock as mock
        with mock.patch.object(fetch_inputs, "_open", return_value=Resp()):
            with self.assertRaises(fetch_inputs.FetchError):
                fetch_inputs.ensure("thing", self.entry(), self.cache)
        self.assertEqual(list(self.cache.iterdir()), [])


def make_zip(path: Path, members: dict[str, bytes], symlinks=()) -> Path:
    with zipfile.ZipFile(path, "w") as zf:
        for name, data in members.items():
            info = zipfile.ZipInfo()
            info.filename = name
            zf.writestr(info, data)
        for name in symlinks:
            info = zipfile.ZipInfo()
            info.filename = name
            info.external_attr = (0o120777 << 16)
            zf.writestr(info, b"target")
    return path


def make_tar(path: Path, members: dict[str, bytes]) -> Path:
    with tarfile.open(path, "w:xz") as tf:
        for name, data in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    return path


class ExtractZipTests(Fixture):
    def setUp(self):
        super().setUp()
        self.rec = stage_runtime.Recorder(self.root)

    def extract(self, members, **kw):
        zf = make_zip(self.scratch / "in.zip", members, kw.pop("symlinks", ()))
        return stage_runtime.extract_zip(zf, self.root, "pkg/", self.rec, "test", **kw)

    def test_strips_prefix_and_records_hashes(self):
        n = self.extract({"pkg/a.txt": b"a", "pkg/dir/b.txt": b"bb", "other/c.txt": b"c"})
        self.assertEqual(n, 2)
        self.assertEqual((self.root / "a.txt").read_bytes(), b"a")
        self.assertEqual({f["path"] for f in self.rec.files}, {"a.txt", "dir/b.txt"})

    def test_deny_lists(self):
        n = self.extract({"pkg/vlc.exe": b"x", "pkg/msi/x.wxs": b"x", "pkg/skins/s": b"x",
                          "pkg/plugins/vlc.exe": b"keep", "pkg/libvlc.dll": b"d"},
                         deny_dirs=stage_runtime.VLC_DENY_DIRS, deny_files=stage_runtime.VLC_DENY_FILES)
        self.assertEqual(n, 2)
        paths = {f["path"] for f in self.rec.files}
        self.assertEqual(paths, {"plugins/vlc.exe", "libvlc.dll"})

    def test_rejects_traversal_and_backslash(self):
        for bad in ("pkg/../evil", "/abs", "pkg/a\\b", "pkg/C:/x"):
            with self.assertRaises(StageErrorOrValueError, msg=bad):
                self.extract({bad: b"x", "pkg/ok.txt": b"y"})

    def test_rejects_symlink(self):
        with self.assertRaises(StageErrorOrValueError):
            self.extract({"pkg/ok.txt": b"y"}, symlinks=("pkg/link.txt",))


# StageError and ValueError share a base only through Exception; make one alias
# for assertRaises clarity.
StageErrorOrValueError = (stage_runtime.StageError, ValueError)


class ExtractTarTests(Fixture):
    def test_extracts_wanted_and_fails_on_missing(self):
        rec = stage_runtime.Recorder(self.root)
        tar = make_tar(self.scratch / "in.tar.xz", {"vlc-3.0.23/COPYING": b"GPL", "vlc-3.0.23/src/x.c": b"c"})
        stage_runtime.extract_tar_members(tar, {"vlc-3.0.23/COPYING": "lic/COPYING"}, rec, "test")
        self.assertEqual((self.root / "lic/COPYING").read_bytes(), b"GPL")
        with self.assertRaises(stage_runtime.StageError):
            stage_runtime.extract_tar_members(tar, {"vlc-3.0.23/NOPE": "lic/NOPE"}, rec, "test")


class FindCabsTests(Fixture):
    def cab_header(self, size, major=1, minor=3):
        h = bytearray(44)
        h[:4] = b"MSCF"
        struct.pack_into("<I", h, 8, size)
        struct.pack_into("<BB", h, 24, minor, major)
        return bytes(h)

    def test_finds_plausible_cabs(self):
        blob = b"x" * 100 + self.cab_header(50) + b"y" * 46 + self.cab_header(46) + b"z" * 42
        # second header starts at 100 + 44 + 46 = 190; sizes must exceed the 44-byte header
        self.assertEqual(stage_runtime._find_cabs(blob), [(100, 50), (190, 46)])

    def test_ignores_implausible(self):
        blob = self.cab_header(10) + b"MSCF" + b"\0" * 30  # too small / truncated
        self.assertEqual(stage_runtime._find_cabs(blob), [])


class AuditGateTests(Fixture):
    """audit_tree on a synthetic Nebulae layout."""

    def build_tree(self, custom_exports=(), with_vcrt=True):
        put = lambda rel, **kw: (self.root / rel).parent.mkdir(parents=True, exist_ok=True) or \
            (self.root / rel).write_bytes(make_pe(**kw))
        put("resources/vlc/libvlc.dll", exports=["libvlc_media_player_navigate", *custom_exports])
        put("resources/vlc/libvlccore.dll")
        put("resources/vlc/plugins/access/liblibbluray_plugin.dll", imports=["libvlccore.dll"])
        put("resources/engine/transmission/transmission-daemon.exe",
            dll=False, imports=["libcurl.dll", "vcruntime140.dll", "kernel32.dll"])
        put("resources/engine/transmission/libcurl.dll", imports=["zlib.dll"])
        put("resources/engine/transmission/zlib.dll")
        if with_vcrt:
            put("resources/engine/transmission/vcruntime140.dll")
        rec = stage_runtime.Recorder(self.root)
        return rec

    def test_stock_tree_passes_unpatched(self):
        rec = self.build_tree()
        report = stage_runtime.audit_tree(rec, patched=False)
        self.assertEqual(report["missing"], {})

    def test_stock_tree_fails_when_patched_expected(self):
        rec = self.build_tree()
        with self.assertRaises(stage_runtime.StageError):
            stage_runtime.audit_tree(rec, patched=True)

    def test_patched_tree_passes_with_custom_exports(self):
        rec = self.build_tree(custom_exports=stage_runtime.CUSTOM_EXPORTS)
        stage_runtime.audit_tree(rec, patched=True)

    def test_missing_vc_runtime_fails(self):
        rec = self.build_tree(with_vcrt=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.audit_tree(rec, patched=False)
        self.assertIn("vcruntime140.dll", str(ctx.exception))

    def test_recorder_rejects_double_stage(self):
        rec = stage_runtime.Recorder(self.root)
        rec.write("a/b.txt", b"1", "test")
        with self.assertRaises(stage_runtime.StageError):
            rec.write("a/b.txt", b"2", "test")


class MsiHelperTests(unittest.TestCase):
    def test_decode_plain_table_name(self):
        self.assertEqual(msi_reader.decode_stream_name("\u4840File"), ("File", True))
        self.assertEqual(msi_reader.decode_stream_name("_StringData"), ("_StringData", False))

    def test_decode_encoded_pairs(self):
        # '!' and '?' are not in the MIME alphabet, so MSI encodes such pairs
        # as one char: 0x3800 + i0 + (i1 << 6) with indexes into the alphabet.
        mime = msi_reader._MIME
        c = chr(0x3800 + mime.index("0") + (mime.index("1") << 6))
        self.assertEqual(msi_reader.decode_stream_name("\u4840" + c), ("01", True))

    def test_long_name(self):
        self.assertEqual(msi_reader._long_name("TRANSM~1.EXE|transmission-daemon.exe"),
                         "transmission-daemon.exe")
        self.assertEqual(msi_reader._long_name("plain.dll"), "plain.dll")

    def test_safe_rel(self):
        self.assertEqual(str(msi_reader.safe_rel("a/b/c.txt")), "a/b/c.txt")
        for bad in ("../x", "/x", "a\\b", "C:/x", "a/../../y"):
            with self.assertRaises(msi_reader.MsiError, msg=bad):
                msi_reader.safe_rel(bad)


def make_mock_jar():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("org/videolan/Libbluray.class", b"cafebabe")
    return buf.getvalue()


def make_mock_mmdb(db_type="GeoLite2-Country"):
    header = b"\x00" * 16384
    marker = b"\xab\xcd\xefMaxMind.com"
    metadata = f'{{"database_type":"{db_type}"}}'.encode("utf-8")
    return header + marker + metadata


class StageVlcContractTests(Fixture):
    def setUp(self):
        super().setUp()
        self.vlc_dir = self.scratch / "vlc-modified"
        self.vlc_dir.mkdir(parents=True, exist_ok=True)
        self.rec = stage_runtime.Recorder(self.root)

    def _create_complete_modified_tree(self, custom_exports=stage_runtime.CUSTOM_EXPORTS,
                                       with_core=True, with_plugin=True, with_jars=True,
                                       with_manifest=True, machine=0x8664, stock_jars=False,
                                       corrupt_jar=False, with_plugins_dir=True,
                                       plugin_count=12, with_video_output=True):
        (self.vlc_dir / "libvlc.dll").write_bytes(make_pe(exports=list(custom_exports), machine=machine))
        if with_core:
            (self.vlc_dir / "libvlccore.dll").write_bytes(make_pe(machine=machine))
        if with_plugins_dir:
            access = self.vlc_dir / "plugins" / "access"
            access.mkdir(parents=True, exist_ok=True)
            if with_plugin:
                (access / "liblibbluray_plugin.dll").write_bytes(make_pe(machine=machine))
            if with_jars:
                jar_data = b"not a zip file" if corrupt_jar else make_mock_jar()
                (access / "libbluray-j2se-1.5.0.jar").write_bytes(jar_data)
                (access / "libbluray-awt-j2se-1.5.0.jar").write_bytes(jar_data)
            if stock_jars:
                (access / "libbluray-j2se-1.4.0.jar").write_bytes(make_mock_jar())
            if with_video_output:
                vout = self.vlc_dir / "plugins" / "video_output"
                vout.mkdir(parents=True, exist_ok=True)
                (vout / "libdirect3d11_plugin.dll").write_bytes(make_pe(machine=machine))
                (vout / "libgl_plugin.dll").write_bytes(make_pe(machine=machine))
                for i in range(plugin_count):
                    (vout / f"libmock{i}_plugin.dll").write_bytes(make_pe(machine=machine))
            else:
                for i in range(plugin_count):
                    (access / f"libmock{i}_plugin.dll").write_bytes(make_pe(machine=machine))
        else:
            if with_plugin:
                (self.vlc_dir / "liblibbluray_plugin.dll").write_bytes(make_pe(machine=machine))
            if with_jars:
                jar_data = b"not a zip file" if corrupt_jar else make_mock_jar()
                (self.vlc_dir / "libbluray-j2se-1.5.0.jar").write_bytes(jar_data)
                (self.vlc_dir / "libbluray-awt-j2se-1.5.0.jar").write_bytes(jar_data)
        if with_manifest:
            manifest = {
                "vlc_version": "3.0.23",
                "libbluray_version": "1.5.0",
                "toolchain": "x86_64-w64-mingw32-gcc (GCC) 16.1.0 (UCRT)",
                "build_date": "2026-10-29",
                "patches": [
                    "navigate-menu.patch",
                    "bluray-playlist-id.patch",
                    "bluray-menu-first.patch"
                ],
                "exports": list(custom_exports)
            }
            (self.vlc_dir / "patched-vlc-manifest.json").write_text(json.dumps(manifest))

    def test_complete_modified_tree_succeeds(self):
        self._create_complete_modified_tree()
        res = stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertTrue(res)
        self.assertTrue((self.root / "resources" / "vlc" / "libvlc.dll").is_file())
        self.assertTrue((self.root / "resources" / "vlc" / "libvlccore.dll").is_file())
        self.assertTrue((self.root / "resources" / "vlc" / "plugins" / "access" / "liblibbluray_plugin.dll").is_file())
        self.assertTrue((self.root / "resources" / "vlc" / "plugins" / "access" / "libbluray-j2se-1.5.0.jar").is_file())

    def test_rejects_missing_tree_when_require_modified(self):
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, None, None, require_modified=True)
        self.assertIn("stock VLC runtime substitution is strictly prohibited", str(ctx.exception))

    def test_rejects_partial_overlay_lacking_vlccore(self):
        self._create_complete_modified_tree(with_core=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("partial overlay rejected", str(ctx.exception))
        self.assertIn("must include libvlccore.dll", str(ctx.exception))

    def test_rejects_missing_bd_jars(self):
        self._create_complete_modified_tree(with_jars=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("lacks required BD-J 1.5.0 jar", str(ctx.exception))

    def test_rejects_forbidden_stock_jars(self):
        self._create_complete_modified_tree(stock_jars=True)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("forbidden stock BD-J jar", str(ctx.exception))

    def test_rejects_missing_provenance_manifest(self):
        self._create_complete_modified_tree(with_manifest=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("lacks build provenance manifest", str(ctx.exception))

    def test_rejects_non_x64_dll(self):
        self._create_complete_modified_tree(machine=0x14c)  # 32-bit x86
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("not x64 architecture", str(ctx.exception))

    def test_audit_rejects_double_resources_nesting(self):
        rec = stage_runtime.Recorder(self.root)
        bad_dir = self.root / "resources" / "resources" / "vlc"
        bad_dir.mkdir(parents=True, exist_ok=True)
        (bad_dir / "libvlc.dll").write_bytes(make_pe(exports=["libvlc_media_player_navigate", *stage_runtime.CUSTOM_EXPORTS]))
        rec.add(bad_dir / "libvlc.dll", "test")
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.audit_tree(rec, patched=True)
        self.assertIn("double resources nesting", str(ctx.exception))

    def test_rejects_corrupt_bdj_jar(self):
        self._create_complete_modified_tree(corrupt_jar=True)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("corrupt or not a valid jar/zip archive", str(ctx.exception))

    def test_rejects_modified_tree_without_plugins_dir(self):
        self._create_complete_modified_tree(with_plugins_dir=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("lacks a plugins directory", str(ctx.exception))

    def test_rejects_incomplete_plugin_suite(self):
        self._create_complete_modified_tree(plugin_count=1)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("contains only", str(ctx.exception))
        self.assertIn("expected complete VLC plugin suite", str(ctx.exception))

    def test_rejects_missing_plugin_category(self):
        self._create_complete_modified_tree(with_video_output=False)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.stage_vlc(self.cache, {}, self.rec, self.vlc_dir, None, require_modified=True)
        self.assertIn("lacks required plugin category", str(ctx.exception))

    def test_stage_geo_validates_mmdb_format(self):
        geo_dir = self.scratch / "geo"
        geo_dir.mkdir(parents=True, exist_ok=True)
        country_file = geo_dir / "GeoLite2-Country.mmdb"
        asn_file = geo_dir / "GeoLite2-ASN.mmdb"
        country_file.write_bytes(make_mock_mmdb("GeoLite2-Country"))
        asn_file.write_bytes(make_mock_mmdb("GeoLite2-ASN"))

        sha_c = stage_runtime.validate_mmdb_format(country_file, "GeoLite2-Country")
        sha_a = stage_runtime.validate_mmdb_format(asn_file, "GeoLite2-ASN")
        self.assertEqual(len(sha_c), 64)
        self.assertEqual(len(sha_a), 64)

    def test_stage_geo_rejects_missing_metadata_marker(self):
        geo_dir = self.scratch / "geo"
        geo_dir.mkdir(parents=True, exist_ok=True)
        bad_file = geo_dir / "GeoLite2-Country.mmdb"
        bad_file.write_bytes(b"\x00" * 20000)
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.validate_mmdb_format(bad_file, "GeoLite2-Country")
        self.assertIn("MaxMind metadata marker not found", str(ctx.exception))

    def test_stage_geo_rejects_wrong_database_type(self):
        geo_dir = self.scratch / "geo"
        geo_dir.mkdir(parents=True, exist_ok=True)
        bad_file = geo_dir / "GeoLite2-Country.mmdb"
        bad_file.write_bytes(make_mock_mmdb("GeoLite2-City"))
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.validate_mmdb_format(bad_file, "GeoLite2-Country")
        self.assertIn("metadata does not declare expected database type", str(ctx.exception))

    def test_stage_geo_rejects_too_small(self):
        geo_dir = self.scratch / "geo"
        geo_dir.mkdir(parents=True, exist_ok=True)
        bad_file = geo_dir / "GeoLite2-Country.mmdb"
        bad_file.write_bytes(b"short")
        with self.assertRaises(stage_runtime.StageError) as ctx:
            stage_runtime.validate_mmdb_format(bad_file, "GeoLite2-Country")
        self.assertIn("too small to be a valid MMDB file", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()

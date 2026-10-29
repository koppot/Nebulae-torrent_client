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
            zf.writestr(name, data)
        for name in symlinks:
            info = zipfile.ZipInfo(name)
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


if __name__ == "__main__":
    unittest.main()

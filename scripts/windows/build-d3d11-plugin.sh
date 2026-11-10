#!/usr/bin/env bash
# scripts/windows/build-d3d11-plugin.sh --work DIR --out DIR [--test-dump]
#
# Cross-builds modules/video_output/libdirect3d11_plugin.dll for Windows x64 from the pristine
# VLC 3.0.23 tarball plus patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch, with mingw-w64
# (-mcrtdll=msvcrt-os, the same C runtime closure as the rest of the Nebulae modified runtime).
#
# Only the plugin is built. It is linked against the libvlccore.dll of the official
# vlc-3.0.23-win64.zip (the plugin ABI is the exported libvlccore API, which the Nebulae
# patches do not change), so libvlccore itself is never rebuilt here.
#
# d3d11-hdr-sdr-tonemap.patch touches only modules/video_output/win32/*, which none of the five
# other Nebulae patches touch, so it applies to the pristine tree on its own and the result is
# the same with or without them.
#
# --baseline skips d3d11-hdr-sdr-tonemap.patch: it builds the unmodified VLC 3.0.23 plugin with the
# same toolchain, only as the "before" reference of the validation (test variant only).
#
# --test-dump additionally applies scripts/windows/vlc-d3d11/test-only/d3d11-test-dump.patch,
# which adds a frame dump used by the validation harness. That binary is a TEST variant and must
# never be staged or shipped; the output file is named libdirect3d11_plugin.testdump.dll.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
PATCH="$REPO/patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch"
PATCH_SHA="$REPO/patches/vlc-3.0.23/d3d11-patched-sources.sha256"
TESTPATCH="$HERE/vlc-d3d11/test-only/d3d11-test-dump.patch"
CXX_COMPAT="$HERE/vlc-d3d11/mingw-msvcrt-cxx-compat.h"
PTHREAD_SHIM="$HERE/vlc-d3d11/winpthread-static-shim.c"

VLC_TARBALL_SHA256=e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0
VLC_ZIP_SHA256=992d19dbd0b8a7cde9167d2f7780b1ef6f92acc8a71acfa736101a21f35181e1
# AMD AMF headers pinned exactly as VLC 3.0.23 contrib/src/amf (version 1.4.34); they make the
# plugin keep the same AMD HQScaler support as the official build.
AMF_VERSION=1.4.34
AMF_SHA512=2f6e43908260d9f7cb8b28ab8dd34f8f2a1f61c6f2b24d847f277deb264de74d37e60b71e35f6eb079af5c1a16967d0fb780271aee751c18f2013023dcb96f6c

fail() { echo "ERROR: $*" >&2; exit 1; }

WORK="" OUT="" TEST=0 BASELINE=0
VLC_TARBALL="${VLC_TARBALL:-$REPO/src-tauri/vlc-runtime-build/downloads/vlc-3.0.23.tar.xz}"
VLC_ZIP="${VLC_ZIP:-}"
AMF_TARBALL="${AMF_TARBALL:-}"
while [ $# -gt 0 ]; do
    case "$1" in
        --work) WORK="$2"; shift 2;;
        --out) OUT="$2"; shift 2;;
        --test-dump) TEST=1; shift;;
        --baseline) BASELINE=1; shift;;
        --vlc-tarball) VLC_TARBALL="$2"; shift 2;;
        --vlc-zip) VLC_ZIP="$2"; shift 2;;
        --amf-headers) AMF_TARBALL="$2"; shift 2;;
        *) fail "unknown argument $1";;
    esac
done
[ -n "$WORK" ] && [ -n "$OUT" ] || fail "usage: $0 --work DIR --out DIR --vlc-zip vlc-3.0.23-win64.zip [--test-dump]"
[ -f "$VLC_ZIP" ] || fail "--vlc-zip (official vlc-3.0.23-win64.zip) is required"
[ -f "$VLC_TARBALL" ] || fail "VLC source tarball not found: $VLC_TARBALL"
[ -n "$AMF_TARBALL" ] || AMF_TARBALL="$(dirname "$VLC_ZIP")/AMF-$AMF_VERSION.tar.gz"
[ -f "$AMF_TARBALL" ] || fail "AMF headers tarball not found: $AMF_TARBALL (v$AMF_VERSION AMF-headers.tar.gz from GPUOpen-LibrariesAndSDKs/AMF)"
for tool in x86_64-w64-mingw32-gcc x86_64-w64-mingw32-g++ x86_64-w64-mingw32-windres x86_64-w64-mingw32-objdump patch unzip shasum make; do
    command -v "$tool" >/dev/null || fail "missing tool: $tool"
done

sha_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
[ "$(sha_of "$VLC_TARBALL")" = "$VLC_TARBALL_SHA256" ] || fail "VLC tarball sha256 mismatch"
[ "$(sha_of "$VLC_ZIP")" = "$VLC_ZIP_SHA256" ] || fail "VLC win64 zip sha256 mismatch"
[ "$(shasum -a 512 "$AMF_TARBALL" | cut -d' ' -f1)" = "$AMF_SHA512" ] || fail "AMF headers sha512 mismatch"

rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
WORK="$(cd "$WORK" && pwd)"; OUT="$(cd "$OUT" && pwd)"

echo "==> unpack pristine VLC 3.0.23 (only what the plugin needs is patched)"
tar -xf "$VLC_TARBALL" -C "$WORK"
SRC="$WORK/vlc-3.0.23"
if [ "$BASELINE" != 1 ]; then
    (cd "$SRC" && patch -p1 -F0 --batch --no-backup-if-mismatch --dry-run < "$PATCH" >/dev/null) || fail "d3d11 patch does not apply with zero fuzz"
    (cd "$SRC" && patch -p1 -F0 --batch --no-backup-if-mismatch < "$PATCH")
    (cd "$SRC" && shasum -a 256 -c "$PATCH_SHA") || fail "patched d3d11 sources differ from d3d11-patched-sources.sha256"
fi
if [ "$TEST" = 1 ]; then
    (cd "$SRC" && patch -p1 -F0 --batch --no-backup-if-mismatch < "$TESTPATCH")
fi
[ -z "$(find "$SRC" \( -name '*.rej' -o -name '*.orig' \) -print | head -1)" ] || fail "patch left reject/backup files"

echo "==> unpack the official libvlccore.dll (link target) and the stock plugin (reference)"
unzip -o -q "$VLC_ZIP" 'vlc-3.0.23/libvlccore.dll' 'vlc-3.0.23/plugins/video_output/libdirect3d11_plugin.dll' -d "$WORK/stock"
CORE="$WORK/stock/vlc-3.0.23/libvlccore.dll"

mkdir -p "$WORK/amf" && tar -xzf "$AMF_TARBALL" -C "$WORK/amf"

echo "==> configure (cross, x86_64-w64-mingw32, msvcrt-os)"
# configure only probes ID3D11VideoDecoder when avcodec is enabled, which this plugin-only build
# disables. Without HAVE_ID3D11VIDEODECODER the plugin compiles out the software-frame staging
# texture setup and crashes in Prepare(), so it is defined here exactly as the full official build does.
export CPPFLAGS="-I$WORK/amf -DHAVE_ID3D11VIDEODECODER=1"
export CC="x86_64-w64-mingw32-gcc -mcrtdll=msvcrt-os -fno-stack-protector -std=gnu11"
export CXX="x86_64-w64-mingw32-g++ -mcrtdll=msvcrt-os -fno-stack-protector"
BUILD="$SRC/build-win64"; mkdir -p "$BUILD"
(cd "$BUILD" && ../configure --host=x86_64-w64-mingw32 --build="$(uname -m | sed s/arm64/aarch64/)-apple-darwin" \
    --disable-nls --disable-lua --disable-vlc --disable-a52 --disable-avcodec --disable-avformat \
    --disable-swscale --disable-postproc --disable-qt --disable-skins2 --disable-update-check \
    --disable-vlm --disable-dbus --disable-mad --disable-libgcrypt --disable-sout --disable-dca \
    --disable-sse --disable-mmx > "$WORK/configure.log" 2>&1) || fail "configure failed, see $WORK/configure.log"

echo "==> compile objects"
CFLAGS_BUILD="$(sed -n 's/^CFLAGS = //p' "$BUILD/Makefile" | sed 's/-fstack-protector-strong//g')"
CXXFLAGS_BUILD="$(sed -n 's/^CXXFLAGS = //p' "$BUILD/Makefile" | sed 's/-fstack-protector-strong//g') -include $CXX_COMPAT"
# __FILE__ and assert strings must not depend on where the build ran
PREFIX_MAP="-ffile-prefix-map=$SRC=/vlc-3.0.23 -ffile-prefix-map=$REPO=/nebulae"
CFLAGS_BUILD="$CFLAGS_BUILD $PREFIX_MAP"
CXXFLAGS_BUILD="$CXXFLAGS_BUILD $PREFIX_MAP"
if [ "$TEST" = 1 ]; then
    CFLAGS_BUILD="$CFLAGS_BUILD -DNEBULAE_TEST_DUMP"
fi
W32=video_output/win32/libdirect3d11_plugin_la
(cd "$BUILD/modules" && make -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc)" \
    CFLAGS="$CFLAGS_BUILD" CXXFLAGS="$CXXFLAGS_BUILD" \
    libchroma_copy.la libd3d11_common.la module.rc.lo \
    $W32-direct3d11.lo $W32-d3d11_quad.lo $W32-d3d11_shaders.lo $W32-common.lo \
    $W32-d3d11_scaler.lo $W32-d3d11_tonemap.lo $W32-events.lo $W32-sensors.lo \
    hw/amf/libdirect3d11_plugin_la-amf_helper.lo \
    $W32-win32touch.lo > "$WORK/compile.log" 2>&1) || fail "compile failed, see $WORK/compile.log"

echo "==> link"
# libstdc++ of this posix-threads toolchain reaches winpthread through __imp_ pointers; the shim
# binds them to the static libwinpthread.a so the plugin does not import libwinpthread-1.dll.
x86_64-w64-mingw32-gcc -mcrtdll=msvcrt-os -O2 -c "$PTHREAD_SHIM" -o "$WORK/winpthread-shim.o"
SYSROOT="$(x86_64-w64-mingw32-gcc -print-sysroot 2>/dev/null || true)"
WINPTHREAD_A="$(x86_64-w64-mingw32-gcc -print-file-name=libwinpthread.a)"
[ -f "$WINPTHREAD_A" ] || fail "libwinpthread.a not found"
DLL="$OUT/libdirect3d11_plugin.dll"
[ "$TEST" = 1 ] && DLL="$OUT/libdirect3d11_plugin.testdump.dll"
if [ "$BASELINE" = 1 ]; then DLL="$OUT/libdirect3d11_plugin.baseline.dll"; [ "$TEST" = 1 ] && DLL="$OUT/libdirect3d11_plugin.baseline.testdump.dll"; fi
(cd "$BUILD/modules" && x86_64-w64-mingw32-g++ -mcrtdll=msvcrt-os -shared -static-libgcc -static-libstdc++ \
    -Wl,--no-undefined -Wl,--disable-auto-image-base -Wl,--image-base=0x180000000 -Wl,--no-insert-timestamp -s -o "$DLL" \
    video_output/win32/.libs/libdirect3d11_plugin_la-*.o hw/amf/.libs/libdirect3d11_plugin_la-amf_helper.o .libs/libd3d11_common.a .libs/libchroma_copy.a \
    .libs/module.rc.o "$WORK/winpthread-shim.o" "$CORE" \
    -lgdi32 -lole32 -luuid -luser32 -ladvapi32 -lshell32 "$WINPTHREAD_A") || fail "link failed"

echo "==> verify"
imports="$(x86_64-w64-mingw32-objdump -p "$DLL" | awk '/DLL Name:/{print tolower($3)}' | sort -u | tr '\n' ' ')"
stock_imports="$(x86_64-w64-mingw32-objdump -p "$WORK/stock/vlc-3.0.23/plugins/video_output/libdirect3d11_plugin.dll" | awk '/DLL Name:/{print tolower($3)}' | sort -u | tr '\n' ' ')"
[ "$imports" = "$stock_imports" ] || fail "imports differ from the stock plugin: [$imports] vs [$stock_imports]"
exports="$(x86_64-w64-mingw32-objdump -p "$DLL" | awk '/vlc_entry/{print $NF}' | sort | tr '\n' ' ')"
[ "$exports" = "vlc_entry__3_0_0f vlc_entry_copyright__3_0_0f vlc_entry_license__3_0_0f " ] || fail "unexpected exports: $exports"

cat > "$OUT/$(basename "$DLL").build-info.json" <<EOF
{
  "artifact": "$(basename "$DLL")",
  "sha256": "$(sha_of "$DLL")",
  "bytes": $(wc -c < "$DLL" | tr -d ' '),
  "test_variant": $([ "$TEST" = 1 ] && echo true || echo false),
  "vlc_source_sha256": "$VLC_TARBALL_SHA256",
  "link_target_zip_sha256": "$VLC_ZIP_SHA256",
  "patch": "d3d11-hdr-sdr-tonemap.patch",
  "patch_sha256": "$(sha_of "$PATCH")",
  "compiler": "$(x86_64-w64-mingw32-gcc --version | head -1) -mcrtdll=msvcrt-os -fno-stack-protector",
  "imports": "$imports"
}
EOF
echo "OK: $DLL"
echo "    sha256 $(sha_of "$DLL")"

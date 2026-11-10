#!/usr/bin/env bash
# scripts/build-macos-deps-11.sh
#
# Builds the Homebrew-sourced dynamic libraries that Nebulae bundles on macOS
# from source with MACOSX_DEPLOYMENT_TARGET=11.0, so that the shipped app does
# not raise its effective minimum macOS above the declared 11.0.
#
# Homebrew bottles target the macOS of the machine that built them (26.0 on
# this host), which silently raises the app's real minimum above 11.0 despite
# the declared minimumSystemVersion. Building from source at 11.0 keeps the
# declared minimum honest.
#
# Output: all libraries are installed into --out-dir as flat .dylib files.
# The script is idempotent: libraries already present in --out-dir are skipped.
#
# Usage: scripts/build-macos-deps-11.sh [--out-dir DIR]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$REPO_ROOT/src-tauri/macos-deps-11"
ARCH="$(uname -m)"
while [ $# -gt 0 ]; do
    case "$1" in
        --out-dir)
            OUT_DIR="$2"; shift 2 ;;
        *)
            echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
COMMON_CFLAGS="-mmacosx-version-min=11.0 -arch $ARCH"
COMMON_LDFLAGS="-mmacosx-version-min=11.0 -arch $ARCH"

mkdir -p "$OUT_DIR"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/nebulae-deps-11.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

have() { [ -f "$OUT_DIR/$1" ]; }

# --- libdeflate 1.26 (used by transmission) ---------------------------------
if ! have libdeflate.0.dylib; then
    echo "==> Building libdeflate 1.26 at minos 11.0"
    curl -fsSL -o "$WORK/libdeflate.tar.gz" "https://github.com/ebiggers/libdeflate/archive/refs/tags/v1.26.tar.gz"
    tar -xzf "$WORK/libdeflate.tar.gz" -C "$WORK"
    ( cd "$WORK/libdeflate-1.26" && mkdir -p build && cd build \
      && cmake .. -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 -DCMAKE_OSX_ARCHITECTURES=$ARCH \
                  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
      >"$WORK/libdeflate.cmake.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/libdeflate.make.log" 2>&1 \
      && make install >>"$WORK/libdeflate.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libdeflate.0.dylib" "$OUT_DIR/libdeflate.0.dylib"
fi

# --- libevent 2.1.13 (used by transmission) ---------------------------------
if ! have libevent-2.1.7.dylib; then
    echo "==> Building libevent 2.1.13 at minos 11.0"
    curl -fsSL -o "$WORK/libevent.tar.gz" "https://github.com/libevent/libevent/archive/refs/tags/release-2.1.13-stable.tar.gz"
    tar -xzf "$WORK/libevent.tar.gz" -C "$WORK"
    ( cd "$WORK/libevent-release-2.1.13-stable" \
      && ./autogen.sh >"$WORK/libevent.autogen.log" 2>&1 \
      && ./configure --prefix="$OUT_DIR" --disable-openssl --enable-shared=yes \
                     CFLAGS="$COMMON_CFLAGS" LDFLAGS="$COMMON_LDFLAGS" \
                     >>"$WORK/libevent.configure.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/libevent.make.log" 2>&1 \
      && make install >>"$WORK/libevent.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libevent-2.1.7.dylib" "$OUT_DIR/libevent-2.1.7.dylib"
fi

# --- libiconv 1.17 (dependency of libpsl and libintl) -------------------------
if ! have libiconv.2.dylib; then
    echo "==> Building libiconv 1.17 at minos 11.0"
    curl -fsSL -o "$WORK/libiconv.tar.gz" "https://ftp.gnu.org/gnu/libiconv/libiconv-1.17.tar.gz"
    tar -xzf "$WORK/libiconv.tar.gz" -C "$WORK"
    ( cd "$WORK/libiconv-1.17" \
      && ./configure --prefix="$OUT_DIR" --disable-static --enable-shared=yes \
                     CFLAGS="$COMMON_CFLAGS" LDFLAGS="$COMMON_LDFLAGS" \
                     >>"$WORK/libiconv.configure.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/libiconv.make.log" 2>&1 \
      && make install >>"$WORK/libiconv.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libiconv.2.dylib" "$OUT_DIR/libiconv.2.dylib"
    # Fix the install name so other libraries can find libiconv at the
    # top-level location (the file was moved out of lib/).
    install_name_tool -id "$OUT_DIR/libiconv.2.dylib" "$OUT_DIR/libiconv.2.dylib"
    # libiconv does not install a pkg-config file by default; libpsl needs one.
    mkdir -p "$OUT_DIR/lib/pkgconfig"
    cat > "$OUT_DIR/lib/pkgconfig/libiconv.pc" <<PCFILE
prefix=$OUT_DIR
exec_prefix=\${prefix}
libdir=\${exec_prefix}/lib
includedir=\${prefix}/include

Name: libiconv
Description: GNU character encoding conversion library
Version: 1.17
Libs: -L\${libdir} -liconv
Cflags: -I\${includedir}
PCFILE
fi

# --- libintl/gettext 0.21 (used by transmission and VLC) ---------------------
if ! have libintl.8.dylib; then
    echo "==> Building gettext 0.21 at minos 11.0 (libintl only)"
    curl -fsSL -o "$WORK/gettext.tar.gz" "https://ftp.gnu.org/gnu/gettext/gettext-0.21.tar.gz"
    tar -xzf "$WORK/gettext.tar.gz" -C "$WORK"
    ( cd "$WORK/gettext-0.21" \
      && ./configure --prefix="$OUT_DIR" --disable-static --enable-shared=yes \
                     --disable-dependency-tracking \
                     --without-libiconv-prefix --without-libintl-prefix \
                     CFLAGS="$COMMON_CFLAGS" LDFLAGS="$COMMON_LDFLAGS" \
                     >>"$WORK/gettext.configure.log" 2>&1 \
      && make -C gettext-runtime -j"$(sysctl -n hw.ncpu)" >>"$WORK/gettext.make.log" 2>&1 \
      && make -C gettext-runtime install >>"$WORK/gettext.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libintl.8.dylib" "$OUT_DIR/libintl.8.dylib"
fi

# --- libminiupnpc 2.3.3 (used by transmission) -------------------------------
if ! have libminiupnpc.21.dylib; then
    echo "==> Building miniupnpc 2.3.3 at minos 11.0"
    curl -fsSL -o "$WORK/miniupnpc.tar.gz" "https://github.com/miniupnp/miniupnp/archive/refs/tags/miniupnpc_2_3_3.tar.gz"
    tar -xzf "$WORK/miniupnpc.tar.gz" -C "$WORK"
    ( cd "$WORK/miniupnp-miniupnpc_2_3_3/miniupnpc" \
      && cmake -S . -B build \
               -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 \
               -DCMAKE_OSX_ARCHITECTURES=$ARCH \
               -DCMAKE_BUILD_TYPE=Release \
               -DCMAKE_INSTALL_PREFIX="$OUT_DIR" \
               >>"$WORK/miniupnpc.cmake.log" 2>&1 \
      && cmake --build build -j"$(sysctl -n hw.ncpu)" >>"$WORK/miniupnpc.make.log" 2>&1 \
      && cmake --install build >>"$WORK/miniupnpc.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libminiupnpc.21.dylib" "$OUT_DIR/libminiupnpc.21.dylib"
fi

# --- libpsl 0.23.3 (used by transmission; needs libiconv) --------------------
if ! have libpsl.5.dylib; then
    echo "==> Building libpsl 0.23.3 at minos 11.0"
    curl -fsSL -o "$WORK/libpsl.tar.gz" "https://github.com/rockdaboot/libpsl/releases/download/0.23.3/libpsl-0.23.3.tar.gz"
    tar -xzf "$WORK/libpsl.tar.gz" -C "$WORK"
    ( cd "$WORK/libpsl-0.23.3" \
      && ./configure --prefix="$OUT_DIR" --disable-static --enable-shared=yes \
                     CFLAGS="$COMMON_CFLAGS -I$OUT_DIR/include" \
                     LDFLAGS="$COMMON_LDFLAGS -L$OUT_DIR/lib -Wl,-rpath,$OUT_DIR/lib" \
                     PKG_CONFIG_PATH="$OUT_DIR/lib/pkgconfig" \
                     >>"$WORK/libpsl.configure.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/libpsl.make.log" 2>&1 \
      && make install >>"$WORK/libpsl.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libpsl.5.dylib" "$OUT_DIR/libpsl.5.dylib"
fi

# --- libpng 1.6.59 (used by VLC) ---------------------------------------------
if ! have libpng16.16.dylib; then
    echo "==> Building libpng 1.6.59 at minos 11.0"
    curl -fsSL -o "$WORK/libpng.tar.gz" "https://github.com/glennrp/libpng/archive/refs/tags/v1.6.59.tar.gz"
    tar -xzf "$WORK/libpng.tar.gz" -C "$WORK"
    ( cd "$WORK/libpng-1.6.59" \
      && ./configure --prefix="$OUT_DIR" --disable-static --enable-shared=yes \
                     CFLAGS="$COMMON_CFLAGS" LDFLAGS="$COMMON_LDFLAGS" \
                     >>"$WORK/libpng.configure.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/libpng.make.log" 2>&1 \
      && make install >>"$WORK/libpng.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libpng16.16.dylib" "$OUT_DIR/libpng16.16.dylib"
fi

# --- libfreetype 2.14.3 (used by VLC; no harfbuzz/brotli to avoid 26.0 deps) --
if ! have libfreetype.6.dylib; then
    echo "==> Building freetype 2.14.3 at minos 11.0 (no harfbuzz/brotli)"
    curl -fsSL -o "$WORK/freetype.tar.gz" "https://download.savannah.gnu.org/releases/freetype/freetype-2.14.3.tar.gz"
    tar -xzf "$WORK/freetype.tar.gz" -C "$WORK"
    ( cd "$WORK/freetype-2.14.3" \
      && ./configure --prefix="$OUT_DIR" --disable-static --enable-shared=yes \
                     --with-harfbuzz=no --with-brotli=no \
                     CFLAGS="$COMMON_CFLAGS" \
                     LDFLAGS="$COMMON_LDFLAGS -L$OUT_DIR/lib" \
                     PKG_CONFIG_PATH="$OUT_DIR/lib/pkgconfig" \
                     >>"$WORK/freetype.configure.log" 2>&1 \
      && make -j"$(sysctl -n hw.ncpu)" >>"$WORK/freetype.make.log" 2>&1 \
      && make install >>"$WORK/freetype.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libfreetype.6.dylib" "$OUT_DIR/libfreetype.6.dylib"
fi

# --- libfontconfig 2.15.0 (used by VLC) --------------------------------------
if ! have libfontconfig.1.dylib; then
    echo "==> Building fontconfig 2.15.0 at minos 11.0"
    curl -fsSL -o "$WORK/fontconfig.tar.gz" "https://gitlab.freedesktop.org/fontconfig/fontconfig/-/archive/2.15.0/fontconfig-2.15.0.tar.gz"
    tar -xzf "$WORK/fontconfig.tar.gz" -C "$WORK"
    ( cd "$WORK/fontconfig-2.15.0" \
      && meson setup build \
           -Dc_args="$COMMON_CFLAGS" \
           -Dc_link_args="$COMMON_LDFLAGS -L$OUT_DIR/lib" \
           --prefix="$OUT_DIR" --libdir=lib \
           -Ddoc=disabled -Dtests=disabled \
           >>"$WORK/fontconfig.meson.log" 2>&1 \
      && ninja -C build -j"$(sysctl -n hw.ncpu)" >>"$WORK/fontconfig.ninja.log" 2>&1 \
      && meson install -C build >>"$WORK/fontconfig.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libfontconfig.1.dylib" "$OUT_DIR/libfontconfig.1.dylib"
fi

# --- libudfread 1.2.0 (used by VLC) ------------------------------------------
if ! have libudfread.3.dylib; then
    echo "==> Building libudfread 1.2.0 at minos 11.0"
    curl -fsSL -o "$WORK/udfread.tar.gz" "https://code.videolan.org/videolan/libudfread/-/archive/1.2.0/libudfread-1.2.0.tar.gz"
    tar -xzf "$WORK/udfread.tar.gz" -C "$WORK"
    ( cd "$WORK/libudfread-1.2.0" \
      && meson setup build \
           -Dc_args="$COMMON_CFLAGS" \
           -Dc_link_args="$COMMON_LDFLAGS" \
           --prefix="$OUT_DIR" --libdir=lib \
           >>"$WORK/udfread.meson.log" 2>&1 \
      && ninja -C build -j"$(sysctl -n hw.ncpu)" >>"$WORK/udfread.ninja.log" 2>&1 \
      && meson install -C build >>"$WORK/udfread.install.log" 2>&1 )
    cp -pL "$OUT_DIR/lib/libudfread.3.dylib" "$OUT_DIR/libudfread.3.dylib"
fi

echo "==> All macOS 11.0 dependencies staged in $OUT_DIR:"
for f in "$OUT_DIR"/*.dylib; do
    [ -f "$f" ] || continue
    minos="$(otool -l "$f" 2>/dev/null | awk '/minos/{print $2; exit}' || true)"
    echo "    $(basename "$f")  minos=${minos:-unknown}"
done

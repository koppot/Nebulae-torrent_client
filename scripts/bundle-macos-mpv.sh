#!/usr/bin/env bash
# scripts/bundle-macos-mpv.sh
# Bundles mpv binary and all non-system dylib dependencies for macOS
# Required by Phase 3 of docs/task-bundle-disc-player.md

set -euo pipefail

TARGET_DIR="${1:-src-tauri/resources/player}"
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-}"

echo "=========================================================="
echo " Nebulae macOS Disc Player Bundling (mpv + dylib tree)    "
echo "=========================================================="
echo "Target directory: $TARGET_DIR"

# Locate mpv binary on build host
MPV_BIN=""
if command -v mpv >/dev/null 2>&1; then
    MPV_BIN=$(command -v mpv)
elif [ -f "/opt/homebrew/bin/mpv" ]; then
    MPV_BIN="/opt/homebrew/bin/mpv"
elif [ -f "/usr/local/bin/mpv" ]; then
    MPV_BIN="/usr/local/bin/mpv"
fi

if [ -z "$MPV_BIN" ]; then
    echo "ERROR: mpv not found. On macOS build runners, run 'brew install mpv' first."
    exit 1
fi

echo "Source mpv binary: $MPV_BIN"

# --- DVD menu capability gate ----------------------------------------------
# Homebrew's mpv formula dropped libdvdnav, so a brew mpv cannot play DVD
# menus (no --dvd-device option). The owner requires ALL menus to play, so
# when the host mpv lacks DVD support, build mpv from the pinned source tag
# with libdvdnav enabled. Every other build dependency is already present
# (they are Homebrew mpv's own dependencies).
if ! "$MPV_BIN" --list-options 2>/dev/null | grep -q -- "--dvd-device"; then
    echo "==> Host mpv lacks libdvdnav (Homebrew dropped it from the formula)."
    echo "==> Building mpv v0.41.0 from source with DVD + Blu-ray menu support..."
    brew install -q libdvdnav libdvdread meson ninja pkg-config
    BUILD_ROOT="$(mktemp -d)"
    trap 'rm -rf "$BUILD_ROOT"' EXIT
    # mpv 0.41 REMOVED DVD menu support ("DVD menu support has been
    # removed." in the 0.41 binary) but 0.41 also updated mpv for the
    # current brew ffmpeg. Pin 0.40 (still has dvdnav menus, newer API
    # surface) and FAIL THE BUILD if the menu removal marker appears -
    # the owner requires DVD menus, always.
    git clone --depth 1 --branch v0.40.0 https://github.com/mpv-player/mpv "$BUILD_ROOT/mpv"
    ( cd "$BUILD_ROOT/mpv" &&
      meson setup build -Dgpl=true \
          -Ddvdnav=enabled -Dlibbluray=enabled \
          -Dtests=false --prefix "$BUILD_ROOT/install" --buildtype release &&
      ninja -C build &&
      ninja -C build install ) || {
        echo "ERROR: source build of mpv with libdvdnav failed."
        exit 1
    }
    MPV_BIN="$BUILD_ROOT/install/bin/mpv"
    echo "Built mpv with DVD menu support: $MPV_BIN"
fi

mkdir -p "$TARGET_DIR/lib"
cp -L "$MPV_BIN" "$TARGET_DIR/mpv"
chmod 755 "$TARGET_DIR/mpv"

# Menu-first lua: disc playback must land on the root menu (the owner
# requirement - discs never auto-play the feature; navigation starts it).
mkdir -p "$TARGET_DIR/scripts"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cp "$REPO_DIR/scripts/mpv/menu-first.lua" "$TARGET_DIR/scripts/menu-first.lua"
chmod 644 "$TARGET_DIR/scripts/menu-first.lua"

# Python 3 script to discover all non-system dylibs, copy them, rewrite IDs and rpaths
python3 - <<EOF
import os
import sys
import shutil
import subprocess

target_dir = "$TARGET_DIR"
lib_dir = os.path.join(target_dir, "lib")
mpv_path = os.path.join(target_dir, "mpv")
signing_identity = "$SIGNING_IDENTITY"

BREW_LIB_DIRS = ['/opt/homebrew/opt', '/usr/local/opt', '/opt/homebrew/Cellar', '/usr/local/Cellar']

def find_brew_lib(name):
    """Last-resort lookup for a Homebrew dylib by filename."""
    for root in BREW_LIB_DIRS:
        if not os.path.isdir(root):
            continue
        if '/opt/' in root:
            hits = glob.glob(os.path.join(root, '*/lib', name))
            if hits:
                return hits[0]
        else:
            hits = glob.glob(os.path.join(root, '*/*/lib', name))
            if hits:
                return hits[0]
    return None

def get_direct_deps(binary_path):
    """All non-system load-command references, INCLUDING @rpath/@loader_path
    forms. Each entry is (raw_ref, resolved_source_path_or_None). Unresolvable
    references are reported so the hard gate can fail the build - shipping a
    bundle with a dangling reference is what killed playback on machines
    without the build host's Homebrew layout."""
    out = subprocess.check_output(['otool', '-L', binary_path]).decode('utf-8')
    deps = []
    for line in out.splitlines()[1:]:
        line = line.strip()
        if not line:
            continue
        dep = line.split(' (compatibility')[0].strip()
        if dep.startswith('/System/') or dep.startswith('/usr/lib/'):
            continue
        if dep.startswith('@loader_path/') or dep.startswith('@executable_path/'):
            suffix = dep.split('/', 1)[1]
            cand = os.path.normpath(os.path.join(os.path.dirname(binary_path), suffix))
            deps.append((dep, cand if os.path.exists(cand) else None))
        elif dep.startswith('@rpath/'):
            name = dep.split('/', 1)[1]
            # Homebrew siblings live next to the referencing lib.
            sib = os.path.join(os.path.dirname(binary_path), name)
            if os.path.exists(sib):
                deps.append((dep, sib))
            else:
                brew = find_brew_lib(name)
                deps.append((dep, brew))
        else:
            if os.path.exists(dep):
                # Keep the RAW load-command path. realpath() would resolve
                # Homebrew symlink layers (opt/... -> Cellar/...), and the
                # realpath never matches the string inside the Mach-O load
                # command, so every install_name_tool -change silently
                # no-ops and the bundle ships with absolute references.
                deps.append((dep, dep))
            else:
                deps.append((dep, None))
    return deps

def resolve_ref(ref, referencing_binary):
    for raw, resolved in get_direct_deps(referencing_binary):
        if raw == ref:
            return resolved
    return None

# Recursively crawl non-system dependencies
unresolved = []
queue = []
for d, r in get_direct_deps(mpv_path):
    if r is None:
        unresolved.append((mpv_path, d))
    else:
        queue.append(r)
visited = set(queue)
raw_refs = {}   # resolved source path -> set of raw load-command strings

while queue:
    curr = queue.pop(0)
    for ref, resolved in get_direct_deps(curr):
        if resolved is None:
            unresolved.append((curr, ref))
            continue
        raw_refs.setdefault(resolved, set()).add(ref)
        if resolved not in visited:
            visited.add(resolved)
            queue.append(resolved)

if unresolved:
    for src, ref in unresolved:
        print(f"UNRESOLVED DEPENDENCY: {src} -> {ref}")
    sys.exit("FATAL: dangling library references cannot produce a working bundle.")

print(f"Discovered {len(visited)} non-system dynamic libraries to bundle.")

# Map original paths and raw references to bundle destination
path_map = {}
for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    shutil.copy2(orig, dest)
    os.chmod(dest, 0o755)
    path_map[orig] = base
    for ref in raw_refs.get(orig, ()):
        path_map[ref] = base
    path_map[os.path.realpath(orig)] = base

# Set install id for each dylib
for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    subprocess.run(['install_name_tool', '-id', f'@rpath/{base}', dest], check=True)

# Update references in mpv binary and all copied dylibs
all_binaries = [mpv_path] + [os.path.join(lib_dir, os.path.basename(d)) for d in visited]
for b in all_binaries:
    for dep, _resolved in get_direct_deps(b):
        if dep in path_map:
            target_base = path_map[dep]
            subprocess.run(['install_name_tool', '-change', dep, f'@rpath/{target_base}', b], check=True)

# HARD GATE: no absolute Homebrew paths or un-bundled @loader_path
# references may survive in the bundle.
for b in all_binaries:
    listing = subprocess.check_output(['otool', '-L', b]).decode('utf-8')
    if ('/opt/homebrew/' in listing or '/usr/local/' in listing
            or '@loader_path/' in listing or '@executable_path/' in listing):
        sys.exit(f"FATAL: {b} still references libraries outside the bundle:\n{listing}")
print("All load commands rewritten to @rpath (no external references remain).")

# Add rpaths to mpv
subprocess.run(['install_name_tool', '-add_rpath', '@executable_path/lib', mpv_path], stderr=subprocess.DEVNULL)
subprocess.run(['install_name_tool', '-add_rpath', '@loader_path/lib', mpv_path], stderr=subprocess.DEVNULL)

# Sign all dylibs and mpv
codesign_cmd = ['codesign', '--force']
if signing_identity:
    codesign_cmd.extend(['--sign', signing_identity, '--options', 'runtime', '--timestamp'])
else:
    codesign_cmd.extend(['--sign', '-'])

for d in visited:
    dest = os.path.join(lib_dir, os.path.basename(d))
    subprocess.run(codesign_cmd + [dest], check=True)

subprocess.run(codesign_cmd + [mpv_path], check=True)
print("Code signing completed for bundled dylib tree and mpv helper binary.")

# Re-sign lua script marker (text file, no signing needed; listed here so
# the bundle contents are obvious when reading CI logs).
print("Menu-first lua script bundled: scripts/menu-first.lua")
EOF

# --- Bundled JRE (BD-J / 4K UHD interactive menus) --------------------------
# The owner requires ALL menus to play, including BD-J interactive menus on
# Blu-ray and 4K UHD discs. libbluray needs a JVM for those, so a minimal
# JRE produced by jlink ships inside the package and is signed like the
# rest of the bundled player tree.
if [ "${NEB_SKIP_JRE:-0}" != "1" ]; then
    if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/jlink" ]; then
        echo "ERROR: jlink is not available (JAVA_HOME='${JAVA_HOME:-unset}')."
        echo "The owner requires full BD-J/UHD interactive menu support, which"
        echo "needs a bundled JRE. Set up a JDK (e.g. actions/setup-java,"
        echo "distribution: temurin, java-version: 21) or set NEB_SKIP_JRE=1"
        echo "to build WITHOUT BD-J menus (not acceptable for releases)."
        exit 1
    fi
    echo "==> Bundling minimal JRE via jlink (BD-J / UHD menus)..."
    "$JAVA_HOME/bin/jlink" \
        --add-modules java.se,jdk.unsupported,jdk.crypto.ec \
        --strip-debug --no-man-pages --no-header-files \
        --output "$TARGET_DIR/jre"
    "$TARGET_DIR/jre/bin/java" -version
    echo "==> Code-signing bundled JRE Mach-O images..."
    while IFS= read -r -d '' f; do
        if file "$f" | grep -q "Mach-O"; then
            if [ -n "$SIGNING_IDENTITY" ]; then
                codesign --force --sign "$SIGNING_IDENTITY" --options runtime --timestamp "$f"
            else
                codesign --force --sign - "$f"
            fi
        fi
    done < <(find "$TARGET_DIR/jre" -type f -print0)
    echo "JRE signing complete. Bundled JRE ready: $TARGET_DIR/jre"
fi

echo "==> Running smoke test on bundled macOS mpv binary..."
# BD-J = interactive Blu-ray/UHD menus (owner requirement). libbluray ships
# from the host brew; assert the copied one actually has BD-J/JVM support,
# otherwise the bundled JRE has nothing to drive.
if [ -f "$TARGET_DIR/lib/libbluray.4.dylib" ]; then
    strings "$TARGET_DIR/lib/libbluray.4.dylib" 2>/dev/null | grep -qi "bd-j" || {
        echo "ERROR: bundled libbluray lacks BD-J support - interactive Blu-ray/UHD menus would be dead!"
        exit 1
    }
    echo "BD-J support verified in bundled libbluray."
fi
# Menu-support gate: this marker string proves dvdnav menus were removed.
if strings "$TARGET_DIR/mpv" 2>/dev/null | grep -q "DVD menu support has been removed"; then
    echo "ERROR: this mpv release has DVD menu support REMOVED - owner requires menus!"
    exit 1
fi
if otool -L "$TARGET_DIR/mpv" 2>/dev/null | grep -qE '/opt/homebrew/|/usr/local/|@loader_path|@executable_path'; then
    echo "ERROR: bundled mpv still references libraries outside the bundle;"
    echo "the dylib rewrite failed and the binary would die at dyld on"
    echo "any machine without this exact Homebrew layout."
    otool -L "$TARGET_DIR/mpv" | grep -E '/opt/homebrew/|/usr/local/|@loader_path|@executable_path' | head -5
    exit 1
fi
SMOKE_OUT=$("$TARGET_DIR/mpv" --version)
echo "$SMOKE_OUT" | head -n 6
# The app passes --deinterlace=auto on every launch; fail the build now if
# this mpv release doesn't support it (option errors exit the player).
"$TARGET_DIR/mpv" --deinterlace=help 2>/dev/null | grep -q "auto" || {
    echo "ERROR: bundled mpv does not support --deinterlace=auto!"
    exit 1
}
OPTIONS_OUT=$("$TARGET_DIR/mpv" --list-options)
case "$OPTIONS_OUT" in
    *--dvd-device*)   : ;;
    *) echo "ERROR: DVD menu support (--dvd-device) missing from bundled mpv!"; exit 1 ;;
esac
case "$OPTIONS_OUT" in
    *--bluray-device*) : ;;
    *) echo "ERROR: Blu-ray menu support (--bluray-device) missing from bundled mpv!"; exit 1 ;;
esac
echo "SUCCESS: macOS disc player, dylib tree, and JRE bundled and verified (DVD + Blu-ray menus)."

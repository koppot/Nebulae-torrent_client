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

mkdir -p "$TARGET_DIR/lib"
cp -L "$MPV_BIN" "$TARGET_DIR/mpv"
chmod 755 "$TARGET_DIR/mpv"

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

def get_direct_deps(binary_path):
    out = subprocess.check_output(['otool', '-L', binary_path]).decode('utf-8')
    deps = []
    for line in out.splitlines()[1:]:
        line = line.strip()
        if not line:
            continue
        dep = line.split(' (compatibility')[0].strip()
        if dep.startswith('/System/') or dep.startswith('/usr/lib/') or dep.startswith('@'):
            continue
        if os.path.exists(dep):
            deps.append(os.path.realpath(dep))
    return deps

# Recursively crawl non-system dependencies
queue = get_direct_deps(mpv_path)
visited = set(queue)

while queue:
    curr = queue.pop(0)
    for d in get_direct_deps(curr):
        if d not in visited:
            visited.add(d)
            queue.append(d)

print(f"Discovered {len(visited)} non-system dynamic libraries to bundle.")

# Map original paths and realpaths to bundle destination
path_map = {}
for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    shutil.copy2(orig, dest)
    os.chmod(dest, 0o755)
    path_map[orig] = base
    path_map[os.path.realpath(orig)] = base

# Set install id for each dylib
for orig in visited:
    base = os.path.basename(orig)
    dest = os.path.join(lib_dir, base)
    subprocess.run(['install_name_tool', '-id', f'@rpath/{base}', dest], check=True)

# Update references in mpv binary and all copied dylibs
all_binaries = [mpv_path] + [os.path.join(lib_dir, os.path.basename(d)) for d in visited]
for b in all_binaries:
    deps = get_direct_deps(b)
    for dep in deps:
        if dep in path_map:
            target_base = path_map[dep]
            subprocess.run(['install_name_tool', '-change', dep, f'@rpath/{target_base}', b], check=False)

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
EOF

echo "==> Running smoke test on bundled macOS mpv binary..."
SMOKE_OUT=$("$TARGET_DIR/mpv" --version)
echo "$SMOKE_OUT" | head -n 5
echo "SUCCESS: macOS disc player and dylib tree bundled and verified."

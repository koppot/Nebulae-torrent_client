#!/usr/bin/env bash
# scripts/verify-macos-app.sh APP
#
# Final check of a built Nebulae.app, before it is packaged or installed. It fails
# when the bundle depends on the machine that built it or differs from what the
# staging scripts verified:
#
#   1. Finder or AppleDouble litter (.DS_Store, ._*) anywhere in the app. A
#      .DS_Store in src-tauri/resources is copied into the bundle by the
#      "resources/**/*" glob, so it has to be gone before "tauri build"; the
#      staging scripts and the workflows delete it, this check proves it.
#   2. The strict recursive Mach-O audit (scripts/audit-macos-bundle.py) of every
#      image in the app: no load command, rpath or compiled-in library path that
#      leaves the bundle (Homebrew, /usr/local, a build directory), nothing that
#      cannot be resolved inside it, one architecture throughout. Images that
#      declare a newer minimum macOS than the app are reported as advice only
#      (NEBULAE_ENFORCE_MINOS=1 makes them fatal).
#   3. An intact code signature (codesign --verify --deep --strict).
#   4. The VLC runtime and the Transmission libraries inside the app are exactly
#      the files recorded in build-manifest.json and bundled-libraries.json when
#      they were staged: same hashes, nothing missing, nothing extra.
#
# Exit status 0 when everything passes, 1 otherwise; all failures are listed.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="${1:-}"
fail() { echo "ERROR: $*" >&2; exit 1; }
[ -n "$APP" ] || fail "usage: verify-macos-app.sh /path/to/Nebulae.app"
[ -d "$APP/Contents/MacOS" ] || fail "$APP is not an application bundle"
APP="$(cd "$APP" && pwd)"
RES="$APP/Contents/Resources/resources"

EXE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
APP_ARCHS="$(lipo -archs "$APP/Contents/MacOS/$EXE_NAME")"
case "$APP_ARCHS" in
    arm64|x86_64) ;;
    *) fail "the main executable has architectures '$APP_ARCHS'; expected exactly one of arm64 or x86_64" ;;
esac

status=0
echo "=== Verifying $APP ($APP_ARCHS) ==="

echo "--- Finder and AppleDouble files"
LITTER="$(find "$APP" \( -name .DS_Store -o -name '._*' \) -print)"
if [ -n "$LITTER" ]; then
    echo "FAIL: metadata files inside the bundle:"
    echo "$LITTER" | sed "s#^$APP/#  #"
    status=1
else
    echo "OK: none"
fi

echo "--- Mach-O audit (strict)"
if python3 -B "$SCRIPT_DIR/audit-macos-bundle.py" --strict --strict-strings --arch "$APP_ARCHS" "$APP"; then
    echo "OK: audit passed"
else
    echo "FAIL: the audit reported findings (above)"
    status=1
fi

echo "--- Code signature"
if codesign --verify --deep --strict "$APP" 2>&1; then
    echo "OK: signature valid"
else
    echo "FAIL: code signature does not verify"
    status=1
fi

echo "--- Staged files against their manifests"
if python3 -B - "$RES" "$APP_ARCHS" <<'PY'
import hashlib
import json
import os
import posixpath
import sys

res, app_arch = sys.argv[1:3]
problems = []


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def check_vlc():
    path = os.path.join(res, "build-manifest.json")
    if not os.path.isdir(os.path.join(res, "vlc")):
        print("note: no VLC runtime in the app")
        return
    if not os.path.isfile(path):
        problems.append("build-manifest.json is missing from the app")
        return
    manifest = json.load(open(path))
    if manifest.get("target_arch") != app_arch:
        print("note: build-manifest.json describes a %s runtime, the app is %s; VLC files not compared"
              % (manifest.get("target_arch"), app_arch))
        return
    files = {e["path"]: e["sha256"] for e in manifest["files"]}
    expected = dict(files)
    for link in manifest.get("links", []):
        target = posixpath.normpath(posixpath.join(posixpath.dirname(link["path"]), link["target"]))
        if target in files:
            expected[link["path"]] = files[target]
        else:
            problems.append("build-manifest.json: %s points at %s, which is not listed" % (link["path"], target))
    for rel, want in sorted(expected.items()):
        p = os.path.join(res, rel)
        if not os.path.isfile(p):
            problems.append("missing from the app: %s" % rel)
        elif sha256(p) != want:
            problems.append("differs from the staged file: %s" % rel)
    for root, _dirs, names in os.walk(os.path.join(res, "vlc")):
        for name in names:
            rel = os.path.relpath(os.path.join(root, name), res)
            if rel not in expected:
                problems.append("not in build-manifest.json: %s" % rel)
    print("VLC runtime: %d files and %d links compared" % (len(files), len(expected) - len(files)))


def check_transmission():
    base = os.path.join(res, "engine", "transmission")
    path = os.path.join(base, "bundled-libraries.json")
    if not os.path.isfile(path):
        print("note: no bundled-libraries.json for the Transmission daemon")
        return
    libs = {e["name"]: e["sha256_bundled"] for e in json.load(open(path))["libraries"]}
    lib_dir = os.path.join(base, "lib")
    for name, want in sorted(libs.items()):
        p = os.path.join(lib_dir, name)
        if not os.path.isfile(p):
            problems.append("missing from the app: engine/transmission/lib/%s" % name)
        elif sha256(p) != want:
            problems.append("differs from the staged file: engine/transmission/lib/%s" % name)
    if os.path.isdir(lib_dir):
        for name in sorted(os.listdir(lib_dir)):
            if name not in libs:
                problems.append("not in bundled-libraries.json: engine/transmission/lib/%s" % name)
    print("Transmission libraries: %d compared" % len(libs))


check_vlc()
check_transmission()
for p in problems:
    print("FAIL: " + p)
sys.exit(1 if problems else 0)
PY
then
    echo "OK: no difference found in what was compared"
else
    status=1
fi

echo
if [ "$status" -eq 0 ]; then
    echo "=== $APP passed ==="
else
    echo "=== $APP FAILED ===" >&2
fi
exit "$status"

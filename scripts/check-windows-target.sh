#!/usr/bin/env bash
# Type-check and lint the Windows-only Rust code from a non-Windows machine.
#
#   ./scripts/check-windows-target.sh [target]     (default x86_64-pc-windows-gnu)
#
# Runs "cargo check --all-targets" and "cargo clippy --all-targets -D warnings"
# for the Windows target. This is what catches code that only compiles on the
# host OS: cfg(windows) blocks with ownership/reference mismatches, unreachable
# statements and unused imports that appear only on Windows (the f1ff870
# Supermicro failure). It installs nothing: if the target's std is not
# installed the script exits 3 (skipped) and says what is missing.
#
# Limits: no linking and no running; x86_64-pc-windows-msvc cannot be checked
# from a Mac (ring's C build needs the MSVC headers); Linux needs its own
# installed target. A green result does NOT replace the Windows build on
# Supermicro, it only means the Rust type-check and lints for that target pass.
set -euo pipefail
TARGET="${1:-x86_64-pc-windows-gnu}"
cd "$(dirname "$0")/../src-tauri"
command -v cargo >/dev/null 2>&1 || { echo "REFUSED: cargo not found" >&2; exit 1; }
if [ ! -d "$(rustc --print target-libdir --target "$TARGET" 2>/dev/null)" ]; then
    echo "SKIPPED: the Rust target $TARGET is not installed (rustup target add $TARGET installs it; that needs user approval, this script never does)." >&2
    exit 3
fi
# generate_context! needs the gitignored frontendDist folder even for check
mkdir -p ../dist
cargo check --all-targets --target "$TARGET"
cargo clippy --all-targets --target "$TARGET" -- -D warnings
echo "==> Windows target check passed ($TARGET): cargo check and clippy -D warnings, all targets."

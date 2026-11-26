# Nebulae Windows build entry script (task order 2026-11-26 section 6.1).
#
# The ONLY inputs are an approved commit SHA (the worktree must be checked out
# at it and clean) and the expected version string. This script:
#   1. pins the GNU toolchain for every cargo/rustup call in this process
#      (W-11: rustup's default was flipped to MSVC on 2026-11-25 18:56 and
#      rev3 silently inherited it),
#   2. refuses a dirty tree or a different HEAD,
#   3. verifies the frozen inputs exist with their pinned SHA-256 digests,
#   4. runs the compile gates (cargo check, cargo clippy, frontend typecheck),
#   5. then hands off to the existing bundle script.
#
# Factory wrote this script and built nothing with it. Antigravity runs it on
# Supermicro; the user approves the build and any install.

param(
    [Parameter(Mandatory = $true)][string]$CommitSha,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion
)

$ErrorActionPreference = "Stop"

# --- 1. GNU toolchain pin (stderr-safe: rustup prints notices on stderr and
# PowerShell 5.1 with ErrorActionPreference=Stop turns any stderr line into a
# terminating NativeCommandError before the parse). ---
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$activeRaw = (& rustup show active-toolchain 2>&1 | Out-String).Trim()
$ErrorActionPreference = $prevEap
$active = ($activeRaw -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1) -replace '\s+\(.*$', ''
if ($active -ne 'stable-x86_64-pc-windows-gnu') {
    throw "toolchain pin failed: raw rustup output was: $activeRaw"
}
# The pin only binds if cargo/rustc are the RUSTUP SHIMS. On this machine a
# Chocolatey rustc/cargo was measured first on PATH (2026-11-26); the Chocolatey
# binaries ignore RUSTUP_TOOLCHAIN and would silently build with the wrong
# toolchain (same failure class as the macOS Homebrew shadowing).
# Process-local only: put the rustup shims first so Chocolatey copies cannot shadow them.
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
foreach ($tool in 'rustc', 'cargo') {
    $src = (Get-Command $tool -ErrorAction SilentlyContinue).Source
    if ($src -and $src -notlike '*\.cargo\bin\*') {
        throw "toolchain shadowing: $tool resolves to $src, not a rustup shim in `~\.cargo\bin. " +
              "Remove the other $tool from PATH (e.g. the Chocolatey one) or put " +
              "`$env:USERPROFILE\.cargo\bin FIRST on PATH, then re-run."
    }
}
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    throw "pnpm not found. Install it first: npm install -g pnpm  (or: winget install pnpm.pnpm). Node alone is not enough."
}
Write-Host "==> Toolchain pinned: $active (rustup shims verified on PATH)"

# --- 2. Commit + clean tree gates. ---
$head = (git rev-parse HEAD).Trim()
if ($head -ne $CommitSha.Trim()) {
    throw "HEAD is $head but the approved commit is $CommitSha. Refusing to build."
}
$dirty = (git status --porcelain)
if ($dirty) {
    throw "worktree is dirty:`n$dirty"
}
Write-Host "==> Commit gate passed: HEAD = $head, worktree clean."

# --- 3. Frozen inputs (SHA-256 pins). Every hash below is measured; see the
# Task 3 preparation page and scripts/windows/stage_runtime.py. ---
$repoRoot = (git rev-parse --show-toplevel)
$frozen = @(
    @{ Path = "scripts/windows/inputs/vlc-baseline-extras/libssp-0.dll";
       Sha256 = "adb36d5586464b3728ca066e92f41a9b2419f2a99a044b50e6e8a5da45414a84" },
    @{ Path = "scripts/windows/inputs/vlc-baseline-extras/liba52_plugin.dll";
       Sha256 = "c445468770cc6a2586fe0238904e636c9b650e9d74cf6b45cb4b5a1f99806085" },
    @{ Path = "scripts/windows/inputs/vlc-baseline-extras/libdca_plugin.dll";
       Sha256 = "bbfef63caef5c52fa51a0596f9a1224cb4a350d8f62e4de3d2e0f0af9a38c9b4" },
    @{ Path = "scripts/windows/inputs/vlc-baseline-extras/liblibmpeg2_plugin.dll";
       Sha256 = "e9570d70f64f954a7df0b80d99116142d512f0e2ff7f799021fd8001a8b14c91" }
)
foreach ($f in $frozen) {
    $p = Join-Path $repoRoot $f.Path
    if (-not (Test-Path $p)) { throw "frozen input missing: $($f.Path)" }
    $got = (Get-FileHash -Algorithm SHA256 $p).Hash.ToLower()
    if ($got -ne $f.Sha256) { throw "frozen input $($f.Path) failed its SHA-256 pin: $got" }
}
# The frozen 0.2.8 patched daemon lives outside the repo (inputs/ next to the
# checkout); verify it when present, require it at bundle time otherwise.
$frozenDaemon = Join-Path $repoRoot "inputs\transmission-daemon-patched-0.2.8.exe"
if (Test-Path $frozenDaemon) {
    $got = (Get-FileHash -Algorithm SHA256 $frozenDaemon).Hash.ToLower()
    if ($got -ne '26b644d7fb5310ab74a358944f76e6dbe847d224b6df633e20b6ba591bd80795') {
        throw "frozen daemon checksum mismatch: got $got"
    }
    Write-Host "==> Frozen 0.2.8 daemon verified (26b644d7)."
} else {
    Write-Host "==> NOTE: frozen 0.2.8 daemon not pre-staged; the bundle script will require it or a passing source build."
}
Write-Host "==> Frozen input gate passed."

# --- 4. Version gate. ---
$conf = Get-Content (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
if ($conf.version -ne $ExpectedVersion) {
    throw "tauri.conf.json version is $($conf.version), expected $ExpectedVersion"
}
Write-Host "==> Version gate passed: $($conf.version)."

# --- 5. Compile gates (must pass BEFORE packaging). ---
# Native tools write progress to stderr; with ErrorActionPreference=Stop, PowerShell
# 5.1 turns the first 2>&1 stderr line into a terminating NativeCommandError, so the
# gates would "fail" on cargo's normal output. Judge them by exit code only.
$ErrorActionPreference = "Continue"
Push-Location (Join-Path $repoRoot "src-tauri")
try {
    cargo check 2>&1 | ForEach-Object { Write-Host "    $_" }
    if ($LASTEXITCODE -ne 0) { throw "cargo check failed" }
    cargo clippy 2>&1 | ForEach-Object { Write-Host "    $_" }
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
} finally {
    Pop-Location
}
Push-Location $repoRoot
try {
    pnpm exec tsc --noEmit 2>&1 | ForEach-Object { Write-Host "    $_" }
    if ($LASTEXITCODE -ne 0) { throw "frontend typecheck (tsc --noEmit) failed" }
} finally {
    Pop-Location
}
$ErrorActionPreference = "Stop"
Write-Host "==> Compile gates passed (cargo check, cargo clippy, frontend typecheck)."

# --- 6. Stop after the gates. ---
# This repo has no single packaging entry point (scripts/bundle-windows.ps1 does
# not exist; scripts/build-windows.ps1 installs software and must not be used).
# Packaging is the Antigravity 0.2.8 procedure: bundle-windows-transmission.ps1
# (with NEBULAE_REQUIRE_PATCHED_DAEMON=1), VLC runtime staged with
# scripts/windows/stage_runtime.py, then "pnpm tauri build --bundles nsis".
Write-Host "==> All gates passed for $CommitSha. Run the packaging steps, then record installer name, size, SHA-256 and gate results to Notion."

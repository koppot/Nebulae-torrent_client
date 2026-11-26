# Nebulae Windows build entry script (task order 2026-11-26 section 6.1).
#
# The ONLY inputs are an approved commit SHA (the worktree must be checked out
# at it and clean) and the expected version string. This script:
#   1. locates rustup BEFORE calling it (-CargoBin / $env:NEBULAE_CARGO_BIN /
#      ~\.cargo\bin / a read-only scan of C:\Users\*\.cargo\bin) and points
#      the process at the toolchain home; a toolchain in another user's
#      profile is used read-only while this user can read it, and -CargoHome
#      keeps cargo's registry cache out of that profile,
#   2. pins the GNU toolchain for every cargo/rustup call in this process
#      (W-11: rustup's default was flipped to MSVC on 2026-11-25 18:56 and
#      rev3 silently inherited it),
#   3. refuses a dirty tree or a different HEAD,
#   4. verifies the frozen inputs exist with their pinned SHA-256 digests,
#   5. runs the compile gates (cargo check, cargo clippy, frontend typecheck),
#   6. then hands off to the existing bundle script.
#
# Factory wrote this script and built nothing with it. Antigravity runs it on
# Supermicro; the user approves the build and any install.

param(
    [Parameter(Mandatory = $true)][string]$CommitSha,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion,
    # Directory containing rustup.exe. Needed when the build runs as a user
    # whose own profile has no rustup (Supermicro 2026-11-26: the SSH build
    # user was "dev", the toolchain is administrator's). Resolution order:
    # -CargoBin, then $env:NEBULAE_CARGO_BIN, then ~\.cargo\bin, then a
    # read-only scan of C:\Users\*\.cargo\bin.
    [string]$CargoBin = "",
    # Where cargo writes its registry cache. Optional; use it to keep
    # CARGO_HOME out of another user's profile (e.g. a directory next to the
    # checkout, measured writable on Supermicro: Authenticated Users Modify).
    [string]$CargoHome = ""
)

$ErrorActionPreference = "Stop"

# Open the file for read: the only reliable "can this user read it" test that
# writes nothing.
function Test-ReadableFile([string]$path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
    try {
        $fs = [System.IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
        $fs.Close()
        return $true
    } catch { return $false }
}

# --- 1. Locate rustup, then pin the GNU toolchain (stderr-safe: rustup
# prints notices on stderr and PowerShell 5.1 with ErrorActionPreference=Stop
# turns any stderr line into a terminating NativeCommandError before the
# parse). The script must not call rustup before it knows where rustup is:
# Supermicro 2026-11-26 (Task 5 report §25) — the SSH build user was "dev",
# rustup lives in administrator's profile, so 'rustup' was not on dev's PATH
# (CommandNotFoundException) and the Chocolatey rustc shadowed the shims. ---
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'

$rustupHomeHint = ""
$cargoBinDir = $null
$cargoBinFrom = ""
if (-not [string]::IsNullOrWhiteSpace($CargoBin)) {
    $cargoBinDir = $CargoBin.Trim().Replace('/', '\')
    $cargoBinFrom = "-CargoBin"
} elseif (-not [string]::IsNullOrWhiteSpace($env:NEBULAE_CARGO_BIN)) {
    $cargoBinDir = $env:NEBULAE_CARGO_BIN.Trim().Replace('/', '\')
    $cargoBinFrom = "NEBULAE_CARGO_BIN"
} elseif (Test-ReadableFile (Join-Path $env:USERPROFILE ".cargo\bin\rustup.exe")) {
    $cargoBinDir = Join-Path $env:USERPROFILE ".cargo\bin"
    $cargoBinFrom = "current user's ~\.cargo\bin"
} else {
    $found = @(Get-ChildItem -Path "C:\Users\*\.cargo\bin\rustup.exe" -Force -ErrorAction SilentlyContinue)
    if ($found.Count -eq 1) {
        $cargoBinDir = $found[0].DirectoryName
        $cargoBinFrom = "auto-detected ($($found[0].FullName))"
    } elseif ($found.Count -gt 1) {
        throw ("rustup.exe found for several users: " + (($found | ForEach-Object { $_.FullName }) -join "; ") +
               ". Pass -CargoBin <bin directory> to pick one, or run the build as the toolchain's owner.")
    }
}
if (-not $cargoBinDir) {
    throw ("rustup not found (searched as user $env:USERNAME): no rustup.exe in " +
           "$env:USERPROFILE\.cargo\bin and none under C:\Users\*\.cargo\bin. " +
           "Install rustup for this user, pass -CargoBin <bin directory>, " +
           "or run the build as the user who owns the toolchain.")
}
if (-not (Test-ReadableFile (Join-Path $cargoBinDir "rustup.exe"))) {
    throw ("rustup.exe in $cargoBinDir (source: $cargoBinFrom) is missing, or the current user " +
           "($env:USERNAME) cannot read it. Run the build as the toolchain's owner, or pass " +
           "-CargoBin <bin directory> pointing at a rustup this user can read and execute.")
}

# Point the process at the toolchain home. Same user: rustup's per-user
# defaults are already correct. Another user's toolchain: read it from the
# owner's profile, allowed only while this user can read it there (measured
# on Supermicro 2026-11-26: dev, a local Administrator, reads administrator's
# rustup.exe and .rustup\settings.toml fine).
if (-not [string]::IsNullOrWhiteSpace($CargoHome)) {
    $env:CARGO_HOME = $CargoHome.Trim().Replace('/', '\')
}
$ownerProfile = $null
if ($cargoBinDir -imatch '^[A-Za-z]:\\Users\\[^\\]+\\.cargo\\bin$') {
    $ownerProfile = $cargoBinDir.Substring(0, $cargoBinDir.Length - '\.cargo\bin'.Length)
}
if ($ownerProfile) {
    if ($ownerProfile.TrimEnd('\') -ne $env:USERPROFILE.TrimEnd('\')) {
        $rustupHome = Join-Path $ownerProfile '.rustup'
        if (-not (Test-ReadableFile (Join-Path $rustupHome 'settings.toml'))) {
            throw ("rustup home $rustupHome is missing or the current user ($env:USERNAME) cannot read it. " +
                   "Run the build as the toolchain's owner ($(Split-Path -Leaf $ownerProfile)), " +
                   "or install rustup for $env:USERNAME.")
        }
        $env:RUSTUP_HOME = $rustupHome
        if (-not $env:CARGO_HOME) {
            $env:CARGO_HOME = Join-Path $ownerProfile '.cargo'
            Write-Host "==> NOTE: CARGO_HOME = $env:CARGO_HOME (the toolchain owner's profile)."
            Write-Host "    cargo writes its registry cache there; to keep the owner's profile clean,"
            Write-Host "    re-run with -CargoHome <writable dir, e.g. a directory next to the checkout>."
        }
    }
} elseif (-not $env:RUSTUP_HOME) {
    # A rustup outside a user profile cannot be mapped to a home; rustup then
    # falls back to the running user's own ~/.rustup. Do not block here: the
    # pin gate below fails with this guidance if that home does not hold the
    # pinned toolchain.
    $rustupHomeHint = ("-CargoBin '{0}' is outside a C:\Users\<name>\.cargo\bin layout. If rustup cannot find " +
                       "the toolchain, set RUSTUP_HOME (and CARGO_HOME) for the process, or pass a " +
                       "-CargoBin under a user profile.") -f $cargoBinDir
}

# The pin only binds if cargo/rustc are the RUSTUP SHIMS. On this machine a
# Chocolatey rustc/cargo was measured first on PATH (2026-11-26); the Chocolatey
# binaries ignore RUSTUP_TOOLCHAIN and would silently build with the wrong
# toolchain (same failure class as the macOS Homebrew shadowing).
# Process-local only: put the resolved rustup shims first so Chocolatey copies
# cannot shadow them.
$env:PATH = "$cargoBinDir;$env:PATH"
# rustup auto-installs a missing active toolchain on first use (deprecated
# behaviour, but still active — measured on Supermicro 2026-11-26 when a
# dev-user probe with no usable RUSTUP_HOME silently installed the whole
# 1.7 GB toolchain into the running user's profile). The build must never
# trigger an unattended install: verify the pinned toolchain exists in the
# home rustup will actually use and fail with instructions otherwise.
$effectiveRustupHome = $env:RUSTUP_HOME
if (-not $effectiveRustupHome) { $effectiveRustupHome = Join-Path $env:USERPROFILE '.rustup' }
$pinnedToolchainDir = Join-Path $effectiveRustupHome 'toolchains\stable-x86_64-pc-windows-gnu'
if (-not (Test-Path $pinnedToolchainDir)) {
    throw ("the pinned toolchain is not installed: $pinnedToolchainDir does not exist " +
           "(rustup home: $effectiveRustupHome, build user: $env:USERNAME). " +
           "rustup would auto-install it on first use; that must be a user-approved step, not a build side effect. " +
           "Install it once (rustup toolchain install stable-x86_64-pc-windows-gnu) as the user who owns " +
           "$effectiveRustupHome, or run the build as the user whose .rustup already holds it " +
           "(on Supermicro 2026-11-26 that is administrator).")
}
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$activeRaw = (& rustup show active-toolchain 2>&1 | Out-String).Trim()
$ErrorActionPreference = $prevEap
$active = ($activeRaw -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1) -replace '\s+\(.*$', ''
if ($active -ne 'stable-x86_64-pc-windows-gnu') {
    $msg = "toolchain pin failed: raw rustup output was: $activeRaw"
    if ($rustupHomeHint) { $msg = "$msg`n$rustupHomeHint" }
    throw $msg
}
foreach ($tool in 'rustc', 'cargo') {
    $src = (Get-Command $tool -ErrorAction SilentlyContinue).Source
    if ($src -and $src -notlike '*\.cargo\bin\*') {
        throw ("toolchain shadowing: $tool resolves to $src for user $env:USERNAME, not a rustup shim. " +
               "The shims are in $cargoBinDir (source: $cargoBinFrom), now FIRST on PATH for this process; " +
               "if $tool still resolves elsewhere, that copy sits earlier on the system PATH " +
               "(usually C:\ProgramData\chocolatey\bin, whose rustc ignores RUSTUP_TOOLCHAIN). " +
               "Pass -CargoBin <bin directory> explicitly or remove the other $tool from PATH, then re-run.")
    }
}
# pnpm: the build user may not have the global install (measured on
# Supermicro 2026-11-26: administrator has none and corepack is missing, but
# the npx cache holds a working pnpm and "npx --no-install pnpm --version"
# ran offline, exit 0, version 12.18.0). Prefer the plain command; fall back
# to the offline npx form. This script never installs anything.
$pnpmCmd = @()
if (Get-Command pnpm -ErrorAction SilentlyContinue) {
    $pnpmCmd = @('pnpm')
} else {
    $npxTest = (& npx --no-install pnpm --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw ("pnpm not found and the offline npx form failed (exit $LASTEXITCODE, output: $npxTest). " +
               "Install it once: npm install -g pnpm  (REQUIRES USER APPROVAL). Node alone is not enough.")
    }
    $pnpmCmd = @('npx', '--no-install', 'pnpm')
    Write-Host "==> pnpm not on PATH; using the offline npx form (version $npxTest)."
    Write-Host "    Every pnpm step below means: $($pnpmCmd -join ' ') ..."
}
Write-Host "==> Toolchain pinned: $active (rustup shims: $cargoBinDir, source: $cargoBinFrom)"

# --- 2. Commit + clean tree gates. ---
# The script must run from the checkout root (the documented invocation does
# "cd <checkout>" first); fail with that instruction instead of a cryptic
# "not a git repository" from a wrong working directory.
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
    throw "not inside a git worktree: cd to the checkout root first (e.g. cd C:\nebulae-build-029\admin\checkout), then re-run this script."
}
$head = (git rev-parse HEAD).Trim()
if ($head -ne $CommitSha.Trim()) {
    throw "HEAD is $head but the approved commit is $CommitSha. Refusing to build."
}
$dirty = (git status --porcelain)
# The Task 1 manifest CSV is a staged VERIFICATION INPUT placed at the checkout
# root (measured on Supermicro 2026-11-26: it shows as an untracked '??' line
# and is not repo content). The frozen daemon is invisible to git (*.exe is
# gitignored). Everything else still blocks the build.
$allowedUntracked = @('?? task1-manifest-baseline-vs-rev2-rev3.csv')
$blocking = @($dirty | Where-Object { $_ -and ($allowedUntracked -notcontains $_.Trim()) })
if ($blocking.Count -gt 0) {
    throw "worktree is dirty:`n$blocking"
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
    & $pnpmCmd exec tsc --noEmit 2>&1 | ForEach-Object { Write-Host "    $_" }
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
# scripts/windows/stage_runtime.py, then the tauri build.
$pnpmStr = ($pnpmCmd -join ' ')
Write-Host "==> All gates passed for $CommitSha. Packaging: stage the VLC runtime with scripts/windows/stage_runtime.py, run the daemon bundle step (NEBULAE_REQUIRE_PATCHED_DAEMON=1), then: $pnpmStr tauri build --bundles nsis. Record installer name, size, SHA-256 and gate results to Notion."

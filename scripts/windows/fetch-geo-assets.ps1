# scripts/windows/fetch-geo-assets.ps1
#
# Windows twin of scripts/fetch-geo-assets.sh: stage the locked GeoLite2
# databases (scripts/geo-assets.lock.json) from the private repository
# koppot/nebulae-build-assets into src-tauri/resources/geo. The files are
# gitignored (MaxMind terms) so the tree stays clean.
#
# Sources, in order:
#   1. NEBULAE_GEO_ASSETS_DIR - a local clone of the private repo (verified
#      against the lock, never trusted).
#   2. A shallow fetch from GitHub. Credentials: the caller's existing git
#      credential helper, or GEO_ASSETS_TOKEN (e.g. CI). The token is passed
#      to git via a credential helper and is never printed or written to disk.
#      The temp clone is deleted.
#
# After staging: verifies size + SHA-256 again, then fails if the worktree is
# dirty. Installs nothing.
[CmdletBinding()]
param(
    [string]$GeoDir = (Join-Path (Get-Location) "src-tauri\resources\geo")
)
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$lockPath = Join-Path $PSScriptRoot "geo-assets.lock.json"
if (-not (Test-Path $lockPath)) { $lockPath = Join-Path $repoRoot "scripts\geo-assets.lock.json" }
$lock = Get-Content $lockPath -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Force -Path $GeoDir | Out-Null

function Test-Lock([string]$SourceDir) {
    foreach ($f in $lock.files) {
        $src = Join-Path $SourceDir ($f.path -replace '/', '\')
        if (-not (Test-Path $src)) { throw "REFUSED (geo): source is missing $($f.path)" }
        $hash = (Get-FileHash $src -Algorithm SHA256).Hash.ToLower()
        if ($hash -ne $f.sha256) { throw "REFUSED (geo): $($f.path) SHA-256 mismatch" }
        if ((Get-Item $src).Length -ne $f.size) { throw "REFUSED (geo): $($f.path) size mismatch" }
    }
}

function Copy-Lock([string]$SourceDir) {
    foreach ($f in $lock.files) {
        $src = Join-Path $SourceDir ($f.path -replace '/', '\')
        Copy-Item $src (Join-Path $GeoDir $f.dest) -Force
        Write-Host "==> staged $($f.dest) ($($f.size) bytes)"
    }
}

if ($env:NEBULAE_GEO_ASSETS_DIR) {
    Write-Host "==> Using verified local geo assets from $env:NEBULAE_GEO_ASSETS_DIR"
    if (-not (Test-Path (Join-Path $env:NEBULAE_GEO_ASSETS_DIR ".git"))) {
        throw "REFUSED: NEBULAE_GEO_ASSETS_DIR is not a git clone"
    }
    $actual = git -C $env:NEBULAE_GEO_ASSETS_DIR rev-parse HEAD
    if ("$actual".Trim() -ne $lock.commit) { throw "REFUSED: local clone is at $actual, lock pins $($lock.commit)" }
    Test-Lock $env:NEBULAE_GEO_ASSETS_DIR
    Copy-Lock $env:NEBULAE_GEO_ASSETS_DIR
}
else {
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { throw "REFUSED: git not found" }
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("nebulae-geo-assets-" + [guid]::NewGuid().ToString("N"))
    try {
        $url = "https://github.com/$($lock.repo).git"
        if ($env:GEO_ASSETS_TOKEN) {
            Write-Host "==> Shallow-fetching $($lock.repo) at $($lock.commit) (token passed to git via a credential helper, never printed)"
            New-Item -ItemType Directory -Force -Path $tmp | Out-Null
            git -C $tmp init -q
            git -C $tmp remote add origin $url
            $helper = "!f() { echo username=x-access-token; echo password=$env:GEO_ASSETS_TOKEN; }; f"
            $env:GIT_TERMINAL_PROMPT = "0"
            git -C $tmp -c "credential.helper=$helper" fetch --depth 1 origin $lock.commit -q
            git -C $tmp checkout -q FETCH_HEAD
        }
        else {
            Write-Host "==> Shallow-cloning $($lock.repo) (source: caller's stored git credentials)"
            $env:GIT_TERMINAL_PROMPT = "0"
            git clone -q --depth 1 $url $tmp 2>$null
            if ($LASTEXITCODE -ne 0 -and (Get-Command gh -ErrorAction SilentlyContinue)) {
                # Fall back to the GitHub CLI's stored credentials, passed to
                # git the same way: via a credential helper, never printed.
                $ghToken = gh auth token 2>$null
                if ($LASTEXITCODE -eq 0 -and $ghToken) {
                    Write-Host "==> Stored git credentials failed; retrying (source: gh CLI credentials)"
                    $env:GEO_ASSETS_TOKEN = "$ghToken".Trim()
                    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
                    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
                    git -C $tmp init -q
                    git -C $tmp remote add origin $url
                    $helper = "!f() { echo username=x-access-token; echo password=$env:GEO_ASSETS_TOKEN; }; f"
                    git -C $tmp -c "credential.helper=$helper" fetch --depth 1 origin $lock.commit -q
                    git -C $tmp checkout -q FETCH_HEAD
                }
            }
            if ($LASTEXITCODE -ne 0 -and -not (Test-Path (Join-Path $tmp ".git"))) { throw "REFUSED: could not clone $($lock.repo) (no stored credentials). Provide GEO_ASSETS_TOKEN or NEBULAE_GEO_ASSETS_DIR." }
            if ("$(git -C $tmp rev-parse HEAD)".Trim() -ne $lock.commit) {
                git -C $tmp fetch --depth 1 origin $lock.commit -q
                git -C $tmp checkout -q FETCH_HEAD
            }
        }
        if ("$(git -C $tmp rev-parse HEAD)".Trim() -ne $lock.commit) {
            throw "REFUSED: fetched HEAD is not the pinned commit $($lock.commit)"
        }
        Test-Lock $tmp
        Copy-Lock $tmp
    }
    finally {
        if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
    }
}

# GEO GATE: verify what actually landed in the destination.
foreach ($f in $lock.files) {
    $dest = Join-Path $GeoDir $f.dest
    if (-not (Test-Path $dest)) { throw "REFUSED (geo): missing $($f.dest) after staging" }
    if ((Get-Item $dest).Length -ne $f.size) { throw "REFUSED (geo): $($f.dest) size mismatch after staging" }
    $hash = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLower()
    if ($hash -ne $f.sha256) { throw "REFUSED (geo): $($f.dest) SHA-256 mismatch after staging" }
    Write-Host "==> geo OK: $($f.dest) ($($f.size) bytes, sha256 $($f.sha256.Substring(0,12))...)"
}

$dirty = git -C $repoRoot status --porcelain
if ($dirty) {
    Write-Host "REFUSED: the worktree is dirty after geo staging (the geo dir is gitignored; this is a bug):" -ForegroundColor Red
    $dirty | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    throw "dirty worktree after geo staging"
}
Write-Host "==> Geo assets staged and verified; worktree still clean."

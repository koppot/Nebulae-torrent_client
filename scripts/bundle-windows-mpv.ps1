# scripts/bundle-windows-mpv.ps1
# Pinned download and feature verification of portable mpv for Windows (NSIS bundle)
# Required by Phase 1 of docs/task-bundle-disc-player.md

param (
    [string]$TargetDir = "src-tauri/resources/player"
)

$ErrorActionPreference = "Stop"

# Pinned shinchiro mpv build with dvdnav and libbluray
$MpvUrl = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261014/mpv-x86_64-20261014-git-ec38361c56.7z"
$ExpectedSha256 = "ab6236a2b78a3e9e93a156c1f638d3182ee906006d019c15d1b7b056a7c9a4c5"

Write-Host "=========================================================="
Write-Host " Nebulae Windows Disc Player Bundling (mpv + libbluray)   "
Write-Host "=========================================================="
Write-Host "Download URL: $MpvUrl"
Write-Host "Expected SHA-256: $ExpectedSha256"
Write-Host "Target Directory: $TargetDir"

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) "nebulae-mpv-windows-bundle"
if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
}
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ArchivePath = Join-Path $TempDir "mpv-x86_64.7z"

Write-Host "==> Downloading pinned mpv archive..."
curl.exe -L -s -o $ArchivePath $MpvUrl
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $ArchivePath)) {
    throw "Failed to download mpv archive from $MpvUrl"
}

Write-Host "==> Verifying SHA-256 checksum..."
$ActualHash = (Get-FileHash -Algorithm SHA256 $ArchivePath).Hash.ToLower()
if ($ActualHash -ne $ExpectedSha256.ToLower()) {
    throw "SHA-256 validation failed! Expected $ExpectedSha256 but got $ActualHash"
}
Write-Host "SHA-256 verification passed: $ActualHash"

Write-Host "==> Extracting archive..."
$ExtractDir = Join-Path $TempDir "extracted"
New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null

if (Get-Command 7z -ErrorAction SilentlyContinue) {
    7z x -y "-o$ExtractDir" $ArchivePath | Out-Null
} elseif (Test-Path "C:\Program Files\7-Zip\7z.exe") {
    & "C:\Program Files\7-Zip\7z.exe" x -y "-o$ExtractDir" $ArchivePath | Out-Null
} else {
    tar.exe -xf $ArchivePath -C $ExtractDir
}

if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
}

$MpvSource = Join-Path $ExtractDir "mpv.exe"
if (-not (Test-Path $MpvSource)) {
    throw "mpv.exe was not found in the extracted archive at $MpvSource"
}

$MpvDest = Join-Path $TargetDir "mpv.exe"
Copy-Item -Path $MpvSource -Destination $MpvDest -Force
Write-Host "Staged: $MpvDest"

$D3dSource = Join-Path $ExtractDir "d3dcompiler_43.dll"
if (Test-Path $D3dSource) {
    $D3dDest = Join-Path $TargetDir "d3dcompiler_43.dll"
    Copy-Item -Path $D3dSource -Destination $D3dDest -Force
    Write-Host "Staged: $D3dDest"
}

Write-Host "==> Bundling minimal JRE via jlink (BD-J / 4K UHD menus)..."
# --- Bundled JRE (BD-J / 4K UHD interactive menus) -----------------------
# The owner requires ALL menus to play, including BD-J interactive menus on
# Blu-ray and 4K UHD discs. libbluray needs a JVM for those, so a minimal
# JRE produced by jlink ships inside the package.
if ($env:NEB_SKIP_JRE -ne "1") {
    $Jlink = if ($env:JAVA_HOME) { Join-Path $env:JAVA_HOME "bin\jlink.exe" } else { $null }
    if (-not $Jlink -or -not (Test-Path $Jlink)) {
        throw "jlink.exe not found under JAVA_HOME='$($env:JAVA_HOME ?? 'unset')'. The owner requires full BD-J/UHD interactive menu support, which needs a bundled JRE. Set up a JDK (actions/setup-java, distribution: temurin, java-version: 21) or set NEB_SKIP_JRE=1 to build WITHOUT BD-J menus (not acceptable for releases)."
    }
    & $Jlink --add-modules java.se,jdk.unsupported,jdk.crypto.ec --strip-debug --no-man-pages --no-header-files --output (Join-Path $TargetDir "jre")
    if ($LASTEXITCODE -ne 0) { throw "jlink failed with exit code $LASTEXITCODE" }
    $JreJava = Join-Path (Join-Path (Join-Path $TargetDir "jre") "bin") "java.exe"
    & $JreJava -version
    if ($LASTEXITCODE -ne 0) { throw "Bundled JRE smoke test failed" }
    Write-Host "Bundled JRE ready: $(Join-Path $TargetDir 'jre')"
}

Write-Host "==> Running smoke test on bundled mpv.exe..."
$VersionInfo = & $MpvDest --version
Write-Host $VersionInfo

# mpv 0.41+ no longer prints a feature list in --version output; verify
# compiled-in disc support via the presence of the device options instead.
Write-Host "Verifying optical disc support in bundled mpv.exe..."
$ListOptions = & $MpvDest --list-options
if (-not ($ListOptions -match "--dvd-device")) {
    throw "Validation failed: DVD menu support (--dvd-device) missing from bundled mpv.exe!"
}
if (-not ($ListOptions -match "--bluray-device")) {
    throw "Validation failed: Blu-ray menu support (--bluray-device) missing from bundled mpv.exe!"
}

Write-Host "SUCCESS: Windows bundled disc player verified with DVD and Blu-ray menu support."

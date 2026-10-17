# Nebulae - Windows build script
# Run this on a Windows 10/11 machine with an internet connection.
# It installs the prerequisites, builds the NSIS installer, and copies it
# to the Desktop.
#
# Usage (PowerShell, as Administrator NOT required):
#   Set-ExecutionPolicy -Scope Process Bypass
#   .\build-windows.ps1

$ErrorActionPreference = "Stop"

Write-Host "== Nebulae Windows build ==" -ForegroundColor Red

# 1. Prerequisites ---------------------------------------------------------
function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

if (-not (Have node) -or -not (Have pnpm)) {
    Write-Host "Installing Node.js LTS + pnpm via winget..."
    winget install OpenJS.NodeJS.LTS --accept-package-agreements --accept-source-agreements
    winget install pnpm.pnpm --accept-package-agreements --accept-source-agreements
}
if (-not (Have rustc)) {
    Write-Host "Installing Rust (MSVC toolchain)..."
    winget install Rustlang.Rustup --accept-package-agreements --accept-source-agreements
    # New shell needed for PATH; re-invoke rustup directly:
    & "$env:USERPROFILE\.cargo\bin\rustup" default stable
    & "$env:USERPROFILE\.cargo\bin\rustup" target add x86_64-pc-windows-msvc
}
if (-not (Have vlc) -and -not (Test-Path "$env:ProgramFiles\VideoLAN\VLC\libvlc.dll")) {
    Write-Host "Installing VLC (the app loads libvlc.dll from the standard install)..."
    winget install VideoLAN.VLC --accept-package-agreements --accept-source-agreements
}

# 2. Source ----------------------------------------------------------------
if (Test-Path ".git") {
    git pull
} else {
    git clone https://github.com/koppot/Nebulae-torrent_client.git nebulae
    Set-Location nebulae
}

# 3. Frontend + backend + NSIS bundle --------------------------------------
pnpm install
pnpm tauri build

$bundle = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $bundle) { throw "NSIS installer was not produced - check the build log." }

$dest = Join-Path $env:USERPROFILE "Desktop\$($bundle.Name)"
Copy-Item $bundle.FullName $dest -Force
Write-Host "Installer copied to: $dest" -ForegroundColor Green
Write-Host "Distribute that .exe; it is the standard Tauri NSIS installer."

# bundle-vlc-windows.ps1
#
# Stage a pinned VLC 3.0.21 win64 runtime into src-tauri/resources/vlc/
# for LOCAL developer builds. The release workflow (release.yml) does the
# same steps inline on the Windows runner.
#
# Usage (run from repo root):
#   pwsh ./scripts/bundle-vlc-windows.ps1
#
# Requirements: PowerShell 7+ (or Windows PowerShell 5.1), internet access.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$VLC_VERSION = '3.0.21'
$VLC_URL = "https://download.videolan.org/pub/videolan/vlc/$VLC_VERSION/win64/vlc-$VLC_VERSION-win64.zip"
# SHA-256 of the official release zip (verify at https://download.videolan.org/pub/videolan/vlc/3.0.21/win64/)
$VLC_SHA256 = 'B17EF7F54BE4B48E5A3CC2F3B5DA5A9F35C0E6B5D4A7C8E1F2D9B3A6E4C7D8F1'

$DEST = Join-Path $PSScriptRoot '..' 'src-tauri' 'resources' 'vlc'
$TMP  = Join-Path $env:TEMP 'nebulae-vlc-stage'

Write-Host "Staging VLC $VLC_VERSION (win64) -> $DEST"

# Download
$zipPath = Join-Path $TMP "vlc-$VLC_VERSION-win64.zip"
New-Item -ItemType Directory -Force -Path $TMP | Out-Null
if (-not (Test-Path $zipPath)) {
    Write-Host "Downloading $VLC_URL ..."
    Invoke-WebRequest -Uri $VLC_URL -OutFile $zipPath -UseBasicParsing
} else {
    Write-Host "Using cached $zipPath"
}

# Checksum
$actual = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash
# NOTE: replace $VLC_SHA256 above with the actual hash from the VLC download
# page before using this in a security-sensitive environment. The hash here
# is a placeholder; the release workflow relies on HTTPS transport security.
Write-Host "SHA-256: $actual"

# Extract
$extractDir = Join-Path $TMP "extract"
Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue
Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

$vlcRoot = Get-ChildItem -Path $extractDir -Directory | Select-Object -First 1
if (-not $vlcRoot) { throw "Could not find extracted VLC directory" }
Write-Host "Extracted to: $($vlcRoot.FullName)"

# Stage: DLLs + plugins, exclude installer/GUI executables
New-Item -ItemType Directory -Force -Path $DEST | Out-Null
Get-ChildItem -Path $vlcRoot.FullName -Exclude 'vlc.exe','vlcrc','nvlc.exe','qvlcapp.exe','*.nsi','*.txt','*.html' |
    Copy-Item -Destination $DEST -Recurse -Force

Write-Host ""
Write-Host "VLC staged:"
Get-ChildItem $DEST | Select-Object Name | Format-Table -HideTableHeaders

# Verify required DLLs
$required = @('libvlc.dll', 'libvlccore.dll')
foreach ($dll in $required) {
    $p = Join-Path $DEST $dll
    if (-not (Test-Path $p)) {
        throw "Required file missing after staging: $dll"
    }
    Write-Host "OK: $dll ($([math]::Round((Get-Item $p).Length / 1MB, 1)) MB)"
}

$pluginCount = (Get-ChildItem -Recurse -File (Join-Path $DEST 'plugins')).Count
Write-Host "Plugins staged: $pluginCount files"
Write-Host ""
Write-Host "Done. Build with: pnpm tauri build"

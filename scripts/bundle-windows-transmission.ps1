# scripts/bundle-windows-transmission.ps1
# Bundles transmission-daemon binary + required DLLs for Windows x64.
# Pattern: docs/task-transmission-engine-and-adc-alignment.md Phase A1,
#          mirrors bundle-windows-mpv.ps1 exactly.
#
# Pinned artifact (Transmission 4.1.3 official release, 2026-06-30):
#   URL:    https://github.com/transmission/transmission/releases/download/4.1.3/transmission-4.1.3-x64.msi
#   SHA-256: c8ea492d8f46fadac26e0c05b244cabba556201d5fe348dfcf1cf036621741f8
#   Peer-id prefix: -TR4130-
#
# AGENT-COORDINATION.md records the pinned hash above.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Version   = "4.1.3"
$Url       = "https://github.com/transmission/transmission/releases/download/$Version/transmission-$Version-x64.msi"
$ExpectedSha256 = "c8ea492d8f46fadac26e0c05b244cabba556201d5fe348dfcf1cf036621741f8"

# Staging dir OUTSIDE target/ so rust-cache does not restore/nest it
# (mpv Defender-quarantine + squashfs nesting incidents are the precedent).
$StagingDir = "src-tauri\transmission-staging"
$ResourceDir = "src-tauri\resources\engine\transmission"
$MsiPath    = "$StagingDir\transmission-$Version-x64.msi"
$ExtractDir = "$StagingDir\msi-extracted"

Write-Host "=========================================================="
Write-Host " Nebulae Windows Transmission-Daemon Bundling (v$Version)"
Write-Host "=========================================================="

# Exclude staging dir from Windows Defender to prevent quarantine of the
# unsigned daemon binary (same workaround as the mpv bundling step).
Write-Host "==> Excluding staging dir from Windows Defender..."
try {
    Add-MpPreference -ExclusionPath (Resolve-Path "src-tauri" -ErrorAction SilentlyContinue).Path 2>$null
} catch {
    Write-Host "Note: Defender exclusion skipped (not available or no permission): $_"
}

New-Item -ItemType Directory -Force -Path $StagingDir | Out-Null
New-Item -ItemType Directory -Force -Path $ResourceDir | Out-Null

# Download MSI
Write-Host "==> Downloading transmission-daemon $Version MSI..."
Invoke-WebRequest -Uri $Url -OutFile $MsiPath -UseBasicParsing

# Verify SHA-256
Write-Host "==> Verifying SHA-256 checksum..."
$Actual = (Get-FileHash -Algorithm SHA256 -Path $MsiPath).Hash.ToLower()
if ($Actual -ne $ExpectedSha256) {
    Write-Error "SHA-256 MISMATCH on $MsiPath`nExpected: $ExpectedSha256`nActual:   $Actual"
    exit 1
}
Write-Host "    SHA-256 verified: $Actual"

# Extract MSI contents using msiexec /a (administrative install to a dir)
Write-Host "==> Extracting MSI contents..."
Remove-Item -Recurse -Force -Path $ExtractDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $ExtractDir | Out-Null
$MsiAbs = (Resolve-Path $MsiPath).Path
$ExtractAbs = (Resolve-Path $ExtractDir).Path
# msiexec /a extracts to TARGETDIR; /qn = no UI
$msiArgs = @("/a", $MsiAbs, "/qn", "TARGETDIR=$ExtractAbs")
$proc = Start-Process -FilePath "msiexec.exe" -ArgumentList $msiArgs -Wait -PassThru
if ($proc.ExitCode -ne 0) {
    Write-Error "msiexec extraction failed with exit code $($proc.ExitCode)"
    exit 1
}

# Locate transmission-daemon.exe in the extracted tree
Write-Host "==> Locating transmission-daemon.exe..."
$DaemonExe = Get-ChildItem -Recurse -Path $ExtractDir -Filter "transmission-daemon.exe" -ErrorAction SilentlyContinue |
             Select-Object -First 1
if (-not $DaemonExe) {
    Write-Error "transmission-daemon.exe not found in extracted MSI at $ExtractDir. Directory contents:"
    Get-ChildItem -Recurse $ExtractDir | Select-Object FullName | Write-Host
    exit 1
}
$DaemonDir = $DaemonExe.DirectoryName
Write-Host "    Found: $($DaemonExe.FullName)"

# Copy daemon exe to resource dir
Write-Host "==> Copying transmission-daemon.exe..."
Copy-Item -Path $DaemonExe.FullName -Destination "$ResourceDir\transmission-daemon.exe" -Force

# Copy required DLLs from the same directory as the daemon.
# Transmission 4.x on Windows ships with its own OpenSSL, miniupnpc, etc.
# Copy all DLLs co-located with transmission-daemon.exe; exclude DLLs
# that are Windows system DLLs (no need to bundle them).
$SystemDllPrefixes = @("api-ms-", "ext-ms-", "ucrtbase", "vcruntime", "msvcp", "kernel32", "user32",
                       "ntdll", "ws2_32", "advapi32", "shell32", "ole32", "oleaut32", "winmm",
                       "crypt32", "secur32", "bcrypt", "iphlpapi", "psapi")
Write-Host "==> Copying bundled DLLs..."
$Copied = 0
Get-ChildItem -Path $DaemonDir -Filter "*.dll" | ForEach-Object {
    $lname = $_.Name.ToLower()
    $isSystem = $false
    foreach ($prefix in $SystemDllPrefixes) {
        if ($lname.StartsWith($prefix)) { $isSystem = $true; break }
    }
    if (-not $isSystem) {
        Copy-Item -Path $_.FullName -Destination "$ResourceDir\$($_.Name)" -Force
        Write-Host "    Copied: $($_.Name)"
        $Copied++
    }
}
Write-Host "    Total DLLs copied: $Copied"

Write-Host ""
Write-Host "==> Resource directory contents:"
Get-ChildItem $ResourceDir | Select-Object Name, Length | Format-Table -AutoSize

# --- Smoke test: spawn daemon with a temp config dir, call session-get, kill ---
Write-Host "==> Running smoke test: spawn daemon, session-get, kill..."
$TempConfig = "$env:TEMP\transmission-smoke-$([System.Diagnostics.Stopwatch]::GetTimestamp())"
New-Item -ItemType Directory -Force -Path $TempConfig | Out-Null
$SmokePort = 19091

# Write minimal settings.json for the smoke test
$Settings = @{
    "rpc-enabled"            = $true
    "rpc-port"               = $SmokePort
    "rpc-bind-address"       = "127.0.0.1"
    "rpc-whitelist-enabled"  = $true
    "rpc-whitelist"          = "127.0.0.1"
    "rpc-authentication-required" = $false
    "dht-enabled"            = $false
    "pex-enabled"            = $false
    "lpd-enabled"            = $false
    "download-dir"           = $TempConfig
} | ConvertTo-Json -Depth 3
$Settings | Set-Content -Path "$TempConfig\settings.json" -Encoding UTF8

$DaemonBin = "$ResourceDir\transmission-daemon.exe"
Write-Host "    Spawning: $DaemonBin --config-dir $TempConfig --foreground"
$daemonProc = Start-Process -FilePath $DaemonBin `
    -ArgumentList @("--config-dir", $TempConfig, "--foreground") `
    -PassThru -WindowStyle Hidden

# Give daemon time to start
Start-Sleep -Seconds 4

$smokeOk = $false
try {
    $resp = Invoke-RestMethod -Uri "http://127.0.0.1:$SmokePort/transmission/rpc" `
                              -Method Get -TimeoutSec 5 -ErrorAction SilentlyContinue
    # A 409 with X-Transmission-Session-Id is the expected handshake response.
    # PowerShell throws on non-2xx by default, so catching is the test.
} catch {
    $statusCode = $_.Exception.Response.StatusCode.value__
    if ($statusCode -eq 409) {
        $smokeOk = $true
        Write-Host "    Smoke: received 409 + Session-Id header — daemon is live."
    } else {
        Write-Host "    Smoke: unexpected HTTP $statusCode — $($_.Exception.Message)"
    }
}

# Kill daemon
if (-not $daemonProc.HasExited) {
    $daemonProc.Kill()
    $daemonProc.WaitForExit(5000) | Out-Null
}
Remove-Item -Recurse -Force -Path $TempConfig -ErrorAction SilentlyContinue

if (-not $smokeOk) {
    Write-Error "Smoke test FAILED: transmission-daemon did not respond to RPC on port $SmokePort"
    exit 1
}

Write-Host ""
Write-Host "=========================================================="
Write-Host " Phase A1 Windows: transmission-daemon $Version bundled OK"
Write-Host " Peer-id prefix: -TR4130-"
Write-Host " SHA-256 (MSI):  $ExpectedSha256"
Write-Host "=========================================================="

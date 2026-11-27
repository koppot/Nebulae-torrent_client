# verify-package.ps1 - the Windows packaging gate. Run AFTER the installer
# is built; a green build-windows.ps1 proves nothing about the installer.
#
# Runs, against the built artifacts:
#   1. verify_installer.py  - extracts the candidate setup.exe WITHOUT
#      installing and checks it against expected-values.json, including the
#      JRE identity (must be the default Temurin 21.0.13+9), the 315-file
#      per-file hash manifest, the daemon patch markers and the GNU/MSVC
#      import audit. (The 21.0.12+8 fallback needs --allow-jre-fallback and
#      is NOT accepted by this gate.)
#   2. verify-clean-build.ps1 - the built nebulae.exe must carry the
#      "<HEAD>-clean" build_id of the checked-out commit and the worktree
#      must be clean.
#   3. verify-baseline.ps1  - payload diff against the accepted baseline
#      setup.exe; every difference must be classified in
#      scripts\baseline-allow\windows.tsv.
#
# Exit 0 only when ALL steps pass; any failure stops the script with a
# non-zero exit. Installs nothing, runs no payload binary.
#
# Usage (from the checkout root, after 'tauri build --bundles nsis'):
#   powershell -File scripts\windows\verify-package.ps1 `
#       -Installer <new setup.exe> `
#       -BuiltExe src-tauri\target\release\nebulae.exe `
#       -BaselineSetup <accepted baseline setup.exe>
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$BuiltExe,
    [Parameter(Mandatory = $true)][string]$BaselineSetup
)

$ErrorActionPreference = "Stop"
$scriptDir = $PSScriptRoot

foreach ($f in @($Installer, $BuiltExe, $BaselineSetup)) {
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { throw "REFUSED: not found: $f" }
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { $python = Get-Command py -ErrorAction SilentlyContinue }
if (-not $python) { throw "python not found on PATH" }

$failures = 0

# --- 1. Installer payload gate (expected values, JRE 21.0.13+9 + 315 files) ---
Write-Host "==> [1/3] verify_installer.py on $Installer"
& $python.Source (Join-Path $scriptDir "verify_installer.py") --installer $Installer
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: verify_installer.py exited $LASTEXITCODE" -ForegroundColor Red
    $failures += 1
} else {
    Write-Host "==> [1/3] PASS"
}

# --- 2. Clean provenance of the built binary ---
Write-Host "==> [2/3] verify-clean-build.ps1 on $BuiltExe"
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scriptDir "verify-clean-build.ps1") -BuiltExe $BuiltExe
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: verify-clean-build.ps1 exited $LASTEXITCODE" -ForegroundColor Red
    $failures += 1
} else {
    Write-Host "==> [2/3] PASS"
}

# --- 3. Baseline-diff gate ---
Write-Host "==> [3/3] verify-baseline.ps1: baseline=$(Split-Path $BaselineSetup -Leaf) candidate=$(Split-Path $Installer -Leaf)"
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scriptDir "verify-baseline.ps1") -BaselineSetup $BaselineSetup -CandidateSetup $Installer
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: verify-baseline.ps1 exited $LASTEXITCODE" -ForegroundColor Red
    $failures += 1
} else {
    Write-Host "==> [3/3] PASS"
}

if ($failures -gt 0) {
    throw "verify-package: $failures of 3 packaging gates FAILED - this installer is NOT verified and must not be delivered"
}
Write-Host "==> verify-package: ALL 3 PACKAGING GATES PASSED. Installer is verified against commit $((& git rev-parse HEAD).Trim())."

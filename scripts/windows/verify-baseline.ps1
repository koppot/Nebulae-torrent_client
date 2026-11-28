# verify-baseline.ps1 — baseline-diff gate for the Windows installer.
#
# Extracts the payload of the accepted baseline installer and of the new
# candidate installer with 7z, then runs scripts/baseline_diff.py on the two
# payload trees by RELATIVE path. Every difference must be classified in
# scripts/baseline-allow/windows.tsv (exact path or a "dir/" prefix entry);
# anything else exits 1. Nothing is installed; extraction goes to a temp dir
# that is removed afterwards.
#
# Usage:
#   powershell -File scripts\windows\verify-baseline.ps1 `
#       -BaselineSetup <accepted-setup.exe> -CandidateSetup <new-setup.exe>
#
# Exit codes: 0 gate pass, non-zero gate fail or tooling error.
param(
    [Parameter(Mandatory = $true)][string]$BaselineSetup,
    [Parameter(Mandatory = $true)][string]$CandidateSetup
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$diff = Join-Path $repoRoot "scripts\baseline_diff.py"
$allow = Join-Path $repoRoot "scripts\baseline-allow\windows.tsv"

foreach ($f in @($BaselineSetup, $CandidateSetup)) {
    if (-not (Test-Path $f)) { throw "installer not found: $f" }
}
foreach ($f in @($diff, $allow)) {
    if (-not (Test-Path $f)) { throw "required file missing: $f" }
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { $python = Get-Command py -ErrorAction SilentlyContinue }
if (-not $python) { throw "python not found on PATH" }

$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
if (-not $sevenZip) {
    $sevenZip = Get-Command "C:\Program Files\7-Zip\7z.exe" -ErrorAction SilentlyContinue
}
if (-not $sevenZip) { throw "7z not found (needed to extract installer payloads)" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("nebulae-baseline-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $baseDir = Join-Path $tmp "baseline"
    $candDir = Join-Path $tmp "candidate"
    New-Item -ItemType Directory -Path $baseDir | Out-Null
    New-Item -ItemType Directory -Path $candDir | Out-Null

    foreach ($pair in @(@($BaselineSetup, $baseDir), @($CandidateSetup, $candDir))) {
        & $sevenZip.Source x $pair[0] ("-o" + $pair[1]) -y | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "7z could not extract $($pair[0])" }
    }

    Write-Host "==> Baseline-diff gate: baseline=$(Split-Path $BaselineSetup -Leaf) candidate=$(Split-Path $CandidateSetup -Leaf)"
    & $python.Source $diff --baseline $baseDir --candidate $candDir --allow $allow
    # baseline_diff.py: exit 3 = differences not classified, 4 = it could not run.
    if ($LASTEXITCODE -eq 3) {
        throw "baseline-diff gate FAILED: differences not classified in scripts/baseline-allow/windows.tsv"
    } elseif ($LASTEXITCODE -ne 0) {
        throw "baseline-diff gate COULD NOT RUN (exit $LASTEXITCODE; see the messages above). This is not a classification result."
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

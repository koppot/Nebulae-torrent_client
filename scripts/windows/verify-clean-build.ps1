# scripts/windows/verify-clean-build.ps1 -BuiltExe <path-to-nebulae.exe>
#
# Windows twin of the macOS/Linux clean-provenance gate. build.rs stamps
# NEBULAE_BUILD_ID="<sha>-clean|dirty" from "git status --porcelain" at
# compile time; a -dirty binary is never delivered (rules page: clean source).
#
# Run AFTER 'pnpm tauri build' and BEFORE delivering anything, from the
# checkout root. Fails (exit 1) unless the embedded build_id matches
# ^<40-hex>-clean$ and equals the checked-out HEAD, and the worktree is
# clean. Installs nothing.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BuiltExe
)
$ErrorActionPreference = "Stop"

if (-not (Test-Path $BuiltExe)) { throw "REFUSED: $BuiltExe does not exist" }
$head = (& git rev-parse HEAD).Trim()
$expected = "$head-clean"

# Extract the embedded build_id: a 40-hex sha followed by -clean or -dirty.
$found = $null
foreach ($line in (& strings.exe -a $BuiltExe 2>$null)) {
    if ($line -match '([0-9a-f]{40})-(clean|dirty)') { $found = $Matches[1] + "-" + $Matches[2]; break }
}
if (-not $found) {
    # strings.exe (Sysinternals) may be absent; fall back to a raw scan.
    $bytes = [System.IO.File]::ReadAllBytes($BuiltExe)
    $text = [System.Text.Encoding]::ASCII.GetString($bytes)
    # Search for the EXPECTED full id first: a generic first match could be
    # a stale string embedded by a dependency. The generic pattern is only a
    # reporting fallback; the equality checks below still decide.
    $m = [regex]::Match($text, [regex]::Escape($expected))
    if ($m.Success) { $found = $expected }
    else {
        $m = [regex]::Match($text, '([0-9a-f]{40})-(clean|dirty)')
        if ($m.Success) { $found = $m.Groups[1].Value + "-" + $m.Groups[2].Value }
    }
}
if (-not $found) { throw "FAIL: could not read the embedded build_id from $BuiltExe" }

Write-Host "==> embedded build_id: $found"
if ($found -eq $expected) {
    Write-Host "==> clean provenance OK (expected $expected)"
}
elseif ($found -eq "$head-dirty") {
    throw ("FAIL: the built binary reports -dirty (expected $expected). A dirty build is never delivered. " +
           "Find the tracked file staging modified ('git status --porcelain') and fix the staging script.")
}
else {
    throw "FAIL: embedded build_id '$found' does not match the checked-out HEAD $head"
}

$dirty = & git status --porcelain
if ($dirty) {
    Write-Host "REFUSED: the worktree is dirty after the build:" -ForegroundColor Red
    $dirty | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    throw "dirty worktree after build"
}
Write-Host "==> worktree clean. Build may be delivered."

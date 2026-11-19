# scripts/build-windows-transmission.ps1
# Builds transmission-daemon.exe 4.1.3 from the official source tarball with
# scripts/patches/transmission-4.1.3-identity-override.patch applied (peer-id
# and user-agent overrides, and the keepalive sent right after 'interested'
# that rtorrent seedboxes need before they unchoke).
#
# Requires: MSVC build tools (vswhere), git, tar, and vcpkg (VCPKG_INSTALLATION_ROOT
# or C:\vcpkg). CMake and Ninja are taken from the Visual Studio installation
# when they are not already on PATH.
#
# Output: <StagingDir>\build\daemon\transmission-daemon.exe
# Exit code is non-zero when the build fails or the result lacks the patch markers.

param(
    [string]$Version = "4.1.3",
    [string]$StagingDir = "src-tauri\transmission-staging"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

function Invoke-Checked([string]$what, [scriptblock]$cmd) {
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$what failed with exit code $LASTEXITCODE" }
}

$RepoRoot = (Get-Location).Path
$StagingAbs = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $StagingDir))
New-Item -ItemType Directory -Force -Path $StagingAbs | Out-Null

# --- Locate MSVC and import its environment ---
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found; install the Visual Studio C++ build tools" }
$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsPath) { throw "no Visual Studio installation with the C++ x64 tools was found" }
$vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
cmd /c "call `"$vcvars`" >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$cmakeDir = Join-Path $vsPath "Common7\IDE\CommonExtensions\Microsoft\CMake"
foreach ($d in @("$cmakeDir\CMake\bin", "$cmakeDir\Ninja")) {
    if (Test-Path $d) { $env:Path = "$d;$env:Path" }
}
foreach ($tool in "cmake", "ninja", "git", "tar") {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool not found on PATH" }
}

$vcpkgRoot = if ($env:VCPKG_INSTALLATION_ROOT) { $env:VCPKG_INSTALLATION_ROOT } else { "C:\vcpkg" }
if (-not (Test-Path (Join-Path $vcpkgRoot "vcpkg.exe"))) { throw "vcpkg not found at $vcpkgRoot" }

# --- Source ---
$tarball = Join-Path $StagingAbs "transmission-$Version.tar.xz"
if (-not (Test-Path $tarball)) {
    Invoke-WebRequest -Uri "https://github.com/transmission/transmission/releases/download/$Version/transmission-$Version.tar.xz" -OutFile $tarball -UseBasicParsing
}
$srcDir = Join-Path $StagingAbs "transmission-$Version"
Remove-Item -Recurse -Force $srcDir -ErrorAction SilentlyContinue
# bsdtar delegates .xz to an external xz that Windows does not ship. Use an
# xz on PATH, the one in Git for Windows, or 7-Zip. Only a single executable is
# used, because Git's usr\bin also holds a link.exe that would shadow the MSVC
# linker if it were put on PATH.
$tarPlain = Join-Path $StagingAbs "transmission-$Version.tar"
Remove-Item -Force $tarPlain -ErrorAction SilentlyContinue
$xzExe = $null
$cmd = Get-Command xz -ErrorAction SilentlyContinue
if ($cmd) { $xzExe = $cmd.Source } else {
    $gitRoot = Split-Path (Split-Path (Get-Command git).Source)
    $candidate = Join-Path $gitRoot "usr\bin\xz.exe"
    if (Test-Path $candidate) { $xzExe = $candidate }
}
if ($xzExe) {
    Invoke-Checked "xz decompress" { & $xzExe -dkf $tarball }
} else {
    $sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
    $sevenZipExe = if ($sevenZip) { $sevenZip.Source } else { Join-Path $env:ProgramFiles "7-Zip\7z.exe" }
    if (-not (Test-Path $sevenZipExe)) { throw "no xz or 7-Zip available to decompress $tarball" }
    Invoke-Checked "7z decompress" { & $sevenZipExe x -y "-o$StagingAbs" $tarball }
}
Invoke-Checked "tar extraction" { tar -xf $tarPlain -C $StagingAbs }
Remove-Item -Force $tarPlain -ErrorAction SilentlyContinue

$patch = Join-Path $RepoRoot "scripts\patches\transmission-$Version-identity-override.patch"
if (-not (Test-Path $patch)) { throw "patch not found: $patch" }
Push-Location $srcDir
try { Invoke-Checked "git apply" { git apply --ignore-whitespace --ignore-space-change $patch } } finally { Pop-Location }
$peerMsgs = Get-Content -Raw (Join-Path $srcDir "libtransmission\peer-msgs.cc")
if ($peerMsgs -notmatch "NEBULAE_INTEREST_KEEPALIVE") { throw "the patch did not apply (keepalive marker missing)" }

# --- Dependencies (OpenSSL and libcurl). Dynamic CRT, because the bundled
# miniupnpc sub-build always uses it. ---
$triplet = "x64-windows-static-md"
Invoke-Checked "vcpkg install" { & (Join-Path $vcpkgRoot "vcpkg.exe") install "curl[ssl]" openssl --triplet $triplet }

# --- Build ---
$buildDir = Join-Path $StagingAbs "build"
Remove-Item -Recurse -Force $buildDir -ErrorAction SilentlyContinue
$toolchain = ($vcpkgRoot -replace '\\', '/') + "/scripts/buildsystems/vcpkg.cmake"
Invoke-Checked "cmake configure" {
    cmake -S $srcDir -B $buildDir -G Ninja -DCMAKE_BUILD_TYPE=Release `
        "-DCMAKE_TOOLCHAIN_FILE=$toolchain" "-DVCPKG_TARGET_TRIPLET=$triplet" `
        -DENABLE_DAEMON=ON -DENABLE_GTK=OFF -DENABLE_QT=OFF -DENABLE_UTILS=OFF -DENABLE_CLI=OFF -DENABLE_TESTS=OFF -DENABLE_MAC=OFF
}
Invoke-Checked "cmake build" { cmake --build $buildDir --target transmission-daemon }

$exe = Join-Path $buildDir "daemon\transmission-daemon.exe"
if (-not (Test-Path $exe)) { throw "build finished without producing $exe" }
$text = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($exe))
foreach ($marker in "TRANSMISSION_PEER_ID_PREFIX", "TRANSMISSION_PEER_ID_SUFFIX", "NEBULAE_INTEREST_KEEPALIVE") {
    if (-not $text.Contains($marker)) { throw "$exe lacks the $marker patch marker" }
}
Write-Host "Patched transmission-daemon.exe built: $exe"

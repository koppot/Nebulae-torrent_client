# Windows 0.2.9 build recipe (Supermicro, user supermicro\administrator)

Every path below was measured read-only on Supermicro on 2026-11-26 unless marked (inferred).
Run the steps in order in one PowerShell window. If any step fails, copy the full
output (file, line, message) to the Task 5 report page and STOP. Do not edit source,
do not retry in a loop, do not diagnose.

Never touch DataDisk_13, V:\, DrivePool or any pool setting. Keep every build folder on C:\.

## 0. Variables
```powershell
$Sha  = "<FULL 40-CHARACTER SHA GIVEN IN THE PROMPT>"
$Root = "C:\nebulae-build-029"
$Src  = "$Root\rc"            # fresh clone; must not exist yet
$Stage = "$Root\stage"        # must not exist yet
```

## 1. Fresh clone, exact SHA
```powershell
git clone --branch release/0.2.9-rc https://github.com/koppot/Nebulae-torrent_client.git $Src
cd $Src
git checkout --detach $Sha
if ((git rev-parse HEAD).Trim() -ne $Sha) { throw "wrong HEAD" }
```

## 2. Inputs that are not in git
- Frozen daemon: source $Root\checkout\inputs\transmission-daemon-patched-0.2.8.exe,
  7,984,128 bytes, SHA-256 26b644d7fb5310ab74a358944f76e6dbe847d224b6df633e20b6ba591bd80795.
- Task 1 manifest CSV: $Root\checkout\task1-manifest-baseline-vs-rev2-rev3.csv, 268,371 bytes.
- Pinned runtime cache: C:\nebulae-cache (VLC zip, Transmission/Tailscale MSI, VC redist, licences,
  GeoLite2 under C:\nebulae-cache\geo, the 21.0.12+8 JRE zip). The 21.0.13+9 JRE zip is NOT there yet;
  fetch_inputs.py downloads it (size and SHA-256 are pinned in inputs.lock.json; a mismatch deletes it and fails).
- Patched VLC overlay: C:\nebulae-factory-hdr\overlay (patched-vlc-manifest.json SHA-256 starts 684583e0).
```powershell
New-Item -ItemType Directory -Force "$Src\inputs" | Out-Null
Copy-Item "$Root\checkout\inputs\transmission-daemon-patched-0.2.8.exe" "$Src\inputs\"
Copy-Item "$Root\checkout\task1-manifest-baseline-vs-rev2-rev3.csv" $Src
(Get-FileHash "$Src\inputs\transmission-daemon-patched-0.2.8.exe" -Algorithm SHA256).Hash.ToLower()   # must be 26b644d7...
python scripts\windows\fetch_inputs.py --cache C:\nebulae-cache --skip-optional
```

## 3. JavaScript dependencies (frozen lockfile)
Use the pnpm form the kit selects (plain pnpm if on PATH, otherwise the offline npx form, measured pnpm 12.18.0):
```powershell
npx --no-install pnpm install --frozen-lockfile --prefer-offline
```
If that fails because the registry is unreachable or pnpm is missing, stop and ask (installing pnpm globally needs user approval).

## 4. Gates (cargo check, clippy, tsc)
```powershell
.\scripts\windows\build-windows.ps1 -CommitSha $Sha -ExpectedVersion 0.2.9 -CargoHome "$Root\cargo-home"
```

## 5. Stage the runtime tree (fail-closed)
```powershell
python scripts\windows\stage_runtime.py --cache C:\nebulae-cache --out $Stage `
  --patched-vlc C:\nebulae-factory-hdr\overlay --require-patched `
  --frozen-daemon "$Src\inputs\transmission-daemon-patched-0.2.8.exe" `
  --geo-dir C:\nebulae-cache\geo --require-geo --offline
```
Default JRE is Temurin 21.0.13+9 (temurin-jre-21013). Do not pass --jre-input. Expected: about 930 files, patched_vlc=True.
Rehearsed on macOS with the same flags: staged 930 files, 415.4 MB, PE audit "every import resolves" (488 PE files);
verify_installer on that tree passes everything except the installer-only rows (nebulae.exe, WebView2 loader and offline installer).

## 6. Copy the staged resources and build the installer
```powershell
robocopy "$Stage\resources" "$Src\src-tauri\resources" /E /NFL /NDL /NJH /NJS   # exit codes 0-7 are success
npx --no-install pnpm tauri build --bundles nsis --config '{"bundle":{"createUpdaterArtifacts":false}}'
```
No Tauri signing key is configured on this machine (measured), so the build is an unsigned test candidate;
the config override keeps tauri.conf.json unmodified. Do not create or import a signing key.
Tauri may download the WebView2 offline installer on this step (offlineInstaller mode); the NSIS tool is already cached.
If the installer step stalls, apply the stall rule in BUILD_FAILURE_MODES.md (capture, do not retry).
The installer is written to src-tauri\target\release\bundle\nsis\.

## 6b. Clean-provenance gate (mandatory before delivery)
```powershell
powershell -File scripts\windows\verify-clean-build.ps1 -BuiltExe "$Src\src-tauri\target\release\nebulae.exe"
```
Required exit code 0. The gate reads the build_id that build.rs stamped into
the binary (`<sha>-clean` or `<sha>-dirty` from `git status` at compile time).
A `-dirty` binary is NEVER delivered — quarantine it and fix the staging
script that modified a tracked file. If it reports a dirty worktree after the
build, that is the same failure; only gitignored paths may change
(`task1-manifest*.csv` is gitignored, so the Task 1 manifest CSV may stay in
the checkout).

## 7. Verify by extraction (installs nothing)
```powershell
$Installer = (Get-ChildItem "$Src\src-tauri\target\release\bundle\nsis\*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
python scripts\windows\verify_installer.py --installer $Installer --manifest "$Src\task1-manifest-baseline-vs-rev2-rev3.csv" --report "$Root\verify-report.json"
```
Required exit code 0. Never edit expected-values.json to make a row pass.

## Allowed downloads, and what is not allowed

Allowed (all hash-gated or fetched by the pinned tool; none of them is an install of software on the machine):
- the Temurin 21.0.13+9 JRE zip into C:\nebulae-cache by fetch_inputs.py (size and SHA-256 pinned in inputs.lock.json);
- project JavaScript dependencies by pnpm from the frozen lockfile (integrity hashes in pnpm-lock.yaml);
- the WebView2 offline installer file and the NSIS tool, fetched by the Tauri bundler during `tauri build` (the NSIS tool is already cached; the WebView2 file is only packaged into the installer, never run);
- Rust crates from the lockfile into C:\nebulae-build-029\cargo-home.
Not allowed without the user's approval: installing or updating any program (including `npm install -g pnpm`, rustup toolchains, Chocolatey packages), Windows Defender exclusions, registry changes, creating or changing services, scheduled tasks, firewall rules, or environment variables outside the PowerShell session. `scripts\bundle-windows-transmission.ps1` adds a Defender exclusion and must NOT be run.

## Not rehearsed

Rehearsed on macOS (measured): `fetch_inputs.py`, `stage_runtime.py` with the flags in step 5, the PE audit and `verify_installer.py` on the staged tree, the kit unit tests.
NOT rehearsed anywhere: the robocopy step, `build-windows.ps1` on this exact checkout, `tauri build` and the WebView2 download, and `verify_installer.py --installer` on a freshly built installer. The first failure may occur in one of those steps. Report it with the full output and stop; do not fix it.

## Note on command lines

Each command above is complete as written. A "." at the end of a sentence in a prompt is punctuation, not an argument: `fetch_inputs.py` has no positional arguments and `... --skip-optional .` fails with "unrecognized arguments".

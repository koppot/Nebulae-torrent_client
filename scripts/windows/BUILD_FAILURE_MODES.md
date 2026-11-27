# BUILD_FAILURE_MODES — Nebulae Windows builds

Every way a past build failed (Tasks 1–2, measured) and the gate that now
stops it. Read alongside `scripts/windows/build-windows.ps1` (preflight +
compile gates) and `scripts/windows/verify_installer.py` (post-build gate).

| # | Past failure (evidence) | Gate that now stops it |
| --- | --- | --- |
| 1 | VLC runtime staged from the 369-file modified tarball only; clean installs missed 224 stock files (Task 1: 950 → 740 entries) | `stage_runtime.py` stages the full stock tree first and fails under 560 stock files; verify gate expects exactly 577 VLC files |
| 2 | rustup default flipped to MSVC 2026-11-25 18:56; rev3 silently built MSVC (Task 1) | `build-windows.ps1` pins `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu` and re-reads `rustup show active-toolchain` (stderr-safe, raw output in the error); verify gate fails on any `vcruntime140*`/`msvcp140*` import in nebulae.exe |
| 3 | rev2/rev3 bundled the STOCK Transmission 4.1.3 MSI daemon (Task 2) | `bundle-windows-transmission.ps1` fail-gate over all 4 patch markers (Latin-1 `.Contains`); frozen-daemon fallback checks SHA 26b644d7…; verify gate re-checks markers + hash |
| 4 | Forbidden 1.3.2/1.4.0 BD-J jars survived stock-zip staging (Task 2 §9) | `stage_runtime.py` filters by basename at ANY depth; verify gate fails on any 1.3.x/1.4.x jar and requires the 1.5.0 jars |
| 5 | Stock bluray plugin could silently survive a missing/failed overlay (Task 2 §9) | `stage_runtime.py` bluray-provenance gate: the staged `liblibbluray_plugin.dll` must record the patched-overlay source or staging fails; verify gate pins its SHA-256 (c5ad85b6…; 0.2.8's ef4956a3 is a documented deviation needing approval) |
| 6 | UHD d3d11 fix could be lost in staging (Task 2 §9) | post-stage `check_d3d11_plugin(dest)` + verify gate pin e1be02cd… (401,920 B) |
| 7 | Weak 10-plugin-DLL gate proved nothing (Task 2 §9) | replaced by exact file count (577), deny-list absence (20 files), pinned SHAs, manifest cross-check |
| 8 | Stale `plugins.dat` plugin cache (open question, Task 3 §9) | stock zip carries none; verify gate fails if `plugins.dat` appears anywhere in the VLC tree |
| 9 | Unpinned frozen inputs (sizes/hashes drifted between builds) | `build-windows.ps1` SHA-256 gate over the repo-frozen baseline extras and the external frozen daemon; `expected-values.json` + verify gate pin every runtime binary |
| 10 | WebView2 not bundled / wrong installer (rev3 shipped no WebView2Loader.dll — measured in the dry run) | verify gate pins WebView2Loader.dll and the offline installer hashes against the 0.2.8 values |
| 11 | Wrong version or wrong commit built | commit gate (HEAD == approved SHA, clean tree) and version gate (tauri.conf.json == expected) |
| 12 | JRE tree drifts from the pinned input (BD-J) | verify gate identifies the staged JRE from `IMPLEMENTOR_VERSION` in `resources/jre/release` and checks file count + `release` + `jvm.dll` against `jre_inputs` in `expected-values.json`; an unidentifiable JRE fails. Default input `temurin-jre` (21.0.12+8); the 0.2.7-era `temurin-jre-21013` (21.0.13+9) is staged only with `--jre-input`/`NEBULAE_JRE_INPUT` (A12) and only on the user's approved JRE-switch decision |
| 13 | Installer stalls at install time (W-10/A8; withdrawn as blocker, cause never reproduced) | No build-side gate can prevent this; the runbook rule is CAPTURE, DO NOT RETRY: re-run the installer once with `/LOG=<path>\install.log` (NSIS logging; if the installer ignores the flag it is not a logging build), and otherwise record the dialog state, the install-directory file count and `nebulae.exe` size, and Task Manager CPU/disk for the installer process. Send the capture to the handoff instead of reinstalling |

Known limits: the verify gate cannot detect a stale frontend `dist/` (the
build script runs `pnpm build` via tauri's beforeBuildCommand; do not package
from a tree with a hand-made dist). `build_rev3.ps1` (C:\nebulae-diag) is
NOT used by this kit and must not be run.

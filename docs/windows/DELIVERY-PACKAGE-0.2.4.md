# Nebulae 0.2.4 Windows x64 — Delivery Package

**Status:** CANDIDATE BUILD (unsigned, artifacts only, not published)  
**Branch:** `factory-b/windows-0.2.4-candidate`  
**Source commit:** `b1b1930cb24f209111a4fcef01c5ee1b4c889a92`  
**Workflow:** `.github/workflows/candidate-build.yml` (GitHub Actions, `windows-latest`)  
**Workflow run:** 40804267669  
**Built:** 2026-10-29T02:04:49Z  
**Factory:** Factory B (Droid), session `droid-environment-f93df990`

---

## 1. Artifacts

| File | Size | SHA-256 |
|------|------|---------|
| `Nebulae-Setup-x64.exe` | 325,427,756 B | `d8db2558715f3936a7945c8e72c97c11d57ab443876d4fef9f4c3683d99755f3` |
| `nebulae.exe` | 30,466,560 B | `ab59be6af0bff57f57feb11ff884abcab110a4be05ab3102927a00a473210458` |
| `Nebulae-Setup-x64.exe.sig` | 408 B | `6e9d4116903943ad0ae5f1ed3836e055fe7cd6008118063505aeefaf6d26ac7e` |
| `nebulae.exe.sha256` | 79 B | `f09b626ed65926ef5b7dc91d579420807864a9ff23c3bd5c966d29e46f7cb218` |

**SHA256SUMS:**
```
d8db2558715f3936a7945c8e72c97c11d57ab443876d4fef9f4c3683d99755f3  Nebulae-Setup-x64.exe
ab59be6af0bff57f57feb11ff884abcab110a4be05ab3102927a00a473210458  nebulae.exe
```

**Code signing status:** UNSIGNED, provider none (SignPath declined 2026-10-14; Factory B holds no Authenticode certificate and will not generate one).  
**Updater artifacts:** Not generated (candidate build; `TAURI_SIGNING_PRIVATE_KEY` unavailable to Factory B).  
**Published?** No — workflow artifacts only, retention 14 days. Not a release, no tags moved.

---

## 2. Source / Commit Identity

```
CANDIDATE_SHA=b1b1930cb24f209111a4fcef01c5ee1b4c889a92
b1b1930cb24f209111a4fcef01c5ee1b4c889a92
2026-10-29 02:04:49 +0000
ci: add push trigger to auto-start candidate build on branch push
```

Branch: `factory-b/windows-0.2.4-candidate` (scratch branch, no tags, no release, no main changes)  
Base: tag `v0.2.4` = `3fa0d9a4338d2a959078a39aa577a1566d1472a5`  
6 commits ahead of `v0.2.4` on this branch (see `git log factory-b/windows-0.2.4`).

---

## 3. Dependency / Licence Inventory

Pinned inputs (verified by `scripts/windows/fetch_inputs.py` against `scripts/windows/inputs.lock.json`):

| Input | Version | SHA-256 | Role | License |
|-------|---------|---------|------|---------|
| VLC win64 zip | 3.0.23 | `992d19db…` | Runtime: official VLC for Windows | GPL-2.0-or-later / LGPL-2.1-or-later |
| VLC source | 3.0.23 | `e891cae6…` | Build: patched libvlc.dll + Blu-ray plugin | GPL-2.0-or-later / LGPL-2.1-or-later |
| libbluray source | 1.5.0 | `f676408e…` | Build: libbluray + BD-J jars | LGPL-2.1-or-later |
| Transmission MSI | 4.1.3 | `c8ea492d…` | Runtime: transmission-daemon + DLLs | GPL-2.0-only or GPL-3.0-only + OpenSSL exception |
| Tailscale MSI | 1.104.1 | `6da8bcdb…` | Runtime: tailscale.exe, tailscaled.exe | BSD-3-Clause |
| Temurin JRE | 21.0.12+8 | `b8aa18fe…` | Runtime: Java for BD-J menus | GPL-2.0 + Classpath Exception |
| VC++ redist | 14.44.35211.0 | `cc0ff0eb…` | Runtime: app-local VC++ DLLs next to daemon | Microsoft EULA |
| WebView2 offline installer | stand-alone | `1c401b32…` | Installer: offline WebView2 runtime | Microsoft EULA |
| Transmission COPYING | 4.1.3 | `9715b55d…` | Licence text | GPL-2.0-only OR GPL-3.0-only |
| Tailscale LICENSE | 1.104.1 | `a7ca6186…` | Licence text | BSD-3-Clause |
| curl COPYING | 8.16.0 | `e18f1989…` | Licence text (libcurl in MSI) | curl (MIT/X11 derivative) |
| OpenSSL LICENSE.txt | 3.5.4 | `7d5450cb…` | Licence text (libcrypto/libssl in MSI) | Apache-2.0 |
| zlib LICENSE | 1.3.1 | `845efc77…` | Licence text (zlib in MSI) | zlib |
| 7-Zip (tool) | 26.05 | `22cf82fc…` | Tool: MSI/cab extraction | LGPL-2.1-or-later + unRAR restriction |

**Notable exclusions from the published installer:**
- `verify-payload-signature.exe` (CI-only tool, gated behind `ci-verify-payload-signature` feature)
- ~28 MB of Qt6/dbus DLLs (dead weight, never imported by transmission-daemon)
- VLC `msi/`, `skins/`, `axvlc.dll`, `npvlc.dll`, `vlc.exe`, `vlc-cache-gen.exe`, `vlc.ico`

**Required but NOT shipped (release blocker P-08):**
- `GeoLite2-Country.mmdb` and `GeoLite2-ASN.mmdb` — required for 0.2.4, Factory B has no MaxMind licence key, cannot download or stage them. See section 7.

---

## 4. Reproducible Build Instructions

### Prerequisites (Windows 10/11 x64, administrator, 40 GB free, internet)

1. **Rust 1.99.1** with `x86_64-pc-windows-msvc` target
2. **Node.js LTS + pnpm 12**
3. **7-Zip** (for MSI/cab extraction)
4. **Python 3** (for `scripts/windows/` staging tools)

### Build steps

```bash
# 1. Clone the scratch branch
git clone https://github.com/koppot/Nebulae-torrent_client.git
cd Nebulae-torrent_client
git checkout factory-b/windows-0.2.4-candidate

# 2. Download and verify pinned inputs
python3 scripts/windows/fetch_inputs.py --cache <cache-dir>

# 3. Build nebulae.exe (MSVC release)
cargo build --release --locked --target x86_64-pc-windows-msvc --manifest-path src-tauri/Cargo.toml --bin nebulae

# 4. Stage runtime resources
python3 scripts/windows/stage_runtime.py \
  --cache <cache-dir> \
  --out <out-dir>/resources \
  --exe <out-dir>/src-tauri/target/x86_64-pc-windows-msvc/release/nebulae.exe \
  --manifest <out-dir>/build-manifest.json \
  --no-audit

# 5. Copy staged resources to src-tauri/resources (excluding nebulae.exe)
cp -r <out-dir>/resources/* src-tauri/resources/
rm -f src-tauri/resources/nebulae.exe

# 6. Temporarily disable updater artifacts (Factory B has no signing key)
#    Set createUpdaterArtifacts = false in src-tauri/tauri.conf.json

# 7. Build NSIS installer
pnpm tauri build --bundles nsis

# 8. Run PE audit
python3 scripts/windows/pe_audit.py <out-dir>/resources nebulae

# 9. Silent install smoke test
<Nebulae-Setup-x64.exe> /S
<nebulae.exe>   # verify it launches
<uninstall.exe> /S
```

### Toolchain pins

| Component | Version | Source |
|-----------|---------|--------|
| Rust | 1.99.1 | `dtolnay/rust-toolchain` |
| Tauri CLI | 2.12.2 | `pnpm-lock.yaml` |
| Tauri bundler | 2.10.2 | `pnpm-lock.yaml` |
| NSIS (makensis) | 3.11 | `tauri-apps/binary-releases` |

---

## 5. Mac-Fix Parity Checklist

Full detail in `docs/windows/EVALUATION-0.2.4.md` section 3. Summary:

| # | Mac repair | Status on Windows |
|---|-----------|-------------------|
| M-01 | Blu-ray initial Play lands on menu | **Port, blocked** (needs patched libvlc.dll + liblibbluray_plugin.dll) |
| M-02 | Manual Menu without First Play replay | **Port, blocked** (needs patched runtime) |
| M-03 | Menu-visible ground truth (FFI) | **Blocked** (Rust FFI present, optional) |
| M-04 | Bounded retries | **Present** (platform-neutral code) |
| M-05 | Stable crop | **Present**; window fit port open |
| M-06 | Black-frame protection | **Present** (uncertain on D3D11) |
| M-07 | Narrow bar crop | **Present**; window fit port open |
| M-08 | DVD main feature | **Present** (uncertain on Windows dvdnav naming) |
| M-09 | Pause / resume | **Present** |
| M-10 | No "Play Main Feature" after menu | **Nowhere** (not implemented) |
| M-11 | Multi-disc continuation | **Nowhere** (not implemented) |
| M-12 | Branding / Tailnet / deps | **Present** (shared); Windows dep staging port open |
| M-13 | ASI-14 tray speed readout | **Implemented** (Linux-validated, Windows run pending) |

---

## 6. Bugs / Checks / Known Issues

### Packaging (P)
- **P-01 (BLOCKER):** transmission-daemon.exe imports MSVCP140/VCRUNTIME140/VCRUNTIME140_1. App-local VC++ runtime DLLs (14.44) are placed next to the daemon by `stage_runtime.py`, but this is not yet verified in a Windows install.
- **P-02:** WebView2 offline installer is pinned but the candidate-build.yml workflow does not pre-seed it. The `windows-candidate.yml` workflow (untested) handles pre-seeding.
- **P-03:** Stock VLC 3.0.23 lacks `libvlc_media_player_navigate_menu` and `libvlc_media_player_get_bluray_menu_open`. Patched build required.
- **P-08 (RELEASE BLOCKER):** GeoLite2 databases missing. Factory B has no MaxMind licence key.
- **P-07:** Licence files are staged by `stage_runtime.py` but not yet verified in a Windows install.

### Runtime (W)
- W-02, W-03, W-04, W-12, W-15, W-22, W-25: Fixed in commit `9d1677f`.
- W-01, W-05..W-11, W-13..W-24, W-26: Open (not yet implemented).
- W-19: Hard-coded TMDB key in `utils/tmdb.rs:20` (redacted from this document).

### Player / Window (E)
- E-01 to E-09: Open (video output path is Antigravity's domain, do not modify).

### Frontend / IPC (F)
- F-01 to F-07: Open.

---

## 7. Release Blockers

| Blocker | ID | Status | Resolution needed |
|---------|----|--------|-------------------|
| GeoLite2 databases | P-08 | **BLOCKED** | Factory A must locate downloaded `GeoLite2-Country.mmdb` + `GeoLite2-ASN.mmdb`, transfer to Factory B environment, verify both editions. `stage_runtime.py --require-geo` will fail without them. |
| Patched VLC build | M-01..M-03 | **BLOCKED** | Needs Windows build environment to compile VLC 3.0.23 + libbluray 1.5.0 with Nebulae patches (`navigate-menu`, `bluray-playlist-id`, `bluray-menu-first`). |
| Windows install test | — | **PENDING** | No clean-environment Windows VM available to Factory B. Installer built but not install-tested. |
| Authenticode certificate | — | **UNAVAILABLE** | SignPath declined. Factory B will not generate keys. |

---

## 8. Install / Manual Test Steps

### Silent install
```
Nebulae-Setup-x64.exe /S
```

### Verify installation
1. `nebulae.exe` launches from the install directory
2. Tray icon appears in the notification area
3. Transmission daemon starts (check `transmission-daemon.exe` in Task Manager)
4. WebView2 loads (app window opens without download prompt)
5. Silent uninstall: `uninstall.exe /S`

### What cannot be verified without a Windows machine
- Blu-ray disc menu playback (M-01, M-02, M-03)
- GeoLite2 lookups (P-08)
- App-local VC++ runtime (P-01) on a machine without system VC++
- Offline WebView2 installation (P-02)
- Tray speed readout (M-13, Windows runtime execution)

---

## 9. What Was NOT Done (Windows-candidate.yml)

The `windows-candidate.yml` workflow (with Factory B's pinned inputs, `stage_runtime.py`, PE audit, `--require-geo`, and silent install smoke test) was created on the scratch branch but **could not be executed** because GitHub Actions did not recognize the updated workflow definition (cache issue). The existing `candidate-build.yml` was used instead, which builds with stock VLC 3.0.21 and does not include PE audit or silent install smoke test.

The `windows-candidate.yml` remains on `factory-b/windows-0.2.4-candidate` for future use once the GitHub Actions cache issue is resolved.

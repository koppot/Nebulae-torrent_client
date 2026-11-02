# Windows 0.2.4 Rollback Baseline & Verified Release Record

## 1. Overview and Executive Statement

This document defines the authoritative, permanent rollback baseline for the **Windows 0.2.4** release of Nebulae.

The installer recorded herein was placed in `C:\Users\administrator\Downloads\Nebulae_0.2.4_x64-setup.exe` on `supermicro.local`, installed from ground zero on a clean Windows target, and tested directly by the user. The user confirmed:

> *"The Windows installer you placed in the Downloads folder on `supermicro.local` is now the approved Windows 0.2.4 release. I have tested it, and everything is working."*

This exact binary has been preserved byte-for-byte in durable, read-only protected storage and published to GitHub release `v0.2.4`. If any future release or candidate exhibits a regression, this verified artifact serves as the authoritative rollback target to be restored directly without rebuilding, repackaging, or recompilation.

---

## 2. Approved Artifact Identities and Checksums

### Primary Installer (`Nebulae_0.2.4_x64-setup.exe` / `Nebulae-Setup-x64.exe`)

| Property | Value |
|---|---|
| **Original Filename** | `Nebulae_0.2.4_x64-setup.exe` |
| **Release Asset Alias** | `Nebulae-Setup-x64.exe` |
| **Exact Size (bytes)** | `322,154,683` bytes |
| **Size (MiB)** | `307.23` MiB |
| **SHA-256 Checksum** | `fb7a2a565903af4e5103307dc1e9272997a94f72a6002c021d6ec13184c2a665` |
| **Packaging Format** | NSIS Unicode (Nullsoft Scriptable Install System) 64-bit |

### Extracted Application Binary (`nebulae.exe`)

| Property | Value |
|---|---|
| **Binary Filename** | `nebulae.exe` |
| **Installed Location** | `C:\Users\administrator\AppData\Local\Nebulae\nebulae.exe` |
| **Exact Size (bytes)** | `56,408,969` bytes |
| **Size (MiB)** | `53.80` MiB |
| **SHA-256 Checksum** | `eb77df6a2597aaa906426ad85b4b61a0022c2bc24c07e2f4b7c8252df9ec265a` |
| **Architecture** | x86_64 PE32+ executable (Windows GUI) |

### Binary Identity Reconciled: Compiler Output (`3CB69FBD…`) vs. NSIS Payload (`EB77DF6A…`)

During candidate verification, two SHA-256 checksums were recorded for `nebulae.exe`:
- `3CB69FBDB0B7595BE0015EBE0617539CEB399B815F2E5812D25DE3CBF188CB0A` (unbundled binary)
- `EB77DF6A2597AAA906426AD85B4B61A0022C2BC24C07E2F4B7C8252DF9EC265A` (packaged installer payload)

A byte-level diff analysis establishes the exact relationship between the two files:
1. **Size Parity**: Both files have the exact same size of `56,408,969` bytes.
2. **Byte Diff Count**: Out of 56,408,969 bytes, exactly **3 bytes** differ (56,408,966 bytes are identical).
3. **Diff Location & Context**:
   - Location: Offset `0x01FD84D8` (byte index 33,391,832)
   - Compiler output (`3CB69FBD...`): `__TAURI_BUNDLE_TYPE_VAR_UNK` (ASCII bytes `0x55, 0x4E, 0x4B` = `UNK`, unbundled/unknown package type)
   - Packaged payload (`EB77DF6A...`): `__TAURI_BUNDLE_TYPE_VAR_NSS` (ASCII bytes `0x4E, 0x53, 0x53` = `NSS`, Nullsoft Scriptable System package type)

**Root Cause**: When `cargo build --release` finishes, Tauri leaves the unbundled marker `__TAURI_BUNDLE_TYPE_VAR_UNK` in the raw binary at `target/release/nebulae.exe`. When Tauri's bundler constructs the NSIS installer (`tauri build`), it copies the binary into the staging bundle and patches those 3 marker bytes to `__TAURI_BUNDLE_TYPE_VAR_NSS` so that the application can query its runtime packaging mechanism.

The code, compilation flags, dependencies, and runtime logic are 100% identical. The authoritative payload delivered by the installer and tested by the user is `EB77DF6A2597AAA906426AD85B4B61A0022C2BC24C07E2F4B7C8252DF9EC265A`.

---

## 3. Source Provenance and Git Identity

- **Repository**: `koppot/Nebulae-torrent_client`
- **Source Commit**: `08176de45d8aa62ff5e13d964319409b5eb0cf45`
- **Branch**: `origin/factory-b/windows-0.2.4-candidate`
- **Commit Subject**: `fix(player): preserve disc type through resolution, route optical images to disc MRLs, and guard menu readiness`
- **Release Tag Policy**: Release tag `v0.2.4` remains anchored at annotated tag `c972dd4fb1591d56373923917bf5550e951e3dba` (`00a8880`). No tag movement was performed.
- **Documentation Commits**: `48b644d`, `8e70429`.

---

## 4. Packaged Runtime Component Manifest

The approved installer bundles a fully integrated, patched libVLC 3.0.23 runtime, Java Runtime Environment, and geolocation databases:

1. **Patched `libvlc.dll`**:
   - Location: `$INSTDIR\libvlc.dll`
   - Size: `185,856` bytes
   - SHA-256: `07eab3d7e7fd0ccdbcd99b9a97d2effa7a6a3885472a7c36011f6da04b80748d`
   - Exports: 319 functions including custom `libvlc_media_player_get_bluray_menu_open`
2. **Patched `libvlccore.dll`**:
   - Location: `$INSTDIR\libvlccore.dll`
   - Size: `1,299,968` bytes
   - SHA-256: `1e98a58a69a03b54cbcf1a2f96b0267756f7e84126c04f982755e14cb75d1945`
3. **Patched `liblibbluray_plugin.dll`**:
   - Location: `$INSTDIR\plugins\access\liblibbluray_plugin.dll`
   - Size: `1,048,576` bytes
   - SHA-256: `cee54342c8f528b74ae8eed207c141e6e3cc433b46de616ddd30bdc04632a4b1`
   - Functionality: Disc menu visibility signaling and startup bypass jump prevention
4. **Java Runtime Environment (JRE 21)**:
   - Location: `$INSTDIR\resources\jre\` (Eclipse Temurin JRE 21.0.9+10 x64)
   - Binary: `$INSTDIR\resources\jre\bin\java.exe`
5. **Geolocation Databases**:
   - `$INSTDIR\resources\geo\GeoLite2-Country.mmdb`
   - `$INSTDIR\resources\geo\GeoLite2-ASN.mmdb`
6. **Build Manifest**:
   - `$INSTDIR\resources\build-manifest.json`

---

## 5. Tauri Updater Continuity & Cryptographic Signatures

The updater signatures were generated using the project secret key (`~/.tauri/nebulae-updater.key`) and verified against the public key declared in `src-tauri/tauri.conf.json`:
- **Public Key**: `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ1MkNBQThCQkJCQzZGNDEKUldSQmI3eTdpNm9zUmFSM2RQOVlHZllGeFM1dHlpNU5JRitYaytjK3BNMTFwMHYrZjBWcXllNVgK`
- **Installer Signature (`Nebulae-Setup-x64.exe.sig`)**:
  ```text
  dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSQmI3eTdpNm9zUlkwbmU0UVBRUHllSGNTQXdNekEwczRRckhJbHI3TjJuZzJzajdySnMveHJHenVrd3E5QVV0UTNqSmZxeDdob1gyMTdaeEJrWGZiZ2dWd3Y2bXh3clFNPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzNjQxMTUyCWZpbGU6TmVidWxhZS1TZXR1cC14NjQuZXhlCXZlcnNpb246MC4yLjQKU01KZm1SVitXc3hXMU45NkNONC81VmZneWorbkFXOCtHcU93Q3pXZnp4bmd3WHhFR3RwejM3dHFUUFJ2RThQTTkwZWxpTHNtanMyTEdHVUhlRFBqQWc9PQo=
  ```
- **Installer Signature (`Nebulae_0.2.4_x64-setup.exe.sig`)**:
  ```text
  dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSQmI3eTdpNm9zUlpTNy9wRnFOYmRuY2JOYmxUZnZBeTBIOWRFWTk4QnhweWk4Skw5cGV4Nk5QVXVDMU0zMEFLTTV6SVFxTEp2NlRiY2tBbS80NVlBckowWlVUaXp5REFzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzNjQxMTUyCWZpbGU6TmVidWxhZV8wLjIuNF94NjQtc2V0dXAuZXhlCXZlcnNpb246MC4yLjQKemY2TW9ETURsUE1KeTZHZ0ErMzNKWk5lMmt4N2N0UlVlYnpVT3dsaW9CUzhSN1JENFdnVkR4bFNQY0RONHlZM0QvdmRMRFpvTmpPTzVaK2daTWtaQUE9PQo=
  ```

---

## 6. Durable Archive Locations & Filesystem Protection

The approved installer is preserved in dedicated local rollback directories. To guard against accidental overwrites or standard deletions, filesystem-level write protections have been explicitly applied:

1. **Target Windows Host (`supermicro.local`)**:
   `C:\nebulae-backup\release-v0.2.4-windows-baseline\Nebulae_0.2.4_x64-setup.exe`
   - Verified SHA-256: `FB7A2A565903AF4E5103307DC1E9272997A94F72A6002C021D6EC13184C2A665`
   - Exact Size: `322,154,683` bytes
   - **Filesystem Protection**: NTFS `ReadOnly` attribute (`attrib +r`). Windows prevents in-place file modifications or standard overwrites unless `attrib -r` is deliberately executed.

2. **Durable Local Storage (macOS Controller)**:
   `/Users/dev/nebulae-recovery-evidence/release-v0.2.4-windows-baseline/Nebulae_0.2.4_x64-setup.exe`
   - Verified SHA-256: `fb7a2a565903af4e5103307dc1e9272997a94f72a6002c021d6ec13184c2a665`
   - Exact Size: `322,154,683` bytes
   - **Filesystem Protection**: POSIX mode `0444` (`-r--r--r--`) plus BSD file flag `uchg` (user immutable flag applied via `chflags uchg`). macOS blocks truncation, deletion, renaming, or modification by any standard user process until `chflags nouchg` is run.

*Note on Immutability Scope*: These archives are protected against ordinary administrative error and automated build script overwrites via OS filesystem attributes; they are local filesystem read-only copies and not stored on hardware WORM (Write Once Read Many) or cloud object-lock vaults.

3. **Published GitHub Release Assets**:
   - https://github.com/koppot/Nebulae-torrent_client/releases/download/v0.2.4/Nebulae_0.2.4_x64-setup.exe
   - https://github.com/koppot/Nebulae-torrent_client/releases/download/v0.2.4/Nebulae-Setup-x64.exe

---

## 7. Rollback Procedure

If a subsequent build or candidate causes a playback or packaging regression, execute the following steps to roll back immediately to this baseline:

### Option A: Direct Local Host Restoration on Supermicro

1. Close any running instances of Nebulae:
   ```cmd
   taskkill /F /IM nebulae.exe
   ```
2. Verify the integrity of the durable archive copy:
   ```powershell
   Get-FileHash -Algorithm SHA256 "C:\nebulae-backup\release-v0.2.4-windows-baseline\Nebulae_0.2.4_x64-setup.exe"
   ```
   Confirm output is:
   `FB7A2A565903AF4E5103307DC1E9272997A94F72A6002C021D6EC13184C2A665`
3. Execute silent or interactive installation:
   ```cmd
   "C:\nebulae-backup\release-v0.2.4-windows-baseline\Nebulae_0.2.4_x64-setup.exe" /S
   ```
4. Verify the installed executable hash:
   ```powershell
   Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\Nebulae\nebulae.exe"
   ```
   Confirm output is:
   `EB77DF6A2597AAA906426AD85B4B61A0022C2BC24C07E2F4B7C8252DF9EC265A`

### Option B: Remote GitHub Asset Restoration

1. Download the baseline installer from GitHub Releases:
   ```cmd
   curl -L -o "%TEMP%\Nebulae_0.2.4_x64-setup.exe" "https://github.com/koppot/Nebulae-torrent_client/releases/download/v0.2.4/Nebulae_0.2.4_x64-setup.exe"
   ```
2. Verify SHA-256 hash against `FB7A2A56...`.
3. Run the installer:
   ```cmd
   "%TEMP%\Nebulae_0.2.4_x64-setup.exe" /S
   ```

---

## 8. Multi-Agent Coordination & Acknowledgment Record

The Windows 0.2.4 recovery completion, hash reconciliation, and rollback baseline have been communicated across the project coordination system:

1. **Notion Coordination**:
   - Checkpoint page `H-P2` published under *Nebulae — Shared Engineering Handoff* ([`40cedf71-48ed-81ad-b3df-fbee737590e0`](https://app.notion.com/p/40cedf7148ed81adb3dffbee737590e0)).
   - Status updated on `A. Current State` ([`406edf71-48ed-81ed-be46-dbc08463d79c`](https://app.notion.com/p/406edf7148ed81edbe46dbc08463d79c)).
   - Acceptance ledger updated on `C. Acceptance Ledger` ([`406edf71-48ed-8105-8540-f359b0231f9d`](https://app.notion.com/p/406edf7148ed81058540f359b0231f9d)).
2. **Linear Task Tracking**:
   - Issue [`ASI-15`](https://linear.app/asiandvdclub/issue/ASI-15/windows-x64-024-evaluation-repair-and-installer-antigravity) updated with full technical resolution and closed as Done.
3. **Decoupled macOS Track**:
   - Factory A retains sole ownership of the macOS release-recovery candidate in `/Users/dev/Documents/Nebulae-macos-0.2.4-recovery`.
   - Windows publication does not authorize macOS release publication, which remains on a separate verification track awaiting explicit user approval.

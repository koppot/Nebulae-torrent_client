# Windows 0.2.4 Rollback Baseline & Verified Release Record

## 1. Overview and Executive Statement

This document defines the authoritative, permanent rollback baseline for the **Windows 0.2.4** release of Nebulae.

The installer recorded herein was placed in `C:\Users\administrator\Downloads\Nebulae_0.2.4_x64-setup.exe` on `supermicro.local`, installed from ground zero on a clean Windows target, and tested directly by the user. The user confirmed:

> *"The Windows installer you placed in the Downloads folder on `supermicro.local` is now the approved Windows 0.2.4 release. I have tested it, and everything is working."*

This exact binary has been preserved byte-for-byte in immutable, durable storage and published to GitHub release `v0.2.4`. If any future release or candidate exhibits a regression, this verified artifact serves as the authoritative rollback target to be restored directly without rebuilding, repackaging, or recompilation.

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

---

## 3. Source Provenance and Git Identity

- **Repository**: `koppot/Nebulae-torrent_client`
- **Source Commit**: `08176de45d8aa62ff5e13d964319409b5eb0cf45`
- **Branch**: `origin/factory-b/windows-0.2.4-candidate`
- **Commit Subject**: `fix(player): preserve disc type through resolution, route optical images to disc MRLs, and guard menu readiness`
- **Release Tag Policy**: Release tag `v0.2.4` remains anchored at annotated tag `c972dd4fb1591d56373923917bf5550e951e3dba` (`00a8880`). No tag movement was performed.
- **Documentation Commit**: `48b644d` (`docs: record final Windows 0.2.4 candidate build and verified test battery results`)

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

## 6. Durable Archive Locations

The approved installer is stored in the following durable locations separate from GitHub release assets:

1. **Target Windows Host (`supermicro.local`)**:
   `C:\nebulae-backup\release-v0.2.4-windows-baseline\Nebulae_0.2.4_x64-setup.exe`
   - Verified SHA-256: `FB7A2A565903AF4E5103307DC1E9272997A94F72A6002C021D6EC13184C2A665`
   - Exact Size: `322,154,683` bytes

2. **Durable Local Storage (macOS Controller)**:
   `/Users/dev/nebulae-recovery-evidence/release-v0.2.4-windows-baseline/Nebulae_0.2.4_x64-setup.exe`
   - Verified SHA-256: `fb7a2a565903af4e5103307dc1e9272997a94f72a6002c021d6ec13184c2a665`
   - Exact Size: `322,154,683` bytes

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

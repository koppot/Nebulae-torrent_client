# Windows UHD HDR-to-SDR Recovery Runbook

Accepted Factory fix for washed-out UHD/HDR playback in Nebulae on Windows.
This document is the canonical recovery reference. It is written so another
agent can identify, rebuild, verify, and integrate the accepted implementation
without re-running the original investigation.

## Status

User-accepted. The user tested the delivered candidate on `supermicro.local`
and confirmed the solution works (Mary UHD playback now matches macOS
appearance). This acceptance covers the delivered installer only; it does not
cover hardware HLG, native HDR passthrough, the full app UI over RDP, audible
audio, or BD-J titles — those were not exercised and are listed as limitations
below.

## Accepted baseline (identity)

| Item | Value |
| --- | --- |
| Repository | https://github.com/koppot/Nebulae-torrent_client.git |
| Branch | `factory/windows-uhd-hdr-sdr` |
| Base revision | `85a91988af6cb8ff036e375867ac664df5a49524` |
| Implementation commit | `e5f239fea6dab43e6668bb08e6129267364f0dd0` |
| Documentation/manifest/test commit | `daf189a4ffa944bbb3cf7e50823ebef35e99cbbe` |
| Recovery tag | `factory-hdr-accepted-baseline-2026-11-07` (annotated; points at the documentation/manifest revision, a child of `e5f239f`) |
| Pull request | integration PR against `main` (see §Preservation report link after creation) |
| Delivered installer | `Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe` |
| Installer location | `C:\Users\administrator\Downloads` on `supermicro.local` |
| Installer SHA-256 | `FB00C1A1904E641C937963B77D954B83FF19A3AB3D5DEC792900C9031EFFCEB0` |
| Installer size | 322,462,423 bytes |
| Checksum sidecar | `Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe.sha256` |
| Signature sidecar | `Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe.sig` (Minisign/Ed25519) |
| Updater public key id | `452CAA8BBBBC6F41` (compiled into the app; verification via `verify-payload-signature.exe`) |
| Extracted payload `nebulae.exe` SHA-256 | `0C798C821B846CE9B9D4176EE222CAE17F20C228B8EA1D76F98ABF40D88DDE32` |
| Patched D3D11 plugin | `plugins/video_output/libdirect3d11_plugin.dll` |
| D3D11 plugin SHA-256 | `E1BE02CD0DCA2F515F7F67B12CF0F5E425D762D3051506CA2E744E5DA0DD9993` |
| D3D11 plugin size | 401,920 bytes |
| Stock D3D11 plugin SHA-256 | `C1BA837963B15835645AB157DC8EFB63E1083D211F4D74010237A3E87D7F0CE8` |
| Stock D3D11 plugin size | 408,472 bytes |
| Shipped Blu-ray plugin | `plugins/access/liblibbluray_plugin.dll`, SHA-256 `C5AD85B60E47C328997337EE7F4D62BA870957624F4EEB890EA52796B6E3A3FF` |

The installer filename embeds `e5f239f` because it was built from the `e5f239f`
source tree plus the uncommitted regenerated manifest at build time. The
complete, reviewed revision is `daf189a` (and its recovery-tag child), which
adds this runbook, the regression check, and the regenerated manifest. The tag
is therefore the revision to check out for recovery; `e5f239f` is the
implementation-only ancestor.

## What the fix does

Nebulae passes no `--vout` on Windows, so VLC 3.0.23 uses Direct3D 11. On an
SDR swapchain, stock D3D11 converts PQ/HLG with a Hable operator scaled by
`100/10000` followed by gamma 1/2.2, which renders mid-tones roughly 40%
darker than a colorimetric conversion. The `--tone-mapping*` arguments and
`apply_edr_to_video_layer` are read only by the OpenGL vout, so they were inert
on Windows; the shipped 0.2.5 D3D11 plugin is byte-identical to stock VLC.

The fix replaces the stock HDR-to-SDR shader in the Windows-only Direct3D 11
plugin (`patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch`):

- PQ: inverse EOTF to absolute light, then ITU-R BT.2390-10 EETF on
  max(R,G,B), with the source peak (MaxCLL, else mastering max display
  luminance, else 1000 nit) mapped to SDR white = 100 cd/m2.
- HLG: inverse OETF / 12, then OOTF with gamma 1.2 at 1000 nit, then the same
  BT.2390 mapper.
- BT.2020 to BT.709 with luma-preserving desaturation of out-of-gamut
  negatives.
- No gain, no clip, no brightness/contrast/saturation/gamma knobs.
- SDR sources and the native-HDR swapchain path are untouched (bit-identical to
  stock).

Also fixed: an upstream VLC 3.0.23 bug in `COLORSPACE_BT2020_YUV_TO_FULL_RGBA`
where the green row multiplied by Kg twice (`-0.127007098661, -0.440987687946`
→ `-0.187326104219f, -0.650424318505f`), which mis-decoded saturated BT.2020
colors in both the tone-mapped and native-HDR paths.

## Rendering paths affected

- Affected: Windows Direct3D 11 video output, HDR (PQ/HLG) sources on an SDR
  swapchain.
- Not affected (unchanged, stock behavior): SDR sources; native-HDR swapchain
  output on HDR-capable displays; macOS OpenGL path (the `--tone-mapping*`
  args are now macOS-only there).
- `d3d11-hdr-mode` only chooses native vs. SDR output by display capability; it
  does not select the SDR tone mapper, so setting it does not fix the issue.

## Build requirements

- VLC 3.0.23 release tarball, SHA-256
  `e891cae6aa3ccda69bf94173d5105cbc55c7a7d9b1d21b9b21666e69eff3e7e0`.
- Patch: `patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch` (applies to the five
  files under `modules/video_output/win32/`; identities in
  `d3d11-patched-sources.sha256`).
- MinGW-w64 cross toolchain, e.g. `x86_64-w64-mingw32-gcc` 16.2.0.
- AMF headers (pinned 1.4.34; see `d3d11-plugin.lock.json`) so the AMF scaler
  object is built with parity to the official plugin.
- Configure with `HAVE_ID3D11VIDEODECODER=1`; without it the software-decode
  staging-texture path is compiled out and `Prepare()` crashes on 10-bit
  software frames.
- Build script: `scripts/windows/build-d3d11-plugin.sh --work DIR --out DIR
  --vlc-zip ZIP --vlc-tarball TARBALL [--amf-headers TGZ]`. It builds only
  `libdirect3d11_plugin.dll` against the stock `libvlccore.dll` from the
  official VLC win64 zip and is bit-reproducible across work directories.

## Retrieving the accepted source and artifacts

Source:

```
git clone https://github.com/koppot/Nebulae-torrent_client.git
cd Nebulae-torrent_client
git fetch origin factory/windows-uhd-hdr-sdr
git checkout factory-hdr-accepted-baseline-2026-11-07   # complete revision
# or: git checkout e5f239fea6dab43e6668bb08e6129267364f0dd0  # implementation only
```

The tag is annotated and points at `daf189a`; the tag message records how it
relates to the installer's `e5f239f` filename.

Artifacts (durable preservation location — see the preservation report for the
exact path; the installer was originally delivered to
`C:\Users\administrator\Downloads` on `supermicro.local`):

- `Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe`
- `...setup.exe.sha256`
- `...setup.exe.sig`

Do not rely on the Downloads folder alone; it is not backed up by the project.

## Rebuilding and staging the compatible complete runtime

1. Check out the tag. Build the patched plugin:

```
scripts/windows/build-d3d11-plugin.sh --work /tmp/hdr-work --out /tmp/hdr-out \
  --vlc-zip <path>/vlc-3.0.23-win64.zip \
  --vlc-tarball <path>/vlc-3.0.23.tar.xz \
  --amf-headers <path>/AMF-1.4.34.tar.gz
```

2. Verify the result against `scripts/windows/d3d11-plugin.lock.json`:

```
python3 scripts/windows/regression-check-d3d11.py /tmp/hdr-out/libdirect3d11_plugin.dll
```

Expected: SHA-256 `E1BE02CD…9993`, 401,920 bytes, PE machine x64, marker
`HDR to SDR tone mapping:` present, stock SHA-256 `C1BA8379…0CE8` differs.

3. Stage the runtime on the Windows build host (read-only for the cache; never
touch `C:\nebulae-build` or shared Antigravity staging):

```
python3 scripts/windows/stage_runtime.py \
  --cache C:\nebulae-cache \
  --out C:\nebulae-factory-hdr\stage \
  --offline \
  --patched-vlc <complete modified VLC tree> \
  --require-patched \
  --geo-dir C:\nebulae-cache\geo \
  --require-geo \
  --patches-dir <repo>/patches/vlc-3.0.23
```

The complete modified VLC tree must include `libvlc.dll`, `libvlccore.dll`,
`plugins/`, and `patched-vlc-manifest.json`; a stock-VLC substitution or a
partial overlay lacking `libvlccore.dll` is rejected. `stage_runtime.py`
rejects the stock D3D11 plugin, any non-x64 build, any build lacking the tone
mapping marker, and any build whose SHA-256 differs from the pinned lock.

4. Copy `stage/resources` into the isolated repo's `src-tauri/resources`
(avoid `resources/resources` nesting), apply the Tauri config override
(`beforeBuildCommand: ""`, `createUpdaterArtifacts: false` for a candidate),
then `pnpm tauri build --bundles nsis`.

## Verifying the delivered installer

1. Confirm the three sidecar files exist next to the installer.
2. `Get-FileHash installer.exe -Algorithm SHA256` must equal
   `FB00C1A1904E641C937963B77D954B83FF19A3AB3D5DEC792900C9031EFFCEB0` and
   must equal the contents of the `.sha256` sidecar.
3. Verify the Minisign signature:

```
verify-payload-signature.exe Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe.sig
```

Expected: "CRYPTOGRAPHIC MINISIGN VERIFICATION: 100% SUCCESSFUL", Ed25519
Payload Signature VALID, Minisign Global Signature VALID, trusted comment
naming the file. A tampered copy must fail.

4. Extract with 7-Zip and compare `resources/vlc/plugins/video_output/libdirect3d11_plugin.dll`
   SHA-256 against `E1BE02CD…9993`; the rest of the payload should be
   byte-identical to the shipped 0.2.5 payload except `build-manifest.json` and
   the patches README/license copies.

## Applying the fix to a newer development revision

1. Rebase or cherry-pick `e5f239f` (+ `daf189a` for docs/manifest/tests) onto
   the target revision. Do not revert unrelated work.
2. Regenerate `src-tauri/resources/build-manifest.json` for the new runtime via
   `stage_runtime.py`; do not blindly copy the 0.2.5-era manifest over a newer
   one — coordinate with Antigravity on shared files, especially
   `build-manifest.json`.
3. Rebuild the D3D11 plugin against the target VLC 3.0.23 sources; if the
   patch context changed, regenerate the patch with `diff -ruN` and re-verify
   the five file identities in `d3d11-patched-sources.sha256`.
4. Re-run `stage_runtime.py --require-patched` and the regression check, then
   re-run the playback validation (see §Regression checks and §Manual checks).
5. A combined candidate (this fix + Antigravity's subsequent changes) is a
   different build and must be verified separately; acceptance of the
   standalone installer does not imply acceptance of an integrated build.

## Detecting recurrence

Symptom: UHD/HDR Blu-ray playback looks dim, flat, or washed out on Windows
SDR displays while SDR content looks normal.

Detection order:

1. Confirm the running `libdirect3d11_plugin.dll` SHA-256 is
   `E1BE02CD…9993`. If it is `C1BA8379…0CE8` (stock), the fix was silently
   replaced — `stage_runtime.py --require-patched` would have rejected this at
   staging time.
2. Confirm `d3d11-hdr-mode` was not changed to rely on the option; the option
   only selects native vs. SDR output and does not fix SDR appearance.
3. Distinguish renderer problems from display/capture/overlay differences:
   - Renderer: every frame dark/flat regardless of scene; correct in
     screen-capture of the video child window; colorimetric luma median off by
     ~40% against the BT.2390 reference.
   - Display: only the physical monitor looks wrong; captures look correct.
   - Capture: PrintWindow/offscreen capture looks correct but photos of the
     panel look wrong; check RDP encoding, color management, and GPU override.
   - Overlay: the Nebulae webview/controls overlay looks wrong but fullscreen
     video is correct; the fix does not touch the webview.

Reference acceptance values (from validation of the delivered installer,
software decode, SDR display, RDP session):

- Synthetic PQ 24 patches vs numpy BT.2390 reference: max error 2.75 / 255
  codes; monotonic, 0.2 nit visible (code 15), no clip.
- Synthetic HLG 24 patches vs reference: max error 1.63 / 255 codes.
- SDR regression: synthetic SDR and Fuuka SDR captures bit-identical to
  stock/0.2.5.
- Mary UHD, 5 scenes vs reference: mean abs error 0.8–2.0 codes; luma p50
  0.610 / 0.625 / 0.144 / 0.238 / 0.672 (shipped 0.2.5: 0.386 / 0.389 / 0.092 /
  0.153 / 0.421); highlights compressed, not clipped; letterbox black
  unchanged.

## Rolling back a failed candidate

- Never uninstall over production media or databases; the candidate never
  modifies `nebulae.db` or media files.
- To remove the candidate: uninstall Nebulae via Windows Settings/Add Remove
  Programs, then reinstall the previous production installer
  (`Nebulae_0.2.5_x64-setup.exe`, SHA-256 `3C2A8DDB6EA4AEDC8F7B215BE93A5066DBAA0557A661FF3FBB3F977D02042E1C`)
  if a rollback build is needed. The installed app and its `resources/` tree are
  self-contained under `%LOCALAPPDATA%\Nebulae`.
- Do not delete `C:\nebulae-cache` or the shared staging; they hold the
  pinned inputs needed to rebuild.

## Known provenance caveats

1. The shipped Blu-ray plugin
   (`plugins/access/liblibbluray_plugin.dll`, SHA-256
   `C5AD85B60E47C328997337EE7F4D62BA870957624F4EEB890EA52796B6E3A3FF`)
   does not match `HEAD`'s `bluray-menu-first.patch` plus
   `patched-sources.sha256` (the `bluray.c` hash in that file is inconsistent
   with pristine VLC 3.0.23 plus patches). This predates the HDR fix. The
   accepted candidate reuses the shipped binary unchanged. Do not silently
   substitute a freshly rebuilt Blu-ray plugin while archiving this solution;
   reproducing that component requires the exact binary plus the build
   evidence noted above.
2. The accepted candidate is based on `85a9198` and does not automatically
   include Antigravity's subsequent changes. Integration into `main` is tracked
   separately (pull request), and any combined candidate must be verified
   independently.

## Limitations (not covered by user acceptance)

- Hardware HLG and native-HDR output: no HDR display or GPU on the validation
  host; untested on hardware.
- Hardware decode (d3d11va/dxva2): unavailable on the host, same as stock.
- Full Nebulae app UI (controls visible/hidden, manual resize, webview over
  RDP): the app is single-instance on the production profile and playback was
  active; not launched.
- Audible audio: no audio endpoint in the RDP session; audio decode and track
  switching were exercised, sound was not.
- BD-J titles: the available test disc is HDMV-only; the JRE loads but no BD-J
  title was run.
- Rust unit tests on Windows: the build compiled; unit tests were not run.

## Regression checks added

- `scripts/windows/regression-check-d3d11.py`: rejects the stock D3D11 plugin
  SHA-256, requires the pinned lock SHA-256/bytes/x64/marker for a built
  plugin, and records expected reference tolerances.
- `scripts/windows/tests/test_stage_runtime.py` (unchanged suite, 60 tests):
  `test_rejects_stock_d3d11_plugin`, `test_rejects_d3d11_plugin_without_tone_mapping`,
  `test_rejects_unpinned_d3d11_plugin` enforce runtime identity at staging time.
- `stage_runtime.py --require-precated` rejects a modified VLC tree whose
  `libdirect3d11_plugin.dll` is stock, non-x64, marker-free, or hash-mismatched
  against `d3d11-plugin.lock.json`.
- Synthetic PQ/HLG reference cases and SDR-invariance captures are in
  `scripts/windows/vlc-d3d11/test-only/analysis/` (redistributable synthetic
  material only; no copyrighted clips).

## Related documents

- `docs/windows/MODIFIED-VLC-RUNTIME-SPEC-0.2.4.md` §5 (corrected HDR plan)
- `docs/windows/UHD-HDR-SDR-VALIDATION.md` (cause, fix, validation numbers)
- `patches/vlc-3.0.23/README.md` (patch application notes)
- `scripts/windows/d3d11-plugin.lock.json` (pinned plugin identity)
- `scripts/windows/build-d3d11-plugin.sh` (reproducible build recipe)

# Windows UHD HDR to SDR: Cause, Fix, Validation

Owner: Factory (Windows UHD color/rendering). Branch `factory/windows-uhd-hdr-sdr`, base `85a9198`.

## Cause

Nebulae on Windows passes no `--vout`, so VLC 3.0.23 uses Direct3D 11. On an SDR
swapchain, stock D3D11 converts PQ/HLG with a Hable operator scaled by
`100/10000` and then gamma 1/2.2. Mid-tones come out about 40% darker than a
colorimetric conversion, which reads as dim and washed out. The `--tone-mapping*`
arguments and `apply_edr_to_video_layer` only affect the OpenGL vout, which is
macOS-only in practice, so they did nothing on Windows. Separately, the stock
BT.2020 YCbCr to RGB matrix has a doubled Kg factor in the green row.

## Fix

`patches/vlc-3.0.23/d3d11-hdr-sdr-tonemap.patch`, built into
`plugins/video_output/libdirect3d11_plugin.dll` by
`scripts/windows/build-d3d11-plugin.sh` (pinned in
`scripts/windows/d3d11-plugin.lock.json`, enforced by `stage_runtime.py`).

Reference transform (also implemented independently in numpy for testing):

1. Decode YCbCr (BT.2020 non-constant, limited range) to R'G'B'.
2. PQ: inverse EOTF to absolute light (cd/m2). HLG: inverse OETF / 12, then OOTF
   with gamma 1.2 at 1000 nit.
3. Source peak = MaxCLL, else mastering max, else 1000 nit (HLG: 1000).
4. BT.2390-10 EETF on max(R,G,B), source peak mapped to SDR white = 100 cd/m2.
   No gain, no clip, no brightness/contrast/saturation/gamma knobs.
5. BT.2020 to BT.709, negatives desaturated toward luma.
6. Encode with the SDR swapchain path (Rec.709, gamma 2.2).

Native HDR swapchain output and all SDR paths are untouched.
Appearance choice: a colorimetric 100 nit reference-white rendering with a smooth
highlight roll-off. It deliberately does not copy the macOS EDR gain.

## Tests on the test host (supermicro, software decode, SDR, no GPU)

Method: a test-only build of the same plugin writes the final RGB10A2 backbuffer
before Present (`scripts/windows/vlc-d3d11/test-only`). Analysis scripts are in
`test-only/analysis`. The production plugin differs from this build only by the
dump patch.

| Test | Result |
| --- | --- |
| Synthetic PQ, 24 patches (16 grays 0.2-1000 nit, 8 colors) vs numpy reference | max error 2.75 / 255 codes; monotonic, 0.2 nit visible (code 15), no clip |
| Same, stock plugin | matches its own Hable prediction (100 nit gray: 147 vs reference 216) |
| Synthetic HLG, 24 patches vs numpy reference | max error 1.63 codes |
| SDR regression (synthetic SDR, Fuuka SDR frames) | bit-identical patched vs stock |
| Mary UHD, 5 scenes (308, 1200, 3600, 3900, 5100 s), luma median stock / patched / reference / colorimetric | 0.321/0.499/0.502/0.505; 0.390/0.626/0.631/0.635; 0.093/0.144/0.145/0.145; 0.153/0.237/0.238/0.238; 0.422/0.674/0.678/0.699 |
| Mary vs reference, mean abs error | 0.8-1.8 codes; highlights compressed (p99 about 0.84), letterbox black unchanged |
| Rebuilt pristine plugin vs stock plugin captures | identical |
| Reproducibility | plugin rebuilt in a separate work dir is bit-identical |

## Limits

- Hardware decode (d3d11va/dxva2) is unavailable on the host. Software decode is
  the path tested, and it is also what stock VLC uses there.
- No HDR display or GPU is available. Native HDR passthrough is stock behavior and
  is not verified on hardware. HLG is verified only with synthetic patches.
- The dump captures the backbuffer at 960x540; RDP remote display encoding was not
  captured, only the playback pipeline on the RDP-hosted session.
- Pre-existing: `patches/vlc-3.0.23/patched-sources.sha256` lists a `bluray.c`
  hash that does not match pristine plus patches at `85a9198`. Not changed here.

## Candidate build validation (installer payload, not the test-only plugin)

Candidate: `Nebulae_0.2.5-factory-hdr-e5f239f_x64-setup.exe` (built from `e5f239f` with the regenerated
`src-tauri/resources/build-manifest.json`). The installer was extracted with 7-Zip and its
`resources\vlc` and `resources\jre` trees were used directly as the runtime under test. Payload
comparison: identical to the staged tree (plus the repository `.gitkeep` placeholders); against the shipped
`Nebulae_0.2.5_x64-setup.exe` payload only `libdirect3d11_plugin.dll`, `build-manifest.json` and the
patches `README.md` (plus the new patch license copy) differ. The Blu-ray plugin and all other binaries are
byte-identical to 0.2.5.

| Test (PrintWindow capture of the real vout, 960x540, 8 bit) | Result |
| --- | --- |
| Mary UHD, 5 scenes, candidate vs numpy reference | mean abs error 0.8-2.0 codes, median 0.6-1.6; luma p50 0.610/0.625/0.144/0.238/0.672 vs reference 0.612/0.630/0.145/0.239/0.677 |
| Same scenes, shipped 0.2.5 payload vs candidate, luma p50 | 0.386 to 0.610, 0.389 to 0.625, 0.092 to 0.144, 0.153 to 0.238, 0.421 to 0.672 |
| SDR regression (Fuuka SDR clip, synthetic SDR), candidate vs 0.2.5 payload | bit-identical captures |
| Blu-ray main feature (91 Days D1, 1080p, TrueHD, folder read-only): play, chapter jump, seek to 50%, pause/resume, audio track switch, subtitle on | all pass |
| Blu-ray menu mode (`--bluray-menu`) with the bundled JRE: `navigate_menu` opens the menu (`menu_open=1`) | pass, same behavior as the 0.2.5 payload (this disc plays its warning reel first in both) |
| Fullscreen / windowed toggled twice during Mary UHD playback | stays in the playing state, tone mapping line logged, no new errors |

Not tested, and why:

- The full Nebulae app UI (controls visible/hidden over the video, window resize by hand, RDP
  rendering of the webview): the app is single-instance and shares the production profile, and playback
  was active, so it was not launched. The tone-mapping change is inside the video child window and does not
  touch the webview or the controls.
- Audible audio: the RDP session has no audio endpoint (`failed to create audio output`), so audio decode and
  track switching were verified but sound was not.
- HLG and native HDR output on hardware: no HDR display or GPU.
- Hardware decode: unavailable on this host (the same for stock VLC).
- BD-J disc: the available disc is HDMV-only (`BD-J Titles: 0`), so the JRE loads but no BD-J title was run.
- The Windows Rust build compiled (`pnpm tauri build`, host `x86_64-pc-windows-gnu` toolchain with the MinGW
  UCRT runtime, the same route as the shipped 0.2.5), but the Rust unit tests were not run on Windows.

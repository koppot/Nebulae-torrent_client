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

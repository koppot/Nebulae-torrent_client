"""Compare a PrintWindow capture (BGRA 960x540, 8-bit, post-Present) of the candidate runtime with the
independent numpy BT.2390 reference. usage: analyze_capture.py CAPTURE.bgra CLIP.mkv PEAK_NITS [FRAMES]"""
import os, sys, numpy as np
from PIL import Image
src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'analyze_mary.py')).read().split("tag=sys.argv[1]")[0]
exec(src)
cap, clip, peak = sys.argv[1], sys.argv[2], float(sys.argv[3])
nfr = int(sys.argv[4]) if len(sys.argv) > 4 else 90
b = np.fromfile(cap, dtype=np.uint8).reshape(540, 960, 4)
img = b[..., [2, 1, 0]].astype(float) / 255
fr = ref_frames(clip, nfr)
best = None
for i, (Y, U, V) in enumerate(fr):
    r = np.power(np.clip(hdr2sdr(to_lin2020(Y, U, V) @ M.T, peak), 0, 1), 1 / 2.2)
    d = np.abs(r[::4, ::4] - img[::4, ::4]).mean()
    if best is None or d < best[0]: best = (d, i, r, to_lin2020(Y, U, V) @ M.T)
d, i, ref, lin = best
col = np.power(np.clip(lin / 100, 0, 1), 1 / 2.2)
e = 255 * np.abs(ref - img)
print(f"{cap.split('/')[-2]}: src frame #{i}; mean|cand-ref| {e.mean():.2f} codes, median {np.median(e):.2f}, p99 {np.percentile(e,99):.1f}, max {e.max():.0f}")
print('  ', stat('candidate', img)); print('  ', stat('reference', ref)); print('  ', stat('colorimetric', col))
Image.fromarray((img * 255).astype(np.uint8)).save(cap.replace('capture.bgra', 'cand.png'))

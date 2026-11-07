import numpy as np, sys
from PIL import Image
def ld(t): return np.fromfile(f'cap/{t}/capture.bgra',dtype=np.uint8).reshape(540,960,4)[...,[2,1,0]].astype(float)/255
L=lambda x: x@np.array([.2126,.7152,.0722])
for s in ('fuuka_sdr','synth_sdr'):
    a,b=ld('cand_'+s),ld('old_'+s); print(f'{s}: candidate vs 0.2.5 identical={np.array_equal(a,b)} max diff {255*np.abs(a-b).max():.0f} codes')
for t in ('m308','m1200','m3600','m3900','m5100'):
    a,b=ld('cand_'+t),ld('old_'+t); print(f'{t}: p50 luma 0.2.5 {np.percentile(L(b),50):.3f} -> candidate {np.percentile(L(a),50):.3f}; p99 {np.percentile(L(b),99):.3f} -> {np.percentile(L(a),99):.3f}')
sheet=np.concatenate([np.concatenate([ld('old_'+t),ld('cand_'+t)],1) for t in ('m308','m3600','m5100')],0)
Image.fromarray((sheet*255).astype(np.uint8)).resize((1440,1215)).save('fr/candidate_vs_025.png')

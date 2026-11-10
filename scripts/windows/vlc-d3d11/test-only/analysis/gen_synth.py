import numpy as np, subprocess, sys, json
W,H=1920,1080; COLS,ROWS=6,4; PW,PH=W//COLS,H//ROWS
m1=2610/16384;m2=2523/4096*128;c1=3424/4096;c2=2413/4096*32;c3=2392/4096*32
def pq(L):
    y=np.power(np.maximum(L,0)/10000,m1);return np.power((c1+c2*y)/(1+c3*y),m2)
def hlg_oetf(E):
    a,b,c=0.17883277,0.28466892,0.55991073
    return np.where(E<=1/12,np.sqrt(3*np.maximum(E,0)),a*np.log(np.maximum(12*E-b,1e-9))+c)
M709to2020=np.array([[0.6274039,0.3292830,0.0433131],[0.0690973,0.9195404,0.0113623],[0.0163914,0.0880133,0.8955953]])
def layout(patches):
    img=np.zeros((H,W,3))
    for i,p in enumerate(patches):
        r,c=divmod(i,COLS); img[r*PH:(r+1)*PH,c*PW:(c+1)*PW]=p
    return img
def ycc10(rgbp,kr=0.2627,kb=0.0593):
    kg=1-kr-kb
    Y=kr*rgbp[...,0]+kg*rgbp[...,1]+kb*rgbp[...,2]
    Cb=(rgbp[...,2]-Y)/(2*(1-kb)); Cr=(rgbp[...,0]-Y)/(2*(1-kr))
    Yq=np.round(64+876*Y); Cbq=np.round(512+896*Cb); Crq=np.round(512+896*Cr)
    return Yq.astype('<u2'),Cbq.astype('<u2'),Crq.astype('<u2')
def write_yuv(rgbp,path,frames=75,bits10=True):
    Y,Cb,Cr=ycc10(rgbp)
    Cb=Cb.reshape(H//2,2,W//2,2).mean((1,3)).round().astype('<u2'); Cr=Cr.reshape(H//2,2,W//2,2).mean((1,3)).round().astype('<u2')
    with open(path,'wb') as f:
        for _ in range(frames): f.write(Y.tobytes()+Cb.tobytes()+Cr.tobytes())
# PQ
grays=[0.2,1,3,6,10,18,26,40,60,80,100,150,203,400,711,1000]
P=[np.array([g]*3) for g in grays]
sk=M709to2020@np.array([0.8,0.5,0.4])*60
cols=[np.array([40,0,0.]),np.array([0,80,0.]),np.array([0,0,20.]),np.array([0,60,60.]),np.array([40,0,40.]),np.array([80,80,0.]),sk,np.array([400,100,0.])]
P+=cols
rgb_nits=layout(P)
write_yuv(pq(rgb_nits),'synth_pq.yuv')
json.dump({'grays':grays,'colors':[c.tolist() for c in cols],'cols':COLS,'rows':ROWS,'W':W,'H':H},open('synth_pq.json','w'))
# HLG: signal values
sig=[0.05,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.75,0.8,0.9,1.0]
P=[np.array([s]*3) for s in sig]
P+= [np.array([s,0.0,0.0]) for s in (0.4,0.6,0.8)]+[np.array([0.0,s,0.0]) for s in (0.4,0.6,0.8)]+[np.array([0,0,s]) for s in (0.4,0.6,0.8)]
P=P[:24]
write_yuv(layout(P),'synth_hlg.yuv')
json.dump({'signals':sig,'cols':COLS,'rows':ROWS,'W':W,'H':H},open('synth_hlg.json','w'))

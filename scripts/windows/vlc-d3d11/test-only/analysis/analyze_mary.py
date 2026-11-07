import numpy as np, subprocess, sys
from PIL import Image
m1=2610/16384;m2=2523/4096*128;c1=3424/4096;c2=2413/4096*32;c3=2392/4096*32
def eotf(e):
    p=np.power(np.maximum(e,0),1/m2);return 10000*np.power(np.maximum(p-c1,0)/(c2-c3*p),1/m1)
def pq(L):
    y=np.power(np.maximum(L,0)/10000,m1);return np.power((c1+c2*y)/(1+c3*y),m2)
M=np.array([[1.660496,-0.587656,-0.072840],[-0.124547,1.132895,-0.008348],[-0.018154,-0.100597,1.118751]])
def bt2390(L,Lw,Lt=100.):
    ew=pq(Lw);mx=pq(Lt)/ew;e=pq(L)/ew;KS=1.5*mx-0.5
    T=np.minimum((e-KS)/(1-KS),1);T2=T*T;T3=T2*T
    p=(2*T3-3*T2+1)*KS+(T3-2*T2+T)*(1-KS)+(-2*T3+3*T2)*mx
    return np.where(e<=KS,L,eotf(p*ew))
def hdr2sdr(rgb,Lw,white=100.):
    Y=rgb@np.array([.2126,.7152,.0722]);m=rgb.min(-1)
    f=np.where((m<0)&(Y>0),Y/np.maximum(Y-m,1e-9),1.0)
    rgb=np.where(((m<0)&(Y>0))[...,None],Y[...,None]+(rgb-Y[...,None])*f[...,None],rgb)
    rgb=np.maximum(rgb,0);pk=rgb.max(-1);g=np.where(pk>0,bt2390(np.maximum(pk,1e-6),Lw,white)/np.maximum(pk,1e-6),1)
    return rgb*g[...,None]/white
def hable(x): return ((x*(0.15*x+0.05)+0.004)/(x*(0.15*x+0.5)+0.06))-0.02/0.3
def load(p):
    v=np.fromfile(p,dtype='<u4').reshape(540,960)
    return np.stack([v&1023,(v>>10)&1023,(v>>20)&1023],-1).astype(float)/1023
def ref_frames(clip,n=40):
    W,H=960,540
    raw=subprocess.run(['ffmpeg','-v','error','-i',clip,'-frames:v',str(n),'-vf',f'scale={W}:{H}:flags=area','-f','rawvideo','-pix_fmt','yuv420p10le','-'],capture_output=True).stdout
    fs=W*H*3//2;a=np.frombuffer(raw,dtype='<u2');out=[]
    for i in range(len(a)//fs):
        f=a[i*fs:(i+1)*fs];Y=f[:W*H].reshape(H,W).astype(float);U=f[W*H:W*H*5//4].reshape(H//2,W//2).astype(float);V=f[W*H*5//4:].reshape(H//2,W//2).astype(float)
        U=np.repeat(np.repeat(U,2,0),2,1);V=np.repeat(np.repeat(V,2,0),2,1);out.append((Y,U,V))
    return out
def to_lin2020(Y,U,V):
    y=(Y-64)/876;cb=(U-512)/896;cr=(V-512)/896;kr,kb=.2627,.0593;kg=1-kr-kb
    R=y+2*(1-kr)*cr;B=y+2*(1-kb)*cb;G=(y-kr*R-kb*B)/kg
    return eotf(np.clip(np.stack([R,G,B],-1),0,1))
def luma(x): return x@np.array([.2126,.7152,.0722])
def stat(name,x):
    l=luma(x);return f"{name:14s} p1 {np.percentile(l,1):.3f} p50 {np.percentile(l,50):.3f} p99 {np.percentile(l,99):.3f} max {l.max():.3f}"
tag=sys.argv[1];Lw=float(sys.argv[2]) if len(sys.argv)>2 else 711.
new=load(f'fr/{tag}_new/f0.raw');base=load(f'fr/{tag}_base/f0.raw')
fr=ref_frames(f'clips/{tag}.mkv');best=None
for i,(Y,U,V) in enumerate(fr[:30]):
    r=np.power(np.clip(hdr2sdr(to_lin2020(Y,U,V)@M.T,Lw),0,1),1/2.2)
    d=np.abs(r[::4,::4]-new[::4,::4]).mean()
    if best is None or d<best[0]: best=(d,i,r)
d,i,ref=best
print(f"{tag}: best matching source frame #{i}; mean |patched-reference| {255*np.abs(ref-new).mean():.2f} codes (median {255*np.median(np.abs(ref-new)):.2f}, p99 {255*np.percentile(np.abs(ref-new),99):.1f})")
lin=to_lin2020(*fr[i])@M.T
colorimetric=np.power(np.clip(lin/100,0,1),1/2.2)
print(' ',stat('stock D3D11',base));print(' ',stat('patched',new));print(' ',stat('reference',ref));print(' ',stat('colorimetric',colorimetric))
print('  source luma nits p50/p99/max',[round(float(np.percentile(luma(lin),q)),1) for q in (50,99,100)])
print('  clipped >0.98 : stock %.4f patched %.4f'%((luma(base)>.98).mean(),(luma(new)>.98).mean()),' crushed <0.02: stock %.4f patched %.4f'%((luma(base)<.02).mean(),(luma(new)<.02).mean()))
sheet=np.concatenate([base,new,ref],1);Image.fromarray((np.clip(sheet,0,1)*255).astype(np.uint8)).resize((1440,270)).save(f'fr/{tag}_cmp.png')

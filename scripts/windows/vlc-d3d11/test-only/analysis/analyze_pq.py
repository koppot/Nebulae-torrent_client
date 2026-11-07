import numpy as np, json, sys
m1=2610/16384;m2=2523/4096*128;c1=3424/4096;c2=2413/4096*32;c3=2392/4096*32
def eotf(e):
    p=np.power(np.maximum(e,0),1/m2);return 10000*np.power(np.maximum(p-c1,0)/(c2-c3*p),1/m1)
def pq(L):
    y=np.power(np.maximum(L,0)/10000,m1);return np.power((c1+c2*y)/(1+c3*y),m2)
M=np.array([[1.6605,-0.5876,-0.0728],[-0.1246,1.1329,-0.0083],[-0.0182,-0.1006,1.1187]])
def bt2390(L,Lw,Lt):
    if Lw<=Lt: return min(L,Lt)
    ew=pq(Lw);mx=pq(Lt)/ew;e=pq(L)/ew;KS=1.5*mx-0.5
    if e<=KS: return L
    T=min((e-KS)/(1-KS),1);T2=T*T;T3=T2*T
    p=(2*T3-3*T2+1)*KS+(T3-2*T2+T)*(1-KS)+(-2*T3+3*T2)*mx
    return float(eotf(p*ew))
def hdr2sdr(rgb,Lw,white=100):
    Y=rgb@np.array([.2126,.7152,.0722]);m=rgb.min()
    if m<0 and Y>0: rgb=Y+(rgb-Y)*(Y/(Y-m))
    rgb=np.maximum(rgb,0);pk=rgb.max()
    g=bt2390(pk,Lw,white)/pk if pk>0 else 1
    return rgb*g/white
def enc(lin): return 255*np.power(np.clip(lin,0,1),1/2.2)
def hable(x):
    return ((x*(0.15*x+0.05)+0.004)/(x*(0.15*x+0.5)+0.06))-0.02/0.3
def quant_gray(L):
    Y=pq(L);code=np.round(64+876*Y);return eotf((code-64)/876)
def quant_color(rgbn):
    e=pq(np.array(rgbn));kr,kb=.2627,.0593;kg=1-kr-kb
    Y=kr*e[0]+kg*e[1]+kb*e[2];Cb=(e[2]-Y)/(2*(1-kb));Cr=(e[0]-Y)/(2*(1-kr))
    Yq=(np.round(64+876*Y)-64)/876;Cbq=(np.round(512+896*Cb)-512)/896;Crq=(np.round(512+896*Cr)-512)/896
    R=Yq+2*(1-kr)*Crq;B=Yq+2*(1-kb)*Cbq;G=(Yq-kr*R-kb*B)/kg
    return eotf(np.array([R,G,B]))
def load(p):
    v=np.fromfile(p,dtype='<u4').reshape(540,960)
    return np.stack([v&1023,(v>>10)&1023,(v>>20)&1023],-1).astype(float)/1023*255
def patch(img,i,cols=6):
    r,c=divmod(i,cols);pw,ph=160,135
    s=img[r*ph+40:r*ph+95,c*pw+50:c*pw+110].reshape(-1,3);return np.median(s,axis=0)
spec=json.load(open('synth_pq.json'));grays=spec['grays'];colors=spec['colors']
Lw=float(sys.argv[3]) if len(sys.argv)>3 else 1000
new=load(sys.argv[1]);base=load(sys.argv[2])
print(f"{'patch':>22s} {'expect':>14s} {'patched':>14s} {'d':>4s} {'baseline(stock)':>16s}  (sRGB-ish 8-bit codes, SDR white 100 nit, Lw={Lw:g})")
mx=0;rows=[]
for i,g in enumerate(grays):
    L=quant_gray(g);exp=enc(hdr2sdr(np.array([L]*3),Lw));hb=enc(hable(np.array([L]*3)*0.01)/hable(11.2))
    o=patch(new,i);b=patch(base,i);d=np.abs(o-exp).max();mx=max(mx,d)
    print(f"{g:9.1f} nit gray {str(exp.round(1)):>14s} {str(o.round(1)):>14s} {d:4.0f} {str(b.round(1)):>16s}  hable-expect {hb[0]:.1f}")
for j,c in enumerate(colors):
    i=len(grays)+j;lin2020=quant_color(c);lin709=M@lin2020
    exp=enc(hdr2sdr(lin709,Lw));o=patch(new,i);b=patch(base,i);d=np.abs(o-exp).max();mx=max(mx,d)
    print(f"color {np.round(c,0).round(1)!s:>16s} {str(exp.round(1)):>14s} {str(o.round(1)):>14s} {d:4.0f} {str(b.round(1)):>16s}")
print("max |patched - reference| code error:",mx)
gs=[patch(new,i)[1] for i in range(len(grays))];print("gray ramp monotonic non-decreasing:",all(np.diff(gs)>=0), "first/last",gs[0],gs[-1])

import numpy as np, json, sys
exec(open('analyze_pq.py').read().split("spec=json")[0])
a_,b_,c_=0.17883277,0.28466892,0.55991073
def inv_hlg(x): return np.where(x<=0.5,x*x/3,(np.exp((x-c_)/a_)+b_)/12)
def quant(sig):
    sig=np.array(sig,float);kr,kb=.2627,.0593;kg=1-kr-kb
    Y=kr*sig[0]+kg*sig[1]+kb*sig[2];Cb=(sig[2]-Y)/(2*(1-kb));Cr=(sig[0]-Y)/(2*(1-kr))
    Yq=(np.round(64+876*Y)-64)/876;Cbq=(np.round(512+896*Cb)-512)/896;Crq=(np.round(512+896*Cr)-512)/896
    R=Yq+2*(1-kr)*Crq;B=Yq+2*(1-kb)*Cbq;G=(Yq-kr*R-kb*B)/kg
    return np.clip([R,G,B],0,1)
def hlg_nits(sig):
    E=inv_hlg(quant(sig));Ys=E@np.array([.2627,.678,.0593]);return E*1000*(Ys**0.2 if Ys>0 else 0)
spec=json.load(open('synth_hlg.json'));sg=spec['signals']
P=[[s]*3 for s in sg]+[[s,0,0] for s in (0.4,0.6,0.8)]+[[0,s,0] for s in (0.4,0.6,0.8)]+[[0,0,s] for s in (0.4,0.6,0.8)]
new=load(sys.argv[1]);base=load(sys.argv[2]);mx=0
print(f"{'HLG signal':>16s} {'nits':>8s} {'expect':>20s} {'patched':>20s} {'d':>4s} {'baseline(stock)':>20s}")
for i,p in enumerate(P):
    n=hlg_nits(p);lin709=M@n;exp=enc(hdr2sdr(lin709,1000));o=patch(new,i);b=patch(base,i);d=np.abs(o-exp).max();mx=max(mx,d)
    print(f"{str(p):>16s} {n.max():8.1f} {str(exp.round(1)):>20s} {str(o.round(1)):>20s} {d:4.0f} {str(b.round(1)):>20s}")
print('max |patched-reference|',round(mx,2))

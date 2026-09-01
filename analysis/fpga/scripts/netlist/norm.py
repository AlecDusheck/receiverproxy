import sys,pickle,re,collections,itertools
sys.path.insert(0,'.')
from slices import slice_netlist
from netlist2 import Design
d=Design('t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'); cells=slice_netlist(d)
pl=pickle.load(open('padlogic_16.53.pkl','rb'))
rows=pickle.load(open('final_16.53.pkl','rb')); by={r['pin']:r for r in rows}
G={'Q4@23,18':'G1','Q5@23,18':'G2','Q4@39,18':'G1b','Q5@39,18':'G2b'}
def classify(init,srcs):
    """srcs: dict pos->name. Return a description in terms of G1,G2 and data inputs."""
    names={}
    for p in 'ABCD': names[p]=G.get(srcs.get(p,('','' ))[0],None)
    # enumerate function
    def f(v): return init[v[0]+2*v[1]+4*v[2]+8*v[3]]=='1'
    pos=[p for p in 'ABCD' if p in srcs]
    g1=[p for p in 'ABCD' if names[p]=='G1' or names[p]=='G1b']
    g2=[p for p in 'ABCD' if names[p]=='G2' or names[p]=='G2b']
    data=[p for p in 'ABCD' if p in srcs and names[p] is None]
    if not g1 or not g2 or len(data)!=2: return None
    i1='ABCD'.index(g1[0]); i2='ABCD'.index(g2[0])
    dd=[ 'ABCD'.index(x) for x in data]
    tab={}
    for a in (0,1):
        for b in (0,1):
            sub={}
            for x in (0,1):
                for y in (0,1):
                    v=[0,0,0,0]; v[i1]=a; v[i2]=b; v[dd[0]]=x; v[dd[1]]=y
                    sub[(x,y)]=f(v)
            # describe sub as function of d0,d1
            if all(not t for t in sub.values()): s='0'
            elif all(sub.values()): s='1'
            elif sub[(0,0)]==sub[(0,1)] and sub[(1,0)]==sub[(1,1)]: s=('d0' if sub[(1,0)] else '!d0')
            elif sub[(0,0)]==sub[(1,0)] and sub[(0,1)]==sub[(1,1)]: s=('d1' if sub[(0,1)] else '!d1')
            else: s='mix'
            tab[(a,b)]=s
    return tab,data
cnt=collections.Counter(); detail=[]
for pin,r in sorted(pl.items(),key=lambda z:(by[z[0]]['row'],by[z[0]]['col'],by[z[0]]['pio'])):
    if not r['cell'] or r['cell']['kind']!='LUT' or not r['cell']['init']: continue
    c=classify(r['cell']['init'],r['fanin'])
    if c is None: continue
    tab,data=c
    sig=tuple(tab[k] for k in [(0,0),(0,1),(1,0),(1,1)])
    cnt[sig]+=1
    detail.append((pin,by[pin]['row'],by[pin]['col'],by[pin]['pio'],sig,[r['fanin'][p][0] for p in data]))
print("normalised pad function, indexed by (G1,G2) = (Q4@23,18, Q5@23,18):")
print("   (G1,G2)=  (0,0)   (0,1)   (1,0)   (1,1)     count")
for k,v in cnt.most_common():
    print("             %-7s %-7s %-7s %-7s   %d"%(k[0],k[1],k[2],k[3],v))
print()
for pin,y,x,p,sig,ds in detail:
    print("  %-4s R%02dC%02d%s  %-28s d0=%-16s d1=%s"%(pin,y,x,p,str(sig),ds[0],ds[1]))

import sys,re,collections,pickle; sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
d=Design('t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'); cells=slice_netlist(d)
rgb=set(pickle.load(open('rgb96.pkl','rb')))
pl=pickle.load(open('padlogic_16.53.pkl','rb'))
def s(n): return d.s(n)
def walkc(n,maxd=40):
    seen=set(); hits=set()
    for _ in range(maxd):
        for up in d.fixed_up(n):
            if '_EBR' in s(up): hits.add(s(up))
        nm=s(n)
        if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$',nm) or nm.startswith('G_'): return n,hits
        u=d.drivenby.get(n)
        if not u:
            f=d.fixed_up(n)
            if len(f)==1: n=f[0]; continue
            return n,hits
        n=sorted(u)[0]
        if n in seen: return n,hits
        seen.add(n)
    return n,hits
def ins_of(c):
    out=[]
    m=re.match(r'^F(\d)$',s(c))
    if m:
        i=int(m.group(1))
        for p in 'ABCD':
            w=d.W(c[0],c[1],'%s%d'%(p,i))
            if w is not None and w in d.drivenby: out.append(sorted(d.drivenby[w])[0])
        return out
    mm=re.match(r'^Q(\d)$',s(c))
    if mm:
        i=int(mm.group(1)); cc=cells.get((c[0],c[1],'Q%d'%i),{})
        if cc.get('sd')=='1':
            w=d.W(c[0],c[1],'M%d'%i)
            if w is not None and w in d.drivenby: out.append(sorted(d.drivenby[w])[0])
        else:
            w=d.W(c[0],c[1],'F%d'%i)
            if w is not None: out.append(w)
    return out
def cone_hits(n,depth):
    front=[n]; seen=set(); hits=set()
    for _ in range(depth):
        nxt=[]
        for c in front:
            key=(c[0],c[1],s(c))
            if key in seen: continue
            seen.add(key)
            for src in ins_of(c):
                nn,h=walkc(src); hits|=h; nxt.append(nn)
        front=nxt
        if not front: break
    return hits
res={}
for pin,r in pl.items():
    if pin not in rgb or not r['cell']: continue
    m=re.match(r'^(\w+)@(\d+),(\d+)$',r['node'])
    if not m: continue
    n=d.W(int(m.group(2)),int(m.group(3)),m.group(1))
    if n is None: continue
    res[pin]=cone_hits(n,3)
hit=[p for p,v in res.items() if v]
print("RGB pads with a resolvable driver:",len(res))
print("  ...whose 3-level fan-in cone reaches an EBR data-out wire:",len(hit))
for p in hit[:10]: print("     ",p,sorted(res[p])[:4])
allw=collections.Counter()
for v in res.values():
    for w in v: allw[re.sub(r'\d+','#',w)]+=1
print("  EBR wire classes:",dict(allw))

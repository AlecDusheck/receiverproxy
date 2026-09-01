import sys,re,collections,pickle,json
sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
from lut import expr
CFG=sys.argv[1]; TAG=sys.argv[2]
d=Design(CFG); cells=slice_netlist(d)
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']
def s(n): return d.s(n)
def walk(n,maxd=40):
    seen=set()
    for _ in range(maxd):
        nm=s(n)
        if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$',nm): return n,'CELL'
        if nm.startswith('G_'): return n,'GLOBAL'
        u=d.drivenby.get(n)
        if not u:
            f=d.fixed_up(n)
            if len(f)==1: n=f[0]; continue
            return n,'DEAD'
        n=sorted(u)[0]
        if n in seen: return n,'LOOP'
        seen.add(n)
    return n,'DEEP'
def fanin(n):
    """A/B/C/D sources of LUT F#"""
    m=re.match(r'^F(\d)$',s(n))
    if not m: return {}
    i=int(m.group(1)); r={}
    for p in 'ABCD':
        w=d.W(n[0],n[1],'%s%d'%(p,i))
        if w is not None and w in d.drivenby:
            src=sorted(d.drivenby[w])[0]
            nn,st=walk(src); r[p]=(d.nm(nn),st)
    return r
res={}
for pin,v in sorted(pkg.items()):
    y,x,p=v['row'],v['col'],v['pio']
    if (x,y) not in d.T: continue
    w=d.W(x,y,'JPADDO%s'%p)
    if w is None: continue
    up=d.fixed_up(w)
    if not up or up[0] not in d.drivenby: continue
    nn,st=walk(sorted(d.drivenby[up[0]])[0])
    key=(nn[0],nn[1],s(nn)); c=cells.get(key)
    res[pin]=dict(node=d.nm(nn),st=st,cell=c,fanin=fanin(nn) if c and c['kind']=='LUT' else {},
                  expr=expr(c['init']) if c and c['kind']=='LUT' and c['init'] else None)
pickle.dump(res,open('padlogic_%s.pkl'%TAG,'wb'))
# which fanin sources are shared across many pads?
sh=collections.Counter()
for pin,r in res.items():
    for p,(src,st) in r['fanin'].items(): sh[src]+=1
print(TAG,'most-shared inputs to pad-driver LUTs:')
for k,v in sh.most_common(25): print('   %-22s %d'%(k,v))

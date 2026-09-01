import sys,re,json,collections,pickle
sys.path.insert(0,'.')
from netlist2 import Design
CFG=sys.argv[1]; TAG=sys.argv[2]
d=Design(CFG)
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']
def s(n): return d.s(n)
def back(n,maxd=30):
    seen=set()
    for _ in range(maxd):
        nm=s(n)
        if nm.startswith('G_') or re.match(r'^(F\d|Q\d)$',nm): return n
        u=d.drivenby.get(n)
        if not u:
            f=d.fixed_up(n)
            if len(f)==1: n=f[0]; continue
            return n
        n=sorted(u)[0]
        if n in seen: return n
        seen.add(n)
    return n
res={}
for pin,v in sorted(pkg.items()):
    y,x,p=v['row'],v['col'],v['pio']
    if (x,y) not in d.T: continue
    sfx='_SIOLOGIC' if d.W(x,y,'PADDI%s_SIOLOGIC'%p) else '_IOLOGIC'
    r={}
    for sig in ('CLK','CE','LSR'):
        w=d.W(x,y,'J%s%s%s'%(sig,p,sfx))
        if w is None: continue
        u=d.fixed_up(w)
        if len(u)!=1: continue
        cib=u[0]
        r[sig]=d.nm(back(cib)) if cib in d.drivenby else ('tie' if cib else None)
        r[sig+'_wire']=d.nm(cib)
    # also the data wire (JTXDATA0) source
    w=d.W(x,y,'JTXDATA0%s%s'%(p,sfx))
    if w is not None:
        u=d.fixed_up(w)
        if len(u)==1 and u[0] in d.drivenby: r['D']=d.nm(sorted(d.drivenby[u[0]])[0])
    res[pin]=r
pickle.dump(res,open('iolclk_%s.pkl'%TAG,'wb'))
rgb=pickle.load(open('rgb96.pkl','rb'))
c=collections.Counter(res[p].get('CLK') for p in rgb)
print(TAG,'CLK of the 96 RGB IO registers:'); 
for k,v in c.most_common(): print('   %-24s %d'%(k,v))
c=collections.Counter(res[p].get('CE') for p in rgb)
print(' CE:'); 
for k,v in c.most_common(): print('   %-24s %d'%(k,v))
c=collections.Counter(res[p].get('LSR') for p in rgb)
print(' LSR:')
for k,v in c.most_common(): print('   %-24s %d'%(k,v))

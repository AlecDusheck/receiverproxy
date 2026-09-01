import sys,re,collections,pickle
sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
CFG=sys.argv[1]; TAG=sys.argv[2]
d=Design(CFG); cells=slice_netlist(d)
def s(n): return d.s(n)
# global net driving each tile's CLK0/CLK1
tileclk={}
for k,v in d.drivenby.items():
    nm=s(k)
    if nm in ('CLK0','CLK1'):
        srcs=[s(z) for z in v]
        g=[z for z in srcs if z.startswith('G_')]
        tileclk[(k[0],k[1],nm)]=g[0] if g else ('|'.join(srcs))
# per-slice MUXCLK selection
muxsel={}
for k,v in d.drivenby.items():
    m=re.match(r'^MUXCLK(\d)$',s(k))
    if m:
        srcs=[s(z) for z in v]
        muxsel[(k[0],k[1],int(m.group(1)))]=srcs[0] if len(srcs)==1 else '|'.join(sorted(srcs))
dom=collections.Counter(); ffdom={}
for (x,y,nm),c in cells.items():
    if c['kind']!='FF': continue
    si='ABCD'.index(c['slice'])
    sel=muxsel.get((x,y,si))
    if sel is None:
        # default: slice A/B -> CLK0, C/D -> CLK1  (only used when no arc is set)
        sel='CLK0' if si<2 else 'CLK1'
        g=tileclk.get((x,y,sel),'UNSET')
    else:
        g=tileclk.get((x,y,sel),'UNSET(%s)'%sel)
    cm=c.get('clkmux')
    dom[(g,cm)]+=1; ffdom[(x,y,nm)]=(g,cm)
pickle.dump((ffdom,tileclk,muxsel),open('ffclk_%s.pkl'%TAG,'wb'))
print(TAG,'FF clock-domain census (global net, CLKMUX):')
for k,v in dom.most_common(20): print('   %-16s %-6s %5d'%(k[0],k[1],v))
print('   total FFs',sum(dom.values()))

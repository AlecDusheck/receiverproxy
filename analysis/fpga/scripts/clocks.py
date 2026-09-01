import sys,re,json,collections
from netbuild import *
CFG='t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'
cc,rg,TLOC,setsinks,setsources,drivenby,drives,tiletype,tilecfg,gn=build(CFG)
def sn(n):
    try: return rg.to_str(n[2])
    except Exception: return "?%d"%n[2]
def nm(n): return "%s@%d,%d"%(sn(n),n[0],n[1])
def up(n):
    tl=TLOC.get((n[0],n[1]))
    if tl is None or n[2] not in tl.wires: return []
    return [(a2.source.loc.x,a2.source.loc.y,a2.source.id) for a in tl.wires[n[2]].uphill for a2 in [TLOC[(a.loc.x,a.loc.y)].arcs[a.id]]]
def dn(n):
    tl=TLOC.get((n[0],n[1]))
    if tl is None or n[2] not in tl.wires: return []
    return [(a2.sink.loc.x,a2.sink.loc.y,a2.sink.id) for a in tl.wires[n[2]].downhill for a2 in [TLOC[(a.loc.x,a.loc.y)].arcs[a.id]]]
def tr(n,d=0,seen=None,lim=40):
    if seen is None: seen=set()
    pre="  "*d
    if n in seen: return [pre+nm(n)+" *"]
    seen.add(n)
    out=[pre+nm(n)]
    src=drivenby.get(n)
    if not src:
        # follow fixed (non-configurable) uphill
        fx=[s for s in up(n)]
        if len(fx)==1: out+=tr(fx[0],d+1,seen)
        elif len(fx)==0: out.append(pre+"  <ROOT>")
        else: out.append(pre+"  <%d fixed uphill: %s>"%(len(fx),[nm(f) for f in fx][:6]))
        return out
    for s in src: out+=tr(s,d+1,seen)
    return out
targets=['G_JPCLKT71','G_JPCLKT30','G_JBLQPCLKCIB0','G_JLLCPLL0CLKOP','G_JLLCPLL0CLKOS','G_JLLCPLL0CLKOS3',
         'G_JURQECLKCIB0','G_JLRQECLKCIB1','G_ULCPCLKCIB0','G_URCPCLKCIB0','G_LLCPCLKCIB0','G_VPFN0000',
         'G_HPFE0200','G_HPFE0300','G_HPFE0400','G_HPFE0600','G_HPFE0800','G_HPFW0200']
# find node for each name: search all tiles
byname=collections.defaultdict(list)
for (x,y),tl in TLOC.items():
    for wid in tl.wires:
        s=rg.to_str(wid)
        if s in targets: byname[s].append((x,y,wid))
for t in targets:
    print("=== "+t, len(byname[t]),"instances")
    for n in byname[t][:3]:
        print("\n".join(tr(n)[:25]))

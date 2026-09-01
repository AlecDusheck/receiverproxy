import sys,re,collections; sys.path.insert(0,'/opt/homebrew/opt/prjtrellis/lib/trellis')
import pytrellis
pytrellis.load_database('/opt/homebrew/opt/prjtrellis/share/trellis/database')
CFG='t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'
cc=pytrellis.ChipConfig.from_string(open(CFG).read())
rg=cc.to_chip().get_routing_graph(True,True)
used=set()
for tname,tc in cc.tiles.items():
    m=re.search(r'R(\d+)C(\d+)',tname)
    if not m: continue
    y,x=int(m.group(1)),int(m.group(2))
    for a in tc.carcs:
        for nm in (a.source,a.sink):
            rid=rg.id_at_loc(x,y,nm); used.add((rid.loc.x,rid.loc.y,rid.id))
TLOC={(tl.loc.x,tl.loc.y):tl for tl in rg.tiles.values()}
def wires_of(x,y):
    return TLOC[(x,y)].wires
for (x,y,pio) in [(27,0,'A'),(31,0,'A'),(29,0,'A'),(0,23,'B')]:
    print('=== R%dC%d PIO%s'%(y,x,pio))
    tl=TLOC.get((x,y))
    if tl is None: print(' no tile'); continue
    for wid,w in tl.wires.items():
        wn=rg.to_str(wid)
        if 'PADD' not in wn: continue
        if pio not in wn[-2:]: continue
        print('  wire',wn,'set=',(x,y,wid) in used)
        for a in list(w.uphill)+list(w.downhill):
            atl=TLOC.get((a.loc.x,a.loc.y))
            try: arc=atl.arcs[a.id]
            except Exception: continue
            s=rg.to_str(arc.source.id); k=rg.to_str(arc.sink.id)
            su=(arc.source.loc.x,arc.source.loc.y,arc.source.id) in used
            ku=(arc.sink.loc.x,arc.sink.loc.y,arc.sink.id) in used
            print('     arc %s -> %s cfg=%s srcUsed=%s snkUsed=%s'%(s,k,arc.configurable,su,ku))

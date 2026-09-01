import sys,re,json,collections,pickle
sys.path.insert(0,'/opt/homebrew/opt/prjtrellis/lib/trellis')
import pytrellis
pytrellis.load_database('/opt/homebrew/opt/prjtrellis/share/trellis/database')

def build(CFG):
    cc=pytrellis.ChipConfig.from_string(open(CFG).read())
    rg=cc.to_chip().get_routing_graph(True,True)
    TLOC={(tl.loc.x,tl.loc.y):tl for tl in rg.tiles.values()}
    gcache={}
    def gn(y,x,name):
        k=(y,x,name)
        if k in gcache: return gcache[k]
        n=rg.globalise_net(y,x,name)
        v=(n.loc.x,n.loc.y,n.id)
        gcache[k]=v; return v
    setsinks=set(); setsources=set(); drivenby=collections.defaultdict(set); drives=collections.defaultdict(set)
    tiletype={}; tilecfg=collections.defaultdict(dict)
    for tname,tc in cc.tiles.items():
        rc=re.search(r'R(\d+)C(\d+)',tname)
        if not rc: continue
        y,x=int(rc.group(1)),int(rc.group(2))
        tiletype.setdefault((y,x),set()).add(tname.split(':')[1])
        for a in tc.carcs:
            s=gn(y,x,a.source); k=gn(y,x,a.sink)
            setsources.add(s); setsinks.add(k); drivenby[k].add(s); drives[s].add(k)
    cur=None
    for line in open(CFG):
        line=line.rstrip()
        if line.startswith('.tile'): cur=line.split()[1]
        elif line.startswith('.'): cur=None
        elif cur and line.strip():
            rc=re.search(r'R(\d+)C(\d+)',cur)
            if not rc: continue
            key=(int(rc.group(1)),int(rc.group(2)))
            p=line.strip().split()
            if p[0] in ('enum:','word:','unknown:'):
                tilecfg[key][p[1]]=' '.join(p[2:])
    return cc,rg,TLOC,setsinks,setsources,drivenby,drives,tiletype,tilecfg,gn

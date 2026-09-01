"""Full logical netlist extraction for an ECP5 .config: slices, LUTs, FFs, and the
   set-arc routing graph reduced to cell-to-cell connectivity."""
import re,json,collections,pickle,sys
sys.path.insert(0,'/opt/homebrew/opt/prjtrellis/lib/trellis')
import pytrellis
_loaded=False
def load():
    global _loaded
    if not _loaded:
        pytrellis.load_database('/opt/homebrew/opt/prjtrellis/share/trellis/database'); _loaded=True

class Design:
    def __init__(self,CFG):
        load()
        self.cfgpath=CFG
        cc=pytrellis.ChipConfig.from_string(open(CFG).read())
        self.rg=rg=cc.to_chip().get_routing_graph(True,True)
        self.T={(t.loc.x,t.loc.y):t for t in rg.tiles.values()}
        # ---- set arcs -> absolute node graph
        gc={}
        def gn(y,x,name):
            k=(y,x,name)
            if k not in gc:
                n=rg.globalise_net(y,x,name); gc[k]=(n.loc.x,n.loc.y,n.id)
            return gc[k]
        self.drivenby=collections.defaultdict(set); self.drives=collections.defaultdict(set)
        self.tilecfg=collections.defaultdict(dict); self.ttypes=collections.defaultdict(set)
        for tname,tc in cc.tiles.items():
            rc=re.search(r'R(\d+)C(\d+)',tname)
            if not rc: continue
            y,x=int(rc.group(1)),int(rc.group(2))
            self.ttypes[(y,x)].add(tname.split(':')[1])
            for a in tc.carcs:
                s=gn(y,x,a.source); k=gn(y,x,a.sink)
                self.drivenby[k].add(s); self.drives[s].add(k)
        cur=None
        for l in open(CFG):
            l=l.rstrip()
            if l.startswith('.tile'): cur=l.split()[1]
            elif l.startswith('.'): cur=None
            elif cur and l.strip():
                rc=re.search(r'R(\d+)C(\d+)',cur)
                if not rc: continue
                p=l.strip().split()
                if p[0] in ('enum:','word:','unknown:'):
                    self.tilecfg[(int(rc.group(1)),int(rc.group(2)))][p[1] if p[0]!='unknown:' else 'unknown.'+p[1]]=' '.join(p[2:]) if len(p)>2 else ''
        self._id=collections.defaultdict(dict)
    # ---- helpers
    def s(self,n):
        try: return self.rg.to_str(n[2])
        except Exception: return "?%d"%n[2]
    def nm(self,n): return "%s@%d,%d"%(self.s(n),n[0],n[1])
    def W(self,x,y,name):
        tl=self.T.get((x,y))
        if tl is None: return None
        wid=self.rg.ident(name)
        return (x,y,wid) if wid in tl.wires else None
    def fixed_up(self,n):
        tl=self.T.get((n[0],n[1]))
        if tl is None or n[2] not in tl.wires: return []
        return [(a2.source.loc.x,a2.source.loc.y,a2.source.id)
                for a in tl.wires[n[2]].uphill for a2 in [self.T[(a.loc.x,a.loc.y)].arcs[a.id]]]
    def fixed_dn(self,n):
        tl=self.T.get((n[0],n[1]))
        if tl is None or n[2] not in tl.wires: return []
        return [(a2.sink.loc.x,a2.sink.loc.y,a2.sink.id)
                for a in tl.wires[n[2]].downhill for a2 in [self.T[(a.loc.x,a.loc.y)].arcs[a.id]]]
    def up(self,n):
        """one logical step back: set-arc drivers, else the unique fixed uphill"""
        d=self.drivenby.get(n)
        if d: return list(d)
        f=self.fixed_up(n)
        return f if len(f)==1 else []
    def source_cell(self,n,limit=60):
        """walk back through pure routing until a cell output (F#, Q#, *_SLICE, J*, G_*) is reached"""
        seen=set()
        while True:
            s=self.s(n)
            if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$',s): return n,'cell'
            if s.startswith('G_') or s.startswith('J') or '_' in s and not re.match(r'^[HV]\d',s):
                if not re.match(r'^[HVEWNS]\d',s) and not re.match(r'^[EWNS]\d_',s): return n,'special'
            u=self.up(n)
            if not u or n in seen: return n,'root'
            seen.add(n)
            n=sorted(u)[0]
            if len(seen)>limit: return n,'deep'

import sys,re,json,collections; sys.path.insert(0,'/opt/homebrew/opt/prjtrellis/lib/trellis')
import pytrellis
pytrellis.load_database('/opt/homebrew/opt/prjtrellis/share/trellis/database')
CFG=sys.argv[1]
cc=pytrellis.ChipConfig.from_string(open(CFG).read())
rg=cc.to_chip().get_routing_graph(True,True)
used=set(); cfgline=collections.defaultdict(list)
for tname,tc in cc.tiles.items():
    m=re.search(r'R(\d+)C(\d+)',tname)
    if not m: continue
    y,x=int(m.group(1)),int(m.group(2))
    for a in tc.carcs:
        for nm in (a.source,a.sink):
            rid=rg.id_at_loc(x,y,nm); used.add((rid.loc.x,rid.loc.y,rid.id))
TLOC={(tl.loc.x,tl.loc.y):tl for tl in rg.tiles.values()}
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
bonded={(v['row'],v['col'],v['pio']):p for p,v in io['packages']['CABGA256'].items()}
meta={(m['row'],m['col'],m['pio']):m for m in io['pio_metadata']}
std={}; iol=collections.defaultdict(list); cur=None
for line in open(CFG):
    line=line.strip()
    if line.startswith('.tile'): cur=line[6:]
    elif line.startswith('.'): cur=None
    elif cur and line:
        rc=re.search(r'R(\d+)C(\d+)',cur)
        if not rc: continue
        k=(int(rc.group(1)),int(rc.group(2)))
        m2=re.search(r'PIO([A-D])\.BASE_TYPE',line)
        if m2: std[k+(m2.group(1),)]=line.split()[-1]
        m3=re.search(r'(?:IOLOGIC([A-D])\.|DATAMUX_O\w+|TRIMUX)',line)
        m4=re.search(r'IOLOGIC([A-D])',line)
        if m4: iol[k+(m4.group(1),)].append(line)
rows=[]
for (y,x,pio),pin in sorted(bonded.items()):
    tl=TLOC.get((x,y))
    flags=set()
    if tl:
        for wid,w in tl.wires.items():
            wn=rg.to_str(wid)
            m=re.match(r'J?PADD([OIT])%s$'%pio,wn)
            if not m: continue
            for a in list(w.uphill)+list(w.downhill):
                atl=TLOC.get((a.loc.x,a.loc.y))
                try: arc=atl.arcs[a.id]
                except Exception: continue
                for e in (arc.source,arc.sink):
                    if rg.to_str(e.id).startswith('PADD') or rg.to_str(e.id).startswith('JPADD'): continue
                    if (e.loc.x,e.loc.y,e.id) in used: flags.add(m.group(1)+':'+rg.to_str(e.id))
    k=(y,x,pio)
    if flags or iol.get(k):
        rows.append((pin,y,x,pio,meta.get(k,{}).get('bank'),meta.get(k,{}).get('function',''),std.get(k,'-'),sorted(flags),len(iol.get(k,[]))))
print("PIN\tROW\tCOL\tPIO\tBANK\tFUNC\tSTD\tFABRIC\tIOLOGIC")
for r in rows:
    print("%s\tR%d\tC%d\t%s\tb%s\t%s\t%s\t%s\t%d"%(r[0],r[1],r[2],r[3],r[4],r[5],r[6],','.join(r[7]) or '-',r[8]))
print("bonded pins in use: %d"%len(rows),file=sys.stderr)

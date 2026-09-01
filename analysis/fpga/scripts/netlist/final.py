import sys,re,json,collections,pickle
from netbuild import *
CFG=sys.argv[1]; TAG=sys.argv[2]
cc,rg,TLOC,setsinks,setsources,drivenby,drives,tiletype,tilecfg,gn=build(CFG)
def sn(n):
    try: return rg.to_str(n[2])
    except Exception: return "?%d"%n[2]
def nm(n): return "%s@%d,%d"%(sn(n),n[0],n[1])
def root(n):
    seen={n}
    while True:
        d=drivenby.get(n)
        if not d:
            tl=TLOC.get((n[0],n[1]))
            fx=[]
            if tl and n[2] in tl.wires:
                fx=[(a2.source.loc.x,a2.source.loc.y,a2.source.id) for a in tl.wires[n[2]].uphill for a2 in [TLOC[(a.loc.x,a.loc.y)].arcs[a.id]]]
            if len(fx)==1 and fx[0] not in seen:
                seen.add(fx[0]); n=fx[0]; continue
            return n
        nx=sorted(d)[0]
        if nx in seen: return n
        seen.add(nx); n=nx
def W(x,y,s):
    tl=TLOC.get((x,y)); wid=rg.ident(s)
    return (x,y,wid) if tl and wid in tl.wires else None
def up1(n):
    tl=TLOC[(n[0],n[1])]
    for a in tl.wires[n[2]].uphill:
        a2=TLOC[(a.loc.x,a.loc.y)].arcs[a.id]
        return (a2.source.loc.x,a2.source.loc.y,a2.source.id)
def dn1(n,pref):
    tl=TLOC[(n[0],n[1])]
    for a in tl.wires[n[2]].downhill:
        a2=TLOC[(a.loc.x,a.loc.y)].arcs[a.id]
        if sn((a2.sink.loc.x,a2.sink.loc.y,a2.sink.id)).startswith(pref):
            return (a2.sink.loc.x,a2.sink.loc.y,a2.sink.id)
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']; meta={(m['row'],m['col'],m['pio']):m for m in io['pio_metadata']}
rows=[]
for pin,v in sorted(pkg.items()):
    y,x,pio=v['row'],v['col'],v['pio']
    if (x,y) not in TLOC: continue
    cfg=tilecfg.get((y,x),{}); tl=TLOC[(x,y)]
    props={k.split('.',1)[1]:val for k,val in cfg.items() if k.startswith('PIO%s.'%pio)}
    iolg={k.split('.',1)[1]:val for k,val in cfg.items() if re.match(r'S?IOLOGIC%s\.'%pio,k)}
    sfx='_SIOLOGIC' if W(x,y,'PADDI%s_SIOLOGIC'%pio) else '_IOLOGIC'
    def fab(wname,dirn):
        w=W(x,y,wname)
        if w is None: return None
        return up1(w) if dirn=='in' else dn1(w,'J')
    o=fab('JPADDO%s'%pio,'in'); t=fab('JPADDT%s'%pio,'in')
    d1=fab('JTXDATA1%s%s'%(pio,sfx),'in')
    di=W(x,y,'JDI%s'%pio); dif=dn1(di,'J') if di else None
    rx0=fab('JRXDATA0%s%s'%(pio,sfx),'out'); rx1=fab('JRXDATA1%s%s'%(pio,sfx),'out')
    ck=fab('JCLK%s%s'%(pio,sfx),'in')
    ec=W(x,y,'ECLK%s%s'%(pio,sfx))
    rows.append(dict(pin=pin,row=y,col=x,pio=pio,bank=meta[(y,x,pio)].get('bank'),
        func=meta[(y,x,pio)].get('function',''),dqs=meta[(y,x,pio)].get('dqs',''),
        tt='/'.join(sorted(tiletype.get((y,x),[]))),props=props,iol=iolg,
        O=o in setsinks, T=t in setsinks if t else False,
        D1=d1 in setsinks if d1 else False,
        I=(dif in setsources) if dif else False,
        RX0=(rx0 in setsources) if rx0 else False, RX1=(rx1 in setsources) if rx1 else False,
        CK=nm(root(ck)) if (ck and ck in setsinks) else '',
        OR=nm(root(o)) if o in setsinks else '', TR=nm(root(t)) if (t and t in setsinks) else ''))
pickle.dump(rows,open('final_%s.pkl'%TAG,'wb'))
hdr="PIN BANK ROW COL PIO TILE FUNC DQS BASETYPE DRIVE PULL SLEW HYST CLAMP ODRAIN IOLMODE IOLCFG OUT ODDR1 TRI IN RX0 RX1 CLKROOT OROOT TROOT".split()
f=open('final_%s.tsv'%TAG,'w'); f.write('\t'.join(hdr)+'\n')
for r in rows:
    p=r['props']
    f.write('\t'.join(map(str,[r['pin'],r['bank'],r['row'],r['col'],r['pio'],r['tt'],r['func'],r['dqs'],
      p.get('BASE_TYPE','-'),p.get('DRIVE',''),p.get('PULLMODE',''),p.get('SLEWRATE',''),p.get('HYSTERESIS',''),
      p.get('CLAMP',''),p.get('OPENDRAIN',''),r['iol'].get('MODE',''),
      ';'.join('%s=%s'%(k,val) for k,val in sorted(r['iol'].items()) if k!='MODE'),
      int(r['O']),int(r['D1']),int(r['T']),int(r['I']),int(r['RX0']),int(r['RX1']),r['CK'],r['OR'],r['TR']]))+'\n')
f.close()
print(TAG,"OUT",sum(r['O'] for r in rows),"ODDR1",sum(r['D1'] for r in rows),"TRI",sum(r['T'] for r in rows),
      "IN",sum(r['I'] for r in rows),"RX0",sum(r['RX0'] for r in rows),"RX1",sum(r['RX1'] for r in rows),
      "IOL",sum(1 for r in rows if r['iol']))
print("--- pins with IOLOGIC or DDR or input:")
for r in rows:
    if r['iol'] or r['I'] or r['RX0'] or r['D1']:
        print("  %-4s b%s R%dC%d%s %-12s %-16s %-14s O=%d D1=%d T=%d I=%d RX=%d%d CK=%s"%(
          r['pin'],r['bank'],r['row'],r['col'],r['pio'],r['func'],r['props'].get('BASE_TYPE','-'),
          r['iol'].get('MODE',''),r['O'],r['D1'],r['T'],r['I'],r['RX0'],r['RX1'],r['CK']))

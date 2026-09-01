import re,json,collections,pickle
CFG='t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'
tilecfg=collections.defaultdict(dict); ttype={}
cur=None
for l in open(CFG):
    l=l.rstrip()
    if l.startswith('.tile'):
        cur=l.split()[1]; rc=re.search(r'R(\d+)C(\d+)',cur)
        if rc: ttype[(int(rc.group(1)),int(rc.group(2)))]=cur.split(':')[1]
    elif l.startswith('.'): cur=None
    elif cur and l.strip():
        rc=re.search(r'R(\d+)C(\d+)',cur)
        if not rc: continue
        p=l.strip().split()
        if p[0] in ('enum:','word:'): tilecfg[(int(rc.group(1)),int(rc.group(2)))][p[1]]=' '.join(p[2:])
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']
def sites(y,x,pio):
    if y==0 or y==50:  # top/bottom: A here, B at col+1
        return [(y,x)] if pio=='A' else [(y,x+1)]
    return [(y,x),(y+1,x),(y+2,x)]
out={}
for pin,v in pkg.items():
    y,x,pio=v['row'],v['col'],v['pio']
    pr={}
    for (ry,rx) in sites(y,x,pio):
        for k,val in tilecfg.get((ry,rx),{}).items():
            m=re.match(r'PIO%s\.(.*)'%pio,k)
            if m: pr.setdefault(m.group(1),[]).append(val)
        for k,val in tilecfg.get((ry,rx),{}).items():
            m=re.match(r'S?IOLOGIC%s\.(.*)'%pio,k)
            if m: pr.setdefault('IOL_'+m.group(1),[]).append(val)
    out[pin]=pr
pickle.dump(out,open('props_16.53.pkl','wb'))
rows=pickle.load(open('final_16.53.pkl','rb'))
c=collections.Counter()
for r in rows:
    pr=out[r['pin']]
    sig=(tuple(sorted(set(x for v in pr.get('BASE_TYPE',[]) for x in [v]))),
         tuple(sorted(set(pr.get('DRIVE',[])))),tuple(sorted(set(pr.get('HYSTERESIS',[])))),
         tuple(sorted(set(pr.get('SLEWRATE',[])))),tuple(sorted(set(pr.get('PULLMODE',[])))))
    d=('OUT' if r['O'] and not r['T'] else 'BIDIR' if r['O'] and r['T'] else 'IN' if (r['I'] or r['RX0']) else '-')
    c[(d,sig)]+=1
for k,v in sorted(c.items(),key=lambda z:-z[1]):
    print("%-6s n=%-3d BASE=%s DRIVE=%s HYST=%s SLEW=%s PULL=%s"%(k[0],v,k[1][0],k[1][1],k[1][2],k[1][3],k[1][4]))

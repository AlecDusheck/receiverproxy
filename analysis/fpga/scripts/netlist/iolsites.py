import re,json,collections,sys
CFG=sys.argv[1]
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']
meta={(m['row'],m['col'],m['pio']):m for m in io['pio_metadata']}
# site -> pin.  Config for a PIO can live in a neighbouring prjtrellis tile of the same PIC group.
# top/bottom: PIOA in tile col X, PIOB in tile col X+1.  left/right: rows R,R+1,R+2 share the group.
site2pin={}
for pin,v in pkg.items():
    y,x,p=v['row'],v['col'],v['pio']
    if y==0 or y==50:
        keys=[(y,x,p)] if p=='A' else [(y,x+1,p)]
    else:
        keys=[(y+d,x,p) for d in (0,1,2)]
    for k in keys: site2pin.setdefault(k,pin)
cur=None; out=[]
for l in open(CFG):
    l=l.rstrip()
    if l.startswith('.tile'): cur=l.split()[1]
    elif l.startswith('.'): cur=None
    elif cur and l.strip():
        m=re.match(r'enum: S?IOLOGIC([A-D])\.MODE (\S+)',l.strip())
        if not m: continue
        rc=re.search(r'R(\d+)C(\d+)',cur)
        y,x=int(rc.group(1)),int(rc.group(2))
        pin=site2pin.get((y,x,m.group(1)),'??')
        out.append((pin,y,x,m.group(1),m.group(2),cur))
for r in sorted(out,key=lambda z:(z[4],z[1],z[2],z[3])):
    print("%-5s R%02dC%02d%s %-14s %s"%(r[0],r[1],r[2],r[3],r[4],r[5]))
print("total",len(out),file=sys.stderr)

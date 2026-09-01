import re,json,collections,pickle,sys
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']
def keys(y,x,p):
    if y==0 or y==50: return [(y,x,p)] if p=='A' else [(y,x+1,p)]
    return [(y+d,x,p) for d in (0,1,2)]
site2pin={}
for pin,v in pkg.items():
    for k in keys(v['row'],v['col'],v['pio']): site2pin.setdefault(k,pin)
def iolmodes(CFG):
    cur=None; d={}
    for l in open(CFG):
        l=l.rstrip()
        if l.startswith('.tile'): cur=l.split()[1]
        elif l.startswith('.'): cur=None
        elif cur and l.strip():
            m=re.match(r'enum: S?IOLOGIC([A-D])\.MODE (\S+)',l.strip())
            if not m: continue
            rc=re.search(r'R(\d+)C(\d+)',cur)
            pin=site2pin.get((int(rc.group(1)),int(rc.group(2)),m.group(1)))
            if pin: d[pin]=m.group(2)
    return d
n=iolmodes('t_E320_PCB6.0_Normal_FPGA13.39_20221101.config')
l=iolmodes('t_E320_PCB6.1_LS0allDA_FPGA6.69_20220907.config')
p=iolmodes('t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config')
rgb=sorted(k for k,v in n.items() if v=='IREG_OREG')
rgb2=sorted(k for k,v in l.items() if v=='IREG_OREG')
print("13.39 IREG_OREG:",len(rgb)," 6.69 IREG_OREG:",len(rgb2)," identical:",rgb==rgb2)
print("16.53 IREG_OREG:",sorted(k for k,v in p.items() if v=='IREG_OREG'))
rows=pickle.load(open('final_16.53.pkl','rb'))
by={r['pin']:r for r in rows}
c=collections.Counter()
for k in rgb:
    r=by[k]
    d=('OUT' if r['O'] and not r['T'] else 'BIDIR' if r['O'] and r['T'] else 'IN' if (r['I'] or r['RX0']) else '-')
    c[(('TOP' if r['row']==0 else 'RIGHT' if r['col']==72 else 'BOTTOM' if r['row']==50 else 'LEFT'),d)]+=1
print("the 96 sites, as classified in 16.53:",dict(c))
pickle.dump(rgb,open('rgb96.pkl','wb'))
# what left/right output pins are NOT in the 96
lr=[r['pin'] for r in rows if (r['col'] in (0,72)) and r['O']]
print("left/right driven-output pins in 16.53:",len(lr))
print("  in 96:",len([x for x in lr if x in set(rgb)]),"  NOT in 96:",sorted(set(lr)-set(rgb)))
print("  in 96 but not a 16.53 output:",sorted(set(rgb)-set(lr)))

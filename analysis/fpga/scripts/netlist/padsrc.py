import sys,re,json,collections,pickle
sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
CFG=sys.argv[1] if len(sys.argv)>1 else 't_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'
TAG=sys.argv[2] if len(sys.argv)>2 else '16.53'
d=Design(CFG); cells=slice_netlist(d)
io=json.load(open('/opt/homebrew/opt/prjtrellis/share/trellis/database/ECP5/LFE5U-25F/iodb.json'))
pkg=io['packages']['CABGA256']; meta={(m['row'],m['col'],m['pio']):m for m in io['pio_metadata']}
def s(n): return d.s(n)
def step(n,maxd=40):
    """walk back over span/route wires; return (node, status)"""
    seen=set()
    for _ in range(maxd):
        nm=s(n)
        if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$',nm): return n,'CELL'
        if nm.startswith('G_'): return n,'GLOBAL'
        if re.match(r'^J',nm) and not re.match(r'^J[A-D]\d$',nm) and not re.match(r'^J(F|Q)\d$',nm): return n,'SPECIAL'
        u=d.drivenby.get(n)
        if not u:
            f=d.fixed_up(n)
            if len(f)==1: n=f[0]; continue
            return n,'DEAD'
        n=sorted(u)[0]
        if n in seen: return n,'LOOP'
        seen.add(n)
    return n,'DEEP'
out=[]
for pin,v in sorted(pkg.items()):
    y,x,p=v['row'],v['col'],v['pio']
    if (x,y) not in d.T: continue
    w=d.W(x,y,'JPADDO%s'%p); t=d.W(x,y,'JPADDT%s'%p)
    rec=dict(pin=pin,row=y,col=x,pio=p,bank=meta[(y,x,p)].get('bank'),func=meta[(y,x,p)].get('function',''))
    for tag,ww in (('O',w),('T',t)):
        cib=d.fixed_up(ww)[0] if ww and d.fixed_up(ww) else None
        rec[tag+'_cib']=cib
        if cib is not None and cib in d.drivenby:
            src=sorted(d.drivenby[cib])[0]
            n2,st=step(src)
            rec[tag+'_src']=d.nm(n2); rec[tag+'_st']=st
            key=(n2[0],n2[1],s(n2)); c=cells.get(key)
            rec[tag+'_cell']=c
        else:
            rec[tag+'_src']=None; rec[tag+'_st']=None; rec[tag+'_cell']=None
    out.append(rec)
pickle.dump(out,open('padsrc_%s.pkl'%TAG,'wb'))
c=collections.Counter(r['O_st'] for r in out)
print(TAG,'output-pad driver resolution:',dict(c))
print('   GLOBAL-driven pads:',[ (r['pin'],r['O_src']) for r in out if r['O_st']=='GLOBAL'])

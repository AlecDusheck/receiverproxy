import sys,re,collections,pickle
sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
CFG=sys.argv[1]; TAG=sys.argv[2]
d=Design(CFG); cells=slice_netlist(d)
loc=collections.defaultdict(set)
for (y,x),cfg in d.tilecfg.items():
    if 'PLC2' not in d.ttypes.get((y,x),()): continue
    for S in 'ABCD':
        m=cfg.get('SLICE%s.MODE'%S)
        if m in ('DPRAM','RAMW'): loc[(x,y)].add(S+':'+m)
print(TAG,'tiles with LUT-RAM:',len(loc))
for k in sorted(loc,key=lambda z:(z[1],z[0])):
    print('   R%02dC%02d %s'%(k[1],k[0],sorted(loc[k])))

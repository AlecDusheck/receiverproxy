import re,collections,pickle,sys
from netlist2 import Design
def slice_netlist(d):
    """returns cells: (x,y,idx) -> dict for LUTs/FFs in PLC2 tiles"""
    cells={}
    for (y,x),cfg in d.tilecfg.items():
        if 'PLC2' not in d.ttypes.get((y,x),()): continue
        for si,S in enumerate('ABCD'):
            mode=cfg.get('SLICE%s.MODE'%S,'LOGIC')
            for k in (0,1):
                idx=si*2+k
                init=cfg.get('SLICE%s.K%d.INIT'%(S,k))
                cells[(x,y,'F%d'%idx)]=dict(kind='LUT',init=init,mode=mode,slice=S,k=k)
                reg=cfg.get('SLICE%s.REG%d.REGSET'%(S,k))
                if reg is not None or cfg.get('SLICE%s.CLKMUX'%S) is not None:
                    cells[(x,y,'Q%d'%idx)]=dict(kind='FF',regset=reg,slice=S,k=k,
                        clkmux=cfg.get('SLICE%s.CLKMUX'%S),lsrmux=cfg.get('SLICE%s.LSRMUX'%S),
                        cemux=cfg.get('SLICE%s.CEMUX'%S),srmode=cfg.get('SLICE%s.SRMODE'%S),
                        sd=cfg.get('SLICE%s.REG%d.SD'%(S,k)),gsr=cfg.get('SLICE%s.GSR'%S),mode=mode)
    return cells

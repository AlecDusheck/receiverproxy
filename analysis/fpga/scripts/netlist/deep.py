import sys,re,collections,pickle
sys.path.insert(0,'.')
from netlist2 import Design
from slices import slice_netlist
from lut import expr
class X:
    def __init__(s_,CFG):
        s_.d=Design(CFG); s_.cells=slice_netlist(s_.d)
    def s(s_,n): return s_.d.s(n)
    def walk(s_,n,maxd=40):
        seen=set()
        for _ in range(maxd):
            nm=s_.s(n)
            if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$',nm): return n,'CELL'
            if nm.startswith('G_'): return n,'GLOBAL'
            u=s_.d.drivenby.get(n)
            if not u:
                f=s_.d.fixed_up(n)
                if len(f)==1: n=f[0]; continue
                return n,'DEAD'
            n=sorted(u)[0]
            if n in seen: return n,'LOOP'
            seen.add(n)
        return n,'DEEP'
    def W(s_,x,y,nm): return s_.d.W(x,y,nm)
    def src(s_,x,y,wname):
        w=s_.W(x,y,wname)
        if w is None or w not in s_.d.drivenby: return None
        return s_.walk(sorted(s_.d.drivenby[w])[0])
    def cone(s_,n,depth,seen=None,out=None,pre='',lbl=''):
        if out is None: out=[]
        if seen is None: seen=set()
        nm=s_.s(n); key=(n[0],n[1],nm); c=s_.cells.get(key)
        tag=s_.d.nm(n)
        if c:
            if c['kind']=='LUT': tag+='  LUT[%s] = %s'%(c['mode'],expr(c['init']) if c['init'] else '?')
            else: tag+='  FF[%s sd=%s reg=%s]'%(c['mode'],c.get('sd'),c.get('regset'))
        out.append(pre+lbl+tag)
        if depth<=0: return out
        if key in seen: out.append(pre+'   (already expanded)'); return out
        seen.add(key)
        m=re.match(r'^F(\d)$',nm)
        if m:
            i=int(m.group(1))
            for p in 'ABCD':
                r=s_.src(n[0],n[1],'%s%d'%(p,i))
                if r: s_.cone(r[0],depth-1,seen,out,pre+'   ',p+': ')
        m=re.match(r'^Q(\d)$',nm)
        if m:
            i=int(m.group(1)); cc=s_.cells.get(key,{})
            if cc.get('sd')=='1':
                r=s_.src(n[0],n[1],'M%d'%i)
                if r: s_.cone(r[0],depth-1,seen,out,pre+'   ','M: ')
            else:
                fn=s_.W(n[0],n[1],'F%d'%i)
                if fn: s_.cone(fn,depth-1,seen,out,pre+'   ','D: ')
            si=0 if (cc.get('slice','A') in 'AB') else 1
            for wn,k in (('CE%d'%si,'CE'),('LSR%d'%si,'LSR')):
                r=s_.src(n[0],n[1],wn)
                if r: out.append(pre+'   .%s = %s'%(k,s_.d.nm(r[0])))
        return out

import itertools
def f_of(init,a,b,c,dd): return init[a+2*b+4*c+8*dd]=='1'
def expr(init,names=('A','B','C','D')):
    """readable sum-of-products with don't-care reduction over the 4 inputs"""
    on=[k for k in range(16) if init[k]=='1']
    if not on: return '0'
    if len(on)==16: return '1'
    # which inputs actually matter
    care=[]
    for i in range(4):
        if any(init[k]!=init[k^(1<<i)] for k in range(16)): care.append(i)
    # brute-force smallest cover of prime implicants over the caring vars
    def lit(i,v): return ('!' if not v else '')+names[i]
    terms=[]
    covered=set()
    # generate all cubes over care vars, keep those that are implicants
    cubes=[]
    for mask in range(1<<len(care)):
        for vals in itertools.product([0,1],repeat=bin(mask).count('1')):
            sel=[care[i] for i in range(len(care)) if mask>>i&1]
            ok=True; mins=[]
            for k in range(16):
                if all((k>>s&1)==v for s,v in zip(sel,vals)):
                    if init[k]!='1': ok=False;break
                    mins.append(k)
            if ok and mins: cubes.append((len(sel),frozenset(mins),tuple(zip(sel,vals))))
    cubes.sort(key=lambda z:(z[0],-len(z[1])))
    need=set(on); out=[]
    for sz,mins,sel in cubes:
        if not need: break
        if mins & need:
            need-=mins
            out.append('&'.join(lit(s,v) for s,v in sel) if sel else '1')
    return ' | '.join(out) if out else '0'

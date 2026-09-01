import re,collections,glob,json
BASES=[4,13,22,33,42,51,60]
def grp(col):
    for i,b in enumerate(BASES):
        if b<=col<=b+8: return i
res={}
for f in sorted(glob.glob('t_*.config')):
    curkey=None; ttype=None
    inst=collections.defaultdict(list)
    for l in open(f):
        l=l.rstrip('\n')
        if l.startswith('.tile'):
            tn,ty=l.split()[1].split(':'); curkey=None; ttype=ty
            m=re.match(r'MIB_R(\d+)C(\d+)$',tn); mm=re.match(r'MIB2?_EBR\d$',ty)
            if m and mm: curkey=(int(m.group(1)),grp(int(m.group(2))))
        elif l.startswith('.'): curkey=None
        elif l and curkey and not l.startswith('arc:'):
            m=re.match(r'(?:enum|word): (EBR\d)\.(\S+) (\S+)',l)
            if m: inst[curkey+(m.group(1),)].append((ttype,m.group(2),m.group(3)))
    print("=== ",f, " EBR instances:",len(inst))
    cls=collections.Counter()
    for k,v in sorted(inst.items()):
        modes=tuple(sorted(x[2] for x in v if x[1]=='MODE'))
        dw=tuple(sorted((x[1],x[2]) for x in v if 'DATA_WIDTH' in x[1]))
        cls[(modes,dw)]+=1
    for k,c in cls.most_common(): print("   x%-3d MODEs=%s  WIDTHS=%s"%(c,k[0],k[1]))
    res[f]={ "R%dG%d.%s"%k : sorted(v) for k,v in sorted(inst.items())}
json.dump(res,open('ebr_detail.json','w'),indent=1)

import re,collections,sys
FILES=[('16.53 PWM','t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'),
       ('10.81 PWM','t_E320_PCB6.0_PWM_FPGA10.81_20230907.config'),
       ('9.53  PWM','t_E320_PCB6.0_PWM_FPGA9.53_20221031.config'),
       ('13.39 Norm','t_E320_PCB6.0_Normal_FPGA13.39_20221101.config'),
       ('6.69  LS0allDA','t_E320_PCB6.1_LS0allDA_FPGA6.69_20220907.config')]
rows=[]
for tag,f in FILES:
    cur=None; c=collections.Counter(); initset=collections.Counter(); ebrmode=collections.Counter()
    lutnz=0; lut0=0; ff=0; ccu2=0; dpram=0; ramw=0; arcs=0; ebr=set(); dsp=collections.Counter()
    for l in open(f):
        l=l.rstrip()
        if l.startswith('.tile'): cur=l.split()[1]; continue
        if l.startswith('.'): cur=None; continue
        if not cur or not l.strip(): continue
        p=l.strip().split()
        if p[0]=='arc:': arcs+=1; continue
        k=p[1] if len(p)>1 else ''
        v=p[2] if len(p)>2 else ''
        if k.endswith('.INIT') and '.K' in k:
            if v=='0'*16: lut0+=1
            elif v!='1'*16: lutnz+=1
        if re.search(r'SLICE[A-D]\.REG[01]\.REGSET',k): ff+=1
        if re.search(r'SLICE[A-D]\.MODE',k):
            if v=='CCU2': ccu2+=1
            elif v=='DPRAM': dpram+=1
            elif v=='RAMW': ramw+=1
        if re.match(r'EBR\d\.MODE',k): ebr.add((cur,k)); ebrmode[v]+=1
        if re.match(r'(MULT18|ALU54)_\d\.MODE',k): dsp[v]+=1
        if re.match(r'S?IOLOGIC[A-D]\.MODE',k): c['iol_'+v]+=1
    rows.append((tag,dict(arcs=arcs,lut_nonzero=lutnz,lut_zero=lut0,ff=ff,ccu2=ccu2,dpram=dpram,
                          ramw=ramw,ebr=len(ebr)//2,ebrmode=dict(ebrmode),dsp=dict(dsp),**c)))
keys=['arcs','lut_nonzero','lut_zero','ff','ccu2','dpram','ramw','ebr','iol_IREG_OREG','iol_IDDRX1_ODDRX1','iol_IDDRXN']
print("%-16s "%'metric'+' '.join('%12s'%t for t,_ in rows))
for k in keys:
    print("%-16s "%k+' '.join('%12s'%str(r[1].get(k,0)) for r in rows))
for k in ('ebrmode','dsp'):
    print(k+':')
    for t,r in rows: print('   %-16s %s'%(t,r[k]))

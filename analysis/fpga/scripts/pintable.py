import pickle,re,collections,json
pr=pickle.load(open('props_16.53.pkl','rb'))
rows=pickle.load(open('final_16.53.pkl','rb'))
# constant-driven detection: CIB.JxMUX for the PIO's CIB tile
CFG='t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.config'
cfg=collections.defaultdict(dict); cur=None
for l in open(CFG):
    l=l.rstrip()
    if l.startswith('.tile'): cur=l.split()[1]
    elif l.startswith('.'): cur=None
    elif cur and l.strip():
        rc=re.search(r'R(\d+)C(\d+)',cur); p=l.strip().split()
        if rc and p[0]=='enum:': cfg[(int(rc.group(1)),int(rc.group(2)))][p[1]]=p[2]
def cibloc(r):
    y,x,pio=r['row'],r['col'],r['pio']
    if y==0 or y==50: return ((y+1 if y==0 else y-1), x + (0 if pio=='A' else 1)),('JA0MUX','JB0MUX')
    cx = 1 if x==0 else 71
    ry = y if pio in 'AB' else y+2
    return (ry,cx), (('JA0MUX','JB0MUX') if pio in 'AC' else ('JA3MUX','JB3MUX'))
def edge(r): return 'TOP' if r['row']==0 else 'RIGHT' if r['col']==72 else 'BOTTOM' if r['row']==50 else 'LEFT'
def direction(r,const):
    if r['RX0'] or r['RX1']: return 'IN-DDR'
    if r['O'] and r['D1']: return 'OUT-DDR'
    if r['O'] and r['T']: return 'BIDIR'
    if r['O']: return 'OUT'
    if r['iol'].get('MODE')=='IDDRX1_ODDRX1' and pr[r['pin']].get('DATAMUX_ODDR'): return 'OUT-DDR(clk)'
    if r['I']: return 'IN'
    if const is not None: return 'OUT-const%s'%const
    p=pr[r['pin']]
    if p.get('BASE_TYPE') and not p.get('DRIVE'): return 'IN(cfg only)'
    return '-'
order={'TOP':0,'RIGHT':1,'BOTTOM':2,'LEFT':3}
rows.sort(key=lambda r:(order[edge(r)], r['col'] if edge(r)=='TOP' else (r['row'] if edge(r)=='RIGHT' else (-r['col'] if edge(r)=='BOTTOM' else -r['row'])), r['pio']))
f=open('PINTABLE_16.53.txt','w')
f.write("""Colorlight E120 -- LFE5U-25F CABGA256 -- pin usage decoded from
t_E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.bit (identical in all 5 vendor images except pin R7).

METHOD / CAVEATS
 * DIRECTION comes from the ROUTING GRAPH, not from BASE_TYPE:
     OUT   = the CIB wire that feeds JPADDO<x> is the sink of a SET arc
     BIDIR = the same, AND the CIB wire feeding JPADDT<x> (tri-state) is also driven
     IN    = the CIB wire fed by JDI<x> is the source of a SET arc
     IN-DDR / OUT-DDR = via IOLOGIC JRXDATA0/1 or JTXDATA0/1 (IDDRX1_ODDRX1)
     OUT-const0/1 = no routing, but the CIB input mux (CIB.JA*MUX/JB*MUX) ties the pad to a constant
 * BASE_TYPE names printed by prjtrellis are NOT the real IO standard.  In PICL0/PICR0 only three
   BASE_TYPE bit-patterns exist (none / single-ended-driver / differential-driver) and prjtrellis
   labels the middle one "OUTPUT_SSTL18_II".  In PIOT0/PICL1 the fuzzed BASE_TYPE patterns absorbed
   the DRIVE-strength bits, so a real BIDIR_LVTTL33 pin at DRIVE 4 decodes as "INPUT_LVTTL33".
 * All 8 banks are BANK.VCCIO = 3V3 (3V3 has its own unique bit, verified in BANKREF*/bits.db),
   so every real IO on this part is 3.3 V.  There is no 1.8 V or 1.5 V signalling.
 * Electrical properties below are the UNION over the 2-3 prjtrellis tiles that cover one PIC group
   (top/bottom: PIOA in tile col X, PIOB in tile col X+1; left/right: rows R,R+1,R+2).

""")
f.write("%-5s %-6s %-5s %-9s %-13s %-14s %-6s %-5s %-5s %-5s %s\n"%("PIN","EDGE","BANK","SITE","DIRECTION","IOLOGIC","DRIVE","SLEW","HYST","PULL","FUNCTION / notes"))
for r in rows:
    p=pr[r['pin']]
    loc,muxes=cibloc(r)
    const=None
    if not (r['O'] or r['I'] or r['RX0'] or r['T']):
        v=cfg.get(loc,{}).get('CIB.'+muxes[0])
        if v is not None: const=v
    def g(k): return ','.join(sorted(set(p.get(k,[])))) or '-'
    f.write("%-5s %-6s bank%-1s %-9s %-13s %-14s %-6s %-5s %-5s %-5s %s\n"%(
      r['pin'],edge(r),r['bank'],"R%02dC%02d%s"%(r['row'],r['col'],r['pio']),direction(r,const),
      r['iol'].get('MODE','-'),g('DRIVE'),g('SLEWRATE'),g('HYSTERESIS'),g('PULLMODE'),r['func']))
f.close()
c=collections.Counter()
for r in rows:
    loc,muxes=cibloc(r); const=None
    if not (r['O'] or r['I'] or r['RX0'] or r['T']):
        v=cfg.get(loc,{}).get('CIB.'+muxes[0]);  const=v
    c[(edge(r),direction(r,const))]+=1
for k,v in sorted(c.items()): print(k,v)

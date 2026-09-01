"""Map every block RAM in an ECP5 .config: which pins are driven, by which cell,
on which clock, and where its address/data generators sit on the die.

EBR bel pins are NOT set-arc sinks.  They are reached by a FIXED (non
configurable) connection from a CIB J-pin: JA#/JB#/JC#/JD#/JCE#/JCLK#/JLSR# in
the CIB_EBR tile fan down to JADA#_EBR / JDIA#_EBR / JWEA_EBR / ... .  So the
recipe is: find every driven J-pin, follow fixed_dn to see which EBR pin it is,
then walk the set-arc graph back to the driving cell.

Usage:  python3.14 ebrmap.py <config> [> ebr_map.txt]
Needs netlist2.py, slices.py, lut.py on the path (analysis/fpga/scripts/netlist).
"""
import sys, re, collections
sys.path.insert(0, '.')
from netlist2 import Design
from slices import slice_netlist
from lut import expr

CFG = sys.argv[1]
d = Design(CFG)
cells = slice_netlist(d)
S = d.s


def walk(n, maxd=40):
    seen = set()
    for _ in range(maxd):
        nm = S(n)
        if re.match(r'^(F\d|Q\d|F[5X][A-D]_SLICE|OFX\d)$', nm):
            return n, 'CELL'
        if nm.startswith('G_'):
            return n, 'GLOBAL'
        u = d.drivenby.get(n)
        if not u:
            f = d.fixed_up(n)
            if len(f) == 1:
                n = f[0]
                continue
            return n, 'DEAD'
        n = sorted(u)[0]
        if n in seen:
            return n, 'LOOP'
        seen.add(n)
    return n, 'DEEP'


def desc(n, k):
    if k != 'CELL':
        return '%s(%s)' % (d.nm(n), k)
    c = cells.get((n[0], n[1], S(n)))
    if c and c['kind'] == 'LUT' and c['init']:
        return '%s LUT[%s]=%s' % (d.nm(n), c['mode'], expr(c['init']))
    return d.nm(n)


# every driven CIB J-pin that fans down to an EBR bel pin
ebr = collections.defaultdict(dict)
for (x, y), tl in d.T.items():
    for wid in tl.wires:
        s = d.rg.to_str(wid)
        if not re.match(r'^J(A|B|C|D|CE|CLK|LSR)\d+$', s):
            continue
        n = (x, y, wid)
        dn = [z for z in d.fixed_dn(n) if S(z).endswith('_EBR')]
        if not dn:
            continue
        pin = S(dn[0])[1:-4]
        drv = d.drivenby.get(n)
        if drv:
            ebr[(dn[0][0], dn[0][1])][pin] = walk(sorted(drv)[0])

# per-EBR configuration: an instance spans 3 consecutive MIB tiles that all use
# the same EBRn prefix; the bel sits at the first of them (sometimes one left).
cfg = collections.defaultdict(dict)
for (y, x), c in d.tilecfg.items():
    for k, v in c.items():
        if k.startswith('EBR'):
            cfg[(x, y)].setdefault(k, v)

CTRL = ['CLKA', 'CLKB', 'CEA', 'CEB', 'OCEA', 'OCEB', 'WEA', 'WEB',
        'CSA0', 'CSA1', 'CSA2', 'CSB0', 'CSB1', 'CSB2', 'RSTA', 'RSTB']

print('# Block-RAM map for %s' % CFG)
print('# %d EBR sites have at least one driven input pin.' % len(ebr))
for loc in sorted(ebr):
    pins = ebr[loc]
    g = collections.Counter()
    for p in pins:
        g[re.sub(r'\d+', '#', p)] += 1
    cc = {}
    for dx in range(0, 4):
        cc.update({k: v for k, v in cfg.get((loc[0] + dx, loc[1]), {}).items()
                   if 'INIT' not in k})
    print('=' * 78)
    print('EBR@%d,%d  %s' % (loc[0], loc[1],
                             ' '.join('%s=%d' % kv for kv in sorted(g.items()))))
    print('   cfg %s' % {k: v for k, v in sorted(cc.items())})
    for p in CTRL:
        if p in pins:
            print('   %-5s <- %s' % (p, desc(*pins[p])))
    for cls, lbl in (('DI', 'data-in'), ('AD', 'address')):
        pts = [walkres[0] for k, walkres in sorted(pins.items())
               if k.startswith(cls) and walkres[1] == 'CELL']
        if not pts:
            continue
        xs = sorted(p[0] for p in pts)
        ys = sorted(p[1] for p in pts)
        print('   %s generator cells: n=%d  x %d..%d (med %d)  y %d..%d (med %d)'
              % (lbl, len(pts), xs[0], xs[-1], xs[len(xs) // 2],
                 ys[0], ys[-1], ys[len(ys) // 2]))

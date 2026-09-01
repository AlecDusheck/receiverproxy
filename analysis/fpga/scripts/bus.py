#!/usr/bin/env python3
"""Given a candidate register bus (set of Q nets), enumerate every LUT cone whose
support lies inside the bus and report which are pure product terms (= equality
comparators against a constant)."""
import sys, collections
import analyze, cones, terms

def fmt(n): return 'R%dC%d_%s' % n

def analyse(path, top=25, minq=6, maxq=26):
    sys.setrecursionlimit(200000)
    tiles, L, drv, fanout, root = analyze.load(path)
    by_out, sup = cones.supports(L)
    E = terms.Extractor(L)
    sups = {}
    for l in L:
        sups[l['out']] = sup(l, 0, ())
    cnt = collections.Counter()
    for l in L:
        s = sups[l['out']]
        q = frozenset(n for n in s if n[2].startswith('Q'))
        if minq <= len(q) <= maxq and len(s) <= cones.MAXSUP:
            cnt[q] += 1
    out = []
    for bus, n in cnt.most_common(top):
        members = []
        for l in L:
            s = sups[l['out']]
            q = frozenset(n2 for n2 in s if n2[2].startswith('Q'))
            if q and q <= bus and len(s) == len(q):
                t = E.term(l)
                members.append((l, q, t))
        out.append((bus, n, members))
    return tiles, L, out

if __name__ == '__main__':
    path = sys.argv[1]
    tiles, L, out = analyse(path)
    for bus, n, members in out:
        pts = [m for m in members if m[2] and len(m[2]) >= 5]
        print('=' * 70)
        print('BUS %d nets, %d cones, %d pure-product-term cones (>=5 lits)' % (
            len(bus), n, len(pts)))
        print('  nets: ' + ' '.join(sorted(fmt(x) for x in bus)))
        order = sorted(bus)
        for l, q, t in pts:
            v = ''.join(str(t.get(x, '-')) for x in order)
            print('   %-10s %s%d init=%04X  pat=%s' % (l['tile'], l['slice'], l['k'], l['init'], v))

#!/usr/bin/env python3
"""Product-term extraction v2: handles inverted polarity / De-Morgan AND trees.

term(lut) -> (constraints {net:bit}, pol) meaning  out = P  (pol=1)
                                           or     out = ~P (pol=0)
where P = AND over constraints of (net == bit).
"""
import sys, collections
import analyze
from terms import subcube

class Extractor2:
    def __init__(self, luts, maxlits=32):
        self.by_out = {l['out']: l for l in luts}
        self.memo = {}
        self.maxlits = maxlits

    def _build(self, lut, lit, pol, depth, stack):
        constr = {}
        for letter, want in lit.items():
            net = lut['nets'][letter]
            if net is None:
                return None
            child = self.by_out.get(net)
            merged = None
            if child is not None:
                sub = self.term(child, depth + 1, stack + (lut['out'],))
                if sub is not None:
                    c, cpol = sub
                    need = want if cpol == 1 else 1 - want
                    # need P == need
                    if need == 1:
                        merged = c
                    elif len(c) == 1:
                        k = next(iter(c))
                        merged = {k: 1 - c[k]}
            if merged is None:
                merged = {net: want}
            for nn, bb in merged.items():
                if constr.get(nn, bb) != bb:
                    return None
                constr[nn] = bb
            if len(constr) > self.maxlits:
                return None
        return (constr, pol)

    def term(self, lut, depth=0, stack=()):
        key = lut['out']
        if key in self.memo:
            return self.memo[key]
        if key in stack or depth > 10:
            return None
        res = None
        lit = subcube(lut['init'])
        if lit is not None:
            res = self._build(lut, lit, 1, depth, stack)
        if res is None:
            lit = subcube(0xFFFF ^ lut['init'])
            if lit is not None:
                res = self._build(lut, lit, 0, depth, stack)
        self.memo[key] = res
        return res

def fmt(n): return 'R%dC%d_%s' % n

if __name__ == '__main__':
    path = sys.argv[1]
    minn = int(sys.argv[2]) if len(sys.argv) > 2 else 8
    tiles, L, drv, fanout, root = analyze.load(path)
    E = Extractor2(L)
    rows = []
    for l in L:
        r = E.term(l)
        if r and len(r[0]) >= minn:
            rows.append((len(r[0]), l, r))
    rows.sort(key=lambda r: -r[0])
    print('# product-term cones (either polarity) with >=%d constrained nets: %d' % (minn, len(rows)))
    for n, l, (t, pol) in rows:
        qn = sum(1 for k in t if k[2].startswith('Q'))
        print('%-10s %s%d init=%04X pol=%d nets=%d (Q=%d)' % (
            l['tile'], l['slice'], l['k'], l['init'], pol, n, qn))
        for net in sorted(t):
            print('     %-18s = %d' % (fmt(net), t[net]))

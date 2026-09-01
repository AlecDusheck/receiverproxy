#!/usr/bin/env python3
"""Compute transitive combinational support (leaf nets) of every LUT cone and
cluster cones by shared register-bus support."""
import sys, collections
import analyze

MAXDEPTH = 6
MAXSUP = 40

def supports(L):
    by_out = {l['out']: l for l in L}
    memo = {}
    def sup(lut, depth, stack):
        k = lut['out']
        if k in memo: return memo[k]
        if depth >= MAXDEPTH or k in stack:
            return frozenset([k])
        s = set()
        for p in 'ABCD':
            n = lut['nets'][p]
            if n is None: continue
            c = by_out.get(n)
            if c is None:
                s.add(n)
            else:
                s |= sup(c, depth + 1, stack + (k,))
            if len(s) > MAXSUP:
                break
        s = frozenset(s)
        if depth == 0 or len(s) <= MAXSUP:
            memo[k] = s
        return s
    return by_out, sup

if __name__ == '__main__':
    path = sys.argv[1]
    tiles, L, drv, fanout, root = analyze.load(path)
    by_out, sup = supports(L)
    sys.setrecursionlimit(100000)
    rows = []
    for l in L:
        s = sup(l, 0, ())
        q = frozenset(n for n in s if n[2].startswith('Q'))
        rows.append((l, s, q))
    # cluster: count how many cones each Q-support-set serves
    c = collections.Counter()
    for l, s, q in rows:
        if 6 <= len(q) <= 24 and len(s) <= MAXSUP:
            c[q] += 1
    print('# distinct Q-support sets (6..24 Q nets) : %d' % len(c))
    for q, n in c.most_common(30):
        print('n_cones=%-4d qbits=%-3d %s' % (
            n, len(q), ' '.join(sorted('R%dC%d_%s' % x for x in q))))

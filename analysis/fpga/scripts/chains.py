#!/usr/bin/env python3
"""CCU2 carry-chain hunt: a wide compare-to-constant is often built as a chain of
CCU2 slices whose LUTs are product terms (subcubes) over the compared bus."""
import sys, collections
import analyze, extract, terms

def chains(tiles, L):
    lut = {(l['rc'], l['slice'], l['k']): l for l in L}
    ccu2 = {}
    for tname, t in tiles.items():
        if t['type'] != 'PLC2' or not tname.startswith('R'): continue
        rc = extract.tile_rc(tname)
        for s in 'ABCD':
            if t['enums'].get('SLICE%s.MODE' % s) == 'CCU2':
                ccu2[(rc, s)] = True
    # order along a column: chain goes north (row decreasing); within tile A->B->C->D
    cols = collections.defaultdict(list)
    for (rc, s) in ccu2:
        cols[rc[1]].append((rc[0], s))
    out = []
    for c, items in cols.items():
        items.sort(key=lambda x: (-x[0], x[1]))
        cur = []
        prev = None
        for (r, s) in items:
            cont = False
            if prev:
                pr, ps = prev
                if pr == r and 'ABCD'.index(s) == 'ABCD'.index(ps) + 1:
                    cont = True
                elif pr == r + 1 and ps == 'D' and s == 'A':
                    cont = True
            if not cont and cur:
                out.append(cur); cur = []
            for k in (0, 1):
                l = lut.get(((r, c), s, k))
                if l: cur.append(l)
            prev = (r, s)
        if cur: out.append(cur)
    return out

def fmt(n): return 'R%dC%d_%s' % n

if __name__ == '__main__':
    path = sys.argv[1]
    tiles, L, drv, fanout, root = analyze.load(path)
    ch = chains(tiles, L)
    print('# CCU2 chains: %d  (len histogram %s)' % (
        len(ch), sorted(collections.Counter(len(c) for c in ch).items())))
    hits = []
    for c in ch:
        cs = {}
        ok = True
        nlit = 0
        for l in c:
            sc = terms.subcube(l['init'])
            if sc is None or len(sc) < 2:
                ok = False; break
            nlit += len(sc)
            for letter, b in sc.items():
                n = l['nets'][letter]
                if n is None: ok = False; break
                if cs.get(n, b) != b: ok = False
                cs[n] = b
            if not ok: break
        if ok and len(cs) >= 6 and 0 in cs.values() and 1 in cs.values():
            hits.append((c, cs))
    hits.sort(key=lambda h: -len(h[1]))
    print('# chains that are pure constant-compares with >=6 mixed bits: %d' % len(hits))
    for c, cs in hits:
        print('chain len=%d nets=%d  head=%s %s%d' % (
            len(c), len(cs), c[0]['tile'], c[0]['slice'], c[0]['k']))
        for l in c:
            print('    %-10s %s%d init=%04X  %s' % (l['tile'], l['slice'], l['k'], l['init'],
                  ' '.join('%s=%s' % (fmt(l['nets'][p]) if l['nets'][p] else '-', p) for p in 'ABCD')))
        for n in sorted(cs): print('      %-16s = %d' % (fmt(n), cs[n]))

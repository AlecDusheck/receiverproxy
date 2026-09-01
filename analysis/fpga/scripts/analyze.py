#!/usr/bin/env python3
"""Resolve LUT input nets through routing and hunt for constant comparators."""
import re, sys, collections, json
import extract, netlist

PRIM = re.compile(r'^(F[0-7]|Q[0-7]|OFX[0-7]|FCO|JQ|G_|.*PAD.*|JDI|DI[0-9]|IOLDO|MULT|.*DOB?[0-9]|.*DO[AB][0-9]|.*_DI|OSC|CLK|.*JQ.*)')

def is_prim(w):
    return bool(PRIM.match(w[2]))

class Net:
    pass

def resolve_all(tiles):
    drivers, fanout = netlist.build(tiles, -1)
    # single driver assumed
    drv = {k: v[0] for k, v in drivers.items()}
    memo = {}
    def root(w, depth=0):
        if w in memo: return memo[w]
        seen = []
        cur = w
        while True:
            if is_prim(cur) or cur not in drv:
                break
            nxt = drv[cur]
            if nxt in seen or len(seen) > 200:
                break
            seen.append(cur)
            cur = nxt
        for s in seen: memo[s] = cur
        memo[w] = cur
        return cur
    return drv, fanout, root

def load(path):
    tiles = extract.parse(path)
    L = [l for l in extract.luts(tiles) if l['init'] not in (0, 0xFFFF)]
    drv, fanout, root = resolve_all(tiles)
    for l in L:
        rc = extract.tile_rc(l['tile'])
        l['rc'] = rc
        l['out'] = (rc[0], rc[1], 'F%d' % l['lutidx'])
        l['qout'] = (rc[0], rc[1], 'Q%d' % l['lutidx'])
        nets = {}
        for p in 'ABCD':
            s = l['inputs'][p]
            nets[p] = root(netlist.canon(rc, s, -1)) if s else None
        l['nets'] = nets
    return tiles, L, drv, fanout, root

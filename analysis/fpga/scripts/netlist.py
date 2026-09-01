#!/usr/bin/env python3
"""Build a global netlist from a prjtrellis .config: canonicalise relative wire
names, then trace LUT-output -> LUT-input connectivity."""
import re, sys, collections
import extract

PFX = re.compile(r'^([NSEW])([13579])_(.*)$')

def canon(tile_rc, wire, ns_sign=-1):
    """Return (row,col,name) global wire id. ns_sign=-1 means N decreases row."""
    r, c = tile_rc
    m = PFX.match(wire)
    if m:
        d, k, rest = m.group(1), int(m.group(2)), m.group(3)
        if d == 'N': r += ns_sign*k
        elif d == 'S': r -= ns_sign*k
        elif d == 'E': c += k
        elif d == 'W': c -= k
        wire = rest
    return (r, c, wire)

def build(tiles, ns_sign=-1):
    """returns drivers: gwire -> list of (tile, srcname); sinks: gwire -> list"""
    drivers = collections.defaultdict(list)   # gwire -> list of (tile, localsink)
    fanout  = collections.defaultdict(list)   # gwire(src) -> list of (tile, sink gwire)
    for tname, t in tiles.items():
        if not tname.startswith('R'): continue
        rc = extract.tile_rc(tname)
        for sink, src in t['arcs']:
            gs = canon(rc, sink, ns_sign)
            gv = canon(rc, src, ns_sign)
            drivers[gs].append(gv)
            fanout[gv].append(gs)
    return drivers, fanout

def check(tiles):
    for sign in (-1, 1):
        d, f = build(tiles, sign)
        multi = sum(1 for k, v in d.items() if len(set(v)) > 1)
        print('ns_sign=%d: wires=%d multi-driven=%d' % (sign, len(d), multi))

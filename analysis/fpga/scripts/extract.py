#!/usr/bin/env python3
"""Extract SLICE LUT INITs + LUT input connectivity from prjtrellis .config files."""
import sys, re, json, collections

SLICE_IDX = {'A':0,'B':1,'C':2,'D':3}

def parse(path):
    tiles = {}          # tilename -> dict
    cur = None
    with open(path) as f:
        for line in f:
            line = line.rstrip('\n')
            if line.startswith('.tile '):
                name, ttype = line[6:].split(':')
                cur = {'type': ttype, 'words': {}, 'enums': {}, 'arcs': [], 'name': name}
                tiles[name] = cur
            elif line.startswith('.'):
                cur = None
            elif cur is None or not line.strip():
                continue
            elif line.startswith('word: '):
                _, n, b = line.split()
                cur['words'][n] = b
            elif line.startswith('enum: '):
                _, n, v = line.split(None, 2)
                cur['enums'][n] = v
            elif line.startswith('arc: '):
                _, sink, src = line.split()
                cur['arcs'].append((sink, src))
    return tiles

def luts(tiles):
    """yield dict per used LUT"""
    for tname, t in tiles.items():
        if t['type'] != 'PLC2':
            continue
        sinks = set(s for s, _ in t['arcs'])
        srcmap = dict((s, v) for s, v in t['arcs'])
        for sl in 'ABCD':
            for k in (0, 1):
                w = t['words'].get('SLICE%s.K%d.INIT' % (sl, k))
                if w is None:
                    continue
                # LSB-first per prjtrellis word convention (verified separately)
                init_lsb = int(w[::-1], 2)   # w[0] is bit0
                n = SLICE_IDX[sl]*2 + k
                ins = {}
                for p in 'ABCD':
                    key = '%s%d' % (p, n)
                    ins[p] = srcmap.get(key)
                # LUT inputs can be tied to constant 1 via SLICEx.<P><k>MUX = 1
                tied = 0
                for b, p in enumerate('ABCD'):
                    if t['enums'].get('SLICE%s.%s%dMUX' % (sl, p, k)) == '1':
                        tied |= 1 << b
                        ins[p] = None
                eff = 0
                for i in range(16):
                    if (init_lsb >> (i | tied)) & 1:
                        eff |= 1 << i
                yield {
                    'tile': tname, 'slice': sl, 'k': k, 'lutidx': n,
                    'bits': w, 'init': eff, 'raw_init': init_lsb, 'tied1': tied,
                    'init_rev': int(w, 2),
                    'mode': t['enums'].get('SLICE%s.MODE' % sl, 'LOGIC'),
                    'inputs': ins,
                }

def tile_rc(name):
    m = re.match(r'R(\d+)C(\d+)', name)
    return (int(m.group(1)), int(m.group(2))) if m else (0, 0)

if __name__ == '__main__':
    path = sys.argv[1]
    tiles = parse(path)
    L = list(luts(tiles))
    print('total LUT INIT words present:', len(L))
    used = [l for l in L if l['init'] not in (0x0000, 0xFFFF)]
    print('non-trivial (not 0000/FFFF):', len(used))

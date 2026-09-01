#!/usr/bin/env python3
"""Comparator hunt: find AND-of-one-hot-LUT4 clusters = equality compare vs constant."""
import sys, collections, itertools, json
import analyze, extract, netlist

ONEHOT = {1 << i: i for i in range(16)}
# AND gate INITs: key = init, value = tuple of input letters that are ANDed
ANDS = {
    0x8000: 'ABCD',
    0x8080: 'ABC', 0x8800: 'ABD', 0xA000: 'ACD', 0xC000: 'BCD',
    0x8888: 'AB', 0xA0A0: 'AC', 0xC0C0: 'BC',
    0xAA00: 'AD', 0xCC00: 'BD', 0xF000: 'CD',
}

def run(path, verbose=True):
    tiles, L, drv, fanout, root = analyze.load(path)
    by_out = {}
    for l in L:
        by_out[l['out']] = l
    clusters = []
    for l in L:
        if l['init'] not in ANDS:
            continue
        letters = ANDS[l['init']]
        srcs = []
        ok = True
        for p in letters:
            n = l['nets'][p]
            src = by_out.get(n)
            if src is None or src['init'] not in ONEHOT:
                ok = False; break
            srcs.append(src)
        if not ok or len(srcs) < 2:
            continue
        # collect bit constraints: net -> required value
        constr = {}
        conflict = False
        for s in srcs:
            p = ONEHOT[s['init']]
            for i, letter in enumerate('ABCD'):
                net = s['nets'][letter]
                bit = (p >> i) & 1
                if net is None:
                    continue
                if net in constr and constr[net] != bit:
                    conflict = True
                constr[net] = bit
        clusters.append({
            'and_lut': l, 'srcs': srcs, 'constr': constr,
            'nbits': len(constr), 'conflict': conflict,
        })
    return tiles, L, by_out, clusters

def fmt_net(n):
    return 'R%dC%d_%s' % n

if __name__ == '__main__':
    path = sys.argv[1]
    tiles, L, by_out, clusters = run(path)
    clusters.sort(key=lambda c: -c['nbits'])
    print('# AND-of-onehot clusters: %d' % len(clusters))
    for c in clusters:
        l = c['and_lut']
        print('%-10s %s%d init=%04X nbits=%d conflict=%s' % (
            l['tile'], l['slice'], l['k'], l['init'], c['nbits'], c['conflict']))
        for s in c['srcs']:
            print('    src %-10s %s%d init=%04X pos=%2d  A=%s B=%s C=%s D=%s' % (
                s['tile'], s['slice'], s['k'], s['init'], ONEHOT[s['init']],
                *[fmt_net(s['nets'][p]) if s['nets'][p] else '-' for p in 'ABCD']))

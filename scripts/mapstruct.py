#!/usr/bin/env python3
"""Summarise a .rcvbp pixel map as structure rather than bytes.

Each 3-byte entry is (scan_line, slot, 0). What matters is not the individual
values but the shape: where the slot counter wraps, when the scan line
advances, and in which direction each runs. Two configs can differ in every
byte and still describe the same geometry, or agree closely and describe
incompatible ones, so the runs are the thing to compare.
"""
import sys
from mapdump import records


def table(path):
    for rtype, body in records(path):
        if rtype == 0x0a03:
            t = body[2:]
            return [(t[3 * i], t[3 * i + 1]) for i in range(len(t) // 3)]
    raise SystemExit(f'no 0x0a03 in {path}')


def runs(entries):
    """Collapse the index->(line, slot) map into monotonic runs."""
    out, start = [], 0
    for i in range(1, len(entries) + 1):
        cont = (i < len(entries)
                and entries[i][0] == entries[start][0]
                and entries[i][1] - entries[i - 1][1] == entries[start + 1][1] - entries[start][1]
                if i > start + 1 else True)
        if i < len(entries) and cont:
            continue
        out.append((start, i - 1, entries[start], entries[i - 1]))
        start = i
    return out


for path in sys.argv[1:]:
    e = table(path)
    r = runs(e)
    print(f'\n{path}')
    print(f'  {len(e)} entries, {len(r)} monotonic runs')
    for start, end, first, last in r[:70]:
        step = (last[1] - first[1]) // max(end - start, 1) if end > start else 0
        print(f'  idx {start:5d}..{end:5d}: line {first[0]:2d} '
              f'slot {first[1]:3d}->{last[1]:3d} step {step:+d}')
    if len(r) > 70:
        print(f'  ... {len(r) - 70} more runs')
    lines = [x[0] for x in e]
    print(f'  scan line advances every {lines.index(lines[0] + 1) if lines[0] + 1 in lines else "?"} entries')

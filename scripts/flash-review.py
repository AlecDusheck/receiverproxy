#!/usr/bin/env python3
"""Diff a block-7 dump against the day-one dump run by run and check the EEPROM control area.

Usage:
  flash-review.py NOW-BLOCK7.bin [DAY-ONE-PRIMARY-REGION.bin]   default day-one: card-dumps/primary-region.bin
"""
import sys

from flash_review_map import EEPROM

MIRROR = 0xF000   # EEPROM mirror within block 0x07

KNOWN = [(0x0000, MIRROR, 'compiled boot image (written by rxp flash restore-block)')] + [
    (MIRROR + off, ln, f'EEPROM 0x{off:03x}: {label}') for off, ln, label in EEPROM
]


def runs(a, b):
    """Contiguous [start, end) spans where a and b differ."""
    out, start = [], None
    n = min(len(a), len(b))
    for i in range(n):
        if a[i] != b[i]:
            if start is None:
                start = i
        elif start is not None:
            out.append((start, i))
            start = None
    if start is not None:
        out.append((start, n))
    return out


def name(off):
    for base, size, label in KNOWN:
        if base <= off < base + size:
            return label
    return '*** UNIDENTIFIED ***'


now = open(sys.argv[1], 'rb').read()
src = sys.argv[2] if len(sys.argv) > 2 else 'card-dumps/primary-region.bin'
day1 = open(src, 'rb').read()[0x70000:0x80000]

print(f'block 0x07: {sys.argv[1]} vs day-one {src}')
print(f'{len(now)} vs {len(day1)} bytes\n')

d = runs(day1, now)
if not d:
    print('identical to the day-one state')
    raise SystemExit

total = sum(e - s for s, e in d)
print(f'{len(d)} differing runs, {total} bytes total\n')
print(f'{"offset":>10}  {"len":>6}  {"day-one":18} {"now":18} region')
for s, e in d:
    fa = day1[s:min(e, s + 6)].hex(' ')
    no = now[s:min(e, s + 6)].hex(' ')
    print(f'  0x{s:05x}  {e - s:6d}  {fa:18} {no:18} {name(s)}')

# An empty control window is silently fatal: the card drops every pixel.
print('\ncontrol area (EEPROM 0x02, big-endian u16s):')
for label, buf in (('day-one', day1), ('now', now)):
    b = buf[0xF000:0xF00A]
    u = lambda o: int.from_bytes(b[o:o + 2], 'big')
    bad = ' <-- EMPTY WINDOW, the card will drop every pixel' \
        if u(2) == 0xFFFF or u(4) == 0xFFFF else ''
    print(f'  {label:8} startX={u(2):5d} startY={u(4):5d} '
          f'endX={u(6):5d} endY={u(8):5d}{bad}')

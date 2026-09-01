#!/usr/bin/env python3
"""Compare the card's parameter block against the day-one dump, region by region.

Block 0x07 is not one thing. It holds the compiled boot image, the EEPROM
mirror and several regions this project has never identified. Erasing the block
and writing back only the parts we understand silently discards the rest — that
is how the receiver's control-area window ended up as an empty rectangle
(startX = startY = 0xFFFF), which made the card drop every pixel it was sent.

This lists every run that differs from the factory state and flags the ones we
cannot account for, so damage of that kind is visible instead of latent.

Usage: flash-review.py <now-block7.bin> [day-one-primary-region.bin]
"""
import sys

MIRROR = 0xF000   # EEPROM mirror within block 0x07

from flash_review_map import EEPROM

KNOWN = [(0x0000, MIRROR, 'compiled boot image (written by restore-flash)')] + [
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
    print('identical to the factory state')
    raise SystemExit

total = sum(e - s for s, e in d)
print(f'{len(d)} differing runs, {total} bytes total\n')
print(f'{"offset":>10}  {"len":>6}  {"factory":18} {"now":18} region')
for s, e in d:
    fa = day1[s:min(e, s + 6)].hex(' ')
    no = now[s:min(e, s + 6)].hex(' ')
    print(f'  0x{s:05x}  {e - s:6d}  {fa:18} {no:18} {name(s)}')

# The control area specifically, since an empty window is silently fatal.
print('\ncontrol area (EEPROM 0x02, big-endian u16s):')
for label, buf in (('factory', day1), ('now', now)):
    b = buf[0xF000:0xF00A]
    u = lambda o: int.from_bytes(b[o:o + 2], 'big')
    bad = ' <-- EMPTY WINDOW, the card will drop every pixel' \
        if u(2) == 0xFFFF or u(4) == 0xFFFF else ''
    print(f'  {label:8} startX={u(2):5d} startY={u(4):5d} '
          f'endX={u(6):5d} endY={u(8):5d}{bad}')

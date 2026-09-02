#!/usr/bin/env python3
"""Decode the driver-chip register record (0x0a84) of a .rcvbp; compare two.

Usage:
  chipregs.py a.rcvbp             print the register table
  chipregs.py a.rcvbp b.rcvbp     print both side by side and mark differences
"""
import sys

from mapdump import records


def regs(path):
    # 4-byte groups: register address, then one value per colour; ends at an all-zero group.
    for rtype, body in records(path):
        if rtype != 0x0a84:
            continue
        out = {}
        for i in range(0, len(body) - 3, 4):
            addr, vals = body[i], tuple(body[i + 1:i + 4])
            if addr == 0 and not any(vals):
                break
            out[addr] = vals
        return out
    raise SystemExit(f'no 0x0a84 in {path}')


def show(vals):
    return ' '.join(f'{v:02x}' for v in vals)


a = regs(sys.argv[1])
if len(sys.argv) < 3:
    print(sys.argv[1])
    for addr in sorted(a):
        print(f'  0x{addr:02x} = {show(a[addr])}')
    raise SystemExit

b = regs(sys.argv[2])
print(f'A = {sys.argv[1]}\nB = {sys.argv[2]}\n')
print('  reg    A         B         ')
for addr in sorted(set(a) | set(b)):
    va, vb = a.get(addr), b.get(addr)
    mark = '' if va == vb else '   <-- differs'
    print(f'  0x{addr:02x}  {show(va) if va else "--      ":9} '
          f'{show(vb) if vb else "--      ":9}{mark}')
same = sum(1 for k in set(a) & set(b) if a[k] == b[k])
print(f'\n{same} registers agree, {len(set(a) | set(b)) - same} differ or are missing')

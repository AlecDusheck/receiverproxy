#!/usr/bin/env python3
"""Decode the pixel-mapping record (0x0a03) of a .rcvbp into (line, slot) pairs.

The mapping is the card's answer to "where does framebuffer pixel N physically
live", so a wrong one scrambles the image no matter what we transmit. Printing
it as structured entries — rather than as a byte diff — is what makes a
disagreement between two configs legible.
"""
import sys
import zlib


def records(path):
    """Walk the record list: 32-byte file header, then a zlib stream of
    [u16 size_le][u16 type_be][payload], where size counts its own header."""
    blob = open(path, 'rb').read()
    data = zlib.decompress(blob[32:])
    pos = 0
    while pos + 4 <= len(data):
        size = int.from_bytes(data[pos:pos + 2], 'little')
        rtype = int.from_bytes(data[pos + 2:pos + 4], 'big')
        if size < 4:
            break
        yield rtype, data[pos + 4:pos + size]
        pos += size


def main():
    path = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 40
    for rtype, body in records(path):
        if rtype != 0x0a03:
            continue
        head, table = body[:2], body[2:]
        n = len(table) // 3
        print(f'{path}\n  0x0a03: header={head.hex()} {n} entries of 3 bytes')
        print('  idx  bytes     b0   b1   b2   | as LE16(b0,b1)+b2')
        for i in range(min(count, n)):
            b = table[3 * i:3 * i + 3]
            le = b[0] | (b[1] << 8)
            print(f'  {i:4d}  {b.hex()}  {b[0]:3d}  {b[1]:3d}  {b[2]:3d}  | {le:5d} {b[2]:3d}')
        # Summarise the whole table by the distinct values each byte takes.
        for k in range(3):
            vals = sorted({table[3 * i + k] for i in range(n)})
            span = f'{vals[0]}..{vals[-1]}' if len(vals) > 8 else ','.join(map(str, vals))
            print(f'  byte{k}: {len(vals)} distinct ({span})')
        return
    raise SystemExit('no 0x0a03 record')


if __name__ == '__main__':
    main()

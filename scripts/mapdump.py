#!/usr/bin/env python3
"""Print the pixel-mapping record (0x0a03) of a .rcvbp as (line, slot) entries.

Usage:
  mapdump.py FILE.rcvbp [COUNT]    print the first COUNT entries (default 40) and per-byte ranges
"""
import sys
import zlib


def records(path):
    """Yield (type, payload) for each [u16 size_le][u16 type_be][payload] record."""
    blob = open(path, 'rb').read()
    # The header is 32 bytes in the files we write, but vendor corpus files vary,
    # so find the zlib stream itself.
    data = None
    for start in range(0, min(len(blob), 256)):
        if blob[start] == 0x78 and blob[start + 1] in (0x01, 0x5E, 0x9C, 0xDA):
            try:
                data = zlib.decompress(blob[start:])
                break
            except zlib.error:
                continue
    if data is None:
        # Older container: records inline after the 16-byte signature and u32 version, 4-byte CRC trailer.
        data = blob[0x14:-4]
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
        for k in range(3):
            vals = sorted({table[3 * i + k] for i in range(n)})
            span = f'{vals[0]}..{vals[-1]}' if len(vals) > 8 else ','.join(map(str, vals))
            print(f'  byte{k}: {len(vals)} distinct ({span})')
        return
    raise SystemExit('no 0x0a03 record')


if __name__ == '__main__':
    main()

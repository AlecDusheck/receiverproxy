#!/usr/bin/env python3
"""Reproduce the E120 SPI-flash layout analysis.

Usage:  python3 flash-map.py [repo_root]      (default: three levels up)

Outputs, next to this script:
  flash-address-map.txt
  image-match-matrix.tsv
  failing-frames-primary-after-restore.tsv
  failing-frames-primary-region.tsv
Everything is read-only.
"""
import os, sys, glob

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.abspath(
    os.path.join(os.path.dirname(__file__), '..', '..'))
OUT = os.path.dirname(os.path.abspath(__file__))

FW_DIR = os.path.join(ROOT, 'third-party', 'firmware')
DUMP_DIR = os.path.join(ROOT, 'card-dumps')

FR0, FRN, FSZ = 0x17A, 7562, 77       # frame data start, frame count, frame stride


def crc16(data, crc=0):
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def bad_frames(d):
    """Return [(idx, start, end, stored_crc, computed_crc, dummy)] for CRC mismatches."""
    out = []
    for i in range(FRN):
        off = FR0 + i * FSZ
        if off + FSZ > len(d):
            break
        pre = d[0x162:FR0] if i == 0 else d[off - 1:off]
        c = crc16(pre + d[off:off + 74])
        exp = (d[off + 74] << 8) | d[off + 75]
        if c != exp:
            out.append((i, off, off + FSZ - 1, exp, c, d[off + 76]))
    return out


def ff_runs(d, minlen=256):
    out, i, n = [], 0, len(d)
    while i < n:
        if d[i] == 0xFF:
            j = i
            while j < n and d[j] == 0xFF:
                j += 1
            if j - i >= minlen:
                out.append((i, j - 1))
            i = j
        else:
            i += 1
    return out


fw = {os.path.basename(p): open(p, 'rb').read() for p in sorted(glob.glob(FW_DIR + '/*.hex'))}
du = {os.path.basename(p): open(p, 'rb').read() for p in sorted(glob.glob(DUMP_DIR + '/*.bin'))}

# ---- image match matrix (per 64K block, delta = 0 alignment) -------------------
with open(os.path.join(OUT, 'image-match-matrix.tsv'), 'w') as f:
    f.write('dump\tblock\tstart\tend\tpct_FF\t' + '\t'.join(fw) + '\n')
    for dn, d in du.items():
        for blk in range(len(d) // 0x10000):
            s, e = blk * 0x10000, (blk + 1) * 0x10000
            seg = d[s:e]
            row = []
            for n, g in fw.items():
                if s >= len(g):
                    row.append('n/a'); continue
                ee = min(e, len(g))
                row.append('%.6f' % (sum(1 for i in range(s, ee) if d[i] == g[i]) / (ee - s)))
            f.write('%s\t0x%02X\t0x%06X\t0x%06X\t%.1f\t%s\n'
                    % (dn, blk, s, e - 1, 100 * seg.count(0xFF) / len(seg), '\t'.join(row)))

# ---- failing frames -----------------------------------------------------------
for dn in ('primary-after-restore.bin', 'primary-region.bin'):
    b = bad_frames(du[dn])
    with open(os.path.join(OUT, 'failing-frames-%s.tsv' % dn[:-4]), 'w') as f:
        f.write('# %s: %d of %d frames fail the frame CRC\n' % (dn, len(b), FRN))
        f.write('frame\tstart\tend\tstored_crc\tcomputed_crc\tdummy\n')
        for i, s, e, ex, c, du_ in b:
            f.write('%d\t0x%06X\t0x%06X\t0x%04X\t0x%04X\t0x%02X\n' % (i, s, e, ex, c, du_))

# ---- summary ------------------------------------------------------------------
with open(os.path.join(OUT, 'flash-address-map.txt'), 'w') as f:
    for n, g in list(fw.items()) + list(du.items()):
        f.write('%-52s len=0x%06X  bad_frames=%d\n' % (n, len(g), len(bad_frames(g))))
        for a, b2 in ff_runs(g):
            f.write('    0xFF run 0x%06X-0x%06X (%d)\n' % (a, b2, b2 - a + 1))
print('written to', OUT)

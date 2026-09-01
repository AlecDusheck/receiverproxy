#!/usr/bin/env bash
# Does the panel resolve position along each axis?
#
# Sends four half-lit patterns — left, right, top, bottom — interleaved and
# repeated, so supply drift and the card's per-run state toggle average out
# instead of masquerading as signal. Left-vs-right isolates column addressing
# (the shift chain and the pixel mapping); top-vs-bottom isolates row
# addressing (the scan lines) and doubles as a positive control: it is the one
# split already known to produce a visible boundary, so if it fails to register
# here the measurement is too insensitive to trust the column result.
#
# Usage: scripts/axis-test.sh [brightness] [reps]

set -uo pipefail
cd "$(dirname "$0")/.."
bright="${1:-32}"
reps="${2:-3}"
out=/tmp/e120-trials/axis
rm -rf "$out"; mkdir -p "$out"

python3 - "$out" <<'PY'
import zlib, struct, sys, os
w, h, d = 128, 64, sys.argv[1]
def write(name, pred):
    rows = []
    for y in range(h):
        row = bytearray(w * 3)
        for x in range(w):
            if pred(x, y):
                row[x*3:x*3+3] = b'\xff\xff\xff'
        rows.append(b'\x00' + bytes(row))
    def chunk(t, b):
        return struct.pack('>I', len(b)) + t + b + struct.pack('>I', zlib.crc32(t + b) & 0xffffffff)
    open(os.path.join(d, name), 'wb').write(
        b'\x89PNG\r\n\x1a\n'
        + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
        + chunk(b'IDAT', zlib.compress(b''.join(rows)))
        + chunk(b'IEND', b''))
write('left.png',   lambda x, y: x < w // 2)
write('right.png',  lambda x, y: x >= w // 2)
write('top.png',    lambda x, y: y < h // 2)
write('bottom.png', lambda x, y: y >= h // 2)
PY

for i in $(seq 1 "$reps"); do
	for p in left right top bottom; do
		pkill -f 'e120 --brightness' 2>/dev/null
		(./target/debug/e120 --brightness "$bright" image "$out/$p.png" --hold >/dev/null 2>&1 &)
		sleep 2
		scripts/snap-avg.sh "$out/$p-$i.jpg" >/dev/null 2>&1
	done
done
pkill -f 'e120 --brightness' 2>/dev/null

python3 - "$out" "$reps" <<'PY'
import subprocess, tempfile, os, statistics, sys
out, reps = sys.argv[1], int(sys.argv[2])
tmp = tempfile.mkdtemp()

def profile(jpg, axis):
    """Mean luminance along one axis, sampled at panel resolution."""
    raw = os.path.join(tmp, os.path.basename(jpg) + '.rgb')
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg,
                    '-vf', 'scale=128:64', '-frames:v', '1', '-f', 'rawvideo',
                    '-pix_fmt', 'rgb24', '-y', raw], check=True)
    b = open(raw, 'rb').read()
    lum = lambda x, y: sum(b[3*(y*128+x):3*(y*128+x)+3]) / 3
    n = 128 if axis == 'x' else 64
    other = range(64) if axis == 'x' else range(128)
    return [statistics.mean(lum(i, j) if axis == 'x' else lum(j, i) for j in other)
            for i in range(n)]

def compare(a, b, axis, label):
    A = [profile(f'{out}/{a}-{i}.jpg', axis) for i in range(1, reps + 1)]
    B = [profile(f'{out}/{b}-{i}.jpg', axis) for i in range(1, reps + 1)]
    n = len(A[0])
    Am = [statistics.mean(c[i] for c in A) for i in range(n)]
    Bm = [statistics.mean(c[i] for c in B) for i in range(n)]
    diff = [Am[i] - Bm[i] for i in range(n)]
    noise = statistics.mean(
        statistics.pstdev([c[i] for c in A] + [c[i] for c in B]) for i in range(n))
    half = n // 2
    lit = statistics.mean(diff[:half])     # positive if the lit half tracks the data
    unlit = statistics.mean(diff[half:])
    print(f'\n{label}: {a} vs {b}')
    print(f'  noise (within-condition stdev): {noise:6.1f}')
    print(f'  first half mean diff:  {lit:+7.1f}')
    print(f'  second half mean diff: {unlit:+7.1f}')
    print(f'  separation: {lit - unlit:+7.1f}  ({(lit-unlit)/noise if noise else 0:+.1f} sigma)')
    print('  ' + ''.join('+' if v > noise else ('-' if v < -noise else '.') for v in diff))

compare('left', 'right', 'x', 'COLUMN addressing (shift chain / pixel mapping)')
compare('top', 'bottom', 'y', 'ROW addressing (scan lines) [positive control]')
PY

#!/usr/bin/env bash
# How many LEDs does each single-row frame actually light?
#
# Supply current is proportional to lit pixels and, unlike the webcam, has no
# auto-exposure to confuse "one row lit" with "the whole panel lit". Sending a
# frame whose only white pixels are one row should cost 1/64th of the
# full-white current; anything near the full-white figure means the card is
# replicating that row across the panel, and anything at the black figure means
# the row never arrived.
#
# Usage: scripts/rowsweep.sh [brightness] [rows...]

set -uo pipefail
cd "$(dirname "$0")/.."
bright="${1:-60}"; shift || true
rows=("$@")
[ ${#rows[@]} -gt 0 ] || rows=(0 1 2 3 4 8 15 16 17 31 32 33 47 48 63)
d=/tmp/e120-trials/rowsweep; mkdir -p "$d"

python3 - "$d" "${rows[@]}" <<'PY'
import zlib, struct, sys, os
w, h, d = 128, 64, sys.argv[1]
def png(name, pred):
    rows = []
    for y in range(h):
        r = bytearray(w * 3)
        for x in range(w):
            if pred(x, y):
                r[x*3:x*3+3] = b'\xff\xff\xff'
        rows.append(b'\x00' + bytes(r))
    def ch(t, b):
        return struct.pack('>I', len(b)) + t + b + struct.pack('>I', zlib.crc32(t+b) & 0xffffffff)
    open(os.path.join(d, name), 'wb').write(
        b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
        + ch(b'IDAT', zlib.compress(b''.join(rows))) + ch(b'IEND', b''))
png('black.png', lambda x, y: False)
png('white.png', lambda x, y: True)
for r in sys.argv[2:]:
    png(f'r{r}.png', lambda x, y, r=int(r): y == r)
PY

read_a() { ka3005p status 2>/dev/null | sed -n 's/.*Current: *\([0-9.]*\).*/\1/p'; }

measure() {
	pkill -f 'e120 --brightness' 2>/dev/null
	(./target/debug/e120 --brightness "$bright" image "$d/$1.png" --hold >/dev/null 2>&1 &)
	sleep 2
	local a b c
	a=$(read_a); sleep 0.4; b=$(read_a); sleep 0.4; c=$(read_a)
	python3 -c "
import statistics,sys
v=[float(x) for x in sys.argv[1:] if x]
print(f'{statistics.median(v):.3f}')" "$a" "$b" "$c"
}

black=$(measure black)
white=$(measure white)
echo "reference: black ${black} A, white ${white} A  (brightness ${bright})"
echo "expected for one row of 64: $(python3 -c "print(f'{$black + ($white-$black)/64:.3f}')") A"
echo
printf '%6s  %8s  %s\n' row current "rows-worth of light"
for r in "${rows[@]}"; do
	i=$(measure "r$r")
	printf '%6s  %8s  %s\n' "$r" "$i" \
		"$(python3 -c "
d=$white-$black
print('n/a' if d<=0 else f'{($i-$black)/d*64:6.1f}')")"
done
pkill -f 'e120 --brightness' 2>/dev/null

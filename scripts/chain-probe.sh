#!/usr/bin/env bash
# Read out the panel's physical shift-chain length by measurement.
#
# Principle: the armed panel is all-on white (chips at full current, grayscale
# SRAM garbage). Streaming black rows erases chip SRAM only as far as one row
# write reaches down the daisy chain; repeated writes advance a black wavefront
# chip by chip (the staircase seen on camera). Clock rows of the RIGHT length
# and the panel blackens fully and fast; too short and it stalls partway.
#
# One trial: power-cycle -> arm -> vendor packs -> stream black rows of width W
# for a fixed time -> photograph -> print the dark fraction of the panel crop.
#
# Usage: scripts/chain-probe.sh <width> <rows> [stream-seconds]

set -uo pipefail
cd "$(dirname "$0")/.."

W="${1:?usage: chain-probe.sh <width> <rows> [secs]}"
ROWS="${2:?}"
SECS="${3:-15}"
e120=./target/debug/e120
SP_HEXDIR="firmware/derived"
out="/tmp/e120-trials/chain-w$W"
mkdir -p "$out"

cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

ka3005p power off >/dev/null 2>&1
sleep 4
ka3005p power on >/dev/null 2>&1
for _ in $(seq 1 30); do
	sleep 1
	$e120 discover 2>/dev/null | grep -q 'receiver card' && break
done

$e120 set-layout >/dev/null 2>&1
$e120 send-params firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp --chip-only >/dev/null 2>&1
sleep 1
# Vendor-computed data-swap (sub 02) and basic (sub 00) packs, from the
# compiled image's own bodies.
$e120 raw-send --type 0500 --payload "0002$(xxd -p -c 9999 $SP_HEXDIR/dataswap-body.bin)" --pad 258 --wait 0 >/dev/null 2>&1
$e120 raw-send --type 0500 --payload "0000$(xxd -p -c 9999 $SP_HEXDIR/factory-basic-pack-body.bin)" --pad 258 --wait 0 >/dev/null 2>&1
sleep 1
armed=$(cur)

($e120 --width "$W" --brightness 40 probe --rows "$ROWS" --sync --repeat 2000 --color 000000 >/dev/null 2>&1 &)
sleep "$SECS"
streamcur=$(cur)
scripts/snap-avg.sh "$out/after-${SECS}s.jpg" >/dev/null 2>&1
pkill -f "e120 --width" >/dev/null 2>&1

python3 - "$out/after-${SECS}s.jpg" "$W" "$ROWS" "$armed" "$streamcur" <<'PY'
import subprocess, sys, tempfile, os
path, w, rows, armed, streamcur = sys.argv[1:6]
raw = os.path.join(tempfile.mkdtemp(), "raw.rgb")
subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", path,
    "-vf", "crop=420:860:1165:45,scale=42:86", "-frames:v", "1",
    "-f", "rawvideo", "-pix_fmt", "rgb24", "-y", raw], check=True)
d = open(raw, "rb").read()
px = [d[i:i+3] for i in range(0, len(d), 3)]
lum = [(p[0]+p[1]+p[2])/3 for p in px]
dark = sum(1 for v in lum if v < 60) / len(lum)
print(f"W={w} rows={rows}: armed {armed} A -> streaming {streamcur} A, "
      f"dark fraction {dark:.2%}, mean luma {sum(lum)/len(lum):.0f}")
PY

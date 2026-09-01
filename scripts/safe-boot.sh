#!/usr/bin/env bash
# Power the panel on without the max-brightness slam.
#
# The installed boot config arms the driver chips from flash, and until the
# raster content is right the armed panel shows garbage SRAM at full current —
# enough to rail the 5.1 A supply limit and sag the rail into brownout
# strobing. This boots, then immediately cuts brightness and streams black
# until the draw is tame.
#
# Usage: scripts/safe-boot.sh

set -uo pipefail
cd "$(dirname "$0")/.."
e120=./target/debug/e120

cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

ka3005p power on >/dev/null 2>&1
for _ in $(seq 1 30); do
	sleep 1
	$e120 discover 2>/dev/null | grep -q 'receiver card' && break
done
$e120 brightness 20 >/dev/null 2>&1

($e120 --brightness 20 fill 000000 --hold >/dev/null 2>&1 &)
for _ in $(seq 1 30); do
	sleep 2
	c=$(cur)
	echo "current: $c"
	awk "BEGIN{exit !($c < 2.5)}" && break
done
pkill -f "fill 000000" 2>/dev/null
echo "panel up, draw $(cur) A at brightness 20"

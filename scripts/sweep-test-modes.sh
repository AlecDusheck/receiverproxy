#!/usr/bin/env bash
# Sweep the card's built-in test-mode selector (docs/config-protocol.md §16.1:
# the enum is not statically recoverable, so try all 256 values on hardware).
# For each value: send the selector, let the panel settle, then record the PSU
# current and a strobe-proof averaged photo. A selector that renders a real
# pattern shows up as a current step and a structured image.
#
# Usage: scripts/sweep-test-modes.sh [first] [last]
# Output: /tmp/e120-trials/testmode-sweep/{mNNN.jpg,current.csv}

set -uo pipefail
cd "$(dirname "$0")/.."

out="${E120_TRIAL_DIR:-/tmp/e120-trials}/testmode-sweep"
mkdir -p "$out"
e120=./target/debug/e120
first="${1:-0}"
last="${2:-255}"

cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

# Keep the camera out of saturation while patterns render.
$e120 brightness 25 >/dev/null 2>&1

for n in $(seq "$first" "$last"); do
	tag=$(printf 'm%03d' "$n")
	$e120 test-mode "$n" >/dev/null 2>&1
	sleep 1.5
	c=$(cur)
	scripts/snap-avg.sh "$out/$tag.jpg" >/dev/null 2>&1
	echo "$n,$c" >>"$out/current.csv"
	echo "selector $n current $c"
done

# Leave the card in normal (non-test) mode.
$e120 test-mode 0 >/dev/null 2>&1

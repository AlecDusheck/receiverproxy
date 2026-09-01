#!/usr/bin/env bash
# Does the panel follow our content? For each variant spec, on a clean boot:
# push the packs, show white, measure; show black, measure. A panel that is
# rendering draws much more for white than for black, so the ratio is the
# verdict and it is immune to the mistake of comparing two different configs.
#
# Both fills use the SAME loaded config; the power cycle clears driver-chip
# registers, which latch until power-off and otherwise carry state between
# variants.
#
# Usage: scripts/contrast.sh <variant.toml> [variant.toml ...]
# Output: /tmp/e120-trials/contrast.csv + white/black photos per variant.

set -uo pipefail
cd "$(dirname "$0")/.."
e120=./target/debug/e120
out=/tmp/e120-trials/contrast
mkdir -p "$out"
csv=/tmp/e120-trials/contrast.csv
[ -f "$csv" ] || echo "variant,white_a,black_a,ratio,white_score,black_score" >"$csv"

cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

fill() { # $1 = colour, $2 = photo path -> echoes current
	pkill -f "e120 --brightness" 2>/dev/null
	sleep 1
	($e120 --brightness 25 fill "$1" --hold >/dev/null 2>&1 &)
	sleep 3
	local a
	a=$(cur)
	scripts/snap-avg.sh "$2" >/dev/null 2>&1
	pkill -f "e120 --brightness" 2>/dev/null
	echo "$a"
}

for spec in "$@"; do
	name=$(basename "$spec" .toml)
	ka3005p power off >/dev/null 2>&1
	sleep 3
	ka3005p power on >/dev/null 2>&1
	for _ in $(seq 1 20); do
		sleep 1
		$e120 discover 2>/dev/null | grep -q 'receiver card' && break
	done
	$e120 brightness 25 >/dev/null 2>&1
	# The card needs its own geometry announced each boot, or incoming pixel
	# rows have no window to land in.
	$e120 set-layout >/dev/null 2>&1
	if ! $e120 send-params --spec "$spec" >/dev/null 2>&1; then
		echo "$name,,,,send-failed," >>"$csv"
		echo "$name: send-params failed"
		continue
	fi
	sleep 1
	w=$(fill ffffff "$out/$name-white.jpg")
	b=$(fill 000000 "$out/$name-black.jpg")
	ratio=$(awk "BEGIN{printf \"%.2f\", ($b>0)? $w/$b : 0}")
	ws=$(python3 scripts/panel-score.py "$out/$name-white.jpg" 2>/dev/null | cut -d, -f2-3)
	bs=$(python3 scripts/panel-score.py "$out/$name-black.jpg" 2>/dev/null | cut -d, -f2-3)
	echo "$name,$w,$b,$ratio,$ws,$bs" >>"$csv"
	printf '%-14s white=%sA black=%sA ratio=%s  (white %s | black %s)\n' \
		"$name" "$w" "$b" "$ratio" "$ws" "$bs"
done

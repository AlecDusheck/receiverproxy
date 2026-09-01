#!/usr/bin/env bash
# Config sweep with photometric scoring. For each variant spec: push the RAM
# packs, stream RGB bars, photograph and score; then a solid red fill, same.
# Appends to /tmp/e120-trials/sweep.csv. Brightness stays low.
#
# Usage: scripts/sweep.sh [--cycle] variant.toml [variant.toml ...]
#   --cycle   power-cycle the card before each variant (clean chip state)

set -uo pipefail
cd "$(dirname "$0")/.."
e120=./target/debug/e120
out=/tmp/e120-trials/sweep
mkdir -p "$out"
csv=/tmp/e120-trials/sweep.csv
[ -f "$csv" ] || echo "variant,test,image,noise,sat,thirds,mean_r,mean_g,mean_b,amps" >"$csv"

cycle=0
if [ "${1:-}" = "--cycle" ]; then cycle=1; shift; fi
cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

for spec in "$@"; do
	name=$(basename "$spec" .toml)
	if [ "$cycle" = 1 ]; then
		ka3005p power off >/dev/null 2>&1; sleep 3; ka3005p power on >/dev/null 2>&1
		for _ in $(seq 1 20); do sleep 1; $e120 discover 2>/dev/null | grep -q 'receiver card' && break; done
		$e120 brightness 20 >/dev/null 2>&1
	fi
	if ! $e120 send-params --spec "$spec" >/dev/null 2>&1; then
		echo "$name: send-params failed" >&2
		continue
	fi
	sleep 1
	for test in rgb red; do
		pkill -f "e120 --brightness" 2>/dev/null
		if [ "$test" = rgb ]; then
			($e120 --brightness 30 test rgb --hold >/dev/null 2>&1 &)
		else
			($e120 --brightness 30 fill ff0000 --hold >/dev/null 2>&1 &)
		fi
		sleep 3
		img="$out/$name-$test.jpg"
		scripts/snap-avg.sh "$img" >/dev/null 2>&1
		amps=$(cur)
		score=$(python3 scripts/panel-score.py "$img")
		echo "$name,$test,$score,$amps" >>"$csv"
		echo "$name $test: $score amps=$amps"
	done
	pkill -f "e120 --brightness" 2>/dev/null
done

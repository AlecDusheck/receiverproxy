#!/usr/bin/env bash
# Sweep the driver-chip id and measure how the card drives the panel.
#
# The chip id selects the gateware's driver-chip protocol, so it is the one
# input that decides whether the raster reaches the panel at all. For each id:
# push the packs, show a white fill, and read the supply. Current alone
# separates the states — dark ~0.5 A, garbage ~3 A, a real white field is
# high and even — and photos are taken only for ids that leave the dark band.
#
# Usage: scripts/chipsweep.sh <ids-file>   (one id per line, 0x hex or decimal)
# Output: /tmp/e120-trials/chipsweep.csv, photos in /tmp/e120-trials/chipsweep/

set -uo pipefail
cd "$(dirname "$0")/.."
e120=./target/debug/e120
out=/tmp/e120-trials/chipsweep
mkdir -p "$out"
csv=/tmp/e120-trials/chipsweep.csv
[ -f "$csv" ] || echo "chip_id,amps,note" >"$csv"
spec=config/panels/p25-128x64-sm16269s.toml
tmp=$(mktemp -d)

cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

$e120 brightness 25 >/dev/null 2>&1
while read -r id; do
	[ -n "$id" ] || continue
	hex=$(printf '0x%03X' $((id)))
	python3 scripts/variants.py "$spec" "$tmp" "chip$hex" \
		"chip.family_id=$hex" chip.sub_id=0 >/dev/null 2>&1 || continue
	if ! $e120 send-params --spec "$tmp/chip$hex.toml" >/dev/null 2>&1; then
		echo "$hex,,send-failed" >>"$csv"; continue
	fi
	pkill -f "e120 --brightness" 2>/dev/null
	($e120 --brightness 25 fill ffffff --hold >/dev/null 2>&1 &)
	sleep 2.5
	a=$(cur)
	note=""
	# Anything outside the "dark" and "known garbage" bands is worth a look.
	if awk "BEGIN{exit !($a > 0.9)}"; then
		scripts/snap-avg.sh "$out/chip$hex.jpg" >/dev/null 2>&1
		note=$(python3 scripts/panel-score.py "$out/chip$hex.jpg" 2>/dev/null | cut -d, -f2-5)
	fi
	echo "$hex,$a,$note" >>"$csv"
	echo "$hex $a $note"
done < "$1"
pkill -f "e120 --brightness" 2>/dev/null

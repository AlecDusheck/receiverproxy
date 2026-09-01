#!/usr/bin/env bash
# Exercise the panel after a config change and photograph the result.
#
# Run this right after power-cycling the card. It sets the screen size, tries
# the card's own test patterns, then drives white, red, green and blue, taking
# a photo at each step so the panel and the supply's current reading can be
# compared afterwards.
#
# Usage: scripts/verify.sh [output-directory]

set -euo pipefail

out="${1:-verify-run}"
e120="${E120_BIN:-./target/debug/e120}"
here="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$out"

echo "== card state"
"$e120" discover --wait 2 | head -3 | tee "$out/discover.txt"

echo "== screen size"
"$e120" set-layout

echo "== built-in test patterns"
for pattern in 1 2 3 4; do
	"$e120" test-mode "$pattern" >/dev/null
	sleep 2
	"$here/snap.sh" "$out/testmode-$pattern.jpg" >/dev/null
	echo "  pattern $pattern captured"
done
"$e120" test-mode 0 >/dev/null

echo "== driven colours"
for spec in ffffff:white ff0000:red 00ff00:green 0000ff:blue; do
	colour="${spec%%:*}"
	name="${spec##*:}"
	"$e120" fill "$colour" --hold >/dev/null 2>&1 &
	held=$!
	sleep 3
	"$here/snap.sh" "$out/$name.jpg" >/dev/null
	kill "$held" 2>/dev/null || true
	wait "$held" 2>/dev/null || true
	echo "  $name captured"
done

echo "== done; photos in $out"

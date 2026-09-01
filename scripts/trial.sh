#!/usr/bin/env bash
# Run one panel experiment end to end: power-cycle, wait for the card, do
# something, then record what the panel and the PSU did.
#
# The PSU is the honest instrument here — the panel photograph is easy to
# misread, current is not. Idle is ~0.32 A; armed driver chips are ~0.79 A;
# lit LEDs would be well above that.
#
# Usage: scripts/trial.sh <name> [command ...]
#   Runs `command` after the card comes up. With no command, just boots and
#   observes, which is how to see what the flash config alone does.
#
# Only reads the supply and toggles its output. Never changes voltage or
# current limits.

set -uo pipefail
cd "$(dirname "$0")/.."

name="${1:?usage: trial.sh <name> [command ...]}"
shift
out="${E120_TRIAL_DIR:-/tmp/e120-trials}/$name"
mkdir -p "$out"

e120=./target/debug/e120
cur() { ka3005p status 2>/dev/null | grep -oE 'Current: [0-9.]+' | head -1 | awk '{print $2}'; }

echo "=== trial: $name ==="
echo "power cycling..."
ka3005p power off >/dev/null 2>&1
sleep 3
ka3005p power on >/dev/null 2>&1

# Wait for the card rather than guessing at a fixed delay.
for i in $(seq 1 30); do
	sleep 1
	if $e120 discover 2>/dev/null | grep -q 'receiver card'; then break; fi
done
info=$($e120 discover 2>/dev/null | grep 'receiver card' | head -1)
echo "card: ${info:-DID NOT ANSWER}"

sleep 2
boot_current=$(cur)
echo "current at boot (nothing sent): $boot_current"
scripts/snap.sh "$out/1-boot.jpg" >/dev/null 2>&1

if [ $# -gt 0 ]; then
	echo "running: $*"
	"$@" 2>&1 | tail -3
	sleep 3
	after_current=$(cur)
	echo "current after:                  $after_current"
	scripts/snap.sh "$out/2-after.jpg" >/dev/null 2>&1
else
	after_current="$boot_current"
fi

{
	echo "trial:   $name"
	echo "card:    ${info:-no answer}"
	echo "command: ${*:-<none, boot only>}"
	echo "current: boot $boot_current -> after $after_current"
} > "$out/result.txt"
cat "$out/result.txt"
echo "photos in $out"

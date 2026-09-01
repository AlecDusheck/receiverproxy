#!/usr/bin/env bash
# Power the panel with a dead-man's switch.
#
# The panel can pull over 4 A and has railed the supply's 5.1 A limit more than
# once, so it must never be left energised unattended — a forgotten `power on`
# is the failure mode this guards. Turning it on always arms an automatic
# shut-off (default and maximum 10 minutes); turn it on again to extend.
#
# Only reads the supply and toggles its output. Never changes voltage or
# current limits.
#
# Usage:
#   scripts/psu.sh on [minutes]   power on, auto-off after N minutes (max 10)
#   scripts/psu.sh off            power off now, cancel the timer
#   scripts/psu.sh status         supply reading, and time left on the timer
#   scripts/psu.sh extend [min]   restart the timer without power cycling

set -uo pipefail
MAX_MINUTES=10
state=/tmp/e120-psu
mkdir -p "$state"
pidfile="$state/timer.pid"
deadline="$state/deadline"

cancel_timer() {
	if [ -f "$pidfile" ]; then
		kill "$(cat "$pidfile")" 2>/dev/null
		rm -f "$pidfile" "$deadline"
	fi
}

arm_timer() {
	local minutes=$1
	cancel_timer
	date -v+"${minutes}"M +%s >"$deadline" 2>/dev/null || \
		date -d "+${minutes} minutes" +%s >"$deadline"
	setsid bash -c "sleep $((minutes * 60)); ka3005p power off >/dev/null 2>&1; \
		rm -f '$pidfile' '$deadline'" >/dev/null 2>&1 &
	echo $! >"$pidfile"
}

clamp() {
	local m=${1:-$MAX_MINUTES}
	case "$m" in
	'' | *[!0-9]*) m=$MAX_MINUTES ;;
	esac
	[ "$m" -ge 1 ] 2>/dev/null || m=1
	[ "$m" -le "$MAX_MINUTES" ] || m=$MAX_MINUTES
	echo "$m"
}

case "${1:-status}" in
on)
	m=$(clamp "${2:-}")
	ka3005p power on >/dev/null 2>&1
	arm_timer "$m"
	echo "panel on; auto-off in ${m} min"
	;;
extend)
	m=$(clamp "${2:-}")
	arm_timer "$m"
	echo "auto-off pushed out to ${m} min"
	;;
off)
	cancel_timer
	ka3005p power off >/dev/null 2>&1
	echo "panel off"
	;;
status)
	ka3005p status
	if [ -f "$deadline" ]; then
		left=$(( $(cat "$deadline") - $(date +%s) ))
		if [ "$left" -gt 0 ]; then
			echo "auto-off in $((left / 60))m $((left % 60))s"
		else
			echo "auto-off due"
		fi
	else
		echo "no auto-off timer armed"
	fi
	;;
*)
	echo "usage: psu.sh {on [min]|extend [min]|off|status}" >&2
	exit 2
	;;
esac

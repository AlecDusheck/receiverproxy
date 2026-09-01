#!/usr/bin/env bash
# A strobe-proof still: average 24 consecutive camera frames so panel refresh
# artifacts integrate out and only sustained content survives.
set -euo pipefail
out="${1:-snap-avg.jpg}"
device="${E120_CAMERA:-0}"
ffmpeg -hide_banner -loglevel error \
	-f avfoundation -pixel_format uyvy422 -framerate 30 -video_size 1920x1080 \
	-i "$device" -frames:v 24 \
	-vf "tmix=frames=24" -frames:v 1 -update 1 -y "$out"
echo "$out"

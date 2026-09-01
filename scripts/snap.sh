#!/usr/bin/env bash
# Take a single still from the built-in camera, to eyeball the panel.
#
# Usage: scripts/snap.sh [output.jpg]

set -euo pipefail

out="${1:-snap.jpg}"
device="${E120_CAMERA:-0}"

ffmpeg -hide_banner -loglevel error \
	-f avfoundation -pixel_format uyvy422 -framerate 30 -video_size 1920x1080 \
	-i "$device" -frames:v 1 -y "$out"

echo "$out"

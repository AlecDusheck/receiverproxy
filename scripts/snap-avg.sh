#!/usr/bin/env bash
# A strobe-proof still: average camera frames so panel refresh artifacts
# integrate out and only sustained content survives.
#
# The panel multiplexes 1/16, so at any instant only one scan line of each
# group is lit. A single 1/30 s exposure catches an arbitrary phase of that
# cycle and comes out as horizontal banding, which reads as scrambled content
# when it is nothing but the refresh. Averaging over many cycles removes it.
#
# The subtlety that bit us: `tmix=frames=N` emits one output frame per *input*
# frame, and its first outputs average only the frames seen so far — the very
# first is a single frame. Taking `-frames:v 1` after it therefore captured one
# 1/30 s snapshot while claiming to be a 24-frame average, and every photo in
# the project up to 2026-09-01 was affected. The window must be primed: capture
# 2N frames and keep one from after the filter is full.
set -euo pipefail
out="${1:-snap-avg.jpg}"
n="${2:-32}"
device="${E120_CAMERA:-0}"
ffmpeg -hide_banner -loglevel error \
	-f avfoundation -pixel_format uyvy422 -framerate 30 -video_size 1920x1080 \
	-i "$device" -frames:v "$((n * 2))" \
	-vf "tmix=frames=$n,select='gte(n\,$n)'" \
	-frames:v 1 -update 1 -y "$out"
echo "$out"

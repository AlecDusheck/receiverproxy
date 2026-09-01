#!/usr/bin/env bash
# Strobe-o-scope: record ~4s of video and print the per-frame mean brightness
# of the panel crop, so temporal behavior (strobing, flicker, decay) becomes a
# waveform instead of an averaged-away artifact.
#
# Usage: scripts/strobe.sh <name> [seconds]
# Output: /tmp/e120-trials/strobe-<name>.{mov,csv} + a summary line.

set -uo pipefail
name="${1:?usage: strobe.sh <name> [seconds]}"
secs="${2:-4}"
device="${E120_CAMERA:-0}"
out="/tmp/e120-trials/strobe-$name"

ffmpeg -hide_banner -loglevel error \
	-f avfoundation -pixel_format uyvy422 -framerate 30 -video_size 1920x1080 \
	-i "$device" -t "$secs" -c:v libx264 -preset ultrafast -crf 20 -y "$out.mov"

# Per-frame mean luma of the panel region.
ffprobe -hide_banner -loglevel error -f lavfi \
	"movie=$out.mov,crop=420:860:1165:45,signalstats" \
	-show_entries "frame_tags=lavfi.signalstats.YAVG" -of csv=p=0 >"$out.csv"

python3 - "$out.csv" <<'PY'
import statistics
import sys

vals = [float(x.strip().rstrip(",")) for x in open(sys.argv[1]) if x.strip().rstrip(",")]
if not vals:
    sys.exit("no frames")
mean = statistics.mean(vals)
mn, mx = min(vals), max(vals)
sd = statistics.pstdev(vals)
# Count how often brightness crosses the midpoint: 2 crossings per cycle.
mid = (mn + mx) / 2
crossings = sum(1 for a, b in zip(vals, vals[1:]) if (a < mid) != (b < mid))
hz = crossings / 2 / (len(vals) / 30)
print(f"frames {len(vals)}  mean {mean:.1f}  min {mn:.1f}  max {mx:.1f}  "
      f"swing {mx - mn:.1f}  sd {sd:.1f}  ~{hz:.1f} Hz (aliased if fast)")
bars = "".join("▁▂▃▄▅▆▇█"[min(7, int((v - mn) / (mx - mn + 1e-9) * 8))] for v in vals)
print(bars)
PY

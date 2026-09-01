#!/usr/bin/env bash
# A readable photo of the panel: exposed so the LEDs are not clipped, cropped
# to the panel, and magnified with nearest-neighbour so individual pixels stay
# square.
#
# The webcam has no manual exposure, and at normal brightness every LED clips
# to white — which reads as "the panel is white" when it is not. So the shot is
# taken at low panel brightness, which is the one exposure control we have.
#
# Usage: scripts/snap-panel.sh <out.png> [brightness]   (default brightness 6)
# Leaves the panel at the brightness it was given; the caller restores it.

set -uo pipefail
cd "$(dirname "$0")/.."
out="${1:?usage: snap-panel.sh <out.png> [brightness]}"
bright="${2:-6}"
raw="${out%.png}-raw.jpg"

./target/debug/e120 brightness "$bright" >/dev/null 2>&1
sleep 2
scripts/snap-avg.sh "$raw" >/dev/null 2>&1

# Locate the panel, then crop and magnify from the full-resolution frame.
crop=$(python3 - "$raw" <<'PY'
import subprocess, sys, os, tempfile
src = sys.argv[1]
cw, ch = 160, 90
raw = os.path.join(tempfile.mkdtemp(), "r.rgb")
subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", src,
                "-vf", f"scale={cw}:{ch}", "-frames:v", "1", "-f", "rawvideo",
                "-pix_fmt", "rgb24", "-y", raw], check=True)
d = open(raw, "rb").read()
lum = [sum(d[3*i:3*i+3])/3 for i in range(cw*ch)]
thr = max(max(lum) * 0.5, 30)
xs = [i % cw for i, v in enumerate(lum) if v >= thr]
ys = [i // cw for i, v in enumerate(lum) if v >= thr]
if not xs:
    print("")
else:
    sx, sy = 1920/cw, 1080/ch
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    print(f"{int((x1-x0)*sx)}:{int((y1-y0)*sy)}:{int(x0*sx)}:{int(y0*sy)}")
PY
)
[ -n "$crop" ] || { echo "panel not found in frame" >&2; exit 1; }

ffmpeg -hide_banner -loglevel error -i "$raw" \
	-vf "crop=$crop,scale=640:-1:flags=neighbor" -y "$out"
python3 - "$raw" "$crop" <<'PY'
import subprocess, sys, os, tempfile
src, crop = sys.argv[1], sys.argv[2]
raw = os.path.join(tempfile.mkdtemp(), "r.rgb")
subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", src,
                "-vf", f"crop={crop},scale=64:128", "-frames:v", "1",
                "-f", "rawvideo", "-pix_fmt", "rgb24", "-y", raw], check=True)
d = open(raw, "rb").read()
px = [tuple(d[3*i:3*i+3]) for i in range(len(d)//3)]
clip = sum(1 for p in px if max(p) >= 250) / len(px)
dark = sum(1 for p in px if max(p) < 30) / len(px)
print(f"crop={crop} clipped={clip*100:.0f}% off={dark*100:.0f}% "
      f"mean={sum(sum(p) for p in px)/len(px)/3:.0f}")
PY
echo "wrote $out"

#!/usr/bin/env python3
"""Find the panel in the camera frame by differencing lit against blanked.

Thresholding a single frame finds whatever is brightest, which in this room is
the window and a reflection off the turntable lid, not the panel. Only the
panel changes when the card is blanked, so the difference isolates it exactly
and survives both the camera moving and the exposure changing.

Usage: locate.py <lit.jpg> <blank.jpg>   -> prints an ffmpeg crop=w:h:x:y
"""
import os
import subprocess
import sys
import tempfile

W, H = 480, 270


def frame(path):
    tmp = os.path.join(tempfile.mkdtemp(), 'f.rgb')
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', path,
                    '-vf', f'scale={W}:{H}', '-frames:v', '1', '-f', 'rawvideo',
                    '-pix_fmt', 'rgb24', '-y', tmp], check=True)
    d = open(tmp, 'rb').read()
    return [sum(d[3 * i:3 * i + 3]) / 3 for i in range(W * H)]


lit, off = frame(sys.argv[1]), frame(sys.argv[2])
diff = [max(0.0, a - b) for a, b in zip(lit, off)]
peak = max(diff)
if peak < 10:
    raise SystemExit('panel did not change between the two frames')
thr = peak * 0.35
xs = [i % W for i, v in enumerate(diff) if v >= thr]
ys = [i // W for i, v in enumerate(diff) if v >= thr]

# Trim outliers so a stray reflection cannot stretch the box.
xs.sort()
ys.sort()
lo, hi = int(len(xs) * 0.01), int(len(xs) * 0.99)
x0, x1, y0, y1 = xs[lo], xs[hi], ys[lo], ys[hi]
sx, sy = 1920 / W, 1080 / H
print(f'{int((x1 - x0) * sx)}:{int((y1 - y0) * sy)}:{int(x0 * sx)}:{int(y0 * sy)}')
print(f'# peak delta {peak:.0f}, box {x1 - x0}x{y1 - y0} at {x0},{y0} '
      f'(aspect {(x1 - x0) / max(y1 - y0, 1):.2f})', file=sys.stderr)

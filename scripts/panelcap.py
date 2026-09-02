#!/usr/bin/env python3
"""Capture the panel properly, and compare captures by structure.

Three things make a single panel photo on this rig misleading, and each has
produced a false result in this project:

* the panel multiplexes 1/16, so a short exposure catches one scan phase and
  comes out as banding — average many frames (default 90 = 3 s);
* the camera auto-exposes, so absolute level drifts between shots — compare
  structure (Pearson correlation after normalisation), not raw difference;
* bright LEDs clip to 255 — the clip fraction is reported, and anything over a
  few percent means shoot dimmer.

Usage:
  panelcap.py capture <name> [--frames 90]        -> /tmp/e120-trials/cap-<name>.rgb
  panelcap.py compare <name-a> <name-b> [...]     -> correlation table vs the first
"""
import os
import statistics
import subprocess
import sys

CROP = os.environ.get('E120_CROP', '420:750:1060:250')
W, H = 64, 128
DIR = '/tmp/e120-trials'


def capture(name, frames=90):
    jpg = f'{DIR}/cap-{name}.jpg'
    raw = f'{DIR}/cap-{name}.rgb'
    dev = os.environ.get('E120_CAMERA', '0')
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error',
                    '-f', 'avfoundation', '-pixel_format', 'uyvy422',
                    '-framerate', '30', '-video_size', '1920x1080', '-i', dev,
                    '-frames:v', str(frames * 2),
                    '-vf', f"tmix=frames={frames},select='gte(n\\,{frames})'",
                    '-frames:v', '1', '-update', '1', '-y', jpg], check=True)
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg,
                    '-vf', f'crop={CROP},scale={W}:{H}', '-frames:v', '1',
                    '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-y', raw], check=True)
    px = load(name)
    clip = sum(1 for v in px if v >= 250) / len(px)
    print(f'{name}: mean {statistics.mean(px):.0f}  clipped {clip * 100:.1f}%'
          + ('   <-- TOO BRIGHT, shoot dimmer' if clip > 0.03 else ''))


def load(name):
    d = open(f'{DIR}/cap-{name}.rgb', 'rb').read()
    return [sum(d[3 * i:3 * i + 3]) / 3 for i in range(len(d) // 3)]


def corr(a, b):
    ma, mb = statistics.mean(a), statistics.mean(b)
    sa, sb = statistics.pstdev(a), statistics.pstdev(b)
    if sa == 0 or sb == 0:
        return 0.0
    return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / (len(a) * sa * sb)


def compare(names):
    ref = load(names[0])
    print(f'structure correlation vs {names[0]} (identical content ~0.95+; '
          f'a different pattern drops well below):')
    for n in names[1:]:
        p = load(n)
        print(f'  {n:14s} r = {corr(ref, p):+.3f}   mean {statistics.mean(p):.0f}')


if __name__ == '__main__':
    if sys.argv[1] == 'capture':
        frames = int(sys.argv[sys.argv.index('--frames') + 1]) if '--frames' in sys.argv else 90
        capture(sys.argv[2], frames)
    elif sys.argv[1] == 'compare':
        compare(sys.argv[2:])

#!/usr/bin/env python3
"""Sweep panel-spec parameters and score each by whether content reaches the panel.

The test is deliberately blunt: show white, show black, and measure how much
the photographed panel changes. A configuration that drives the module
correctly must produce a large difference; one that does not produces none,
whatever the panel happens to look like. That makes the score robust to the
panel showing something structured-but-wrong, which is where eyeballing has
repeatedly misled this project.

Both photos for a variant are taken back to back, and the score is a difference
between them, so supply drift and camera auto-gain affect both roughly equally.
A same-content control run is included so a nonzero baseline is visible.

Parameters live in the panel TOML; a variant is `section.key=value`.

Usage:
  specsweep.py --spec config/panels/p25-128x64-sm16269s.toml \
      module.serial_clock=6,7,8,10,12,15
"""
import argparse
import os
import statistics
import subprocess
import sys
import tempfile
import time

CROP = os.environ.get('E120_CROP', '420:750:1060:250')


def sh(cmd, **kw):
    return subprocess.run(cmd, shell=isinstance(cmd, str), capture_output=True,
                          text=True, **kw)


def snap(tag):
    jpg = f'/tmp/e120-trials/sweep-{tag}.jpg'
    sh(['scripts/snap-avg.sh', jpg])
    raw = os.path.join(tempfile.mkdtemp(), 'r.rgb')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg, '-vf',
        f'crop={CROP},scale=64:128', '-frames:v', '1', '-f', 'rawvideo',
        '-pix_fmt', 'rgb24', '-y', raw])
    d = open(raw, 'rb').read()
    return [sum(d[3 * i:3 * i + 3]) / 3 for i in range(len(d) // 3)]


def show(exe, spec, img, bright, raster):
    sh(['pkill', '-f', 'e120 --brightness'])
    subprocess.Popen(
        f'{exe} --brightness {bright} image {img} --hold --raster {raster} '
        '>/dev/null 2>&1', shell=True)
    time.sleep(3)


def score(exe, bright, raster, a, b):
    """Mean absolute panel difference between two images, 0-255."""
    show(exe, None, a, bright, raster)
    pa = snap('a')
    show(exe, None, b, bright, raster)
    pb = snap('b')
    sh(['pkill', '-f', 'e120 --brightness'])
    return statistics.mean(abs(x - y) for x, y in zip(pa, pb)), \
        statistics.mean(pa), statistics.mean(pb)


def variant_spec(base, assignments):
    """Write a copy of the spec TOML with `section.key = value` applied."""
    lines = open(base).read().splitlines()
    for dotted, value in assignments:
        section, key = dotted.split('.', 1)
        out, cur, done = [], None, False
        for ln in lines:
            s = ln.strip()
            if s.startswith('[') and s.endswith(']'):
                if cur == section and not done:
                    out.append(f'{key} = {value}')
                    done = True
                cur = s[1:-1]
            if cur == section and s.split('=')[0].strip() == key:
                ln = f'{key} = {value}'
                done = True
            out.append(ln)
        if not done:
            out.append(f'[{section}]')
            out.append(f'{key} = {value}')
        lines = out
    path = os.path.join(tempfile.mkdtemp(), os.path.basename(base))
    open(path, 'w').write('\n'.join(lines) + '\n')
    return path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--spec', required=True)
    ap.add_argument('--exe', default='./target/debug/e120')
    ap.add_argument('--brightness', type=int, default=20)
    ap.add_argument('--raster', default='rows')
    ap.add_argument('--white', default='/tmp/e120-trials/rowsweep/white.png')
    ap.add_argument('--black', default='/tmp/e120-trials/rowsweep/black.png')
    ap.add_argument('sweeps', nargs='+', help='section.key=v1,v2,v3')
    a = ap.parse_args()

    combos = []
    for s in a.sweeps:
        dotted, _, values = s.partition('=')
        for v in values.split(','):
            combos.append((dotted, v))

    print('control: same content twice (this is the noise floor)')
    sh([a.exe, 'send-params', '--spec', a.spec])
    d, ma, mb = score(a.exe, a.brightness, a.raster, a.white, a.white)
    print(f'  white vs white: {d:6.1f}   means {ma:.0f}/{mb:.0f}\n')
    floor = d

    print(f'{"variant":34} {"w-vs-b":>7} {"white":>6} {"black":>6}  verdict')
    results = []
    for dotted, v in combos:
        spec = variant_spec(a.spec, [(dotted, v)])
        r = sh([a.exe, 'send-params', '--spec', spec])
        if r.returncode != 0:
            print(f'{dotted}={v:20} send-params failed: {r.stderr.strip()[:40]}')
            continue
        time.sleep(1)
        d, mw, mb = score(a.exe, a.brightness, a.raster, a.white, a.black)
        verdict = 'CONTENT REACHES THE PANEL' if d > max(3 * floor, 10) else ''
        print(f'{dotted}={v:20} {d:7.1f} {mw:6.0f} {mb:6.0f}  {verdict}')
        results.append((d, dotted, v))

    sh(['pkill', '-f', 'e120 --brightness'])
    if results:
        results.sort(reverse=True)
        d, dotted, v = results[0]
        print(f'\nbest: {dotted}={v} at {d:.1f} (noise floor {floor:.1f})')


main()

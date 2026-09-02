#!/usr/bin/env python3
"""The bench, in one tool: power, arm, stream, capture, compare.

Every experiment on this rig has the same shape — put the card in a known
state, show something, photograph and meter it, and compare against a control
— and every false result this project produced came from doing one of those
steps badly: single-frame photos of a 1/16-multiplexed panel, a current reading
taken while the supply drifted, or two conditions separated by a stream
restart, which flips the card's state on its own. This tool does each step the
right way once, so an experiment is a command line rather than a script.

  bench.py power on|off|cycle|status         dead-man timer via psu.sh
  bench.py boot --spec SPEC                  power-cycle, wait for the card, push the spec's packs
  bench.py locate                            find the panel in frame (lit vs blanked) and remember the crop
  bench.py capture NAME [--frames 90]        averaged, cropped, clip-checked still
  bench.py compare A B ...                   structure correlation of captures vs A
  bench.py tile NAME... --out X.png          side-by-side strip of captures
  bench.py run --spec SPEC [--boot] COND...  the experiment: see below

A condition is `label=pattern[@brightness]`, where pattern is a PNG path or a
built-in: black white red green blue top bottom left right rgbrows gray-N
row-N col-N. `run` shows each condition and records supply current, panel mean,
clip fraction and correlation against the first condition. Two modes:

  --continuous   ONE stream for the whole run: the conditions become segments
                 of a looping video played by `e120 play`, captured mid-segment.
                 Nothing restarts between conditions, so the card's per-restart
                 state toggle cannot masquerade as a result. Default.
  --restart      restart the stream per condition (the old way; only when a
                 condition needs a different raster/row-base flag).

The first condition is repeated at the end as the same-content control; a
difference between conditions only counts if it clears that.
"""
import argparse
import os
import statistics
import subprocess
import sys
import tempfile
import time
import zlib
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
E120 = os.path.join(ROOT, 'target', 'debug', 'e120')
PSU = os.path.join(HERE, 'psu.sh')
DIR = '/tmp/e120-trials'
CROP_FILE = os.path.join(DIR, 'crop.txt')
W, H = 128, 64
os.makedirs(DIR, exist_ok=True)


# ---------------------------------------------------------------- utilities
def sh(cmd, check=False, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, check=check, **kw)


def current():
    out = sh(['ka3005p', 'status']).stdout
    import re
    m = re.search(r'Current:\s*([0-9.]+)', out)
    return float(m.group(1)) if m else float('nan')


def kill_streams():
    sh(['pkill', '-f', 'e120 --brightness'])
    sh(['pkill', '-f', 'e120 -b '])


def crop():
    if 'E120_CROP' in os.environ:
        return os.environ['E120_CROP']
    try:
        return open(CROP_FILE).read().strip()
    except FileNotFoundError:
        return '420:750:1060:250'


# ---------------------------------------------------------------- patterns
def pattern_png(spec):
    """A built-in pattern name → PNG path (existing paths pass through)."""
    if os.path.exists(spec):
        return spec
    name = spec
    path = os.path.join(DIR, 'pat', f'{name}.png')
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if os.path.exists(path):
        return path
    solid = {'black': (0, 0, 0), 'white': (255, 255, 255), 'red': (255, 0, 0),
             'green': (0, 255, 0), 'blue': (0, 0, 255)}

    def px(x, y):
        if name in solid:
            return solid[name]
        if name == 'top':
            return (255,) * 3 if y < H // 2 else (0,) * 3
        if name == 'bottom':
            return (255,) * 3 if y >= H // 2 else (0,) * 3
        if name == 'left':
            return (255,) * 3 if x < W // 2 else (0,) * 3
        if name == 'right':
            return (255,) * 3 if x >= W // 2 else (0,) * 3
        if name == 'rgbrows':
            return [(255, 0, 0), (0, 255, 0), (0, 0, 255)][(y // 4) % 3]
        if name == 'hbands':      # four row bands: R G B W, 16 rows each
            return [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)][y // 16]
        if name == 'vbands':      # four column bands: R G B W, 32 columns each
            return [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)][x // 32]
        if name.startswith('gray-'):
            v = int(name[5:])
            return (v, v, v)
        if name.startswith('row-'):
            return (255,) * 3 if y == int(name[4:]) else (0,) * 3
        if name.startswith('col-'):
            return (255,) * 3 if x == int(name[4:]) else (0,) * 3
        sys.exit(f'unknown pattern {name!r}')

    rows = []
    for y in range(H):
        r = bytearray()
        for x in range(W):
            r.extend(px(x, y))
        rows.append(b'\x00' + bytes(r))

    def chunk(t, b):
        return struct.pack('>I', len(b)) + t + b + struct.pack('>I', zlib.crc32(t + b) & 0xffffffff)
    open(path, 'wb').write(b'\x89PNG\r\n\x1a\n'
                           + chunk(b'IHDR', struct.pack('>IIBBBBB', W, H, 8, 2, 0, 0, 0))
                           + chunk(b'IDAT', zlib.compress(b''.join(rows)))
                           + chunk(b'IEND', b''))
    return path


# ---------------------------------------------------------------- power / boot
def power(action, minutes=10):
    if action == 'cycle':
        sh([PSU, 'off'])
        time.sleep(4)
        sh([PSU, 'on', str(minutes)])
        return wait_for_card()
    r = sh([PSU, action] + ([str(minutes)] if action == 'on' else []))
    print(r.stdout.strip())
    return True


def wait_for_card(timeout=25):
    """Poll discovery until the card answers; returns the discovery line."""
    t0 = time.time()
    while time.time() - t0 < timeout:
        out = sh([E120, 'discover']).stdout
        for ln in out.splitlines():
            if 'receiver card' in ln:
                print(ln.strip())
                return True
        time.sleep(1)
    print('card did not answer discovery', file=sys.stderr)
    return False


def boot(spec, minutes=10):
    kill_streams()
    if not power('cycle', minutes):
        sys.exit(1)
    time.sleep(2)
    r = sh([E120, 'send-params', '--spec', spec])
    print(r.stdout.strip().splitlines()[-1] if r.stdout.strip() else r.stderr.strip())
    time.sleep(1.5)
    print(f'armed: {current():.3f} A')


# ---------------------------------------------------------------- camera
def capture(name, frames=90, quiet=False):
    """Average `frames` camera frames (primed tmix), crop to the panel, sample 64x128."""
    jpg = f'{DIR}/cap-{name}.jpg'
    raw = f'{DIR}/cap-{name}.rgb'
    dev = os.environ.get('E120_CAMERA', '0')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'avfoundation',
        '-pixel_format', 'uyvy422', '-framerate', '30', '-video_size', '1920x1080',
        '-i', dev, '-frames:v', str(frames * 2),
        '-vf', f"tmix=frames={frames},select='gte(n\\,{frames})'",
        '-frames:v', '1', '-update', '1', '-y', jpg], check=True)
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg, '-vf',
        f'crop={crop()},scale=64:128', '-frames:v', '1', '-f', 'rawvideo',
        '-pix_fmt', 'rgb24', '-y', raw], check=True)
    px = load(name)
    clip = sum(1 for v in px if v >= 250) / len(px)
    if not quiet:
        print(f'{name}: mean {statistics.mean(px):.0f}  clipped {clip * 100:.1f}%'
              + ('   <-- too bright, shoot dimmer' if clip > 0.03 else ''))
    return px, clip


def load(name):
    d = open(f'{DIR}/cap-{name}.rgb', 'rb').read()
    return [sum(d[3 * i:3 * i + 3]) / 3 for i in range(len(d) // 3)]


def corr(a, b):
    ma, mb = statistics.mean(a), statistics.mean(b)
    sa, sb = statistics.pstdev(a), statistics.pstdev(b)
    if sa == 0 or sb == 0:
        return 0.0
    return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / (len(a) * sa * sb)


def tile(names, out):
    ins, filt = [], ''
    for i, n in enumerate(names):
        png = f'{DIR}/tile-{n}.png'
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', f'{DIR}/cap-{n}.jpg',
            '-vf', f'crop={crop()},scale=100:200', '-y', png], check=True)
        ins += ['-i', png]
        filt += f'[{i}]'
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error'] + ins +
       ['-filter_complex', f'{filt}hstack=inputs={len(names)}', '-y', out], check=True)
    print(f'tile: {out}   ({" | ".join(names)})')


def locate():
    """Difference a lit frame against a blanked one; only the panel changes."""
    kill_streams()
    sh([E120, 'blank'])
    time.sleep(2)
    off = capture('locate-off', frames=30, quiet=True)
    subprocess.Popen(f'{E120} --brightness 10 image {pattern_png("white")} --hold >/dev/null 2>&1', shell=True)
    time.sleep(3)
    on = capture('locate-on', frames=30, quiet=True)
    kill_streams()
    # Work on the full frames, not the crop: re-read the two jpgs at low res.
    w, h = 480, 270

    def lum(name):
        raw = os.path.join(tempfile.mkdtemp(), 'f.rgb')
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', f'{DIR}/cap-{name}.jpg',
            '-vf', f'scale={w}:{h}', '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-y', raw], check=True)
        d = open(raw, 'rb').read()
        return [sum(d[3 * i:3 * i + 3]) / 3 for i in range(w * h)]
    diff = [max(0.0, a - b) for a, b in zip(lum('locate-on'), lum('locate-off'))]
    thr = max(diff) * 0.5
    xs = sorted(i % w for i, v in enumerate(diff) if v >= thr)
    ys = sorted(i // w for i, v in enumerate(diff) if v >= thr)
    lo, hi = int(len(xs) * 0.02), int(len(xs) * 0.98)
    x0, x1, y0, y1 = xs[lo], xs[hi], ys[lo], ys[hi]
    sx, sy = 1920 / w, 1080 / h
    c = f'{int((x1 - x0) * sx)}:{int((y1 - y0) * sy)}:{int(x0 * sx)}:{int(y0 * sy)}'
    open(CROP_FILE, 'w').write(c)
    print(f'panel crop {c} (aspect {(x1 - x0) / max(1, y1 - y0):.2f}; rotated panel is ~0.5) saved to {CROP_FILE}')


# ---------------------------------------------------------------- experiments
def parse_condition(s, default_bright):
    label, _, rest = s.partition('=')
    if not rest:
        label, rest = s, s
    pat, _, br = rest.partition('@')
    return label, pattern_png(pat), int(br) if br else default_bright


def run(args):
    conds = [parse_condition(c, args.brightness) for c in args.conditions]
    conds.append((conds[0][0] + '-ctl', conds[0][1], conds[0][2]))   # same-content control
    if args.boot:
        boot(args.spec)
    elif args.spec:
        sh([E120, 'send-params', '--spec', args.spec])
        time.sleep(1.5)

    results = []
    if args.continuous:
        # One looping video, one stream. Brightness is per stream, so the
        # first condition's brightness applies to all.
        seg = args.segment
        clips = []
        for label, png, _ in conds:
            mp4 = f'{DIR}/seg-{label}.mp4'
            sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-loop', '1', '-i', png,
                # 30 fps, matching `play --fps 30`: a 10 fps source played at
                # 30 ran the segments three times too fast and the captures
                # landed in the wrong conditions.
                '-t', str(seg), '-r', '30', '-pix_fmt', 'yuv420p', '-y', mp4], check=True)
            clips.append(mp4)
        lst = f'{DIR}/segs.txt'
        open(lst, 'w').write(''.join(f"file '{c}'\n" for c in clips))
        video = f'{DIR}/run.mp4'
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'concat', '-safe', '0',
            '-i', lst, '-c', 'copy', '-y', video], check=True)
        kill_streams()
        subprocess.Popen(f'{E120} --brightness {conds[0][2]} play {video} --looping --fps 30 --raster {args.raster} >/dev/null 2>&1', shell=True)
        t0 = time.time()
        for k, (label, _, _) in enumerate(conds):
            mid = k * seg + seg * 0.3
            while time.time() - t0 < mid:
                time.sleep(0.1)
            a = current()
            px, clip = capture(f'run-{label}', frames=args.frames, quiet=True)
            results.append((label, a, px, clip))
        kill_streams()
    else:
        for label, png, br in conds:
            kill_streams()
            subprocess.Popen(f'{E120} --brightness {br} image {png} --hold {args.stream_flags} >/dev/null 2>&1', shell=True)
            time.sleep(3)
            a = current()
            px, clip = capture(f'run-{label}', frames=args.frames, quiet=True)
            results.append((label, a, px, clip))
        kill_streams()

    ref = results[0][2]
    print(f'\n{"condition":16} {"amps":>6} {"mean":>5} {"clip%":>5}  {"corr vs " + results[0][0]:>16}')
    for label, a, px, clip in results:
        print(f'{label:16} {a:6.3f} {statistics.mean(px):5.0f} {clip * 100:5.1f}  {corr(ref, px):+16.3f}')
    ctl = corr(ref, results[-1][2])
    print(f'\nsame-content control correlates {ctl:+.3f}; a condition only differs if it is well below that.')
    tile([f'run-{r[0]}' for r in results], f'{DIR}/run-tile.png')


# ---------------------------------------------------------------- main
def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('power'); p.add_argument('action', choices=['on', 'off', 'cycle', 'status']); p.add_argument('--minutes', type=int, default=10)
    p = sub.add_parser('boot'); p.add_argument('--spec', required=True)
    sub.add_parser('locate')
    p = sub.add_parser('capture'); p.add_argument('name'); p.add_argument('--frames', type=int, default=90)
    p = sub.add_parser('compare'); p.add_argument('names', nargs='+')
    p = sub.add_parser('tile'); p.add_argument('names', nargs='+'); p.add_argument('--out', default=f'{DIR}/tile.png')
    p = sub.add_parser('run')
    p.add_argument('conditions', nargs='+')
    p.add_argument('--spec')
    p.add_argument('--boot', action='store_true', help='power-cycle and arm with --spec first')
    p.add_argument('--brightness', type=int, default=40)
    p.add_argument('--frames', type=int, default=60)
    p.add_argument('--segment', type=float, default=8.0, help='seconds per condition (continuous)')
    m = p.add_mutually_exclusive_group()
    m.add_argument('--continuous', dest='continuous', action='store_true', default=True)
    m.add_argument('--restart', dest='continuous', action='store_false')
    p.add_argument('--stream-flags', default='', help='extra flags for `image` in --restart mode, e.g. "--raster halves"')
    p.add_argument('--raster', default='rows', help='row packing for the continuous stream: rows|halves|halves-swapped|interleaved')
    a = ap.parse_args()

    if a.cmd == 'power':
        power(a.action, a.minutes)
    elif a.cmd == 'boot':
        boot(a.spec)
    elif a.cmd == 'locate':
        locate()
    elif a.cmd == 'capture':
        capture(a.name, a.frames)
    elif a.cmd == 'compare':
        ref = load(a.names[0])
        for n in a.names[1:]:
            px = load(n)
            print(f'  {n:16s} r = {corr(ref, px):+.3f}   mean {statistics.mean(px):.0f}')
    elif a.cmd == 'tile':
        tile(a.names, a.out)
    elif a.cmd == 'run':
        run(a)


if __name__ == '__main__':
    main()

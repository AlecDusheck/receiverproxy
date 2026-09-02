#!/usr/bin/env python3
"""The bench, in one tool: power, arm, stream, capture, compare.

Each step is done the one way that does not fool the rig (docs/bench-measurement.md):
primed averaged photos of the 1/16-multiplexed panel, current read mid-condition,
one stream for a whole run, and a same-content control.

  bench.py power on|off|cycle|status         dead-man timer via psu.sh
  bench.py boot --spec SPEC                  power-cycle, wait for the card, push the spec's packs
  bench.py locate                            find the panel in frame (lit vs blanked) and remember the crop
  bench.py capture NAME [--frames 90]        averaged, cropped, clip-checked still
  bench.py compare A B ...                   structure correlation of captures vs A
  bench.py tile NAME... --out X.png          side-by-side strip of captures
  bench.py run --spec SPEC [--boot] COND...  the experiment: see below

  bench.py flicker|bands|glitch NAME         experiment-only flicker probes (per-frame
                                             series, rolling-shutter bands, band events);
                                             the 30 fps camera cannot resolve the panel's
                                             flicker (docs/rendering-recipe.md)

A condition is `label=pattern[@brightness]`, where pattern is a PNG path or a
built-in: black white red green blue top bottom left right rgbrows hbands vbands
gray-N row-N col-N. `run` shows each condition and records supply current, panel
mean, clip fraction and correlation against the first condition. Two modes:

  --continuous   ONE stream for the whole run: the conditions become segments
                 of a looping video played by `e120 play`, captured mid-segment.
                 Nothing restarts between conditions, so the card's per-restart
                 state toggle cannot masquerade as a result. Default.
  --restart      restart the stream per condition; only for experiments that
                 need per-condition `image` flags (--stream-flags).

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


def boot(spec, minutes=10, settle=12):
    kill_streams()
    if not power('cycle', minutes):
        sys.exit(1)
    # Discovery answers before boot parameters have loaded; packs pushed earlier are lost.
    time.sleep(settle)
    r = sh([E120, 'send-params', '--spec', spec])
    if r.returncode:
        print(r.stderr.strip(), file=sys.stderr)
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
    # Per-LED crop (~5 camera pixels per LED) for eyeballing individual pixels.
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg, '-vf',
        f'crop={crop()},scale=320:640:flags=neighbor', '-y', f'{DIR}/hi-{name}.png'])
    px = load(name)
    clip = sum(1 for v in px if v >= 250) / len(px)
    out = outliers(px)
    if not quiet:
        print(f'{name}: mean {statistics.mean(px):.0f}  clipped {clip * 100:.1f}%  outliers {out}'
              + ('   <-- too bright, shoot dimmer' if clip > 0.03 else ''))
    return px, clip


def outliers(px, thresh=60):
    """LEDs deviating more than `thresh` levels from their 3x3 median."""
    w, h = 64, 128
    n = 0
    for y in range(1, h - 1):
        for x in range(1, w - 1):
            nb = sorted(px[(y + dy) * w + x + dx] for dy in (-1, 0, 1) for dx in (-1, 0, 1) if dy or dx)
            med = (nb[3] + nb[4]) / 2
            if abs(px[y * w + x] - med) > thresh:
                n += 1
    return n


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


def flicker(name, seconds=3.0):
    """Record the panel for a few seconds and report per-frame brightness.

    A steady picture gives a flat series; buffer swaps, scan beating with the
    camera, or a slow refresh show up as periodic modulation. The dominant
    period is estimated from the autocorrelation of the mean series.
    """
    mp4 = f'{DIR}/flk-{name}.mp4'
    dev = os.environ.get('E120_CAMERA', '0')
    n = int(seconds * 30)
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'avfoundation',
        '-pixel_format', 'uyvy422', '-framerate', '30', '-video_size', '1920x1080',
        '-i', dev, '-frames:v', str(n), '-vf', f'crop={crop()},scale=64:128',
        '-c:v', 'libx264', '-qp', '0', '-y', mp4], check=True)
    # A static reference region (wood, left of the panel) gives the camera's own noise.
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'avfoundation',
        '-pixel_format', 'uyvy422', '-framerate', '30', '-video_size', '1920x1080',
        '-i', dev, '-frames:v', str(n), '-vf', 'crop=300:600:600:300,scale=64:128',
        '-c:v', 'libx264', '-qp', '0', '-y', f'{DIR}/flk-{name}-ref.mp4'], check=True)
    def series(path):
        raw = os.path.join(tempfile.mkdtemp(), 'f.rgb')
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', path, '-f', 'rawvideo',
            '-pix_fmt', 'gray', '-y', raw], check=True)
        d = open(raw, 'rb').read()
        fr = 64 * 128
        return d, fr, [sum(d[i * fr:(i + 1) * fr]) / fr for i in range(len(d) // fr)]
    _, _, ref = series(f'{DIR}/flk-{name}-ref.mp4')
    d, fr, means = series(mp4)
    rsd = statistics.pstdev(ref) / max(statistics.mean(ref), 1) * 100
    if len(means) < 8:
        print('too few frames'); return
    m = statistics.mean(means); sd = statistics.pstdev(means)
    # per-pixel temporal std (flicker that a whole-frame mean would average out)
    step = max(1, fr // 512)
    ptsd = statistics.mean(
        statistics.pstdev([d[i * fr + p] for i in range(len(means))])
        for p in range(0, fr, step))
    # dominant lag by autocorrelation
    c = [(x - m) for x in means]
    best, bestv = 0, 0.0
    for lag in range(2, len(c) // 2):
        v = sum(c[i] * c[i + lag] for i in range(len(c) - lag)) / (len(c) - lag)
        if v > bestv:
            best, bestv = lag, v
    print(f'{name}: {len(means)} frames  mean {m:.1f}  frame-to-frame {sd / max(m, 1) * 100:.1f}% '
          f'(camera reference {rsd:.1f}%)  per-pixel temporal std {ptsd:.1f}  '
          f'dominant period {best / 30:.2f} s')
    print('  series: ' + ' '.join(f'{x:.0f}' for x in means[:40]))


def bands(name):
    """Fast panel modulation, read off the rolling shutter of one frame.

    The camera exposes its 1080 rows in sequence over roughly a frame time,
    so a panel modulating at hundreds of Hz appears as horizontal bands in a
    single still. The band period in camera rows, against the readout time,
    gives the modulation frequency; a steady panel gives a flat row profile.
    """
    jpg = f'{DIR}/band-{name}.jpg'
    dev = os.environ.get('E120_CAMERA', '0')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'avfoundation',
        '-pixel_format', 'uyvy422', '-framerate', '30', '-video_size', '1920x1080',
        '-i', dev, '-frames:v', '1', '-update', '1', '-y', jpg], check=True)
    w, h, x, y = (int(v) for v in crop().split(':'))
    raw = os.path.join(tempfile.mkdtemp(), 'f.rgb')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg, '-vf',
        f'crop={w}:{h}:{x}:{y},scale=32:{h}', '-frames:v', '1', '-f', 'rawvideo',
        '-pix_fmt', 'gray', '-y', raw], check=True)
    d = open(raw, 'rb').read()
    rows = [sum(d[r * 32:(r + 1) * 32]) / 32 for r in range(h)]
    # Smooth over the LED pitch (~5 camera rows per LED) so the LED grid is not read as modulation.
    k = 21
    rows = [statistics.mean(rows[max(0, i - k // 2):i + k // 2 + 1]) for i in range(h)]
    m = statistics.mean(rows)
    c = [v - m for v in rows]
    import math
    # Strongest single-frequency DFT bin over band periods of 40..400 camera rows.
    best, bestamp = 0, 0.0
    for period in range(40, 400, 2):
        re = sum(c[i] * math.cos(2 * math.pi * i / period) for i in range(h))
        im = sum(c[i] * math.sin(2 * math.pi * i / period) for i in range(h))
        amp = 2 * math.hypot(re, im) / h
        if amp > bestamp:
            best, bestamp = period, amp
    freq = 1080 / best * 30 if best else 0
    print(f'{name}: strongest band period {best} rows, amplitude {bestamp / max(m, 1) * 100:.1f}% of mean '
          f'≈ {freq:.0f} Hz (lower bound; readout assumed 1/30 s over 1080 rows)')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', jpg, '-vf',
        f'crop={w}:{h}:{x}:{y},scale=160:-1', '-y', f'{DIR}/band-{name}.png'])


def glitch(name, seconds=4.0):
    """Find brief events via the rolling shutter: one frame in N with a band.

    Records the panel crop at full resolution, smooths each frame's row
    profile over the LED pitch, and flags frames whose profile departs from
    the run's median profile by more than the quiet frames do. Prints the
    flagged frames and their spacing.
    """
    mp4 = f'{DIR}/gl-{name}.mp4'
    dev = os.environ.get('E120_CAMERA', '0')
    n = int(seconds * 30)
    w, h, x, y = (int(v) for v in crop().split(':'))
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'avfoundation',
        '-pixel_format', 'uyvy422', '-framerate', '30', '-video_size', '1920x1080',
        '-i', dev, '-frames:v', str(n), '-vf', f'crop={w}:{h}:{x}:{y},scale=16:{h}',
        '-c:v', 'libx264', '-qp', '0', '-y', mp4], check=True)
    raw = os.path.join(tempfile.mkdtemp(), 'f.rgb')
    sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', mp4, '-f', 'rawvideo',
        '-pix_fmt', 'gray', '-y', raw], check=True)
    d = open(raw, 'rb').read()
    fr = 16 * h
    frames = len(d) // fr
    k = 21
    profs = []
    for i in range(frames):
        rows = [sum(d[i * fr + r * 16:i * fr + (r + 1) * 16]) / 16 for r in range(h)]
        profs.append([statistics.mean(rows[max(0, j - k // 2):j + k // 2 + 1]) for j in range(h)])
    profs = profs[6:]            # drop the auto-exposure settling
    med = [statistics.median(p[j] for p in profs) for j in range(h)]
    dev_ = [max(abs(p[j] - med[j]) for j in range(h)) for p in profs]
    floor = statistics.median(dev_)
    thr = max(floor * 3, 6)
    flagged = [i for i, v in enumerate(dev_) if v > thr]
    print(f'{name}: {len(profs)} frames, quiet max-row-deviation {floor:.1f}, threshold {thr:.1f}')
    print(f'  flagged frames: {flagged}')
    if len(flagged) > 1:
        gaps = [b - a for a, b in zip(flagged, flagged[1:])]
        print(f'  spacing (frames): {gaps}  ->  ~{statistics.median(gaps) / 30 * 1000:.0f} ms')
    # keep the worst frame for viewing
    if dev_:
        worst = max(range(len(dev_)), key=lambda i: dev_[i]) + 6
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', mp4, '-vf',
            f"select='eq(n\,{worst})',scale=64:{h // 2}", '-frames:v', '1', '-y', f'{DIR}/gl-{name}-worst.png'])


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
            # -r 30 must match `play --fps 30` or the segments run at the wrong speed.
            sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-loop', '1', '-i', png,
                '-t', str(seg), '-r', '30', '-pix_fmt', 'yuv420p', '-y', mp4], check=True)
            clips.append(mp4)
        lst = f'{DIR}/segs.txt'
        open(lst, 'w').write(''.join(f"file '{c}'\n" for c in clips))
        video = f'{DIR}/run.mp4'
        sh(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'concat', '-safe', '0',
            '-i', lst, '-c', 'copy', '-y', video], check=True)
        kill_streams()
        subprocess.Popen(f'{E120} --brightness {conds[0][2]} play {video} --looping --fps 30 >/dev/null 2>&1', shell=True)
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
    p = sub.add_parser('flicker'); p.add_argument('name'); p.add_argument('--seconds', type=float, default=3.0)
    p = sub.add_parser('bands'); p.add_argument('name')
    p = sub.add_parser('glitch'); p.add_argument('name'); p.add_argument('--seconds', type=float, default=4.0)
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
    p.add_argument('--stream-flags', default='', help='experiment-only: extra flags for `image` in --restart mode')
    a = ap.parse_args()

    if a.cmd == 'power':
        power(a.action, a.minutes)
    elif a.cmd == 'boot':
        boot(a.spec)
    elif a.cmd == 'locate':
        locate()
    elif a.cmd == 'flicker':
        flicker(a.name, a.seconds)
    elif a.cmd == 'bands':
        bands(a.name)
    elif a.cmd == 'glitch':
        glitch(a.name, a.seconds)
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

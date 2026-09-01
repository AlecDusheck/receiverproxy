#!/usr/bin/env python3
"""Score a webcam still of the panel: noise, or a rendered pattern?

Finds the panel itself (the bright rectangle in a dark room) rather than
trusting a fixed crop, so a bumped camera shows up as a bad locate instead
of silently scoring the wall. Then reports, over the panel only:

  noise    mean absolute luma difference between neighbouring cells —
           per-pixel noise scores high, solid fills and bars score low
  sat      mean saturation — noise reads pastel, real colour reads saturated
  thirds   dominant channel of each third along the long axis (RGB bars
           should read as three different letters)
  black    fraction of cells that are essentially off

Usage: panel-score.py <image> [--debug crop.png]
Prints CSV: image,noise,sat,thirds,black,mean_r,mean_g,mean_b,box
"""
import os
import subprocess
import sys
import tempfile

W, H = 32, 64  # analysis grid over the located panel

def rgb_grid(path, w, h, crop=None):
    """Decode `path` (optionally cropped) to a w x h RGB grid."""
    raw = os.path.join(tempfile.mkdtemp(), "raw.rgb")
    vf = f"crop={crop}," if crop else ""
    subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", path,
         "-vf", f"{vf}scale={w}:{h}:flags=area", "-frames:v", "1",
         "-f", "rawvideo", "-pix_fmt", "rgb24", "-y", raw],
        check=True,
    )
    d = open(raw, "rb").read()
    return [tuple(d[3 * i : 3 * i + 3]) for i in range(w * h)]

def locate(path, cw=160, ch=90):
    """Bounding box of the brightest connected region, as ffmpeg crop w:h:x:y."""
    px = rgb_grid(path, cw, ch)
    lum = [sum(p) / 3 for p in px]
    peak = max(lum)
    if peak < 40:
        return None, 0.0
    thr = max(peak * 0.45, 30)
    seen = [False] * (cw * ch)
    best = None
    for start in range(cw * ch):
        if seen[start] or lum[start] < thr:
            continue
        stack, cells = [start], []
        seen[start] = True
        while stack:
            i = stack.pop()
            cells.append(i)
            x, y = i % cw, i // cw
            for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                if 0 <= nx < cw and 0 <= ny < ch:
                    j = ny * cw + nx
                    if not seen[j] and lum[j] >= thr:
                        seen[j] = True
                        stack.append(j)
        if best is None or len(cells) > len(best):
            best = cells
    if not best or len(best) < 40:
        return None, 0.0
    xs = [i % cw for i in best]
    ys = [i // cw for i in best]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    # Inset by a cell to avoid the bloomed edge, then scale to source pixels.
    fill = len(best) / max((x1 - x0 + 1) * (y1 - y0 + 1), 1)
    sx, sy = 1920 / cw, 1080 / ch
    x, y = int((x0 + 0.6) * sx), int((y0 + 0.6) * sy)
    w, h = int((x1 - x0 - 0.2) * sx), int((y1 - y0 - 0.2) * sy)
    if w < 60 or h < 60:
        return None, fill
    return f"{w}:{h}:{x}:{y}", fill

def main():
    args = sys.argv[1:]
    debug = None
    if "--debug" in args:
        i = args.index("--debug")
        debug = args[i + 1]
        del args[i : i + 2]
    path = args[0]

    crop, fill = locate(path)
    if crop is None:
        print(f"{os.path.basename(path)},NA,NA,NA,NA,NA,NA,NA,no-panel-found")
        return 1
    px = rgb_grid(path, W, H, crop)
    if debug:
        subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", path,
             "-vf", f"crop={crop}", "-frames:v", "1", "-y", debug],
            check=True,
        )

    lum = [sum(p) / 3 for p in px]
    noise, n = 0.0, 0
    for y in range(H):
        for x in range(W):
            i = y * W + x
            if x + 1 < W:
                noise += abs(lum[i] - lum[i + 1])
                n += 1
            if y + 1 < H:
                noise += abs(lum[i] - lum[i + W])
                n += 1
    noise /= max(n, 1)
    sat = sum((max(p) - min(p)) / max(max(p), 1) for p in px) / len(px)
    black = sum(1 for v in lum if v < 25) / len(lum)
    mean = tuple(sum(p[c] for p in px) / len(px) for c in range(3))

    thirds = []
    for t in range(3):
        rows = range(t * H // 3, (t + 1) * H // 3)
        m = [sum(px[y * W + x][c] for y in rows for x in range(W)) / (len(rows) * W) for c in range(3)]
        thirds.append("RGB"[m.index(max(m))] if max(m) - min(m) > 12 else "-")

    print(
        f"{os.path.basename(path)},{noise:.1f},{sat:.3f},{''.join(thirds)},"
        f"{black:.2f},{mean[0]:.0f},{mean[1]:.0f},{mean[2]:.0f},{crop}"
    )
    return 0

if __name__ == "__main__":
    sys.exit(main())

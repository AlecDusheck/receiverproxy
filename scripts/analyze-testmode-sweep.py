#!/usr/bin/env python3
"""Rank test-mode sweep photos by how much visible structure the panel shows.

For each /tmp/e120-trials/testmode-sweep/mNNN.jpg: crop the panel, downscale,
and report mean RGB plus two structure scores — the std-dev across row means
and across column means. A solid field scores near zero on both; bars, lines,
or gradients score high on at least one. Prints a ranked table and flags
outliers against the sweep's own median.
"""
import glob
import os
import statistics
import subprocess
import sys
import tempfile

SWEEP = sys.argv[1] if len(sys.argv) > 1 else "/tmp/e120-trials/testmode-sweep"
W, H = 32, 64  # downscaled panel crop (panel is mounted portrait in frame)


def panel(path):
    raw = os.path.join(tempfile.mkdtemp(), "raw.rgb")
    subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", path,
         "-vf", f"crop=420:860:1165:45,scale={W}:{H}", "-frames:v", "1",
         "-f", "rawvideo", "-pix_fmt", "rgb24", "-y", raw],
        check=True)
    d = open(raw, "rb").read()
    px = [[d[3 * (y * W + x) + c] for c in range(3)] for y in range(H) for x in range(W)]
    return px


def stats(px):
    lum = [sum(p) / 3 for p in px]
    mean = statistics.mean(lum)
    rows = [statistics.mean(lum[y * W:(y + 1) * W]) for y in range(H)]
    cols = [statistics.mean(lum[x::W]) for x in range(W)]
    rstd = statistics.pstdev(rows)
    cstd = statistics.pstdev(cols)
    return mean, rstd, cstd


results = []
for f in sorted(glob.glob(os.path.join(SWEEP, "m*.jpg"))):
    n = int(os.path.basename(f)[1:4])
    try:
        results.append((n, *stats(panel(f))))
    except Exception as e:  # noqa: BLE001 - a truncated jpg should not kill the sweep report
        print(f"skip {f}: {e}", file=sys.stderr)

if not results:
    sys.exit("no photos found")

med_mean = statistics.median(r[1] for r in results)
med_r = statistics.median(r[2] for r in results)
med_c = statistics.median(r[3] for r in results)
print(f"photos: {len(results)}  median mean {med_mean:.1f}  row-std {med_r:.1f}  col-std {med_c:.1f}")
print("selector  mean   rowstd colstd  flags")

interesting = []
for n, mean, rstd, cstd in results:
    flags = []
    if abs(mean - med_mean) > 12:
        flags.append("MEAN")
    if rstd - med_r > 6:
        flags.append("ROWS")
    if cstd - med_c > 6:
        flags.append("COLS")
    if flags:
        interesting.append((n, mean, rstd, cstd, "+".join(flags)))

for n, mean, rstd, cstd, fl in interesting:
    print(f"  {n:3d}    {mean:6.1f} {rstd:6.1f} {cstd:6.1f}  {fl}")
if not interesting:
    print("  (no selector differs from the sweep median - all photos look alike)")

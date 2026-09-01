#!/usr/bin/env bash
# A/B test one command's visible effect on the panel, inside a single boot.
#
# Takes a photo, runs the command, takes another, and prints the mean RGB of
# the panel region in both, so "did anything change" gets a number instead of
# an opinion.
#
# Usage: scripts/ab.sh <name> <command ...>

set -uo pipefail
cd "$(dirname "$0")/.."

name="${1:?usage: ab.sh <name> <command ...>}"
shift
out="/tmp/e120-trials/ab-$name"
mkdir -p "$out"

scripts/snap.sh "$out/a.jpg" >/dev/null 2>&1
"$@" >/dev/null 2>&1
sleep 2
scripts/snap.sh "$out/b.jpg" >/dev/null 2>&1

python3 - "$out" <<'PY'
import sys, subprocess, tempfile, os
out = sys.argv[1]

def mean_rgb(path):
    # Crop the panel area (fixed camera) and average it via ffmpeg.
    raw = os.path.join(tempfile.mkdtemp(), 'raw.rgb')
    subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',path,
        '-vf','crop=420:860:1165:45,scale=32:64','-frames:v','1',
        '-f','rawvideo','-pix_fmt','rgb24','-y',raw],check=True)
    d = open(raw,'rb').read()
    n = len(d)//3
    return tuple(sum(d[i::3])//n for i in range(3))

a = mean_rgb(f'{out}/a.jpg')
b = mean_rgb(f'{out}/b.jpg')
delta = sum(abs(x-y) for x,y in zip(a,b))
print(f'before RGB {a}  after RGB {b}  delta {delta}')
print('CHANGED' if delta > 15 else 'no visible change')
PY

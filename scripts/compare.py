#!/usr/bin/env python3
"""Compare panel conditions by supply current, interleaved and repeated.

The card has a per-run state toggle: the same frame sent twice can differ by
more than an amp, and the supply drifts over tens of seconds. Measuring one
condition and then the other therefore attributes drift to the change, which
has produced two false breakthroughs on this bench already. This runs the
conditions round-robin so drift is common to all of them, repeats, and reports
the spread alongside the mean so a difference can be judged against the noise
rather than eyeballed.

A condition is a shell command that starts a stream in the background; it is
killed before the next one starts.

Usage:
  compare.py --reps 4 --settle 2.5 "label=command" "label=command" ...
"""
import argparse
import statistics
import subprocess
import sys
import time


def current():
    out = subprocess.run(['ka3005p', 'status'], capture_output=True, text=True).stdout
    for tok in out.replace(',', ' ').split():
        pass
    import re
    m = re.search(r'Current:\s*([0-9.]+)', out)
    return float(m.group(1)) if m else None


def stop():
    subprocess.run(['pkill', '-f', 'e120 --brightness'], capture_output=True)
    subprocess.run(['pkill', '-f', 'e120 -b'], capture_output=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--reps', type=int, default=4)
    ap.add_argument('--settle', type=float, default=2.5)
    ap.add_argument('--samples', type=int, default=3)
    ap.add_argument('conditions', nargs='+')
    a = ap.parse_args()

    conds = []
    for c in a.conditions:
        label, _, cmd = c.partition('=')
        if not cmd:
            sys.exit(f'condition {c!r} must be label=command')
        conds.append((label, cmd))

    readings = {label: [] for label, _ in conds}
    for rep in range(a.reps):
        for label, cmd in conds:
            stop()
            subprocess.Popen(f'{cmd} >/dev/null 2>&1', shell=True)
            time.sleep(a.settle)
            vals = []
            for _ in range(a.samples):
                v = current()
                if v is not None:
                    vals.append(v)
                time.sleep(0.3)
            if vals:
                readings[label].append(statistics.median(vals))
            print(f'  rep {rep + 1} {label}: '
                  f'{statistics.median(vals) if vals else float("nan"):.3f} A', flush=True)
    stop()

    print('\ncondition            mean      stdev     n')
    stats = {}
    for label, _ in conds:
        v = readings[label]
        if not v:
            continue
        m = statistics.mean(v)
        s = statistics.pstdev(v)
        stats[label] = (m, s)
        print(f'{label:20s} {m:6.3f} A  {s:6.3f}   {len(v)}')

    # A difference only counts if it clears the within-condition spread.
    if len(stats) >= 2:
        noise = statistics.mean(s for _, s in stats.values()) or 1e-9
        print(f'\npooled within-condition stdev: {noise:.3f} A')
        items = sorted(stats.items(), key=lambda kv: kv[1][0])
        lo, hi = items[0], items[-1]
        d = hi[1][0] - lo[1][0]
        print(f'largest gap: {lo[0]} -> {hi[0]} = {d:+.3f} A '
              f'({d / noise:+.1f}x the noise)')
        print('VERDICT: ' + ('a real difference' if abs(d) > 3 * noise
                             else 'indistinguishable from drift'))


main()

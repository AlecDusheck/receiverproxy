#!/usr/bin/env python3
"""Survey record 0x01 +0x0AE (HR_SetMinOE) across the vendor .rcvbp corpus.

Why: the scan-table solver (crates/e120-rcvbp/src/image/scan_table.rs) computes
every PWM bit time as `2^level * minOE / segments`, snapped to 8-unit quanta.
At minOE = 1e-4 every snapped value rounds to zero, so the emitted scan table
carries only the LEVEL ordering and no timing at all. That looked like a
candidate fault, so it was checked against the corpus.

Result (2026-09, 370 files): minOE = 1e-4 in 93 files overall and in 24 of the
29 files that use the modern 764-byte record 0x01. So an all-zero bit-time
field is the vendor's NORMAL output for modern configs, and the factory image
on this very card has exactly the same bytes we generate. This largely rules
minOE out as the cause of the non-rendering panel.

Run from the repo root:  python3 analysis/fpga/minoe_corpus_survey.py
"""

import collections
import glob
import os
import re
import struct
import sys
import zlib

SIG_ZLIB = bytes.fromhex("202019be74234345b1c793039b83aeab")
SIG_INLINE = bytes.fromhex("cb3a3f2152073d45a8d608435f7a6cd5")
MIN_OE = 0x0AE


def load(path):
    data = open(path, "rb").read()
    if data[:16] == SIG_ZLIB:
        return zlib.decompress(data[0x20:])
    if data[:16] == SIG_INLINE:
        return data[0x14:]
    raise ValueError(f"{path}: unknown signature {data[:16].hex()}")


def record_01(buf):
    i = 0
    while i + 4 <= len(buf):
        size = struct.unpack_from("<H", buf, i)[0]
        if size < 4 or i + size > len(buf):
            return None
        if buf[i + 3] == 0x01:
            return buf[i + 4 : i + size]
        i += size
    return None


def main():
    files = sorted(
        set(
            glob.glob("vendor/led-config-files/**/*.rcvbp", recursive=True)
            + glob.glob("third-party/configs/*.rcvbp")
        )
    )
    if not files:
        sys.exit("no .rcvbp files found - run from the repo root")
    by_value = collections.Counter()
    by_len = collections.Counter()
    rows = []
    for path in files:
        try:
            payload = record_01(load(path))
        except Exception:
            continue
        if not payload or len(payload) < MIN_OE + 4:
            continue
        value = round(struct.unpack_from("<f", payload, MIN_OE)[0], 5)
        by_value[value] += 1
        by_len[(len(payload), value == 0.0001)] += 1
        chips = re.findall(
            r"(?:SM|MBI|ICN|ICND|FM|DP|LS|RT|CS|SUM|TC)\d{3,5}[A-Z]*",
            os.path.basename(path).upper(),
        )
        rows.append((os.path.basename(path), len(payload), value, ",".join(chips)))

    print(f"record 0x01 parsed in {len(rows)} of {len(files)} files")
    print("min_oe histogram (top 12):")
    for value, n in by_value.most_common(12):
        print(f"  {value:>12}  {n}")
    print("\n(record 0x01 payload length, min_oe == 1e-4) -> count:")
    for key, n in sorted(by_len.items()):
        print(f"  {key}: {n}")
    return rows


if __name__ == "__main__":
    main()

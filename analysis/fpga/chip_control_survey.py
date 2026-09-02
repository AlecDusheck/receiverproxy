#!/usr/bin/env python3
"""Survey the 20-byte SChipControl block (record 0x01 +0x0C4) across the
vendor .rcvbp corpus, alongside the chip id and the record 0x84 register table.

Why: SChipControl is the driver-chip *serial-protocol descriptor* -- it carries
the LE/LAT command tail lengths and the GCLK/RCLK-per-row counts. See
docs/fpga/chip-protocol-microcode.md.  The decode rests on this survey:

  [1] = 14 in every non-zero block                 pre-activation LE tail
  [3],[4]                                          register / second-command tails
  [5] = 1, [6] = 3 in every non-zero block         data latch, VSYNC tails
  [10:11],[12:13] big-endian                       GCLK/RCLK pulses per row

Cross-checks that make it credible:
  * 0x00FD (SM16380) gives 14/4/8/1/3 -- exactly the open-source SM16380SC
    command enum PREACTIVE=14, CFG1=4, CFG2=8, latch=1, VSYNC=3 -- and ships
    with NO record 0x84, as the tail-selected (non-SH) protocol requires.
  * 0x00E5 (DP3265S) gives tails 5/5 and a 13-register record 0x84 at
    addresses 0x02..0x11 -- matching the open-source DP3265S profile exactly.
  * The block is all-zero for exactly the non-S-PWM chips (9930/9935/2038/6047).
  * GCLK values 33, 67, 129, 257, 513 are (1024 >> n) + 1 or + 3.

Run from the repo root:  python3 analysis/fpga/chip_control_survey.py
"""

import glob
import os
import struct
import sys
import zlib

SIG_ZLIB = bytes.fromhex("202019be74234345b1c793039b83aeab")
SIG_INLINE = bytes.fromhex("cb3a3f2152073d45a8d608435f7a6cd5")


def load(path):
    data = open(path, "rb").read()
    if data[:16] == SIG_ZLIB:
        return zlib.decompress(data[0x20:])
    if data[:16] == SIG_INLINE:
        return data[0x14:]
    return None


def records(buf):
    out, i = [], 0
    while i + 4 <= len(buf):
        size = struct.unpack_from("<H", buf, i)[0]
        if size < 4 or i + size > len(buf):
            break
        out.append((buf[i + 3], buf[i + 2], buf[i + 4 : i + size]))
        i += size
    return out


def main():
    files = sorted(
        set(
            glob.glob("vendor/led-config-files/**/*.rcvbp", recursive=True)
            + glob.glob("third-party/configs/*.rcvbp")
        )
    )
    if not files:
        sys.exit("no .rcvbp corpus found; run from the repo root")
    print(
        "chipid\tsecondary\tchip_control\ttails(1,2,3,4,5,6)\tgclkA\tgclkB"
        "\tregs\taddrs\tfile"
    )
    for path in files:
        try:
            buf = load(path)
        except Exception:
            continue
        if not buf:
            continue
        recs = records(buf)
        r1 = next((v for t, _, v in recs if t == 0x01), None)
        if r1 is None or len(r1) < 0x210:
            continue
        cid = r1[0x036] | (r1[0x204] << 8)
        sec = r1[0x0E9] | (r1[0x205] << 8)
        cc = bytes(r1[0x0C4:0x0D8])
        nreg, addrs = 0, "-"
        for t, _, v in recs:
            if t != 0x84:
                continue
            quads = [i for i in range(0, min(len(v), 256), 4) if v[i : i + 4] != b"\0\0\0\0"]
            nreg = len(quads)
            if quads:
                ids = sorted({v[i] for i in quads})
                addrs = "0x%02x..0x%02x" % (ids[0], ids[-1])
        print(
            "0x%04X\t0x%04X\t%s\t%d,%d,%d,%d,%d,%d\t%d\t%d\t%d\t%s\t%s"
            % (
                cid, sec, cc.hex(" "),
                cc[1], cc[2], cc[3], cc[4], cc[5], cc[6],
                (cc[10] << 8) | cc[11], (cc[12] << 8) | cc[13],
                nreg, addrs, os.path.basename(path),
            )
        )


if __name__ == "__main__":
    main()

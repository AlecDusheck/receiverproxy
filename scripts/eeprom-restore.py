#!/usr/bin/env python3
"""Restore the card's EEPROM from the day-one flash dump.

Erasing flash block 0x07 clears the EEPROM mirror at `0x07F000`, and writing
back only the compiled boot image leaves everything else at 0xFF. That is how
the receiver's control area became an empty rectangle
(startX = startY = 0xFFFF), after which the card drops every pixel sent to it.
Several other runs of real factory data were lost the same way and are still
unidentified — so restore them from the dump rather than from what we think we
understand.

Emits `e120 raw-send` commands built from the factory bytes; run with --commit
to execute them. Read-only without it.

Usage:
  eeprom-restore.py [--dump card-dumps/primary-region.bin] [--commit]
"""
import argparse
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

MIRROR = 0x7F000          # EEPROM mirror inside the primary-region dump

# Every record must be written at its own address and its own length. Writing
# a span that crosses record boundaries is silently ignored by the card — that
# is why a 16-byte write at 0x040 (which is a one-byte record followed by four
# more) changed nothing, while the 42-byte write at 0x002 took immediately.
# Addresses and lengths from docs/eeprom-map.md.
from flash_review_map import EEPROM as RECORDS  # noqa: E402  (see below)


def frame_payload(addr, data):
    """Type 0x1900 EEPROM write: index, opcode 0x85, address, length, data."""
    return (bytes([0x00])                       # frame offset 14
            + b'\x00\x00'                       # receiver index, BE
            + bytes([0x85])                     # write
            + addr.to_bytes(4, 'big')
            + len(data).to_bytes(4, 'big')
            + data).hex()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--dump', default='card-dumps/primary-region.bin')
    ap.add_argument('--commit', action='store_true')
    ap.add_argument('--live', help='a current block-7 dump; only rewrite records that differ')
    ap.add_argument('--exe', default='./target/debug/e120')
    a = ap.parse_args()

    blob = open(a.dump, 'rb').read()
    if len(blob) < MIRROR + 0x1000:
        sys.exit(f'{a.dump} is too short to contain the EEPROM mirror')
    eeprom = blob[MIRROR:MIRROR + 0x1000]

    # Only records that actually differ from the factory state are rewritten;
    # the card's live EEPROM is read back through the flash mirror by
    # scripts/flash-review.py, so this stays idempotent.
    live = open(a.live, 'rb').read() if a.live else None
    for addr, length, note in RECORDS:
        data = eeprom[addr:addr + length]
        if len(data) < length:
            continue
        if live is not None and live[0xF000 + addr:0xF000 + addr + length] == data:
            continue
        payload = frame_payload(addr, data)
        pad = max(0x80, length + 0x12)
        cmd = [a.exe, 'raw-send', '--type', '1900',
               '--pad', str(pad), '--payload', payload, '--wait', '0']
        print(f'0x{addr:03x} +{length:3d}  {data[:8].hex(" "):24} {note}')
        if a.commit:
            subprocess.run(cmd, capture_output=True)
        else:
            print('   ' + ' '.join(cmd[1:]))

    if a.commit:
        # Opcode 0x87 commits the EEPROM to flash, with no data and addr 0
        # (CReceiverOP::SaveEepromFlash). Without it some records read back
        # unchanged: the write lands in the working copy and is then lost.
        subprocess.run([a.exe, 'raw-send', '--type', '1900', '--pad', '128',
                        '--payload', '00000087' + '00000000' * 2, '--wait', '0'],
                       capture_output=True)
        # ReLoadLocalParam: opcode 0x77, data 01 01 00 00 00.
        subprocess.run([a.exe, 'raw-send', '--type', '0600', '--pad', '126',
                        '--payload', '00000077000000000101000000', '--wait', '0'],
                       capture_output=True)
        print('\nwrote and asked the card to reload; power-cycle and verify with:')
        print('  e120 dump-flash --block 7 --out now.bin && '
              'python3 scripts/flash-review.py now.bin')
    else:
        print('\ndry run: nothing sent. Re-run with --commit.')


main()

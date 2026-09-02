#!/usr/bin/env python3
"""Rewrite the card's EEPROM records from the day-one flash dump, one record at a time.

Usage:
  eeprom-restore.py [--dump card-dumps/primary-region.bin] [--live NOW.bin] [--exe ./target/debug/rxp] [--commit]
  Dry run unless --commit. With --live, only records that differ from that block-7 dump are written.
"""
import argparse
import subprocess
import sys
import time

from flash_review_map import EEPROM

MIRROR = 0x7F000          # EEPROM mirror inside the primary-region dump

# Records above 0x0fd use opcodes 0x45/0x88, not 0x85; the map keeps them for labelling only.
RECORDS = [r for r in EEPROM if r[0] <= 0x0fd]


def frame_payload(addr, data):
    """Type 0x1900 EEPROM write: index, opcode 0x85, address, length, data."""
    # Broadcast index: a write to index 0 is ignored while the cabinet record is corrupt.
    return (bytes([0x00])                       # frame offset 14
            + b'\xff\xff'                       # receiver index, BE (broadcast)
            + bytes([0x85])                     # write
            + addr.to_bytes(4, 'big')
            + len(data).to_bytes(4, 'big')
            + data).hex()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--dump', default='card-dumps/primary-region.bin')
    ap.add_argument('--commit', action='store_true')
    ap.add_argument('--live', help='a current block-7 dump; only rewrite records that differ')
    ap.add_argument('--exe', default='./target/debug/rxp')
    a = ap.parse_args()

    blob = open(a.dump, 'rb').read()
    if len(blob) < MIRROR + 0x1000:
        sys.exit(f'{a.dump} is too short to contain the EEPROM mirror')
    eeprom = blob[MIRROR:MIRROR + 0x1000]

    live = open(a.live, 'rb').read() if a.live else None
    for addr, length, note in RECORDS:
        data = eeprom[addr:addr + length]
        if len(data) < length:
            continue
        if live is not None and live[0xF000 + addr:0xF000 + addr + length] == data:
            continue
        payload = frame_payload(addr, data)
        pad = max(0x80, length + 0x12)
        cmd = [a.exe, 'debug', 'send', '--type', '1900',
               '--pad', str(pad), '--payload', payload, '--wait', '0']
        print(f'0x{addr:03x} +{length:3d}  {data[:8].hex(" "):24} {note}')
        if a.commit:
            subprocess.run(cmd, capture_output=True)
            time.sleep(0.5)     # back-to-back writes are dropped by the card
        else:
            print('   ' + ' '.join(cmd[1:]))

    if a.commit:
        # 0x87 save-to-flash (addr 0, no data); without it the writes stay in the working copy.
        subprocess.run([a.exe, 'debug', 'send', '--type', '1900', '--pad', '128',
                        '--payload', '00ffff87' + '00000000' * 2, '--wait', '0'],
                       capture_output=True)
        # 0x77 reload local params.
        subprocess.run([a.exe, 'debug', 'send', '--type', '0600', '--pad', '126',
                        '--payload', '00ffff77000000000101000000', '--wait', '0'],
                       capture_output=True)
        print('\nwrote and asked the card to reload; power-cycle and verify with:')
        print('  rxp flash dump --block 7 --out now.bin && '
              'python3 scripts/flash-review.py now.bin')
    else:
        print('\ndry run: nothing sent. Re-run with --commit.')


main()

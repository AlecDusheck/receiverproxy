#!/bin/sh
# Reproduce the gateware decode. Requires: brew install prjtrellis
set -e
WORK=${1:-/tmp/e120-trellis}
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
FW=$REPO/third-party/firmware
mkdir -p "$WORK"
for f in "$FW"/*.hex; do
  b=$(basename "$f" .hex)
  # The .hex IS a Lattice .bit (342-byte ASCII header + command stream).
  # Trailing 0xFF padding to the 721024-byte flash page plus an 8-byte
  # trailer confuses ecpunpack, so truncate to just past the last command.
  python3 -c "import sys;d=open(sys.argv[1],'rb').read();open(sys.argv[2],'wb').write(d[:0x8ed30])" "$f" "$WORK/t_$b.bit"
  ecpunpack --idcode 0x41111043 "$WORK/t_$b.bit" "$WORK/t_$b.config"
done
ls -la "$WORK"/*.config

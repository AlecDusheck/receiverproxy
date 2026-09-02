#!/bin/sh
# Mirror this machine's screen, or a region of it, onto the panel through `e120 show stream`.
# macOS captures with avfoundation, Linux with x11grab on $DISPLAY.
#
# Usage:
#   scripts/mirror.sh                      whole screen, scaled to 128x64
#   scripts/mirror.sh -s 256x64 -f 30      panel size and frame rate
#   scripts/mirror.sh -c 0,0,640,320       crop x,y,w,h from the top-left first
#   scripts/mirror.sh -d "Capture screen 1"  another avfoundation device (macOS)
#   scripts/mirror.sh -d :1                another X display (Linux)
#   extra arguments are passed to `e120 show stream`
set -eu

size=128x64
fps=30
crop=
if [ "$(uname -s)" = Darwin ]; then
    grab=avfoundation
    device="Capture screen 0"
    # The screen device offers no planar formats; ask for one it has.
    infmt="-pixel_format bgr0"
else
    grab=x11grab
    device=${DISPLAY:-:0}
    infmt=
fi

# The installed e120, else a local build.
here=$(cd "$(dirname "$0")/.." && pwd)
if command -v e120 >/dev/null; then e120=e120
elif [ -x "$here/target/release/e120" ]; then e120=$here/target/release/e120
elif [ -x "$here/target/debug/e120" ]; then e120=$here/target/debug/e120
else echo "mirror.sh: e120 not found; cargo install --path crates/e120-cli" >&2; exit 1
fi
while getopts 's:f:c:d:h' o; do
    case $o in
        s) size=$OPTARG ;;
        f) fps=$OPTARG ;;
        c) crop=$OPTARG ;;
        d) device=$OPTARG ;;
        *) sed -n '2,11p' "$0"; exit 2 ;;
    esac
done
shift $((OPTIND - 1))

filter="scale=${size}:flags=area"
if [ -n "$crop" ]; then
    IFS=, read -r x y w h <<EOF
$crop
EOF
    filter="crop=${w}:${h}:${x}:${y},${filter}"
fi

# Both grabbers draw the cursor by default. macOS asks for Screen Recording
# permission for the terminal the first time.
exec ffmpeg -hide_banner -loglevel error \
    -f "$grab" $infmt -framerate "$fps" -i "$device" \
    -vf "$filter" -f rawvideo -pix_fmt rgb24 - \
    | "$e120" show stream --size "$size" --fps "$fps" "$@"

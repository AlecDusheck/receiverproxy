#!/bin/sh
# Mirror this Mac's screen, or a region of it, onto the panel through `e120 show stream`.
#
# Usage:
#   scripts/mirror.sh                      whole screen, scaled to 128x64
#   scripts/mirror.sh -s 256x64 -f 30      panel size and frame rate
#   scripts/mirror.sh -c 0,0,640,320       crop x,y,w,h from the top-left first
#   scripts/mirror.sh -d "Capture screen 1"  another avfoundation device
#   extra arguments are passed to `e120 show stream`
set -eu

size=128x64
fps=30
crop=
device="Capture screen 0"
while getopts 's:f:c:d:h' o; do
    case $o in
        s) size=$OPTARG ;;
        f) fps=$OPTARG ;;
        c) crop=$OPTARG ;;
        d) device=$OPTARG ;;
        *) sed -n '2,9p' "$0"; exit 2 ;;
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

# macOS asks for Screen Recording permission for the terminal the first time.
exec ffmpeg -hide_banner -loglevel error \
    -f avfoundation -capture_cursor 1 -framerate "$fps" -i "$device" \
    -vf "$filter" -f rawvideo -pix_fmt rgb24 - \
    | e120 show stream --size "$size" --fps "$fps" "$@"

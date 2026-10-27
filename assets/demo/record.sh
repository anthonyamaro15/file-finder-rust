#!/bin/bash
# Re-records assets/demo.gif from demo.tape in a made-up home folder.
# Needs vhs, ffmpeg, python3 and the "JetBrainsMono Nerd Font Mono" font.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release
DEMO=$(cd /tmp && pwd -P)/ff-demo
rm -rf "$DEMO" && mkdir -p "$DEMO"
bash assets/demo/make-home.sh "$DEMO/home"
PATH="${CARGO_TARGET_DIR:-$PWD/target}/release:$PATH" vhs assets/demo/demo.tape

# vhs saves the text and cursor layers as frames; stack them on the theme
# background and build the GIF with one palette.
read -r W H < <(python3 -c 'import struct,sys; d=open(sys.argv[1],"rb").read(24); print(*struct.unpack(">II", d[16:24]))' "$DEMO/frames/frame-text-00001.png")
ffmpeg -loglevel error -y -framerate 25 -i "$DEMO/frames/frame-text-%05d.png" -framerate 25 -i "$DEMO/frames/frame-cursor-%05d.png" \
  -filter_complex "color=c=#1f2329:s=${W}x${H}:r=25[bg];[bg][0]overlay=shortest=1[t];[t][1]overlay,pad=w=iw+40:h=ih+40:x=20:y=20:color=#1f2329,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
  assets/demo.gif
rm -rf "$DEMO"
echo "Wrote assets/demo.gif"

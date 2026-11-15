#!/bin/bash
# Re-records assets/demo.gif, and target/demo.mp4 for sites that take video, from the
# scene tapes in a made-up home folder.
# Needs vhs, ffmpeg, ImageMagick, fontconfig, python3 and the
# "JetBrainsMono Nerd Font Mono" font.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release
DEMO=$(cd /tmp && pwd -P)/dm-rec
rm -rf "$DEMO" && mkdir -p "$DEMO"
bash assets/demo/make-home.sh "$DEMO/home"
export PATH="${CARGO_TARGET_DIR:-$PWD/target}/release:$PATH"

# The caption above each scene
captions=(
  "Type part of a name. Enter. You're there."
  "Initials work too: ffr → file-finder-rust"
  "Index too big? Switch off what you never search."
)
for i in 1 2 3; do
  vhs "assets/demo/scene-$i.tape"
done

# vhs saves the text and cursor layers as frames: stack them on the theme background,
# put the caption bar on top, then join the scenes. One palette for the whole GIF.
font=$(fc-match -f '%{file}' 'JetBrainsMono Nerd Font Mono:bold')
read -r W H < <(python3 -c 'import struct,sys; d=open(sys.argv[1],"rb").read(24); print(*struct.unpack(">II", d[16:24]))' "$DEMO/scene-1/frame-text-00001.png")
inputs=() filters="" joined=""
for i in 1 2 3; do
  magick -size "$(( (W + 1) / 2 * 2 + 16 ))x92" xc:'#15181c' -font "$font" -pointsize 34 -fill '#e3e7ee' \
    -gravity center -annotate +0+0 "${captions[$((i - 1))]}" "$DEMO/caption-$i.png"
  inputs+=(-framerate 25 -i "$DEMO/scene-$i/frame-text-%05d.png" -framerate 25 -i "$DEMO/scene-$i/frame-cursor-%05d.png" -loop 1 -framerate 25 -i "$DEMO/caption-$i.png")
  t=$((3 * i - 3)) c=$((3 * i - 2)) k=$((3 * i - 1))
  filters+="color=c=#1f2329:s=${W}x${H}:r=25[bg$i];[bg$i][$t]overlay=shortest=1[t$i];[t$i][$c]overlay,"
  filters+="pad=w=ceil(iw/2)*2+16:h=ceil(ih/2)*2+16:x=8:y=8:color=#1f2329,"
  filters+="pad=w=iw:h=ih+92:y=92:color=#15181c[p$i];[p$i][$k]overlay=shortest=1[s$i];"
  joined+="[s$i]"
done
ffmpeg -loglevel error -y "${inputs[@]}" \
  -filter_complex "${filters}${joined}concat=n=3,split[v][g];[g]split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle[gif]" \
  -map "[gif]" assets/demo.gif \
  -map "[v]" -c:v libx264 -pix_fmt yuv420p -crf 18 -movflags +faststart target/demo.mp4
rm -rf "$DEMO"
echo "Wrote assets/demo.gif and target/demo.mp4"

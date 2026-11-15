#!/bin/bash
# Re-records assets/demo.gif (and target/demo.mp4) from the tapes in a made-up home folder.
# Needs vhs, ffmpeg, python3 and the "JetBrainsMono Nerd Font Mono" font.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release
DEMO=$(cd /tmp && pwd -P)/dm-rec
rm -rf "$DEMO" && mkdir -p "$DEMO"
bash assets/demo/make-home.sh "$DEMO/home"
export PATH="${CARGO_TARGET_DIR:-$PWD/target}/release:$PATH"
vhs assets/demo/demo.tape
vhs assets/demo/index.tape

# vhs saves the text and cursor layers as frames; stack them on the theme
# background. demo.tape becomes the README GIF; demo.tape then index.tape becomes
# target/demo.mp4.
read -r W H < <(python3 -c 'import struct,sys; d=open(sys.argv[1],"rb").read(24); print(*struct.unpack(">II", d[16:24]))' "$DEMO/frames/frame-text-00001.png")
layers() { echo "-framerate 25 -i $1/frame-text-%05d.png -framerate 25 -i $1/frame-cursor-%05d.png"; }
stack="color=c=#1f2329:s=${W}x${H}:r=25"
pad="pad=w=ceil(iw/2)*2+16:h=ceil(ih/2)*2+16:x=8:y=8:color=#1f2329"
# shellcheck disable=SC2046
ffmpeg -loglevel error -y $(layers "$DEMO/frames") $(layers "$DEMO/frames-index") \
  -filter_complex "$stack[bg1];[bg1][0]overlay=shortest=1[t1];[t1][1]overlay,$pad,split[s][m1];$stack[bg2];[bg2][2]overlay=shortest=1[t2];[t2][3]overlay,$pad[m2];[s]split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle[g];[m1][m2]concat=n=2[v]" \
  -map "[g]" assets/demo.gif \
  -map "[v]" -c:v libx264 -pix_fmt yuv420p -crf 18 -movflags +faststart target/demo.mp4
rm -rf "$DEMO"
echo "Wrote assets/demo.gif and target/demo.mp4 (the MP4 is for sites that take video)"

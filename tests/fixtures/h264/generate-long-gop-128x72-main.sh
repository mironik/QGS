#!/usr/bin/env sh
set -eu

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=6 \
  -frames:v 12 \
  -c:v libx264 \
  -profile:v main \
  -pix_fmt yuv420p \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -an \
  -f h264 \
  tests/fixtures/h264/long-gop-128x72-main.h264

#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

ffmpeg -y \
  -f lavfi \
  -i testsrc2=size=64x64:rate=1:duration=1 \
  -frames:v 1 \
  -pix_fmt yuv420p \
  -c:v libx264 \
  -profile:v baseline \
  -level:v 3.0 \
  -preset ultrafast \
  -tune zerolatency \
  -x264-params keyint=1:min-keyint=1:scenecut=0:bframes=0:ref=1:repeat-headers=1 \
  -f h264 \
  "$SCRIPT_DIR/idr-64x64-baseline.h264"

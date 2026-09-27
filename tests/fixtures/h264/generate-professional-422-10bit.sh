#!/usr/bin/env sh
set -eu

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=1 \
  -frames:v 1 \
  -c:v libx264 \
  -profile:v high422 \
  -pix_fmt yuv422p10le \
  -x264-params keyint=1:min-keyint=1:scenecut=0:repeat-headers=1 \
  -an \
  -f h264 \
  tests/fixtures/h264/professional-422-10bit-idr-128x72.h264

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=6 \
  -frames:v 12 \
  -c:v libx264 \
  -profile:v high422 \
  -pix_fmt yuv422p10le \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -an \
  -f h264 \
  tests/fixtures/h264/professional-422-10bit-long-gop-128x72.h264

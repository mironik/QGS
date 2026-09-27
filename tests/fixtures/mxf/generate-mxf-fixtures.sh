#!/usr/bin/env sh
set -eu

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=25 \
  -frames:v 12 \
  -c:v libx264 \
  -profile:v main \
  -pix_fmt yuv420p \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -an \
  -f mxf \
  tests/fixtures/mxf/h264-8bit-420-long-gop-128x72.mxf

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=25 \
  -frames:v 12 \
  -c:v libx264 \
  -profile:v high422 \
  -pix_fmt yuv422p10le \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -an \
  -f mxf \
  tests/fixtures/mxf/h264-10bit-422-long-gop-128x72.mxf

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=25 \
  -f lavfi -i sine=frequency=1000:sample_rate=48000:duration=0.48 \
  -frames:v 12 \
  -c:v libx264 \
  -profile:v main \
  -pix_fmt yuv420p \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -c:a pcm_s16le \
  -ar 48000 \
  -ac 2 \
  -timecode 01:00:00:00 \
  -f mxf \
  tests/fixtures/mxf/h264-8bit-420-long-gop-128x72-pcm-stereo.mxf

ffmpeg -y \
  -f lavfi -i testsrc2=size=128x72:rate=25 \
  -f lavfi -i sine=frequency=440:sample_rate=48000:duration=0.48 \
  -f lavfi -i sine=frequency=880:sample_rate=48000:duration=0.48 \
  -frames:v 12 \
  -map 0:v:0 \
  -map 1:a:0 \
  -map 2:a:0 \
  -c:v libx264 \
  -profile:v main \
  -pix_fmt yuv420p \
  -x264-params keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1 \
  -c:a pcm_s16le \
  -ar 48000 \
  -ac 1 \
  -timecode 01:00:00:00 \
  -f mxf \
  tests/fixtures/mxf/h264-8bit-420-long-gop-128x72-two-mono.mxf

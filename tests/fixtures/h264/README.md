# H.264 Fixture Provenance

`idr-64x64-baseline.h264` is a tiny synthetic H.264 Annex B stream used for
QGS M2 Step 3B decoder tests and hardware demonstrations.

It contains no third-party footage. The picture is generated from FFmpeg's
`testsrc2` synthetic source.

Generation:

```sh
./generate-idr-64x64-baseline.sh
```

Properties:

- tool: FFmpeg, development fixture generation only
- source: `testsrc2=size=64x64:rate=1:duration=1`
- dimensions: 64 x 64
- frame count: 1
- H.264 profile: Constrained Baseline
- bit depth: 8-bit
- chroma: 4:2:0
- pixel format before encode: `yuv420p`
- scan: progressive
- format: Annex B raw H.264
- GOP: IDR/I-frame only

Normal `cargo build` and `cargo test` do not invoke FFmpeg. QGS runtime code
must not depend on the `ffmpeg` executable or FFmpeg libraries.

`long-gop-128x72-main.h264` is a synthetic H.264 Annex B stream used for QGS M2
Step 5 Long-GOP, DPB, and picture-reordering tests.

It contains no third-party footage. The picture sequence is generated from
FFmpeg's `testsrc2` synthetic source.

Generation:

```sh
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
```

Observed properties:

- tool: FFmpeg with libx264, development fixture generation only
- source: `testsrc2=size=128x72:rate=6`
- dimensions: 128 x 72
- frame count: 12
- H.264 profile: Main
- bit depth: 8-bit
- chroma: 4:2:0
- pixel format before encode: `yuv420p`
- scan: progressive
- format: Annex B raw H.264
- GOP: closed 12-frame GOP with IDR/I, P, and B pictures
- ffprobe presentation picture types: `I B P B B P B B P B B P`

The raw Annex B stream is submitted to QGS in access-unit/decode order by the
test client. Presentation order is derived by the QGS H.264 frontend from codec
POC/DPB state, not by packet arrival order.

`professional-422-10bit-idr-128x72.h264` is a synthetic H.264 Annex B stream
used for QGS M2 Step 6 professional 10-bit / 4:2:2 parser classification.

Generation:

```sh
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
```

Observed properties:

- tool: FFmpeg with libx264, development fixture generation only
- source: `testsrc2=size=128x72:rate=1`
- dimensions: 128 x 72
- frame count: 1
- H.264 profile: High 4:2:2 Intra
- level: 1.0
- bit depth: 10-bit
- chroma: 4:2:2
- pixel format before encode: `yuv422p10le`
- scan: progressive
- format: Annex B raw H.264
- GOP: IDR/I-frame only

`professional-422-10bit-long-gop-128x72.h264` is a synthetic H.264 Annex B
stream used for QGS M2 Step 6 professional Long-GOP parser classification.

Generation:

```sh
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
```

Observed properties:

- tool: FFmpeg with libx264, development fixture generation only
- source: `testsrc2=size=128x72:rate=6`
- dimensions: 128 x 72
- frame count: 12
- H.264 profile: High 4:2:2
- level: 1.0
- bit depth: 10-bit
- chroma: 4:2:2
- pixel format before encode: `yuv422p10le`
- scan: progressive
- format: Annex B raw H.264
- GOP: closed 12-frame GOP with IDR/I, P, and B pictures
- ffprobe presentation picture types: `I B P B B P B B P B B P`

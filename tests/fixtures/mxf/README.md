# MXF Fixture Provenance

These MXF files are synthetic development fixtures. They contain no camera
footage and no third-party program material. FFmpeg/libx264 is used only to
generate fixtures and is not a QGS runtime dependency.

Normal `cargo build` and `cargo test` do not invoke FFmpeg.

## Generated Fixtures

`h264-8bit-420-long-gop-128x72.mxf`

Generation:

```sh
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
```

Observed properties:

- container: MXF OP1a as written by FFmpeg
- source: `testsrc2=size=128x72:rate=25`
- codec: H.264
- profile: Main
- bit depth: 8-bit
- chroma: 4:2:0
- pixel format: `yuv420p`
- dimensions: 128 x 72
- edit rate: 25/1
- frame count: 12
- GOP: closed GOP with IDR/I, P, and B pictures
- timecode tag: `00:00:00:00`

`h264-10bit-422-long-gop-128x72.mxf`

Generation:

```sh
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
```

Observed properties:

- container: MXF OP1a as written by FFmpeg
- source: `testsrc2=size=128x72:rate=25`
- codec: H.264
- profile: High 4:2:2
- bit depth: 10-bit
- chroma: 4:2:2
- pixel format: `yuv422p10le`
- dimensions: 128 x 72
- edit rate: 25/1
- frame count: 12
- GOP: closed GOP with IDR/I, P, and B pictures
- timecode tag: `00:00:00:00`

## Future Legal Camera Fixture Matrix

Future fixtures should be short, anonymized, redistribution-safe samples with
clear provenance. QGS low-level APIs should continue to describe technical
codec/container properties rather than product or manufacturer labels.

High-priority categories:

- Sony professional H.264/AVC Long-GOP 10-bit 4:2:2 MXF
- Sony professional H.264/AVC Intra 10-bit 4:2:2 MXF
- Panasonic professional AVC Long-GOP/Intra MXF

Later categories:

- Canon professional MXF
- JVC professional MXF
- legacy MPEG-2 4:2:2 MXF

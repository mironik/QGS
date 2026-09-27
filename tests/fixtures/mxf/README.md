# MXF Fixture Provenance

These MXF files are synthetic development fixtures. They contain no camera
footage and no third-party program material. FFmpeg/libx264 is used only to
generate fixtures and is not a QGS runtime dependency.

Normal `cargo build` and `cargo test` do not invoke FFmpeg.

## Generated Fixtures

| Fixture | Operational pattern | Codec/profile | Bit depth/chroma | Edit rate | GOP | Audio | Timecode | Index/partitions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `h264-8bit-420-long-gop-128x72.mxf` | OP1a | H.264 Main | 8-bit 4:2:0 | 25/1 | Long-GOP I/P/B | none | 00:00:00:00 | MXF index segment, RIP, header/body/footer |
| `h264-10bit-422-long-gop-128x72.mxf` | OP1a | H.264 High 4:2:2 | 10-bit 4:2:2 | 25/1 | Long-GOP I/P/B | none | 00:00:00:00 | MXF index segment, RIP, header/body/footer |
| `h264-8bit-420-long-gop-128x72-pcm-stereo.mxf` | OP1a | H.264 Main | 8-bit 4:2:0 | 25/1 | Long-GOP I/P/B | PCM s16le, 48 kHz, 2 channels | 01:00:00:00 tag; qgs-mxf parses local Timecode Component start frame 90000 | MXF index segment, RIP, header/body/footer |
| `h264-8bit-420-long-gop-128x72-two-mono.mxf` | OP1a | H.264 Main | 8-bit 4:2:0 | 25/1 | Long-GOP I/P/B | two PCM s16le mono tracks, 48 kHz | 01:00:00:00 tag; qgs-mxf parses local Timecode Component start frame 90000 | MXF index segment, RIP, header/body/footer |

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

`h264-8bit-420-long-gop-128x72-pcm-stereo.mxf`

Generation:

```sh
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
```

Observed properties:

- container: MXF OP1a as written by FFmpeg
- video: H.264 Main, 8-bit, 4:2:0, 128 x 72, 12 frames
- audio: PCM signed 16-bit little-endian, 48 kHz, 2 channels
- timecode metadata tag: `01:00:00:00`
- local Timecode Component parsed by qgs-mxf: start frame `90000`, rounded base `25`

`h264-8bit-420-long-gop-128x72-two-mono.mxf`

Generation:

```sh
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
```

Observed properties:

- container: MXF OP1a as written by FFmpeg
- video: H.264 Main, 8-bit, 4:2:0, 128 x 72, 12 frames
- audio: two PCM signed 16-bit little-endian mono tracks, 48 kHz
- timecode metadata tag: `01:00:00:00`
- local Timecode Component parsed by qgs-mxf: start frame `90000`, rounded base `25`

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

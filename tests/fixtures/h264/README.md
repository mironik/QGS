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

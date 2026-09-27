# QGS M2 Step 6 Report

## Summary

M2 Step 6 adds the professional H.264 10-bit / 4:2:2 foundation without
claiming unsupported Haswell hardware decode. QGS now distinguishes valid
professional H.264 parse/classification from backend decode support.

The Haswell VA -> Vulkan zero-copy path remains frozen. No MXF, software
decode, XAVC product classification, HEVC, MPEG-2/XDCAM, encode, or VA ->
Vulkan zero-copy work was started.

## Professional Fixture Generation

Fixtures:

- `tests/fixtures/h264/professional-422-10bit-idr-128x72.h264`
- `tests/fixtures/h264/professional-422-10bit-long-gop-128x72.h264`

Generation script:

- `tests/fixtures/h264/generate-professional-422-10bit.sh`

Intra/IDR command:

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

Long-GOP command:

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

FFmpeg/libx264 is a development fixture-generation tool only and is not a QGS
runtime dependency.

## Fixture Technical Properties

Intra fixture:

- codec: H.264
- profile: High 4:2:2 Intra
- dimensions: 128 x 72
- pixel format: `yuv422p10le`
- bit depth: 10-bit
- chroma: 4:2:2
- scan: progressive
- frame count: 1
- format: Annex B

Long-GOP fixture:

- codec: H.264
- profile: High 4:2:2
- dimensions: 128 x 72
- pixel format: `yuv422p10le`
- bit depth: 10-bit
- chroma: 4:2:2
- scan: progressive
- frame count: 12
- GOP: closed GOP with IDR/I, P, and B pictures
- ffprobe presentation picture types: `I B P B B P B B P B B P`
- format: Annex B

## H.264 High 4:2:2 Parser Result

`qgs-codec-h264` recognizes High 4:2:2 and High 4:2:2 Intra profiles as
QGS-owned profile values. Parser-library profile enums remain private to
`qgs-codec-h264`.

## Bit-Depth Parsing

The parser preserves actual SPS luma/chroma bit depth. Step 6 accepts matching
8-bit and 10-bit luma/chroma depths. The professional fixtures parse as:

- `bit_depth_luma_minus8 = 2`
- `bit_depth_chroma_minus8 = 2`
- QGS `BitDepth = 10`

Mismatched luma/chroma bit depths and unsupported bit depths are rejected
explicitly.

## Chroma Parsing

The parser preserves `chroma_format_idc`. The professional fixtures parse as:

- `chroma_format_idc = 2`
- QGS `ChromaSubsampling::Cs422`

4:2:0 continues to map to `Cs420`. 4:4:4 is distinguished by the parser but no
QGS Step 6 surface storage representation is implemented for it.

## Intra Classification Result

`professional-422-10bit-idr-128x72.h264` parses as:

- profile: `H264Profile::High422Intra`
- surface format: `VideoSurfaceFormat::Yuv422_10`
- bit depth: 10
- chroma: `Cs422`
- picture kind: IDR/I

## Long-GOP Classification Result

`professional-422-10bit-long-gop-128x72.h264` parses as:

- profile: `H264Profile::High422`
- surface format: `VideoSurfaceFormat::Yuv422_10`
- bit depth: 10
- chroma: `Cs422`
- picture kinds: I, P, and B
- DPB state updates succeed for the fixture

No Intel hardware decode is attempted for this unsupported configuration.

## Parse Support Vs Decode Support

QGS now treats these as separate outcomes:

- malformed stream: invalid compressed data or parser failure
- unsupported parser feature: valid syntax outside the implemented parser subset
- unsupported backend configuration: valid parsed stream not supported by the
  selected hardware/backend

H.264 High 4:2:2 10-bit is parse/classify supported in Step 6, but the current
Intel i965 VA backend does not advertise it and rejects decoder creation with
`UnsupportedDecodeConfiguration`.

## Capability Matching Changes

`DecoderConfig` can now match itself against a `VideoDecodeCapability` using:

- codec
- codec-specific profile
- bit depth
- chroma subsampling
- coded dimensions
- scan mode
- compatible output surface format semantics

Backend selection is not based on `codec == H264` alone.

## Intel Match Results

Current Intel HD Graphics 4600 / i965 advertised capabilities include:

- H.264 Baseline 8-bit 4:2:0 NV12
- H.264 Main 8-bit 4:2:0 NV12
- H.264 High 8-bit 4:2:0 NV12

Pure capability tests show:

- H.264 8-bit 4:2:0 config -> Intel-style capability match
- H.264 10-bit 4:2:2 config -> Intel-style capability mismatch

## NVIDIA Result

NVIDIA GTX 950M / nouveau continues to report zero VA decode capabilities.
H.264 decoder creation fails cleanly as unsupported. No software fallback is
attempted.

## VideoSurface 10-bit / 4:2:2 Model

The existing QGS model now explicitly covers the professional semantic surface:

- `VideoSurfaceFormat::Yuv422_10`
- `BitDepth::new(10)`
- `ChromaSubsampling::Cs422`

This is a semantic QGS format, not a promise of one backend memory layout.

## Storage Representation Decision

QGS keeps video surface semantics separate from backend storage. 10-bit 4:2:2
may later be packed, planar, semi-planar, or driver-specific. Step 6 does not
choose a VA fourcc, Vulkan format, DRM modifier, or plane layout for a real
10-bit / 4:2:2 allocation.

## MXF Readiness

`qgs-codec-h264` remains a codec frontend. It accepts compressed H.264 access
unit bytes and has no file, timeline, or container ownership. A future MXF
demux/index layer can supply access units without introducing MXF semantics into
the codec parser.

## Test Results

`cargo test --workspace` passed.

Total: 165 tests passed.

- `qgs-codec-h264`: 6 passed
- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 118 passed
- `qgs-vaapi`: 9 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Intel 8-bit Long-GOP Regression

The existing real Intel HD Graphics 4600 / i965 regression passed:

- IDR H.264 8-bit 4:2:0 decode succeeded
- 12-frame H.264 Main 8-bit 4:2:0 Long-GOP decode succeeded
- IDR/I, P, and B pictures decoded
- presentation flush returned the final two delayed surfaces
- validation-only VA readback succeeded for every decoded frame
- max DPB occupancy: 5
- max live VA surfaces: 5

## Unsafe Inventory

No new unsafe Rust was added for Step 6. The QGS-owned unsafe block count
remains 38 from the existing audited qgs-vulkan interop boundaries and
diagnostics. `qgs-codec-h264`, `qgs-vaapi`, `qgs-core`, `qgs-protocol`, `qgsd`,
and `qgs-test` remain `#![forbid(unsafe_code)]`.

## Cleanup And Disconnect

The hardware regression left one transient H.264 decoder and one decoded
`VideoSurface` alive for disconnect cleanup. `qgsd` logged:

```text
client disconnected; releasing 1 resource(s), 0 sync object(s), and 1 decoder(s) for session 1
qgs-core: releasing 1 resource(s) owned by session
qgs-core: releasing 1 decoder(s) owned by session
```

## Commit And Push Verification

This report is included in the M2 Step 6 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Limitations

Step 6 does not implement:

- 10-bit / 4:2:2 hardware decode on current Intel Haswell
- real 10-bit / 4:2:2 VideoSurface allocation
- software decode fallback
- MXF demuxing/indexing
- XAVC or other product/workflow classification
- MPEG-2/XDCAM
- HEVC
- VA -> Vulkan zero-copy resume

## Recommendation For M2 Step 7

Proceed to MXF foundation next. The codec frontend can now classify the
professional H.264 stream properties that an MXF layer will need to preserve,
while backend capability matching can cleanly reject unsupported hardware
decode without treating valid professional streams as malformed.

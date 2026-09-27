# QGS M2 Step 9 Report

## Summary

M2 Step 9 adds the first real software pixel decoder fallback for valid
professional H.264 streams that the selected hardware decoder cannot handle.
The acceptance path uses `qgs-mxf` for MXF parsing/indexing, `qgs-codec-h264`
for syntax classification, QGS capability matching for backend selection, and a
new `qgs-software-video` backend for libavcodec pixel decode.

The external Sony FX6 sample remains outside the repository. This report uses
the privacy-safe name "Sony FX6 sample 001" and omits the original clip name,
local path, serial number, full private UMIDs, and private production
timestamps.

## Software Decoder Dependency Decision

The selected software decoder backend is FFmpeg libavcodec through
`rsmpeg 0.18.0` with the `ffmpeg8` and `link_system_ffmpeg` features.

The dependency audit favored libavcodec because it provides mature H.264
High 4:2:2 10-bit Long-GOP decode, including I/P/B pictures and delayed output.
No mature pure-Rust H.264 decoder option was found that could satisfy the
professional High 4:2:2 10-bit requirement. GStreamer would add a larger media
framework boundary than this milestone needs. QGS does not invoke the `ffmpeg`
executable for runtime decode.

Installed FFmpeg development libraries verified by `pkg-config`:

- libavcodec: `62.11.100`
- libavutil: `60.8.100`
- libswscale: `9.1.100`
- libavformat: `62.3.100`
- libavdevice: `62.1.100`

`libavformat` and `libavdevice` are present only because the selected Rust
wrapper links against the system FFmpeg development surface. QGS does not use
libavformat to open, demux, seek, or index MXF files, and does not use
libavdevice for ingest.

## Rust FFmpeg Integration

`qgs-software-video` uses safe `rsmpeg` wrappers for:

- H.264 decoder lookup
- codec context creation/opening
- packet parser creation
- packet submission
- frame receive/drain
- AVFrame pixel-format and plane access
- safe frame copy into QGS-owned memory

No new QGS-owned unsafe Rust was added. `qgs-software-video` uses
`#![forbid(unsafe_code)]`.

## Licensing And Deployment Observations

The current system FFmpeg build is dynamically linked and GPL-enabled. That is
acceptable for this development milestone, but product distribution needs a
separate legal/deployment review. A future distributable QGS build may need a
controlled LGPL-compatible FFmpeg configuration or another explicitly approved
codec distribution strategy. This report makes no legal claim beyond recording
the technical dependency.

## Software Backend Architecture

The implemented path is:

```text
MXF
    |
qgs-mxf
    |
bounded compressed H.264 access units
    |
QGS decoder selection
   / \
VA-API hardware    qgs-software-video
when supported         |
                       v
                 rsmpeg / libavcodec
                       |
                       v
              software-backed VideoSurface
```

`qgs-mxf` remains the MXF demux/index implementation. `qgs-codec-h264` remains
the QGS syntax/classification frontend. `qgs-software-video` owns only software
decoder lifecycle, compressed access-unit submission, decoded-frame ownership,
pixel-format translation, and software-backed VideoSurface storage.

## Backend Selection Policy

The daemon selection policy remains minimal and deterministic:

1. Try `qgs-vaapi` first.
2. If VA-API reports `UnsupportedDecodeConfiguration`, test whether
   `qgs-software-video` supports the requested technical configuration.
3. Use software fallback only for supported software configurations.
4. Otherwise return the original stable unsupported-configuration result.

This keeps the existing Intel H.264 8-bit 4:2:0 path on hardware while allowing
valid H.264 High 4:2:2 10-bit streams to fall back to software decode.

## Software VideoSurface Backing

Decoded software frames are represented as QGS VideoSurface resources with
separate semantic and storage properties.

Semantic properties include dimensions, visible region, bit depth, chroma,
scan mode, field order, and surface format. Storage properties include decoder
pixel format, plane count, per-plane stride, source stride, byte length, and
QGS-owned backing memory.

AVFrame pointers and decoder-owned memory do not escape the backend. The
backend copies decoded rows once from libavcodec-owned frame memory into
QGS-owned plane buffers. This is acceptable for software decode and is not
reported as zero-copy.

## Real Decoded Pixel Format

Sony FX6 sample 001 decodes as:

- decoder pixel format: `yuv422p10le`
- QGS storage format: `Yuv422P10Le`
- plane count: 3
- width: 1920
- height: 1080
- bit depth: 10
- chroma: 4:2:2

First-frame plane model:

| Plane | Width samples | Height | QGS stride | Source stride | Bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Y | 1920 | 1080 | 3840 | 3840 | 4147200 |
| Cb | 960 | 1080 | 1920 | 1920 | 2073600 |
| Cr | 960 | 1080 | 1920 | 1920 | 2073600 |

The implementation does not reduce 10-bit to 8-bit, does not reduce 4:2:2 to
4:2:0, and does not convert to RGB or NV12.

## Full FX6 Decode Result

The external MXF hash matched the expected compatibility-corpus SHA-256 before
analysis. QGS parsed/indexed the MXF through `qgs-mxf`, classified the video as
H.264 High 4:2:2 10-bit through `qgs-codec-h264`, rejected Intel i965 hardware
selection as unsupported for that technical configuration, and selected the
software backend.

Full sequential decode produced exactly 106 presentation frames.

Delayed B-frame output is drained through the decoder flush path. The output
frame count matches the MXF/index duration of 106 edit units.

## Random Access Result

For target edit unit 53:

- `qgs-mxf` selected nearest prior random-access edit unit 48.
- QGS extracted bounded compressed access units from edit unit 48 through 53.
- The software decoder reconstructed the required GOP state from that random
  access point.
- Presentation frame 53 was produced.
- The random-access frame checksum matched sequential frame 53.

Checksums over visible decoded plane rows:

- first frame: `0x547598eb5c3f9886`
- sequential frame 53: `0xf28dbe5ef53de314`
- random-access frame 53: `0xf28dbe5ef53de314`
- final frame: `0x0ef21e20fed93d04`

No decoded pixel payloads are committed.

## Memory Observations

For the FX6 software decode:

- bytes per QGS software VideoSurface: 8,294,400
- maximum simultaneously live QGS software surfaces during the proof: 2
- approximate peak QGS-owned decoded-frame memory during the proof: 16,588,800
  bytes

This does not include FFmpeg/libavcodec internal decoder memory, which was not
measured.

## Development Performance Observation

Development observation, not a benchmark:

- full 106-frame decode time: 29.977 seconds
- approximate decode rate: 3.54 fps

This is a correctness milestone. No optimization work was performed.

## Intel Hardware Regression

The existing Intel H.264 8-bit 4:2:0 hardware decode regression remains on the
VA-API backend. The daemon logs `decoder backend selected: vaapi` for that
supported configuration.

For Sony FX6 sample 001, Intel i965 remains a clean hardware capability
mismatch for H.264 High 4:2:2 10-bit. The stream is valid, but not supported by
that hardware backend.

## NVIDIA Behavior

The current GTX 950M/nouveau VA path still reports no suitable hardware decode
profiles. Step 9 adds no NVIDIA-specific hardware decode path and does not
involve NVK or Vulkan. For H.264 configurations supported by
`qgs-software-video`, the backend-neutral decoder lifecycle can still create a
software decoder on this machine after VA-API reports no decode capability.

## Tests And Quality Gates

Added hardware-independent tests for:

- software backend support for H.264 High 4:2:2 10-bit
- unsupported software configurations
- software plane ownership
- 10-bit 4:2:2 plane model
- checked decoded-frame size arithmetic
- deterministic software-surface checksum

Final gate results are recorded after the quality-gate run below.

## Unsafe Inventory

No QGS-owned unsafe code was added. `qgs-software-video` uses
`#![forbid(unsafe_code)]`. The QGS-owned unsafe inventory remains unchanged
from the M2 Step 4B diagnostic count: 38 unsafe blocks, all in the existing
Vulkan interop/diagnostic boundary.

The selected FFmpeg wrapper and FFmpeg libraries contain dependency-internal
unsafe/FFI code, but QGS does not add a new unsafe Rust boundary for Step 9.

## Cleanup And Lifetime

Software VideoSurface contents are owned by QGS `Vec<u8>` plane buffers after
decode. No AVFrame pointer or libavcodec-owned plane pointer escapes
`qgs-software-video`.

Daemon-created decoded surfaces continue to use the existing session-owned
VideoSurface/resource cleanup model. Decoder destruction and disconnect cleanup
release backend state through Rust ownership.

## Limitations

- Software VideoSurface -> GPU upload is not implemented yet.
- Audio decode is not implemented.
- SMPTE 436M ANC interpretation is not implemented.
- HEVC, MPEG-2/XDCAM decode, and video encode are not implemented.
- The Haswell VA -> Vulkan zero-copy path remains frozen.
- Current software decode uses a conservative correctness-first configuration
  and is not performance tuned.

## Recommendation

The next milestone should define the software VideoSurface -> GPU upload path
so systems without suitable hardware decode can still enter the GPU processing
pipeline. That path should remain separate from the frozen VA -> Vulkan
zero-copy investigation.

## Final Results

- `cargo fmt --all -- --check`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass, 203 unit tests plus doc-tests
- Intel hardware regression: pass; H.264 8-bit 4:2:0 IDR and Long-GOP selected
  VA-API and validated through backend readback
- NVIDIA behavior: no advertised VA decode profiles; backend-neutral H.264
  proof uses software fallback without NVIDIA-specific hardware decode
- FX6 sequential software decode: pass, 106 frames
- FX6 random-access frame 53: pass, nearest random access 48, checksum matched
  sequential frame 53
- commit: `Implement QGS professional H264 software fallback`
- push/main verification: completed after commit

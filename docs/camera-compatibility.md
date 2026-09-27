# QGS Camera Compatibility Notes

This document records privacy-safe technical compatibility observations from
legally obtained external camera samples. The original media files and sidecar
metadata are not part of the QGS repository.

QGS records technical container, codec, essence, audio, timecode, and index
behavior here. Product/workflow classification remains future work and must not
be encoded into low-level QGS protocol types.

## Sony FX6 Sample 001

Status: external read-only compatibility corpus, not committed.

Hash verification:

- MXF SHA-256 matched the expected compatibility-corpus hash.

Container:

- MXF OP1a
- duration: 106 edit units
- edit rate: 50/1
- MXF-provided Index Table Segment present
- RIP present

Video:

- codec: H.264 / AVC
- profile: High 4:2:2
- bit depth: 10-bit
- chroma: 4:2:2
- display dimensions: 1920 x 1080
- stored dimensions: 1920 x 1088
- scan: progressive
- GOP: Long-GOP with I/P/B pictures

Audio:

- 4 independent mono PCM streams
- 48 kHz
- 24-bit

Data:

- SMPTE 436M ANC data track present
- QGS discovers the track and preserves descriptor identity
- ANC payload interpretation is not implemented

Sidecar XML technical comparison:

- camera class/model: Sony FX6
- video codec tag indicates AVC 50 Mbit/s class, 1920 x 1080, High 4:2:2
- frame rate metadata reports 50p
- audio metadata reports 4 LPCM24 channels
- color/acquisition metadata reports Rec.709-related values

QGS result:

- `qgs-mxf` parses structural metadata, descriptors, timecode, index, audio
  tracks, and data track.
- H.264 access units extracted from MXF classify through `qgs-codec-h264` as
  High 4:2:2, 10-bit, 4:2:2, 1920 x 1080.
- Metadata, H.264 SPS, and sidecar XML agree on the core technical video shape.
- Current Intel Haswell VA-API capability still does not support H.264 10-bit
  4:2:2 hardware decode. This is a capability mismatch, not malformed media.
- Step 9 software fallback decodes all 106 presentation frames through
  `rsmpeg`/libavcodec while preserving `yuv422p10le` 10-bit 4:2:2 software
  VideoSurface semantics.
- Random access to edit unit 53 starts from the MXF-derived random-access point
  at edit unit 48 and produces the same decoded-frame checksum as sequential
  decode.
- Step 10 uploads the software-decoded `yuv422p10le` VideoSurface planes to
  Vulkan storage buffers and runs a fixed GPU YCbCr -> RGBA validation shader.
  The proof passes on Intel HD Graphics 4600 and NVIDIA GTX 950M / NVK with no
  CPU pixel copy between the software VideoSurface and the GPU processing input
  beyond the explicit fallback upload.
- Step 10 validates first frame, sequential edit unit 53, random-access edit
  unit 53, and final frame. Sequential and random-access edit unit 53 produce
  the same GPU output checksum.

Privacy handling:

- The original media filename, local filesystem path, camera serial number,
  full UMIDs, and private production timestamps are intentionally omitted.

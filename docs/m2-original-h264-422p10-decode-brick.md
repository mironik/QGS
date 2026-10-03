# M2 Original H.264 4:2:2 10-bit Decode Brick

This checkpoint starts a separate QGS LEGO brick for original Sony FX6 video
decode. It is intentionally isolated from `qgs-media-runtime` and does not
import FFmpeg, rsmpeg, libavcodec, or libavformat.

## Scope

The new crate is:

- `crates/qgs-h264-422p10`

It is only for the original MXF video profile currently needed by QGS:

- H.264 High 4:2:2
- 10-bit
- progressive
- coded 1920x1088
- visible 1920x1080
- Long-GOP access-unit input from `qgs-codec-h264`
- output contract: owned `yuv422p10le` planes suitable for the existing
  `qgs-vulkan::GpuFrameProcessor` upload model

Other profiles are rejected by contract. Proxy 8-bit 4:2:0 remains on the
`qgs-vaapi` proxy path.

## Current Result

Implemented in this checkpoint:

- standalone Annex B byte-stream and RBSP extraction
- standalone bit reader with Exp-Golomb support
- 10-bit 4:2:2 decoded-frame storage preserving Y/Cb/Cr plane geometry
- public `H264422P10Decoder::decode_access_unit` entrypoint that parses an
  access unit, validates the original Sony FX6 profile, walks slices and
  macroblocks, and returns owned `yuv422p10le` planes only when the whole
  picture is reconstructed by the supported native path
- `H264422P10Decoder::submit_access_unit` decoded-picture store that keeps
  native decoded frames for Long-GOP reference/output ordering
- H.264 inverse 4x4 transform primitive
- H.264 intra 16x16 prediction primitives
- H.264 Intra4x4 prediction primitives for the native I_NxN path
- flat-scaling inverse quantization for 4x4 residual blocks
- luma intra16x16 macroblock reconstruction into the decoded-frame store
- 4:2:2 chroma macroblock reconstruction into separate Cb/Cr planes
- I-slice macroblock type model for `I_NxN`, `I_16x16`, and `I_PCM`
- residual-presence modeling that distinguishes absent, present,
  separately-signaled, and PCM raw-sample cases
- raster macroblock grid/address cursor from parsed slice metadata
- narrow `Intra16x16` macroblock object that writes reconstructed Y/Cb/Cr
  samples into the owned frame store
- slice-data payload extraction from parsed H.264 NAL/RBSP offsets
- CABAC/CAVLC entropy-mode identification for parsed slices
- CABAC arithmetic reader initialization, context-coded decision bins, bypass
  bins, and terminate bins
- CABAC context initialization from slice QP using the H.264 initialization
  formula
- CABAC unary and truncated-unary syntax helpers for upcoming macroblock and
  residual syntax elements
- CABAC I-slice `mb_type` syntax bridge that returns the standard
  `ISliceMacroblockType`
- CABAC I-slice macroblock-type neighbor context tracking for previously
  reconstructed left/top macroblocks
- CABAC Intra4x4 prediction-mode decision/rem syntax using context-coded bins
- CABAC intra chroma prediction-mode syntax using left/top chroma-mode context
- CABAC `mb_qp_delta` syntax and local reconstruction-QP state tracking
- I_PCM raw-sample escape path for 10-bit 4:2:2 macroblocks
- CABAC coded-block-pattern decoder for intra macroblocks
- bounded CABAC 4x4 residual decoder for coded-block flag,
  significant/last-significant flags, sign bypass bins, and coefficient level
  prefixes
- preparatory CABAC residual-category context primitives for luma16 DC,
  luma16 AC, luma4x4, chroma422 DC, and chroma422 AC.
- H.264 `scan8`-shaped non-zero residual neighbor-state model for progressive
  4:2:2 pictures, including luma16 DC, luma4x4, chroma422 DC, chroma422 AC,
  and I_PCM marking.
- standalone 4:2:2 CABAC residual syntax engine that combines residual
  category contexts with the non-zero neighbor-state model for Intra16x16
  luma, Intra4x4 luma, and 4:2:2 chroma DC/AC syntax.
- Intra16x16 luma DC inverse Hadamard/quantization transform and DC+AC merge
  into sixteen 4x4 reconstruction residual blocks.
- 4:2:2 chroma DC inverse transform and DC+AC merge into eight chroma 4x4
  reconstruction residual blocks per chroma plane.
- CABAC `transform_size_8x8_flag` syntax for High-profile I_NxN macroblocks,
  including left/top neighbor context tracking from ctxIdx 399..401.
- corrected High-profile IntraNxN parse order: `transform_size_8x8_flag` is
  parsed before Intra4x4/Intra8x8 prediction-mode syntax.
- standalone 8x8 residual coefficient model with H.264 8x8 scan order.
- CABAC 8x8 residual parser for block category 5 using ctxIdx 402..459
  significant/last/level contexts.
- native Sony path now reads through Intra8x8 prediction-mode syntax, coded
  block pattern, QP delta, 8x8 luma residual coefficient syntax, and 4:2:2
  chroma residual syntax before stopping at the missing Intra8x8 picture
  reconstruction step.
- The category/non-zero residual syntax engine is intentionally not yet active
  in the Sony picture path. A measured activation attempt with the current
  category/non-zero residual engine reduced the first-picture boundary from
  1,344/8,160 to 703/8,160 macroblocks, so the active Sony path was returned
  to the current stable subset. The new engine remains a tested decoder brick
  for the next more precise activation step rather than weakening the active
  path.
- luma residual wiring into `Intra16x16` picture reconstruction
- Intra16x16 DC, vertical, and horizontal luma prediction wiring
- Intra16x16 plane prediction wiring
- IntraNxN/Intra4x4 luma reconstruction into the decoded-frame store
- permissive DC fallback for unavailable intra prediction neighbors while the
  full H.264 neighbor-availability model is being completed
- P-skip branch that copies from L0 reference frame 0 using the current
  macroblock motion predictor
- 16x16 motion compensation for 4:2:2 Y/Cb/Cr planes, including luma
  half/quarter interpolation and chroma bilinear interpolation
- 16x8 and 8x16 inter-prediction pixel-region writers for 4:2:2 Y/Cb/Cr
  geometry
- 8x8, 8x4, 4x8, and 4x4 B sub-partition pixel-region writers through the
  shared 4:2:2 region motion-compensation path
- per-picture motion-vector field for neighboring macroblock predictor state
- P-slice macroblock type model and CABAC parser for `P_L0_16x16` and related
  table entries
- B-slice macroblock type model and CABAC parser for direct, L0, L1,
  bidirectional 16x16/16x8/8x16, B_8x8, and intra macroblock branches
- B-slice CABAC context initialization for the PB `cabac_init_idc=0` skip and
  macroblock-type contexts used by the current professional 4:2:2 Long-GOP
  fixture
- B-slice direct and bidirectional motion-compensation path using L0/L1 decoded
  reference frames from the native decoded-picture store
- B_8x8 sub-macroblock type parsing for direct, 8x8, 8x4, 4x8, and 4x4
  L0/L1/Bi sub-partitions
- CABAC motion-vector-difference component decoder with signed bypass suffix
  handling
- first non-skip `P_L0_16x16` path for ref0 median motion-vector prediction
  plus luma-residual inter prediction
- block-level Cb/Cr residual wiring for 4:2:2 macroblock planes so chroma
  coded-block-pattern presence no longer immediately stops supported I/P
  macroblocks
- CABAC coefficient level suffix decoding beyond the previous prefix-only
  subset
- native decompressor demo example:
  `cargo run -q -p qgs-h264-422p10 --example decode_fixture -- <annex-b.h264>`
- native decompressor demonstration on the checked-in professional High 4:2:2
  10-bit Long-GOP fixture:
  - 12 access units submitted
  - 12 owned `yuv422p10le` frames produced
  - 491,520 decoded plane bytes emitted
  - output order follows the parsed DPB/reorder path rather than raw decode
    submission order
- exact Sony FX6 original profile contract
- access-unit inspection through `qgs-codec-h264`
- Long-GOP decode/output planner backed by `qgs-codec-h264` DPB state
- `yuv422p10le` owned plane/frame model
- byte-size and plane-shape validation for 10-bit 4:2:2 output
- Vulkan upload adapter boundary for future native `yuv422p10le` frames
- focused qgs-test command for original MXF access-unit submission:
  `cargo run -q -p qgs-test -- --native-original-video-decode <original-mxf>`
- Sony FX6 sample 002 original MXF path validation up to the current native
  decoder boundary:
  - profile detected as H.264 High 4:2:2 10-bit
  - coded 1920x1088, visible 1920x1080
  - 106 original MXF video access units detected
  - first AU contains four I slices with `first_mb` starts
    `0,2040,4080,6120`
  - current per-slice native boundary report: `0:56`, `2040:676`,
    `4080:591`, `6120:21`, where each pair is `first_mb:reconstructed_count`
  - no proxy video, proxy AAC, FFmpeg, rsmpeg, libavcodec, or libavformat is
    used by this native decode command
  - current native Sony boundary: the first picture reaches the real
    High-profile Intra8x8 picture reconstruction boundary. The native path
    parses the `transform_size_8x8_flag`, Intra8x8 prediction-mode syntax,
    coded block pattern, QP delta, 8x8 luma residual coefficients, and 4:2:2
    chroma residual syntax for that macroblock, then stops without emitting
    fake or partial planes.

Not implemented yet:

- CAVLC residual decode into macroblock coefficients
- full H.264 CABAC residual context activation in the Sony picture path
- Intra8x8 luma prediction, inverse 8x8 transform/quantization wiring, and
  picture reconstruction into the 10-bit 4:2:2 frame store
- PB CABAC initialization tables for `cabac_init_idc=1` and `cabac_init_idc=2`
- spec-complete intra neighbor-availability handling; current unavailable
  neighbor fallback is intentionally permissive to keep native plane output
  moving while exact macroblock syntax is completed
- full reference-index syntax and multi-reference motion-vector prediction;
  current inter path uses the first parsed L0/L1 reference entries exposed by
  `qgs-codec-h264`
- weighted prediction / weighted bipred
- spec-complete H.264 4:2:2 chroma residual activation in the Sony picture
  path
- deblocking
- complete Sony FX6 first-picture decode; the original MXF access-unit path is
  now connected to the native decoder, but full Sony picture reconstruction is
  still blocked by the Intra8x8 reconstruction kocka
- replacement of the legacy `qgs-software-video` original-video path

This is not a completed decoder. The crate intentionally does not expose a
fake decode method that only returns an error. The current decoder returns
real `yuv422p10le` planes for a fully supported access unit and refuses to
emit partial/fake planes when it reaches unsupported inter prediction,
unsupported intra prediction, or unsupported residual mapping. Phase 2 must
not start until this crate produces real `yuv422p10le` planes from the Sony FX6
original access units needed by the live path.

The current decompressor demo uses the existing professional H.264 High 4:2:2
10-bit Long-GOP fixture and produces owned pixel planes for all 12 frames. This
is native pixel reconstruction evidence for that bounded fixture.

The Sony FX6 original-MXF path is now wired far enough to extract and submit
real original access units to this crate. It is not yet original-live-path
success evidence: the first Sony picture currently reaches the real
High-profile Intra8x8 reconstruction boundary and then stops without emitting
fake or partial output.

## Current Broadcast Player Boundary

`qgs-broadcast-player` still uses the legacy `qgs-software-video`/rsmpeg path
for original-video monitor pixels. That path is explicitly temporary. Once
`qgs-h264-422p10` produces real planes, the original live path should switch to:

```text
original MXF video access units
 -> qgs-codec-h264
 -> qgs-h264-422p10
 -> yuv422p10le planes
 -> qgs-vulkan::GpuFrameProcessor
```

The proxy path remains:

```text
proxy MP4 video
 -> qgs-mp4 / qgs-codec-h264
 -> qgs-vaapi
```

## Non-Claims

This checkpoint does not implement:

- real display output
- Wayland/Vulkan swapchain
- `real_display`
- `visual_verified`
- realtime playback
- A/V sync
- FFmpeg/rsmpeg/libav-backed original decode replacement

`qgs-media-runtime` remains backend-neutral and must not gain a decoder
dependency.

# QGS M2 Step 15 Report: H.264 VA Submission Differential Diagnosis

## Summary

Step 15 identified and fixed the QGS VA H.264 submission bug responsible for the
Step 14 clean decode collapse on Intel HD Graphics 4600 / i965.

Root cause:

```text
QGS submitted all H.264 slices for a picture as:

PictureParameter
IQMatrix
one VASliceParameterBufferH264 array containing all slices
one concatenated VASliceDataBuffer

with non-zero slice_data_offset values into that concatenated data buffer.
```

The i965 path accepted the submission and produced frames, but `vaEndPicture`
blocked for roughly 100 ms per picture. FFmpeg's VAAPI H.264 path submits each
slice as its own slice-parameter/slice-data pair. QGS now uses the same semantic
layout while remaining on the safe libva wrapper:

```text
PictureParameter
IQMatrix
SliceParameter(slice 0)
SliceData(slice 0)
SliceParameter(slice 1)
SliceData(slice 1)
...
```

Clean proxy decode improved from about 9 fps to about 572 fps in the
non-traced development run, while still producing 106 / 106 presentation frames.

## 1. vaEndPicture Wrapper-Path Finding

`libva 0.1.4` `Picture::end()` is a thin typestate wrapper around the C
`vaEndPicture` call. It does not perform an implicit surface sync, readback, or
large Rust-side cleanup inside the measured path.

The Step 14 timing therefore represented time in the driver/API call reached
through `Picture::end()`, not hidden wrapper destruction.

## 2. Selected QGS Parameter Dumps

Before the fix, QGS libva trace for the clean proxy showed:

```text
vaCreateSurfaces: width=1920 height=1080
vaCreateContext:  width=1920 height=1080
VAPictureParameterBufferH264.picture_height_in_mbs_minus1 = 67
vaRenderPicture num_buffers = 4
```

`picture_height_in_mbs_minus1 = 67` means a coded H.264 height of 1088 pixels.
QGS was allocating visible-height surfaces for a cropped stream.

After the coded/visible cleanup and slice-buffer fix, trace showed:

```text
vaCreateSurfaces: width=1920 height=1088
vaCreateContext:  width=1920 height=1088
VAPictureParameterBufferH264.picture_height_in_mbs_minus1 = 67
vaRenderPicture num_buffers = 10 for a four-slice picture
VASliceParameterBufferH264 slice_data_offset = 0 for each slice
```

## 3. VA Struct Initialization Audit

The Rust `libva` H.264 buffer wrappers initialize reserved fields with
`Default::default()` and construct the VA structs deterministically:

- `PictureH264::new` initializes `va_reserved`.
- `PictureParameterBufferH264::new` initializes all fields and `va_reserved`.
- `SliceParameterBufferH264::add_slice_parameter` initializes all fields and
  `va_reserved`.
- `IQMatrixBufferH264::new` initializes the scaling lists and `va_reserved`.

No evidence of uninitialized VA H.264 parameter bytes was found.

## 4. Reference-Frame Audit

QGS unused reference entries use `VA_INVALID_SURFACE` and
`VA_PICTURE_H264_INVALID` through `invalid_picture()`.

Reference surfaces used in `ReferenceFrames[]`, `RefPicList0`, and `RefPicList1`
are resolved from QGS codec-owned picture identity. No reference-lifetime leak
into qgs-vaapi was introduced.

## 5. POC / frame_num Audit

The proxy remains progressive H.264 High 8-bit 4:2:0 with Long-GOP I/P/B
structure. QGS preserves:

- H.264 `frame_num`
- POC values
- PicNum ordering after wrap
- decode order distinct from presentation order

The VA submission still derives `CurrPic`, `TopFieldOrderCnt`,
`BottomFieldOrderCnt`, and reference-list entries from the qgs-codec-h264 state.
The performance bug was not caused by the Step 12 frame-num wrap fix.

## 6. Slice-Data Audit

QGS submits complete NAL-unit bytes for each slice, with emulation-prevention
bytes preserved as expected by VA slice data. The slice header bit offset is
provided through `slice_data_bit_offset`.

The incorrect part was not the bytes themselves. It was the layout:

- old: one slice-parameter array plus one concatenated slice-data buffer
- fixed: one slice-parameter buffer paired with one slice-data buffer per slice

For per-slice buffers, `slice_data_offset` is zero because each slice-data buffer
starts at the corresponding slice.

## 7. Slice-Count Findings

The clean Sony FX6 sample 002 proxy uses multiple slices per picture. The first
IDR picture contains four slices. Fixed trace for that picture showed four
`VASliceParameterBufferH264` entries, each paired with its own slice-data buffer.

## 8. Buffer-Order Audit

The safe wrapper can issue one `vaRenderPicture` call per `Picture`, so QGS
cannot exactly mirror FFmpeg's multiple render calls without raw VA FFI.

However, the fixed QGS buffer order is now semantically equivalent:

```text
PictureParameter
IQMatrix
SliceParameter 0
SliceData 0
SliceParameter 1
SliceData 1
SliceParameter 2
SliceData 2
SliceParameter 3
SliceData 3
```

This was sufficient to remove the `vaEndPicture` stall.

## 9. Buffer-Lifetime Audit

QGS still lets the safe wrapper own VA buffers until the ended `Picture` is
retained or recycled. The performance fix did not require changing buffer Drop
or surface typestate behavior.

The current safe-wrapper lifetime remains acceptable for this path.

## 10. Context / Config Comparison

Observed differences with FFmpeg:

- FFmpeg creates 1920x1088 surfaces/context for the cropped 1920x1080 stream.
- QGS now also creates 1920x1088 surfaces/context.
- FFmpeg creates a context with `num_render_targets = 0`; QGS supplies its
  bounded surface pool. Step 14 pool experiments showed this was not the
  dominant factor.
- FFmpeg does not need QGS's public resource identity/lifetime model, but QGS's
  context setup is no longer the measured bottleneck.

## 11. Render-Target Comparison

QGS continues to create a bounded 24-surface default pool for the proxy proof.
After the slice-buffer fix, pool size is no longer masking a submission stall:

```text
pool=8:  633.38 fps
pool=12: 589.18 fps
pool=16: 667.32 fps
pool=24: 674.67 fps
pool=32: 591.46 fps
```

These are development observations, not benchmark numbers.

## 12. Synthetic IDR Timing

The daemon-backed tiny IDR hardware proof was rerun as part of the final
hardware regression. It passed on Intel VA-API.

## 13. Synthetic Long-GOP Timing

The daemon-backed 12-frame 8-bit 4:2:0 Long-GOP hardware proof was rerun as
part of the final hardware regression. It passed on Intel VA-API.

## 14. Proxy Timing Distribution

The existing aggregate counters are sufficient for the root cause:

Before fix:

```text
Clean VA hardware decode: 106 frames, 11.762 s, 9.01 fps
vaEndPicture: 11677.919 ms aggregate (~99.3%)
```

After fix, non-traced run:

```text
Clean VA hardware decode: 106 frames, 0.185 s, 572.27 fps
vaEndPicture: 26.586 ms aggregate (~15.4%)
```

A traced run with `LIBVA_TRACE_BUFDATA=1` confirmed the fixed buffer layout but
ran slower due to trace overhead:

```text
Clean VA hardware decode: 106 frames, 0.494 s, 214.47 fps
vaEndPicture: 23.963 ms aggregate
```

## 15. FFmpeg Semantic Comparison

FFmpeg VAAPI trace for the same proxy showed:

- 1920x1088 VA surfaces/context for the cropped 1920x1080 stream
- H.264 picture height `67` macroblocks minus one
- per-slice slice parameters and per-slice slice data
- no single concatenated slice-data buffer for the first four-slice IDR picture

QGS now matches the important slice-data submission semantics while preserving
its own MXF/MP4/H.264 frontend and VA backend architecture.

## 16. libva Trace Comparison

The key trace-level difference before the fix was:

```text
QGS old:    one slice-parameter array + one concatenated slice-data buffer
FFmpeg:     slice-parameter/slice-data pairs per slice
QGS fixed:  slice-parameter/slice-data pairs per slice
```

The fixed QGS trace shows one `vaRenderPicture` call with ten buffers for the
first four-slice picture. The safe wrapper still batches the call, but the
buffer contents are paired per slice and `slice_data_offset` is zero for each
slice-data buffer.

## 17. Concrete Differences Found

Two concrete differences were found:

1. QGS previously used cropped visible dimensions as H.264 coded dimensions.
   This is now fixed by separating macroblock-coded dimensions from visible
   region metadata.
2. QGS previously submitted H.264 slice data as one concatenated buffer indexed
   by per-slice offsets. This triggered the i965 `vaEndPicture` stall. QGS now
   submits one slice-parameter buffer and one slice-data buffer per slice.

The second difference is the throughput root cause.

## 18. Root Cause

The root cause was QGS VA H.264 slice submission layout. The old layout was
accepted by the API and decoded correctly enough to produce outputs, but i965
handled it with a severe synchronous stall in `vaEndPicture`.

The safe, proven fix is to submit each H.264 slice as its own VA slice-parameter
buffer plus its own slice-data buffer.

## 19. Fix

Implemented fixes:

- `qgs-codec-h264` now reports H.264 macroblock-coded dimensions separately
  from cropped visible dimensions.
- `qgs-mp4` keeps visible dimensions for technical inspection while exposing
  coded dimensions for decoder configuration.
- `qgs-software-video` accepts decoder output dimensions that are visible-size
  and no larger than the configured coded allocation.
- `qgs-vaapi` now creates per-slice VA parameter/data buffer pairs.
- Added a unit test proving per-slice VA parameter buffer construction.

No raw VA FFI was added.

## 20. Before / After Throughput

Development observations on Intel HD Graphics 4600 / i965:

| Path | Before | After |
| --- | ---: | ---: |
| Container + H.264 frontend | ~1611 fps | 1689.60 fps |
| Clean QGS VA decode | ~9 fps | 572.27 fps |
| FFmpeg VAAPI reference | ~530 fps | unchanged reference |
| Diagnostic QGS VA decode | ~1.2 fps | 2.08 fps |

Diagnostic decode remains dominated by explicit validation/readback/export work.

## 21. Correctness Result

The clean proxy proof decoded:

```text
106 / 106 presentation frames
```

Selected diagnostic checksums after the fix:

```text
frame 0:   0x8ec49c02
frame 53:  0xcfe7774b
frame 105: 0x743d3b95
```

The checksum values changed from the old path, which is expected because the
old submission shape was not the reference-good decode path.

## 22. Tests

Added/updated tests:

- H.264 professional fixture now asserts coded `128x80` and visible `128x72`.
- qgs-vaapi asserts one VA slice-parameter buffer per H.264 slice, each with
  offset zero into its paired slice-data buffer.

Quality gates:

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed
- `cargo test --workspace`: passed
  - qgs-codec-h264: 8 tests
  - qgs-core: 26 tests
  - qgs-linux: 4 tests
  - qgs-mp4: 5 tests
  - qgs-mxf: 31 tests
  - qgs-protocol: 118 tests
  - qgs-software-video: 5 tests
  - qgs-test: 4 tests
  - qgs-vaapi: 12 tests
  - qgs-vulkan: 11 tests

Hardware/proof runs:

- `qgs-test --h264-decode-only`: passed.
  - Intel HD Graphics 4600: tiny IDR VA decode passed.
  - Intel HD Graphics 4600: 12-frame synthetic Long-GOP VA decode passed.
  - NVIDIA GTX 950M/NVK: no VA decode capabilities; software fallback passed.
- Clean proxy throughput proof: passed, 106 / 106 frames, 572.27 fps in the
  non-traced development run.

## 23. Safety Result

No unsafe Rust was added. `qgs-vaapi` remains:

```rust
#![forbid(unsafe_code)]
```

The fix stays within the existing safe libva API. No raw VA backend is required
for this bug.

## 24. Recommendation

Playback scheduling can now proceed from a much healthier production VA decode
baseline. Future work may still improve surface lifetime and scheduling, but
Step 15 does not justify a raw VA backend or wrapper migration.

QGS VA SUBMISSION BUG FIXED

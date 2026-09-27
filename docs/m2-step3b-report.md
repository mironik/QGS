# QGS M2 Step 3B Report

## Summary

M2 Step 3B implements the first narrow H.264 hardware decode proof. The
accepted frontend boundary is now represented in code:

```text
compressed H.264 Annex B access unit
    -> qgs-codec-h264
    -> QGS-owned parsed H.264 descriptions
    -> qgs-vaapi
    -> VA-API H.264 VLD
    -> VA NV12 surface
    -> QGS VideoSurface ResourceId
```

This milestone proves Intel i965 VA-API H.264 decode for a tiny synthetic
8-bit 4:2:0 progressive IDR stream. It does not implement general H.264 decode,
MPEG-2 decode, XDCAM, XAVC, FFmpeg integration, software fallback, or VA surface
import into Vulkan.

## Files And Crates Changed

- Added `crates/qgs-codec-h264`.
- Added H.264 decoder lifecycle protocol and tests in `qgs-protocol`.
- Added session-owned decoder lifecycle and decoded `VideoSurface` registration
  in `qgs-core`.
- Added VA-API H.264 VLD decode proof, validation readback, and DRM PRIME export
  probe in `qgs-vaapi`.
- Added decoder request handling in `qgsd`.
- Extended `qgs-test` with `--h264-decode-only` hardware proof mode.
- Added synthetic fixture and provenance under `tests/fixtures/h264/`.
- Updated `docs/architecture.md` and `docs/protocol.md`.

## H.264 Parser Frontend

`qgs-codec-h264` uses `h264-reader 0.9.0` for Annex B NAL parsing and SPS/PPS
syntax parsing. Parser-library types do not escape the crate.

The Step 3B supported stream subset is deliberately narrow:

- Annex B H.264
- 8-bit
- 4:2:0
- progressive frame coding
- IDR/I slices
- no field pictures
- no MBAFF
- no P/B frames
- no slice groups

Unsupported features are rejected explicitly rather than silently interpreted as
the Step 3B subset.

## Fixture

Fixture:

- `tests/fixtures/h264/idr-64x64-baseline.h264`

Generated locally with FFmpeg from the synthetic `testsrc2` source. FFmpeg is a
development fixture-generation tool only; QGS has no FFmpeg runtime or code
dependency.

Fixture properties:

- 64 x 64
- 1 frame
- H.264 Constrained Baseline
- 8-bit
- 4:2:0
- progressive
- Annex B raw H.264
- IDR/I-frame only
- size: 2734 bytes

## Protocol Additions

New request opcodes:

- `CREATE_DECODER`: request kind `1`, opcode `11`
- `SUBMIT_ACCESS_UNIT`: request kind `1`, opcode `12`
- `DESTROY_DECODER`: request kind `1`, opcode `13`

New response opcodes:

- `DECODER_CREATED`: response kind `2`, opcode `12`
- `DECODE_OUTPUT`: response kind `2`, opcode `13`
- `DECODER_DESTROYED`: response kind `2`, opcode `14`

New stable errors:

- `InvalidDecoderId`: `50`
- `UnknownDecoder`: `51`
- `UnsupportedDecodeConfiguration`: `52`
- `MalformedCompressedData`: `53`
- `CompressedPacketTooLarge`: `54`
- `UnsupportedH264StreamFeature`: `55`
- `DecodeFailed`: `56`

The compressed access-unit payload limit is 4000 bytes. This stays within the
existing QGS v0.1 maximum wire payload size while accommodating the Step 3B
fixture. Raw decoded pixels are not carried in QGS protocol messages.

## Decoder Ownership

`DecoderId` is non-zero, opaque, session-owned, and not persistent across daemon
restarts. A decoded output surface is registered as a normal session-owned
`ResourceId` with `ResourceKind::VideoSurface`.

Destroying a decoder releases decoder backend state. Session disconnect releases
all still-owned decoders and decoded surfaces. Cross-session decoder access is
rejected through the existing session ownership model.

## VA-API Decode Path

`qgs-vaapi` owns:

- VA display/device binding
- VA config and context
- VA decode surface allocation
- translation from QGS-owned H.264 descriptions into VA H.264 parameter buffers
- picture, IQ matrix, slice parameter, and slice data buffers
- `vaBeginPicture` / `vaRenderPicture` / `vaEndPicture`
- decode completion through the safe libva picture `sync()` path
- validation-only surface readback
- DRM PRIME export probing

VA types do not escape `qgs-vaapi`.

## VideoSurface Output

Intel decode produced a session-owned QGS `VideoSurface` with:

- format: `Nv12`
- bit depth: 8
- chroma: `Cs420`
- scan mode: progressive
- coded size: 64 x 64
- visible region: 64 x 64

The `VideoSurface` is not modeled as a generic QGS `Image`.

## Validation

Step 3B uses validation-only CPU readback from the decoded VA surface after VA
decode completion. The daemon computed checksum `0x0bbbb30d` from the decoded
NV12 surface. This validates that VA produced readable decoded output, but it is
not the final zero-copy processing architecture.

No raw decoded frame pixels crossed normal QGS IPC.

## DRM PRIME Export Probe

After Intel decode, `qgs-vaapi` probed `vaExportSurfaceHandle` through the safe
libva wrapper. The probe succeeded:

- memory type: DRM PRIME descriptor path exposed by libva
- DRM fourcc: `0x3231564e` (`NV12`)
- size: 64 x 64
- exported objects: 1
- layers: 1
- object sizes: `[12288]`
- modifiers: `[72057594037927938]`
- layer formats: `[842094158]`
- pitches: `[[128, 128, 0, 0]]`
- offsets: `[[0, 8192, 0, 0]]`

All exported descriptor FDs are owned by the libva descriptor object and closed
on drop. Step 3B does not import this surface into Vulkan.

## Intel Result

Device:

- Intel HD Graphics 4600 / HSW GT2
- render node: `/dev/dri/renderD128`
- VA driver: i965

Result:

- QGS advertised H.264 Baseline/Main/High 8-bit 4:2:0 NV12 decode capability.
- `CREATE_DECODER` succeeded for H.264 Baseline 64 x 64 progressive.
- qgs-codec-h264 parsed the synthetic Annex B stream.
- qgs-vaapi created real VA H.264 config, context, and NV12 surface.
- VA picture parameter, IQ matrix, slice parameter, and slice data buffers were
  submitted.
- VA decode completed successfully.
- Output became a session-owned QGS `VideoSurface`.
- Validation-only readback succeeded.
- DRM PRIME export probe succeeded.
- Decoder and surface destruction succeeded.

## NVIDIA Result

Device:

- NVIDIA GeForce GTX 950M / NVK GM107
- render node: `/dev/dri/renderD129`
- VA driver: nouveau

Result:

- QGS advertised zero decode capabilities.
- `CREATE_DECODER` for the H.264 Baseline test configuration failed cleanly
  with `UnsupportedDecodeConfiguration`.
- No software fallback was attempted.
- No backend-specific error leaked through the protocol.

## Disconnect Cleanup

The hardware run deliberately left one decoded `VideoSurface` and one decoder
alive before client disconnect. `qgsd` logged:

```text
client disconnected; releasing 1 resource(s), 0 sync object(s), and 1 decoder(s) for session 1
qgs-core: releasing 1 resource(s) owned by session
qgs-core: releasing 1 decoder(s) owned by session
```

## Tests

`cargo test --workspace` passed.

Total: 155 tests passed.

- `qgs-codec-h264`: 2 passed
- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 113 passed
- `qgs-vaapi`: 8 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Safety

No new unsafe QGS-owned code was added in M2 Step 3B.

The unsafe inventory remains the Step 6 audited qgs-vulkan external-memory
import boundary. `qgs-protocol`, `qgs-core`, `qgs-linux`, `qgs-vaapi`, `qgsd`,
`qgs-test`, and `qgs-codec-h264` retain `#![forbid(unsafe_code)]`.

## Architectural Concerns

- The H.264 frontend currently supports only the narrow IDR/I-frame proof path.
- P/B frames, DPB management beyond this proof, interlaced streams, High 4:2:2,
  and 10-bit streams remain future work.
- MPEG-2/XDCAM remains a future `qgs-codec-mpeg2` style frontend, not a
  qgs-vaapi parser responsibility.
- XAVC-relevant H.264 High 4:2:2 10-bit streams are recognized as outside the
  current supported subset.
- VA NV12 surface import into Vulkan is not implemented yet. The successful DRM
  PRIME export probe is the evidence for M2 Step 4 planning.
- The 4000-byte compressed packet limit is an M2 proof limit, not a final
  ingest design.

## Commit And Push Verification

This report is included in the M2 Step 3B commit. The exact commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit creation and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

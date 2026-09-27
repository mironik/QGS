# QGS M2 Step 1 Report

## VideoCodec Model

M2 Step 1 adds QGS-owned video codec values:

- `H264`
- `Mpeg2`

No HEVC, AV1, Sony product names, container names, or vendor API enums were
added.

## Profile Model

Profiles are codec-specific:

- H.264: `Baseline`, `Main`, `High`, `High422`
- MPEG-2: `Main`, `Profile422`

The wire decoder validates profile values in the context of the selected codec.
QGS does not expose VA-API, Vulkan Video, or vendor profile enum values.

## Bit-Depth Model

Bit depth is explicit through `BitDepth`. Valid values are:

- `8`
- `10`
- `12`

Invalid bit depths are rejected by protocol/model validation.

## Chroma Model

QGS now models:

- `4:2:0`
- `4:2:2`
- `4:4:4`

This avoids a 4:2:0-only assumption and supports representation of broadcast
4:2:2 workflows without encoding product-format semantics.

## Scan And Field Model

QGS models:

- `Progressive`
- `Interlaced`

Field order values are:

- `Unknown`
- `TopFieldFirst`
- `BottomFieldFirst`

No deinterlacing or frame-rate/timeline behavior was added.

## VideoSurface Model

`ResourceKind` now includes conceptual `VideoSurface` alongside `Buffer` and
`Image`. M2 Step 1 does not allocate or export real video surfaces.

`VideoSurfaceDesc` contains:

- coded width and height
- visible region
- video surface format
- explicit bit depth
- chroma subsampling
- scan mode
- field order

No timestamps, PTS/DTS, timeline/project concepts, MXF metadata, or decode
state were added.

## VideoSurfaceFormat Model

Initial QGS-owned surface formats:

- `Nv12`
- `P010`
- `Yuv422_8`
- `Yuv422_10`

Bit depth and chroma remain independently queryable. QGS does not assume that
all video surfaces are NV12 or Vulkan images.

## Color Metadata Decision

Color primaries, transfer characteristics, matrix coefficients, and range were
not added in M2 Step 1. The model boundary is documented, but adding partial
color metadata without a backend or decode path would expand the milestone
without improving implemented behavior.

## Protocol Additions

New request:

- `QUERY_VIDEO_CAPABILITIES`: request kind `1`, opcode `10`

New response:

- `VIDEO_CAPABILITIES`: response kind `2`, opcode `11`

New stable errors:

- `UnsupportedVideoCodec`: `39`
- `UnsupportedVideoProfile`: `40`
- `InvalidVideoBitDepth`: `41`
- `UnsupportedChromaSubsampling`: `42`
- `UnsupportedScanMode`: `43`
- `UnsupportedFieldOrder`: `44`
- `UnsupportedVideoSurfaceFormat`: `45`
- `InvalidVideoSurfaceDimensions`: `46`
- `InvalidVideoVisibleRegion`: `47`
- `VideoDecodeCapabilityCountTooLarge`: `48`
- `VideoOutputFormatCountTooLarge`: `49`

`qgsd` gates the query on an established session. For known devices it returns
an empty capability list because no real video decode backend exists yet.
Unknown devices return the existing stable `UnknownDeviceId` error.

## Wire Limits

- maximum payload size: 4096 bytes
- maximum video decode capability count: 32
- maximum output surface formats per decode capability: 8
- maximum VideoSurface coded width: 8192
- maximum VideoSurface coded height: 8192

All wire counts are validated before allocation. Unknown enum values, invalid
bit depths, truncated entries, malformed reserved/boolean fields, invalid
dimensions, and trailing bytes are rejected.

## Reference Representation Tests

Pure model tests demonstrate representation of:

- H.264, 8-bit, 4:2:0, progressive
- MPEG-2, 8-bit, 4:2:2, interlaced, top-field-first workflow semantics
- H.264, 10-bit, 4:2:2, progressive

These tests do not claim hardware support.

## VA-API Probe

`vainfo` is not installed on the current system:

```text
vainfo: command not found
```

The system exposes DRM/render nodes:

```text
/dev/dri/card0
/dev/dri/card1
/dev/dri/renderD128
/dev/dri/renderD129
```

No VA-API decode capability claim is made in this milestone.

## Vulkan Video Probe

`vulkaninfo` is installed. A read-only probe found the existing Vulkan devices
but no `VK_KHR_video_*` or Vulkan Video decode extension matches in the current
reported extension data.

Observed devices:

- Intel HD Graphics 4600, Mesa Intel Vulkan, API 1.2.335
- NVIDIA GeForce GTX 950M, Mesa NVK, API 1.3.335
- llvmpipe, Mesa llvmpipe, API 1.4.335

No Vulkan Video decode capability claim is made.

## Intel HD 4600 Observations

The Intel device remains discoverable as an integrated GPU. The current Vulkan
probe did not advertise Vulkan Video decode extensions. VA-API could not be
queried because `vainfo` is missing.

## GTX 950M / NVK Observations

The NVIDIA GTX 950M remains discoverable through NVK as a discrete GPU. The
current Vulkan probe did not advertise Vulkan Video decode extensions. VA-API
could not be queried because `vainfo` is missing.

## Test Results

`cargo test --workspace` passed.

Total: 137 tests passed.

- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 105 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## IPC Demonstration

After rebuilding normal binaries, `qgsd` and `qgs-test` exchanged
`QUERY_VIDEO_CAPABILITIES` / `VIDEO_CAPABILITIES` for all discovered devices.
Each device reported:

```text
Video decode:
  advertised capabilities: 0
  backend: not implemented in M2 Step 1
```

The later legacy M1 demo path in the same `qgs-test` run hit an NVK
`ExportFailed` during external sync export. That is outside the M2 Step 1
video model/protocol scope and did not affect the video capability query.

## Unsafe Inventory

No new unsafe code was added. The unsafe inventory remains the M1 inventory:
six unsafe blocks, all isolated inside audited `qgs-vulkan` modules. All other
QGS crates remain `#![forbid(unsafe_code)]`.

## Commit And Push Verification

This report is included in the M2 Step 1 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Architectural Concerns

No Qnc, Sony, XDCAM, XAVC, MXF parsing, FFmpeg, VA-API decode, Vulkan Video
decode, video encode, scheduling, telemetry, or public shader/compute API was
added.

`qgs-protocol` and `qgs-core` use QGS-owned video types only. No backend-specific
VA-API or Vulkan Video types escape into the public model.

The main architectural concern is backend selection. The current Vulkan stack
does not advertise Vulkan Video decode support, and VA-API could not be probed
because `vainfo` is unavailable. QGS therefore correctly returns an empty video
capability set today.

## First Decode Backend Recommendation

Recommendation: investigate VA-API first.

This is based on the current probe: Vulkan Video decode extensions were not
advertised by the installed Vulkan stack for the target devices, while VA-API
could not yet be queried only because the user-space probe tool is missing.
Before implementing decode, the next step should be a read-only VA-API
development/runtime availability check and a real capability probe. QGS should
not claim H.264 or MPEG-2 decode support until that backend proves it.

# QGS M2 Step 3A H.264 Decode Frontend Architecture

This is a research spike for the first real QGS H.264 decode milestone. It
does not implement decoding and does not define final protocol wire messages.

## 1. VA-API H.264 Decode Input Requirements

VA-API H.264 VLD decode is not a "submit compressed bytes and forget" API.
The client side of VA-API must parse enough H.264 syntax to populate codec
parameter buffers and provide the original slice bytes.

Per sequence, the VA backend needs SPS-derived state:

- coded dimensions in macroblocks
- profile, level, bit depth, and chroma format
- `num_ref_frames`
- frame numbering and picture-order-count modes
- `frame_mbs_only_flag`, MBAFF/direct-8x8-related flags, and related sequence
  flags
- VUI/SEI-derived timing or scan metadata where QGS decides to preserve it

Per picture, it needs PPS/SPS/slice-derived state:

- current output surface
- DPB reference frame list as VA picture descriptors
- picture width/height in macroblocks
- bit depth and chroma values
- `seq_fields` and `pic_fields`
- initial QP/QS and chroma QP offsets
- `frame_num`
- field-picture and reference-picture flags
- inverse quantization/scaling matrices

Per slice, it needs:

- original NAL/slice bytes in a `VASliceDataBuffer`
- `slice_data_size`, `slice_data_offset`, and `slice_data_bit_offset`
- `first_mb_in_slice`, `slice_type`, CABAC/deblocking/QP fields
- active reference counts
- reference picture lists 0 and 1
- prediction weight table values when present

The important detail is `slice_data_bit_offset`: VA-API defines it as the bit
offset from the NAL header to `slice_data()` after emulation-prevention bytes
are removed, while the data buffer still carries the original bitstream bytes.
That means the frontend must parse slice headers accurately; simple NAL
splitting is not enough.

VA-API decode submission shape is:

```text
vaBeginPicture(target_surface)
    picture parameter buffer
    IQ matrix buffer
    one or more slice parameter buffers
    one or more slice data buffers
vaRenderPicture(...)
vaEndPicture(...)
```

The existing Rust `libva 0.1.4` crate already provides safe wrappers for these
H.264 VA buffers: `PictureParameterBufferH264`,
`SliceParameterBufferH264`, and `IQMatrixBufferH264`. It also provides safe
context buffer creation and surface APIs. This suggests Step 3B can stay in
safe QGS-owned Rust unless surface export/import or an uncovered VA function
requires a new audited boundary.

## 2. Parser And Frontend Options Evaluated

### Small Rust H.264 Parser Crate

`h264-reader 0.9.0` is mature enough to parse Annex B and AVCC bytestreams,
SPS, PPS, SEI subsets, access unit delimiters, and slice headers. It is
licensed MIT/Apache-2.0, matching QGS well. Its README notes that full
`slice_data()` support is baseline-profile oriented, but VA-API does not need
QGS to parse full slice payloads; it needs the header fields plus original
slice bytes. It is a strong candidate if its slice-header model exposes every
field needed for P/B slices, weighted prediction, and CABAC paths on real Main
and High streams.

`h264-parser 0.4.2` is a smaller MIT crate that advertises Annex B parsing,
SPS/PPS, slice headers, and access unit assembly. Local source inspection shows
simple public structures for SPS, PPS, slice headers, and access units. It is
attractive for a tiny proof, but it appears less established than
`h264-reader` and would need careful validation against Main/High streams,
DPB handling, and reference list requirements.

`scuffle-h264 0.2.2` is MIT/Apache-2.0 and cleanly parses/builds H.264 header
structures, including SPS bit depth/chroma fields and AVCC records. Its README
states that it is under active development and may not be stable. It looks more
like a header/config parser than a decode frontend.

`cros-codecs 0.0.6` is BSD-3-Clause and already contains a hardware decode
architecture for Linux, including H.264 parsing, DPB management, and VA-API
backend translation. Its README describes VAAPI decoder support for H.264,
H.265, VP8, VP9, and AV1. Local source inspection shows a useful separation:
`codec::h264::parser`, `codec::h264::dpb`, a stateless H.264 decoder frontend,
and `decoder::stateless::h264::vaapi` translating parsed data into VA buffers.
This is the best architectural reference and possibly a future dependency, but
it is larger than a parser crate and brings backend abstractions QGS may not
want to adopt wholesale.

### FFmpeg / libavcodec Parser Frontend

FFmpeg has industrial-strength H.264 parsing and decode frontend code. It is
the most battle-tested option for professional media, including edge cases,
interlacing, B-frames, reordering, MXF-originated streams, and high-profile
syntax. It is also a large dependency family. Rust bindings such as
`ffmpeg-next` default to broad FFmpeg components like codec, format, filters,
devices, scaling, and resampling unless carefully feature-limited.

Licensing is also a deployment concern. FFmpeg itself is LGPL in many builds,
but common distribution builds may include GPL components depending on enabled
libraries. Using parser-only/libavcodec APIs can likely be done in an
LGPL-compatible way, but QGS would still inherit significant dynamic-linking,
distribution, and system-package complexity.

FFmpeg is the strongest correctness reference and fallback option, but it is
not the best first QGS dependency if the goal is a minimal vendor-neutral
system service.

### GStreamer Codec Parsing Components

GStreamer has mature H.264 parsing through codecparser APIs such as
`gst_h264_parser_parse_sps`, `gst_h264_parser_parse_pps`, and
`gst_h264_parser_parse_slice_hdr`. It is professionally maintained and widely
used in Linux media pipelines.

The downside is architectural weight. Pulling GStreamer into QGS as an
in-process parser dependency introduces GLib/GObject runtime conventions,
plugin packaging, versioning, and deployment surface. On this machine,
`gst-launch-1.0` and `gst-inspect-1.0` are installed, but `h264parse` and
encoder plugins such as `x264enc`, `openh264enc`, and `avenc_h264` are not
available. The codecparser library might still be installable separately, but
using it directly from Rust would require either C FFI/sys bindings or a broader
GStreamer integration.

GStreamer is a good external pipeline option later, but not the cleanest first
QGS-internal parser frontend.

### Minimal QGS-Owned H.264 Parsing

A tiny QGS parser that only splits Annex B NAL units and reads SPS/PPS/slice
headers is tempting, but H.264 decode quickly needs more than "tiny":

- Exp-Golomb parsing and RBSP emulation-prevention handling
- SPS/PPS state and changes mid-stream
- access unit boundaries
- POC and frame numbering
- IDR and non-IDR picture handling
- DPB reference tracking
- ref list modification
- memory management control operations
- weighted prediction
- field pictures and MBAFF for broadcast material

Writing this from scratch would be a correctness and security liability. It
would also work against the "safe Rust by default" policy unless we are very
disciplined, and it would delay the VA-API backend on parser work rather than
QGS architecture.

### Established Linux/Mesa/Rust Approach

The strongest established Rust/Linux pattern found is `cros-codecs`:

```text
encoded stream
    |
codec parser + stateless decoder frontend
    |
DPB and backend-neutral picture/slice state
    |
VA-API backend
```

QGS should learn from that split even if it does not import the whole crate
immediately.

## 3. Dependency And Licensing Analysis

Recommended first dependency candidate:

- `h264-reader 0.9.0`
- License: MIT/Apache-2.0
- Rust version: 1.83
- Scope: H.264 syntax, Annex B, AVCC, SPS, PPS, SEI subsets, slice headers
- Fit: small, Rust-native, compatible with QGS licensing and safe-Rust goals

Secondary candidate:

- `cros-codecs 0.0.6`
- License: BSD-3-Clause
- Scope: full Linux hardware codec architecture, including H.264 VAAPI
- Fit: excellent reference, but heavier than a parser and may duplicate QGS
  backend/resource ownership architecture

Less preferred for Step 3B:

- FFmpeg/libavcodec: mature and professionally robust, but broad dependency and
  licensing/deployment complexity
- GStreamer codecparser: mature, but broad GLib/GStreamer runtime and packaging
  surface
- `h264-parser`: small, but less proven for professional/editor decode
- `scuffle-h264`: promising header parser, but explicitly under active
  development and not sufficient alone for VA decode

## 4. Recommended Codec Frontend Architecture

QGS should not make `qgs-vaapi` a general H.264 parser. The cleaner long-term
boundary is:

```text
compressed access units / packets
    |
qgs-codec-h264 frontend
    |
QGS-owned parsed picture description
    |
qgs-vaapi
    |
VA picture, IQ, slice parameter buffers
```

For Step 3B, add a small frontend crate or module such as `qgs-codec-h264`.
Its responsibility should be:

- accept bounded compressed H.264 access-unit data
- handle Annex B start codes initially
- track SPS and PPS
- assemble/access access units
- parse slice headers
- maintain minimal DPB/picture-order state needed for decode submission
- produce QGS-owned parsed sequence, picture, and slice descriptions

`qgs-vaapi` should translate those descriptions into VA buffers and own the VA
display/config/context/surface details. VA structs remain private to
`qgs-vaapi`.

This avoids exposing VA structures through protocol, avoids making VA the
codec parser, and leaves room for a future Vulkan Video backend to consume the
same QGS-owned parsed picture description.

## 5. Recommended Rust/Library Choice

For M2 Step 3B, start with `h264-reader` for H.264 syntax parsing and add the
minimum QGS-owned glue for access-unit validation, DPB/reference tracking, and
VA parameter construction.

The implementation should begin with a deliberately narrow stream class:

- Annex B H.264 elementary stream
- one small progressive 8-bit 4:2:0 Baseline or constrained stream
- then expand to Main/High P/B-frame behavior once DPB validation is correct

Before committing to `h264-reader` for the whole H.264 path, Step 3B should
prototype one real slice-header-to-VA mapping and confirm that the crate
exposes all fields needed by `VASliceParameterBufferH264`. If it does not,
switch to either:

- directly reusing the relevant `cros-codecs` H.264 frontend concepts, or
- using `cros-codecs` itself as the frontend/backend reference while wrapping it
  behind QGS-owned APIs.

## 6. Proposed Future Decoder API Shape

The future application-facing API should accept compressed packets/access
units, not VA structs, SPS/PPS structs, or hardware-specific parse data.

Conceptual protocol shape:

```text
CREATE_DECODER(DeviceId, VideoDecodeConfig)
    -> DECODER_CREATED(DecoderId)

SUBMIT_PACKET(DecoderId, compressed bytes, packet metadata)
    -> PACKET_ACCEPTED or decoded-output event/response

OUTPUT VideoSurface(ResourceId or VideoSurfaceId)

DESTROY_DECODER(DecoderId)
```

Open design questions for Step 3B:

- whether compressed bytes travel in normal protocol messages for M2 only, with
  strict size limits, or through a future bulk-data path
- whether `SUBMIT_PACKET` accepts one access unit only or arbitrary packet
  fragments
- how decoded output readiness is reported without introducing a full async
  runtime
- whether `DecoderId` belongs to a session like `ResourceId` and `SyncId`

The API should carry QGS concepts:

- codec: H.264
- profile/bit depth/chroma expected or auto-detected
- packet format: Annex B initially, AVCC later
- coded/visible dimensions after sequence detection
- output surface format: NV12 for the Intel M2 path

It should not carry:

- `VAPictureParameterBufferH264`
- `VASliceParameterBufferH264`
- VA surface IDs
- parser crate native structs
- Vulkan handles

## 7. VA Surface -> DMA-BUF -> Vulkan Feasibility

The zero-copy path is feasible in principle and should be the goal for the
first decode proof:

```text
VA decode surface
    |
vaExportSurfaceHandle(..., VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2, ...)
    |
VADRMPRIMESurfaceDescriptor
    |
DMA-BUF FD(s), DRM format, modifier, planes, offsets, pitches
    |
Vulkan import
    |
QGS GPU processing
```

The libva headers state that `vaExportSurfaceHandle` exports handles owned by
the caller, and that the caller must close DRM PRIME FDs. The API does not
perform synchronization: callers must use `vaSyncSurface()` before reading the
surface externally, and must complete external writes before using the surface
again through VA.

`VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2` uses
`VADRMPRIMESurfaceDescriptor`. That descriptor includes:

- overall VA fourcc
- width and height
- object FD(s), object size, DRM format modifier
- layer/plane DRM formats
- plane object indices, offsets, and pitches

This metadata is exactly the kind of backend-private import metadata that QGS
will need to keep away from normal application semantics while still allowing
`qgs-vulkan` to import the image correctly.

Important caveat: NV12 import into Vulkan is materially more complex than the
M1 RGBA image proof. It may require DRM format modifier support,
multi-planar/external-memory image creation, and format-specific shader
sampling or conversion. If the current Vulkan path cannot import the i965
NV12 DMA-BUF directly, Step 3B should stop at proving VA decode plus DMA-BUF
export metadata, then schedule a dedicated VA-surface-to-Vulkan import step.
It should not fall back to CPU readback as the final architecture.

## 8. Intel i965 Considerations

The current Intel HD Graphics 4600/i965 VA driver reports H.264 Baseline,
Main, and High VLD decode with YUV420/NV12 output. It does not report H.264
10-bit or 4:2:2 decode capability.

For the first implementation:

- target progressive 8-bit 4:2:0 H.264
- expect NV12 surfaces
- use `/dev/dri/renderD128` via the existing QGS device-to-render-node mapping
- keep output surfaces session-owned as `VideoSurface`, not generic `Image`
- query/export the actual VA surface descriptor before assuming Vulkan import
  layout

The i965 stack is old enough that driver limitations should be expected around
modifiers, surface export flags, and synchronization. A failure to export or
import a VA decode surface should be documented precisely rather than masked by
CPU copies.

## 9. Expected Unsafe Boundaries

No new unsafe QGS-owned code is required merely to parse H.264 or create VA
H.264 parameter buffers if QGS continues using `libva 0.1.4`.

Possible future unsafe boundaries:

- if `libva 0.1.4` lacks a safe wrapper for a required decode or surface API
- if `qgs-vulkan` needs new unsafe Vulkan image import paths for NV12,
  DRM format modifiers, or multi-plane external images
- if a GStreamer or FFmpeg FFI route is chosen later

If any of those occur, they must follow `docs/safety.md`: isolate the unsafe
operation in the backend, provide a safe QGS-owned wrapper, document safety
preconditions, and keep unsafe out of `qgs-core`, `qgs-protocol`, `qgs-linux`,
`qgsd`, and normal clients.

## 10. Synthetic Test-Stream Strategy

Do not download broadcast footage. Use generated media.

Current machine state:

- `ffmpeg`/`ffprobe` are not installed
- GStreamer tools are installed
- `h264parse`, `x264enc`, `openh264enc`, and `avenc_h264` are not currently
  available

Recommended test strategy:

1. For source-controlled parser unit tests, include a tiny hand-audited Annex B
   H.264 fixture generated during development from synthetic frames, with
   license/provenance documented next to the fixture.
2. Prefer a generation script that uses an installed encoder when available,
   for example `ffmpeg -f lavfi -i testsrc2=... -c:v libx264 ...`, but do not
   make normal tests depend on `ffmpeg`.
3. Keep the checked-in fixture extremely small, for example 16x16 or 64x64,
   1 to 3 frames, 8-bit 4:2:0, Baseline or Constrained Baseline, Annex B.
4. For hardware integration tests, decode the fixture through QGS and validate
   output by checksum or by a deterministic shader/readback path.

For Step 3B, if no local encoder is available, use a small generated fixture
created on a development machine with a documented command and commit the
result only after confirming redistribution safety.

## 11. XDCAM Future Path

XDCAM HD 422 maps to technical requirements, not product labels:

- MPEG-2
- 8-bit
- 4:2:2
- often interlaced
- often MXF container

The recommended frontend split supports this cleanly. Add a
`qgs-codec-mpeg2` frontend later that parses MPEG-2 sequence/picture/slice
headers and produces QGS-owned parsed picture descriptions. The VA backend then
translates those into MPEG-2 VA buffers if hardware reports support.

MXF demuxing should remain above QGS. QGS should receive compressed elementary
packets/access units plus explicit technical metadata, not XDCAM product
objects.

Current Intel i965 did not report MPEG-2 4:2:2 decode support, so this future
path may require different hardware, software fallback above QGS, or a later
backend. The QGS model should still represent it accurately.

## 12. XAVC Future Path

XAVC-relevant low-level requirements are:

- H.264/AVC
- 10-bit
- 4:2:2
- often MXF container

The recommended H.264 frontend must preserve SPS profile, bit depth, chroma,
and slice syntax even when current hardware cannot decode that stream. That
lets QGS reject unsupported hardware decode accurately while still allowing a
future backend or application-level fallback to make decisions.

The frontend must not assume all H.264 is 8-bit 4:2:0 or progressive. It should
parse and expose High 4:2:2 profile syntax as QGS-owned parsed metadata, even
if the M2 VA backend initially rejects it because Intel i965 does not support
that output path.

MXF recognition and XAVC product semantics remain above QGS.

## 13. Recommendation For M2 Step 3B

Implement a narrow H.264 VA-API decode proof with this architecture:

```text
QGS client submits one bounded Annex B H.264 access unit or packet stream
    |
qgs-codec-h264 parses SPS/PPS/slice headers and tracks minimal DPB state
    |
QGS-owned parsed H.264 picture/slice description
    |
qgs-vaapi creates VA config/context/surfaces and VA H.264 buffers
    |
VA decodes to an NV12 VideoSurface
    |
attempt VA surface DMA-BUF export for the zero-copy path
```

Concrete Step 3B scope:

- add `qgs-codec-h264` or an equivalent internal codec frontend boundary
- use `h264-reader` first, with a quick spike test confirming Main/High slice
  header fields needed for VA are available
- keep `qgs-vaapi` responsible only for VA translation and surface lifecycle
- decode a tiny progressive 8-bit 4:2:0 H.264 stream on Intel i965
- create session-owned `VideoSurface` resources for decoded output
- probe and report `vaExportSurfaceHandle` with DRM PRIME for decoded surfaces
- do not implement MXF, FFmpeg integration, GStreamer integration, MPEG-2,
  H.264 10-bit/4:2:2, generic decode scheduling, or CPU readback as the final
  data path

Fallback decision point:

If `h264-reader` lacks required Main/High slice or DPB information after the
prototype mapping, switch to `cros-codecs` as the architectural source or
dependency rather than writing a large QGS-owned H.264 parser.

## References

- VA-API H.264 picture and slice parameter structs:
  https://intel.github.io/libva/structVAPictureParameterBufferH264.html and
  https://intel.github.io/libva/structVASliceParameterBufferH264.html
- libva DRM PRIME surface descriptor and export semantics:
  `/usr/include/va/va.h` and `/usr/include/va/va_drmcommon.h` on the current
  system, plus https://github.com/intel/libva/blob/master/va/va_drmcommon.h
- `h264-reader`: https://github.com/dholroyd/h264-reader
- GStreamer H.264 codec parser documentation:
  https://gstreamer.freedesktop.org/documentation/codecparsers/gsth264parser.html
- `cros-codecs` H.264 parser/backend model:
  https://docs.rs/cros-codecs/latest/cros_codecs/codec/h264/parser/index.html

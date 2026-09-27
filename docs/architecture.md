# QGS Architecture

QGS is an independent, vendor-neutral media and GPU system service. It is
independent from Qnc: Qnc may eventually become one client, but QGS must not
contain Qnc-specific concepts.

QGS is intended for any media application that needs a system service for
discovering and coordinating media/GPU capabilities.

QGS reports hardware capabilities. Application schedulers make workload
decisions. The service should provide accurate system information and control
interfaces without deciding how an application should partition its work.

The control plane and media data plane are separate. Normal protocol messages
are for control, discovery, negotiation, and coordination.

Large video/GPU data must not travel through normal protocol messages. Bulk
media and GPU resources need mechanisms designed for large data movement and
sharing.

Future zero-copy interoperability will use native mechanisms such as DMA-BUF
and Vulkan external memory where appropriate.

QGS must not attempt to replace Vulkan. Vulkan remains the rendering and GPU
execution API; QGS should coordinate capabilities and integration points around
such APIs.

QGS should eventually be Linux-distribution independent. Linux-specific
integration can exist in future platform layers, but the protocol model should
not require a specific distribution.

## Device Discovery Boundary

The M1 device-discovery path is:

```text
Application
    |
QGS Protocol
    |
qgsd / qgs-core
    |
DeviceDiscovery abstraction
    |
qgs-vulkan
    |
Vulkan loader/driver
    |
hardware
```

Vulkan-specific types and handles stop at `qgs-vulkan`. `qgs-core` and
`qgs-protocol` use QGS-owned device descriptions and must not expose
`VkPhysicalDevice` or other Vulkan implementation details.

## Resource Ownership Boundary

The M1 buffer-resource path is:

```text
Session
    owns
ResourceId
    represents
Resource
    +-- Buffer
    +-- Image
    +-- VideoSurface
    backed by
backend resource
```

`qgs-core` owns resource lifecycle semantics. A `ResourceId` is non-zero,
opaque, session-owned, and not persistent across daemon restarts. A resource
must not outlive the session that owns it, and a resource from one session is
not usable by another session. When a client disconnects, its session is
dropped and all resources still registered under that session are released by
Rust ownership/drop.

`qgs-vulkan` owns the Vulkan implementation objects that back buffers and
images. Vulkan buffers, images, memory objects, layouts, tiling details, and
handles do not escape `qgs-vulkan`. `qgs-protocol` owns only the protocol
representation.

M1 Step 9 supports buffers and simple single-plane RGBA images. M2 Step 1 adds
the vendor-neutral `VideoSurface` model, but does not allocate real video
surfaces or decode video yet. The maximum single buffer size is 64 MiB as a
conservative M1 safety limit, not as a final product limit.

M1 Step 6 adds Linux external-memory sharing for buffers that were created as
exportable. Exporting a resource does not transfer QGS ownership: the
`ResourceId` remains owned by its session, while the exported native FD is a
duplicated OS/Vulkan reference transferred to the client. Client-side imported
objects have their own lifetime after import according to OS and Vulkan
external-memory semantics. QGS does not add distributed reference counting.

The control/data/native-handle split is:

```text
QGS protocol payload metadata
    !=
native Linux transport attachment
    !=
shared GPU resource contents
```

Normal QGS protocol messages carry bounded metadata only. The Linux transport
uses `SCM_RIGHTS` for the native FD attachment. Buffer contents are not copied
through IPC.

Unsafe Rust remains prohibited in all normal QGS crates. A narrowly scoped,
documented unsafe boundary exists in `qgs-vulkan` only for Vulkano external
memory import where no practical safe API is available. See `docs/safety.md`.

## External Synchronization Boundary

M1 Step 7 keeps shared resource identity and execution ordering separate:

```text
ResourceId
    represents
shared memory/resource identity

SyncId
    represents
producer-to-consumer GPU execution ordering
```

A synchronization primitive is not a buffer, and a buffer is not a semaphore.
Both are owned by the session that created them. Another session cannot export
or use them, and disconnecting the client drops any still-owned resources and
sync objects.

The Step 7 proof uses a Vulkan binary external semaphore exported as a Linux
sync FD. The producer submits GPU transfer work that fills the shared buffer and
signals the semaphore. The sync FD is transferred as a Linux transport
attachment with `SCM_RIGHTS`. The consumer imports the semaphore payload,
submits GPU work that waits on it, copies from the shared buffer into a
readback buffer, and then waits on a fence only to let the CPU inspect the
consumer result. `device.wait_idle()` and `queue.wait_idle()` are not used as
the producer-to-consumer dependency mechanism.

Step 7 proves cross-process GPU ordering for a minimal transfer operation. It
does not implement compute shaders, image/video resources, a video pipeline,
external semaphore reuse protocols, or concurrent producer/consumer scheduling.

## Compute Processing Proof

M1 Step 8 proves that an exported QGS buffer can be imported into a second
Vulkan context and used by a real Vulkan compute shader while preserving the
external-memory and external-synchronization architecture from Steps 6 and 7.
The proof uses a fixed QGS-owned shader inside `qgs-vulkan` that increments
`u32` values in a shared storage buffer.

This is not a public QGS compute API. The protocol does not expose shader
modules, SPIR-V blobs, descriptor binding, pipeline creation, dispatch commands,
or scheduler policy. QGS still reports and manages resources; it does not become
a replacement for Vulkan.

## Image Processing Proof

M1 Step 9 proves that a QGS-owned Vulkan Image can be session-owned through
`ResourceId`, created as external-shareable, exported as DMA-BUF, imported into
a second Vulkan context, synchronized with the existing external sync-FD path,
and processed by a private built-in GPU shader.

The proof uses only `PixelFormat::Rgba8Unorm` and a 64 x 64 image. The
producer side initializes the image with GPU work and signals a sync FD. The
consumer side imports the image, waits on the sync FD, runs a fixed
`qgs-vulkan` image shader, and copies the result to a readback buffer only for
validation. Pixel payloads are not carried in normal QGS IPC messages.

This is not a public QGS image-processing or shader API. The protocol does not
expose image layouts, Vulkan tiling, DRM format modifiers, shader modules,
SPIR-V blobs, descriptor binding, pipeline creation, dispatch commands, color
management, video formats, or presentation/display operations.

## Video Capability And Surface Model

M2 begins video/media support by defining semantics before implementing a
decoder backend. The intended layering is:

```text
Media application
    |
container / demux layer
    |
elementary compressed stream
    |
QGS video decoder abstraction
    |
VideoSurface
    |
GPU processing
```

QGS models technical codec and surface properties: codec, codec-specific
profile, bit depth, chroma subsampling, coded and visible dimensions, scan
structure, field order, and output surface format. Product or workflow labels
such as XDCAM and XAVC are not low-level QGS API concepts. A media application
or higher container/workflow layer may interpret source metadata and choose how
to use QGS, but QGS low-level APIs do not encode Sony product semantics.

`VideoSurface` is distinct from `Image`. A video surface describes
decoder/media-oriented storage and frame semantics; it must not be assumed to
be `Rgba8Unorm`, NV12, a Vulkan image, or a generic byte buffer. M2 Step 1 does
not add timestamps, timeline/project concepts, deinterlacing, color conversion,
or real decoder allocation.

The M2 video backend abstraction is intentionally small. Backends such as
VA-API, Vulkan Video, or another vendor-neutral path translate their native
profile and surface information into QGS-owned capability entries.

## MXF Demux And Media Index Boundary

M2 Step 7 introduces a small QGS-owned MXF demux/index layer:

```text
MXF
    |
qgs-mxf
    |\
    | +-- Media Index
    | +-- Tracks / Timecode / Descriptors
    |
compressed essence access units
    |
qgs-codec-h264
    |
decoder backend
```

Container semantics and codec semantics remain separate. `qgs-mxf` owns KLV/BER
scanning, partition recognition, track/source-media modeling, edit-rate and
timecode preservation, essence location, and media-index entries. It does not
parse H.264 SPS/PPS/slice syntax. It exposes bounded compressed access-unit
bytes to `qgs-codec-h264`, which remains the codec frontend and owns H.264
classification, POC, DPB, and reference semantics.

The MXF media index is source-media structure, not an editor timeline. It uses
exact rational edit rates and source timecode metadata where available. MXF
track IDs, track numbers, QGS track identifiers, and codec stream identity are
separate concepts and must not be treated as array indices or Qnc clip/timeline
IDs.

The Step 7 parser is intentionally bounded and fixture-driven. It recognizes
the generated OP1a H.264 MXF structure, safely skips unknown KLV triplets where
permitted, derives a video index from H.264 essence KLVs, and records whether
the index is QGS-derived rather than MXF-provided. It does not implement the
complete SMPTE MXF ecosystem, persistent index caching, file-opening protocol
messages, audio decoding, video decoding, or product/workflow classification.

## H.264 Decode Frontend Boundary

M2 Step 3B adds the first narrow hardware decode proof:

```text
compressed H.264 Annex B access unit
    |
qgs-codec-h264
    |
QGS-owned parsed H.264 picture/slice description
    |
qgs-vaapi
    |
VA-API H.264 VLD
    |
VA NV12 surface
    |
QGS VideoSurface ResourceId
```

`qgs-codec-h264` owns H.264 syntax parsing and unsupported-stream detection.
Parser-library types do not escape that crate. `qgs-vaapi` owns VA display,
config, context, decode surface, VA parameter buffer translation, VA decode
submission, completion, validation-only readback, and VA surface export probing.
VA types do not escape `qgs-vaapi`.

M2 Step 5 extends the frontend to stateful H.264 Long-GOP behavior:

```text
compressed access units
    |
qgs-codec-h264
    |
SPS/PPS + POC + DPB + reference lists
    |
backend-neutral decoded-picture description
    |
qgs-vaapi
    |
VA hardware decoder
```

`qgs-codec-h264` owns picture identity, POC calculation, short-term reference
tracking, reference-list construction, DPB output ordering, IDR reset, and
flush/drain behavior. `qgs-vaapi` maps those QGS-owned picture identities to VA
surfaces and VA H.264 parameter buffers; it must not invent H.264 reference
ordering.

Decode order is not presentation order for B-frame GOPs. A successful access
unit submission may produce no display-ready surface yet, and an end-of-stream
flush may produce multiple delayed surfaces. `VideoSurface` display lifetime is
also distinct from reference lifetime: a decoded surface may remain retained by
the decoder as a future reference even after the corresponding display output
has been handed to the session resource registry.

M2 Step 5 supports a bounded subset: H.264 8-bit 4:2:0 progressive Annex B with
IDR/I, P, and B pictures, POC type 0 for the Long-GOP proof, short-term
references, sliding-window reference marking, short-term unused-for-reference
MMCO, and short-term reference-list modifications. It does not claim general
H.264 support, MPEG-2 support, XDCAM, XAVC, 10-bit, 4:2:2, interlaced decode,
Vulkan import of decoded NV12 surfaces, or video scheduling.

M2 Step 6 establishes the professional H.264 representation boundary. A valid
codec stream is not the same thing as a hardware-decodable stream:

```text
H.264 access unit
    |
qgs-codec-h264 parses/classifies profile + bit depth + chroma + GOP syntax
    |
decoder capability matcher
    |
supported by backend?
   / \
 yes no
  |   |
hardware decode     future fallback / clean unsupported result
```

For example, QGS can parse and classify valid H.264 High 4:2:2 10-bit Intra
and Long-GOP syntax while the current Intel i965 VA backend cleanly rejects
that configuration because it advertises only H.264 8-bit 4:2:0 decode. This
distinction keeps malformed compressed data, unsupported parser features, and
valid-but-unsupported backend configurations separate.

Compressed access units may travel through bounded normal QGS IPC for this M2
proof. Raw decoded frames remain backend-owned VideoSurface resources and do
not travel through normal protocol messages.

M2 Step 2 adds real VA-API decode capability discovery through a backend
boundary:

```text
QGS video capability model
    ^
    |
qgs-vaapi
    |
VA-API
    |
DRM render node
    |
hardware
```

VA-API types, profiles, entrypoints, display handles, and driver details stop
at `qgs-vaapi`. `qgs-core`, `qgs-protocol`, and ordinary clients see only
QGS-owned `VideoDecodeCapability` entries. `qgs-vaapi` binds a QGS device to a
Linux render node using sysfs PCI vendor/device identity; it does not rely on
enumeration order. A known device with no usable VA decode backend reports an
empty video capability list rather than inferred or fabricated decode support.

## Current Scope

The initial workspace contains an explicit v0.1 protocol wire encoding, minimal
session management, Linux Unix Domain Socket transport, and Vulkan-backed
device enumeration and static capability discovery through the `DeviceDiscovery`
abstraction. It reports compute queue limits, memory heap/type summaries, and
external-memory/synchronization mechanism availability. It can create and
destroy session-owned Vulkan-backed buffer and RGBA image resources and can
export/import external-memory FDs for explicitly exportable resources on
supported drivers. It can also prove one-shot external GPU synchronization with
Linux sync FDs on supported drivers. It can run private built-in compute and
image-processing proofs against imported shared resources. It also defines the
video capability and `VideoSurface` model and can perform narrow H.264
VA-API hardware decode proofs on supported hardware. It does not include an
async runtime, daemonization, DRM/KMS, video encode, a public compute or shader
API, reusable semaphore workflows, workload scheduling, performance
benchmarking, telemetry, free-memory reporting, or resumed VA -> Vulkan
zero-copy video processing.

Capability discovery is static information reported by the backend. It is not a
measurement of current load, available/free VRAM, throughput, or scheduling
suitability. Interop mechanism availability means that the backend exposes an
API mechanism; it does not guarantee that every resource, image format, or usage
can be exported or imported with that mechanism.

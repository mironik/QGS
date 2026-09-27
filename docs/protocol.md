# QGS Protocol

QGS v0.1 has an explicit little-endian wire representation. Rust in-memory
types are not the wire ABI and are never serialized by copying struct memory.

## Header

Every v0.1 message starts with a fixed 24-byte header:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u32 | magic, `0x51475300` |
| 4 | u16 | protocol major |
| 6 | u16 | protocol minor |
| 8 | u8 | message kind: request `1`, response `2`, event `3` |
| 9 | u8 | opcode |
| 10 | u16 | flags, currently `0` |
| 12 | u32 | payload length in bytes |
| 16 | u64 | request id |

All integer fields are little-endian. The maximum payload size is 4 MiB plus a
small framing margin for M2 Step 5 compressed access-unit transport. Decoders
must reject oversized payload lengths before allocating payload storage.

`request_id` correlates a response with a request. `request_id` 0 is reserved
for messages that do not correlate to a request. Normal request/response
messages preserve the original non-zero request id.

`SessionId` is not part of the transport header. It appears only in payloads
that need it, such as WELCOME.

## Message Types

HELLO is request kind `1`, opcode `1`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u16 | minimum protocol major |
| 2 | u16 | minimum protocol minor |
| 4 | u16 | maximum protocol major |
| 6 | u16 | maximum protocol minor |

ENUMERATE_DEVICES is request kind `1`, opcode `2`. It has no payload. It is a
session operation and must be sent after HELLO/WELCOME.

QUERY_DEVICE_CAPABILITIES is request kind `1`, opcode `3`. It is a session
operation and must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` from the current `qgsd` process/session |

CREATE_BUFFER is request kind `1`, opcode `4`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is a fixed-size `BufferDesc`:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` from the current `qgsd` process/session |
| 8 | u64 | buffer size in bytes |
| 16 | u32 | buffer usage flags |
| 20 | u8 | memory preference flags |
| 21 | u8 | external sharing selector |
| 22 | 2 bytes | reserved, currently `0` |

Buffer usage flag values are:

| Bit | Usage |
| ---: | --- |
| 0 | TRANSFER_SRC |
| 1 | TRANSFER_DST |
| 2 | STORAGE |

At least one usage flag must be set. Unknown usage bits are rejected.

Memory preference flags are:

| Bit | Meaning |
| ---: | --- |
| 0 | device-local memory preferred |
| 1 | host-visible memory required |
| 2 | host-coherent memory preferred |

The memory model is preference/requirement based. It does not assume that GPU
memory is split into simple RAM vs VRAM categories; integrated GPUs may select
memory that is both device-local and host-visible.

External sharing selector values are:

| Value | Meaning |
| ---: | --- |
| 0 | no external sharing |
| 1 | external sharing required with DMA-BUF FD |
| 2 | external sharing required with opaque Vulkan external-memory FD |

External sharing is a creation-time requirement. A resource not created with
external sharing cannot later be retroactively exported.

DESTROY_RESOURCE is request kind `1`, opcode `5`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` owned by the current session |

EXPORT_RESOURCE is request kind `1`, opcode `6`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` owned by the current session |
| 8 | u8 | requested external handle type |
| 9 | 7 bytes | reserved, currently `0` |

External handle type values are:

| Value | Type |
| ---: | --- |
| 1 | DMA-BUF FD |
| 2 | opaque Vulkan external-memory FD |

CREATE_SYNC is request kind `1`, opcode `7`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` from the current `qgsd` process/session |
| 8 | u8 | sync kind |
| 9 | u8 | requested sync export handle type |
| 10 | 6 bytes | reserved, currently `0` |

Sync kind values are:

| Value | Kind |
| ---: | --- |
| 1 | binary semaphore |

Sync export handle type values are:

| Value | Type |
| ---: | --- |
| 1 | Linux sync FD |

EXPORT_SYNC is request kind `1`, opcode `8`. It is a session operation and
must be sent after HELLO/WELCOME. For M1 Step 7, exporting a sync object submits
a minimal producer GPU operation against a session-owned resource and signals
the exported synchronization primitive from that GPU submission. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `SyncId` owned by the current session |
| 8 | u64 | QGS `ResourceId` owned by the current session |
| 16 | u32 | producer fill pattern for the Step 7 validation operation |
| 20 | 4 bytes | reserved, currently `0` |

CREATE_IMAGE is request kind `1`, opcode `9`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is a fixed-size `ImageDesc`:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` from the current `qgsd` process/session |
| 8 | u32 | image width in pixels |
| 12 | u32 | image height in pixels |
| 16 | u8 | pixel format |
| 17 | u8 | external sharing selector |
| 18 | 2 bytes | reserved, currently `0` |
| 20 | u32 | image usage flags |

Pixel format values are:

| Value | Format |
| ---: | --- |
| 1 | Rgba8Unorm |

Image usage flag values are:

| Bit | Usage |
| ---: | --- |
| 0 | TRANSFER_SRC |
| 1 | TRANSFER_DST |
| 2 | STORAGE |

M1 Step 9 supports only single-plane `Rgba8Unorm` images. The maximum image
dimensions are 8192 x 8192. Width and height must be non-zero, usage must be
non-empty, and unknown usage bits or pixel formats are rejected. Vulkan image
tiling and layout transitions are backend implementation details and are not
exposed in the QGS protocol.

QUERY_VIDEO_CAPABILITIES is request kind `1`, opcode `10`. It is a session
operation and must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` from the current `qgsd` process/session |

The response is backend-neutral. Real decode capabilities may be supplied by a
backend such as VA-API after it translates native profiles, entrypoints, and
surface formats into QGS-owned values. A known device with no proven decode
backend or no supported decode entrypoints returns a valid empty capability
list rather than inferred support.

CREATE_DECODER is request kind `1`, opcode `11`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` |
| 8 | u8 | `VideoCodec` |
| 9 | u8 | codec-specific profile |
| 10 | u8 | bit depth |
| 11 | u8 | chroma subsampling |
| 12 | u32 | coded width |
| 16 | u32 | coded height |
| 20 | u8 | scan mode |
| 21 | 7 bytes | reserved, currently `0` |

SUBMIT_ACCESS_UNIT is request kind `1`, opcode `12`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` |
| 8 | u32 | compressed access-unit byte length |
| 12 | variable | compressed access-unit bytes |

The maximum compressed access-unit payload is 4 MiB. M2 Step 5 permits bounded
compressed H.264 access units through normal QGS IPC as a temporary development
transport. Raw decoded video pixels must not travel through normal protocol
messages. Future high-throughput ingest may use a dedicated bulk-data plane.

FLUSH_DECODER is request kind `1`, opcode `13`. It drains presentation-ready
frames that remain pending because of codec reordering. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` owned by the current session |

DESTROY_DECODER is request kind `1`, opcode `14`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` owned by the current session |

WELCOME is response kind `2`, opcode `1`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u16 | selected protocol major |
| 2 | u16 | selected protocol minor |
| 4 | u64 | session id |

ERROR is response kind `2`, opcode `2`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u32 | stable numeric error code |

Clients must use numeric error codes for behavior. Human-readable diagnostics
are for logging only and are not part of v0.1 ERROR payloads.

DEVICE_LIST is response kind `2`, opcode `3`. Its payload starts with a device
count and then one entry per device:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u16 | device count |

Each device entry is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` |
| 8 | u8 | `DeviceClass` |
| 9 | u8 | backend API |
| 10 | u16 | UTF-8 device name length in bytes |
| 12 | u32 | vendor id |
| 16 | u32 | device id |
| 20 | u16 | API major version |
| 22 | u16 | API minor version |
| 24 | u16 | API patch version |
| 26 | u16 | reserved, currently `0` |
| 28 | u32 | backend driver version |
| 32 | variable | UTF-8 device name bytes |

`DeviceClass` numeric values are:

| Value | Class |
| ---: | --- |
| 1 | IntegratedGpu |
| 2 | DiscreteGpu |
| 3 | Software |
| 4 | Other |

Backend API value `1` means Vulkan.

M1 Step 3 limits a DEVICE_LIST response to 16 devices. Each device name is
limited to 128 UTF-8 bytes. `DeviceId` is a non-zero QGS-owned identifier that
is stable only within the current `qgsd` process/session for M1; clients
must not treat it as globally persistent or stable across boots.

DEVICE_CAPABILITIES is response kind `2`, opcode `4`. It reports static
capability information for one `DeviceId`.

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` |
| 8 | u8 | compute supported, boolean |
| 9 | 3 bytes | reserved, currently `0` |
| 12 | 3 x u32 | maximum compute workgroup count |
| 24 | 3 x u32 | maximum compute workgroup size |
| 36 | u32 | maximum compute workgroup invocations |
| 40 | u16 | memory heap count |
| 42 | u16 | memory type count summary |
| 44 | u8 | host-visible memory exists, boolean |
| 45 | u8 | host-coherent memory exists, boolean |
| 46 | u8 | device-local memory exists, boolean |
| 47 | u8 | reserved, currently `0` |
| 48 | u8 | external memory FD mechanism available, boolean |
| 49 | u8 | DMA-BUF external memory mechanism available, boolean |
| 50 | u8 | external semaphore FD mechanism available, boolean |
| 51 | u8 | external fence FD mechanism available, boolean |

Each memory heap entry is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | heap size in bytes |
| 8 | u8 | heap is device-local, boolean |
| 9 | 7 bytes | reserved, currently `0` |

M1 Step 4 limits DEVICE_CAPABILITIES to 16 memory heaps and 32 memory types.
The memory type count is a summary only; individual memory type flags are not
transmitted in this milestone.

Capability discovery is static backend-reported information. It does not report
telemetry, current load, free memory, free VRAM, throughput, or scheduling
decisions. Interop mechanism fields report that the backend exposes the
mechanism. They do not guarantee that every resource, image format, or usage is
exportable/importable with that mechanism.

BUFFER_CREATED is response kind `2`, opcode `5`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` |
| 8 | u64 | buffer size in bytes |
| 16 | u8 | selected memory property flags |
| 17 | 7 bytes | reserved, currently `0` |

Selected memory property flags are:

| Bit | Meaning |
| ---: | --- |
| 0 | selected memory is device-local |
| 1 | selected memory is host-visible |
| 2 | selected memory is host-coherent |

RESOURCE_DESTROYED is response kind `2`, opcode `6`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | destroyed QGS `ResourceId` |

RESOURCE_EXPORTED is response kind `2`, opcode `7`. Its payload contains only
bounded metadata. The native FD is not encoded as an integer in this payload;
it is carried as one Linux transport attachment.

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | exported QGS `ResourceId` |
| 8 | u64 | QGS `DeviceId` |
| 16 | u8 | resource kind |
| 17 | u8 | external handle type |
| 18 | u8 | selected memory property flags |
| 19 | u8 | dedicated allocation, boolean |
| 20 | u8 | expected attachment count, currently `1` |
| 21 | u8 | pixel format, or `0` for non-image resources |
| 22 | 2 bytes | reserved, currently `0` |
| 24 | u64 | backend allocation size in bytes |
| 32 | u32 | backend memory type index |
| 36 | u32 | buffer usage flags, or `0` for non-buffer resources |
| 40 | u32 | image usage flags, or `0` for non-image resources |
| 44 | u32 | image width, or `0` for non-image resources |
| 48 | u32 | image height, or `0` for non-image resources |
| 52 | u64 | logical resource size in bytes |
| 60 | 4 bytes | reserved, currently `0` |
| 64 | u64 | backend image layout token, or `0` when absent |

Resource kind values are:

| Value | Kind |
| ---: | --- |
| 1 | Buffer |
| 2 | Image |
| 3 | VideoSurface |

The backend memory type index and backend image layout token are backend import
metadata. Normal clients treat them as opaque. They are interpreted only by the
matching `qgs-vulkan` import helper for the same backend/device identity.

SYNC_CREATED is response kind `2`, opcode `8`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `SyncId` |

SYNC_EXPORTED is response kind `2`, opcode `9`. Its payload contains only
bounded metadata. The native sync FD is not encoded as an integer in this
payload; it is carried as one Linux transport attachment.

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | exported QGS `SyncId` |
| 8 | u8 | sync export handle type |
| 9 | u8 | expected attachment count, currently `1` |
| 10 | 2 bytes | reserved, currently `0` |
| 12 | u32 | producer fill pattern used by the Step 7 validation operation |

IMAGE_CREATED is response kind `2`, opcode `10`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` |
| 8 | u32 | image width in pixels |
| 12 | u32 | image height in pixels |
| 16 | u8 | pixel format |
| 17 | u8 | selected memory property flags |
| 18 | 6 bytes | reserved, currently `0` |

VIDEO_CAPABILITIES is response kind `2`, opcode `11`. Its payload starts with:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DeviceId` |
| 8 | u16 | decode capability count |
| 10 | 2 bytes | reserved, currently `0` |

Each decode capability entry is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u8 | video codec |
| 1 | u8 | codec-specific profile value |
| 2 | u8 | bit depth |
| 3 | u8 | chroma subsampling |
| 4 | u32 | maximum coded width |
| 8 | u32 | maximum coded height |
| 12 | u8 | progressive supported, boolean |
| 13 | u8 | interlaced supported, boolean |
| 14 | u8 | output surface format count |
| 15 | u8 | reserved, currently `0` |
| 16 | variable | one u8 per output surface format |

Video codec values are:

| Value | Codec |
| ---: | --- |
| 1 | H264 |
| 2 | Mpeg2 |

H.264 profile values are:

| Value | Profile |
| ---: | --- |
| 1 | Baseline |
| 2 | Main |
| 3 | High |
| 4 | High10 |
| 5 | High10Intra |
| 6 | High422 |
| 7 | High422Intra |

MPEG-2 profile values are:

| Value | Profile |
| ---: | --- |
| 1 | Main |
| 2 | Profile422 |

Profile values are codec-specific. Receivers must reject a profile value that
is not defined for the entry's codec.

Valid bit depths are `8`, `10`, and `12`.

Chroma subsampling values are:

| Value | Chroma |
| ---: | --- |
| 1 | 4:2:0 |
| 2 | 4:2:2 |
| 3 | 4:4:4 |

Video surface format values are:

| Value | Format |
| ---: | --- |
| 1 | Nv12 |
| 2 | P010 |
| 3 | Yuv422_8 |
| 4 | Yuv422_10 |

The QGS-owned `VideoSurfaceDesc` model contains coded width and height, a
visible region, surface format, explicit bit depth, chroma subsampling, scan
mode, and field order. Scan mode values are progressive `1` and interlaced
`2`. Field order values are unknown `0`, top-field-first `1`, and
bottom-field-first `2`. The semantic surface format is separate from the
backend storage representation; `Yuv422_10` represents a 10-bit 4:2:2 decoded
surface without committing the public QGS model to one VA fourcc, Vulkan
format, packing, or plane layout.

M2 limits VIDEO_CAPABILITIES to 32 decode entries and 8 output surface formats
per entry. VideoSurface coded dimensions are limited to 8192 x 8192, must be
non-zero, and visible regions must be non-empty and contained inside the coded
dimensions. If a real backend reports profile/surface support but does not
expose exact maximum coded dimensions through the queried capability interface,
QGS reports its bounded M2 model ceiling in the max-width/max-height fields and
documents that backend limitation in the milestone report.

DECODER_CREATED is response kind `2`, opcode `12`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` |

DECODE_OUTPUT is response kind `2`, opcode `13`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` |
| 8 | u16 | output surface count |

Each output surface entry is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` for the decoded `VideoSurface` |
| 8 | u32 | coded width |
| 12 | u32 | coded height |
| 16 | u32 | visible x |
| 20 | u32 | visible y |
| 24 | u32 | visible width |
| 28 | u32 | visible height |
| 32 | u8 | `VideoSurfaceFormat` |
| 33 | u8 | bit depth |
| 34 | u8 | chroma subsampling |
| 35 | u8 | scan mode |
| 36 | u8 | field order |
| 37 | 3 bytes | reserved, currently `0` |

The output count is bounded to 16. A successful `SUBMIT_ACCESS_UNIT` may return
zero outputs when the decoded picture is still waiting for reference pictures
or presentation reordering. A flush may return multiple output-ready surfaces.

DECODER_DESTROYED is response kind `2`, opcode `14`. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `DecoderId` |

`DecoderId` is non-zero, opaque, session-owned, and not persistent across daemon
restarts. Decoder state is released by `DESTROY_DECODER` or by session
disconnect. Decoded output is owned as a normal session resource with
`ResourceKind::VideoSurface`.

`SyncId` is a non-zero, opaque, QGS-owned identifier. It represents an ordering
primitive, not resource memory. For M1 Step 7, a `SyncId` is unique within its
owning session/lifetime and is not persistent across sessions or daemon
restarts. Another session cannot export or use it.

M1 Step 7 uses a Vulkan binary external semaphore exported as a Linux sync FD.
Linux sync FDs have copy-transference semantics: each exported FD represents one
signal/wait cycle. The importer uses a temporary semaphore payload, and the wait
consumes that payload. This model is intentionally one-shot for M1.

`ResourceId` is a non-zero, opaque, QGS-owned identifier. It does not expose
pointers, Vulkan handles, file descriptors, or any native resource handle.
For M1 Step 5, a `ResourceId` is unique within its owning session/lifetime and
is not persistent across sessions or daemon restarts. A resource belongs to
exactly one session. Another session cannot use or destroy it.

M1 Step 9 supports allocated `Buffer` and `Image` resources. M2 Step 1 adds
`VideoSurface` as a protocol/model resource kind, but real VideoSurface
allocation and export are not implemented yet. The maximum single buffer size
is 64 MiB. This is an M1 safety limit to prevent unbounded allocation requests;
it is not a final product limit. Image dimensions are limited as documented
above.

## Validation

Receivers must reject invalid magic, unknown message kinds, unknown opcodes,
nonzero flags, oversized payload lengths, malformed or truncated headers,
malformed or truncated payloads, unexpected trailing payload bytes, invalid
protocol versions, invalid protocol ranges, zero `SessionId` values, zero
`DeviceId` values, excessive device counts, excessive device-name lengths, and
invalid UTF-8 device names. DEVICE_CAPABILITIES receivers must also reject
excessive memory heap counts, excessive memory type counts, malformed boolean
or reserved fields, truncated heap entries, and trailing payload bytes.
CREATE_BUFFER receivers must reject zero or oversized buffer sizes, unknown
usage bits, empty usage flags, malformed memory preference flags, nonzero
reserved bytes, truncated payloads, and trailing payload bytes. DESTROY_RESOURCE
receivers must reject zero `ResourceId` values. Unknown resources and
cross-session resource attempts return a stable UnknownResource error.
CREATE_SYNC receivers must reject zero `DeviceId` values, unknown sync kinds,
unknown sync handle types, nonzero reserved bytes, truncated payloads, and
trailing payload bytes. EXPORT_SYNC receivers must reject zero `SyncId` values,
zero `ResourceId` values, nonzero reserved bytes, truncated payloads, and
trailing payload bytes. Unknown sync IDs and cross-session sync attempts return
a stable UnknownSync error. CREATE_IMAGE receivers must reject zero dimensions,
oversized dimensions, unsupported pixel formats, invalid usage flags, nonzero
reserved bytes, truncated payloads, and trailing payload bytes. Exported image
metadata must be internally consistent with the resource kind, dimensions,
format, logical byte size, and attachment count.
QUERY_VIDEO_CAPABILITIES receivers must reject zero `DeviceId` values.
VIDEO_CAPABILITIES receivers must reject excessive capability counts, excessive
output format counts, unknown video codecs, unknown codec-specific profiles,
invalid bit depths, unknown chroma values, unknown output surface formats,
invalid dimensions, malformed boolean or reserved fields, truncated entries,
and trailing payload bytes. VideoSurface model validation rejects zero or
oversized coded dimensions and visible regions outside the coded frame.
CREATE_DECODER receivers must reject zero `DeviceId` values, unknown codecs,
unknown codec-specific profiles, invalid bit depths, unsupported chroma values,
unsupported scan modes, invalid coded dimensions, nonzero reserved bytes,
truncated payloads, and trailing payload bytes. SUBMIT_ACCESS_UNIT receivers
must reject zero `DecoderId` values, compressed payloads larger than 4 MiB,
truncated compressed payloads, and trailing payload bytes. Unsupported
H.264 syntax returns a stable UnsupportedH264StreamFeature error rather than
being silently interpreted as the current M2 subset. A valid stream whose
technical requirements do not match backend capabilities returns
UnsupportedDecodeConfiguration rather than MalformedCompressedData.
FLUSH_DECODER and DESTROY_DECODER receivers must reject zero `DecoderId`
values. Unknown decoder IDs and cross-session decoder attempts return a stable
UnknownDecoder error.

Major protocol versions are incompatible. Minor versions are backward-compatible
only within the same major version. A server selects the highest protocol
version it supports that is inside the client's supported range. The
implementation through M2 Step 3B supports only server version `0.1`.

## Transport

The protocol is transport-independent. `qgs-protocol` contains concepts and
wire encoding/decoding, but no Unix-specific code.

For Linux M1, `qgsd` and `qgs-test` communicate over a Unix Domain Socket using
synchronous blocking I/O. This is the first real IPC transport, but it does not
change the protocol's transport independence.

M1 Step 6 adds Linux transport attachments for external-memory FDs. M1 Step 7
reuses the same attachment model for Linux sync FDs. The Linux transport uses
Unix Domain Socket ancillary data (`SCM_RIGHTS`) and supports at most one
attached FD per message. Missing or excessive attachments are rejected by the
attachment receive path. FD ownership is represented with owned file descriptor
types; descriptors are closed by RAII when dropped.

These are three separate things:

- QGS protocol payload metadata
- native Linux transport attachments
- shared GPU resource contents

Large video/GPU data and buffer contents are not carried in normal QGS IPC
messages. Step 6 transfers only a native reference to shared external memory.
Step 7 transfers only a native synchronization handle and bounded metadata.

External memory sharing and external synchronization are separate. The Step 7
sync FD proves producer-to-consumer GPU ordering for one minimal transfer/fill
validation path. It does not implement a reusable per-frame synchronization
protocol, external semaphore pooling, image/video resource synchronization, or a
compute/video pipeline.

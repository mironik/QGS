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

All integer fields are little-endian. The maximum payload size is 4096 bytes.
Decoders must reject oversized payload lengths before allocating payload
storage.

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
| 21 | 3 bytes | reserved, currently `0` |

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

DESTROY_RESOURCE is request kind `1`, opcode `5`. It is a session operation and
must be sent after HELLO/WELCOME. Its payload is:

| Offset | Width | Field |
| --- | ---: | --- |
| 0 | u64 | QGS `ResourceId` owned by the current session |

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

`ResourceId` is a non-zero, opaque, QGS-owned identifier. It does not expose
pointers, Vulkan handles, file descriptors, or any native resource handle.
For M1 Step 5, a `ResourceId` is unique within its owning session/lifetime and
is not persistent across sessions or daemon restarts. A resource belongs to
exactly one session. Another session cannot use or destroy it.

M1 Step 5 supports only `Buffer` resources. The maximum single buffer size is
64 MiB. This is an M1 safety limit to prevent unbounded allocation requests; it
is not a final product limit.

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

Major protocol versions are incompatible. Minor versions are backward-compatible
only within the same major version. A server selects the highest protocol
version it supports that is inside the client's supported range. The
implementation through M1 Step 5 supports only server version `0.1`.

## Transport

The protocol is transport-independent. `qgs-protocol` contains concepts and
wire encoding/decoding, but no Unix-specific code.

For Linux M1, `qgsd` and `qgs-test` communicate over a Unix Domain Socket using
synchronous blocking I/O. This is the first real IPC transport, but it does not
change the protocol's transport independence.

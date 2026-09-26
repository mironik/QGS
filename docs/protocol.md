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
is stable only within the current `qgsd` process/session for Step 3; clients
must not treat it as globally persistent or stable across boots.

## Validation

Receivers must reject invalid magic, unknown message kinds, unknown opcodes,
nonzero flags, oversized payload lengths, malformed or truncated headers,
malformed or truncated payloads, unexpected trailing payload bytes, invalid
protocol versions, invalid protocol ranges, zero `SessionId` values, zero
`DeviceId` values, excessive device counts, excessive device-name lengths, and
invalid UTF-8 device names.

Major protocol versions are incompatible. Minor versions are backward-compatible
only within the same major version. A server selects the highest protocol
version it supports that is inside the client's supported range. M1 Step 2
supports only server version `0.1`.

## Transport

The protocol is transport-independent. `qgs-protocol` contains concepts and
wire encoding/decoding, but no Unix-specific code.

For Linux M1, `qgsd` and `qgs-test` communicate over a Unix Domain Socket using
synchronous blocking I/O. This is the first real IPC transport, but it does not
change the protocol's transport independence.

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
are for logging only and are not part of M1 Step 2 payloads.

## Validation

Receivers must reject invalid magic, unknown message kinds, unknown opcodes,
nonzero flags, oversized payload lengths, malformed or truncated headers,
malformed or truncated payloads, unexpected trailing payload bytes, invalid
protocol versions, invalid protocol ranges, and zero `SessionId` values.

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

# QGS Protocol

The QGS protocol starts with a HELLO/WELCOME exchange.

## HELLO

A client sends HELLO with the minimum and maximum QGS protocol versions it can
support. The current Rust model represents this as `HelloRequest`.

## WELCOME

The service selects a compatible protocol version and replies with WELCOME. The
current Rust model represents this as `WelcomeResponse`, containing the selected
protocol version and a typed `SessionId`.

If the requested version range is invalid or incompatible, negotiation fails
with a protocol error instead of WELCOME.

## Wire Format Boundary

The current Rust in-memory types are not the final wire format. They are a
temporary protocol model used to keep early crate boundaries clear.

A future wire format must be specified explicitly. It must define framing,
integer encoding, byte order, validation rules, compatibility behavior, and
transport error handling. QGS must not use serde or bincode as its wire
protocol.

## Temporary Demonstration

The current daemon and test client demonstrate HELLO/WELCOME in-process. This
is temporary and is not the final transport. No async runtime, Unix socket, or
other IPC transport is part of this initial skeleton.


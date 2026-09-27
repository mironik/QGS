# QGS M1 Step 6 Report

## Files And Crates Changed

- `qgs-protocol`: external-sharing fields, export request/response metadata,
  stable errors, and protocol tests.
- `qgs-core`: session-owned resource export authorization and mock ownership
  tests.
- `qgs-linux`: bounded SCM_RIGHTS attachment transfer using owned FDs.
- `qgs-vulkan`: export-capable buffer allocation, external-memory export,
  audited import boundary, and shared-allocation validation helper.
- `qgsd`: `EXPORT_RESOURCE` handling and FD attachment response path.
- `qgs-test`: external-memory hardware demonstration and client-side import.
- `docs/architecture.md`, `docs/protocol.md`, `docs/safety.md`, this report.
- `Cargo.lock`, `crates/qgs-linux/Cargo.toml`, `tools/qgs-test/Cargo.toml`.

## External-Sharing Resource Model

Buffers must be created exportable from the beginning. `BufferDesc` now carries
`ExternalSharing`:

- `None`
- `Required { handle_type }`

Exporting a resource does not transfer QGS ownership. The server-side
`ResourceId` remains session-owned; the FD transferred to the client is a native
reference suitable for import by the client.

## Protocol Additions

New request opcode:

- `EXPORT_RESOURCE`: request kind `1`, opcode `6`

New response opcode:

- `RESOURCE_EXPORTED`: response kind `2`, opcode `7`

New stable errors:

- `ResourceNotExportable`: `28`
- `UnsupportedExternalHandleType`: `29`
- `ExportFailed`: `30`

`CREATE_BUFFER` offset `21` now encodes external sharing:

- `0`: none
- `1`: DMA-BUF FD required
- `2`: opaque external-memory FD required

`RESOURCE_EXPORTED` carries bounded metadata only. The native FD is transferred
as a Linux transport attachment, not encoded as a payload integer.

## Transport Attachment Model

`qgs-linux` uses Unix Domain Socket ancillary data (`SCM_RIGHTS`) for Step 6.
The maximum attachment count is `1`. The API uses `OwnedFd`/`AsFd`; received
FDs close automatically on drop.

Unit tests prove a real FD can be transferred over a temporary Unix socket and
that excessive outgoing attachments are rejected.

## SCM_RIGHTS Dependency

Dependency added:

- `unix-ancillary 0.6.0`

It was selected because it provides a narrow safe Rust API for SCM_RIGHTS using
`OwnedFd` and `BorrowedFd`, with no raw FD public API required by QGS transport
code.

## Vulkan External Handle Type

The successful hardware tests used:

- DMA-BUF FD

Opaque Vulkan external-memory FD support is implemented as a protocol/backend
option, but the current acceptance run did not need it because DMA-BUF worked
on both physical GPUs.

## Device Matching

The qgs-test import helper creates a second Vulkan context and matches the
exporting physical device using QGS-visible backend identity:

- backend API
- vendor id
- device id
- device name
- API version
- driver version

This does not create a persistent device identity guarantee. It is an M1
process/runtime matching method and does not rely on Vulkan enumeration order.

## FD Ownership And Lifetime

`qgsd` owns the QGS resource and original Vulkan allocation until
`DESTROY_RESOURCE` or session drop. Exporting duplicates/transfers a native FD
reference to the client. The client owns the received FD/imported object after
receipt. QGS does not implement distributed reference counting in Step 6.

FDs are not exposed as protocol integers. They are held by RAII owned file
descriptor types and close on drop.

## Shared-Memory Validation

The server writes a small deterministic marker into the host-visible exported
buffer before export. The client receives the FD, imports it into a second
Vulkan context for the matching device, maps the imported memory, and verifies
the marker from the shared allocation.

No buffer contents are copied through the QGS protocol or Unix socket payload.

This validation is deliberately quiescent. External semaphore/fence
synchronization for concurrent GPU workloads is deferred.

## Unsafe Boundary Audit

Exact unsafe API called:

- `vulkano::memory::DeviceMemory::import`

Why unsafe is required:

Vulkano 0.35.2 exposes external memory export as safe, but external memory
import from FD is `unsafe`. No higher-level safe Vulkano buffer import path was
found that satisfies Step 6.

Vulkano safety preconditions include:

- the FD must be valid
- Vulkan takes ownership of the FD after import
- once imported, QGS must not operate on that FD or its duplicates
- for opaque Vulkan FDs, allocation size and memory type index must match the
  original allocation
- dedicated-allocation state must be reproduced when applicable
- imported memory must be used with a compatible device and resource

Validations before the call:

- handle type matches the QGS metadata/request
- device extensions for external memory FD and DMA-BUF are enabled as needed
- resource-specific external buffer properties are exportable/importable
- importer device matches QGS-visible exporter identity
- buffer size, usage, and external handle type match metadata
- allocation size is at least the buffer size
- exporter memory type index and allocation size are supplied by qgs-vulkan
- dedicated-allocation state is mirrored when reported
- the owned FD is moved into the import call and not reused by QGS code

Unsafe source file:

- `crates/qgs-vulkan/src/external_memory.rs`

Unsafe blocks in QGS-owned code after Step 6:

- `1`

All other QGS crates remain `#![forbid(unsafe_code)]`. `qgs-vulkan` uses
`#![deny(unsafe_code)]` crate-wide and `#[allow(unsafe_code)]` only for the
private external-memory module.

Remaining assumptions:

- Vulkano and the Vulkan driver uphold the documented FD ownership behavior.
- Backend memory type index metadata is interpreted only by the matching
  qgs-vulkan backend/device.
- M1 host-visible validation does not solve external GPU synchronization.

## Test Results

`cargo test --workspace` passed.

Total: 83 tests passed.

- `qgs-core`: 14 passed
- `qgs-linux`: 3 passed
- `qgs-protocol`: 64 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Intel HD 4600 Result

Device:

- `Intel(R) HD Graphics 4600 (HSW GT2)`
- class: `IntegratedGpu`
- vendor/device: `0x8086 / 0x0416`

Step 6 result:

- created 1 MiB exportable buffer
- selected memory: device-local, host-visible, host-coherent
- exported DMA-BUF FD
- received FD through SCM_RIGHTS
- imported into a second Vulkan context
- shared allocation validation succeeded
- resource destroyed successfully

Mesa also printed the existing Haswell Vulkan support warning.

## GTX 950M Result

Device:

- `NVIDIA GeForce GTX 950M (NVK GM107)`
- class: `DiscreteGpu`
- vendor/device: `0x10de / 0x139a`

Step 6 result:

- created 1 MiB exportable buffer
- selected memory: device-local, host-visible, host-coherent
- exported DMA-BUF FD
- received FD through SCM_RIGHTS
- imported into a second Vulkan context
- shared allocation validation succeeded
- resource destroyed successfully

## Disconnect And Cleanup Result

The successful qgs-test run left three transient buffers alive and disconnected.
`qgsd` logged:

```text
client disconnected; releasing 3 resource(s) for session 2
qgs-core: releasing 3 resource(s) owned by session
```

An earlier failed validation attempt also demonstrated disconnect cleanup of
one still-owned resource:

```text
client disconnected; releasing 1 resource(s) for session 1
qgs-core: releasing 1 resource(s) owned by session
```

## IPC Payload Contents

No actual buffer contents crossed normal QGS IPC. The protocol carried only
bounded metadata. The native FD crossed via SCM_RIGHTS. Shared allocation
validation read from the imported shared memory mapping.

## Commit And Push Verification

This report is included in the M1 Step 6 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Architectural Concerns

No blocking architectural concerns remain.

- `qgs-protocol` has no Vulkan or Linux FD dependency.
- `qgs-core` has no Vulkan dependency.
- `qgs-linux` knows only transport FD attachment semantics, not Vulkan.
- Vulkan-specific types remain inside `qgs-vulkan`.
- Unsafe Rust is isolated to the audited qgs-vulkan import boundary.
- No Qnc concepts were added.
- No image resources, video surfaces, compute execution, shaders, video APIs,
  DRM/KMS, scheduling, telemetry, or external semaphore workflow were added.
- External memory sharing does not claim to solve synchronization.
- Device matching is runtime/backend identity matching, not persistent identity.

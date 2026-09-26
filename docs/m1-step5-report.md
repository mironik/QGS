# QGS M1 Step 5 Report

## Resource Model

M1 Step 5 adds session-owned GPU buffer resources.

```text
Session
    owns
ResourceId
    represents
Buffer
    backed by
backend resource
```

Only buffer resources are implemented. Images, video surfaces, external handle
export/import, compute execution, and persistent resources remain out of scope.

## ResourceId Semantics

`ResourceId` is a non-zero, opaque QGS-owned identifier. It does not expose
pointers, Vulkan handles, file descriptors, or native handles. Resource IDs are
unique within the owning session/lifetime and are not persistent across daemon
restarts. A resource from one session cannot be used or destroyed by another
session.

## BufferDesc

`CREATE_BUFFER` carries a fixed-size `BufferDesc`:

- `DeviceId`
- `size_bytes`
- usage flags
- memory preference flags

Step 5 usage flags are `TRANSFER_SRC`, `TRANSFER_DST`, and `STORAGE`.

## Memory Preference Model

The memory model is preference/requirement based:

- `DevicePreferred`
- `HostVisibleRequired`
- `HostCoherentPreferred`

This avoids assuming a simple RAM vs VRAM split. Integrated GPUs may validly
select memory that is both device-local and host-visible.

## Protocol Opcodes

New request opcodes:

- `CREATE_BUFFER`: request kind `1`, opcode `4`
- `DESTROY_RESOURCE`: request kind `1`, opcode `5`

New response opcodes:

- `BUFFER_CREATED`: response kind `2`, opcode `5`
- `RESOURCE_DESTROYED`: response kind `2`, opcode `6`

Stable errors added:

- `InvalidResourceId`: `22`
- `UnknownResource`: `23`
- `InvalidBufferSize`: `24`
- `AllocationFailed`: `25`
- `UnsupportedMemoryRequirements`: `26`
- `InvalidBufferUsage`: `27`

## Maximum M1 Buffer Size

The maximum single buffer size is `64 MiB`.

This is a conservative M1 safety limit to prevent unbounded client allocation
requests. It is not a final product limit.

## Ownership And Lifetime

`qgs-core` owns resource lifecycle policy. A `Session` contains a resource
registry. `CREATE_BUFFER` allocates through the backend, then registers the
opaque backend resource under the session. `DESTROY_RESOURCE` removes the
resource from that session registry, causing the backend object to drop.

`qgs-vulkan` owns the Vulkan buffer implementation object. Vulkan types and
handles do not escape the crate.

## Disconnect Cleanup

When a client disconnects, `qgsd` drops the session. Dropping the session drops
the resource registry and all still-owned backend resources. Unit tests use
mock backend resources with drop counters to verify this cleanup without
requiring hardware.

The manual daemon/client run also left three transient buffers alive before
disconnect. `qgsd` logged:

```text
client disconnected; releasing 3 resource(s) for session 1
qgs-core: releasing 3 resource(s) owned by session
```

## Test Results

`cargo test --workspace` passed.

Total: 73 tests passed.

- `qgs-core`: 12 passed
- `qgs-linux`: 1 passed
- `qgs-protocol`: 58 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Clippy Result

`cargo clippy --workspace --all-targets -- -D warnings` passed.

`cargo fmt --all -- --check` also passed.

## Intel Buffer Allocation Result

Manual `qgsd` / `qgs-test` run against Intel HD Graphics 4600:

- device class: `IntegratedGpu`
- created 1 MiB buffer
- returned `ResourceId`: `1`
- destroyed successfully
- selected memory:
  - device-local: yes
  - host-visible: yes
  - host-coherent: yes

## NVIDIA Buffer Allocation Result

Manual `qgsd` / `qgs-test` run against NVIDIA GeForce GTX 950M:

- device class: `DiscreteGpu`
- created 1 MiB buffer
- returned `ResourceId`: `2`
- destroyed successfully
- selected memory:
  - device-local: yes
  - host-visible: yes
  - host-coherent: yes

## Selected Memory Properties

Both tested physical GPUs selected memory reported by Vulkan as device-local,
host-visible, and host-coherent for the M1 demo buffer request. QGS reports the
selected properties as QGS-owned booleans and does not expose Vulkan memory
types or handles.

## Commit And Push Verification

This report is included in the M1 Step 5 commit. The final commit hash cannot
be embedded in the committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

After the final push, `main` and `origin/main` are expected to point to the same
M1 Step 5 commit. The exact verification is reported after push.

## Architectural Concerns

No blocking architectural concerns were found.

- `qgs-protocol` has no Vulkan dependency.
- `qgs-core` has no Vulkan dependency.
- `qgs-linux` has no Vulkan dependency.
- Vulkan-specific types remain inside `qgs-vulkan`.
- QGS-owned code remains `#![forbid(unsafe_code)]`.
- No Qnc concepts were added.
- Only buffer resources were implemented.
- No image or video-surface resources were added.
- No external memory export/import or native handle transfer was added.
- No compute execution, shaders, or pipelines were added.
- No video decode/encode APIs were introduced.
- No DRM/KMS path was introduced.
- No scheduling or telemetry was added.
- Resource IDs do not claim persistence beyond the owning session/lifetime.

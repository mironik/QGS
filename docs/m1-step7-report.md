# QGS M1 Step 7 Report

## Synchronization Primitive

Step 7 uses a Vulkan binary external semaphore exported as a Linux sync FD.

This was chosen because sync FD has one-shot copy-transference semantics that
match a producer-to-consumer frame ordering event: producer GPU work completes,
signals the primitive, and the consumer GPU waits before accessing the shared
resource.

External fences were not chosen because the proof needs GPU queue-to-queue
ordering. Timeline semaphores were not chosen for M1 because the one-shot sync
FD model is smaller and sufficient for the milestone proof.

## Vulkan External Handle Type

The exact external synchronization handle type used is:

- `VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT`

The memory/resource handle remains:

- DMA-BUF FD

## Reusable vs One-Shot Semantics

Linux sync FD has copy-transference semantics. Each exported FD represents one
signal/wait cycle. Imports must be temporary. The wait consumes the imported
payload. QGS does not implement reusable or timeline synchronization in Step 7.

## Import Semantics

The consumer imports the sync FD as a temporary payload into a binary Vulkan
semaphore. The imported FD is moved into Vulkano and is not reused by QGS after
the import call.

## Protocol Additions

New request opcodes:

- `CREATE_SYNC`: request kind `1`, opcode `7`
- `EXPORT_SYNC`: request kind `1`, opcode `8`

New response opcodes:

- `SYNC_CREATED`: response kind `2`, opcode `8`
- `SYNC_EXPORTED`: response kind `2`, opcode `9`

New stable errors:

- `InvalidSyncId`: `31`
- `UnknownSync`: `32`
- `UnsupportedSyncHandleType`: `33`
- `SyncExportFailed`: `34`

## SyncId Ownership Model

`SyncId` is non-zero, opaque, session-owned, and not persistent across sessions
or daemon restarts. It represents execution ordering, not resource memory.
`ResourceId` and `SyncId` are separate registries. Session disconnect releases
all still-owned sync objects.

## Native FD Ownership Lifecycle

`qgsd` owns the server-side QGS sync object. `EXPORT_SYNC` submits producer GPU
work that signals the semaphore and exports a native sync FD. The FD is
transferred to the client through `SCM_RIGHTS` as a transport attachment. The
client owns the received FD/imported payload after receipt. No FD is encoded as
an integer in the normal QGS payload.

## Unsafe Boundary Additions

New unsafe boundary:

- `crates/qgs-vulkan/src/external_sync.rs`

Unsafe APIs called:

- `vulkano::sync::semaphore::Semaphore::export_fd`
- `vulkano::sync::semaphore::Semaphore::import_fd`
- `vulkano::device::QueueGuard::submit`

Unsafe is required because Vulkano 0.35.2 exposes these external synchronization
and direct queue submit operations as unsafe. The public qgs-vulkan API remains
safe. All other QGS crates remain `#![forbid(unsafe_code)]`.

Unsafe blocks in QGS-owned code after Step 7:

- `4`

## Producer GPU Operation

The producer GPU operation is a minimal Vulkan transfer fill of the shared
external buffer with deterministic pattern `0x51475337`. The producer queue
submission signals the exportable binary semaphore.

## Consumer GPU Wait And Validation

The client imports the DMA-BUF into a second Vulkan context for the matching
physical device. It imports the sync FD into a binary semaphore, submits a
consumer GPU copy that waits on the imported semaphore, copies the first bytes
of the shared buffer into a host-visible readback buffer, waits on a fence for
that consumer submission to complete, and validates the copied pattern.

## Idle-Wait Confirmation

Producer-to-consumer ordering does not use `device.wait_idle()` or
`queue.wait_idle()`. The dependency is carried by the sync FD semaphore wait.
Fence waiting is used only after the consumer GPU submission so the CPU can
inspect the readback result.

## Test Results

`cargo test --workspace` passed.

Total: 98 tests passed.

- `qgs-core`: 19 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 73 passed
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

Step 7 result:

- created 1 MiB DMA-BUF-exportable buffer
- selected memory: device-local, host-visible, host-coherent
- exported DMA-BUF FD
- created binary sync object
- producer GPU fill submitted
- exported Linux sync FD
- transferred sync FD through `SCM_RIGHTS`
- imported shared buffer and sync FD in a second Vulkan context
- consumer GPU wait and dependent copy validation succeeded
- producer-to-consumer ordering used no device/queue idle wait

Mesa printed the existing Haswell Vulkan support warning.

## NVIDIA GTX 950M Result

Device:

- `NVIDIA GeForce GTX 950M (NVK GM107)`
- class: `DiscreteGpu`
- vendor/device: `0x10de / 0x139a`

Step 7 result:

- created 1 MiB DMA-BUF-exportable buffer
- selected memory: device-local, host-visible, host-coherent
- exported DMA-BUF FD
- created binary sync object
- producer GPU fill submitted
- exported Linux sync FD
- transferred sync FD through `SCM_RIGHTS`
- imported shared buffer and sync FD in a second Vulkan context
- consumer GPU wait and dependent copy validation succeeded
- producer-to-consumer ordering used no device/queue idle wait

## Cleanup And Disconnect Result

The successful qgs-test run left three transient buffers and two sync objects
alive, then disconnected. `qgsd` logged:

```text
client disconnected; releasing 3 resource(s) and 2 sync object(s) for session 1
qgs-core: releasing 3 resource(s) owned by session
qgs-core: releasing 2 sync object(s) owned by session
```

## Commit And Push Verification

This report is included in the M1 Step 7 commit. The final commit hash cannot
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
- Unsafe Rust remains isolated to audited qgs-vulkan boundaries.
- Synchronization and resource identity remain separate.
- No Qnc concepts were added.
- No image resources, video surfaces, compute shaders, video APIs, DRM/KMS,
  scheduling, telemetry, or reusable external semaphore workflow were added.
- Step 7 proves one-shot external GPU ordering; it does not implement the final
  frame synchronization system.

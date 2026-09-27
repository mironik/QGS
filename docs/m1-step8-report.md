# QGS M1 Step 8 Report

## Compute Operation

Step 8 uses one fixed internal compute operation:

```text
output[i] = input[i] + 1
```

The proof runs over 1024 `u32` elements. The visible validation sample is:

```text
[0, 1, 2, 3, 100, 1000] -> [1, 2, 3, 4, 101, 1001]
```

This is a processing proof only. It is not a public QGS compute API.

## Shader Implementation

The shader is fixed QGS-owned SPIR-V embedded in `qgs-vulkan`. It is not
provided by the client and is not sent through the QGS protocol.

`qgs-vulkan` creates a Vulkano compute pipeline from this embedded shader,
reflects the descriptor layout, binds the imported shared buffer as a storage
buffer, and records one compute dispatch.

No shader compiler/runtime dependency was added.

## Resource Flow

The Step 8 flow is:

```text
qgs-test
    -> CREATE_BUFFER(storage, transfer, external sharing required)
qgsd/qgs-vulkan
    -> creates exportable QGS buffer resource
qgs-test
    -> EXPORT_RESOURCE
qgsd
    -> transfers DMA-BUF FD through SCM_RIGHTS
qgs-test/qgs-vulkan
    -> imports the DMA-BUF into a second Vulkan context
    -> writes input values through a local upload buffer
    -> runs the fixed compute shader against the imported shared buffer
    -> copies the GPU result to a local readback buffer for validation
```

The QGS `ResourceId` remains owned by the server-side session. The imported
client-side Vulkan objects are proof infrastructure and are not added to the
server registry.

## Synchronization Chain

Step 8 reuses the Step 7 external synchronization path:

```text
qgs-test
    -> CREATE_SYNC
qgs-test
    -> EXPORT_SYNC(resource_id, fill_pattern)
qgsd/qgs-vulkan producer queue
    -> submits GPU transfer work
    -> signals a binary external semaphore
qgsd
    -> transfers Linux sync FD through SCM_RIGHTS
qgs-test/qgs-vulkan consumer queue
    -> imports the sync FD
    -> waits on it in the compute submission
    -> uploads input, dispatches compute, copies result to readback
```

The producer-to-consumer ordering is carried by the external sync FD. No
`device.wait_idle()` or `queue.wait_idle()` is used for this dependency.

A fence wait is used only after the consumer compute submission so the CPU can
inspect the readback buffer for validation.

## GPU Execution Confirmation

The increment transformation is recorded as a Vulkan compute dispatch using the
fixed embedded shader. CPU code prepares input values and computes the expected
values only after GPU execution so the result can be checked.

The CPU does not perform the transformation used to claim compute success.

## IPC Payload Contents

Processed buffer data does not cross normal QGS IPC.

The protocol carries bounded metadata. Native DMA-BUF and sync FDs are carried
as Linux transport attachments with `SCM_RIGHTS`. The working buffer contents
remain resource-backed and are processed through Vulkan buffer operations.

## Tests

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

Step 8 result:

- created 1 MiB exportable storage buffer
- exported DMA-BUF FD through SCM_RIGHTS
- created and exported Linux sync FD through SCM_RIGHTS
- imported buffer and sync FD into a second Vulkan context
- compute shader processed 1024 `u32` elements
- validation passed
- sample: `[0, 1, 2, 3, 100, 1000] -> [1, 2, 3, 4, 101, 1001]`
- resource destroyed successfully

Mesa also printed the existing Haswell Vulkan support warning.

## GTX 950M Result

Device:

- `NVIDIA GeForce GTX 950M (NVK GM107)`
- class: `DiscreteGpu`
- vendor/device: `0x10de / 0x139a`

Step 8 result:

- created 1 MiB exportable storage buffer
- exported DMA-BUF FD through SCM_RIGHTS
- created and exported Linux sync FD through SCM_RIGHTS
- imported buffer and sync FD into a second Vulkan context
- compute shader processed 1024 `u32` elements
- validation passed
- sample: `[0, 1, 2, 3, 100, 1000] -> [1, 2, 3, 4, 101, 1001]`
- resource destroyed successfully

## Unsafe Inventory Changes

Step 8 adds one private qgs-vulkan module:

- `crates/qgs-vulkan/src/external_compute.rs`

New unsafe APIs used there:

- `vulkano::shader::ShaderModule::new`
- `vulkano::command_buffer::AutoCommandBufferBuilder::dispatch`

The public qgs-vulkan API remains safe. No unsafe API is exposed to
`qgs-core`, `qgs-protocol`, `qgs-linux`, `qgsd`, or `qgs-test`.

Unsafe block count in QGS-owned code after Step 8: 6.

All crates other than `qgs-vulkan` remain `#![forbid(unsafe_code)]`.

## Device And Queue Idle Usage

No `device.wait_idle()` or `queue.wait_idle()` is used for producer-to-consumer
ordering.

The compute proof uses a fence wait after the consumer queue submission only so
CPU validation can read the final result.

## Disconnect Cleanup

The final qgs-test run left three transient buffers alive and disconnected.
`qgsd` logged:

```text
client disconnected; releasing 3 resource(s) and 4 sync object(s) for session 1
qgs-core: releasing 3 resource(s) owned by session
qgs-core: releasing 4 sync object(s) owned by session
```

## Commit And Push Verification

This report is included in the M1 Step 8 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Architectural Concerns

No blocking architectural concerns remain.

- No QGS public compute protocol was added.
- No shader, pipeline, descriptor, or dispatch operations are exposed over the
  wire.
- No SPIR-V blobs are accepted from clients or sent through QGS IPC.
- `qgs-protocol` has no Vulkan or Linux FD dependency.
- `qgs-core` has no Vulkan dependency.
- `qgs-linux` knows only transport FD attachment semantics, not Vulkan.
- Vulkan-specific types remain inside `qgs-vulkan`.
- Unsafe Rust remains isolated to audited qgs-vulkan modules.
- No Qnc concepts were added.
- No image resources, video surfaces, video decode, video encode, DRM/KMS,
  scheduling, telemetry, or generic compute API were added.

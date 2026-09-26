# QGS M1 Step 4 Report

## Files Changed

Changed:

- `crates/qgs-protocol/`
- `crates/qgs-core/`
- `crates/qgs-vulkan/`
- `daemon/qgsd/`
- `tools/qgs-test/`
- `docs/architecture.md`
- `docs/protocol.md`
- `docs/m1-step4-report.md`

## Capability Model

Step 4 adds a small QGS-owned capability model:

- `DeviceCapabilities`
- `ComputeCapabilities`
- `MemoryCapabilities`
- `MemoryHeapDesc`
- `InteropCapabilities`

The model remains static discovery only. It does not report load, free memory,
free VRAM, throughput, telemetry, scheduling suitability, or benchmark data.

## Protocol Additions

New request:

- `QUERY_DEVICE_CAPABILITIES`: request kind `1`, opcode `3`

New response:

- `DEVICE_CAPABILITIES`: response kind `2`, opcode `4`

Existing messages remain:

- `HELLO`: request kind `1`, opcode `1`
- `ENUMERATE_DEVICES`: request kind `1`, opcode `2`
- `WELCOME`: response kind `2`, opcode `1`
- `ERROR`: response kind `2`, opcode `2`
- `DEVICE_LIST`: response kind `2`, opcode `3`

Unknown `DeviceId` maps to stable protocol error code `UnknownDeviceId`.

## Compute Properties Reported

For each device QGS reports:

- whether a compute-capable queue family exists
- maximum compute workgroup count
- maximum compute workgroup size
- maximum compute workgroup invocations

QGS does not infer compute support from device class, device name, or vendor ID.

## Memory Model Reported

For each device QGS reports:

- memory heaps
- heap size in bytes
- whether each heap is device-local
- memory type count summary
- whether any host-visible memory type exists
- whether any host-coherent memory type exists
- whether any device-local memory type exists

QGS does not calculate free VRAM or current memory pressure in this milestone.

## Interop Mechanisms Reported

For each device QGS reports whether Vulkan exposes:

- external memory FD
- DMA-BUF external memory mechanism
- external semaphore FD
- external fence FD

These fields report mechanism availability only. They do not guarantee that any
particular image format, buffer, or resource usage is exportable/importable.

## Protocol Limits

- maximum payload size: 4096 bytes
- maximum device count: 16
- maximum device name length: 128 UTF-8 bytes
- maximum memory heap count: 16
- maximum memory type count summary: 32

All untrusted wire counts are validated before allocation.

## Test Results

`cargo test --workspace` passed.

Total tests: 51

- `qgs-core`: 2 passed
- `qgs-linux`: 1 passed
- `qgs-protocol`: 46 passed
- `qgs-vulkan`: 2 passed
- `qgsd`: 0 tests
- `qgs-test`: 0 tests

## Clippy

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Intel HD Graphics 4600 Result

- class: `IntegratedGpu`
- vendor `0x8086`, device `0x0416`
- Vulkan API `1.2.335`
- compute supported: yes
- max workgroup count: `65535 x 65535 x 65535`
- max workgroup size: `1024 x 1024 x 1024`
- max invocations: `1024`
- memory heaps:
  - heap 0: `1536 MiB`, device-local
- memory types: `2`
- host-visible: yes
- host-coherent: yes
- device-local: yes
- external-memory-fd: yes
- dma-buf mechanism: yes
- semaphore-fd: yes
- fence-fd: yes

The Mesa Intel driver emitted:

`MESA-INTEL: warning: Haswell Vulkan support is incomplete`

Discovery still completed successfully.

## NVIDIA GTX 950M Result

- class: `DiscreteGpu`
- vendor `0x10de`, device `0x139a`
- Vulkan API `1.3.281`
- compute supported: yes
- max workgroup count: `2147483647 x 65535 x 65535`
- max workgroup size: `1024 x 1024 x 64`
- max invocations: `1024`
- memory heaps:
  - heap 0: `2048 MiB`, device-local
  - heap 1: `256 MiB`, device-local
  - heap 2: `5482 MiB`
- memory types: `3`
- host-visible: yes
- host-coherent: yes
- device-local: yes
- external-memory-fd: yes
- dma-buf mechanism: yes
- semaphore-fd: yes
- fence-fd: yes

## llvmpipe Result

- class: `Software`
- vendor `0x10005`, device `0x0000`
- Vulkan API `1.3.281`
- compute supported: yes
- max workgroup count: `65535 x 65535 x 65535`
- max workgroup size: `1024 x 1024 x 1024`
- max invocations: `1024`
- memory heaps:
  - heap 0: `7310 MiB`, device-local
- memory types: `1`
- host-visible: yes
- host-coherent: yes
- device-local: yes
- external-memory-fd: yes
- dma-buf mechanism: yes
- semaphore-fd: yes
- fence-fd: yes

## UMA / Integrated GPU Observations

The Intel integrated GPU reports a device-local heap and host-visible,
host-coherent memory availability. QGS must not model all GPU memory as
dedicated discrete VRAM. Integrated and software devices can expose memory that
is simultaneously useful to the device and visible to the host.

## Commit Hash Handling

This report is included in the M1 Step 4 commit. The final commit hash cannot
be embedded in the committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

## Origin Verification

After the final push, `main` and `origin/main` are expected to point to the same
M1 Step 4 commit. The exact hash is reported after push verification.

## Architectural Concerns

No blocking architectural concerns were found.

- `qgs-protocol` has no Vulkan dependency.
- `qgs-core` has no Vulkan dependency.
- `qgs-linux` has no Vulkan dependency.
- Vulkan-specific types remain inside `qgs-vulkan`.
- QGS-owned code remains `#![forbid(unsafe_code)]`.
- No Qnc concepts were added.
- No GPU resources are allocated.
- No compute execution occurs.
- No video APIs were introduced.
- No persistent `DeviceId` assumption was added.
- All untrusted wire counts are bounded before allocation.

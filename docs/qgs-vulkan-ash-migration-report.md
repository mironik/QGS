# QGS Vulkan Ash Migration Report

## Summary

GPU R1 replaces the `qgs-vulkan` production backend from Vulkano 0.35.2 plus
raw escape hatches to a QGS-owned safe Vulkan backend implemented directly over
`ash`.

Final production dependency chain:

```text
QGS
  |
qgs-vulkan
  |
QGS-owned safe wrappers
  |
ash
  |
Vulkan loader / driver
```

No `ash` or raw Vulkan type escapes `qgs-vulkan`. `qgs-core` and
`qgs-protocol` remain Vulkan-independent.

## Old Vulkano Architecture

The old implementation used Vulkano for instance/device enumeration, memory
allocation, buffers, images, command buffers, descriptor/pipeline setup, and
ordinary GPU work. M1/M2 interop work already required raw Vulkan escape
hatches for external memory, sync FD, compute dispatch, DRM modifier queries,
and Haswell imported-video diagnostics.

## Final Direct Ash Architecture

The new backend uses private RAII wrappers in `crates/qgs-vulkan/src/lib.rs`
for:

- `GpuInstance`
- `GpuDevice`
- `GpuMemory`
- `GpuBuffer`
- `GpuImage`
- `GpuImageView`
- `GpuSemaphore`
- `GpuFence`
- `GpuCommandPool`
- fixed `ComputePipelineState`

The retained `haswell_video_diagnostic.rs` remains diagnostic-only and is not a
production video-processing path.

## Files And Dependencies

Introduced/reworked:

- `crates/qgs-vulkan/src/lib.rs`: direct ash production backend

Removed obsolete Vulkano escape-hatch modules:

- `crates/qgs-vulkan/src/external_memory.rs`
- `crates/qgs-vulkan/src/external_sync.rs`
- `crates/qgs-vulkan/src/external_compute.rs`

Dependency changes:

- removed direct `vulkano = 0.35.2`
- kept explicit direct `ash = 0.38.0`
- `cargo tree -p qgs-vulkan` now shows only `ash`, `qgs-core`, and
  `qgs-protocol` under `qgs-vulkan`
- `Cargo.lock` no longer contains Vulkano

## Ownership Model

Vulkan objects are owned by Rust RAII wrappers and destroyed in dependency
order. Device resources hold `Arc<GpuDevice>` so the logical device and instance
outlive resources. Bound memory is kept alive until buffers/images are
destroyed. Mapped memory is unmapped before free.

Submitted command buffers are paired with fences through `PendingSubmission`.
Sync-resource drop waits pending submissions before semaphore destruction.
Buffer/image drop performs a cleanup-boundary device idle wait before resource
destruction; this is not used as the producer-to-consumer synchronization path.

## Memory Model

The backend implements a small memory selector over Vulkan memory properties:

- required flags must be present
- preferred flags are selected when available
- host-visible and coherent properties are reported through QGS metadata
- imported memory is not pooled
- external memory import checks borrowed-FD memory type bits before consuming
  the owned FD

## Command And Compute Model

Command support is intentionally narrow:

- one-shot primary command buffers
- queue submit
- fences for CPU validation/cleanup
- fill buffer
- copy buffer
- clear RGBA image
- image-to-buffer copy
- fixed compute dispatch

Fixed embedded SPIR-V shaders are retained for:

- storage-buffer `u32 + 1` proof
- storage-image RGBA invert proof

The buffer shader now includes the required
`SPV_KHR_storage_buffer_storage_class` declaration so validation accepts it.
Descriptor layouts are explicit QGS-owned layouts rather than Vulkano
reflection.

## Parity Results

Validated production parity:

- physical-device enumeration
- `DeviceDesc` mapping
- compute capabilities
- memory heap/type reporting
- external-memory FD and DMA-BUF capability reporting
- external semaphore sync-FD capability reporting
- buffer creation and cleanup
- memory preference behavior
- external buffer export/import
- DMA-BUF buffer sharing
- sync-FD export/import
- producer/consumer GPU synchronization proof
- compute increment proof
- RGBA image creation/export/import/invert proof
- H.264 hardware decode regression
- Step 9 software decode regression

The Haswell VA->Vulkan zero-copy path remains frozen and was not resumed.

## DRM Modifier Foundation

The production backend preserves reusable ash foundations needed later:

- borrowed-FD `vkGetMemoryFdPropertiesKHR`
- external-memory image creation hooks
- exact external image support validation
- QGS documentation retains the rule:

```text
DRM object count != video format plane count != Vulkan DRM modifier memory-plane count
```

The crashing Haswell imported NV12 path was not reintroduced as a claimed
success path.

## Unsafe Inventory

Before GPU R1:

- QGS-owned unsafe block count: 38
- all were in the existing `qgs-vulkan` interop/diagnostic boundary

After GPU R1:

- production direct ash backend: 82 unsafe blocks
- retained Haswell diagnostic: 32 unsafe blocks
- total QGS-owned unsafe block count: 114
- unsafe remains confined to `qgs-vulkan`
- all non-`qgs-vulkan` crates retain their existing safe-Rust policy

The increase is expected: direct `ash` makes Vulkan FFI explicit. The unsafe
surface is now coherent, private, and under QGS ownership instead of split
between Vulkano state and raw escape hatches.

## Validation Layer

`qgs-vulkan` can enable `VK_LAYER_KHRONOS_validation` with:

```sh
QGS_VULKAN_ENABLE_VALIDATION=1
```

Validation-enabled qgsd/qgs-test parity was run. Initial validation found two
migration issues:

- the embedded buffer shader lacked the SPIR-V storage-buffer extension
- cleanup could destroy resources/semaphores while submitted work was still
  tracked as in-flight

Both were fixed. The final validation-enabled parity run completed without
Vulkan validation errors.

## Hardware Results

Intel HD Graphics 4600 / Haswell:

- enumerated as integrated Vulkan device
- buffer/resource proofs passed
- DMA-BUF export/import passed
- sync-FD proof passed
- compute proof passed
- RGBA image proof passed
- H.264 8-bit 4:2:0 VA-API hardware decode regression passed

NVIDIA GTX 950M / NVK:

- enumerated as discrete Vulkan device
- buffer/resource proofs passed
- DMA-BUF export/import passed
- sync-FD proof passed
- compute proof passed
- RGBA image proof passed
- VA decode capabilities remain empty; software fallback handled unsupported
  H.264 decode cases

llvmpipe:

- enumerated as software Vulkan device
- capability reporting works
- qgs-test hardware proof loop continues to target physical GPU devices only

## Test Results

Quality gates:

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed
- `cargo test --workspace`: passed

Workspace tests:

- 205 unit tests passed
- doc tests passed

Hardware and integration proofs:

- `cargo run -q -p qgs-test`: passed
- `QGS_VULKAN_ENABLE_VALIDATION=1 cargo run -q -p qgs-test`: passed without
  Vulkan validation errors
- `cargo run -q -p qgs-test -- --h264-decode-only`: passed
- external Sony FX6 sample SHA-256 matched
- `cargo run -q -p qgs-test -- --software-decode-mxf <external FX6 MXF>`:
  passed, decoded 106 `yuv422p10le` frames and matched random-access frame 53
  checksum against sequential frame 53

## Step 9 Regression

The software video path remains independent of Vulkan:

- `qgs-mxf` still parses/indexes MXF
- `qgs-software-video` still owns libavcodec software decode
- 10-bit 4:2:2 software surfaces remain CPU-backed `VideoSurface` resources
- no software VideoSurface -> GPU upload was added

## Limitations

- No software VideoSurface -> GPU upload path yet.
- No resumed VA->Vulkan zero-copy path.
- No generic shader API.
- No general Vulkan memory allocator.
- Cleanup-boundary device idle waits are intentionally conservative and should
  be refined when QGS grows reusable frame scheduling.
- Device matching remains based on current QGS-visible identity rather than a
  full persistent DRM/PCI identity framework.

## Recommendation

Proceed next with the software VideoSurface -> GPU upload milestone on top of
the direct ash backend. Keep the first upload path narrow: preserve
`yuv422p10le` semantics, use explicit staging/upload resources, and add color
conversion only as a fixed internal processing proof rather than a general
color pipeline.

## Commit And Push Verification

This report is included in the migration commit:

- `Migrate QGS Vulkan backend to direct ash`

The final pushed commit hash is reported after the push completes.

# QGS Vulkan Backend Migration Audit

## Executive Summary

This audit reviews whether `qgs-vulkan` should continue as a Vulkano 0.35.2
backend with raw Vulkan escape hatches, or migrate now to a small QGS-owned
safe Vulkan backend implemented directly over `ash`.

The conclusion is that QGS should migrate now, before adding software
VideoSurface upload, 10-bit 4:2:2 GPU processing, modern zero-copy video
interop, HEVC, or encode support.

The reason is not dependency taste. It is architectural fit. QGS's future GPU
work is professional video interop: DMA-BUF, DRM modifiers, multi-planar YUV,
disjoint memory, externally produced image contents, explicit ownership
transfer, external synchronization, and fixed compute/video-processing shaders.
Those are exactly the areas where QGS already had to bypass Vulkano or freeze a
milestone because Vulkano's safe image-state and ownership abstractions could
not express the required semantics.

## Current qgs-vulkan Inventory

Files:

| File | LOC | Role |
| --- | ---: | --- |
| `src/lib.rs` | 2149 | Production Vulkan device discovery, resources, sync, external sharing, compute/image proofs |
| `src/external_memory.rs` | 19 | Vulkano external memory import escape hatch |
| `src/external_sync.rs` | 50 | Vulkano external semaphore export/import and queue-submit escape hatches |
| `src/external_compute.rs` | 28 | Vulkano shader-module and compute-dispatch escape hatches |
| `src/haswell_video_diagnostic.rs` | 598 | Raw ash diagnostic for VA/DRM PRIME NV12 imported-image failure isolation |
| `Cargo.toml` | 11 | `ash`, `qgs-core`, `qgs-protocol`, `vulkano` |

Total: 2855 lines including `Cargo.toml`.

Current direct dependencies:

- `ash 0.38.0` with `linked`, `std`, `debug`
- `vulkano 0.35.2` with default features disabled
- `qgs-core`
- `qgs-protocol`

`qgs-vulkan` already has an explicit direct `ash` dependency. It should remain
explicit if Vulkano is removed.

## Functional Categories

### A. Device Discovery

Implemented in `VulkanDeviceDiscovery::new`, `describe_device`, and
`describe_capabilities`.

Current Vulkano use:

- `VulkanLibrary`
- `Instance`
- `InstanceCreateInfo`
- `PhysicalDevice`
- physical-device properties
- memory properties
- queue-family properties
- supported device extensions

Direct Vulkan complexity: simple to moderate. Enumeration and property queries
map directly to ash calls. QGS already implements equivalent raw enumeration in
the Haswell diagnostic.

### B. Device Creation

Implemented in `VulkanDeviceDiscovery::new`.

Current Vulkano use:

- `Device`
- `DeviceCreateInfo`
- `DeviceExtensions`
- `QueueCreateInfo`
- enabled external-memory, DMA-BUF, external semaphore, dedicated allocation,
  and DRM modifier extensions

Direct Vulkan complexity: moderate. It requires explicit extension-name lists,
feature chains, queue creation, and destruction ordering. The Haswell
diagnostic already creates an ash device with external-memory and DRM-modifier
extensions.

### C. Queues

Implemented by selecting one queue family with graphics+compute, compute, or
any queue fallback.

Current Vulkano use:

- `Queue`
- `QueueFlags`
- `QueueGuard::submit` through `external_sync::submit_queue`

Direct Vulkan complexity: simple. QGS needs one queue today, later perhaps a
dedicated transfer/compute queue only if profiling or interop requires it.

### D. Memory

Implemented through Vulkano's `StandardMemoryAllocator`, Vulkano memory
filters, and explicit export/import metadata.

Current Vulkano use:

- `StandardMemoryAllocator`
- `GenericMemoryAllocatorCreateInfo`
- `AllocationCreateInfo`
- `MemoryTypeFilter`
- `MemoryAllocatePreference`
- `DeviceMemory`
- `MemoryAllocateInfo`
- `MemoryImportInfo`
- `DedicatedAllocation`
- memory mapping helpers
- export FD helpers

Direct Vulkan complexity: moderate. QGS currently needs host-visible upload and
readback buffers, device-preferred storage buffers/images, exportable
allocations, and imported DMA-BUF allocations. A small allocator is enough for
near-term needs; imported/external memory will remain explicitly allocated
rather than pooled.

### E. Buffers

Implemented in `create_buffer`, `import_external_buffer`,
`validate_synced_gpu_copy`, and compute proofs.

Current Vulkano use:

- `Buffer`
- `RawBuffer`
- `Subbuffer`
- `BufferCreateInfo`
- `BufferUsage`
- `ExternalBufferInfo`
- `BufferMemory`
- buffer read/write guards

Direct Vulkan complexity: moderate but bounded. QGS needs `vkCreateBuffer`,
`vkGetBufferMemoryRequirements`, `vkAllocateMemory`, `vkBindBufferMemory`,
`vkMapMemory`, `vkFlushMappedMemoryRanges`/`vkInvalidateMappedMemoryRanges`
where not coherent, copy commands, and export/import FD plumbing.

### F. Images

Implemented in `create_image`, `import_external_image`, image export, image
clear, and image invert proof.

Current Vulkano use:

- `Image`
- `RawImage`
- `ImageCreateInfo`
- `ImageUsage`
- `ImageTiling`
- `ImageDrmFormatModifierInfo`
- `ImageFormatInfo`
- `ImageMemory`
- `ImageView`
- `CopyImageToBufferInfo`
- `ClearColorImageInfo`

Direct Vulkan complexity: moderate for ordinary RGBA images; high for video
interop. The hard part is exactly where Vulkano was insufficient: external
multi-planar images with DRM modifiers and externally produced contents.

### G. Compute

Implemented by fixed embedded SPIR-V shaders:

- `COMPUTE_INCREMENT_SHADER`
- `IMAGE_INVERT_SHADER`

Current Vulkano use:

- `ShaderModule`
- `ComputePipeline`
- `PipelineLayout`
- `PipelineShaderStageCreateInfo`
- `PipelineDescriptorSetLayoutCreateInfo`
- reflection-derived descriptor layouts
- `DescriptorSet`
- `WriteDescriptorSet`
- `AutoCommandBufferBuilder::dispatch`

Direct Vulkan complexity: moderate. QGS uses fixed internal shaders and simple
descriptor sets. Direct ash code would be more verbose, but the scope is small:
storage buffers, storage images, sampled video images later, and fixed compute
pipelines.

### H. Command Buffers

Implemented with `AutoCommandBufferBuilder` for:

- fill buffer
- clear image
- copy buffer
- copy image to buffer
- bind compute pipeline
- bind descriptor sets
- dispatch

Direct Vulkan complexity: moderate. QGS needs primary command buffers,
one-time submissions, pipeline barriers, copies, clears, dispatches, and later
image layout/ownership transitions.

### I. Synchronization

Implemented with:

- binary semaphores
- sync FD export/import
- fences for validation completion
- queue submit

Current Vulkano use:

- `Semaphore`
- `SemaphoreCreateInfo`
- `ExternalSemaphoreInfo`
- `ExternalSemaphoreHandleTypes`
- `ExternalSemaphoreHandleType`
- `ImportSemaphoreFdInfo`
- `SemaphoreImportFlags::TEMPORARY`
- `Fence`
- `FenceCreateInfo`
- `SubmitInfo`
- `SemaphoreSubmitInfo`

Direct Vulkan complexity: moderate. QGS must own binary semaphores, fences,
external sync FD import/export, wait/signal submit arrays, and later timeline
or cross-domain sync only when needed.

### J. External Memory

Implemented for buffers and RGBA images through Vulkano plus
`DeviceMemory::import` escape hatch.

Current Vulkano limitations:

- Vulkano memory export is usable.
- Vulkano memory import requires unsafe `DeviceMemory::import`.
- Vulkano's `Device::memory_fd_properties` consumes `File`, which was not
  suitable for borrowed-FD `vkGetMemoryFdPropertiesKHR` queries.

Direct Vulkan complexity: moderate and clearer. QGS can expose safe wrappers
that explicitly distinguish borrowed FD property queries from FD-consuming
memory import.

### K. DMA-BUF

Implemented for M1 buffer/image sharing and attempted VA surface interop.

Current Vulkano use:

- `ExternalMemoryHandleType::DmaBuf`
- `ExternalMemoryHandleTypes`
- external buffer/image property queries
- export FD from device memory

Direct Vulkan complexity: moderate. Direct control is beneficial because QGS
must validate handle type, memory type bits, dedicated allocation, import
ownership, and FD lifetime precisely.

### L. DRM Modifiers

Production use is limited to exported/imported RGBA image metadata. The
Haswell diagnostic uses raw ash for exact NV12 modifier queries and attempted
creation/import.

Current Vulkano problems:

- Normal Vulkano image state starts first use from `Undefined`, which is wrong
  for externally produced image contents that must be preserved.
- Vulkano creation support is not enough; QGS also needs exact ownership,
  memory-plane, and layout semantics.

Direct Vulkan complexity: high but necessary for professional video.

### M. External Synchronization

Step 7 uses sync FD external semaphore semantics. The helper lives in
`external_sync.rs`.

Direct Vulkan complexity: moderate and expected to remain in a narrow backend
boundary.

### N. Diagnostics

`haswell_video_diagnostic.rs` is already a raw ash implementation. It:

- creates an instance with validation and synchronization validation
- queries exact DRM modifier properties
- creates a device with external-memory and DRM-modifier extensions
- queries borrowed-FD memory properties
- creates an explicit DRM modifier image on the retained path
- imports DMA-BUF memory
- attempts image memory binding
- records a foreign acquire barrier
- submits acquire-only work where the driver/path permits

This is diagnostic-only and should not be preserved as production architecture.
Its findings should be preserved.

### O. Tests

Current qgs-vulkan unit tests are small:

- Vulkan physical-device type to QGS `DeviceClass`
- bounded UTF-8 device-name handling

Most GPU validation is exercised through `qgs-test` hardware proofs and
workspace integration.

## Vulkano API Inventory

QGS actually uses these Vulkano modules/types:

| Vulkano API | Why QGS uses it | Fundamental? | Direct Vulkan replacement |
| --- | --- | --- | --- |
| `VulkanLibrary`, `Instance`, `InstanceCreateInfo` | Instance/discovery | No | Simple ash instance wrapper |
| `PhysicalDevice`, `PhysicalDeviceType` | Device properties/capabilities | No | Simple/moderate ash queries |
| `Device`, `DeviceCreateInfo`, `DeviceExtensions` | Logical device and extension enablement | No | Moderate ash wrapper |
| `Queue`, `QueueCreateInfo`, `QueueFlags` | Queue selection/submission | No | Simple wrapper |
| `StandardMemoryAllocator` | General buffer/image allocation | Convenience only | Simple QGS allocator initially |
| `Buffer`, `RawBuffer`, `Subbuffer` | Buffers and imported buffers | No | Moderate RAII buffer wrapper |
| `BufferCreateInfo`, `BufferUsage`, `ExternalBufferInfo` | Buffer creation/properties | No | Direct create/properties structs |
| `Image`, `RawImage`, `ImageCreateInfo` | Images and imported images | No | Moderate/high RAII image wrapper |
| `ImageDrmFormatModifierInfo` | Modifier property validation | Not sufficient | Direct modifier query/create path |
| `ImageView` | Storage-image view | No | Direct image-view wrapper |
| `AutoCommandBufferBuilder` | Command recording | Convenience only | Direct command-buffer wrapper |
| `CopyBufferInfo`, `CopyImageToBufferInfo`, `ClearColorImageInfo` | Demo commands | No | Direct cmd calls |
| `ShaderModule`, `ShaderModuleCreateInfo` | Fixed shaders | No | Direct shader module wrapper |
| `ComputePipeline`, `PipelineLayout` | Fixed compute pipelines | No | Moderate pipeline builder |
| `PipelineDescriptorSetLayoutCreateInfo` | Reflection-derived layouts | Convenience only | Explicit fixed layouts |
| `DescriptorSet`, `WriteDescriptorSet` | Buffer/image descriptors | No | Descriptor pool/set wrappers |
| `Semaphore`, `Fence` | Sync | No | Direct RAII wrappers |
| `SubmitInfo`, `SemaphoreSubmitInfo` | Submission | No | Direct submit wrapper |
| `DeviceMemory`, `MemoryImportInfo` | External memory import | Already unsafe | Direct import wrapper clearer |

None of these Vulkano APIs are fundamental to QGS's public architecture. The
most useful pieces are allocator and command-builder convenience, but they are
not aligned with the next video milestones.

## Raw Vulkan Inventory

### `external_memory.rs`

Reason Vulkano was insufficient:

- external memory import is exposed as unsafe

Vulkan operation:

- import external FD as Vulkan device memory through Vulkano's unsafe wrapper

Unsafe blocks: 1.

Interop awkwardness:

- QGS passes exporter memory type index/allocation size through metadata
  because Vulkano import uses its own memory object model.
- Borrowed-FD property query could not use Vulkano's File-consuming interface.

Direct ownership simplification:

- QGS would explicitly query `vkGetMemoryFdPropertiesKHR`, choose memory type,
  import FD, and bind memory in one coherent wrapper.

### `external_sync.rs`

Reason Vulkano was insufficient:

- sync FD export/import and guarded queue submission are unsafe Vulkano escape
  hatches

Vulkan operations:

- semaphore sync FD export
- semaphore sync FD temporary import
- queue submit

Unsafe blocks: 3.

Interop awkwardness:

- QGS relies on Vulkano objects but must reason about Vulkan external semaphore
  ownership and temporary import semantics itself.

Direct ownership simplification:

- A QGS semaphore wrapper can make FD consumption/reference-transference
  explicit and pair it with QGS submit semantics.

### `external_compute.rs`

Reason Vulkano was insufficient:

- Vulkano requires unsafe shader-module creation from SPIR-V.
- Vulkano requires unsafe compute dispatch recording.

Vulkan operations:

- shader module creation
- dispatch

Unsafe blocks: 2.

Direct ownership simplification:

- QGS can validate embedded SPIR-V at build/test time and keep shader-module
  creation inside a fixed-shader wrapper.

### `haswell_video_diagnostic.rs`

Reason Vulkano was insufficient:

- exact DRM modifier query and imported-image memory-plane analysis
- borrowed-FD memory property query
- explicit DMA-BUF import
- explicit foreign ownership acquire
- validation/sync-validation capture
- avoiding Vulkano image first-use transitions that discard external contents

Vulkan operations:

- raw instance/device creation
- debug utils messenger
- physical-device enumeration and properties
- DRM modifier format property queries
- `vkGetMemoryFdPropertiesKHR`
- DRM modifier image create
- device memory import
- bind image memory
- command pool/buffer/fence creation
- image memory barrier
- queue submit and fence wait
- cleanup

Unsafe blocks: 32 block expressions, plus unsafe functions/callbacks.

Interop awkwardness:

- The diagnostic is effectively a second Vulkan implementation inside
  `qgs-vulkan`.
- It cannot cleanly share Vulkano's device, allocator, image state, or command
  state.
- The diagnostic had to be conservative and stop before becoming production
  architecture.

Direct ownership simplification:

- A direct backend would make this code part of the normal Vulkan ownership
  model instead of a separate diagnostic island.

## Unsafe Analysis

Current QGS-owned unsafe block count:

- 38 `unsafe { ... }` blocks
- 32 in `haswell_video_diagnostic.rs`
- 6 in Vulkano escape-hatch modules

Classification:

| Category | Count/area | Notes |
| --- | --- | --- |
| Fundamentally required by Vulkan FFI | Most raw ash diagnostic blocks | ash calls are unsafe by design |
| Required because of Vulkano escape hatches | 6 | import/export/submit/shader/dispatch |
| Diagnostic-only | 32 blocks | Haswell Step 4B diagnostic |
| Potentially removable | Some diagnostic duplication | after clean direct wrappers exist |
| Likely still required in ash backend | most creation/query/submit/drop calls | but can be grouped behind RAII wrappers |

Unsafe-block count alone is not the quality metric. A direct ash backend may
still have a comparable or larger raw count during migration, but the quality
can improve if:

- each unsafe boundary maps directly to a QGS-owned wrapper;
- FD ownership is explicit;
- resource destruction order is encoded by RAII;
- image layout/ownership transitions are QGS decisions, not Vulkano side
  effects;
- diagnostic-only raw code is either removed or rebuilt on top of production
  wrappers.

A clean ash backend should aim for fewer public unsafe islands, not
necessarily fewer individual `unsafe {}` expressions. The important change is
that unsafe code would be coherent backend implementation rather than a
Vulkano/raw interop seam.

## Dependency Weight

`cargo tree -p qgs-vulkan` shows Vulkano brings, among others:

- `ash`
- `bytemuck` and derive dependencies
- `crossbeam-queue`
- `foldhash`
- `half`
- `once_cell`
- `parking_lot`
- `raw-window-handle`
- `slabbin`
- `smallvec`
- `thread_local`
- build dependencies including `vk-parse`, `serde`, `serde_json`, `indexmap`,
  `nom`, `heck`

`ash` itself brings `libloading`.

Debug build artifacts on the current machine show large Vulkano rlibs/rmetas,
but those numbers are not a controlled benchmark because multiple build hashes
exist in `target/debug/deps`. The useful architectural fact is simpler:
Vulkano is a broad Vulkan abstraction stack while QGS needs a narrow,
video-oriented Vulkan subset and already depends directly on `ash`.

Dependency count is not the main reason to migrate. Control over external
memory/image/synchronization semantics is.

## Actual QGS Vulkan Subset

Current required subset:

- instance creation
- physical-device enumeration and properties
- logical-device creation with selected extensions
- one queue family and queue
- memory properties
- host-visible buffers
- device-preferred buffers
- exportable buffers
- exportable RGBA images
- imported buffers
- imported RGBA images
- device memory export/import FD
- buffer/image memory binding
- memory mapping for validation/readback
- command pool
- primary command buffer
- fill buffer
- copy buffer
- clear color image
- copy image to buffer
- fixed compute shader modules
- descriptor set layout/pool/set for one storage buffer or image
- compute pipeline
- dispatch
- binary semaphore
- sync FD export/import
- fence wait for validation

Near-term professional-video subset:

- software-frame upload buffers
- 10-bit 4:2:2 planar image/buffer upload representation
- NV12 and P010-class multi-planar images
- sampled YUV image access
- storage/output RGBA or higher precision intermediate images
- fixed color conversion/scaling/compositing compute pipelines
- DMA-BUF import/export
- DRM modifier image creation
- disjoint images and per-plane memory binding
- `vkGetMemoryFdPropertiesKHR`
- external/foreign queue-family ownership
- explicit image layout transitions preserving external contents
- external semaphore/sync FD integration
- modern Intel/AMD/NVIDIA capability variation

Vulkan features QGS does not currently need:

- swapchains and presentation
- graphics render passes/framebuffers for UI
- rasterization pipeline state
- dynamic rendering for on-screen output
- tessellation/geometry shaders
- ray tracing
- mesh shaders
- general-purpose user shader API
- complex scene/resource graph
- timeline scheduling engine
- descriptor indexing unless future shader set needs it

## Video Workload Fit

Vulkano helps with ordinary Vulkan chores: device wrappers, allocator,
command-builder ergonomics, descriptor set creation, and pipeline creation.
Those are useful for small demos.

QGS's workload is not ordinary game rendering. It is imported and processed
professional media. The hard correctness points are:

- preserving externally produced image contents;
- representing DRM modifier images exactly;
- validating modifier-specific features, not generic format features;
- distinguishing DRM objects, video format planes, and Vulkan modifier memory
  planes;
- importing borrowed/owned FDs with correct ownership;
- acquiring external/foreign queue-family ownership;
- avoiding implicit first-use image-state transitions from `Undefined`;
- supporting future multi-plane/disjoint binding;
- making synchronization domains explicit.

Step 4 proved that these semantics cannot be treated as edge details. The
Haswell path froze specifically because the safe Vulkano image-state path was
not adequate to prove zero-copy video correctness.

For QGS, Vulkano is now helpful for the easy half and obstructive or bypassed
for the hard half.

## Minimal QGS Vulkan API If Starting Today

The public API outside `qgs-vulkan` should remain QGS-owned:

- `DeviceDiscovery`
- `ResourceBackend`
- `SyncBackend`
- capability structs
- QGS resource/sync/video metadata

Inside `qgs-vulkan`, a direct backend should expose safe internal wrappers:

- `QgsGpuInstance`
- `QgsGpuDevice`
- `QgsGpuQueue`
- `QgsGpuMemory`
- `QgsGpuBuffer`
- `QgsGpuImage`
- `QgsGpuImageView`
- `QgsCommandPool`
- `QgsCommandBuffer`
- `QgsFence`
- `QgsSemaphore`
- `QgsDescriptorPool`
- `QgsComputePipeline`
- `ImportedDmaBufMemory`
- `ImportedVideoImage`
- `ExternalSyncFd`

These names are illustrative. The key is that they should model QGS work, not
mirror every Vulkan object for its own sake.

## RAII Model

Recommended ownership:

| Wrapper | Owner/dependencies | Drop behavior |
| --- | --- | --- |
| `QgsGpuInstance` | Owns `ash::Entry`, `ash::Instance`; optional debug utils in diagnostics | destroys debug messenger before instance |
| `QgsGpuDevice` | `Arc<QgsGpuInstance>`, physical device handle, `ash::Device`, enabled extensions, memory properties | waits idle or requires all children dropped, then destroys device |
| `QgsGpuQueue` | `Arc<QgsGpuDevice>`, queue handle, family index | no Vulkan destroy; lifetime tied to device |
| `QgsGpuMemory` | `Arc<QgsGpuDevice>`, `vk::DeviceMemory`, allocation size/type, import/export ownership state | unmaps if mapped; frees memory |
| `QgsGpuBuffer` | `Arc<QgsGpuDevice>`, `vk::Buffer`, optional bound `Arc<QgsGpuMemory>` | destroys buffer after uses complete; memory owned separately or by aggregate |
| `QgsGpuImage` | `Arc<QgsGpuDevice>`, `vk::Image`, format/extent/usage/tiling/modifier metadata, bound memory list | destroys image after views; memory owned separately or by aggregate |
| `QgsGpuImageView` | `Arc<QgsGpuImage>` or device+image handle | destroys image view |
| `QgsCommandPool` | queue-family-specific, device-owned | destroys pool and command buffers |
| `QgsCommandBuffer` | command pool allocation token | freed by pool or explicit free/reset |
| `QgsFence` | device-owned | destroys fence |
| `QgsSemaphore` | device-owned, external handle capabilities | destroys semaphore |
| `QgsDescriptorPool` | device-owned | destroys pool |
| `QgsComputePipeline` | shader modules, descriptor set layout, pipeline layout, pipeline | destroys pipeline/layouts/modules in dependency order |

Construction/destruction/submit/map/import/export functions remain unsafe
internally, but all QGS-facing APIs stay safe.

## Error Model

Direct Vulkan errors should translate as:

```text
vk::Result
    ->
qgs_vulkan::BackendError
    ->
qgs-core ResourceError / SyncError / DeviceDiscoveryError
    ->
stable qgs-protocol errors where applicable
```

Raw `vk::Result` must not escape `qgs-core` or `qgs-protocol`. Diagnostics may
log raw Vulkan result names as backend diagnostics.

## Ash Dependency

`qgs-vulkan` already directly depends on `ash 0.38.0+1.3.281`. Vulkano 0.35.2
also depends on ash 0.38.0.

If QGS migrates, keep `ash` as an explicit direct dependency. Do not rely on a
transitive dependency. The current version is already compatible with the
workspace and Vulkan loader environment demonstrated by the Step 4B diagnostic.

No dependency change is made by this audit.

## Shader Strategy

Current shaders are embedded SPIR-V word arrays. Vulkano reflection is used to
derive pipeline layout information.

Future model:

```text
QGS-owned shader source
    ->
build-time SPIR-V generation/validation or checked-in SPIR-V
    ->
embedded SPIR-V words
    ->
fixed descriptor-layout declarations in qgs-vulkan
    ->
VkShaderModule
```

QGS should not expose arbitrary client shaders. It needs fixed internal
processing shaders for upload conversion, YUV sampling/conversion, scaling,
compositing, and validation/proof operations.

The useful part of Vulkano here is reflection convenience. It is not enough to
justify keeping Vulkano if the rest of the video image model must be raw.

## Memory Allocation

QGS does not yet need a general-purpose Vulkan allocator comparable to a game
engine. Near-term needs:

- small number of persistent device buffers/images;
- upload/readback buffers;
- exportable allocations that should often be dedicated;
- imported DMA-BUF allocations that are never pooled;
- transient processing images/buffers.

Recommended first direct allocator:

- enumerate memory types once per device;
- helper to select memory type by required/preferred flags and memory type bits;
- explicit dedicated allocations for exportable/imported resources;
- simple host-visible allocation helpers for upload/readback;
- defer suballocation/pooling until profiling or resource count justifies it.

A dedicated Vulkan allocator dependency can be reconsidered after QGS has real
video-processing workloads, but importing/exporting external memory will still
need explicit paths.

## Migration Parity Targets

Required production parity before removing Vulkano:

- physical-device enumeration
- `DeviceDesc` mapping
- capability discovery
- memory heap/type reporting
- compute capability reporting
- interop capability reporting
- buffer creation
- memory preference behavior for current tests
- resource cleanup
- external memory export
- DMA-BUF buffer sharing
- external memory import
- external sync FD export/import
- producer GPU fill and consumer GPU wait/copy proof
- compute increment proof
- RGBA image allocation/export/import/invert proof
- qgs-test hardware paths that currently pass
- all unit tests

Diagnostic/debt parity:

- preserve Step 4/4A/4B findings in documentation;
- do not necessarily port the exact Haswell diagnostic unchanged;
- rebuild useful modifier/memory-plane query tools on top of the new wrappers;
- keep the rule: DRM object count != video format plane count != Vulkan DRM
  modifier memory-plane count.

## Haswell Diagnostic Debt

Recommended handling:

- Do not preserve `haswell_video_diagnostic.rs` as production architecture.
- During migration, either keep it temporarily behind the old module or rewrite
  its useful queries on top of the direct backend.
- Archive/remove crashing or non-actionable experimental paths.
- Preserve the reports and tests that prevent future code from assuming one
  DRM object, one format plane, or one Vulkan memory plane.

Haswell remains a compatibility/fallback target, not the driver on which to
force zero-copy video success.

## Migration Size Estimate

Concrete repository estimate:

- Files directly affected: `crates/qgs-vulkan/src/*.rs`,
  `crates/qgs-vulkan/Cargo.toml`, possibly docs/tests/qgs-test only as
  integration expectations change.
- Existing Vulkano-specific production LOC: most of `src/lib.rs` plus
  `external_memory.rs`, `external_sync.rs`, and `external_compute.rs`.
- Existing raw ash LOC retained conceptually: parts of
  `haswell_video_diagnostic.rs`, but likely rewritten.
- Expected new direct wrapper LOC: roughly 2500-4500 LOC for initial parity,
  depending on how much diagnostic tooling is rebuilt immediately.
- Tests affected: qgs-vulkan unit tests, qgs-test M1/M2 GPU proofs, hardware
  acceptance scripts/commands.

No calendar estimate is given.

## Migration Plan

Recommended incremental stages:

1. Freeze current Vulkano behavior at the Step 9 commit.
2. Add a private `ash_backend` module behind the existing `qgs-vulkan` public
   traits.
3. Port instance/device enumeration and `DeviceDesc`/capability reporting.
4. Port memory-type selection, host-visible buffers, and buffer mapping.
5. Port command pool/buffer, queue submit, fences, and basic copy/fill commands.
6. Port buffer external memory export/import and M1 shared-buffer proof.
7. Port external semaphore/sync FD export/import and Step 7 synchronization
   proof.
8. Port fixed shader module, descriptor set, compute pipeline, and Step 8
   compute proof.
9. Port RGBA image creation/export/import/invert proof.
10. Rebuild DRM modifier/multi-planar query helpers on the new wrappers.
11. Remove or archive old Haswell diagnostic code that is superseded.
12. Remove Vulkano dependency.
13. Run full cargo gates and hardware regression on Intel/NVIDIA/llvmpipe
    where applicable.

At each stage, keep the existing public `qgs-vulkan` traits stable so the rest
of QGS does not see two permanent GPU backends.

## Rollback Strategy

The pushed Step 9 state is the known-good baseline. Migration should happen in
ordinary commits that preserve build/test pass points where practical.

Recommended validation:

- compare device enumeration output before/after;
- compare exported metadata for buffers/images;
- compare qgs-test proof outputs;
- keep the old branch reachable through git history;
- avoid maintaining two permanent Vulkan backends.

A temporary internal module split during migration is acceptable. The final
goal is one `qgs-vulkan` implementation.

## Decision Criteria

| Criterion | Vulkano hybrid | Direct ash backend |
| --- | --- | --- |
| External-memory control | Partly good, but import/query escape hatches remain | Strong, explicit |
| DRM modifier control | Insufficient for production video semantics | Strong, if wrappers are designed carefully |
| Multi-planar video support | Awkward and already blocked/frozen | Stronger fit |
| Synchronization control | Works for simple sync FD with unsafe wrappers | Strong, explicit |
| Safety reviewability | Split between Vulkano state and raw escapes | Better if wrappers are narrow and documented |
| Maintenance burden | Lower short-term, higher hybrid complexity | Higher migration cost, lower long-term ambiguity |
| Dependency burden | Larger abstraction stack | Smaller direct stack |
| Portability | Good for ordinary Vulkan; unclear for video interop | Depends on QGS validation, but exposes actual driver behavior |
| Future Intel/AMD/NVIDIA work | Vulkano may hide required edge semantics | Better diagnostic and control surface |
| Implementation complexity | Lower now | Higher now |
| Migration cost | None now | Real, but bounded by small current subset |
| Long-term QGS fit | Poor for professional video interop | Better |

## Why Now

Waiting until after software VideoSurface upload, color conversion, modern
zero-copy validation, HEVC, or encode would build more code on the part of the
stack already known to be least aligned with QGS's requirements.

The current Vulkan subset is still small. The next milestone will create the
first substantial CPU-decoded-frame-to-GPU path, including 10-bit 4:2:2 upload
and processing choices. That is exactly the point at which QGS should own its
image, memory, layout, and synchronization model directly.

Migrating later would require either:

- carrying both Vulkano and raw ash for more features, making safety review
  harder; or
- rewriting a larger video-processing stack after it already exists.

## Recommendation

MIGRATE QGS-VULKAN TO DIRECT ASH NOW

# QGS M2 Step 4 Vulkan Interop Audit

## Scope

This document audits the Vulkan interoperability boundary needed for:

```text
VA-API decoded NV12 VideoSurface
    -> DRM PRIME / DMA-BUF descriptor
    -> Vulkan import
    -> GPU processing
```

It is documentation only. It does not approve implementation and does not
modify the QGS safety inventory.

## Root Cause Of The Vulkano Blocker

Step 3B proved that Intel i965 can decode the synthetic H.264 fixture into a
real VA NV12 surface and export it as a DRM PRIME descriptor. Step 4 needs
Vulkan to import that same underlying allocation and preserve the decoded
pixels.

Vulkano 0.35.2 can express many pieces of this path, but its safe image-state
model is not sufficient for imported foreign image contents:

- Vulkan images created for external memory must use `initialLayout =
  VK_IMAGE_LAYOUT_UNDEFINED`.
- Vulkano tracks the first use of such an image as starting from
  `Undefined`.
- A normal transition from `Undefined` to a usable layout may discard previous
  image contents.
- The decoded pixels were produced outside Vulkan, so treating the first use as
  an ordinary undefined-layout transition cannot prove preservation.

The blocker is semantic, not just ergonomic. A proof that begins with
`Undefined -> General` or `Undefined -> ShaderReadOnlyOptimal` through
Vulkano's normal state path could legally read undefined data.

Relevant local source observations:

- `vulkano::image::sys::ImageCreateInfo` validation rejects external-memory
  images whose `initial_layout` is not `ImageLayout::Undefined`.
- `AutoCommandBufferBuilder` initializes first-use image state from
  `image.initial_layout()`, then inserts a first-use barrier.
- Vulkano does not expose a safe API to tell the command-buffer state machine
  that a foreign producer already owns valid contents in an externally defined
  layout.

## Relevant Vulkan External Image Rules

The Vulkan specification separates several concerns that QGS must keep
separate:

- image creation parameters
- memory import
- memory type compatibility
- queue-family ownership
- image layout
- execution completion and memory visibility

For DRM modifier images, `VK_EXT_image_drm_format_modifier` explicitly
describes importing Linux images with a dma-buf plus modifier metadata using
`VkImportMemoryFdInfoKHR` and
`VkImageDrmFormatModifierExplicitCreateInfoEXT`. The extension also states
that DRM formats must be translated to Vulkan formats by the application.
QGS must therefore map DRM `NV12` to Vulkan
`VK_FORMAT_G8_B8R8_2PLANE_420_UNORM` inside `qgs-vulkan`.

For POSIX FD memory handles, the Vulkan memory chapter defines
`vkGetMemoryFdPropertiesKHR` for handles created outside Vulkan. Its result is
the `memoryTypeBits` mask that constrains the later `VkMemoryAllocateInfo`.
This is required for VA/DRM PRIME imports because qgs-vulkan did not allocate
the memory and cannot reuse its own allocator metadata.

For synchronization and ownership, Vulkan queue-family ownership transfer rules
allow special external queue families. `VK_QUEUE_FAMILY_EXTERNAL` represents
external queues compatible with the same physical device/driver class, while
`VK_QUEUE_FAMILY_FOREIGN_EXT` can represent foreign non-Vulkan producers such
as non-Vulkan-capable devices or other vendors. Queue-family ownership transfer
is defined through image memory barriers whose queue family indices differ.

Specification references:

- Vulkan resources, external image layout rules:
  <https://docs.vulkan.org/spec/latest/chapters/resources.html#resources-image-layouts-external>
- Vulkan memory, `vkGetMemoryFdPropertiesKHR`:
  <https://docs.vulkan.org/spec/latest/chapters/memory.html#vkGetMemoryFdPropertiesKHR>
- Vulkan memory, FD import ownership:
  <https://docs.vulkan.org/spec/latest/chapters/memory.html#memory-import-fd>
- Vulkan synchronization, queue-family ownership transfer:
  <https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-queue-transfers>
- `VK_EXT_image_drm_format_modifier`:
  <https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_image_drm_format_modifier.html>
- `VK_EXT_queue_family_foreign`:
  <https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_queue_family_foreign.html>

## Correct Preservation Semantics For Externally Produced Contents

For a VA-produced image, the initial valid contents are not established by
Vulkan image creation. They are established by an external producer. Vulkan must
therefore acquire ownership and visibility from that producer before reading
the image.

The correct model is:

1. VA completes all decode writes to the surface.
2. QGS exports DRM PRIME metadata and FD object references.
3. Vulkan creates an image matching the exported metadata, with external memory
   and DRM modifier structures.
4. Vulkan imports/binds the DMA-BUF memory objects.
5. Vulkan records an acquire barrier from the appropriate external queue family
   to the Vulkan queue family.
6. That acquire barrier uses an old layout that represents the externally
   valid contents, not a content-discarding "do not care" transition.
7. Subsequent Vulkan shader sampling or transfer reads operate on preserved VA
   decode output.

`PREINITIALIZED` is not the right general answer. Vulkan external-memory image
creation requires `initialLayout = UNDEFINED` when
`VkExternalMemoryImageCreateInfo` is used. `PREINITIALIZED` is also limited in
what it can represent and is not a safe way to describe decoded image contents
written by a VA engine.

`GENERAL` may be the eventual layout used by Vulkan processing, but QGS must
not get there by a plain discardable `UNDEFINED -> GENERAL` transition.

The key operation is an external or foreign queue-family acquire barrier that
preserves contents. For VA surfaces the likely source queue family is
`VK_QUEUE_FAMILY_FOREIGN_EXT`, because VA-API is not a Vulkan queue and may not
be the same driver abstraction. QGS should prefer `FOREIGN_EXT` when the device
supports `VK_EXT_queue_family_foreign`; otherwise it must evaluate whether
`VK_QUEUE_FAMILY_EXTERNAL` is valid for the actual VA/Vulkan driver pairing.
This must be tested on Intel i965 before claiming success.

If the acquire path cannot be expressed, QGS must continue to treat Step 4 as
blocked.

## DRM Modifier Image Creation Requirements

The VA DRM PRIME descriptor must be represented without hardcoding Intel's
current layout. QGS metadata must carry:

- DRM fourcc
- coded width and height
- object list, with one FD per object attachment
- object size
- object modifier
- layer list
- layer DRM format
- plane list per layer
- plane object index
- plane offset
- plane pitch

For the current Intel i965 Step 3B surface:

- DRM fourcc: `NV12` / `0x3231564e`
- size: `64 x 64`
- objects: `1`
- layers: `1`
- object size: `12288`
- modifier: `72057594037927938`
- Y plane pitch/offset: `128 / 0`
- UV plane pitch/offset: `128 / 8192`

This maps semantically to:

- Vulkan format: `VK_FORMAT_G8_B8R8_2PLANE_420_UNORM`
- Vulkan tiling: `VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT`
- Vulkan external handle type: `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT`
- explicit modifier creation:
  `VkImageDrmFormatModifierExplicitCreateInfoEXT`

Important detail: the DRM PRIME layer's image planes are not necessarily the
same thing as Vulkan DRM modifier memory planes. Vulkan's DRM modifier
properties report the number of memory planes for a modifier. For the current
Intel surface, one object may back two format planes. Other drivers may export
multiple objects or require disjoint binding.

The importer must:

1. map DRM fourcc/layer format to the QGS-supported Vulkan format;
2. query format modifier properties for the exact modifier;
3. validate that the modifier supports the required usage;
4. construct explicit plane layouts with offset and row pitch, using
   `size = 0` as required by the DRM modifier extension discussion;
5. create the `VkImage` with external memory and explicit modifier structures;
6. create one or more `VkDeviceMemory` objects from the received FDs;
7. bind memory normally or per memory plane as required by the modifier plane
   count and `VK_IMAGE_CREATE_DISJOINT_BIT`.

The design must not assume:

- linear tiling
- `pitch == width`
- `offset == width * height`
- one object
- one object per image plane
- Intel's current modifier

## Memory FD Query Ownership Analysis

Vulkan's `vkGetMemoryFdPropertiesKHR` takes an integer FD and returns
properties. It does not import memory and does not define transfer of FD
ownership. The FD remains owned by the caller after the query.

By contrast, `VkImportMemoryFdInfoKHR` import transfers ownership of the FD to
the Vulkan implementation on successful import. After successful import, QGS
must not use or close that FD; the imported `VkDeviceMemory` owns the payload
reference.

Vulkano 0.35.2 exposes:

```rust
unsafe fn Device::memory_fd_properties(
    &self,
    handle_type: ExternalMemoryHandleType,
    file: File,
) -> Result<MemoryFdProperties, Validated<VulkanError>>
```

The wrapper consumes `File` and internally calls `into_raw_fd()` before
`vkGetMemoryFdPropertiesKHR`. Since the Vulkan query does not take ownership,
this is unsuitable for QGS if used directly: it intentionally leaks ownership
from Rust's RAII model unless the implementation closes the raw FD elsewhere,
which it does not.

Options:

- Duplicate the FD before calling Vulkano's wrapper. This avoids losing the
  original import FD, but the duplicate consumed by Vulkano is still converted
  to a raw FD for a non-consuming query. That leaks the duplicate unless
  Vulkano later changes behavior. QGS should not rely on this.
- Add a narrowly audited qgs-vulkan wrapper around
  `vkGetMemoryFdPropertiesKHR` that borrows an `AsFd`, calls the raw function,
  and does not consume ownership. This is the cleanest approach.
- Find another Vulkano safe path. None was found in 0.35.2.

Recommendation: implement a private unsafe boundary for borrowed FD memory
property queries.

## Vulkano Safe API Capability Matrix

| Operation | Vulkano safe API? | Correct for imported VA contents? | Custom boundary required? |
| --- | --- | --- | --- |
| DRM modifier format property query | Yes: `PhysicalDevice::format_properties` and `image_format_properties` | Yes, for capability validation | No |
| DRM modifier image creation | Mostly: `RawImage::new` with DRM modifier fields | Structurally yes, but not sufficient for content ownership/layout | Possibly no for creation itself |
| Explicit DRM plane layouts | Yes: `drm_format_modifier_plane_layouts` | Yes if metadata is validated correctly | No for creation |
| External memory import | Unsafe Vulkano API already wrapped by QGS for M1 | Correct when allocation size, memory type, and FD ownership are correct | Existing boundary can be reused |
| Memory FD property query | Unsafe Vulkano API consumes `File` | Not acceptable for QGS ownership | Yes |
| Memory binding | Yes: `RawImage::bind_memory` | Yes for normal/disjoint binding if memory plane count matches | No unless Vulkano validation blocks a valid case |
| Initial image state | Vulkano internal safe state model only | No; first use from `Undefined` can discard VA contents | Yes |
| External/foreign ownership acquisition | Not safely expressible for this imported-content case through Vulkano image state | Required | Yes |
| Layout transition preserving contents | Not safely expressible because Vulkano owns first-use layout state | Required | Yes |
| Multi-planar image views | Yes | Yes for compatible views | No |
| YCbCr sampling | Yes with sampler conversion and immutable samplers | Likely yes after correct import/acquire | No |
| Storage image access to NV12 | Driver/format dependent | Not required for first proof if sampling path works | Avoid initially |

## Proposed Minimal qgs-vulkan Interop Layer

Do not replace Vulkano. Add a private, narrowly audited interop layer for the
parts Vulkano cannot safely model:

```text
crates/qgs-vulkan/src/interop/
    drm_prime_image.rs
    fd_properties.rs
    external_acquire.rs
```

The public qgs-vulkan API remains safe and QGS-owned:

```rust
pub fn import_and_process_video_surface(
    source_device: &DeviceDesc,
    metadata: &ExportedVideoSurfaceMetadata,
    handles: Vec<File>,
) -> Result<VideoInteropProofResult, ResourceError>
```

No raw Vulkan handles, VA handles, DRM structs, or FDs as integers escape
`qgs-vulkan`.

The interop layer should:

1. validate QGS metadata bounds and object/plane relationships;
2. match the Vulkan device to the VA render-node/PCI identity;
3. query DRM modifier support and memory plane count;
4. query memory FD properties without consuming FD ownership;
5. create the DRM modifier image;
6. import DMA-BUF FDs into `VkDeviceMemory`;
7. bind image memory;
8. record a raw acquire barrier from `FOREIGN_EXT` or `EXTERNAL` to the Vulkan
   queue family, preserving contents;
9. return an object that can be safely sampled or copied by existing Vulkano
   command code after the acquire has established a known image state.

The first proof should prefer sampling NV12 through a fixed private shader with
`VK_KHR_sampler_ycbcr_conversion`, because it resembles the future video
pipeline. A fallback proof may copy/read selected planes only if it still uses
the imported image allocation directly and does not CPU-copy VA pixels into a
normal Vulkan image.

## Proposed Unsafe Functions And Boundaries

### `query_dma_buf_memory_type_bits`

Vulkan API:

- `vkGetMemoryFdPropertiesKHR`

Rust inputs:

- Vulkano `Device`
- `BorrowedFd`
- `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT`

Preconditions:

- logical device enabled `VK_KHR_external_memory_fd`;
- FD is a valid DMA-BUF memory handle;
- FD remains open for the duration of the call;
- FD is not consumed by the query;
- caller owns or borrows the FD legally.

Postconditions:

- returns `memoryTypeBits`;
- FD ownership remains unchanged.

Unsafe reason:

- raw Vulkan call through function table and raw FD integer.

Safe wrapper validation:

- handle type is DMA-BUF;
- extension is enabled;
- FD is borrowed, not moved;
- result is checked and mapped to QGS error.

### `create_drm_modifier_image`

Vulkan API:

- `vkCreateImage`

Structures:

- `VkImageCreateInfo`
- `VkExternalMemoryImageCreateInfo`
- `VkImageDrmFormatModifierExplicitCreateInfoEXT`

Preconditions:

- width/height/format are validated QGS values;
- DRM modifier is supported for the Vulkan format and usage;
- plane layouts come from the VA-exported descriptor and pass protocol bounds;
- `pPlaneLayouts[].size == 0`;
- image usage is limited to the proof path;
- external handle type is DMA-BUF;
- initial layout is `VK_IMAGE_LAYOUT_UNDEFINED` as required by external memory
  image creation.

Postconditions:

- returns an owned `VkImage` wrapper internal to qgs-vulkan.

Unsafe reason:

- manual pNext chain and raw image handle lifetime.

Safe wrapper validation:

- validates all count bounds and object/plane indices;
- validates exact modifier support before create;
- destroys image on failure paths.

### `import_dma_buf_memory`

Vulkan API:

- `vkAllocateMemory` with `VkImportMemoryFdInfoKHR`

Preconditions:

- allocation size is sufficient for Vulkan image memory requirements;
- memory type index is selected from the intersection of image requirements and
  `vkGetMemoryFdPropertiesKHR` bits;
- FD is owned by QGS and intended to be consumed;
- if import succeeds, QGS must not use or close that FD.

Postconditions:

- success transfers FD ownership to Vulkan;
- failure leaves QGS responsible for closing the FD.

Unsafe reason:

- raw FD ownership transfer and pNext chain.

Safe wrapper validation:

- import uses `OwnedFd`/`File` move semantics;
- result handling closes FDs only on failure;
- imported memory is destroyed when wrapper drops.

This may reuse the existing QGS `DeviceMemory::import` unsafe wrapper for the
actual import, provided memory type selection is fixed by the new borrowed FD
query.

### `bind_drm_modifier_image_memory`

Vulkan API:

- `vkBindImageMemory` or `vkBindImageMemory2`

Preconditions:

- number of memory bindings matches DRM modifier memory plane count;
- non-disjoint image receives one binding;
- disjoint image receives one binding per memory plane;
- memory/device/image all belong to the same Vulkan logical device;
- offsets and sizes satisfy Vulkan requirements.

Postconditions:

- image is backed by imported DMA-BUF memory.

Unsafe reason:

- may require raw binding if Vulkano cannot bind a valid multi-object
  descriptor shape.

Safe wrapper validation:

- prefer Vulkano `RawImage::bind_memory` where it accepts the shape;
- use raw bind only for cases Vulkano cannot express but Vulkan allows;
- map validation failures to QGS errors.

### `acquire_foreign_image_contents`

Vulkan API:

- `vkCmdPipelineBarrier2` or `vkCmdPipelineBarrier`

Structures:

- `VkImageMemoryBarrier2` or `VkImageMemoryBarrier`

Preconditions:

- VA decode completion has been established before export/import;
- image memory is bound;
- queue family source is selected as `VK_QUEUE_FAMILY_FOREIGN_EXT` when
  supported and applicable, otherwise `VK_QUEUE_FAMILY_EXTERNAL` only if valid
  for the VA/Vulkan pairing;
- destination queue family is the selected Vulkan queue family;
- old layout is the external producer layout determined by the interop design;
- new layout is the layout required by the proof operation;
- subresource range covers the imported VideoSurface planes.

Postconditions:

- Vulkan queue owns the image for the selected subresource range;
- decoded contents are available and visible for Vulkan reads.

Unsafe reason:

- bypasses Vulkano's image-state model to express a foreign acquire that safe
  Vulkano cannot represent.

Safe wrapper validation:

- command buffer is internal to qgs-vulkan;
- acquire command is recorded before any read;
- image wrapper records that acquire has occurred;
- no public API can access the image before acquire.

## VA -> Vulkan Synchronization Model

`vaSyncSurface` establishes VA decode execution completion for the surface. It
is a CPU wait. It does not by itself describe Vulkan queue-family ownership or
Vulkan image layout.

The full handoff model is:

```text
VA decode submission
    -> vaSyncSurface
        execution completion in VA domain
    -> vaExportSurfaceHandle
        native metadata and DMA-BUF object references
    -> Vulkan image/memory import
        object identity and memory binding
    -> external/foreign queue-family acquire
        Vulkan ownership and visibility
    -> Vulkan shader/transfer work
```

This is correct for the first proof but not the final asynchronous media
pipeline. Future work should replace the CPU wait with explicit synchronization
where the VA stack and driver expose a suitable sync object. QGS must not claim
that `vaSyncSurface` is an external GPU semaphore.

## Intel Current-Surface Mapping

The current Intel i965 descriptor is compatible with the proposed model:

- one DMA-BUF object FD;
- one DRM PRIME layer;
- two NV12 format planes within that layer;
- non-linear DRM modifier;
- pitches and offsets supplied explicitly by VA;
- object size larger than the visible pixel payload.

For import:

- metadata maps to one QGS VideoSurface export payload plus one SCM_RIGHTS FD;
- Vulkan format is `VK_FORMAT_G8_B8R8_2PLANE_420_UNORM`;
- explicit layouts use the VA pitch/offset values;
- image usage should be minimal, likely sampled/read-only for proof;
- memory import should use a memory type from
  `vkGetMemoryFdPropertiesKHR(fd) & image.memoryTypeBits`.

The implementation must still confirm at runtime that the Intel Vulkan driver
accepts the exact modifier/usage combination and the foreign/external acquire
barrier.

## Multi-Object And Multi-Plane Portability Model

Other systems may expose:

- multiple DMA-BUF objects;
- one object per memory plane;
- one object backing several image planes;
- different modifiers per object;
- different layer counts;
- different pitches and offsets;
- disjoint memory binding requirements.

QGS should bound and encode up to four objects, four layers, and four planes
per layer for M2. That is enough for common DRM PRIME descriptors without
opening an unbounded attack surface.

The importer must treat object count, layer count, and Vulkan modifier memory
plane count as related but distinct concepts:

- DRM PRIME object: native FD-backed memory object.
- DRM PRIME layer plane: media/image plane layout metadata.
- Vulkan DRM modifier memory plane: Vulkan binding unit reported by modifier
  properties.

The interop layer must build the Vulkan binding list from Vulkan's memory plane
requirements and the VA object/plane mapping, not from assumptions about NV12.

## Dependency Implications

No new Vulkan loader dependency is needed for the audit recommendation.
Vulkano already depends on `ash 0.38`, and Vulkano exposes device function
tables and raw handles internally enough for narrow boundary code.

Implementation may need to add a direct `ash` dependency to `qgs-vulkan` so QGS
can name Vulkan structs/functions deliberately rather than relying on
transitive dependency availability. That should be treated as a compile-time
dependency clarification, not a replacement of Vulkano.

No dependencies should be added to `qgs-protocol`, `qgs-core`, `qgs-linux`,
`qgsd`, or `qgs-test` for this boundary.

## Recommendation

Implement a narrow audited raw Vulkan interop boundary inside `qgs-vulkan`.

Do not upgrade or replace Vulkano yet.

Reasoning:

- Vulkano 0.35.2 already handles most ordinary device, queue, memory, shader,
  descriptor, and sampler work used by QGS.
- The missing parts are specific to imported foreign image contents and FD
  property ownership.
- A small raw boundary can preserve the current architecture and safety policy
  while accurately expressing Vulkan semantics.
- Upgrading/replacing Vulkano before isolating the exact boundary risks a broad
  dependency churn without proving that the semantic issue is solved.

This recommendation should be reviewed before implementation because it adds
new unsafe qgs-vulkan responsibilities.

## Exact Implementation Plan For Resuming M2 Step 4

1. Add QGS protocol metadata for VideoSurface DRM PRIME export:
   objects, layers, planes, modifiers, pitches, offsets, and expected FD count.
2. Generalize `qgs-linux` SCM_RIGHTS attachments from one FD to a bounded
   count, initially four.
3. Promote `qgs-vaapi` DRM PRIME export probe to a real `BackendResource`
   export path for `ResourceKind::VideoSurface`.
4. Add `qgs-vulkan::interop::fd_properties` with a borrowed-FD
   `vkGetMemoryFdPropertiesKHR` wrapper.
5. Add `qgs-vulkan::interop::drm_prime_image` to validate metadata, create the
   explicit DRM modifier image, import/bind DMA-BUF memory, and own cleanup.
6. Add `qgs-vulkan::interop::external_acquire` to record and submit the
   foreign/external ownership acquire barrier before any Vulkan read.
7. Build the first proof operation as a fixed private NV12 read path:
   preferably YCbCr sampler to small RGBA/readback output; otherwise selected
   deterministic sample extraction, as long as Vulkan reads the imported image
   allocation directly.
8. Keep the validation readback after Vulkan processing only.
9. Update `docs/safety.md` with the new unsafe inventory only after
   implementation is approved.
10. Run protocol/unit tests, qgs-linux FD tests, full workspace gates, and the
    real Intel H.264 decode -> VA export -> Vulkan import -> GPU processing
    proof.

## Architectural Concerns

This boundary is deliberately backend-specific. `qgs-protocol` should describe
bounded VideoSurface export metadata and attachment counts, but it must not
expose Vulkan structures, VA structures, or raw native FD integers.

`qgs-core` should continue to own session/resource lifecycle and authorization,
but it should not interpret DRM modifiers.

`qgs-linux` should remain only a transport attachment layer.

All Vulkan-specific interpretation of DRM PRIME metadata belongs in
`qgs-vulkan`.

The implementation must stop again if Intel i965 cannot perform the required
foreign/external acquire semantics or if the Vulkan driver rejects the actual
modifier/usage combination.

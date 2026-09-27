# QGS M1 Step 9 Report

## Image Resource Model

Step 9 extends session-owned `ResourceId` resources from Buffer-only to:

- `Buffer`
- `Image`

Images use the same qgs-core session resource registry as buffers. A QGS image
is owned by exactly one session, cannot be used across sessions, and is released
on explicit `DESTROY_RESOURCE` or session disconnect.

## PixelFormat Model

Step 9 implements one QGS-owned format:

- `PixelFormat::Rgba8Unorm`

No YUV, NV12, P010, multiplanar, video, color-management, presentation, or
format-negotiation model was added.

## ImageDesc

`ImageDesc` contains:

- `DeviceId`
- `width`
- `height`
- `PixelFormat`
- `ImageUsageFlags`
- `ExternalSharing`

Implemented image usage flags are:

- `TRANSFER_SRC`
- `TRANSFER_DST`
- `STORAGE`

## Image Dimension Limits

M1 image limits are:

- maximum width: 8192
- maximum height: 8192
- width and height must be non-zero
- computed RGBA byte size is checked for overflow

These are M1 safety limits, not final product limits.

## Protocol Additions

New request:

- `CREATE_IMAGE`: request kind `1`, opcode `9`

New response:

- `IMAGE_CREATED`: response kind `2`, opcode `10`

`RESOURCE_EXPORTED` was generalized with a resource-kind discriminator so the
existing `EXPORT_RESOURCE` path can export both buffers and images.

New stable errors:

- `InvalidImageDimensions`: `35`
- `UnsupportedPixelFormat`: `36`
- `UnsupportedImageUsage`: `37`
- `UnsupportedImageExternalSharing`: `38`

## Vulkan Image Format And Tiling

QGS `PixelFormat::Rgba8Unorm` maps inside `qgs-vulkan` to:

- Vulkan format: `VK_FORMAT_R8G8B8A8_UNORM`
- Vulkan tiling for exportable Step 9 DMA-BUF images: linear tiling

Vulkan image tiling, layouts, and import metadata remain backend details and do
not become public QGS application semantics.

## External-Memory Image Support Validation

For Step 9, qgs-vulkan validates resource-specific external image support for:

- `Rgba8Unorm`
- 2D image
- linear tiling
- `TRANSFER_SRC | TRANSFER_DST | STORAGE`
- DMA-BUF external memory

The implementation does not assume that buffer DMA-BUF support implies image
DMA-BUF support.

## DMA-BUF Export/Import Path

The successful path is:

1. qgs-test requests `CREATE_IMAGE` with DMA-BUF external sharing required.
2. qgsd allocates a real Vulkan Image through qgs-vulkan.
3. qgs-test requests `EXPORT_RESOURCE`.
4. qgsd exports the image memory as a DMA-BUF FD.
5. qgs-linux transfers the FD with `SCM_RIGHTS`.
6. qgs-test imports the FD into a second qgs-vulkan context.

The FD is a transport attachment, not a protocol integer.

## Synchronization Chain

The Step 9 proof reuses the Step 7 external sync-FD path:

1. qgsd creates a binary exportable Vulkan semaphore.
2. qgsd submits producer GPU work that clears the image and signals the
   semaphore.
3. qgsd exports a Linux sync FD.
4. qgs-test receives the sync FD through `SCM_RIGHTS`.
5. qgs-test imports the semaphore payload.
6. consumer GPU work waits on the semaphore before processing the shared image.

No `device.wait_idle()` or `queue.wait_idle()` is used for the producer to
consumer dependency.

## GPU Image Operation

The private Step 9 shader is a fixed qgs-vulkan-owned image compute proof. It
reads each RGBA pixel from the imported image and writes:

- `R = 255 - R`
- `G = 255 - G`
- `B = 255 - B`
- `A = A`

This is not exposed as a public QGS protocol operation or generic shader API.

## GPU Execution Confirmation

The validation result is produced by Vulkan GPU work in the consumer context.
CPU code calculates expected bytes only after GPU execution for validation.
The protocol does not expose shader creation, pipeline creation, descriptor
binding, dispatch, or SPIR-V payloads.

## IPC Pixel Payload

The 64 x 64 image payload does not cross normal QGS IPC. Protocol messages carry
only bounded metadata and request IDs. Native FDs are transferred as
attachments. Image contents remain in the shared external image allocation.

## Validation And Readback

For M1 validation, qgsd initializes the image with producer GPU clear work.
qgs-test imports the image, waits on the sync FD, runs the private image shader,
then copies the processed image into a host-visible readback buffer so CPU code
can verify the result.

Readback is validation-only and is not a QGS data transport mechanism.

## Test Results

`cargo test --workspace` passed.

Total: 118 tests passed.

- `qgs-core`: 25 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 87 passed
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

Step 9 result:

- created 64 x 64 `Rgba8Unorm` Vulkan Image
- selected memory: device-local, not host-visible, not host-coherent
- exported DMA-BUF FD
- transferred FD through `SCM_RIGHTS`
- imported into a second Vulkan context
- external sync FD wait succeeded
- GPU image shader validation passed
- no producer-to-consumer device/queue idle was used
- resource destroyed successfully

Mesa printed the existing Haswell Vulkan support warning.

## GTX 950M Result

Device:

- `NVIDIA GeForce GTX 950M (NVK GM107)`
- class: `DiscreteGpu`
- vendor/device: `0x10de / 0x139a`

Step 9 result:

- created 64 x 64 `Rgba8Unorm` Vulkan Image
- selected memory: device-local, not host-visible, not host-coherent
- exported DMA-BUF FD
- transferred FD through `SCM_RIGHTS`
- imported into a second Vulkan context
- external sync FD wait succeeded
- GPU image shader validation passed
- no producer-to-consumer device/queue idle was used
- resource destroyed successfully

## Cleanup And Disconnect Result

The final qgs-test run left three transient buffers and one transient image
alive before disconnecting. qgsd logged:

```text
client disconnected; releasing 4 resource(s) and 6 sync object(s) for session 3
qgs-core: releasing 4 resource(s) owned by session
qgs-core: releasing 6 sync object(s) owned by session
```

This confirms image resources use the same session cleanup path as buffers.

## Unsafe Inventory Changes

No new unsafe boundary was added for Step 9.

The QGS-owned unsafe block count remains `6`, all inside qgs-vulkan audited
boundaries from Steps 6-8. All other QGS crates remain `#![forbid(unsafe_code)]`.

## Unavoidable Copies/Staging

No image pixel payload is copied through QGS IPC.

Step 9 uses a GPU image-to-buffer copy into a host-visible readback buffer only
for final validation. This is not part of the QGS data path.

## Commit And Push Verification

This report is included in the M1 Step 9 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Architectural And Driver Concerns

- `qgs-protocol` has no Vulkan or Linux FD dependency.
- `qgs-core` has no Vulkan dependency.
- `qgs-linux` knows only transport FD attachment semantics, not Vulkan.
- Vulkan-specific image format, tiling, layout, and import details remain inside
  `qgs-vulkan`.
- No Qnc concepts were added.
- No VideoSurface, YUV, NV12, P010, video decode/encode, DRM/KMS, presentation,
  generic shader API, generic compute API, scheduling, or telemetry was added.
- The successful hardware path uses linear DMA-BUF images. During development,
  DRM-format-modifier image creation through the current safe Vulkano path
  terminated qgsd on the Intel/Mesa path, so modifiers were not used for Step 9.
- On the tested linear DMA-BUF image path, the producer clear was observed with
  RGB channels as zero and alpha preserved. The validation therefore proves the
  consumer GPU shader writes RGB and preserves alpha on the shared image, but it
  does not claim a complete future color/video pipeline.

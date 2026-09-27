# QGS M2 Step 4A Intel VA/Vulkan Interop Diagnosis

## Scope

This is a failure-isolation report for the Intel Haswell VA/Vulkan zero-copy
path. It does not claim M2 Step 4 success and does not introduce a CPU-copy
fallback.

Observed failing path:

```text
synthetic H.264 Annex B
    -> Intel i965 VA hardware decode
    -> NV12 VA VideoSurface
    -> DRM PRIME export
    -> SCM_RIGHTS FD transfer
    -> Vulkan DMA-BUF import
    -> raw Vulkan external/foreign acquire + image-to-buffer GPU operation
    -> VK_ERROR_DEVICE_LOST
```

## Exact Environment

Kernel:

```text
Linux mironik 7.0.0-34-generic #34-Ubuntu SMP PREEMPT_DYNAMIC Wed Sep  2 14:29:37 UTC 2026 x86_64 GNU/Linux
```

Installed relevant packages:

```text
i965-va-driver:amd64       2.4.1+dfsg1-2build1
libva-dev:amd64            2.23.0-1ubuntu1
libva2:amd64               2.23.0-1ubuntu1
libva-drm2:amd64           2.23.0-1ubuntu1
mesa-vulkan-drivers:amd64  26.0.8-1ubuntu0.3
vulkan-tools               1.4.341.0+dfsg1-1
libvulkan1:amd64           1.4.341.0-1
libvulkan-dev:amd64        1.4.341.0-1
vainfo                     2.22.0+ds1-2build1
```

Intel render node:

```text
/dev/dri/renderD128
vendor: 0x8086
device: 0x0416
PCI_SLOT_NAME: 0000:00:02.0
kernel driver: i915
```

VA-API probe:

```text
VA-API version: 1.23.0
libva runtime: 2.22.0 as reported by vainfo
iHD driver: attempted, failed to open
i965 driver: /usr/lib/x86_64-linux-gnu/dri/i965_drv_video.so
i965 init: __vaDriverInit_1_22
driver: Intel i965 driver for Intel(R) Haswell Mobile - 2.4.1
```

Intel Vulkan device:

```text
deviceName: Intel(R) HD Graphics 4600 (HSW GT2)
apiVersion: 1.2.335
driverVersion: 26.0.8
vendorID: 0x8086
deviceID: 0x0416
deviceType: integrated GPU
driverID: DRIVER_ID_INTEL_OPEN_SOURCE_MESA
driverName: Intel open-source Mesa driver
driverInfo: Mesa 26.0.8-1ubuntu0.3
conformanceVersion: 0.0.0.0
deviceUUID: 86801604-0600-0000-0002-000000000000
```

Mesa prints:

```text
MESA-INTEL: warning: Haswell Vulkan support is incomplete
```

Selected Intel queue family:

```text
queue family index: 0
queueCount: 1
queueFlags: GRAPHICS | COMPUTE | TRANSFER
```

Relevant Intel device extensions exposed:

```text
VK_EXT_external_memory_dma_buf
VK_EXT_image_drm_format_modifier
VK_EXT_queue_family_foreign
VK_KHR_external_memory
VK_KHR_external_memory_fd
```

## Exact VA DRM PRIME Descriptor

The decoded Intel i965 Step 3B surface exported through DRM PRIME as:

```text
width: 64
height: 64
DRM fourcc: NV12 / 0x3231564e
objects: 1
layers: 1
object 0 size: 12288
object 0 modifier: 72057594037927938
layer 0 format: 842094158 / 0x3231564e
layer 0 plane 0 object index: 0
layer 0 plane 0 pitch: 128
layer 0 plane 0 offset: 0
layer 0 plane 1 object index: 0
layer 0 plane 1 pitch: 128
layer 0 plane 1 offset: 8192
```

These values are observations from this Intel/i965 system only. They must not
be hardcoded into production behavior.

## Exact Vulkan Import Configuration

The failed implementation used this intended Vulkan mapping:

```text
Vulkan format: VK_FORMAT_G8_B8R8_2PLANE_420_UNORM
tiling: VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT
external handle type: VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT
DRM modifier: 72057594037927938
object FD attachments: 1
memory object count used by QGS attempt: 1
binding model used by QGS attempt: non-disjoint single memory binding
proof operation attempted: image-to-buffer copy/read path
```

The intended image usage included transfer source so the proof could copy from
the imported image into a validation buffer. The setup attempted to preserve
foreign contents through a raw Vulkan barrier instead of a Vulkano first-use
transition from `UNDEFINED`.

The failed run did not retain all required low-level diagnostics. In
particular, the exact `vkGetMemoryFdPropertiesKHR` `memoryTypeBits`, selected
memory type index, and `vkGetImageMemoryRequirements` values were not logged
before the temporary code was reverted. That missing telemetry is now a
required part of the next reduced diagnostic run.

## Validation-Layer Results

`VK_LAYER_KHRONOS_validation` is not installed on this machine.

Forcing it with:

```text
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation vulkaninfo --summary
```

produced:

```text
Layer "VK_LAYER_KHRONOS_validation" was not found but was requested by env var VK_INSTANCE_LAYERS
```

Available layers are:

```text
VK_LAYER_INTEL_nullhw
VK_LAYER_MESA_anti_lag
VK_LAYER_MESA_device_select
VK_LAYER_MESA_overlay
VK_LAYER_MESA_screenshot
```

Therefore no validation or synchronization-validation messages are available
from the installed environment. No validation errors were suppressed.

## Exact First Failing Vulkan Stage

The previous temporary Step 4 implementation reached the combined submit path.
The first observed failing Vulkan call was:

```text
vkQueueSubmit -> VK_ERROR_DEVICE_LOST
```

for a command buffer containing the external/foreign acquire barrier and the
first image-to-buffer GPU operation.

From that run, the following stages completed far enough to reach submit:

```text
A. Create DRM modifier VkImage
B. Query image memory requirements
C. Query DMA-BUF memory FD properties
D. Import VkDeviceMemory
E. Bind imported memory
G. Record ownership/acquire barrier
H. End command buffer
```

The run did not isolate:

```text
I. submit command buffer containing only the acquire barrier
J. wait for that acquire-only submission
K. submit a second command buffer containing the first image read/copy
L. wait for that second submission
```

So the exact smallest failing semantic operation is not yet proven. The known
failure is narrower than setup/import and broader than "image read is bad":
the combined acquire-plus-copy queue submission lost the Intel Vulkan device.

The next diagnostic must split acquire-only from read/copy. Without that split,
QGS cannot honestly classify the fault as an acquire barrier issue, a
transfer-usage issue, or a driver failure on the first real image access.

## FOREIGN_EXT Support And Result

The Intel Vulkan device exposes:

```text
VK_EXT_queue_family_foreign: extension revision 1
```

That makes `VK_QUEUE_FAMILY_FOREIGN_EXT` the spec-justified first candidate for
acquiring contents produced by VA-API, because VA is a non-Vulkan producer.

The temporary implementation attempted the foreign-acquire path, but it
combined that acquire with image read/copy in one command buffer. Therefore:

```text
FOREIGN_EXT extension exposed: yes
FOREIGN_EXT acquire-only result: not isolated
FOREIGN_EXT acquire + first read/copy result: vkQueueSubmit returned VK_ERROR_DEVICE_LOST
```

This is not enough evidence to call `VK_EXT_queue_family_foreign` itself broken
on Haswell. It does prove that the combined path is unsafe to claim as working.

## EXTERNAL Support And Result

`VK_QUEUE_FAMILY_EXTERNAL` is available as a core external queue-family token
when using external memory. However, VA-API is not a Vulkan queue, and the Step
4 audit identified `FOREIGN_EXT` as the likely better semantic match when the
extension is present.

The prior run did not produce a separately documented `EXTERNAL` acquisition
result. Blindly trying `EXTERNAL` until the device does not crash would not be
a valid diagnosis. A future diagnostic may test it only with a written
spec-based justification for the Intel VA/Mesa pairing.

## Acquire Barrier Parameters

The intended acquire barrier model was:

```text
source queue family: VK_QUEUE_FAMILY_FOREIGN_EXT
destination queue family: Intel Vulkan queue family 0
old layout: externally valid imported layout, not UNDEFINED
new layout for copy proof: TRANSFER_SRC_OPTIMAL
source stage: all commands / external producer equivalent
source access: memory write
destination stage: transfer
destination access: transfer read
aspect: color aspect for the multi-planar image
```

The old-layout value is the sensitive part. Using `UNDEFINED` would permit
discarding VA-produced pixels and would not prove zero-copy preservation. The
diagnostic implementation avoided the ordinary Vulkano first-use undefined
transition, but the exact submitted barrier was not retained in committed
source or logs.

For the next reduced diagnostic, every barrier parameter must be logged exactly
before submission, including:

```text
srcStageMask
srcAccessMask
dstStageMask
dstAccessMask
oldLayout
newLayout
srcQueueFamilyIndex
dstQueueFamilyIndex
aspectMask
baseMipLevel
levelCount
baseArrayLayer
layerCount
```

## NV12 Modifier Runtime Capabilities

`vulkaninfo --show-formats` reports generic support for
`VK_FORMAT_G8_B8R8_2PLANE_420_UNORM`.

For Intel, the common format group containing that format reports:

```text
linear tiling:
  SAMPLED_IMAGE
  SAMPLED_IMAGE_FILTER_LINEAR
  TRANSFER_SRC
  TRANSFER_DST
  MIDPOINT_CHROMA_SAMPLES
  SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER
  SAMPLED_IMAGE_YCBCR_CONVERSION_SEPARATE_RECONSTRUCTION_FILTER
  DISJOINT
  COSITED_CHROMA_SAMPLES
  STORAGE_WRITE_WITHOUT_FORMAT

optimal tiling:
  SAMPLED_IMAGE
  SAMPLED_IMAGE_FILTER_LINEAR
  TRANSFER_SRC
  TRANSFER_DST
  MIDPOINT_CHROMA_SAMPLES
  SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER
  SAMPLED_IMAGE_YCBCR_CONVERSION_SEPARATE_RECONSTRUCTION_FILTER
  DISJOINT
  COSITED_CHROMA_SAMPLES
  STORAGE_WRITE_WITHOUT_FORMAT
```

This is useful but insufficient. Generic linear/optimal feature bits are not
the same as the feature set for the exact DRM modifier:

```text
72057594037927938
```

`vulkaninfo` on this system did not print a DRM modifier property table for
that format. The next diagnostic must call the Vulkan format-properties path
directly and log the exact matching `VkDrmFormatModifierPropertiesEXT` entry:

```text
drmFormatModifier
drmFormatModifierPlaneCount
drmFormatModifierTilingFeatures
```

## Modifier Memory-Plane Analysis

The VA descriptor has:

```text
format planes: 2 (Y and interleaved UV)
DRM objects: 1
```

That does not by itself determine the Vulkan DRM modifier memory-plane count.
Vulkan's modifier properties determine whether this imported image should be
bound as:

```text
one non-disjoint memory binding
```

or:

```text
multiple disjoint memory-plane bindings
```

The failed attempt used one memory object/binding because the VA descriptor had
one object. That is plausible for the current Intel surface, but it is not
proven correct without logging the modifier's reported Vulkan memory-plane
count and the image memory requirements.

The next diagnostic must explicitly confirm:

```text
DRM object count
Vulkan drmFormatModifierPlaneCount
VK_IMAGE_CREATE_DISJOINT_BIT presence/absence
number of memory requirements queried
number of memory binds submitted
```

## TRANSFER_SRC Support

The generic format table reports `TRANSFER_SRC` for
`VK_FORMAT_G8_B8R8_2PLANE_420_UNORM`.

However, Step 4A must care about the exact imported DRM modifier. At this point
QGS has not proven that the exported i965 modifier supports `TRANSFER_SRC` for
the created image. If the modifier does not support `TRANSFER_SRC`, then the
image-to-buffer proof operation was invalid even though sampling may still be
valid.

Therefore:

```text
generic NV12 TRANSFER_SRC support: yes
exact VA modifier TRANSFER_SRC support: not yet proven
```

If exact modifier support lacks `TRANSFER_SRC`, the next proof operation must
switch to a supported path such as YCbCr sampling, not image-to-buffer copy.

## Sampled Access Support

The generic NV12 format table reports:

```text
SAMPLED_IMAGE
SAMPLED_IMAGE_FILTER_LINEAR
SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER
```

Again, this is not proof for the exact VA modifier. The next diagnostic must
query the matching DRM modifier entry to determine whether sampled access is
valid for this imported image.

If sampled access is supported and transfer-source is not, the technically
correct reduced proof should be:

```text
acquire-only command buffer
then minimal shader sampling of one small region
then fence/readback of a separate validation buffer
```

## Reduced GPU-Read Tests

Reduced tests have not yet been run after the repository was restored clean.
The only observed hardware result is the combined acquire-plus-copy submit:

```text
reproducible result: VK_ERROR_DEVICE_LOST at queue submit
```

Required next sequence:

1. Create/import/bind the image and log every setup value.
2. Submit an acquire-only command buffer.
3. Wait for the acquire-only fence.
4. If acquire succeeds, submit the first runtime-supported GPU read path.
5. Prefer sampling if exact modifier capabilities do not include transfer
   source.

M2 Step 4 must not resume until this split confirms which stage fails.

## DEVICE_LOST Reproducibility

The device loss reproduced on the Intel HD Graphics 4600 path during the real
M2 Step 4 hardware proof. The same run proved that:

```text
VA decode succeeded
DRM PRIME export succeeded
SCM_RIGHTS FD transfer succeeded
Vulkan setup reached queue submission
```

The failure is therefore in the Vulkan imported-image ownership/access region,
not in H.264 parsing, VA decode, DRM PRIME export, or Unix FD transfer.

## Evidence Classification

Current evidence points to:

```text
QGS bug: possible, not proven
Vulkano limitation: already known for safe imported-image state handling
raw interop bug: possible, not proven
Mesa Haswell Vulkan limitation/bug: plausible, not proven
unsupported advertised combination: possible, not proven
```

Most likely unresolved assumptions:

1. QGS may have used a transfer-source proof on a modifier that does not
   actually support transfer source.
2. QGS may have bound the image as one memory plane when the Vulkan modifier
   model required a different binding shape.
3. The acquire barrier may have used a layout/access/stage combination the
   Haswell driver does not support for this external producer.
4. The Haswell Vulkan driver may advertise the relevant pieces but lose the
   device on a spec-valid imported NV12 DRM modifier access. Its conformance
   version is `0.0.0.0`, and Mesa warns that Haswell Vulkan support is
   incomplete.

The evidence does not justify changing QGS architecture or adding a CPU-copy
fallback.

## Smallest Technically Correct Fix If Identified

No final code fix is identified yet.

The smallest technically correct next step is a reusable, clean diagnostic
inside the audited qgs-vulkan interop boundary that logs:

```text
vkGetMemoryFdPropertiesKHR memoryTypeBits
selected memory type index
VkMemoryRequirements
exact VkDrmFormatModifierPropertiesEXT for the VA modifier
modifier plane count
exact image create flags and usage
exact memory bind path
exact acquire barrier parameters
result of acquire-only submit and fence wait
result of first supported GPU access submit and fence wait
```

If this diagnostic proves transfer source is unsupported for the modifier, the
Step 4 proof should use the supported sampled path.

If acquire-only fails with a spec-valid barrier, this hardware/driver path
should be treated as blocked.

If acquire-only succeeds and the first read fails despite exact modifier
support for that read usage, the failure is likely a Mesa Haswell imported
NV12 access limitation or driver bug.

## Can M2 Step 4 Resume On This Hardware?

Not yet.

M2 Step 4 can resume on this Intel Haswell machine only after the acquire-only
and exact-modifier capability diagnostics above are implemented and pass.

The current system proves the architecture up to exported decoded VA NV12 DRM
PRIME surfaces. It does not yet prove Vulkan can safely acquire and read that
surface on Haswell.

## Modern GPU Recommendation

Testing on a modern Intel or AMD Mesa system is recommended.

Reasons:

- The current Intel Haswell Vulkan driver reports conformance `0.0.0.0`.
- Mesa prints an explicit "Haswell Vulkan support is incomplete" warning.
- The device exposes the relevant extensions, but the exact imported NV12 DRM
  modifier access path still loses the device.
- Modern Intel and AMD drivers are more likely to exercise current
  external-memory, DRM modifier, multi-planar image, and YCbCr sampling paths.

This recommendation is for failure isolation and portability validation, not a
replacement for fixing QGS correctness.

## Conclusion

The zero-copy video architecture remains technically sound, but Step 4 is still
blocked on the Intel Haswell Vulkan imported-image access path.

The first observed Vulkan failure is `vkQueueSubmit` returning
`VK_ERROR_DEVICE_LOST` for the combined foreign-acquire plus image-to-buffer
operation. The investigation has not yet isolated acquire-only from the first
image read. It also has not proven exact `TRANSFER_SRC` support for the
VA-exported DRM modifier.

Do not claim M2 Step 4 success on this hardware until a reduced diagnostic
proves:

```text
correct memory-plane binding
exact modifier support for the chosen operation
successful external/foreign acquire
successful runtime-supported Vulkan read of the imported VA allocation
```

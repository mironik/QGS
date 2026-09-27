# QGS M2 Step 4B Haswell Final Diagnosis

## Scope

This is the final Intel Haswell imported-video diagnostic before deciding
whether M2 Step 4 must move to newer hardware.

It does not claim zero-copy success. It does not add a CPU-copy fallback.

## Validation Layer

`VK_LAYER_KHRONOS_validation` is installed and was enabled for the diagnostic.

```text
VK_LAYER_KHRONOS_validation 1.4.341
```

Synchronization validation was requested through `VkValidationFeaturesEXT` with
`SYNCHRONIZATION_VALIDATION`.

Captured validation output contained loader/device-selection informational
messages only. No relevant validation error was produced before the diagnostic
stopped at the memory-plane model.

## VA Decode And DRM PRIME Descriptor

Input:

```text
tests/fixtures/h264/idr-64x64-baseline.h264
```

VA path:

```text
/dev/dri/renderD128
Intel HD Graphics 4600 / i915
i965 VA driver
```

VA decode succeeded. Validation checksum:

```text
0x0bbbb30d
```

Exported descriptor:

```text
DRM fourcc: 0x3231564e (NV12)
width: 64
height: 64
objects: 1
layers: 1
object 0 size: 12288
object 0 modifier: 72057594037927938
layer 0 drm_format: 0x3231564e
layer 0 planes: 2
plane 0 object: 0 pitch: 128 offset: 0
plane 1 object: 0 pitch: 128 offset: 8192
```

## Exact Vulkan Modifier Query

Vulkan device:

```text
Intel(R) HD Graphics 4600 (HSW GT2)
queue family: 0
```

The exact VA-exported modifier is exposed by Vulkan:

```text
drmFormatModifier: 72057594037927938
drmFormatModifierPlaneCount: 2
drmFormatModifierTilingFeatures: 0x00000001008ed001
```

Decoded feature support for the exact modifier:

```text
SAMPLED_IMAGE: yes
TRANSFER_SRC: yes
TRANSFER_DST: yes
YCbCr linear filter: yes
YCbCr separate reconstruction filter: yes
```

This answers the first Step 4B question: the modifier is visible to Vulkan and
advertises the operations needed for either transfer or sampled access.

## Memory-Plane Model

The exact memory model is:

```text
VA DRM object count: 1
NV12 format plane count: 2
Vulkan DRM modifier memory-plane count: 2
```

This is the decisive result. The previous failing Step 4 path used one
non-disjoint image binding because the VA descriptor contained one DRM object.
That assumption is wrong for this modifier. Vulkan reports two modifier memory
planes, so the binding model must follow the Vulkan modifier memory-plane count
rather than the VA DRM object count.

Because the diagnostic stopped at this proven mismatch, these values were not
queried on the retained non-crashing path:

```text
vkGetMemoryFdPropertiesKHR memoryTypeBits: not run
VkMemoryRequirements size: not run
VkMemoryRequirements alignment: not run
VkMemoryRequirements memoryTypeBits: not run
selected memory type index: not run
number of imported memory objects: 0
number of Vulkan memory bindings: 0
binding offsets: []
```

## Corrected Binding Attempt

A temporary corrected diagnostic path was attempted but not retained as
committed reusable code. It changed the image model to:

```text
VK_IMAGE_CREATE_DISJOINT_BIT: yes
explicit DRM modifier plane layouts: 2
plane 0 layout: offset 0, row pitch 128
plane 1 layout: offset 8192, row pitch 128
```

With validation enabled, that temporary path segfaulted in native code during
`vkCreateImage`, before QGS could query per-plane memory requirements or submit
an acquire barrier.

GDB backtrace:

```text
Thread 1 "qgs-test" received signal SIGSEGV, Segmentation fault.
0x00007fffe8c5d22d in ?? () from /usr/lib/x86_64-linux-gnu/libvulkan_intel_hasvk.so
#0  0x00007fffe8c5d22d in ?? () from /usr/lib/x86_64-linux-gnu/libvulkan_intel_hasvk.so
#1  0x00007fffe657c37b in ?? () from /lib/x86_64-linux-gnu/libVkLayer_khronos_validation.so
#2  0x00007fffe684ef5d in ?? () from /lib/x86_64-linux-gnu/libVkLayer_khronos_validation.so
#3  0x00007fffe68ed041 in ?? () from /lib/x86_64-linux-gnu/libVkLayer_khronos_validation.so
#4  0x00007fffe6438f65 in ?? () from /lib/x86_64-linux-gnu/libVkLayer_khronos_validation.so
#5  ash::device::Device::create_image
#6  qgs_vulkan::haswell_video_diagnostic::run_raw_diagnostic
```

That crash was not committed as the reusable diagnostic path. The committed
diagnostic stops at the exact proven mismatch instead of leaving an invalid or
crashing experimental path in main.

## Acquire-Only Submission

Acquire-only was not run.

Reason:

```text
The exact modifier query proved that the previous one-binding image model was
invalid before any spec-valid acquire-only command buffer could be recorded.
```

No acquire barrier was submitted on the retained path. Therefore:

```text
vkQueueSubmit result: not run
fence wait result: not run
```

No arbitrary alternative barriers were tried.

## First GPU Access

No GPU read was attempted.

Reason:

```text
The diagnostic did not reach a valid imported/bound image. A GPU read before
correct memory-plane binding would not be meaningful.
```

The exact modifier supports both sampled access and transfer-source access, so
the first future GPU read should prefer the sampled NV12 path once a valid
two-plane binding path exists.

## Validation Messages

Validation captured only informational loader/device-selection messages, such
as Mesa device ordering and unused ICD removal. No relevant validation error
about image creation, binding, layout, ownership transfer, or access was
captured on the retained path because the diagnostic stopped before those
operations.

The temporary corrected disjoint image-create attempt crashed in the Intel
Haswell Vulkan driver with validation on the stack before a validation message
could be returned.

## Failure Classification

Classification:

```text
B. incorrect QGS memory-plane/binding implementation
```

The earlier one-binding path is invalid because:

```text
VA DRM object count: 1
Vulkan modifier memory-plane count: 2
```

A second finding is recorded but not used as the primary classification:

```text
G. temporary corrected disjoint create path segfaulted at vkCreateImage inside
libvulkan_intel_hasvk.so with validation enabled
```

This should be treated as a driver/validation-stack fragility or an incomplete
corrected binding implementation until independently reproduced with a minimal
C/Vulkan reproducer or a modern GPU.

## Step 4 Status

M2 Step 4 should not resume on this Haswell system right now.

The architecture remains valid, but this machine has not produced a valid
imported and acquired Vulkan image. The final diagnostic found a concrete QGS
assumption that must be fixed before interpreting `VK_ERROR_DEVICE_LOST` as a
driver bug:

```text
QGS must bind according to Vulkan DRM modifier memory planes, not VA DRM object
count.
```

After that fix, Haswell still carries elevated risk because Mesa reports:

```text
MESA-INTEL: warning: Haswell Vulkan support is incomplete
```

and the corrected diagnostic create path crashed before acquire-only could be
tested.

## Recommendation

FREEZE HASWELL ZERO-COPY PATH AND VALIDATE M2 STEP 4 ON MODERN INTEL/AMD

# QGS Rust Safety Policy

QGS is safe Rust by default.

`#![forbid(unsafe_code)]` remains enabled in:

- `qgs-protocol`
- `qgs-core`
- `qgs-linux`
- `qgsd`
- `qgs-test`

Unsafe Rust is permitted only when all of the following are true:

1. A required system/backend operation has no practical safe Rust API.
2. The unsafe operation is isolated inside a narrowly scoped backend/FFI module.
3. No unsafe API is exposed to `qgs-core`, `qgs-protocol`, `qgs-linux`,
   `qgsd`, or normal clients.
4. A safe QGS-owned wrapper validates all invariants that can reasonably be
   validated before entering the unsafe block.
5. Every unsafe block has a SAFETY comment documenting why its preconditions
   hold.
6. The unsafe surface is covered by focused tests where practical.
7. The unsafe boundary and its assumptions are documented for later audit.

## Current Unsafe Inventory

### qgs-vulkan direct ash backend

Location:

- `crates/qgs-vulkan/src/lib.rs`

Unsafe API categories:

- Vulkan instance/device/layer/extension/property calls through `ash`
- Vulkan object creation and destruction for instance, device, memory, buffers,
  images, image views, semaphores, fences, command pools, shader modules,
  descriptor resources, pipelines, and command buffers
- Vulkan queue submission and command recording
- Vulkan external-memory FD export/import
- borrowed-FD `vkGetMemoryFdPropertiesKHR`
- Vulkan external semaphore sync-FD export/import
- host memory mapping, flush, invalidate, and bounded pointer copies
- fixed embedded SPIR-V shader module creation and compute dispatch
- software-decoded YUV422P10 plane upload through staging buffers, fixed
  storage-buffer compute processing, push constants, and validation readback

Why unsafe is required:

GPU R1 migrated `qgs-vulkan` from Vulkano to a QGS-owned Vulkan backend over
`ash`. Vulkan is a C ABI and `ash` exposes Vulkan calls as unsafe. QGS keeps
that unsafety private to `qgs-vulkan` and exposes only QGS-owned safe APIs to
the rest of the workspace.

The safe QGS wrapper validates:

- device descriptions are copied into QGS-owned protocol types and raw Vulkan
  handles do not escape `qgs-vulkan`
- queue-family selection chooses a reported queue family and checks compute
  support for compute proofs
- memory type selection intersects Vulkan `memoryTypeBits` with required and
  preferred QGS memory properties
- allocation sizes come from Vulkan memory requirements or QGS-validated import
  metadata
- host-visible map/copy/read paths bounds-check byte ranges and flush or
  invalidate non-coherent memory as required
- imported FD memory properties are queried with a borrowed FD before the owned
  FD is consumed by Vulkan memory import
- external buffer/image support is queried before exportable/importable resource
  creation
- DMA-BUF and opaque-FD handle types are translated only inside `qgs-vulkan`
- sync FD support is queried before semaphore creation/export/import
- the semaphore is binary and created with sync FD export support
- producer GPU work signals the exported semaphore payload
- sync FD import uses temporary payload semantics as required for
  copy-transference handles
- fixed compute shaders use explicit descriptor set layout `set=0,binding=0`
  and no client-provided SPIR-V is accepted
- the fixed YUV422P10 proof shader uses explicit descriptor bindings for Y,
  Cb, Cr, and RGBA output storage buffers plus a 16-byte push-constant block
- software video upload validates dimensions, even-width 4:2:2 policy, plane
  dimensions, source strides, byte lengths, and arithmetic overflow before
  recording Vulkan commands
- CPU YUV422P10 samples are expanded from little-endian 16-bit storage into
  `u32` GPU storage-buffer samples without reducing 10-bit precision
- the reusable GPU frame processor bounds slot count, keeps descriptor sets and
  command buffers per slot, resets command pools only after the slot fence has
  completed, and records explicit buffer barriers for staging upload, shader
  reads/writes, and validation readback
- normal frame retirement uses per-slot Vulkan fences; cleanup waits submitted
  slot fences before slot-owned resources are destroyed
- submitted work is retained or waited at cleanup boundaries before Vulkan
  resources and semaphores are destroyed
- optional validation-layer runs can be enabled with
  `QGS_VULKAN_ENABLE_VALIDATION=1`

Remaining assumptions:

- `ash` forwards raw Vulkan calls according to the Vulkan ABI.
- The Vulkan loader and driver uphold Vulkan object, FD, and synchronization
  semantics.
- Backend memory type indices in export metadata are interpreted only by
  `qgs-vulkan` for the matching backend/device.
- The backend/device identity used by M1 is sufficient for the local
  two-process proof, but it is not a persistent identity model.
- The external sync proofs remain one-shot validation paths, not a reusable
  frame synchronization protocol.
- Device idle waits are used at resource cleanup boundaries to make destruction
  validity explicit; they are not the producer-to-consumer dependency.

Unsafe block count in QGS-owned production Vulkan backend code after M2 Step 11:
104.

### qgs-vulkan Haswell video interop diagnostic

Location:

- `crates/qgs-vulkan/src/haswell_video_diagnostic.rs`

Unsafe APIs:

- raw Vulkan instance/device creation through `ash`
- `VK_LAYER_KHRONOS_validation` debug callback setup
- DRM modifier format-property queries
- borrowed-FD `vkGetMemoryFdPropertiesKHR`
- diagnostic raw image creation/import/bind/acquire commands where reached

Why unsafe is required:

M2 Step 4B needs exact Vulkan diagnostics for an imported VA/DRM PRIME NV12
surface, including validation-layer capture and DRM modifier memory-plane
inspection. These operations sit at the same interop boundary identified in the
M2 Step 4 audit and are not exposed by Vulkano's safe image-state model.

The safe QGS wrapper constrains:

- the diagnostic to the Intel Haswell vendor/device identity
- validation-layer enablement
- QGS-owned bounded DRM PRIME descriptor metadata
- exact modifier lookup before any import/acquire attempt
- RAII `OwnedFd` input from the VA diagnostic helper
- early stop when the exact modifier memory-plane model does not match the
  currently safe reusable binding path

Remaining assumptions:

- `ash` forwards raw Vulkan calls according to the Vulkan ABI.
- The Vulkan loader, validation layer, and Intel driver are trusted diagnostic
  components.
- The diagnostic is not a public QGS protocol surface and must not be treated
  as a production video processing path.

Unsafe block count in retained Haswell diagnostic code after GPU R1: 32.

Unsafe block count in all QGS-owned code after M2 Step 11: 136.

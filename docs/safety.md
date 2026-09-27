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

### qgs-vulkan external-memory import

Location:

- `crates/qgs-vulkan/src/external_memory.rs`

Unsafe API:

- `vulkano::memory::DeviceMemory::import`

Why unsafe is required:

Vulkano 0.35.2 exposes memory export as a safe API, but importing external
Vulkan device memory from an FD requires `unsafe DeviceMemory::import`. No
higher-level safe Vulkano buffer-import API satisfying M1 Step 6 was found.

The safe QGS wrapper validates:

- the imported handle type matches QGS export metadata
- the logical device enables the required external-memory extensions
- the target physical device matches the QGS-visible exported device identity
- the imported buffer is created with matching size, usage, and external handle
  type
- allocation size and memory type index come from the exporter metadata
- dedicated-allocation state is reproduced when reported by the exporter
- the owned FD is moved into the import call and is not used again by QGS code

Remaining assumptions:

- Vulkano and the Vulkan driver uphold the documented FD ownership behavior.
- Backend memory type indices in export metadata are interpreted only by
  `qgs-vulkan` for the matching backend/device.
- M1 Step 6 validation uses host-visible shared memory and a quiescent CPU
  read/write sequence. GPU ordering is handled separately by Step 7.

### qgs-vulkan external synchronization

Location:

- `crates/qgs-vulkan/src/external_sync.rs`

Unsafe APIs:

- `vulkano::sync::semaphore::Semaphore::export_fd`
- `vulkano::sync::semaphore::Semaphore::import_fd`
- `vulkano::device::QueueGuard::submit`

Why unsafe is required:

Vulkano 0.35.2 exposes POSIX FD semaphore export/import and direct queue submit
as unsafe operations. Step 7 needs the external semaphore FD to be signaled by a
GPU queue submission, transferred to another process, imported into a second
Vulkan context, and waited on by a consumer GPU submission. No higher-level safe
Vulkano path satisfying those exact external-handle semantics was found.

The safe QGS wrapper validates or constrains:

- the selected handle type is Linux sync FD
- the physical device reports sync FD external semaphore import/export support
- the semaphore is binary and created with sync FD export support
- export occurs only after QGS has submitted exactly one producer GPU signal
  operation and no producer-side wait operation
- the sync FD is transferred as an owned transport attachment and moved into
  the import call
- sync FD import uses temporary payload semantics as required for
  copy-transference handles
- producer and consumer contexts are matched using QGS-visible backend/device
  identity rather than Vulkan enumeration order
- submitted command buffers, semaphores, and fences are retained for the
  operation lifetime

Remaining assumptions:

- Vulkano and the Vulkan driver uphold documented sync FD ownership and
  semaphore-payload semantics.
- The backend/device identity used by M1 is sufficient for the local
  two-process proof, but it is not a persistent identity model.
- The Step 7 proof validates one-shot ordering for a minimal transfer path; it
  is not a reusable frame synchronization protocol.

### qgs-vulkan compute proof

Location:

- `crates/qgs-vulkan/src/external_compute.rs`

Unsafe APIs:

- `vulkano::shader::ShaderModule::new`
- `vulkano::command_buffer::AutoCommandBufferBuilder::dispatch`

Why unsafe is required:

Vulkano 0.35.2 requires unsafe calls for creating a shader module from SPIR-V
words and for recording a dispatch. M1 Step 8 needs a real GPU compute dispatch
against the imported shared buffer, but it does not expose shaders or dispatch
commands to QGS clients.

The safe QGS wrapper validates or constrains:

- the SPIR-V is a fixed QGS-owned shader embedded in `qgs-vulkan`
- no client-provided SPIR-V or shader parameters are accepted
- the shader layout is reflected by Vulkano before pipeline creation
- descriptor set `0`, binding `0` is bound to the imported shared storage
  buffer before dispatch
- the input element count is non-zero and a multiple of the fixed local size
- the dispatch group count is derived from the validated input element count
- the selected queue family supports compute
- the compute submission waits on the imported external sync FD before touching
  the shared buffer
- a fence wait is used only after submission so CPU validation can read the
  result

Remaining assumptions:

- Vulkano and the Vulkan driver uphold shader-module and dispatch validation
  requirements for the fixed embedded SPIR-V.
- The embedded shader remains a small audited proof shader and is not treated as
  a public QGS compute interface.

Unsafe block count in QGS-owned code after M1 Step 8: 6.

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

Unsafe block count in QGS-owned code after M2 Step 4B diagnostics: 38.

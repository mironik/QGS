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
- M1 validation uses host-visible shared memory and a quiescent CPU read/write
  sequence; concurrent GPU synchronization is deferred.

Unsafe block count in QGS-owned code after M1 Step 6: 1.

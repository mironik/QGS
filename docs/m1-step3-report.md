# QGS M1 Step 3 Report

## Files And Crates Added Or Changed

Added:

- `crates/qgs-vulkan/`
- `docs/m1-step3-report.md`

Changed:

- `Cargo.toml`
- `Cargo.lock`
- `crates/qgs-core/`
- `crates/qgs-protocol/`
- `daemon/qgsd/`
- `tools/qgs-test/`
- `docs/architecture.md`
- `docs/protocol.md`

## Vulkan Rust Dependency

Step 3 uses `vulkano` `0.35.2` with default features disabled.

`vulkano` was selected because it is a mature safe wrapper for Vulkan. This
allows QGS-owned code to keep `#![forbid(unsafe_code)]` while still using real
Vulkan physical-device enumeration. QGS does not use `serde`, `bincode`, or any
Rust struct memory layout as a wire protocol.

## QGS DeviceDesc Fields

`DeviceDesc` contains:

- QGS `DeviceId`
- `DeviceClass`
- `vendor_id`
- `device_id`
- human-readable device `name`
- backend/API identity as `BackendApi`
- `api_version`
- `driver_version`

No large capability structure, resource allocation, compute workload, video
decode, video encode, DRM/KMS, or DMA-BUF path was added.

## DeviceId Semantics

`DeviceId` is a non-zero QGS-owned identifier. In Step 3 it is assigned from the
Vulkan enumeration index plus one and is stable only within the current `qgsd`
process/session. Clients must not treat it as globally persistent or stable
across boots.

## DeviceClass Mapping

Vulkan device type maps to QGS `DeviceClass` as follows:

- integrated GPU -> `IntegratedGpu`
- discrete GPU -> `DiscreteGpu`
- CPU Vulkan device -> `Software`
- virtual GPU -> `Other`
- other -> `Other`

Classification does not use device names or vendor IDs.

## Protocol Additions

New request:

- `ENUMERATE_DEVICES`: request kind `1`, opcode `2`, empty payload

New response:

- `DEVICE_LIST`: response kind `2`, opcode `3`

Existing opcodes remain:

- `HELLO`: request kind `1`, opcode `1`
- `WELCOME`: response kind `2`, opcode `1`
- `ERROR`: response kind `2`, opcode `2`

`qgs-test` now performs HELLO/WELCOME first, then sends ENUMERATE_DEVICES and
validates the DEVICE_LIST `request_id`.

## Wire Limits

- maximum payload size: 4096 bytes
- maximum device count: 16
- maximum device name length: 128 UTF-8 bytes

The decoder validates device count, name length, UTF-8, truncated entries,
malformed counts, trailing payload, and zero `DeviceId` before exposing decoded
device descriptions.

## Test Results

`cargo test --workspace` passed.

Total tests: 37

- `qgs-core`: 2 passed
- `qgs-linux`: 1 passed
- `qgs-protocol`: 32 passed
- `qgs-vulkan`: 2 passed
- `qgsd`: 0 tests
- `qgs-test`: 0 tests

## Clippy

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Real Hardware Discovery

Manual two-process test:

- Process 1: `qgsd /tmp/qgs-step3-codex.sock`
- Process 2: `qgs-test /tmp/qgs-step3-codex.sock`

Observed devices:

- `[IntegratedGpu] Intel(R) HD Graphics 4600 (HSW GT2)`
  - vendor `0x8086`, device `0x0416`, backend `Vulkan`, API `1.2.335`
- `[DiscreteGpu] NVIDIA GeForce GTX 950M (NVK GM107)`
  - vendor `0x10de`, device `0x139a`, backend `Vulkan`, API `1.3.281`
- `[Software] llvmpipe (LLVM 21.1.8, 256 bits)`
  - vendor `0x10005`, device `0x0000`, backend `Vulkan`, API `1.3.281`

The implementation does not rely on this ordering or hardcode these devices.

The Mesa Intel driver emitted:

`MESA-INTEL: warning: Haswell Vulkan support is incomplete`

Discovery still completed successfully.

## Commit

This report is included in the M1 Step 3 commit. The final commit hash cannot
be embedded in the committed file without making the commit self-referential;
the exact hash is reported after the commit is created and pushed.

## Origin Verification

After the final push, `main` and `origin/main` are expected to point to the same
M1 Step 3 commit. The exact hash is reported after push verification.

## Architectural Concerns

No blocking architectural concerns were found.

- `qgs-protocol` has no Vulkan crate dependency.
- `qgs-core` has no Vulkan crate dependency.
- `qgs-linux` has no Vulkan crate dependency.
- Vulkan-specific types remain inside `qgs-vulkan`.
- QGS-owned code remains `#![forbid(unsafe_code)]`.
- No Qnc concepts were added.
- No capability system was added beyond basic device description.
- No GPU resources are allocated.
- No compute work occurs.
- No video decode or video encode was introduced.
- Wire lengths are bounded and validated.
- Device ordering is not assumed.

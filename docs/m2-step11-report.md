# QGS M2 Step 11 Report

## Summary

M2 Step 11 replaces the Step 10 one-frame GPU proof lifecycle with a bounded
reusable frame-processing runtime for software-decoded `YUV422P10LE`
VideoSurfaces.

The proven path remains the software fallback path, not zero-copy:

```text
software VideoSurface
    -> reusable qgs-vulkan FrameSlot
    -> explicit staging upload
    -> fixed YCbCr -> RGBA GPU processing
    -> validation readback
```

The frozen Haswell VA -> Vulkan imported-image path was not resumed.

## Frame Processor Architecture

`qgs-vulkan` now exposes a QGS-owned safe `GpuFrameProcessor` over the direct
ash backend. It accepts validated `Yuv422P10Upload` inputs, owns bounded slots,
submits Vulkan work, tracks completion with fences, and returns processed
QGS-owned frame outputs for explicit validation.

No ash or raw Vulkan type escapes `qgs-vulkan`.

## FrameSlot Model

Each slot owns reusable per-frame resources:

- host-visible staging buffers for Y, Cb, and Cr
- device-local GPU Y, Cb, and Cr storage buffers
- device-local RGBA `u32` output buffer
- host-visible validation readback buffer
- one command pool
- one command buffer
- one fence
- one descriptor set
- frame identity and submission metadata while in flight

Shared processor resources:

- fixed YUV422P10 shader module
- descriptor set layout
- pipeline layout
- compute pipeline
- descriptor pool sized for the configured slots

## State Machine

Slot state is explicit:

```text
Available -> Preparing -> Submitted -> Completed -> Available
```

Only `Available` slots can be submitted. Submitted slots become `Completed`
only through fence observation or explicit wait. Completed slots are retired
before reuse.

## Slot Policy

The default proof configuration uses 3 slots. The implementation accepts a
bounded slot count of 1 through 8.

When all slots are busy, submission returns `NoFrameSlotAvailable`; it does not
allocate an unbounded extra slot.

## Frame Identity

The processor uses an opaque `FrameToken` for submission tracking and preserves
a QGS-owned presentation position in `FrameIdentity`.

These remain distinct:

- presentation/edit-unit position
- decoded VideoSurface resource identity
- FrameSlot index
- submission sequence
- processed output identity

## Synchronization And Retirement

Normal frame processing uses per-slot fences. The normal path does not use
`vkDeviceWaitIdle` or queue idle to decide whether a slot can be reused.

The command buffer records:

1. reuse barriers for slot-owned GPU/readback buffers
2. staging-to-GPU plane copies
3. transfer-write -> shader-read barriers for uploaded planes
4. fixed compute dispatch
5. shader-write -> transfer-read barrier for output
6. output-to-readback copy
7. transfer-write -> host-read barrier for readback validation

Drop/cleanup waits submitted slot fences. Lower-level qgs-vulkan resource
destructors still contain conservative cleanup-boundary device-idle waits; they
are not used as the normal frame dependency model.

## Command And Descriptor Strategy

The processor creates one command pool and one command buffer per slot. Command
pools are reset only after the slot's previous fence has completed. Descriptor
sets are per-slot and are not overwritten while submitted work can reference
them.

## Resource Reuse

The 3-slot / 6-frame proof created:

- pipeline creations: 1
- shader module creations: 1
- staging allocations: 9
- GPU plane allocations: 9
- output allocations: 3
- readback allocations: 3
- command buffers: 3
- frame submissions: 6
- slot reuses: 3

This proves bounded reuse after initial slot creation.

## Haswell Scheduling Note

During development, reusing one completed slot while other older submissions
were still in flight produced incorrect Haswell output without validation-layer
errors. The accepted proof therefore uses bounded batches: submit three frames,
hit backpressure, wait/retire the batch, then reuse the slots for the next
batch.

This still proves multiple in-flight submissions, bounded backpressure,
fence-based retirement, and slot reuse. Fine-grained overlapping slot reuse is
left as a future scheduling/driver investigation rather than being hidden
inside Step 11.

## Synthetic 6-Frame Result

Validation-enabled qgs-test synthetic proof passed on both physical GPUs.

Intel HD Graphics 4600:

- six frames processed through three slots
- max CPU/GPU delta: 0 or 1 for all frames
- checksums:
  - `0xc24694e6820ff094`
  - `0xa2b8a1254925458b`
  - `0xfb3a271e2223edd1`
  - `0x5bc9abb93a2b5514`
  - `0x6a98e0bd4739ad3b`
  - `0xf9018b92baf5437f`
- development observation: 12.183 ms for six reusable submissions

NVIDIA GTX 950M / NVK:

- six frames processed through three slots
- max CPU/GPU delta: 0 or 1 for all frames
- checksums matched Intel for all six frames
- development observation: 2.475 ms for six reusable submissions

## FX6 Sequence Result

External media was used read-only. The SHA-256 of the local MXF copy matched
the expected value:

`6bb8d23f91be8812f0bf9c09b6ee680dce0560757b9d778333d3e996b5f69653`

The external sample is reported only as Sony FX6 sample 001.

The software decode stage produced 106 presentation frames in 28.490 s
development-observation time.

For the first six presentation outputs, both Intel and NVIDIA processed the
same six software-decoded frames through the reusable GPU processor. The output
checksums matched across devices:

- `0xbf2155038add7e05`
- `0x47184f2fb1e4ddcf`
- `0x0772c6ece86f926e`
- `0x99795fc17876b588`
- `0xc1d1804018a56f5e`
- `0xf3bd6512bb4b9a73`

For random access:

- requested frame: edit unit 53
- nearest random access: 48
- sequential frame 53 GPU checksum: `0x4903f1968454f2e0`
- random-access frame 53 GPU checksum: `0x4903f1968454f2e0`
- result: match

Final frame checksum on both devices:

- `0xeda8a822766fe375`

All selected frames stayed within max CPU/GPU delta 1.

## Memory Model

For one 1920x1080 FX6 frame:

- CPU software surface bytes: 8,294,400
- staging bytes per slot: 16,588,800
- GPU YUV plane bytes per slot: 16,588,800
- output bytes per slot: 16,588,800

For 3 slots:

- staging: 49,766,400 bytes
- GPU YUV planes: 49,766,400 bytes
- output: 49,766,400 bytes
- readback: 49,766,400 bytes

The `u32` plane representation is intentionally unchanged from Step 10.

## Validation Layers

Hardware proofs were run with:

```text
QGS_VULKAN_ENABLE_VALIDATION=1
```

No Vulkan validation errors were reported. The expected Mesa Haswell warning
about incomplete Haswell Vulkan support appeared.

## Unsafe Inventory

Before Step 11:

- production `qgs-vulkan`: 98 unsafe blocks
- retained Haswell diagnostic: 32 unsafe blocks
- total QGS-owned unsafe blocks: 130

After Step 11:

- production `qgs-vulkan`: 104 unsafe blocks
- retained Haswell diagnostic: 32 unsafe blocks
- total QGS-owned unsafe blocks: 136

No unsafe code was added outside `qgs-vulkan`. No new unsafe functional area was
introduced outside the existing direct-ash Vulkan backend boundary; the new
blocks cover reusable command/fence/barrier operations.

## Tests

Added/covered:

- frame processor config bounds
- slot state reuse predicates
- frame token stability
- padded YUV input preparation
- synthetic 6-frame / 3-slot GPU proof
- bounded backpressure
- resource-count accounting
- FX6 first-six reusable processor proof
- FX6 sequential/random frame 53 GPU proof

Quality gates:

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed
- `cargo test --workspace`: passed
- validation-enabled qgs-test hardware proof: passed
- validation-enabled external FX6 GPU proof: passed

Workspace unit/doc test total: 212 tests passed.

## Cleanup

Processor Drop waits submitted slot fences before slot-owned resources are
destroyed. qgs-test disconnect cleanup also passed after transient resource
creation. No FD or Vulkan validation-layer cleanup errors were observed.

## Limitations

- The runtime is still a proof-oriented frame processor, not a playback
  scheduler.
- The first reusable proof uses batch retirement on Haswell rather than
  fine-grained overlapping slot reuse.
- The GPU representation still expands 10-bit samples to `u32`.
- Readback remains validation-only.
- No final color pipeline, presentation path, timeline integration, audio
  decode, encode, HEVC, MPEG-2, or VA -> Vulkan zero-copy work was added.

## Commit And Push

This report is included in the Step 11 implementation commit:

```text
Implement QGS reusable GPU frame pipeline
```

After push, final verification is:

```text
git status --short
git rev-parse main
git rev-parse origin/main
```

## Recommendation

Next milestone: optimize the GPU video representation, especially replacing
the portable `u32` plane representation with a smaller native representation
where supported, while preserving the reusable slot/fence runtime.

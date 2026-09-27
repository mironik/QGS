# QGS M2 Step 10 Report

## Summary

M2 Step 10 proves the fallback GPU processing path for professional H.264
streams that are decoded in software:

```text
MXF
  -> qgs-mxf
  -> H.264 High 4:2:2 10-bit access units
  -> qgs-software-video / libavcodec
  -> CPU-backed YUV422P10LE VideoSurface
  -> explicit qgs-vulkan upload
  -> fixed GPU YCbCr -> RGBA processing proof
  -> validation readback
```

This is not zero-copy. The CPU-decoded software VideoSurface is uploaded
explicitly to Vulkan resources. The frozen Haswell VA -> Vulkan imported-image
path was not resumed.

## GPU Representation

QGS uses a plane-oriented upload model for `yuv422p10le`:

- Y plane -> staging buffer -> GPU storage buffer
- Cb plane -> staging buffer -> GPU storage buffer
- Cr plane -> staging buffer -> GPU storage buffer
- fixed compute shader -> RGBA `u16` validation output buffer

Alternatives considered:

- Three Vulkan images: closer to image-processing terminology, but more format
  feature variability and no benefit for the first CPU-upload fallback proof.
- Vulkan multi-planar 10-bit 4:2:2 image: not chosen for the fallback path
  because support is less portable on the current Haswell/NVK devices and would
  blur this upload path with future external-memory video interop.
- Three GPU buffers: chosen because it preserves plane identity and 4:2:2
  chroma addressing, works on both current physical GPUs, and keeps the path
  simple and explicit.

The CPU source stores 10-bit samples in little-endian 16-bit slots. QGS expands
those numeric samples to `u32` GPU storage-buffer elements. This preserves the
10-bit values exactly and avoids requiring 16-bit storage-buffer features on
older GPUs.

## Upload API

`qgs-vulkan` now exposes a narrow safe API for this proof:

- `Yuv422P10Upload`
- `Yuv422P10Plane`
- `YcbcrConversion::Rec709Limited`
- `VulkanDeviceDiscovery::process_yuv422p10_surface`

No `ash` or Vulkan handle types escape `qgs-vulkan`.

Validation covers:

- non-zero dimensions
- maximum video dimensions
- even-width 4:2:2 policy
- Y/Cb/Cr plane dimensions
- padded source strides
- byte lengths
- staging/output size arithmetic overflow

## Shader Contract

The fixed internal shader uses:

- binding 0: Y storage buffer
- binding 1: Cb storage buffer
- binding 2: Cr storage buffer
- binding 3: RGBA output storage buffer
- push constants: width, height, Y stride in samples, chroma stride in samples

For pixel `x`, chroma index is `floor(x / 2)`. Cb/Cr height is the full luma
height, so this is 4:2:2 rather than 4:2:0.

## Conversion Proof

The proof uses a simple Rec.709 limited-range code-value conversion scaled for
10-bit inputs and writes RGBA `u16` validation output. This is a processing
proof, not final color management. QGS does not implement LUTs, HDR, tone
mapping, monitor transforms, or broadcast-accurate color management in this
milestone.

## Synthetic 8x4 Proof

The default `qgs-test` hardware pass now includes an 8x4 synthetic
YUV422P10LE surface with padded CPU strides.

Validation-enabled results:

- Intel HD Graphics 4600: PASS, max CPU/GPU delta 1
- NVIDIA GTX 950M / NVK: PASS, max CPU/GPU delta 1

Synthetic memory observation per proof frame:

- CPU surface bytes: 176
- staging bytes: 256
- GPU plane bytes: 256
- output bytes: 256

## Real FX6 Result

External sample: Sony FX6 sample 001.

The media remains outside the repository. The proof uses `qgs-mxf` for MXF
indexing/extraction and `qgs-software-video` for libavcodec decode. No
libavformat demuxing and no external ffmpeg process are used.

Decoded source properties:

- H.264 High 4:2:2
- 10-bit
- 4:2:2
- 1920 x 1080
- `yuv422p10le`
- 106 presentation frames
- random-access target 53 starts from MXF random-access point 48

### Intel HD Graphics 4600

Validation-enabled GPU upload/processing results:

- first frame: GPU checksum `0xbf2155038add7e05`, max CPU/GPU delta 1
- sequential frame 53: GPU checksum `0x4903f1968454f2e0`, max CPU/GPU delta 1
- random-access frame 53: GPU checksum `0x4903f1968454f2e0`, max CPU/GPU delta 1
- final frame: GPU checksum `0xeda8a822766fe375`, max CPU/GPU delta 1
- sequential/random frame 53 GPU result: match

### NVIDIA GTX 950M / NVK

Validation-enabled GPU upload/processing results:

- first frame: GPU checksum `0xbf2155038add7e05`, max CPU/GPU delta 1
- sequential frame 53: GPU checksum `0x4903f1968454f2e0`, max CPU/GPU delta 1
- random-access frame 53: GPU checksum `0x4903f1968454f2e0`, max CPU/GPU delta 1
- final frame: GPU checksum `0xeda8a822766fe375`, max CPU/GPU delta 1
- sequential/random frame 53 GPU result: match

Intel and NVIDIA produced matching GPU checksums for the tested frames.

## Memory Observations

For one 1920 x 1080 FX6 proof frame:

- CPU software surface bytes: 8,294,400
- staging bytes: 16,588,800
- GPU plane bytes: 16,588,800
- RGBA `u16` output bytes: 16,588,800

The staging/GPU plane size is larger than the CPU surface because the first
portable GPU representation expands each 10-bit sample into one `u32` storage
element.

## Development Timing Observations

Development observations, not benchmarks:

- full software decode observation: about 30.070 seconds for 106 frames
- Intel per-frame upload/process/readback: about 1.15 to 1.37 seconds
- NVIDIA/NVK per-frame upload/process/readback: about 3.39 to 4.90 seconds
- CPU reference conversion per tested FX6 frame: about 0.41 to 0.45 seconds

The current proof uses conservative allocations and one-shot validation
submissions. No optimization work was performed.

## Validation Layer

The default hardware proof and real FX6 proof were run with:

```sh
QGS_VULKAN_ENABLE_VALIDATION=1
```

No Vulkan validation errors were reported. The expected Mesa Haswell warning
about incomplete Haswell Vulkan support appeared and is not a QGS validation
error.

## Unsafe Inventory

Before Step 10:

- production `qgs-vulkan`: 82 unsafe blocks
- retained Haswell diagnostic: 32 unsafe blocks
- total QGS-owned unsafe blocks: 114

After Step 10:

- production `qgs-vulkan`: 98 unsafe blocks
- retained Haswell diagnostic: 32 unsafe blocks
- total QGS-owned unsafe blocks: 130

The increase is confined to `qgs-vulkan` and covers ash calls for the fixed
storage-buffer shader path, descriptor setup, push constants, buffer barriers,
and command recording. No unsafe code was added outside `qgs-vulkan`.

## Tests And Quality Gates

Added hardware-independent tests for:

- YUV422P10 plane dimensions
- padded CPU stride
- invalid even-width/stride cases
- sample-value expansion
- 4:2:2 chroma addressing
- CPU reference conversion
- deterministic RGBA output checksum

Final gate results:

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed
- `cargo test --workspace`: passed
- workspace unit tests: 209 passed
- doc tests: passed
- validation-enabled `qgs-test`: passed
- validation-enabled real FX6 software decode -> GPU proof: passed

## Cleanup

The proof uses RAII Vulkan wrappers for staging buffers, GPU plane buffers,
output buffers, readback buffers, descriptor resources, pipelines, command
pools, and fences. Validation-enabled hardware runs completed without resource
lifetime errors.

## Commit And Push Verification

This report is included in the milestone commit:

- `Implement QGS software video GPU upload`

The final pushed commit hash is reported after the push completes.

## Limitations

- This is an explicit upload fallback path, not zero-copy.
- Output is a validation buffer, not a final display surface.
- The shader uses one Rec.709 limited-range proof conversion.
- The first portable GPU representation expands 10-bit samples to `u32`, which
  is simple but memory-heavy.
- Timing observations are development data from one machine, not benchmarks.

## Recommendation

Next milestone: design the production software VideoSurface -> GPU processing
surface boundary. The main follow-up is to replace proof allocations with a
small reusable upload/processing resource model while keeping final color
management and presentation out of scope until their own milestones.

# QGS M2 Step 13 Report: Production VA Decode Path and Proxy Throughput

## Summary

M2 Step 13 separates the qgs-vaapi normal hardware decode path from explicit
diagnostic/validation work. The normal path now decodes and manages VA surfaces
without automatic per-frame CPU checksum readback, DRM PRIME export probing, or
verbose per-frame diagnostics. Diagnostic mode remains available explicitly for
validation and interop investigation.

The clean Sony FX6 sample 002 proxy remains the Step 12/13 acceptance proxy.
The damaged Sony FX6 sample 001 proxy remains a strict damaged-media diagnostic
case and is not used for all-frame proxy acceptance.

## Implementation

`qgs-vaapi` now supports two decode modes:

- `VaapiDecodeMode::Normal`: production decode path.
- `VaapiDecodeMode::Diagnostic`: validation/readback/export/logging path.

`VaapiVideoDiscovery::new` uses normal mode by default. Diagnostic callers must
explicitly construct the discovery object with diagnostic mode or use the
existing DRM PRIME diagnostic helper.

The normal path removes these operations from ordinary frame submission:

- per-frame CPU validation checksum
- per-frame DRM PRIME export probe
- per-frame diagnostic logging
- immediate sync solely for validation

The VA surface lifetime model was also made explicit. A decoded surface can be:

- retained by the H.264 DPB as a future reference
- held by an emitted QGS `VideoSurface`
- released by codec state but still pending safe VA surface reclamation

Because the current safe libva wrapper only returns surface ownership from an
ended picture after synchronization, qgs-vaapi now keeps released pictures in a
bounded pending-recycle list. It reclaims them only when the free pool is
exhausted or during decoder cleanup. This avoids immediate sync on every DPB
release while preserving safe surface reuse without adding raw VA-API FFI.

## Measurement Tooling

`qgs-test` adds `--proxy-throughput <original> <proxy>` for Step 13 local
diagnostics. It keeps the architecture intact:

```text
qgs-mp4
    |
AVC access units
    |
qgs-codec-h264
    |
qgs-vaapi normal or diagnostic decode
```

This mode does not use libavformat, does not invoke FFmpeg as the runtime
decoder, and does not modify the daemon protocol.

The measurement separates:

1. Container + H.264 frontend only: qgs-mp4 plus qgs-codec-h264, no pixel
   decode.
2. Clean VA hardware decode: qgs-vaapi normal mode, no checksum/export
   diagnostics.
3. Diagnostic VA hardware decode: qgs-vaapi diagnostic mode with checksum and
   DRM PRIME export probing.

## External Proxy Verification

Sony FX6 sample 002 proxy was rechecked locally:

- strict FFmpeg decode with `-xerror`: exit 0
- video packets: 106
- decoded/read frames: 106
- codec/profile: H.264 High
- format: 1920 x 1080, `yuv420p`, 50/1 fps

Hashes for the local external corpus were verified against the existing Step 12
constants. The media files remain outside the repository and were not copied or
committed.

## Results

Development observations on the current Intel HD Graphics 4600 / i965 machine:

| Path | Frames | Time | FPS | Realtime factor |
| --- | ---: | ---: | ---: | ---: |
| Container + H.264 frontend only | 106 | 0.060 s | 1755.17 | 35.10x |
| Clean VA hardware decode | 106 | 11.749 s | 9.02 | 0.18x |
| Diagnostic VA hardware decode | 106 | 86.573 s | 1.22 | 0.02x |

These numbers are DEVELOPMENT OBSERVATIONS, NOT BENCHMARKS.

Clean VA decode counters:

- diagnostics: 0 frames, 0 export probes
- pool allocation: 24 VA surfaces
- peak checked-out surfaces: 24
- recycled surfaces: 82
- recycle syncs: 82
- peak H.264 DPB occupancy: 3
- peak output pending: 2
- peak live VA surfaces tracked by codec state: 3

Diagnostic VA decode counters:

- diagnostics: 106 frames
- export probes: 106
- selected diagnostic checksums:
  - frame 0: `0x306273f9`
  - frame 53: `0x6d3e95c5`
  - frame 105: `0x6d3e95c5`
- DRM PRIME export probe: NV12, one DRM object, one layer, modifier
  `72057594037927938`

The clean path is now materially separated from the diagnostic path, but it is
not realtime on this Haswell/i965 stack. The remaining normal-path syncs come
from bounded surface-pool reclamation through the safe libva wrapper, not from
checksum/readback/export diagnostics.

## Damaged Sample 001

Sony FX6 sample 001 proxy remains classified as damaged/unrecoverable for
complete proxy playback:

- the MP4 declares 106 video samples
- video sample 97 contains a zero-length first AVC NAL
- qgs-mp4 strict normalization rejects it
- strict FFmpeg `-xerror` also fails
- frame-hash diagnostics produced only 96 decoded frames

QGS does not reinterpret zero-length NAL units as padding. The damaged packet
is not committed; synthetic tests preserve the strict policy.

## Safety

No unsafe Rust was added. `qgs-vaapi` remains `#![forbid(unsafe_code)]`.

The implementation deliberately uses the safe libva typestate API. Since that
API only permits reclaiming a submitted picture's surface after synchronization,
qgs-vaapi defers reclamation within a bounded pool instead of adding raw VA FFI.

The Vulkan unsafe inventory is unchanged.

## Tests

Added focused tests for:

- VA decode mode defaults
- diagnostic mode detection
- initial VA surface pool accounting

Quality gates:

- `cargo fmt --all -- --check`: passed
- `cargo clippy --workspace --all-targets -- -D warnings`: passed
- `cargo test --workspace`: passed
  - qgs-codec-h264: 8 tests
  - qgs-core: 26 tests
  - qgs-linux: 4 tests
  - qgs-mp4: 5 tests
  - qgs-mxf: 31 tests
  - qgs-protocol: 118 tests
  - qgs-software-video: 5 tests
  - qgs-test: 4 tests
  - qgs-vaapi: 11 tests
  - qgs-vulkan: 11 tests

Hardware/proof runs:

- H.264 VA regression through qgsd:
  - Intel HD Graphics 4600: VA-API H.264 IDR and 12-frame Long-GOP decode
    passed.
  - NVIDIA GTX 950M / NVK: no VA decode capabilities; software fallback
    remained clean.
- Step 12 proxy proof through qgsd:
  - proxy association and timing proof passed for Sony FX6 sample 002
  - proxy software reference decode: 106 frames
  - original software decode comparison: 106 frames
  - Intel proxy hardware decode: 106 frames, 11.871 s, 8.93 fps, 0.18x
    realtime at 50 fps
  - selected proxy frames processed through reusable GPU path on Intel and
    NVIDIA
  - sequential/random proxy frame 53 GPU checksum matched:
    `0x0c8466c6a8727f05`
  - VA -> Vulkan zero-copy remained frozen and unused
- Validation-enabled qgs-test regression:
  - external memory sharing passed on Intel and NVIDIA
  - external sync FD proof passed on Intel and NVIDIA
  - compute proof passed on Intel and NVIDIA
  - RGBA image processing proof passed on Intel and NVIDIA
  - reusable YUV422P10 GPU frame processor proof passed on Intel and NVIDIA
  - H.264 decode proof passed on Intel; NVIDIA remained software fallback

## Limitations

- Haswell/i965 proxy decode remains below realtime in this development proof.
- The current safe libva wrapper still requires synchronization before surface
  ownership can be reclaimed for reuse.
- Step 13 does not implement a playback scheduler, Qnc proxy switching, audio
  playback, VA -> Vulkan zero-copy, final presentation, or final performance
  policy.

## Recommendation

Proceed to a proxy-aware editing architecture only after defining scheduling
policy separately. The normal VA decode path is now clean enough to measure and
iterate on, but Haswell/i965 should not be treated as evidence of realtime proxy
playback readiness.

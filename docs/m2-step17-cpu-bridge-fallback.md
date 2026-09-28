# QGS M2 Step 17 CPU-Bridge NV12 Fallback

This document finalizes the M2 Step 17 exploratory work as a documented
fallback milestone, not as a final realtime Step 17 acceptance success.

The original Step 17 target was stable zero-drop 1080p50 playback of the Sony
FX6 sample 002 proxy through:

```text
qgs-mp4
  -> qgs-codec-h264
  -> qgs-vaapi hardware decode
  -> VA NV12 surface
  -> explicit CPU transfer
  -> CPU-backed QGS NV12 VideoSurface
  -> qgs-vulkan reusable frame processor
  -> test presentation sink
```

That stable realtime target did not pass on Intel Haswell/i965 through the
CPU bridge. The work is still valuable because it proves a bounded, correct
compatibility fallback path and isolates the remaining performance limit.

## Scope

VA -> Vulkan zero-copy remains frozen and was not used.

The fallback path uses:

- H.264 High 8-bit 4:2:0 proxy decode through qgs-vaapi
- explicit safe VA image transfer into QGS-owned CPU NV12 memory
- bounded CPU NV12 frame pool
- compact NV12 upload into qgs-vulkan
- bounded reusable GPU FrameSlots
- fixed internal NV12 -> RGBA-u16 GPU processing
- test presentation scheduling

No DMA-BUF import, DRM modifier import, external-memory video image import, or
VA/Vulkan zero-copy path is part of this milestone.

## Validated Results

The exploratory work established:

- The bounded VA decode -> CPU NV12 -> Vulkan -> presentation pipeline exists.
- CPU NV12 pool allocation remains bounded.
- GPU FrameSlot allocation remains bounded.
- The GPU processor reuses staging, input, output, command, fence, descriptor,
  and pipeline resources.
- Compact NV12 upload replaced the original u32-expanded input
  representation.
- Isolated compact NV12 -> Vulkan processing reaches hundreds of fps on the
  current Intel and NVIDIA devices.
- The remaining bottleneck is VA -> CPU transfer, specifically copying mapped
  VA image memory into QGS-owned NV12 CPU memory.
- The VA image for the sample is already tight, contiguous NV12.
- As-fast full pixel processing reaches about 50.66 fps, but with only about
  1% headroom over 50p.
- Stable zero-drop 1080p50 realtime playback is not achieved on Haswell/i965
  through this CPU bridge.

## Compact NV12 Upload

The first Step 17 implementation expanded CPU NV12 into a much larger u32 GPU
input representation before upload:

- CPU NV12 input: 3,133,440 bytes/frame
- old expanded GPU input: 10,444,800 bytes/frame
- output RGBA-u16: 16,588,800 bytes/frame

Step 17B removed the per-sample CPU expansion. The current compact input path:

- keeps Y and interleaved UV bytes compact
- row-copies compact bytes directly into reusable mapped staging memory
- stores packed bytes in storage buffers
- unpacks bytes in the fixed NV12 shader

Observed isolated compact NV12 -> Vulkan throughput:

- Intel HD Graphics 4600: about 337 fps in one run, about 332 fps in a later
  run
- NVIDIA GTX 950M / NVK: about 263 fps

The Vulkan input representation is no longer the limiting stage for this
fallback path.

## VA Image Layout

Step 17C inspected the actual VA-derived image for Sony FX6 sample 002 proxy:

- fourcc: NV12
- width/height: 1920 x 1088
- planes: 2
- data size: 3,133,440 bytes
- derived image: yes
- offsets: `[0, 2088960, 0]`
- pitches: `[1920, 1920, 0]`
- Y and UV planes are contiguous

The source pitch already equals the tight coded destination pitch. There is no
hidden 2048-byte row stride, unusual plane layout, or large padding region that
can be avoided by a simple safe layout change.

## Transfer Variants

Safe VA -> CPU transfer variants were measured using the same 106-frame proxy:

| Variant | Bytes/frame | Wall | Transfer | Throughput | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| tight-coded | 3,133,440 | 2104.871 ms | 1970.876 ms | 50.36 fps | GPU-compatible default |
| tight-visible | 3,110,400 | 2102.342 ms | 1972.175 ms | 50.42 fps | No material gain |
| source-pitch-coded | 3,133,440 | 2112.205 ms | 1979.339 ms | 50.18 fps | Same pitch as default |
| full-source-rows | 3,133,440 | 2085.549 ms | 1953.205 ms | 50.83 fps | Slight run-to-run win |

The visible-only copy removes only 23,040 bytes/frame, about 0.7%. It does not
change the conclusion.

The current best safe transfer path is a derived-image bulk copy into a
QGS-owned coded NV12 CPU frame. No unsafe qgs-vaapi code was added.

## Full Path Observation

A representative full compact playback run after Step 17B/17C produced:

- access units submitted: 106
- decoder outputs: 106
- GPU submissions/completions: 106 / 106
- presentation decisions: 106
- presented: 52
- on-time: 5
- late: 47
- dropped: 54
- duplicated: 0
- wall clock: 2.264 s for a 2.120 s source
- max lateness: 102.805 ms

The same code also produced an immediately previous run with 106 presented and
0 dropped, but the wall clock was still around 2.245 s for a 2.120 s source.
This variability confirms the CPU bridge is at the edge rather than providing
stable 50p headroom.

## As-Fast Observation

The as-fast path removes presentation sleeping but keeps VA decode, safe
VA -> CPU transfer, bounded CPU pool, compact Vulkan processing, and bounded
GPU FrameSlots.

Observed result:

- frames: 106
- wall time: 2092.192 ms
- throughput: 50.66 fps
- realtime factor relative to 50p: 1.01x
- decode total: 118.476 ms
- transfer total: 1821.375 ms
- GPU submit loop: 1967.167 ms, including transfer
- GPU completion: 4.586 ms
- copied bytes/frame: 3,133,440

The fallback path is barely above source rate in the as-fast run. That is not
enough margin for reliable realtime playback on this Haswell/i965 machine.

## External FFplay Observation

An external ffplay check was attempted on the same Sony FX6 sample 002 proxy:

```text
ffplay -hwaccel vaapi -sync video -stats "Sony FX6 sample 002 proxy"
```

This was a development observation only. It does not measure the QGS
CPU-bridge path because ffplay uses a different playback and rendering
pipeline.

Observed behavior:

- ffplay initialized the Vulkan/libplacebo renderer.
- It selected Intel HD Graphics 4600 / Haswell.
- It recognized the file as MP4 / XAVC brand.
- It recognized H.264 High, yuv420p / bt709 / progressive video.
- It reported 1920 x 1080, 50 fps video.
- It reported AAC stereo audio.
- It printed `MESA-INTEL: warning: Haswell Vulkan support is incomplete`.
- It printed `FINISHME: support more multi-planar formats with DRM modifiers`.
- It printed `Derive vaapi from vulkan not supported.`
- The run ended with `Segmentation fault`.

This ffplay VAAPI/Vulkan/libplacebo run did not prove stable 50p playback on
the Haswell/i965 system. It also does not prove anything directly about QGS
correctness or relative performance.

The observation is consistent with the QGS Step 17 conclusion: Haswell/i965
VAAPI + Vulkan interop is fragile, this machine should not be treated as the
production zero-copy reference platform, and QGS should keep the CPU bridge as
a compatibility fallback while moving production performance work to modern
Intel/AMD zero-copy validation or to an explicitly approved lower-level VA
transfer boundary.

## Decision

This work should be retained as a compatibility fallback milestone.

Keep:

- safe VA NV12 CPU transfer
- bounded CPU NV12 pool
- compact qgs-vulkan NV12 processor
- diagnostic and as-fast measurement paths behind explicit diagnostic flags

Do not claim realtime Step 17 acceptance success.

Production performance should move to a separate modern-hardware zero-copy
milestone, or to an explicitly approved lower-level VA transfer boundary if a
future audit justifies it. The CPU bridge remains useful for correctness,
diagnostics, portability, and systems where compatibility matters more than
stable 1080p50 realtime playback.

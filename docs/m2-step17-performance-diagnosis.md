# QGS M2 Step 17 Performance Diagnosis

Exploratory diagnosis for the uncommitted Step 17 implementation.

This is not the final Step 17 milestone report. The Step 17 realtime
acceptance target did not pass.

## Scope

The diagnosed path is:

```text
camera proxy MP4
  -> qgs-mp4
  -> qgs-codec-h264
  -> qgs-vaapi hardware decode
  -> VA NV12 surface
  -> explicit CPU transfer
  -> CPU-backed NV12 surface
  -> qgs-vulkan reusable NV12 processor
  -> fixed NV12 -> RGBA-u16 compute shader
  -> bounded presentation runtime
```

VA -> Vulkan zero-copy remains frozen and was not used.

## Test Clip

Privacy-safe identifier: Sony FX6 sample 002 proxy.

Technical properties:

- H.264 High
- 8-bit 4:2:0
- 1920 x 1080 visible
- 1920 x 1088 coded
- 50/1 fps
- 106 presentation frames
- 2.12 seconds

## Original Step 17 Failure

Before the compact-upload change, the bounded architecture worked but the full
pixel path was far too slow:

- playback wall clock: about 8.5 s for a 2.12 s source
- 1 on-time frame
- 105 dropped frames
- VA -> CPU transfer: about 2.06 s / 106 frames
- CPU NV12 -> u32 expansion: about 5.88 s / 106 frames
- isolated CPU NV12 -> Vulkan processor: about 17.32 fps

The original GPU input representation expanded compact NV12 into one `u32` per
Y sample plus one `u32` per UV chroma pair before staging.

For 1920 x 1088 coded NV12:

- CPU NV12 bytes/frame: 3,133,440
- old GPU input bytes/frame: 10,444,800
- old output bytes/frame: 16,588,800

The failed run was therefore dominated by CPU-side representation preparation,
not by fences, readback, or queue idle.

## Compact NV12 Upload Result

Step 17B changed only the qgs-vulkan NV12 input representation. VA -> CPU
transfer, playback scheduling, FrameSlots, fences, descriptor lifetime,
Haswell batch-retirement policy, and the output representation were preserved.

Chosen representation:

- storage-buffer input remains portable across the current Intel and NVIDIA
  Vulkan devices
- NV12 input bytes are packed into `uint` storage-buffer words
- the CPU path row-copies compact bytes directly into reusable mapped staging
  buffers
- the shader unpacks individual bytes from the packed words
- no Vulkan multi-planar image path, DMA-BUF import, DRM modifier import, or
  zero-copy path is used

This avoids requiring portable byte-addressable storage-buffer features while
removing the per-sample Rust conversion loops.

For 1920 x 1088 coded NV12:

- Y input: 2,088,960 bytes
- UV input: 1,044,480 bytes
- new staging/GPU input bytes/frame: 3,133,440
- output bytes/frame: 16,588,800

The qgs-vulkan diagnostic fields are named `compact_y_copy` and
`compact_uv_copy`; they measure compact row copy into staging, not u8-to-u32
expansion.

## Full Pixel Path After Compact Upload

`QGS_STEP17_DIAG=1 cargo run -q -p qgs-test -- --proxy-playback ...`

Result:

- 106 access units submitted
- 106 decoder outputs
- 106 GPU submissions
- 106 GPU completions
- 106 presentation decisions
- 61 presented frames
- 4 on-time frames
- 57 late frames
- 45 dropped frames
- 0 duplicated frames
- playback wall clock: 2.255 s for a 2.120 s source
- max lateness: 85.431 ms

The path is still bounded, but it is not yet a zero-drop 50p realtime path on
this machine.

## Queue And Resource Bounds

- compressed queue peak: bounded by the Step 16 playback runtime
- decoded queue peak: bounded by the Step 16 playback runtime
- GPU queue peak: 3
- CPU NV12 pool: 6 frames
- CPU NV12 pool bytes/frame: 3,133,440
- CPU NV12 pool reuses: 100
- GPU submissions: 106
- GPU slot reuses: 103

No unbounded frame allocation was observed.

## VA Decode And Transfer Breakdown

Full path aggregate after compact upload:

- MP4 open/sample extraction: 2.799 ms
- VA decoder creation: 0.893 ms
- VA decode submit/flush: 115.592 ms
- VA sync + CPU transfer: 1986.230 ms

Per-frame VA transfer distribution:

- total: 1986.230 ms
- average: 18.738 ms
- median: 18.655 ms
- p90: 19.966 ms
- max: 22.515 ms

Sub-stage totals:

- VA surface sync: 2.078 ms
- VA image derive/get: 1.614 ms
- Y-plane copy: 1479.740 ms
- UV-plane copy: 501.494 ms
- image release: 0.742 ms

Conclusion: VA synchronization is not the transfer bottleneck. CPU plane copy
dominates the VA -> CPU bridge.

## Vulkan-Side Breakdown After Compact Upload

Full path qgs-vulkan NV12 processor counters:

- submit calls: 106
- poll calls: 315
- fence wait calls: 1
- retire calls: 106
- validation readback: no

CPU preparation:

- slot acquisition: 0.156 ms
- compact Y row copy: 53.846 ms
- compact UV row copy: 27.179 ms
- separate staging write: 0.000 ms

Command and submission:

- command pool reset: 0.763 ms
- fence reset: 0.942 ms
- command begin: 0.399 ms
- command recording: 4.338 ms
- command end: 0.068 ms
- queue submit: 21.666 ms

Completion:

- poll: 1.750 ms
- fence wait: 2.318 ms
- retire: 0.301 ms

Conclusion: the compact path removed the dominant Vulkan-side CPU preparation
cost. Vulkan synchronization and submission are no longer the primary limiter.

## Isolated VA Decode + CPU Transfer

The isolated path decoded the same 106 access units, transferred each VA NV12
surface to a bounded CPU NV12 pool, and immediately released the CPU frame.
No Vulkan work was performed.

Result after compact-upload code:

- frames: 106
- wall time: 2302.264 ms
- throughput: 46.04 fps
- realtime factor vs 50p source: 0.92x
- decode aggregate: 111.553 ms
- transfer aggregate: 2168.028 ms
- pool reuses: 100

Transfer/frame:

- total: 2168.028 ms
- average: 20.453 ms
- median: 20.379 ms
- p90: 21.373 ms
- max: 22.177 ms

Conclusion: the safe VA -> CPU bridge alone is below 50p in this observation,
before Vulkan upload or processing is added.

## Isolated Synthetic CPU NV12 -> Vulkan

The isolated Vulkan path used a synthetic 1920 x 1088 coded / 1920 x 1080
visible CPU NV12 frame and processed it 106 times through the same bounded
NV12 frame processor. No VA, MP4, H.264 parsing, or playback clock was used.

Intel HD Graphics 4600:

- frames: 106
- wall time: 314.169 ms
- throughput: 337.40 fps
- realtime factor vs 50p source: 6.75x
- compact Y row copy: 120.208 ms
- compact UV row copy: 59.713 ms
- queue submit: 32.031 ms
- fence wait: 92.085 ms

NVIDIA GeForce GTX 950M / NVK:

- frames: 106
- wall time: 402.311 ms
- throughput: 263.48 fps
- realtime factor vs 50p source: 5.27x
- compact Y row copy: 152.355 ms
- compact UV row copy: 76.244 ms
- queue submit: 3.238 ms
- fence wait: 160.022 ms

The old isolated CPU NV12 -> Vulkan result was about 17.32 fps. The compact
input path raises isolated throughput to about 337 fps on Intel and about
263 fps on NVIDIA/NVK.

## Copy Graph After Compact Upload

Current effective copy/transform graph:

```text
VA surface
  -> libva image mapping/derive
  -> QGS-owned CPU NV12 pool frame
  -> mapped Vulkan staging Y bytes
  -> mapped Vulkan staging UV bytes
  -> device-local GPU Y/UV byte-packed buffers
  -> compute shader byte unpack
  -> GPU RGBA-u16 output
```

The temporary per-frame `Vec<u32>` Y and UV expansions have been removed from
the normal NV12 processing path.

## Readback And Synchronization

Timed playback did not perform validation readback.

Observed:

- validation readback enabled: no
- no per-frame checksum
- no CPU reference conversion
- no Vulkan validation during the timed run
- no device-wide idle or queue-wide idle in the normal frame path
- fence wait/completion overhead is small compared with VA transfer

Therefore the remaining realtime failure is not caused by GPU readback,
checksum, CPU reference conversion, validation layers, device idle, or queue
idle.

## Resource Creation

Observed processor counters confirm no per-frame creation of:

- shader module
- compute pipeline
- descriptor layout/pipeline layout
- staging buffers
- GPU input buffers
- output buffers
- command buffers

The reusable resource model is functioning.

## Root Cause Allocation After Step 17B

Approximate explanation of the remaining full-path 2.255 s run:

- VA decode submit/flush: 0.116 s, about 5%
- VA -> CPU transfer: 1.986 s, about 88%
- compact NV12 staging preparation: 0.081 s, about 4%
- Vulkan command/submit/completion: about 0.032 s, about 1%
- other measured overhead: small remainder

More than 90% of the relevant work time is now explained by VA decode plus
VA -> CPU transfer, and the transfer dominates.

## M2 Step 17C VA Transfer Result

Step 17C investigated only the safe VA -> CPU NV12 transfer path. It did not
change qgs-mp4, qgs-codec-h264, H.264 VA submission, the compact Vulkan NV12
representation, shaders, playback scheduling, queue capacities, GPU slot
lifecycle, or any zero-copy path.

### VA Image Layout

The derived VA image for Sony FX6 sample 002 proxy is already the ideal simple
NV12 layout for the current CPU bridge:

- fourcc: NV12
- width/height: 1920 x 1088
- planes: 2
- data size: 3,133,440 bytes
- derived image: yes
- offsets: `[0, 2088960, 0]`
- pitches: `[1920, 1920, 0]`
- Y and UV planes are contiguous
- source pitch equals the tight coded destination pitch

There is no hidden 2048-byte pitch or unusual plane separation to exploit. The
visible picture is 1920 x 1080, while the coded allocation remains 1920 x 1088.

### Copy Path Audit

The transfer path is safe Rust and uses mapped `Image` slices from the libva
wrapper. The current default destination is a QGS-owned tight coded NV12 frame:

- Y source base: mapped image offset 0
- UV source base: mapped image offset 2,088,960
- source pitch: 1920 bytes for both planes
- destination pitch: 1920 bytes for both planes
- Y copied rows: 1088
- UV copied rows: 544
- copied bytes/frame: 3,133,440

Because source pitch, destination pitch, and copied row width all match, Step
17C added a safe bulk-slice copy path for this contiguous case. No unsafe
qgs-vaapi code was added.

### Variants Tested

The same 106-frame proxy was decoded/transferred for each variant:

| Variant | Bytes/frame | Wall | Transfer | Throughput | Notes |
| --- | ---: | ---: | ---: | ---: | --- |
| tight-coded | 3,133,440 | 2104.871 ms | 1970.876 ms | 50.36 fps | GPU-compatible coded layout |
| tight-visible | 3,110,400 | 2102.342 ms | 1972.175 ms | 50.42 fps | Copies 1080 Y + 540 UV rows only |
| source-pitch-coded | 3,133,440 | 2112.205 ms | 1979.339 ms | 50.18 fps | Same as tight-coded because pitch is 1920 |
| full-source-rows | 3,133,440 | 2085.549 ms | 1953.205 ms | 50.83 fps | Same layout; fastest observation by a small margin |

The small differences are within normal run-to-run variance for this path.
Visible-only copying removes only 23,040 bytes/frame, about 0.7%, and did not
materially change throughput.

Representative sub-stage timings from the current full compact playback run:

- VA surface sync: 1.950 ms total
- VA image derive/get: 1.349 ms total
- Y copy: 1499.580 ms total
- UV copy: 497.335 ms total
- image release: 0.729 ms total
- transfer total: 2001.499 ms / 106 frames

The copy from mapped VA image memory remains the dominant cost. The safe libva
wrapper is not spending meaningful time in image derive/map/release, and sync
is negligible in the full playback pipeline where decode-ahead has already
made surfaces ready.

### Full Playback Retest

Current full path after Step 17B/17C instrumentation:

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
- VA decode submit/flush: 112.873 ms
- VA sync + CPU transfer: 2001.499 ms
- compact staging preparation: 82.771 ms
- GPU completion handling: 12.076 ms

An immediately previous run with the same code reached 106 presented / 0
dropped, but with wall clock still around 2.245 s for a 2.120 s source. This
confirms the CPU bridge is operating at the edge of 50p rather than providing
stable headroom.

### As-Fast Full Pixel Path

The as-fast path removes presentation sleeping but keeps:

- VA hardware decode
- safe VA -> CPU NV12 transfer
- bounded CPU pool
- compact Vulkan NV12 upload/processing
- bounded GPU FrameSlots

Result:

- frames: 106
- wall time: 2092.192 ms
- throughput: 50.66 fps
- realtime factor vs 50p source: 1.01x
- access units: 106
- decoder outputs: 106
- GPU submissions/completions: 106 / 106
- decode total: 118.476 ms
- transfer total: 1821.375 ms
- GPU submit loop: 1967.167 ms, including transfer
- GPU completion: 4.586 ms
- copied bytes/frame: 3,133,440

The fallback path is technically just over source rate in this observation, but
only by about 1%. That is not enough margin for stable zero-drop realtime
playback on this Haswell/i965 machine.

### Step 17C Recommendation

The best safe transfer path is the current derived-image bulk copy into a
QGS-owned coded NV12 CPU frame. The VA image is already tight and contiguous,
so destination pitch matching and visible-only copying do not reveal a major
safe optimization.

The CPU bridge should remain a compatibility fallback. On Haswell/i965 it is
at the edge of 1080p50 and cannot be treated as a stable realtime production
path with the current safe VA image mapping/copy model. Further work should
either investigate a lower-level VA copy/readback boundary with explicit
approval, or resume modern-hardware zero-copy as a separate milestone.

## Recommendation

E. COMBINATION:

1. keep the compact Vulkan upload representation
2. treat the safe VA -> CPU bridge as a compatibility fallback on Haswell
3. evaluate modern-hardware zero-copy or an approved lower-level VA transfer
   boundary as a separate milestone

The compact NV12 Vulkan input path fixed the previous primary bottleneck. The
Vulkan processor is no longer the limiting stage for this fallback path, and
Step 17C did not find a large remaining safe-Rust transfer optimization.

The remaining bottleneck is the CPU copy from mapped VA image memory into
QGS-owned NV12 memory. The VA image is derived, tight, and contiguous, so the
current safe copy is already close to the simplest CPU bridge. It can hover
around 50 fps in favorable runs, but it does not provide stable realtime
headroom for 1080p50 playback on this Haswell/i965 machine once decode, GPU
upload/processing, and scheduling are included.

Do not change synchronization policy first: fence waits, batch retirement, and
readback are not the source of the remaining limitation.

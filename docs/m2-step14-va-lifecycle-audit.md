# QGS M2 Step 14 Report: VA Surface Lifecycle and Decode Throughput Audit

## Summary

M2 Step 14 audited the Sony FX6 sample 002 proxy VA decode path without adding
raw VA-API FFI or changing the safe-Rust policy.

The clean qgs-vaapi path still decodes all 106 presentation frames correctly,
but the new timing counters show that the observed ~9 fps result is not caused
primarily by surface-pool recycle synchronization. In the measured clean run,
almost all time was spent inside the `vaEndPicture` wrapper call itself.

Root-cause classification:

```text
D. another QGS backend issue
```

The current safe libva wrapper does constrain surface reclamation, but that
constraint is not the measured dominant cost in this audit.

## 1. Clean Decode Call/Lifecycle Diagram

Clean mode uses this per-picture path:

```text
MP4 AVC access unit
    |
qgs-codec-h264 parse / POC / DPB / reference lists
    |
qgs-vaapi acquire VA surface from bounded pool
    |
build H.264 VA picture/IQ/slice buffers
    |
Picture::begin      -> vaBeginPicture
Picture::render     -> vaRenderPicture
Picture::end        -> vaEndPicture
    |
store PictureEnd surface state
    |
qgs-codec-h264 finish_picture
    |
emit output VideoSurface when display-ready
    |
release DPB references when codec state permits
    |
defer released VA surface to pending recycle
    |
sync only when pool must reclaim a surface or during cleanup
```

Clean mode performs no per-frame checksum, DRM PRIME export, or diagnostic
surface readback.

## 2. Exact Synchronization Points

Clean mode synchronization points:

- Reclaim sync: `Picture<PictureEnd>::sync()` or `Surface::sync()` only when a
  DPB/output-released surface is reclaimed from the pending recycle list.
- Drop cleanup sync: the pool drains pending recycled surfaces on decoder drop.

Diagnostic mode additionally performs:

- inline sync immediately after `vaEndPicture`
- CPU validation checksum
- DRM PRIME export probe

For the clean run, there were no checksum/export diagnostic syncs.

## 3. Aggregate Timing By VA Operation Category

Sony FX6 sample 002 proxy, Intel HD Graphics 4600 / i965:

```text
Container + H.264 frontend only:
  frames: 106
  0.066 s, 1611.33 fps, 32.23x realtime

Clean VA hardware decode:
  frames: 106
  11.762 s, 9.01 fps, 0.18x realtime

Clean aggregate timing:
  surface acquisition:    0.911 ms
  H.264 frontend parse:  65.762 ms
  parameter/list build:   7.618 ms
  VA buffer creation:     2.415 ms
  vaBeginPicture:         0.097 ms
  vaRenderPicture:        0.213 ms
  vaEndPicture:       11677.919 ms
  inline surface sync:    0.000 ms
  reclaim surface sync:   0.259 ms
  diagnostics:            0.000 ms
  finish picture:         0.856 ms
  output mapping:         0.352 ms
  release/drop:           0.290 ms
  flush:                  0.003 ms
  submit total:       11757.668 ms
```

`vaEndPicture` accounts for about 99.3% of clean submit time.

Diagnostic contrast:

```text
Diagnostic VA hardware decode:
  frames: 106
  98.366 s, 1.08 fps, 0.02x realtime

Diagnostic aggregate timing:
  inline surface sync: 46690.815 ms
  diagnostics:        51530.788 ms
  vaEndPicture:          18.246 ms
  submit total:       98356.458 ms
```

Diagnostic mode is intentionally slow and is not the production path.

## 4. Sync Count

Clean run counters:

```text
surfaces allocated:        24
surfaces recycled:         82
recycle syncs:             82
deferred recycle events:   106
peak checked-out surfaces: 24
peak pending recycle:      23
minimum free surfaces:     0
diagnostic frames:         0
export probes:             0
```

The clean path performed 82 recycle syncs, but their aggregate measured time was
only 0.259 ms. They are not the dominant throughput cost in this run.

## 5. Maximum Outstanding Decode Submissions

The instrumented clean path reached:

```text
peak submitted-unsynchronized surfaces: 24
```

This means QGS can retain many ended pictures without explicitly syncing them
for reclamation. However, because `vaEndPicture` itself consumed nearly all wall
time, those outstanding objects do not currently translate into high decode
throughput on this QGS path.

## 6. Surface Ownership Lifecycle

The qgs-vaapi surface lifecycle is:

```text
available Surface
    -> decode target
    -> DecodedVaSurface::Submitted(PictureEnd)
    -> retained by DPB/output resource state
    -> codec/output released
    -> pending_recycle
    -> sync on reclaim
    -> available Surface
```

Display/output lifetime and reference lifetime remain distinct. The throughput
tool releases outputs immediately after counting selected metadata; it does not
retain a client-side backlog.

## 7. Wrapper-Imposed Constraints

QGS uses `libva 0.1.4` with `linked` mode.

The wrapper typestate model defines:

```text
PictureNew -> PictureBegin -> PictureRender -> PictureEnd -> PictureSync
```

The wrapper's `PictureReclaimableSurface` trait is implemented only for
`PictureNew` and `PictureSync`. Therefore, after `vaEndPicture`, QGS cannot take
ownership of the underlying `Surface<()>` until `PictureEnd::sync()` has
produced `PictureSync`.

The same wrapper exposes `Surface::query_status()`, but that does not make a
`PictureEnd` surface reclaimable by typestate. Within this safe API, polling
status cannot replace sync for returning the surface to the pool.

## 8. Underlying VA-API Requirements

The local system headers are from libva 2.22.0 / VA-API 1.23. Relevant
contracts in `/usr/include/va/va.h` state:

- `vaEndPicture` is non-blocking and the client can start another
  Begin/Render/End sequence on a different render target.
- `vaSyncSurface` blocks until pending operations on a render target complete;
  after it returns, it is safe to use the render target for a different picture.
- `vaExportSurfaceHandle` does not perform synchronization; if contents will be
  read, the client must call `vaSyncSurface`.
- VA functions are thread-safe at the library level, but applications must
  synchronize their use of shared VA objects to get expected results.

So, VA-API itself permits multiple submitted pictures on different render
targets before explicit surface synchronization. It requires synchronization for
surface reuse/readback, not for merely submitting the next picture.

## 9. Wrapper vs VA Distinction

VA-API distinction:

```text
submit picture to driver        -> vaEndPicture, intended non-blocking
reuse/read completed surface    -> vaSyncSurface
```

Current wrapper distinction:

```text
PictureEnd may be retained safely
Surface ownership cannot be reclaimed until PictureSync
```

QGS already works within that wrapper model by deferring reclaim syncs. The
audit shows the wrapper's reclaim constraint is real but not the clean-path
time sink.

## 10. Pool-Size Experiment Results

All pool-size runs decoded 106 / 106 frames correctly:

| Pool | Time | FPS | Recycle syncs | Peak pending recycle | Peak submitted unsynced | Min free | Max client-held outputs |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 12.036 s | 8.81 | 98 | 7 | 8 | 0 | 1 |
| 12 | 11.969 s | 8.86 | 94 | 11 | 12 | 0 | 1 |
| 16 | 12.026 s | 8.81 | 90 | 15 | 16 | 0 | 1 |
| 24 | 11.991 s | 8.84 | 82 | 23 | 24 | 0 | 1 |
| 32 | 11.954 s | 8.87 | 74 | 31 | 32 | 0 | 1 |

Increasing the pool reduced recycle sync count but did not materially improve
throughput. This argues against simple pool exhaustion as the primary cause.

## 11. Client-Output Retention Result

The throughput mode releases each output as soon as it is counted. Measured:

```text
max client-held outputs: 1
peak H.264 DPB occupancy: 3
peak output pending: 2
peak codec-state live VA surfaces: 3
```

The test is not accidentally benchmarking a client-held output backlog.

## 12. Approximate Surface Memory Observations

For a 1920 x 1080 NV12 proxy frame:

- visible NV12 bytes: 1920 x 1080 x 1.5 = 3,110,400 bytes
- diagnostic DRM PRIME object size observed from i965: 3,133,440 bytes

Approximate i965 object memory by pool size:

| Pool | Approximate VA surface memory |
| ---: | ---: |
| 8 | 25.1 MB |
| 12 | 37.6 MB |
| 16 | 50.1 MB |
| 24 | 75.2 MB |
| 32 | 100.3 MB |

More surfaces are not free, and in this audit they did not improve throughput.

## 13. Independent VA Decoder Comparison

An independent FFmpeg VA-API decode sanity check was run against the same local
proxy copy and the same Intel render node.

Observed:

```text
VAAPI driver: Intel i965 driver for Intel(R) Haswell Mobile - 2.4.1
decoded frames: 106
decode errors: 0
wall time with VAAPI output frames: about 0.20 s
```

The verbose FFmpeg run showed `pix_fmt: vaapi` for the decoded output and
reported all 106 video frames decoded.

FFmpeg/libavformat/libavcodec remain development reference tools only. QGS does
not use them for MP4 parsing or VA runtime decode here.

## 14. QGS vs Reference Throughput

Development observations:

| Path | Frames | Time | FPS / speed |
| --- | ---: | ---: | ---: |
| QGS qgs-mp4 + qgs-codec-h264 frontend only | 106 | 0.066 s | 1611 fps |
| QGS clean qgs-vaapi decode | 106 | 11.762 s | 9.01 fps |
| FFmpeg VAAPI reference decode | 106 | ~0.20 s | ~530 fps |

The same Haswell/i965 stack can decode this proxy materially faster through an
independent implementation. Haswell/i965 alone is therefore not the primary
limit shown by this audit.

## 15. Root-Cause Classification

Classification:

```text
D. another QGS backend issue
```

Evidence:

- Container and H.264 frontend work is already much faster than decode.
- Clean mode performs no diagnostic checksum/export work.
- Reclaim sync is measured at sub-millisecond aggregate time.
- Increasing the pool from 8 to 32 surfaces does not materially change
  throughput.
- FFmpeg VAAPI uses the same i965 stack and decodes the same clip much faster.
- The measured hot category is QGS's `vaEndPicture` call path.

The exact low-level difference between QGS's H.264 VA submission and FFmpeg's
submission remains to be isolated. Likely next investigation areas are VA H.264
parameter contents, reference picture fields, slice buffer layout, buffer
lifetime, and driver behavior around QGS's one-context submission pattern.

## 16. Whether Current Safe Wrapper Remains Suitable

For this milestone, yes.

The wrapper imposes a conservative reclaim model, but the measured bottleneck
is not reclaim synchronization. A raw VA backend is not justified by the current
evidence.

The wrapper remains suitable for the current safe decode path while QGS audits
the actual VA submission semantics.

## 17. Alternative Safe-Wrapper Findings

Crates.io inspection on this machine showed:

- `libva 0.1.4`: current QGS dependency; latest visible `libva` crate.
- `cros-libva 0.0.13`: older ChromeOS-lineage safe wrapper.
- `cros-libva-extended 0.0.13-extended.2`: older fork/extension.
- `fev 0.2.3`: high-level VA-API bindings, not a drop-in replacement for QGS's
  current H.264/DPB-owned architecture.
- `cros-codecs-generic-vaapi 0.0.6-generic.2`: codec/backend stack, not merely
  a safer surface lifecycle wrapper.

No alternative safe wrapper was identified that clearly solves the measured
`vaEndPicture` bottleneck without a larger architecture migration.

## 18. Proposed Raw-VA Boundary

No raw-VA boundary is proposed in Step 14.

The audit did not justify adding QGS-owned unsafe FFI merely to avoid wrapper
surface-reclaim synchronization. If future evidence proves that QGS needs a
VA-API operation unavailable in the safe wrapper, a separate approval step
should define the exact C APIs, ownership model, and safety contract.

## 19. Safety Implications

No unsafe Rust was added. `qgs-vaapi` remains:

```rust
#![forbid(unsafe_code)]
```

The Vulkan unsafe inventory is unchanged.

The new instrumentation uses safe `Instant` timing counters and aggregate
statistics only. It does not print per-frame timing logs in clean mode.

## 20. Recommendation

The next VA work should compare QGS's H.264 VA submission against a known-good
VA implementation at the parameter/buffer/reference level before considering
raw FFI or wrapper migration. Specifically, investigate why QGS's
`vaEndPicture` call blocks for almost all clean-path wall time when the VA API
documents it as non-blocking and FFmpeg's VAAPI path is much faster on the same
driver.

KEEP CURRENT SAFE LIBVA PATH

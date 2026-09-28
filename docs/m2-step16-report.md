# QGS M2 Step 16 Report: Bounded 50p Playback Pipeline

## Summary

M2 Step 16 adds `qgs-media-runtime`, a safe Rust playback-runtime foundation
for exact media timing, bounded queues, preroll, presentation decisions, and
test-clock scheduler coverage. `qgs-test --proxy-playback` proves the clean
Sony FX6 sample 002 proxy through:

```text
qgs-mp4
    -> qgs-codec-h264
    -> qgs-vaapi
    -> bounded decoded-frame queue
    -> bounded GPU scheduling queue
    -> bounded presentation queue
    -> TestPresentationSink
```

The timed acceptance run produced 106 / 106 presentation decisions at 50/1 fps
with zero drops and zero duplicates. VA -> Vulkan zero-copy remains frozen; the
normal timed path therefore does not claim per-frame pixel GPU processing of VA
surfaces. Selected-frame pixel validation continues to use the existing
CPU-backed proxy bridge into the reusable GPU frame processor.

## 1. Playback Runtime Architecture

`crates/qgs-media-runtime` contains backend-neutral playback primitives:

- `RationalRate` for exact reduced integer rate math.
- `RealTimeClock` for monotonic wall-clock playback.
- `TestClock` for deterministic tests without sleeping.
- `BoundedQueue<T>` with peak-depth and backpressure counters.
- `TestPresentationSink` with recorded presentation decisions.
- `PlaybackConfig` for queue capacities, preroll, and late/drop thresholds.

The runtime crate has `#![forbid(unsafe_code)]`.

## 2. Crate / Module Placement

Playback scheduling was not placed in `qgs-mp4`, `qgs-mxf`, `qgs-codec-h264`,
`qgs-vaapi`, or `qgs-vulkan`. Backend crates remain backend-focused.

`qgs-test` owns the Step 16 local proof command:

```text
--proxy-playback <original-mxf> <proxy-mp4>
```

No qgsd protocol playback commands were added.

## 3. Playback State Machine

The runtime exposes a minimal state model:

```text
Idle
Prerolling
Playing
Draining
Completed
Failed
```

The Step 16 command uses `Prerolling -> Playing -> Completed`. Seek, pause,
jog, shuttle, GUI transport, and Qnc timeline states are intentionally absent.

## 4. Clock Model

The real acceptance command uses `RealTimeClock`, backed by a monotonic
`Instant`. Unit tests use `TestClock`, which advances deterministically and does
not sleep.

Video is the Step 16 timing source, but the clock model does not require video
to remain the permanent master. Future audio/master-clock integration can add a
different clock authority above the same presentation-time model.

## 5. Exact Rational Timestamp Model

`RationalRate::new(numerator, denominator)` validates non-zero terms and
reduces equivalent rates for reporting. Presentation time is computed with
checked integer nanosecond arithmetic:

```text
offset_ns = presentation_position * denominator * 1_000_000_000 / numerator
```

Synthetic tests cover:

- 50/1 = 20 ms per frame
- 25/1 = 40 ms per frame
- 30000/1001 rational timing
- reduced MP4 proxy rate `5300000/106000 -> 50/1`

## 6. Preroll Policy

Default proof configuration:

```text
preroll frames: 3
presentation queue capacity: 4
```

The playback clock starts after the presentation-ready queue reaches the
bounded preroll threshold, not after the whole clip is decoded.

## 7. Queue Capacities

Default Step 16 proof capacities:

```text
compressed/decode input: 8
decoded frames:          6
GPU scheduling queue:    3
presentation-ready:      4
```

These are local proof defaults, not protocol constants.

## 8. Backpressure Propagation

All queues are bounded. A full queue returns backpressure instead of allocating
another unbounded slot. In the real acceptance run, stage throughput exceeded
50p, so no backpressure events occurred, but the peaks reached the configured
bounds:

```text
queue peaks: compressed=8 decoded=6 gpu=3 presentation=4
backpressure events: compressed=0 decoded=0 gpu=0 presentation=0
```

Unit tests cover bounded growth, decoder-faster-than-presentation pressure,
temporary GPU pressure, and slow-stage drop classification.

## 9. VA Surface Ownership Through Playback

Decoded VA `VideoSurface` resources are carried through the decoded,
GPU-scheduling, and presentation queues. This preserves client/output
ownership separately from codec DPB/reference lifetime.

Clean VA ownership observation from the same proxy:

```text
pool allocated:              24
pool reused:                 82
pool recycled:               82
recycle syncs:               82
deferred recycle events:     106
peak checked out surfaces:   24
peak pending recycle:        23
minimum free surfaces:       0
peak DPB:                    3
peak output pending:         2
peak live VA surfaces:       3
peak submitted unsynced:     24
max client-held outputs:     1
```

## 10. GPU FrameSlot Integration

The existing Step 11 `GpuFrameProcessor` remains intact and was regression
tested through the default qgs-test hardware path. Both Intel and NVIDIA/NVK
processed the synthetic 6-frame / 3-slot sequence successfully with:

```text
frames submitted: 6
slot reuses:      3
pipelines:        1
shaders:          1
command buffers:  3
```

For the real-time proxy path, VA -> Vulkan zero-copy remains frozen and no
normal per-frame VA readback bridge was introduced. The timed path therefore
uses a bounded GPU scheduling queue but does not perform per-frame pixel GPU
processing of VA surfaces. Selected-frame pixel validation uses the existing
CPU-backed proxy bridge and `GpuFrameProcessor`.

## 11. Presentation Sink

`TestPresentationSink` records:

- expected presentation time
- actual presentation time
- lateness
- frame identity
- presented/late/dropped/duplicated status

No real display, Wayland, X11, swapchain, or UI was added.

## 12. Late / Drop Policy

Default policy:

```text
on-time tolerance: 5 ms
drop threshold:   40 ms
```

Frames later than the on-time tolerance are marked `Late`; frames later than
the drop threshold are marked `Dropped`. The acceptance run produced no late or
dropped frames.

## 13. EOS / Drain Behavior

The playback loop handles:

- all compressed samples submitted
- decoder flush
- delayed presentation outputs
- remaining queued frames
- final sink decision accounting

Acceptance requires:

```text
presented + late + dropped + duplicated decisions == source presentation frames
```

For the clean proxy:

```text
source frames:          106
presentation decisions: 106
```

## 14. Threading / Worker Model

Step 16 uses a single-threaded bounded pump in qgs-test. This keeps ownership
and backpressure explicit while proving scheduler semantics. It does not create
a thread per stage or introduce a permanent async runtime.

## 15. Shutdown Behavior

The local playback proof drops queued decoded surfaces only after presentation
decisions are recorded. qgsd session cleanup and existing backend resource Drop
behavior were regression tested separately by the default qgs-test path.

## 16. Real-Time Acceptance Result

External media remained outside the repository and was verified by local hash.
Committed documentation uses privacy-safe sample names only.

```text
proxy: H.264 High, 8-bit 4:2:0
visible: 1920 x 1080
coded:   1920 x 1088
rate:    50/1
frames:  106
target duration: 2.120 s
```

Real-time run:

```text
state: Completed
access units submitted: 106
decoder outputs:        106
GPU queue submissions:  106
GPU queue completions:  106
presentation decisions: 106
```

## 17. Presented / Dropped / Duplicated Counts

```text
presented decisions: 106
on-time:             106
late:                0
dropped:             0
duplicated:          0
```

## 18. Lateness Observations

Development observation:

```text
playback wall-clock: 2.132 s
target duration:     2.120 s
max lateness:        0.146 ms
mean lateness:       0.101 ms
median lateness:     0.099 ms
```

This is not a benchmark.

## 19. Queue Peaks

```text
compressed queue peak:   8 / 8
decoded queue peak:      6 / 6
GPU queue peak:          3 / 3
presentation queue peak: 4 / 4
```

The peaks show bounded decode-ahead and bounded presentation buffering. Queue
capacities were not exceeded.

## 20. VA Surface Peaks / Reuse

The clean VA observation for the same access units showed:

```text
VA surfaces allocated: 24
VA surfaces reused:    82
peak checked out:      24
peak live codec state: 3
peak output pending:   2
```

This remains compatible with Step 15's corrected VA submission path.

## 21. GPU Slot Reuse / Backpressure

The Step 11 reusable GPU regression remains passing:

```text
Intel:  6 frames, 3 slots, slot reuses=3, validation PASS
NVIDIA: 6 frames, 3 slots, slot reuses=3, validation PASS
```

The timed proxy path used the bounded GPU scheduling queue but did not call the
pixel GPU processor per VA frame because no approved VA-surface CPU bridge or
zero-copy path exists yet.

## 22. Stage Timing Observations

DEVELOPMENT OBSERVATION - NOT A BENCHMARK:

```text
MP4 open/sample extraction:       3.371 ms
VA decoder creation:              1.098 ms
VA decode submit/flush aggregate: 160.812 ms
GPU scheduling aggregate:         0.178 ms
presentation wait aggregate:      1947.177 ms
```

The timing confirms Step 15's result that clean VA decode is now comfortably
faster than 50p for this proxy.

## 23. Selected-Frame Correctness

Selected diagnostic validation used software proxy decode plus the existing
CPU-backed bridge into `GpuFrameProcessor`. It was not part of the timed path.

```text
requested first:  actual presentation=1   max_delta=1
requested middle: actual presentation=53  max_delta=1
requested final:  actual presentation=105 max_delta=1
```

The first software-reference output carries presentation timestamp 1 rather
than 0 from libavcodec metadata. The timed VA path still produced 106 ordered
presentation decisions; this timestamp-origin quirk remains a diagnostic-only
validation detail.

## 24. Synthetic Scheduler Tests

`qgs-media-runtime` adds tests for:

- exact 50/1 presentation timing
- 25/1 timing
- 30000/1001 timing
- reduced equivalent rates
- bounded queues
- backpressure
- preroll capacity validation
- faster-decoder pressure
- temporary GPU pressure
- slow-stage drop classification
- EOS-style drain ordering
- frame identity preservation
- deterministic test clock behavior

## 25. Damaged-Media Regression Status

The damaged Sony FX6 sample 001 proxy semantics from Step 12 were not changed.
Strict zero-length AVC NAL rejection remains in qgs-mp4 tests.

## 26. Tests / Results

Quality gates run:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Workspace tests:

```text
237 tests/doc-tests passed
```

Hardware proofs:

- existing daemon-backed H.264 VA hardware regression: passed
- default qgs-test GPU/resource/reusable processor regression: passed
- Step 16 real-time proxy playback acceptance: passed

## 27. fmt / clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## 28. Unsafe Inventory

No unsafe code was added outside `qgs-vulkan`.

`qgs-media-runtime` uses `#![forbid(unsafe_code)]`.

The existing qgs-vulkan unsafe inventory is unchanged by Step 16.

## 29. Commit / Push Verification

Commit message:

```text
Implement QGS bounded realtime playback pipeline
```

Final push and `main == origin/main` verification are performed after this
report is committed, because recording a final pushed commit SHA inside the
same commit would change the SHA being recorded.

## 30. Limitations

- No real display presentation.
- No audio decode or audio master clock.
- No Qnc timeline, project, clip, UI, proxy switching, seek, jog, or shuttle.
- No per-frame pixel GPU processing of VA proxy surfaces in the normal timed
  path.
- VA -> Vulkan zero-copy remains frozen.
- The selected-frame GPU validation uses software decode / CPU bridge and is
  diagnostic only.
- The playback command is a qgs-test proof, not a qgsd protocol feature.

## 31. Recommendation for Audio / Master-Clock Integration

Next, add a media-runtime clock-authority layer that can accept a future audio
clock without making video presentation timestamps the permanent master. After
that, design a clean CPU-backed NV12 upload path or an approved hardware-surface
GPU ingestion path so normal proxy playback can include real per-frame GPU
pixel processing without reopening the frozen Haswell zero-copy experiment.

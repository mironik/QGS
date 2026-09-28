# QGS M2 Step 18A Broadcast 50i Journalist Preview Profile

M2 Step 18A adds an explicit broadcast preview profile for journalist/news
workflows targeting 1080i50-style delivery while using the existing bounded
fallback path.

This is not a full 50i renderer. It does not implement field rendering,
field cadence, deinterlacing, or interlaced output. It is a conservative
50i-compatible broadcast journalist preview over the existing progressive Sony
FX6 sample 002 proxy path. The processing workload is 25 frame periods/s.

## Why This Profile Exists

M2 Step 17 proved a bounded and correct CPU-bridge fallback path:

```text
qgs-mp4
  -> qgs-codec-h264
  -> qgs-vaapi hardware decode
  -> VA NV12 surface
  -> safe CPU NV12 transfer
  -> compact qgs-vulkan NV12 processor
  -> bounded presentation runtime
```

That path does not provide stable zero-drop 1080p50 playback on Intel
Haswell/i965 because the safe VA -> CPU bridge has almost no headroom at
50 frames/s.

Many TV/news delivery workflows still target 1080i50, which has 50 fields/s
but 25 frame periods/s. Step 18A demonstrates a practical
50i-compatible journalist preview profile where the processing workload is 25
frame periods/s while using the same proven bounded fallback path.

## Profile Semantics

The profile is selected in qgs-test with:

```text
--proxy-playback-profile journalist-50i-preview
```

The normal source-rate path remains available as:

```text
--proxy-playback-profile source-rate
```

For `journalist-50i-preview`:

- all 106 H.264 access units are submitted to the VA decoder
- all 106 decoder outputs are produced
- every second source presentation frame is selected for preview
- the other source frames are intentionally skipped by the profile
- intentional skips are reported separately from lateness drops
- the presentation clock runs at 25/1 fps
- the selected 53 presentation frames cover the same 2.12 s source duration

This is a 50i-compatible broadcast journalist preview profile. It uses a 25
frame-period/s progressive processing workload as a conservative preview
representation for 50i news workflows. It is not full interlaced output.

## Measured Result

Test input: Sony FX6 sample 002 proxy.

Source properties:

- H.264 High
- 8-bit 4:2:0
- 1920 x 1080 visible
- 1920 x 1088 coded
- 50/1 fps source
- 106 source presentation frames
- 2.12 s duration

Command:

```text
cargo run -q -p qgs-test -- --proxy-playback-profile journalist-50i-preview --proxy-playback "Sony FX6 sample 002" "Sony FX6 sample 002 proxy"
```

Observed playback result:

- source access units submitted: 106
- source decoder outputs: 106
- selected presentation frames: 53
- intentionally skipped source frames: 53
- GPU submissions/completions: 53 / 53
- presentation decisions: 53
- presented frames: 53
- on-time frames: 53
- late frames: 0
- lateness drops: 0
- duplicated frames: 0
- playback wall clock: 2.256 s
- target duration: 2.120 s
- max lateness: 0.141 ms

Queue/resource result:

- compressed queue peak: 8
- decoded queue peak: 6
- GPU queue peak: 3
- presentation queue peak: 4
- CPU NV12 pool capacity: 6 frames
- CPU NV12 pool peak checked out: 1
- CPU NV12 pool reuses: 47
- GPU slot reuses: 50

Stage timing observations:

- MP4 open/sample extraction: 2.829 ms
- VA decoder creation: 1.011 ms
- VA decode submit/flush aggregate: 124.568 ms
- VA sync + CPU transfer aggregate: 981.786 ms
- compact Vulkan processor CPU prep: 41.298 ms
- Vulkan completion handling aggregate: 1.044 ms
- presentation wait aggregate: 1032.907 ms

The selected frames were presented without lateness drops or duplicates. The
intentional source-frame skips are profile behavior, not playback failure.

## Interpretation

The journalist 50i-compatible preview profile demonstrates that the existing
bounded CPU-bridge fallback can support a stable news-preview workload on Intel
Haswell/i965 when the processing workload is 25 frame periods/s rather than
full 50 progressive frames/s.

This does not change the Step 17 conclusion. The CPU bridge remains a
compatibility fallback and should not be treated as the production full-50p
performance path. Full 1080p50 production playback still needs a separate
modern-hardware zero-copy milestone or an explicitly approved lower-level VA
transfer boundary.

## QNC Journalist Relevance

This profile gives QNC Journalist a realistic early demonstration mode:

- real camera-generated proxy media
- hardware H.264 proxy decode
- bounded CPU bridge
- bounded GPU processing
- deterministic presentation clock
- explicit skipped-frame accounting
- no zero-copy dependency on Haswell

It is suitable for conservative broadcast preview demonstrations, while the
final production playback path remains future work.

## Limitations

- Not a full 50i renderer.
- No field rendering.
- No audio playback.
- No real display/presenter.
- No automatic proxy/original switching.
- No VA -> Vulkan zero-copy.
- Does not make Haswell/i965 the production zero-copy reference platform.

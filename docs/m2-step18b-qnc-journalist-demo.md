# QGS M2 Step 18B QNC Journalist Demo

M2 Step 18B packages existing QGS media primitives into the first
product-shaped QNC Journalist workflow demonstration.

This is not a GUI, timeline editor, transport controller, audio player, or
export engine. It is a deterministic qgs-test command that demonstrates how
camera media, proxy association, bounded preview playback, and a simple edit
decision summary fit together for a journalist/news workflow.

## Command

```text
cargo run -q -p qgs-test -- --qnc-journalist-demo "Sony FX6 sample 002" "Sony FX6 sample 002 proxy"
```

The command uses privacy-safe labels in output and documentation. It does not
print private camera serial metadata, private filesystem paths, or private
production metadata.

## How It Connects Prior Work

Step 12 proved original/proxy association for the Sony FX6 sample 002 camera
corpus.

Step 17 proved the bounded CPU-bridge fallback:

```text
VA NV12 surface
  -> safe CPU NV12 transfer
  -> compact qgs-vulkan NV12 processing
```

Step 18A proved a stable journalist 50i-compatible broadcast preview profile on
Intel Haswell/i965:

- 50p source proxy
- every second source frame selected
- intentional profile skips reported separately from lateness drops
- 53 selected preview frames
- 0 lateness drops
- 0 duplicates

Step 18B packages those pieces into a QNC-shaped flow:

```text
camera media
  -> original/proxy association
  -> journalist-50i-preview profile
  -> frame/time selection accounting
  -> simple edit-decision summary
  -> export-plan stub
```

## Demo Flow

The demo performs:

1. Hash-based identification of the local Sony FX6 sample 002 corpus.
2. MXF inspection through qgs-mxf for the original media.
3. MP4/proxy inspection through qgs-mp4.
4. H.264 classification through qgs-codec-h264.
5. The Step 18A journalist-50i-preview acceptance path.
6. A deterministic news cut from 0.400 s to 1.600 s.
7. A structured export-plan stub.

No real export is attempted.

## Measured Haswell/i965 Result

Input:

- Sony FX6 sample 002 original/proxy pair
- proxy video: H.264 High, 8-bit 4:2:0
- visible size: 1920 x 1080
- coded size: 1920 x 1088
- source rate: 50/1 fps
- source frames: 106

Preview profile:

- `journalist-50i-preview`
- broadcast target: 1080i50-compatible news preview
- processing workload: 25 frame periods/s
- true interlaced output: not implemented in this milestone
- selected presentation frames: 53
- intentionally skipped source frames: 53
- presentation clock: 25/1 fps

Observed result:

- access units submitted: 106
- decoder outputs: 106
- GPU submissions/completions: 53 / 53
- presentation decisions: 53
- presented frames: 53
- on-time frames: 53
- late frames: 0
- lateness drops: 0
- duplicated frames: 0
- max lateness: 0.150 ms
- playback wall clock: 2.265 s
- target duration: 2.120 s

Resource bounds:

- compressed queue peak: 8
- decoded queue peak: 6
- GPU queue peak: 3
- presentation queue peak: 4
- CPU NV12 pool capacity: 6 frames
- CPU NV12 peak checked out: 1
- CPU NV12 pool reuses: 47
- GPU slot reuses: 50

Stage timing observations:

- MP4/sample extraction: 2.620 ms
- VA decode submit/flush: 123.761 ms
- VA -> CPU transfer: 981.477 ms
- compact Vulkan CPU prep: 42.960 ms
- Vulkan completion handling: 1.025 ms
- presentation wait: 1041.484 ms

## News Cut

The deterministic demo cut is:

- start: preview frame 10 / source frame 20 / 0.400 s
- end: preview frame 40 / source frame 80 / 1.600 s
- preview frames in cut: 30
- selected source frames in cut: 30
- source frames skipped by profile in cut: 30
- estimated duration: 1.200 s

This is an edit-decision summary only. It does not create a timeline object or
render a file.

## Export Plan Stub

The demo prints:

- status: planned only, not rendered
- profile: journalist-50i-preview
- source: Sony FX6 original/proxy pair
- preview media: proxy MP4
- finishing media: original MXF available
- selected range: 0.400 s..1.600 s
- target delivery: future milestone

This confirms that QGS can represent the product intent without claiming export
functionality that does not exist yet.

## QNC Journalist Value

The demo supports the QNC Journalist value proposition by showing:

- camera-original media remains available for finishing
- camera proxy media can drive responsive preview
- preview playback is bounded and deterministic
- intentional 50p -> 50i-compatible preview skips are explicit
- late drops and duplicates are reported separately
- a simple news cut can be summarized in source-frame and preview-frame terms

This is the first product-shaped bridge from QGS technical media primitives to
QNC OS journalist workflow.

## Limitations

- No GUI.
- No timeline engine.
- No audio playback.
- No real display output.
- No export/render.
- No graphics or title rendering.
- No automatic proxy/original switching policy.
- No full interlaced 50i renderer.
- No VA -> Vulkan zero-copy.

Full 50p production playback still needs a modern-hardware zero-copy milestone
or an explicitly approved lower-level VA transfer boundary.

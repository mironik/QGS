# M2 Step 20O — QGS Broadcast Player Runtime Original MXF Video Payload Binding

Step 20O adds the first bounded `OriginalMedia` video payload binding path to
the QGS Broadcast Player Runtime.

This milestone does not claim realtime original MXF playback. It proves that
the runtime can bind original MXF video frames to real QGS-produced processed
GPU payload references for a small prepared working set.

## Why OriginalMedia Mode Exists

The QGS/QNC media model has two legitimate Broadcast Player source modes:

- `ProxyPreview`: proxy MP4 video plus original MXF audio, intended for
  responsive journalist/edit preview on modest hardware.
- `OriginalMedia`: original MXF video plus original MXF audio, intended for
  sufficiently powerful laptops, workstations, finishing, and original-quality
  workflows.

Proxy MP4 AAC is not authoritative and is not used by either mode.

## Original Video Pipeline

For Sony FX6 sample 002, `OriginalMedia` video payload binding uses:

```text
original MXF video
  -> qgs-mxf access-unit extraction
  -> software H.264 10-bit 4:2:2 decode
  -> CPU YUV422P10 surface
  -> qgs-vulkan YUV422P10 processing
  -> processed GPU frame token
```

The payload reference records:

- source mode: `OriginalMedia`
- video source role: original finishing media
- source frame index/edit unit
- presentation time and duration
- source dimensions
- processed output format
- backend path: `SoftwareH264Yuv422P10Vulkan`
- bounded runtime slot index
- payload token id

The processed payload format is `RgbaU16`. The source format is H.264
High 4:2:2 10-bit / YUV422P10.

## Bounded Working Set

Step 20O does not process all original frames.

For the Sony FX6 sample 002 acceptance run, the prepared working set is three
presentation periods aligned with the existing `journalist-50i-preview`
selection:

- target source frames: 0, 2, 4
- random-access start: 0
- decode end: 20
- decoded original frames: 21
- original video payload bindings: 3
- original audio payload bindings: 3
- presentation payload-ready slots: 3

The decode walk is bounded and only retained the target payload frames.

## Acceptance Result

For Sony FX6 sample 002:

- `OriginalMedia` video source: original MXF
- `OriginalMedia` audio source: original MXF
- proxy video used for `OriginalMedia` payload: no
- proxy AAC used: no
- processed original GPU frame tokens created: 3
- GPU submissions: 3
- GPU completions: 3
- bounded GPU slots: 3
- video binding status: `PayloadReady`
- presentation readiness: `PayloadReady`
- `DevicePayloadReady`: no
- `FramePresented`: 0
- realtime support claimed: no

Development observation, not a benchmark:

- bounded original payload binding took approximately 9.8 seconds on the
  Haswell/i965 system.

This is correct and bounded, but it is not a realtime original-quality playback
proof.

## ProxyPreview Regression Status

The same command still reports the established `ProxyPreview` path:

- video source: proxy MP4
- audio source: original MXF
- proxy video payload bindings: 3
- backend path: `VaapiCpuNv12Vulkan`
- presentation readiness: `PayloadReady`

`ProxyPreview` remains the responsive journalist/edit preview path.

## Limitations

Step 20O does not implement:

- realtime original playback guarantee
- real display output
- real speaker output
- device output
- realtime scheduler
- export/render
- QNC UI integration
- proxy AAC as primary audio

The original video path is currently a bounded payload-binding proof. It is
allowed to be slow and should not be treated as a production realtime path on
Haswell/i965.

## Next Steps

Future work can build on this by:

- connecting original-video payloads to test presenter evidence if useful
- measuring original decode/GPU processing on modern hardware
- adding real device boundary implementations
- keeping `ProxyPreview` as the modest-hardware preview path
- defining production policy for when `OriginalMedia` is selected

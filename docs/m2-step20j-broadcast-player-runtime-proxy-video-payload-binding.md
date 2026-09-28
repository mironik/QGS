# M2 Step 20J — QGS Broadcast Player Runtime Proxy Video Payload Binding

Step 20J advances `ProxyPreview` video prepared slots from accounting-only records to real backend payload references.

This is still not real playback. QGS does not output to a display, does not emit `FramePresented`, does not output audio to a device, does not run a realtime Broadcast Player loop, does not export/render, and does not import QNC crates.

## Relationship To Step 20I

Step 20I bound prepared audio slots to original MXF PCM runtime blocks and left proxy-video slots as `AccountedOnly`.

Step 20J keeps the Step 20I original-audio binding and adds a real proxy-video payload binding for the finite preroll working set.

## Media Roles

`ProxyPreview`:

- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used

`OriginalMedia`:

- video source: original MXF
- audio source: original MXF
- original-video runtime payload readiness: `CapabilityMissing` in this milestone

Proxy MP4 AAC is not authoritative audio in either mode.

## Proxy Video Payload Definition

For this milestone, the proxy-video payload reference is a bounded processed GPU frame token:

- kind: `ProcessedGpuFrame`
- format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- source frame index
- selected preview frame index
- presentation time and duration
- visible and coded dimensions
- bounded runtime slot index
- session index
- payload token id

The payload is produced by QGS through the existing conservative fallback path:

```text
proxy MP4
 -> qgs-mp4
 -> qgs-codec-h264
 -> qgs-vaapi hardware decode
 -> safe VA -> CPU NV12 transfer
 -> compact qgs-vulkan NV12 processing
 -> completed GPU frame token
```

VA -> Vulkan zero-copy remains frozen and was not used.

## Payload Readiness Vs Device Readiness

Step 20J adds the `PayloadReady` presentation readiness state.

The distinction is intentional:

- `RuntimeAccountingReady`: audio is bound and video is accounted, but no real video payload is bound
- `PayloadReady`: original-audio payload and proxy-video backend payload are both bound
- `DevicePayloadReady`: reserved for a future device output backend
- `CapabilityMissing`: a required backend capability is unavailable
- `NotReady`: the binding is incomplete

The Sony FX6 `ProxyPreview` proof reaches `PayloadReady`, not `DevicePayloadReady`.

No `FramePresented` event is emitted.

## ProxyPreview Result

For Sony FX6 sample 002 with `journalist-50i-preview`:

- prepared video payload bindings: 3
- bound source frame indices: 0, 2, 4
- audio payload source: original MXF PCM
- proxy AAC: not used
- audio payload bindings: 3
- presentation payload bindings: 3
- video payload status: `PayloadReady`
- presentation readiness: `PayloadReady`
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- visible dimensions: 1920x1080
- coded dimensions: 1920x1088
- GPU submissions: 3
- GPU completions: 3
- bounded GPU slots: 3
- `FramePresented`: 0

The command keeps the prepared working set finite. It does not create an all-frame payload map.

## OriginalMedia Result

`OriginalMedia` remains represented by the contract:

- video source: original MXF
- audio source: original MXF
- original audio payload binding is possible
- original-video payload binding reports `CapabilityMissing`
- presentation readiness: `CapabilityMissing`

QGS does not fake original-video runtime payload readiness.

## Limitations

Not implemented:

- display output
- `FramePresented`
- realtime Broadcast Player loop
- speaker output
- export/render
- QNC UI integration
- real original MXF video payload binding
- device-output readiness
- audio/video sync correction or resampling

## Next Steps

Future milestones can connect these payload tokens to a real Broadcast Player device-output backend, define display/speaker readiness, and add source-mode-specific runtime policies while preserving the current media rule: original MXF audio is authoritative, proxy MP4 video is the responsive preview path, and proxy AAC is diagnostic/fallback only.

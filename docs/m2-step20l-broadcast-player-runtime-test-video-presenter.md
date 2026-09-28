# M2 Step 20L — QGS Broadcast Player Runtime Test Video Presenter Evidence

Step 20L adds a test-only video presenter boundary for the QGS Broadcast Player Runtime.

This is not real display output. QGS does not create a Vulkan swapchain, does not present through Wayland/X11/DRM/KMS, does not run a realtime playback loop, does not output audio to a device, does not export/render, and does not import QNC crates.

## Relationship To Step 20K

Step 20K defined the device boundary contract and established the rule:

`FramePresented` must not be inferred from payload readiness or GPU completion.

Step 20L proves that rule by adding a test presenter that can accept processed proxy-video payload references and return explicit `BroadcastPresentationEvidence`.

## Test Presenter Capabilities

`BroadcastTestVideoPresenter` is a backend-neutral, test-only presenter. It advertises:

- `AcceptsProcessedGpuFrame`
- `ProvidesPresentationEvidence`

For the Sony FX6 proxy proof it validates:

- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- visible dimensions: 1920x1080
- coded dimensions: 1920x1088

Unsupported payload kinds, formats, or dimensions are rejected and produce no evidence.

## Presentation Evidence

The test presenter produces `BroadcastPresentationEvidence` records with:

- presentation slot index
- video binding index
- media time
- evidence kind: `TestPresenterAccepted`
- source device kind: `VideoPresenter`
- payload token id

The evidence record contains no local file path.

`FramePresented` appears only after evidence exists. In this milestone, `FramePresented` means test-presenter evidence, not a real user-visible display presentation.

## ProxyPreview Result

For Sony FX6 sample 002 with `journalist-50i-preview`:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- processed proxy GPU frame payloads available: 3
- source frames: 0, 2, 4
- payloads submitted to test presenter: 3
- accepted payloads: 3
- rejected payloads: 0
- presentation evidence records: 3
- evidence kind: `TestPresenterAccepted`
- `FramePresented` count: 3
- no real display output claimed

The video presenter boundary reports `DevicePayloadReady` for the processed GPU frame payload shape it accepts. The full A/V device boundary is still not a real output path because no audio sink or display presenter exists.

## OriginalMedia Result

`OriginalMedia` remains contract-valid but not payload-presentable:

- video source: original MXF
- audio source: original MXF
- original-video payload binding: `CapabilityMissing`
- test presenter is not attempted
- `FramePresented` count: 0

QGS does not fake original-video presentation.

## Limitations

Not implemented:

- real display output
- Vulkan swapchain presentation
- Wayland/X11/DRM/KMS presentation
- real realtime playback loop
- speaker output
- PipeWire, ALSA, or PulseAudio
- QNC UI integration
- export/render
- original MXF realtime video presenter
- audio/video sync correction or resampling

## Next Steps

Future milestones can replace the test presenter with real video presenter adapters. Those adapters must preserve the same rule: `FramePresented` is emitted only after explicit presentation evidence from the presenter backend.

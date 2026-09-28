# M2 Step 20K — QGS Broadcast Player Runtime Device Boundary Contract

Step 20K defines the backend-neutral boundary between Broadcast Player Runtime payload readiness and future device/presenter readiness.

This is still not real device output. QGS does not open an audio device, does not present to a display, does not create a Vulkan swapchain, does not run a realtime playback loop, does not emit `FramePresented`, and does not import QNC crates.

## Relationship To Step 20J

Step 20J proved `ProxyPreview` payload binding:

- original MXF PCM audio blocks are bound
- proxy MP4 video frames are decoded, transferred through CPU NV12, processed by Vulkan, and represented by processed GPU frame tokens
- presentation payload readiness reaches `PayloadReady`

Step 20K keeps those payloads and adds a separate device boundary contract.

## Readiness Levels

The runtime now distinguishes:

- `PayloadReady`: QGS owns or references backend payloads, such as original PCM audio and processed proxy GPU frame tokens
- `DevicePayloadReady`: a configured device/presenter boundary has accepted the payload type as suitable for submission
- `SubmittedToDevice`: reserved for a future backend that actually submits payloads to an audio sink or video presenter
- `FramePresented`: allowed only after a real presenter provides presentation evidence

Because Step 20K does not configure real devices, `DevicePayloadReady` remains false and `FramePresented` remains zero.

## Device Boundary Types

The backend-neutral device boundary model includes:

- `BroadcastDeviceKind`: `AudioSink` or `VideoPresenter`
- `BroadcastDeviceStatus`: `NotConfigured`, `CapabilityMissing`, `Ready`, or `Failed`
- `BroadcastDeviceCapability`: examples include `AcceptsOriginalPcm`, `AcceptsF32Pcm`, `AcceptsProcessedGpuFrame`, `AcceptsCpuImage`, and `ProvidesPresentationEvidence`
- `BroadcastDevicePayloadStatus`: `PayloadReady`, `DevicePayloadReady`, `DeviceNotConfigured`, `DeviceCapabilityMissing`, `SubmittedToDevice`, `PresentationEvidenceReceived`, or `Failed`
- `BroadcastAudioDeviceSubmission`
- `BroadcastVideoPresenterSubmission`
- `BroadcastPresentationEvidence`

These are contract records, not device implementations.

## Presentation Evidence Rule

`FramePresented` is not inferred from payload readiness or GPU processing completion.

A future presenter must provide explicit presentation evidence before QGS reports a frame as presented. Step 20K provides the `BroadcastPresentationEvidence` model but creates no evidence events because there is no real presenter.

## ProxyPreview Result

For Sony FX6 sample 002 with `journalist-50i-preview`:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- audio payload ready: yes
- video payload ready: yes
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- audio sink status: `NotConfigured`
- video presenter status: `NotConfigured`
- audio submission status: `DeviceNotConfigured`
- video submission status: `DeviceNotConfigured`
- `DevicePayloadReady`: no
- `FramePresented`: 0
- reason: device boundary not configured

This preserves the Step 20J payload proof while avoiding any claim of speaker or display output.

## OriginalMedia Result

`OriginalMedia` remains contract-valid:

- video source: original MXF
- audio source: original MXF
- original audio payload binding is possible
- original-video payload binding remains `CapabilityMissing`
- device boundary is not attempted
- `DevicePayloadReady`: no
- `FramePresented`: 0

QGS does not fake original-video readiness.

## Future Compatibility

Future audio backends may accept original PCM directly or require conversion to a device format such as f32. They must report capability status honestly.

Future video presenter backends may accept processed GPU frames, CPU images, swapchain images, textures, or imported external-memory paths. They must report capability status honestly and provide presentation evidence before `FramePresented` appears.

## Limitations

Not implemented:

- actual audio output
- PipeWire, ALSA, or PulseAudio integration
- actual display output
- Vulkan swapchain presentation
- real presenter
- realtime scheduler
- QNC UI integration
- export/render
- original MXF realtime video payload binding
- audio/video sync correction or resampling

## Next Steps

Future milestones can add concrete audio sink and video presenter adapters against this contract. Those adapters should advance payloads from `PayloadReady` to `DevicePayloadReady`, then to submitted/device-evidence states only when real backend operations provide proof.

# M2 Step 20Q — QGS Broadcast Player Runtime Acceptance Verification Matrix

Step 20Q adds an explicit verification matrix for the QGS Broadcast Player
Runtime work completed through Step 20O.

This is not a new media feature. It is a truthfulness milestone: the matrix
states what is actually proven, what is only modeled or test-boundary verified,
and what remains unimplemented.

## Evidence Levels

- `NotImplemented`: no runtime implementation exists.
- `CompileChecked`: code builds, but no stronger evidence is claimed.
- `UnitTested`: behavior is covered by hardware-independent tests.
- `MediaInspected`: real media was parsed or inspected.
- `PayloadExtracted`: real media payload bytes/access units were extracted.
- `PayloadBound`: runtime slots bind to concrete backend payload references.
- `TestBoundaryEvidence`: a test-only sink/presenter accepted payloads and
  returned explicit evidence.
- `NativeBufferSubmissionVerified`: a tiny bounded payload was queued to a
  native device boundary, without claiming audible/visible output.
- `NativePostSubmitEvidence`: a native device boundary accepted a tiny bounded
  payload and provided bounded post-submit evidence such as a drain callback,
  without claiming audible/visible output.
- `RuntimeAudioPayloadDrainCompleted`: a prepared Broadcast Player Runtime
  original-audio payload binding was submitted to native PipeWire and drained;
  this is below full audio-device verification.
- `RuntimeAudioPayloadAudibleConfirmed`: a human confirmed hearing a prepared
  Broadcast Player Runtime original-audio payload through the native PipeWire
  boundary; this is still narrower than full Broadcast Player playback,
  realtime playback, A/V sync, channel certification, or full audio-device
  verification.
- `ManualAudibleSignalDetectedContentUnverified`: a human detected a slight
  hum/buzz from a native PipeWire path, but recognizable original audio content
  was not confirmed. This does not verify PCM content correctness, channel
  routing, full audio device output, or full audio-device verification.
- `ManualContentAudibilityPartiallyObserved`: a human later heard voice-like
  content from the original-audio-derived diagnostic/output path, but channel
  routing, gain, production output, realtime playback, and full audio-device
  verification remain unverified.
- `ManualMonitorPairPreferenceObserved`: a human compared diagnostic monitor
  WAVs and preferred one monitor pair for a bounded original-audio range. This
  is diagnostic listening evidence only; it does not certify production
  routing, channel mapping, full playback, realtime playback, A/V sync, or full
  audio-device verification.
- `VisualVerified`: visual/image correctness was verified by comparison or
  display evidence.
- `AudioDeviceVerified`: a real audio device path was verified.
- `RealtimeVerified`: timed realtime acceptance with margin was verified.
- `HardwareValidated`: hardware path was validated for the stated subsystem.

Test-boundary evidence is deliberately below real visual/audio-device
verification.

## Current Matrix

For Sony FX6 sample 002:

| Subsystem | Evidence Level | Current Evidence |
| --- | --- | --- |
| original/proxy association | `MediaInspected` | Original/proxy timing and metadata association proved for sample 002. |
| proxy MP4 inspection | `MediaInspected` | Proxy MP4 container, H.264 video, timing, and tracks inspected. |
| original MXF inspection | `MediaInspected` | Original MXF structure, essence descriptors, video, audio, and timing inspected. |
| proxy H.264 hardware decode | `HardwareValidated` | Proxy H.264 VA decode proved for 106/106 frames after Step 15. |
| original H.264 10-bit 4:2:2 software decode | `PayloadExtracted` | Original access units decode to QGS software video surfaces. |
| original MXF PCM metadata | `MediaInspected` | Authoritative original MXF LPCM metadata modeled. |
| original MXF PCM extraction | `PayloadExtracted` | Original MXF LPCM packets extracted without synthesis. |
| PCM runtime blocks | `PayloadBound` | PCM packets converted to runtime mono-track blocks with timing. |
| proxy video payload binding | `TestBoundaryEvidence` | Processed proxy GPU frame payloads bind and pass test presenter evidence. |
| original video payload binding | `PayloadBound` | Bounded original MXF processed GPU frame payloads bind; realtime is not claimed. |
| Broadcast Player Runtime state machine | `UnitTested` | State transitions and invalid transitions are covered by tests. |
| preroll plan | `UnitTested` | Bounded ready/not-ready behavior is covered. |
| prepared slots | `UnitTested` | Finite prepared audio/video/presentation slot behavior is covered. |
| event surface | `UnitTested` | Deterministic backend-neutral runtime event accounting is covered. |
| device boundary contract | `UnitTested` | `PayloadReady` and `DevicePayloadReady` remain distinct. |
| test video presenter evidence | `TestBoundaryEvidence` | Test presenter evidence gates `FramePresented`; this is not real display output. |
| test audio sink evidence | `TestBoundaryEvidence` | Test audio sink evidence accepts original PCM; this is not real speaker output. |
| native PipeWire buffer submission | `NativePostSubmitEvidence` | A 20 ms original-audio-derived f32 buffer was queued to a native PipeWire stream and a drain callback was observed; audible output and full playback are not claimed. |
| native PipeWire audible smoke test | `ManualContentAudibilityPartiallyObserved` | Bounded original-audio-derived PipeWire smoke-test buffers submitted and drained; later listening found voice-like content rather than only hum/buzz, but routing/gain and production output remain unverified. |
| native PipeWire original-audio segment playback | `ManualContentAudibilityPartiallyObserved` | Bounded sequential original MXF audio segment submitted and drained; later listening found voice-like content in both tested versions, with the second clearer and apparently present on both channels. |
| PipeWire audio content sanity audit | `MediaInspected` | Original MXF PCM statistics, endian/sign interpretation, f32 conversion, segment/runtime path equality, and PipeWire buffer geometry audited; manual listening partially supports content audibility, but routing and device-output correctness remain unverified. |
| Mironik 2002 monitor diagnostic | `ManualMonitorPairPreferenceObserved` | For the 0-1000 ms bounded original-MXF audit range, the track 4 / track 1 loudest-pair stereo monitor WAV sounded best among tested diagnostics and matches the RMS statistics. Proxy AAC was not used; this is diagnostic monitor preference only. |
| broadcast runtime audio payload to PipeWire | `RuntimeAudioPayloadDrainCompleted` | First prepared `ProxyPreview` Broadcast Player Runtime original-audio payload binding submits to native PipeWire and drains; an audible helper path exists but manual confirmation is tracked separately and does not imply full playback, realtime playback, A/V sync, channel certification, or full audio-device verification. |
| simulated playback loop | `TestBoundaryEvidence` | Prepared slots flow through test audio/video boundaries deterministically. |
| real speaker output | `NotImplemented` | No audible speaker output, audio-device clock, or full audio playback path exists; native PipeWire buffer submission is tracked separately. |
| real display output | `NotImplemented` | No swapchain, Wayland, X11, DRM/KMS, or real display presenter exists. |
| realtime playback | `NotImplemented` | Broadcast Player realtime scheduler has not been implemented or accepted. |
| modern-hardware zero-copy | `NotImplemented` | VA/Vulkan zero-copy remains frozen and not verified on modern hardware. |

## Truth Rules

The matrix intentionally does not overstate these areas:

- Test presenter evidence is not real display output.
- Test audio sink evidence is not real speaker output.
- Native PipeWire buffer submission/drain evidence is not audible playback or
  `AudioDeviceVerified`.
- PipeWire audible smoke-test and bounded original-audio segment manual
  evidence were later clarified as voice-like content partially observed, not
  only hum/buzz. This still does not certify channel routing, gain, production
  output, realtime/full Broadcast Player playback, A/V sync, or full
  `AudioDeviceVerified`.
- Mironik 2002 monitor diagnosis identified track 4 / track 1 as the preferred
  diagnostic monitor pair for the 0-1000 ms range. This is not channel
  certification, production routing, full playback, realtime playback, A/V
  sync, or full `AudioDeviceVerified`.
- Runtime-prepared audio payload submission/drain evidence is not full playback,
  realtime playback, A/V sync, channel certification, or full
  `AudioDeviceVerified`.
- The audio content sanity audit is inspection evidence, not audible content
  verification.
- Haswell CPU-bridge 1080p50 is not marked realtime verified.
- Modern-hardware zero-copy is not marked verified.
- `FramePresented` from the current test presenter remains test evidence, not
  user-visible display presentation.

## QGS-Test Command

The verification matrix can be printed with:

```bash
cargo run -q -p qgs-test -- --broadcast-player-runtime-verification <original-mxf> <proxy-mp4>
```

The command parses the supplied media enough to report the media-backed context,
then prints the canonical current evidence matrix and validates the truth rules.

## Remaining Unverified Work

The major remaining runtime upgrades are:

- real audio device output and device-clock evidence
- real display/presenter output and presentation evidence
- realtime Broadcast Player scheduler
- realtime acceptance with margin
- modern-hardware VA/Vulkan zero-copy validation
- production policy for choosing `ProxyPreview` vs `OriginalMedia`

Future milestones should upgrade evidence levels only when they produce the
corresponding proof.

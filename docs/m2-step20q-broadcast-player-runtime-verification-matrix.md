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
- `FilePresenterManifestWritten`: a file presenter diagnostic wrote a
  deterministic public-safe descriptor manifest for a prepared video payload.
  This is not a real display backend and does not verify visual pixels.
- `FilePresenterImageWritten`: a file presenter diagnostic wrote a still-image
  artifact from real frame pixels. This remains below `VisualVerified` unless
  comparison or display evidence is added.
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
- `ControlSurfaceEvidence`: a production-shaped control facade exposes stable
  commands, snapshots, readiness, position, prepared-window, device-status, and
  event summaries over already-separated runtime modules. This is not real
  display output, realtime playback, A/V sync, or production audio-device
  verification.
- `SelectionPolicyEvidence`: backend selection policy is modeled and tested,
  including candidate availability and conservative non-claims. This is not
  backend implementation, real display output, realtime playback, A/V sync, or
  production audio-device verification.
- `OperationalStateEvidence`: deterministic player state, command legality,
  logical tick/position progression, seek/stop/completion behavior, buffer
  health, and warnings are modeled and tested. This is not realtime playback,
  real display output, A/V sync, or production audio-device verification.
- `FaultRecoveryPolicyEvidence`: structured fault severity, scope, recovery
  action, snapshot counters, backend warnings, and rejected-command no-mutation
  rules are modeled and tested. This is not realtime playback, real display
  output, A/V sync, or production audio-device verification.
- `RuntimeBehaviorEvidence`: a running backend runtime demo exercises
  deterministic load/prepare/cue/play/tick/pause/seek/stop/unload behavior and
  reports live state transitions. This is not realtime playback, real display
  output, A/V sync, or production audio-device verification.
- `AssemblySurfaceEvidence`: a modular Broadcast Player Runtime composition
  root wires existing LEGO modules and creates operational runtimes without
  becoming a monolithic player. This is not realtime playback, real display
  output, A/V sync, or production audio-device verification.
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
- `DesktopMonoListeningHelperDrainCompleted`: one original MXF mono source
  track was duplicated to L/R only as an ad-hoc desktop listening helper,
  submitted, and drained. This is not discrete mono broadcast output and does
  not certify production routing, channel mapping, or full audio-device
  verification.
- `ManualDesktopMonoListeningHelperHeard`: a human reported hearing the desktop
  mono listening helper. This remains a diagnostic helper observation and is
  below discrete 4-mono output verification, channel certification, production
  routing, realtime playback, A/V sync, and full audio-device verification.
- `Discrete4MonoOutputDrainCompleted`: original MXF tracks 1, 2, 3, and 4 were
  submitted as output channels 1, 2, 3, and 4 to a native PipeWire 4-channel
  f32 boundary and drained. This does not certify physical channel mapping,
  production routing, realtime playback, A/V sync, or full audio-device
  verification.
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
| presenter/monitor boundary | `TestBoundaryEvidence` | Prepared video payload descriptors can be submitted to the test presenter boundary and projected into monitor facts. Real display output and visual verification remain unimplemented. |
| file presenter visual diagnostic | `FilePresenterManifestWritten` | A deterministic public-safe descriptor manifest can be written for a prepared proxy video payload. Current runtime payload binding exposes a processed GPU frame token/descriptor, not CPU-readable pixels, so no image artifact, real display output, or visual verification is claimed. |
| GPU payload readback visual diagnostic | `FilePresenterImageWritten` | A bounded validation readback can write a PPM image from real prepared proxy GPU payload pixels. This is a diagnostic file artifact only; no real display output, visual comparison, `VisualVerified`, or realtime evidence is claimed. |
| test audio sink evidence | `TestBoundaryEvidence` | Test audio sink evidence accepts original PCM; this is not real speaker output. |
| native PipeWire buffer submission | `NativePostSubmitEvidence` | A 20 ms original-audio-derived f32 buffer was queued to a native PipeWire stream and a drain callback was observed; audible output and full playback are not claimed. |
| native PipeWire audible smoke test | `ManualContentAudibilityPartiallyObserved` | Bounded original-audio-derived PipeWire smoke-test buffers submitted and drained; later listening found voice-like content rather than only hum/buzz, but routing/gain and production output remain unverified. |
| native PipeWire original-audio segment playback | `ManualContentAudibilityPartiallyObserved` | Bounded sequential original MXF audio segment submitted and drained; later listening found voice-like content in both tested versions, with the second clearer and apparently present on both channels. |
| PipeWire audio content sanity audit | `MediaInspected` | Original MXF PCM statistics, endian/sign interpretation, f32 conversion, segment/runtime path equality, and PipeWire buffer geometry audited; manual listening partially supports content audibility, but routing and device-output correctness remain unverified. |
| Mironik 2002 monitor diagnostic | `ManualMonitorPairPreferenceObserved` | For the 0-1000 ms bounded original-MXF audit range, the track 4 / track 1 loudest-pair stereo monitor WAV sounded best among tested diagnostics and matches the RMS statistics. Proxy AAC was not used; this is diagnostic monitor preference only. |
| native PipeWire desktop mono listening helper | `DesktopMonoListeningHelperDrainCompleted` | One original MXF mono track is duplicated to L/R as an ad-hoc desktop listening helper and drained through PipeWire. This is not discrete mono broadcast output, production routing, channel certification, full playback, realtime playback, A/V sync, or `AudioDeviceVerified`. |
| native PipeWire discrete 4-mono output boundary | `Discrete4MonoOutputDrainCompleted` | Original MXF track 1/2/3/4 can be submitted as output channel 1/2/3/4 to a native PipeWire 4-channel f32 boundary and drained. Physical channel mapping, production routing, full playback, realtime playback, A/V sync, and `AudioDeviceVerified` are not claimed. |
| broadcast runtime audio payload to PipeWire | `RuntimeAudioPayloadDrainCompleted` | First prepared `ProxyPreview` Broadcast Player Runtime original-audio payload binding submits to native PipeWire and drains; an audible helper path exists but manual confirmation is tracked separately and does not imply full playback, realtime playback, A/V sync, channel certification, or full audio-device verification. |
| runtime surface end-to-end acceptance | `TestBoundaryEvidence` | Integration Block A session orchestration and Block B test presenter/monitor boundary are wired together as separate backend modules. This is not monolithic runtime ownership; real display output, realtime playback, A/V sync, and visual verification remain unimplemented. |
| broadcast player control core | `ControlSurfaceEvidence` | Production-shaped Broadcast Player control facade exposes commands, snapshots, readiness, position, prepared-window, device status, and product events over existing Lego modules without claiming real display, realtime playback, A/V sync, or production audio-device verification. |
| device backend selection | `SelectionPolicyEvidence` | Backend-neutral selection policies identify diagnostic/future video and audio backends truthfully. Wayland/Vulkan is the QNC OS display target but remains `NotImplemented`, X11 is unsupported for QNC OS, and PipeWire prototypes are not production verified. |
| broadcast player operational runtime | `OperationalStateEvidence` | Deterministic Broadcast Player runtime enforces command legality, advances logical frame/audio position on playing ticks, models seek/stop/completion, reports buffer health and warnings, and keeps real display, realtime playback, A/V sync, and production audio-device claims false. |
| broadcast player fault and recovery rules | `FaultRecoveryPolicyEvidence` | Structured fault severity, scope, recovery action, snapshot counters, backend warnings, and rejected-command no-mutation rules are modeled and tested. Real display, realtime playback, A/V sync, and production audio-device claims remain false. |
| broadcast player running runtime demo | `RuntimeBehaviorEvidence` | Running operational demo streams step-by-step backend player state, frame/audio progression, pause freeze, seek/reprepare/replay, stop, unload, buffer health, and non-claims using the operational runtime. Real display, realtime playback, A/V sync, and production audio-device claims remain false. |
| broadcast player modular runtime assembly | `AssemblySurfaceEvidence` | `QgsBroadcastPlayerAssembly` is a composition root over input planning, transport, frame-clock facts, preroll, payload providers, device selection, fault/recovery, events, snapshots, and session/control surfaces. It creates existing operational runtimes without duplicating player logic or claiming real display, realtime playback, A/V sync, or production audio-device verification. |
| simulated playback loop | `TestBoundaryEvidence` | Prepared slots flow through test audio/video boundaries deterministically. |
| real speaker output | `NotImplemented` | No audible speaker output, audio-device clock, or full audio playback path exists; native PipeWire buffer submission is tracked separately. |
| real display output | `NotImplemented` | No QNC OS Wayland/Vulkan presenter, optional DRM/KMS direct-output presenter, legacy X11 compatibility presenter, swapchain, or real display presenter exists. X11 is not a QNC OS target. |
| realtime playback | `NotImplemented` | Broadcast Player realtime scheduler has not been implemented or accepted. |
| modern-hardware zero-copy | `NotImplemented` | VA/Vulkan zero-copy remains frozen and not verified on modern hardware. |

## Truth Rules

The matrix intentionally does not overstate these areas:

- Test presenter evidence is not real display output.
- Presenter/monitor boundary evidence from the test presenter is not real
  display output and is not visual verification.
- File presenter visual diagnostics are not real display output. A descriptor
  manifest is not visual verification, and an image artifact would still require
  comparison or display evidence before `VisualVerified`.
- Runtime surface end-to-end acceptance combines session commands with
  test-presenter monitor evidence through separate Lego modules; it is not
  monolithic runtime ownership, realtime playback, A/V sync, real display
  output, or visual verification.
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
- The PipeWire desktop mono listening helper does not verify discrete mono
  channel output. Duplicated L/R output is an ad-hoc desktop listening helper
  only, not stereo runtime truth, production routing, or channel-correct
  broadcast output.
- Discrete 4-mono PipeWire output has bounded submit/drain evidence only. It
  does not certify physical channel mapping, production routing, full playback,
  realtime playback, A/V sync, or full `AudioDeviceVerified`.
- Runtime-prepared audio payload submission/drain evidence is not full playback,
  realtime playback, A/V sync, channel certification, or full
  `AudioDeviceVerified`.
- Broadcast Player control surface evidence proves product-shaped command and
  snapshot orchestration only. It is not real display output, realtime playback,
  A/V sync, visual verification, or production audio-device verification.
- Device backend selection evidence proves selection policy and reporting only.
  It is not Wayland/Vulkan presenter implementation, real display output,
  `FramePresented` from a real backend, `VisualVerified`, realtime playback,
  A/V sync, `AudioDeviceVerified`, or production PipeWire audio output.
- Broadcast Player operational runtime evidence proves deterministic command
  legality, logical tick/position progression, seek/stop/completion state, and
  buffer-health reporting only. It is not a realtime scheduler, real display
  output, real backend `FramePresented`, visual verification, A/V sync, or
  production audio-device verification.
- Broadcast Player fault/recovery evidence proves structured severity, scope,
  recovery action, counters, backend warnings, and rejected-command no-mutation
  rules only. It is not a realtime scheduler, real display output, real backend
  `FramePresented`, visual verification, A/V sync, or production audio-device
  verification.
- Broadcast Player running runtime demo evidence proves deterministic backend
  runtime behavior over a scripted scenario only. It is not a realtime
  scheduler, real display output, real backend `FramePresented`, visual
  verification, A/V sync, production audio output, or full playback acceptance.
- Broadcast Player modular runtime assembly evidence proves composition and
  module ownership clarity only. It is not a monolithic player implementation,
  realtime scheduler, real display output, visual verification, A/V sync,
  production audio output, or full playback acceptance.
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

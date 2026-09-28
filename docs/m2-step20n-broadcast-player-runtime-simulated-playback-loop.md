# M2 Step 20N — QGS Broadcast Player Runtime Simulated Playback Loop

Step 20N connects the prepared payload, test video presenter, and test audio
sink layers into a deterministic Broadcast Player Runtime simulation.

This is still not real playback. It does not use a realtime scheduler, wall
clock, real speaker output, PipeWire, ALSA, PulseAudio, Vulkan swapchain,
Wayland, X11, DRM/KMS presentation, export/render, or QNC UI integration.

## Relationship To Steps 20L And 20M

Step 20L introduced test video presenter evidence for processed proxy GPU frame
tokens. Step 20M introduced test audio sink evidence for original MXF PCM audio
payload bindings.

Step 20N consumes prepared presentation slots and submits each slot's payloads
to those test boundaries:

1. original MXF audio payload binding -> test audio sink
2. processed proxy video payload binding -> test video presenter
3. evidence events are emitted
4. `FramePresented` is emitted only from test video presenter evidence
5. the simulation completes after all prepared slots succeed

The evidence proves sequencing and contract behavior. It does not claim
user-visible display output or audible playback.

## Deterministic Ordering

The simulation uses a fixed per-slot ordering:

1. verify the presentation payload is `PayloadReady`
2. submit audio to the test audio sink
3. receive `TestAudioSinkAccepted` evidence
4. account the original-audio range
5. submit video to the test video presenter
6. receive `TestPresenterAccepted` evidence
7. emit test-presenter `FramePresented`
8. account the selected video frame

This ordering is only a simulation policy. It is not a final realtime playback
scheduling policy.

## ProxyPreview Result

For Sony FX6 sample 002 in `ProxyPreview` mode:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- prepared working set: 3 presentation slots
- audio submissions: 3
- audio accepted: 3
- video submissions: 3
- video accepted: 3
- test audio evidence records: 3
- test video evidence records: 3
- total test evidence records: 6
- `FramePresented`: 3, test presenter evidence only
- lateness drops: 0
- failed slots: 0
- final state: `Completed`
- completed: yes

The `FramePresented` count in this milestone means the test video presenter
returned explicit evidence. It does not mean a frame was displayed by a real
display server or scanout path.

## OriginalMedia Result

For `OriginalMedia` mode:

- audio source: original MXF
- original audio payload binding remains possible
- original video payload remains `CapabilityMissing`
- simulation does not present fake original video
- `FramePresented`: 0
- final state: `Failed`
- result: not ready / capability missing

This preserves the Step 20F/20G contract: OriginalMedia is a valid source mode,
but the realtime original MXF video payload backend is not integrated yet.

## Event And Evidence Rules

The simulation keeps these rules explicit:

- audio acceptance requires `TestAudioSinkAccepted` evidence
- audio evidence is distinct from audible playback
- video presentation requires `TestPresenterAccepted` evidence
- `FramePresented` is emitted only after test video presenter evidence
- no raw local filesystem paths are carried in runtime events
- proxy AAC is not authoritative and is not used

## Limitations

Step 20N does not implement:

- realtime scheduler
- real audio output
- real display output
- device clock
- audio/video drift correction
- resampling
- QNC UI integration
- export/render
- original MXF realtime video playback

## Next Steps

Future milestones can replace the test boundaries with real device backends:

- an audio device backend with explicit clock/capability reporting
- a video presenter backend with real presentation evidence
- a realtime scheduler that consumes the same bounded prepared payload slots
- OriginalMedia video payload readiness on capable hardware

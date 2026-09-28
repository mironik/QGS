# M2 Step 21H — Bounded PipeWire Original-Audio Segment Playback

Step 21H adds bounded sequential original-audio segment playback through the
native PipeWire boundary.

This is still not full playback. It does not implement the Broadcast Player
realtime scheduler, A/V sync, video output, media/device clock policy, QNC UI
integration, export, or production channel-routing policy.

## Relationship To Step 21G

Step 21G proved a bounded audible smoke test:

- original MXF-derived audio
- 500 ms
- repeated 20 ms source segment
- manual audible confirmation: `ManualAudibleSmokeTestConfirmed`

Step 21H is stronger because it does not loop one source block as primary
evidence. It builds the submitted buffers from sequential original MXF PCM
blocks in timeline order.

## Source Audio Rule

The source is only original MXF audio:

- original MXF PCM is authoritative
- proxy AAC is not used
- source tracks: 4 mono tracks
- source sample rate: 48 kHz
- source format: signed 24-bit little-endian PCM-style runtime payload

The QGS runtime PCM model remains original 24-bit PCM. The f32 interleaved
format is only a PipeWire device-boundary format.

## Segment Construction

The command selects a bounded segment from sequential original PCM blocks:

- preferred duration: 1 second
- fallback duration: 500 ms if the preferred segment is unavailable
- selected start sample: 0
- block size for Sony FX6 sample 002: 960 samples per track
- each output PipeWire buffer: 960 frames, 4 channels, 3,840 f32 samples,
  15,360 bytes

Before submission, the command verifies:

- four mono original tracks are present
- all selected blocks are 24-bit little-endian PCM
- sample rate is consistent
- per-track block sample counts are consistent
- selected blocks are contiguous in sample time
- no gap or overlap is accepted

If continuity cannot be verified, the command refuses sequential playback
rather than synthesizing or looping data.

## PipeWire Lifecycle

The command reuses the native `qgs-audio-pipewire` callback lifecycle:

1. Create/configure a native PipeWire output stream.
2. Submit the next sequential original-audio buffer on each process callback.
3. Stop after the finite segment buffer count.
4. Request a safe drain.
5. Observe drain completion if PipeWire reports it.

There is no infinite loop, no full-file playback, and no A/V scheduler.

## Routing

Prototype routing remains:

| Original track | PipeWire channel |
| --- | --- |
| track 1 | FL |
| track 2 | FR |
| track 3 | RL |
| track 4 | RR |

This is not production channel-routing policy, channel certification, or
speaker calibration.

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-segment-playback <original-mxf>
```

For Sony FX6 sample 002:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-segment-playback "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

## Expected Evidence

The command reports:

- original MXF audio source
- proxy AAC not used
- selected segment start sample
- selected segment duration
- source blocks per track
- continuity status
- buffers planned/submitted
- bytes copied
- drain requested/completed
- manual confirmation result
- evidence level
- `AudioDeviceVerified`: no

Manual confirmation labels are:

- `ManualOriginalSegmentAudibleConfirmed`
- `ManualOriginalSegmentNotHeard`
- `ManualOriginalSegmentConfirmationRequired`

Noninteractive runs report `ManualOriginalSegmentConfirmationRequired`.

## Observed Local Result

Observed manual command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-segment-playback "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result:

- audio source: original MXF
- proxy AAC: not used
- sequential original blocks: yes
- full playback: no
- realtime Broadcast Player playback: no
- A/V sync: no
- selected segment start sample: 0
- selected segment duration: 1000.000 ms
- source blocks per track: 50
- samples per track: 48,000
- continuity status: contiguous/no gaps/no overlaps
- routing: track 1 -> FL, track 2 -> FR, track 3 -> RL, track 4 -> RR
- output buffers planned: 50
- output bytes planned: 768,000
- stream configured: yes
- observed stream states: `Connecting`, `Paused`, `Streaming`
- final stream state: `Streaming`
- process callback reached: yes
- buffer capacity: 196,608 bytes
- buffers submitted: 50
- output channels: 4
- f32 samples written: 192,000
- bytes copied: 768,000
- drain requested: yes
- drain completed: yes
- post-submit callbacks observed: 0
- post-submit timeout: no
- PipeWire evidence level: `DrainCompleted`
- manual confirmation answer: yes
- manual confirmation status: `ManualOriginalSegmentAudibleConfirmed`
- segment evidence level: `ManualOriginalSegmentAudibleConfirmed`
- `AudioDeviceVerified`: no
- `AudioDeviceVerified` scope: not upgraded by bounded segment playback
- audible output claimed: yes

## Verification Matrix

Step 20Q includes a separate `native PipeWire original-audio segment playback`
row. Its current evidence is `ManualOriginalSegmentAudibleConfirmed`: a human
confirmed hearing the bounded sequential original MXF audio segment.

This remains separate from:

- full Broadcast Player playback
- realtime playback
- A/V sync
- full `AudioDeviceVerified`
- channel certification
- speaker calibration

## Not Implemented

- full playback
- realtime Broadcast Player scheduler
- A/V sync
- media clock/device clock policy
- drift correction
- underrun recovery policy
- video output
- QNC UI integration
- waveform work
- audio editing
- export/render
- proxy AAC primary path
- production channel-routing policy

## Next Step

The next audio milestones should move from bounded segment proof toward a
proper finite stream runner with timing observation, underrun reporting, device
clock policy, and explicit criteria for narrow vs full audio-device
verification.

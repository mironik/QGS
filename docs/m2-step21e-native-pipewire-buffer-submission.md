# M2 Step 21E — Native PipeWire Buffer Submission

Step 21E extends the Step 21D native PipeWire stream creation prototype by
submitting one tiny bounded buffer to a real PipeWire output stream.

This is still not full playback. It does not implement the Broadcast Player
realtime scheduler, audio-device clock policy, A/V sync, audible verification,
or QNC UI integration.

## Relationship To Step 21D

Step 21D proved that QGS can create and configure an isolated native PipeWire
stream:

- stream format: `F32Interleaved`
- sample rate: 48,000 Hz
- channels: 4
- observed evidence: `NativeStreamConfigured`
- buffer submitted: no

Step 21E keeps the same isolated Linux device-boundary crate,
`qgs-audio-pipewire`, and adds bounded dequeue/write/queue behavior for one
prototype buffer.

`qgs-media-runtime` remains backend-neutral. PipeWire-specific code remains
outside the runtime crate.

## Source Audio

The source is original MXF audio from the Sony FX6 sample 002 original media:

- audio source: original MXF
- proxy AAC: not used
- tracks: 4 mono tracks
- sample rate: 48 kHz
- source payload: signed 24-bit PCM-style little-endian bytes

The QGS runtime audio truth remains the original 24-bit PCM payload. The f32
buffer below is only a Linux audio-device boundary representation.

## Callback And Buffer Lifecycle

The prototype uses the PipeWire process callback as the one-shot submission
boundary:

1. Connect to PipeWire and create/configure an output stream.
2. Wait for the stream to reach a configured state.
3. On the process callback, dequeue one writable PipeWire buffer.
4. Verify that the writable data plane is large enough.
5. Copy the bounded converted f32 buffer into the PipeWire buffer.
6. Set chunk metadata:
   - offset: `0`
   - stride: `channels * sizeof(f32)`
   - size: copied byte count
7. Return ownership of the buffer to PipeWire through the safe stream buffer
   wrapper.
8. Stop the prototype loop after one submission.

If the callback fires again, the prototype does not enqueue additional audio.
This milestone submits at most one bounded buffer.

## Conversion

The device-boundary conversion is:

```text
original MXF PCM, 4 mono tracks, signed 24-bit little-endian
    ->
f32 interleaved, 4 channels, 48 kHz
```

Signed 24-bit values are sign-extended from three little-endian bytes and
normalized by `8388608.0`. The conversion keeps zero centered at `0.0`, maps
the minimum negative sample to `-1.0`, and maps the maximum positive sample just
below `1.0`.

The conversion is local to `qgs-audio-pipewire`; it does not mutate the
backend-neutral PCM payload model.

## Prototype Channel Mapping

The prototype uses a direct four-track mapping:

| Original track | Device channel |
| --- | --- |
| track 1 | channel 0 / FL |
| track 2 | channel 1 / FR |
| track 3 | channel 2 / RL |
| track 4 | channel 3 / RR |

This is prototype device-boundary routing, not a production channel-routing or
mixdown policy.

## Submitted Buffer Geometry

Observed local command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result:

- stream created/configured: yes
- observed stream states: `Connecting`, `Paused`, `Streaming`
- final stream state: `Streaming`
- process callback reached: yes
- buffer dequeued: yes
- writable buffer capacity: 196,608 bytes
- source range: 960 samples per track
- duration: 20 ms at 48 kHz
- output channels: 4
- f32 samples written: 3,840
- bytes copied: 15,360
- buffer queued: yes
- evidence level: `BufferSubmitted`
- `AudioDeviceVerified`: no
- audible output claimed: no

## Evidence Meaning

`BufferSubmitted` means QGS copied one bounded original-audio-derived buffer
into a native PipeWire output buffer and returned that buffer to PipeWire.

It does not mean:

- audible output was verified
- a device clock was measured
- full Broadcast Player playback exists
- realtime scheduling exists
- A/V sync exists

The Step 20Q verification matrix records this separately as
`NativeBufferSubmissionVerified`, below `AudioDeviceVerified`.

## Not Implemented

- audible verification
- full-file playback
- realtime Broadcast Player scheduler
- A/V sync
- audio-device clock policy
- production channel-routing policy
- waveform UI
- audio editing
- mixdown/export
- QNC UI integration
- proxy AAC primary path

## Next Step

The next audio milestone should decide how the Broadcast Player runtime will
own a real audio-device clock policy. That should include longer bounded stream
operation, underrun/error reporting, device timing evidence, and a clear rule
for when `AudioDeviceVerified` can be claimed.

# M2 Step 21F — Native PipeWire Processed Callback / Drain Evidence

Step 21F extends the one-shot native PipeWire buffer submission from Step 21E.
It keeps the same tiny original-audio-derived buffer, then continues the
PipeWire event loop for a bounded period to see whether PipeWire reports
post-submit evidence.

This is still not full playback. QGS does not claim audible speaker output,
audio-device clock ownership, realtime Broadcast Player scheduling, or A/V sync.

## Relationship To Step 21E

Step 21E proved:

- native PipeWire stream configured
- process callback reached
- output buffer dequeued
- 20 ms original-audio-derived f32 buffer copied
- buffer queued back to PipeWire
- evidence level: `BufferSubmitted`
- `AudioDeviceVerified`: no

Step 21F adds post-submit observation:

- request a safe PipeWire stream drain after the one queued buffer
- continue a bounded event loop
- record any process callbacks after submission
- record stream states after submission
- record drain completion if PipeWire reports it
- report timeout when no stronger evidence arrives

## Source Audio Rule

The source remains original MXF audio:

- original MXF PCM: authoritative
- proxy AAC: not used
- runtime PCM truth: signed 24-bit original blocks
- device-boundary format: f32 interleaved only inside `qgs-audio-pipewire`

No proxy MP4 AAC path participates in this milestone.

## One-Shot Lifecycle

The native prototype now follows this bounded lifecycle:

1. Create and configure a PipeWire output stream.
2. Wait for the first process callback.
3. Dequeue one writable output buffer.
4. Copy one 20 ms converted buffer into it.
5. Set chunk metadata:
   - offset: `0`
   - stride: `channels * sizeof(f32)`
   - size: copied byte count
6. Queue the buffer by returning ownership through the safe PipeWire wrapper.
7. Request `flush(true)` as a safe drain request.
8. Continue the event loop only until drain/error evidence or timeout.

The process callback does not queue a second audio buffer after the first
submission.

## Observed Local Result

Command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result:

- stream configured: yes
- observed stream states: `Connecting`, `Paused`, `Streaming`
- final stream state: `Streaming`
- first process callback reached: yes
- buffer dequeued: yes
- buffer capacity: 196,608 bytes
- source range: 960 samples per track
- output channels: 4
- f32 samples written: 3,840
- bytes copied: 15,360
- buffer submitted: yes
- post-submit callbacks observed: 0
- stream states after submission: none observed
- drain requested: yes
- drain completed: yes
- stream error after submit: no
- post-submit timeout: no
- evidence level: `DrainCompleted`
- `AudioDeviceVerified`: no
- audible output claimed: no

## Evidence Meaning

`DrainCompleted` means QGS queued one bounded buffer to a native PipeWire
stream and PipeWire invoked the safe drained callback after the drain request.

It does not mean:

- audio was audibly verified
- a device clock was measured
- full Broadcast Player playback exists
- realtime scheduling exists
- A/V sync exists

The Step 20Q verification matrix records this as `NativePostSubmitEvidence`,
below `AudioDeviceVerified`.

## Drain Policy

The PipeWire Rust binding exposes a safe `flush(true)` call and a safe
`drained` callback. Step 21F uses those APIs after the one submitted buffer.

No unsafe QGS code was added for drain evidence.

## Not Implemented

- audible speaker verification
- full-file playback
- continuous audio streaming
- realtime Broadcast Player scheduler
- A/V sync
- audio-device clock policy
- drift correction
- underrun recovery
- waveform UI
- audio editing
- mixdown/export
- QNC UI integration
- proxy AAC primary path

## Next Step

The next audio-device milestone should move from one-shot evidence toward a
bounded multi-buffer stream model. That should still keep original MXF PCM as
runtime truth, keep conversion at the device boundary, and define exactly what
future `AudioDeviceVerified` requires.

# M2 Step 21B — PipeWire Stream Open Prototype

Step 21B attempts the first real Linux audio device-boundary operation for the
QGS Broadcast Player Runtime. It is still not full playback, not realtime
scheduling, and not QNC UI integration.

The authoritative media rule remains:

- original MXF audio is authoritative
- proxy MP4 video is for preview/edit performance
- proxy MP4 AAC is not used

## Dependency And Boundary Choice

No new Rust dependency was added.

The prototype uses the local PipeWire command-line client `pw-cat` from
`qgs-linux`. This keeps `qgs-media-runtime` backend-neutral and avoids importing
PipeWire-specific APIs into the runtime contract before the stream lifecycle and
threading model are settled.

This is deliberately a prototype boundary. A future production backend should
use an isolated PipeWire crate/module once QGS is ready to own stream callbacks,
buffer lifecycle, and device clock evidence directly.

## Source Audio

Sony FX6 sample 002 original MXF audio:

- source: original MXF
- tracks: 4 mono tracks
- sample rate: 48 kHz
- bit depth: 24-bit
- source runtime truth: signed 24-bit PCM blocks
- proxy AAC: not used

## Device-Boundary Conversion

The prototype converts only at the Linux audio device boundary:

- input: 4 mono 24-bit original MXF PCM tracks
- output prototype format: 4-channel `f32` interleaved
- sample rate: 48 kHz
- sample count: 960
- duration: 20 ms
- output bytes: 15,360

The original runtime PCM model is not mutated and is not replaced by `f32`.
This is not mixdown/export. It is a tiny bounded device-boundary conversion for
stream-open investigation.

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-prototype <original-mxf>
```

Observed local command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-prototype "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result:

- PipeWire available: yes
- stream open attempted: yes
- stream opened: yes
- buffer submitted: no
- bytes submitted: 0
- evidence level: `StreamOpened`
- `AudioDeviceVerified`: no

`pw-cat --verbose` reported a PipeWire stream reaching streaming state, but the
process exited with status 1. QGS therefore treats the stream-open evidence as
real, but does not claim buffer-submission evidence.

## Evidence Meaning

The evidence level is `StreamOpened`.

This means:

- QGS can reach PipeWire through the local Linux boundary
- a PipeWire playback stream can be created far enough to enter streaming state
- the current command-backed prototype does not prove the bounded original-audio
  buffer was accepted by the device

`AudioDeviceVerified` is not claimed.

## What Is Not Implemented

- full audio playback
- realtime Broadcast Player scheduling
- audio-device clock ownership
- A/V sync
- video output
- waveform UI
- audio editing
- mixdown/export
- QNC UI integration
- proxy AAC primary path
- production PipeWire callback/buffer lifecycle

## Next Steps

The next audio-device milestone should replace the command-backed prototype
with an isolated PipeWire stream module that can:

- negotiate stream format directly
- allocate/dequeue PipeWire buffers
- copy a tiny bounded original-audio-derived buffer into a real stream buffer
- observe process/callback evidence that the buffer was queued
- expose timing or stream-position evidence if available

Only then should QGS consider upgrading the verification matrix to
`AudioDeviceVerified`, and even then the claim should remain scoped until full
Broadcast Player playback exists.

# M2 Step 21I — Broadcast Runtime Audio Payload to PipeWire Boundary

Step 21I connects the QGS Broadcast Player Runtime audio payload binding layer
to the native PipeWire device boundary.

This is still not full playback. It does not implement the realtime Broadcast
Player scheduler, A/V sync, audio-device clock policy, QNC UI integration, or
export/render. Original MXF audio remains authoritative, and proxy AAC is not
used.

## Runtime Source

The command uses the existing `ProxyPreview` runtime model:

- video source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- preview profile: `journalist-50i-preview`
- prepared slot: first runtime presentation/audio slot

The PipeWire buffer is built from the existing `BroadcastAudioPayloadBinding`.
The command walks the binding's PCM block coverage, copies the referenced
original MXF 24-bit mono-track payload slices, validates track/channel identity,
and converts only at the device boundary.

## Device-Boundary Format

Runtime truth remains:

- source: original MXF PCM
- tracks: 4 mono tracks
- sample rate: 48 kHz
- bit depth: 24-bit signed little-endian PCM payload

PipeWire boundary format:

- `F32Interleaved`
- 48 kHz
- 4 channels
- prototype routing: track 1 -> FL, track 2 -> FR, track 3 -> RL, track 4 -> RR

The conversion is local to the PipeWire boundary. It does not mutate the QGS
runtime PCM model and is not mixdown/export.

## Command

```bash
cargo run -q -p qgs-test -- --broadcast-runtime-audio-pipewire <original-mxf> <proxy-mp4>
```

The command reports:

- runtime source mode
- prepared audio slot index
- presentation range duration
- source PCM block coverage
- source byte count
- output sample/frame/byte geometry
- PipeWire stream and submission evidence
- optional manual audible confirmation status

## Expected Sony FX6 Sample 002 Geometry

For the first `journalist-50i-preview` prepared presentation period:

- presentation range: 40 ms
- source samples: 1920 samples per track
- source blocks: 2 blocks per track, 8 blocks total
- source bytes: 23040
- output frames: 1920
- output channels: 4
- f32 samples: 7680
- output bytes: 30720

This geometry comes from the runtime audio payload binding, not from the older
standalone original-audio segment builder.

## Evidence Policy

Evidence labels remain conservative:

- `RuntimeAudioPayloadSubmittedToPipeWire`: the runtime-prepared payload was
  queued to PipeWire.
- `RuntimeAudioPayloadDrainCompleted`: the runtime-prepared payload was queued
  and a PipeWire drain callback completed.
- `RuntimeAudioPayloadConfirmationRequired`: a non-interactive run submitted or
  drained the payload but did not receive human audible confirmation.
- `RuntimeAudioPayloadAudibleConfirmed`: a human explicitly confirmed hearing
  the runtime-prepared payload.
- `RuntimeAudioPayloadNotHeard`: a human explicitly did not hear it.

Even with drain completion or manual audible confirmation, this milestone does
not upgrade full `AudioDeviceVerified`, realtime playback, A/V sync, channel
certification, or full Broadcast Player playback.

## Local Result

The Step 21I acceptance command was run on the Sony FX6 sample 002
original/proxy pair:

```bash
cargo run -q -p qgs-test -- --broadcast-runtime-audio-pipewire "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF" "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560S03.MP4"
```

Observed result:

- source mode: `ProxyPreview`
- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- prepared audio slot index: 0
- presentation range: 40.000 ms
- presentation sample range: start sample 0, 1920 samples per track
- source blocks: 2 per track, 8 total
- source bytes: 23040
- output format: `F32Interleaved`, 48000 Hz, 4 channels
- output frames: 1920
- f32 samples: 7680
- output bytes: 30720
- PipeWire runtime library/server/pkg-config/headers: available
- stream states: `Connecting`, `Paused`, `Streaming`
- process callback reached: yes
- buffer dequeued: yes
- buffer capacity: 196608 bytes
- buffers submitted: 1
- drain requested: yes
- drain completed: yes
- post-submit callbacks observed: 0
- post-submit timeout: no
- PipeWire evidence level: `DrainCompleted`
- manual confirmation status: `RuntimeAudioPayloadConfirmationRequired`
- runtime payload evidence level: `RuntimeAudioPayloadDrainCompleted`
- `AudioDeviceVerified`: no
- audible output claimed: no

This verifies the runtime-prepared audio payload to PipeWire boundary for one
bounded prepared presentation slot. It does not verify full playback, realtime
playback, A/V sync, channel certification, or full audio-device behavior.

## Not Implemented

- full Broadcast Player playback
- realtime scheduler
- A/V sync
- audio-device clock ownership policy
- production channel routing
- full audio-device verification
- QNC UI integration
- proxy AAC audio path

# M2 Step 20M — QGS Broadcast Player Runtime Test Audio Sink Evidence

Step 20M adds a test-only audio sink evidence path to the QGS Broadcast Player
Runtime. It proves that original MXF PCM audio payloads can cross the runtime
device-boundary contract only when a sink explicitly validates and accepts
them.

This is not real audio playback. No speaker output, PipeWire, ALSA, PulseAudio,
audio device clock, realtime playback loop, waveform UI, mixdown/export, or QNC
UI integration is implemented in this milestone.

## Relationship To Earlier Runtime Work

Step 20I bound prepared audio slots to real original MXF PCM runtime blocks.
Step 20K separated `PayloadReady`, `DevicePayloadReady`, device submission, and
presentation evidence. Step 20L applied that rule to a test video presenter.

Step 20M applies the same rule to audio:

- `PayloadReady` means the Broadcast Player Runtime has bound original MXF PCM
  audio payloads.
- test audio sink evidence means a test-only sink accepted those payloads.
- the evidence does not mean audio was audible on a real device.
- audio sink evidence does not emit `FramePresented`.

## Test Audio Sink Capabilities

The test sink is backend-neutral and explicit about what it accepts:

- original PCM payloads
- signed integer PCM
- 24-bit PCM
- 48 kHz audio
- mono-track PCM block coverage
- audio sink evidence production

The sink rejects bindings whose sample rate, bit depth, track coverage,
completion state, or byte counts do not match the declared payload.

## ProxyPreview Result

For Sony FX6 sample 002 in `ProxyPreview` mode:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- prepared presentation slots: 3
- audio payload bindings submitted to test sink: 3
- accepted: 3
- rejected: 0
- evidence records: 3
- evidence kind: `TestAudioSinkAccepted`
- audio format: signed integer PCM, 24-bit, 48 kHz
- tracks covered: 4
- samples per presentation range: 1920 per track
- bytes per presentation range: 23040
- total bytes accepted: 69120
- audio evidence events: 3
- `FramePresented`: 0

The audio evidence is test-sink acceptance evidence only. It does not claim
speaker output or audible playback.

## OriginalMedia Result

For `OriginalMedia` mode:

- audio source: original MXF
- original audio payload binding is possible
- test audio sink accepted the prepared original-audio ranges
- original video payload remains `CapabilityMissing`
- full OriginalMedia presentation remains not ready

Accepting original audio alone does not imply that OriginalMedia presentation is
ready, because original MXF video payload binding is still not implemented in
the Broadcast Player Runtime.

## Evidence Contract

Audio sink evidence records include:

- presentation slot index
- audio binding index
- start sample
- sample count
- sample rate
- track count
- accepted payload bytes
- evidence kind
- source device kind

They intentionally contain no raw local filesystem paths and no user-private
camera metadata.

## Limitations

Step 20M does not implement:

- real speaker output
- PipeWire, ALSA, or PulseAudio
- real audio device clock
- realtime playback scheduling
- waveform UI
- audio editing
- mixdown/export
- QNC UI integration
- proxy AAC as a primary audio source
- f32 conversion as engine truth
- s16 downconversion

Original MXF audio remains authoritative. Proxy MP4 AAC remains diagnostic or
fallback-only and is not used by the Broadcast Player Runtime audio sink path.

## Next Steps

Future milestones can add a real audio backend that advertises its device
capabilities, accepts or rejects original PCM payloads honestly, and reports
device evidence only when actual audio-device submission or playback evidence
exists.

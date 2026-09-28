# M2 Step 21A — Linux Audio Device Boundary Spike

Step 21A is the first QGS investigation of a real Linux audio device boundary
for the Broadcast Player Runtime. It is a spike and capability probe, not full
audio playback.

The media rule remains unchanged:

- original MXF audio is authoritative
- proxy MP4 video is for preview/edit performance
- proxy MP4 AAC is diagnostic/fallback only and was not used

## Scope

The spike adds a Linux-specific audio probe in `qgs-linux` and a focused
`qgs-test` command:

```bash
cargo run -q -p qgs-test -- --linux-audio-device-probe <original-mxf>
```

The probe keeps `qgs-media-runtime` backend-neutral. It does not add a PipeWire
Rust dependency and does not open an audio stream. Instead it checks the local
PipeWire boundary with safe process/socket inspection:

- `XDG_RUNTIME_DIR/pipewire-0` runtime socket presence
- `pw-cli` availability
- `pw-cli info 0` server reachability
- `wpctl` availability
- `wpctl status` default-output visibility

## Source Audio

For Sony FX6 sample 002, the original MXF audio source is:

- 4 original MXF audio tracks
- 1 mono channel per track
- signed PCM-style original payload
- 48 kHz
- 24-bit
- proxy AAC not used

The probe preserves this source model. It does not convert original PCM to
`f32` or `s16` as runtime truth.

## Local Observation

Command:

```bash
cargo run -q -p qgs-test -- --linux-audio-device-probe "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result on this machine:

- PipeWire runtime socket: yes
- `pw-cli` available: yes
- PipeWire server reachable: yes
- `wpctl` available: yes
- default output device visible: yes
- direct 48 kHz 24-bit original PCM acceptance: unknown
- device-boundary conversion needed: yes
- timing/playback-position evidence available: unknown
- `AudioDeviceVerified`: no
- probe outcome: `CapabilityProbeOnly`

## Device Format Boundary

The source has four mono 24-bit tracks. The current spike cannot prove that the
default Linux audio device accepts that exact engine representation directly.
Future audio-device integration therefore needs an explicit device-boundary
format stage.

Likely future boundary work:

- route or interleave the four mono original tracks
- convert only at the audio device boundary if a backend requires `f32`,
  packed `s24`, `s32`, or another device format
- preserve the original 24-bit PCM block model as QGS runtime truth
- obtain real stream timing or playback-position evidence before claiming
  `AudioDeviceVerified`

## Verification Status

This milestone verifies only capability discovery. It does not verify audible
output.

- `PayloadReady`: already provided by earlier original MXF PCM payload binding
- `DevicePayloadReady`: not claimed by this spike
- `SubmittedToDevice`: not claimed by this spike
- `AudioDeviceVerified`: not claimed by this spike

No fake device evidence is generated.

## Dependency Decision

No new dependency was added.

A future PipeWire integration should be isolated in a Linux-specific crate or
module, not in `qgs-media-runtime`. A direct PipeWire crate is appropriate only
when the next milestone opens/configures a real stream or queries native node
formats in a way command-line probing cannot answer.

## Not Implemented

- full realtime audio playback
- PipeWire stream creation
- ALSA/Pulse/PipeWire speaker output
- audio-device clock
- A/V sync correction
- resampling policy
- waveform UI
- audio editing
- mixdown/export
- QNC UI integration
- proxy AAC primary path

## Recommendation

The next audio milestone should add an isolated PipeWire stream-open prototype
that can negotiate an output format, submit a tiny bounded buffer derived from
original MXF PCM, and report real timing/evidence if PipeWire exposes it
cleanly. Conversion, if required, should remain a device-boundary operation and
must not replace the authoritative original PCM runtime model.

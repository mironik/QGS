# M2 Step 21D — Native PipeWire Stream Creation

Step 21D adds the first isolated native PipeWire stream creation path for QGS.
It follows Step 21C, where the native development boundary became available
after installing PipeWire development packages.

This is still not full playback. No audio buffer is dequeued, written, queued,
or submitted in this milestone.

## Dependency Choice

New crate:

- `qgs-audio-pipewire`

New dependency:

- `pipewire = 0.10.1`
- `libspa-sys = 0.10`

The `pipewire` crate is the upstream Rust binding for PipeWire and is the
smallest direct route to native stream creation. It stays isolated in
`qgs-audio-pipewire`; `qgs-media-runtime` remains backend-neutral and does not
expose PipeWire APIs.

QGS code in the new crate remains `#![forbid(unsafe_code)]`. Unsafe code needed
for PipeWire FFI remains inside the upstream binding crates.

Build requirements:

- `libpipewire-0.3` runtime library
- `libpipewire-0.3` pkg-config metadata
- PipeWire headers
- SPA headers

## Source Audio Rule

The command uses original MXF audio as the authoritative source:

- original MXF audio: 4 mono tracks
- sample rate: 48 kHz
- bit depth: 24-bit signed PCM-style payload
- proxy AAC: not used

The runtime audio truth remains original 24-bit PCM. The stream format below is
only a device-boundary format.

## Stream Format

Selected native stream format:

- sample rate: 48,000 Hz
- sample format: `f32`
- layout: interleaved
- channels: 4
- channel positions: front-left, front-right, rear-left, rear-right

This matches the Step 21B/21C planned boundary closely and avoids introducing a
stereo-only prototype while the source has four original mono tracks.

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype <original-mxf>
```

Observed local command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result:

- PipeWire runtime library available: yes
- PipeWire server reachable: yes
- PipeWire pkg-config entry available: yes
- PipeWire headers available: yes
- stream create attempted: yes
- stream created: yes
- stream configured: yes
- selected stream format: `F32Interleaved`, 48,000 Hz, 4 channels
- observed stream states: `Connecting`, `Paused`
- final stream state: `Paused`
- buffer dequeued: no
- buffer submitted: no
- evidence level: `NativeStreamConfigured`
- `AudioDeviceVerified`: no

## Evidence Meaning

`NativeStreamConfigured` means QGS created a native PipeWire playback stream and
observed it reach a configured PipeWire state. It does not mean audio was
submitted, heard, or played.

The following remain false:

- `BufferSubmitted`
- `AudioDeviceVerified`
- full Broadcast Player playback
- realtime A/V sync

## Not Implemented

- audio buffer dequeue/write/queue
- long-running audio playback
- realtime Broadcast Player scheduler
- A/V sync
- audio-device clock
- video output
- waveform UI
- export/render
- QNC UI integration
- proxy AAC primary path

## Next Step

The next milestone should add bounded PipeWire buffer dequeue/write/queue for a
tiny original-audio-derived device-boundary buffer. Only a real queued buffer
should upgrade evidence to `BufferSubmitted`. `AudioDeviceVerified` should
remain narrowly scoped until QGS obtains stronger stream callback/device
evidence.

# M2 Step 21M — PipeWire Desktop Mono Listening Helper

Step 21M added a limited PipeWire desktop listening helper for one original MXF
mono audio source track.

This is not the correct broadcast channel output model and must not be treated
as production routing. Broadcast/news audio remains discrete mono-channel based:

- original MXF audio consists of separate mono tracks
- each original mono track remains individually addressable
- QGS Broadcast Player Runtime preserves mono channel identity
- correct broadcast direction is track 1 -> output channel 1, track 2 -> output
  channel 2, track 3 -> output channel 3, and track 4 -> output channel 4
- proxy AAC is not used as runtime audio
- duplicated L/R output is only an ad-hoc desktop listening helper

## Command

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-mono-monitor <original-mxf> <proxy-mp4> \
  --track <n> \
  --start-ms <ms> \
  --duration-ms <ms>
```

Defaults for the Mironik 2002 diagnostic corpus:

- track: 4
- start: 0 ms
- duration: 1000 ms

Example:

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-mono-monitor \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002.MXF" \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002S03.MP4"
```

## Source Rule

The command uses original MXF PCM only. Proxy AAC is not used. The proxy MP4 is
accepted only for media-context consistency with the original/proxy workflow.

The selected original mono track is converted at the PipeWire device boundary
to f32 interleaved stereo by duplicating the same mono source to left and right:

```text
source: original mono track N
monitor output: duplicated mono to L/R
```

This duplication only proves that selected source samples can be made available
to a desktop stereo monitor helper. It does not prove channel-correct broadcast
output, discrete mono output, production routing, channel certification, or full
audio device verification. It does not alter QGS runtime audio truth.

## Mironik 2002 Default

Step 21L identified track 4 / track 1 as the preferred diagnostic desktop
monitor pair for the Mironik 2002 0-1000 ms range. Step 21M narrowed the
desktop listening helper to a single mono source, starting with track 4 because
it was the loudest source in that range.

For the default range:

- selected original track: 4
- selected sample range: 0..48000
- output frames: 48000
- output channels: 2
- f32 samples: 96000
- output bytes: 384000
- output buffers: 50 20 ms buffers

The command reports selected track RMS/peak information, PipeWire stream state,
buffer submission, drain status, manual confirmation status, and a scoped
evidence label.

## Evidence

Possible evidence labels include:

- `DesktopMonoListeningHelperSubmitted`
- `DesktopMonoListeningHelperDrainCompleted`
- `ManualDesktopMonoListeningHelperHeard`
- `ManualDesktopMonoListeningHelperNotHeard`
- `ManualDesktopMonoListeningHelperConfirmationRequired`

These labels are below `AudioDeviceVerified`. They do not certify:

- discrete 4-mono PipeWire output
- channel routing
- production routing
- speaker calibration
- full Broadcast Player playback
- realtime playback
- A/V sync

## Not Implemented

Step 21M does not implement:

- discrete track 1/2/3/4 to output channel 1/2/3/4 behavior
- production audio routing
- channel certification
- full Broadcast Player playback
- realtime scheduling
- A/V sync
- audio device clock policy
- QNC UI integration
- export/render

## Next Step

The next required diagnostic is:

```text
M2 Step 21N — Discrete 4-Mono PipeWire Output Boundary
```

Step 21N should output original MXF tracks 1, 2, 3, and 4 as four discrete mono
channels using a 4-channel PipeWire boundary where possible. It must preserve
channel identity, avoid stereo fold, avoid duplicated mono as proof, avoid L/R
semantic claims, and still avoid production routing certification or
`AudioDeviceVerified` upgrades until those are separately proven.

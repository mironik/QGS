# M2 Step 21M — PipeWire Mono Channel Monitor Diagnostic

Step 21M adds a diagnostic PipeWire monitor command for listening to one
authoritative original MXF mono audio track at a time.

This is not production routing and not a new Broadcast Player playback model.
Broadcast/news audio remains mono-channel based:

- original MXF audio consists of separate mono tracks
- each original mono track remains individually addressable
- QGS Broadcast Player Runtime preserves mono channel identity
- proxy AAC is not used as runtime audio
- duplicated L/R output is only a temporary desktop monitor helper

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

This duplication is a desktop listening helper. It does not alter the QGS
runtime audio truth and does not define production routing.

## Mironik 2002 Default

Step 21L identified track 4 / track 1 as the preferred diagnostic stereo monitor
pair for the Mironik 2002 0-1000 ms range. Step 21M narrows the next diagnostic
boundary to individual mono sources, starting with track 4 because it was the
loudest source in that range.

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

- `MonoTrackMonitorSubmitted`
- `MonoTrackMonitorDrainCompleted`
- `ManualMonoTrackMonitorHeard`
- `ManualMonoTrackMonitorNotHeard`
- `ManualMonoTrackMonitorConfirmationRequired`

These labels are below `AudioDeviceVerified`. They do not certify:

- channel routing
- production routing
- speaker calibration
- full Broadcast Player playback
- realtime playback
- A/V sync

## Not Implemented

Step 21M does not implement:

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
M2 Step 21N — PipeWire Mono Channel Monitor Diagnostic
```

Step 21N should continue individual mono-channel testing across the original
MXF tracks and report which mono channels carry recognizable content for the
selected Mironik 2002 ranges.

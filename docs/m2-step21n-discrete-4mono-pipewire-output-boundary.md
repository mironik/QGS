# M2 Step 21N — Discrete 4-Mono PipeWire Output Boundary

Step 21N corrects the audio-device direction after the Step 21M desktop
listening helper.

The QGS broadcast/news audio model is discrete mono-channel audio. Original MXF
audio remains authoritative, each original mono track remains individually
addressable, and proxy AAC is not used as runtime audio.

## Why This Exists

Step 21M duplicated one selected mono source to L/R so it could be heard on a
normal desktop monitor path. That was useful as an ad-hoc listening helper, but
it is not channel-correct broadcast output.

Step 21N instead submits four separate original MXF mono sources to a 4-channel
PipeWire device boundary:

- original track 1 -> output channel 1
- original track 2 -> output channel 2
- original track 3 -> output channel 3
- original track 4 -> output channel 4

No stereo fold is used. No mono source is duplicated. No L/R semantic claim is
made by QGS.

## Command

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-discrete-4mono <original-mxf> <proxy-mp4> \
  --start-ms <ms> \
  --duration-ms <ms>
```

Default diagnostic range:

- start: 0 ms
- duration: 1000 ms

Mironik 2002 example:

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-discrete-4mono \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002.MXF" \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002S03.MP4" \
  --start-ms 0 \
  --duration-ms 1000
```

## Source And Output Geometry

For the Mironik 2002 start-0, duration-1000 ms range:

- audio source: original MXF
- proxy AAC: not used
- bounded extraction: yes
- full MXF loaded into memory: no
- samples per track: 48000
- source tracks: 4
- output format: F32Interleaved
- sample rate: 48000 Hz
- output channels: 4
- output frames: 48000
- f32 samples: 192000
- output bytes: 768000
- output buffers: 50 20 ms buffers

PipeWire may expose API/device position labels such as FL/FR/RL/RR for a
4-channel stream. Those labels are not QGS production routing semantics. QGS
source identity remains track 1/2/3/4 to output channel 1/2/3/4.

## Evidence

Successful submission/drain is reported as:

```text
Discrete4MonoOutputDrainCompleted
```

This means QGS submitted original tracks 1, 2, 3, and 4 as four separate f32
interleaved output channels and observed PipeWire drain completion.

This does not prove:

- physical device channel mapping
- production routing policy
- channel certification
- speaker calibration
- full Broadcast Player playback
- realtime playback
- A/V sync
- `AudioDeviceVerified`

## Current Result

On the local PipeWire system, the command submits and drains the bounded
Mironik 2002 4-mono range through a native 4-channel f32 PipeWire stream. The
verification matrix is upgraded only to `Discrete4MonoOutputDrainCompleted`,
which remains below `AudioDeviceVerified`.

## Next Step

The next audio-device milestones should focus on physical channel mapping and
device policy without weakening the source model. QGS still needs a production
routing policy and audio-device verification criteria before any stronger
broadcast output claim.

## Not Implemented

Step 21N does not implement:

- physical channel certification
- production routing policy
- full Broadcast Player playback
- realtime scheduler
- A/V sync
- QNC UI integration
- export/render

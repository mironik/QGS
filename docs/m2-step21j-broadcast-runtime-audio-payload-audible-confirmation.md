# M2 Step 21J — Broadcast Runtime Audio Payload Audible Confirmation

Step 21J adds an explicit manual audible confirmation path for the QGS
Broadcast Player Runtime audio payload to PipeWire boundary.

This builds directly on Step 21I. Step 21I proved that the first prepared
`ProxyPreview` runtime audio payload can be submitted to native PipeWire and
drained. Step 21J keeps that same payload source but repeats it for a short,
bounded helper window so a human can confirm whether it is audible.

This is still not full playback. It is not realtime Broadcast Player playback,
not timeline playback, not A/V sync, not channel certification, and not full
`AudioDeviceVerified`.

## Source Rule

The source remains the QGS Broadcast Player Runtime prepared audio binding:

- source mode: `ProxyPreview`
- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- prepared audio slot: 0
- original PCM truth: 4 mono tracks, 48 kHz, 24-bit

The f32 interleaved buffer is a PipeWire device-boundary conversion only. The
runtime PCM model remains original MXF 24-bit PCM.

## Why a Helper Repeat Exists

The prepared runtime payload for one `journalist-50i-preview` presentation
period is only 40 ms:

- source blocks: 2 per track, 8 total
- source bytes: 23040
- output frames: 1920
- output channels: 4
- output bytes: 30720

Forty milliseconds is too short for reliable manual audibility. The audible
confirmation command repeats the exact same prepared runtime payload for a
bounded helper duration. This is an audibility boundary test, not sequential
timeline playback.

## Command

```bash
cargo run -q -p qgs-test -- --broadcast-runtime-audio-pipewire-audible <original-mxf> <proxy-mp4>
```

The command asks:

```text
Did you hear the runtime-prepared original-audio payload from the default PipeWire output? yes/no:
```

If no interactive answer is available, the result remains
`RuntimeAudioPayloadConfirmationRequired`.

## Helper Geometry

The helper uses a 500 ms target and whole repeated 40 ms payloads:

- repeat count: 13
- helper duration: 520 ms
- frames submitted: 24960
- bytes planned: 399360

Each submitted buffer is still the same runtime-prepared original-audio payload
from prepared audio slot 0.

## Evidence Labels

- `RuntimeAudioPayloadDrainCompleted`: the runtime-prepared payload was
  submitted and drained.
- `RuntimeAudioPayloadConfirmationRequired`: submission/drain completed, but no
  manual audible answer was available.
- `RuntimeAudioPayloadAudibleConfirmed`: a human explicitly confirmed hearing
  the repeated runtime-prepared payload.
- `RuntimeAudioPayloadNotHeard`: a human explicitly reported not hearing it.

These labels do not imply full playback, realtime playback, A/V sync, channel
certification, or full `AudioDeviceVerified`.

## Local Result

The Step 21J acceptance command was run on the Sony FX6 sample 002
original/proxy pair:

```bash
cargo run -q -p qgs-test -- --broadcast-runtime-audio-pipewire-audible "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF" "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560S03.MP4"
```

Observed result:

- source mode: `ProxyPreview`
- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- full playback: no
- realtime Broadcast Player playback: no
- A/V sync: no
- prepared audio slot index: 0
- single payload duration: 40.000 ms
- source blocks: 2 per track, 8 total
- source bytes: 23040
- output format: `F32Interleaved`, 48000 Hz, 4 channels
- output frames per payload: 1920
- output bytes per payload: 30720
- helper repeat enabled: yes
- helper repeat count: 13
- helper repeat duration: 520.000 ms
- total frames submitted: 24960
- total bytes copied: 399360
- stream configured: yes
- stream states: `Connecting`, `Paused`, `Streaming`
- process callback reached: yes
- buffer dequeued: yes
- buffer capacity: 196608 bytes
- buffers submitted: 13
- drain requested: yes
- drain completed: yes
- post-submit callbacks observed: 0
- post-submit timeout: no
- PipeWire evidence level: `DrainCompleted`
- manual confirmation status: `RuntimeAudioPayloadConfirmationRequired`
- runtime payload evidence level: `RuntimeAudioPayloadDrainCompleted`
- `AudioDeviceVerified`: no
- audible output claimed: no

The command was run non-interactively, so no manual audible answer was
available. The result is a successful bounded runtime-payload submission/drain
through the audible helper, not an audible confirmation.

## Not Implemented

- full Broadcast Player playback
- realtime scheduler
- A/V sync
- media/device clock policy
- video sync
- QNC UI integration
- channel certification
- production routing
- export/render

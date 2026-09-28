# M2 Step 21G — PipeWire Audible Smoke Test Boundary

Step 21G adds a controlled manual audible smoke-test boundary for the native
PipeWire path. It is still not full playback and it does not automatically
verify speaker output.

The smoke test exists to answer one narrow question: can QGS submit a short,
bounded, original-audio-derived signal to the default PipeWire output so a human
can confirm whether sound was heard?

## Relationship To Step 21F

Step 21F proved one 20 ms original-audio-derived buffer can be submitted to a
native PipeWire stream and followed by a safe drain callback:

- stream configured: yes
- buffer submitted: yes
- drain requested/completed: yes
- evidence level: `DrainCompleted`
- `AudioDeviceVerified`: no
- audible output claimed: no

Step 21G keeps that same native boundary and extends it to a short bounded smoke
sequence.

## Source Audio Rule

The smoke test does not use proxy AAC.

Source:

- original MXF PCM
- 4 mono tracks
- 48 kHz
- 24-bit signed little-endian payload

Device-boundary conversion:

- f32 interleaved
- 48 kHz
- 4 channels

The original 24-bit PCM runtime model remains unchanged. The f32 buffer is only
for the PipeWire device-boundary smoke test.

## Smoke-Test Shape

The current smoke test repeats the first original-audio-derived 20 ms segment:

- samples per source segment: 960 samples per track
- segment duration: 20 ms
- repeated buffers: 25
- total smoke-test duration: 500 ms
- output channels: 4
- f32 samples submitted: 96,000
- bytes submitted: 384,000

This repeat is a smoke-test construction, not real media playback and not an
export/mixdown path.

## Routing

Prototype channel routing:

| Original track | PipeWire channel |
| --- | --- |
| track 1 | FL |
| track 2 | FR |
| track 3 | RL |
| track 4 | RR |

This is not production channel-routing policy, speaker calibration, or channel
mapping certification.

## Manual Confirmation Model

The command requires explicit human confirmation before producing an audible
smoke-test confirmation label.

Possible confirmation states:

- `ManualAudibleSignalDetectedContentUnverified`
- `ManualAudibleSmokeTestNotHeard`
- `ManualAudibleConfirmationRequired`

Noninteractive runs report `ManualAudibleConfirmationRequired`. QGS does not
silently assume that sound was heard.

Even when a human answers yes, this remains a narrow signal-detection result.
It does not confirm recognizable original audio content, PCM content
correctness, channel routing, full `AudioDeviceVerified`, realtime playback,
A/V sync, broadcast timing verification, or speaker calibration.

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-audible-smoke-test <original-mxf>
```

Observed local command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-audible-smoke-test "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed local manual result:

- audio source: original MXF
- proxy AAC: not used
- generated tone primary evidence: no
- full playback: no
- realtime Broadcast Player playback: no
- A/V sync: no
- routing: track 1 -> FL, track 2 -> FR, track 3 -> RL, track 4 -> RR
- stream configured: yes
- observed stream states: `Connecting`, `Paused`, `Streaming`
- final stream state: `Streaming`
- process callback reached: yes
- smoke-test source range per buffer: 960 samples per track
- smoke-test duration: 500.000 ms
- buffers planned: 25
- buffers submitted: 25
- samples submitted per track: 24,000
- output channels: 4
- f32 samples written: 96,000
- bytes copied: 384,000
- drain requested: yes
- drain completed: yes
- PipeWire evidence level: `DrainCompleted`
- manual confirmation answer: yes, later clarified
- manual confirmation status: `ManualAudibleSignalDetectedContentUnverified`
- smoke-test evidence level: `ManualAudibleSignalDetectedContentUnverified`
- heard result: slight hum/buzz only
- recognizable original audio content confirmed: no
- `AudioDeviceVerified`: no
- `AudioDeviceVerified` scope: not upgraded by smoke test
- audible signal detected: yes
- audible original audio content claimed: no

## Correction Note

The earlier Step 21G note used `ManualAudibleSmokeTestConfirmed`. The user later
clarified that the heard output was only a slight hum/buzz, not recognizable
original audio content. The evidence is therefore corrected to
`ManualAudibleSignalDetectedContentUnverified`.

The positive evidence remains:

- native PipeWire stream configured
- buffers submitted
- drain completed
- original MXF path used
- proxy AAC not used

The correction means this smoke test does not verify PCM content correctness,
channel routing, full audio device output, or full `AudioDeviceVerified`.

## Verification Matrix

Step 20Q now includes a separate `native PipeWire audible smoke test` row. Its
current evidence is `ManualAudibleSignalDetectedContentUnverified`: a human
detected a slight hum/buzz from the bounded original-audio-derived PipeWire
smoke test, but recognizable original audio content was not confirmed.

This remains separate from full Broadcast Player playback, realtime playback,
A/V sync, full `AudioDeviceVerified`, channel certification, and speaker
calibration.

## Not Implemented

- production playback
- realtime Broadcast Player scheduler
- A/V sync
- media clock/device clock policy
- speaker calibration
- channel mapping certification
- waveform UI
- editing
- mixdown/export
- QNC UI integration
- proxy AAC primary path

## Next Step

The next audio milestone should move toward a bounded multi-buffer device stream
with explicit device timing observations, underrun/error handling, and a clear
definition for when narrow smoke-test evidence can graduate toward real
`AudioDeviceVerified`.

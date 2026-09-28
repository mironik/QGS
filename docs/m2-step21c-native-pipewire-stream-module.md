# M2 Step 21C — Native PipeWire Stream Module Boundary

Step 21C follows the command-backed `pw-cat` prototype from Step 21B and checks
whether QGS can move to an isolated native PipeWire stream module.

This milestone is still not full playback. It does not implement a realtime
Broadcast Player scheduler, A/V sync, UI integration, waveform work, export, or
production audio output policy.

## Source Audio

The command uses original MXF audio only:

- source: Sony FX6 sample 002 original MXF
- tracks: 4 mono tracks
- sample rate: 48 kHz
- bit depth: 24-bit
- proxy AAC: not used

The QGS runtime audio truth remains original 24-bit PCM. The planned device
format is local to the PipeWire device boundary.

## Planned Native Boundary

The native module boundary is intended to:

1. connect to PipeWire
2. create/configure an output stream
3. choose or negotiate a simple device format
4. dequeue a PipeWire buffer
5. copy a tiny bounded original-audio-derived buffer into it
6. queue the buffer
7. observe callback/evidence from PipeWire

The planned prototype buffer matches Step 21B:

- input: four original MXF 24-bit mono PCM tracks
- output boundary format: `f32` interleaved
- channels: 4
- sample rate: 48 kHz
- samples: 960
- duration: 20 ms
- bytes: 15,360

This is device-boundary conversion only. It is not mixdown/export and does not
change the original PCM runtime model.

## Dependency Boundary

No Rust PipeWire dependency was added.

Step 21C distinguishes the PipeWire runtime boundary from the native
development boundary needed by the Rust `pipewire` crate or by a safe native
wrapper in this workspace.

Before PipeWire development packages were installed, the machine had the
runtime library but not the native build surface:

- `libpipewire-0.3.so.0`: available
- `pkg-config --exists libpipewire-0.3`: unavailable
- `/usr/include/pipewire-0.3`: unavailable
- `/usr/include/spa-0.2`: unavailable

That state is classified as `NativeDevelopmentBoundaryMissing`.

After installing the PipeWire development packages, the native build surface is
available:

- `libpipewire-0.3.so.0`: available
- `pkg-config --exists libpipewire-0.3`: available
- `/usr/include/pipewire-0.3`: available
- `/usr/include/spa-0.2`: available

That state is classified as `NativeStreamNotImplemented`. It means the machine
can now build a native PipeWire module, but QGS has not yet implemented stream
creation, buffer dequeue, or buffer queueing. QGS therefore still stops before
claiming buffer submission.

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype <original-mxf>
```

Observed local command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-native-prototype "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF"
```

Observed result before PipeWire development packages:

- PipeWire runtime library available: yes
- PipeWire server reachable: yes
- PipeWire pkg-config entry available: no
- PipeWire headers available: no
- stream create attempted: no
- buffer dequeued: no
- buffer submitted: no
- evidence level: `NativeDevelopmentBoundaryMissing`
- `AudioDeviceVerified`: no

Observed result after PipeWire development packages:

- PipeWire runtime library available: yes
- PipeWire server reachable: yes
- PipeWire pkg-config entry available: yes
- PipeWire headers available: yes
- stream create attempted: no
- buffer dequeued: no
- buffer submitted: no
- evidence level: `NativeStreamNotImplemented`
- `AudioDeviceVerified`: no

## Evidence Result

Step 21C does not upgrade evidence beyond the Step 21B command-backed
`StreamOpened` result.

The native boundary evidence before installing development packages was:

- native module build boundary missing
- no native stream created
- no PipeWire buffer dequeued
- no PipeWire buffer queued
- no audio device verification claimed

The native boundary evidence after installing development packages is:

- native module build boundary available
- native stream creation not implemented yet
- no native stream created
- no PipeWire buffer dequeued
- no PipeWire buffer queued
- no audio device verification claimed

This remains an honest acceptance outcome under the milestone rules. Available
headers and pkg-config metadata are necessary for native work, but they are not
evidence that a buffer was submitted.

## Not Implemented

- native PipeWire Rust stream callbacks
- PipeWire buffer dequeue/queue
- full playback
- realtime scheduler
- A/V sync
- audio-device clock
- UI integration
- export/render
- waveform
- proxy AAC primary path
- production audio output policy

## Next Steps

The next milestone can now add an isolated native PipeWire module that owns
stream creation, callbacks, buffer lifecycle, and evidence reporting. Only a
real queued buffer should be classified as `BufferSubmitted`; only stronger
callback/device evidence should move toward `AudioDeviceVerified`.

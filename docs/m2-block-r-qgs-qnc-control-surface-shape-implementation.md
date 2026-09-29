# M2 Block R - QGS QNC Control Surface Shape Implementation

Block R implements the QGS-side shape of a QNC-compatible Broadcast Player
control surface.

This is an in-process facade over the existing QGS Broadcast Player assembly,
control core, operational runtime facts, fault/recovery model, and device
selection model. It does not create the future shared contract crate, IPC, QNC
UI integration, device backends, realtime playback, A/V sync certification, or
export/render behavior.

## Relationship To Block P And Block Q

Block P planned the future `qnc-qgs-contract` field surface. Block R implements
the QGS-side draft shape only, inside `qgs-media-runtime`, so the fields can be
exercised before the shared crate is created.

Block Q maps QNC `PreparedInput`-shaped descriptors into QGS descriptors,
`QgsInputPlan`, and `QgsBroadcastPlayerAssembly`. Block R consumes that already
prepared assembly context. It does not invent a second input model and it does
not parse private media paths inside the control surface.

## Control Surface vs Assembly

`QgsBroadcastPlayerAssembly` remains the composition root for the Lego modules:
input planning, transport, prepared windows, payload providers, device
selection, events, faults, snapshots, and session/control surfaces.

`QgsQncControlSurface` sits above the assembly/control core and provides:

- QNC-shaped command request envelopes.
- QNC-shaped command reply envelopes.
- Generation checks.
- Public-safe runtime snapshots.
- Public-safe event envelopes.
- Fault and recovery projections.

It does not own decode, frame-clock math, prepared-window policy, or device
backend internals.

## Command Request Shape

`QgsQncCommandRequestEnvelope` contains:

- `command_id`
- optional `session_id`
- optional `expected_generation`
- command kind
- command payload
- optional `client_tag`

Supported command kinds:

- `LoadPreparedInput`
- `Prepare`
- `Cue`
- `Preroll`
- `Play`
- `Pause`
- `Seek`
- `Stop`
- `Unload`
- `Snapshot`

Payloads are intentionally small:

- `Frame { frame }` for cue/seek.
- `Play { frame_count }` for bounded logical frame stepping.
- `Preroll { target_frame }` for prepared-window refresh.
- `Empty` for the remaining commands.

## Command Reply Shape

`QgsQncCommandReplyEnvelope` returns:

- echoed command id
- accepted/rejected result
- generation before/after
- status before/after
- state mutation flag
- optional rejection reason
- optional fault record
- optional recovery action
- current snapshot
- projected events
- private path exposure flag

Every command returns a snapshot. Rejected commands return a snapshot without
mutating state.

## Generation Rules

Generation starts at `0`.

Accepted mutating commands increment generation. `Snapshot` does not increment
generation. Rejected commands do not increment generation.

If a request carries an `expected_generation` that differs from the current
generation, the control surface rejects it before touching the runtime:

- `accepted=false`
- `rejection_reason=StaleGeneration`
- `state_mutated=false`
- `generation_before == generation_after`
- `CommandRejected` event emitted
- snapshot returned

## Snapshot Shape

`QgsQncRuntimeSnapshot` includes:

- generation
- player status
- source loaded flag
- public-safe source URI
- source mode
- picture representation
- authoritative audio source
- proxy AAC authority flag
- broadcast audio model
- active range
- current frame
- current audio sample range
- prepared window
- video/audio payload readiness
- buffer status
- device status
- active faults
- warnings
- private path exposure flag

The snapshot preserves the current media truth:

- ProxyPreview uses proxy MP4 video.
- Original MXF audio is authoritative.
- Proxy AAC is not authoritative.
- Broadcast audio remains discrete original MXF mono lanes.
- Public output must not expose private local paths.

## Device Status Shape

`QgsQncControlDeviceStatus` projects the existing device selection facts:

- device policy
- selected video backend
- selected audio backend
- QNC OS display target
- X11 target status
- real display status
- presenter evidence level
- audio device status
- visual/realtime/audio-device/A-V-sync verified flags

The current QNC OS display target remains Wayland + Vulkan. X11 remains
legacy/non-target only. Real display output, visual verification, realtime
verification, production audio-device verification, and A/V sync verification
remain false.

## Event Envelope Shape

`QgsQncControlEventEnvelope` carries:

- sequence
- generation
- event kind
- optional command id
- optional status
- optional frame
- optional audio sample range
- optional prepared window
- optional fault
- optional recovery action
- optional evidence level
- private path exposure flag

Event kinds include:

- `SessionCreated`
- `SourceLoaded`
- `PreparedInputAccepted`
- `Cued`
- `PrerollReady`
- `Started`
- `Ticked`
- `Paused`
- `Seeked`
- `Stopped`
- `Unloaded`
- `CommandRejected`
- `FaultRecorded`
- `RecoverySuggested`
- `SnapshotReported`

No real-backend `FramePresented` event is emitted by this block.

## Fault And Recovery Shape

`QgsQncFaultRecord` projects the existing QGS operational fault model into a
QNC-safe shape:

- fault kind
- severity
- scope
- recoverable flag
- recovery action
- optional command id
- state mutation flag
- QNC-safe code
- QNC-safe message
- private path exposure flag

The mapping is conservative. It is enough for QNC-shaped observation and command
rejection handling, but it is not yet the final shared crate fault schema.

## qgs-test Control Session Update

`--qgs-broadcast-player-control-session` now drives
`QgsQncControlSurface` instead of calling the operational runtime directly.

The command prints:

- command id
- generation before/after
- accepted/rejected result
- status
- frame/audio/window
- stale-generation rejection behavior
- final passive view facts
- non-claims for real display, visual verification, realtime verification,
  production audio, A/V sync, and `FramePresented`

## Sample 002 Result

The Sony FX6 sample 002 control-session run remains ProxyPreview:

- video: proxy MP4
- audio: original MXF mono lanes
- proxy AAC authoritative: no
- private source path: hidden
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real-display `FramePresented`: no

The default script runs through load, prepare, cue, preroll, play, pause, seek,
preroll, play, stop, unload, then demonstrates a stale-generation rejection.

## Mironik 2002 Result

The Mironik 2002 command path uses the same QGS-side control surface and keeps
the same boundaries:

- original MXF audio is authoritative
- proxy AAC is not authoritative
- private paths remain hidden
- no realtime/device/display claim is made

## Limitations

Block R does not implement:

- shared `qnc-qgs-contract` crate
- IPC
- QNC UI/client protocol
- QNC DB/workflow integration
- Wayland/Vulkan display presenter
- X11
- DRM/KMS
- production PipeWire audio
- realtime scheduler
- A/V sync certification
- export/render

## Next Recommended Block

Recommended next step:

```text
M2 Block S - QNC Bridge Prototype Plan
```

That block should decide how the QGS-side control surface is hosted and how QNC
will connect to it without importing UI/DB/workflow concerns into QGS.

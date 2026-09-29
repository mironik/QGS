# M2 Phase 27 - QNC-Compatible Command/Event Projection

Phase 27 adds a backend-neutral projection layer that translates internal QGS
runtime facts into a QNC-compatible command, event, passive-view, timeline, and
monitor shape.

This is projection only. It is not IPC, a process launcher, QNC UI, realtime
playback, A/V sync, device output, video presentation, audio output policy, or
export/render.

## Relationship To Step 22A

Step 22A identified the missing `qnc-player-client`, timeline, and monitor
projection gaps. QNC remains the DB, UI, application workflow, and orchestration
layer. QGS owns backend readiness, source lifecycle, timing facts, prepared
state, and evidence distinctions.

Phase 27 mirrors the audited QNC roles without importing QNC crates:

- a future client can issue commands against public `qnc://` source identity
- a future timeline can observe active range, cue, carrier, and prepared window
- a future monitor can observe prepared frame descriptors without claiming
  display presentation
- QNC UI remains an observer/controller, not the owner of backend timing facts

## Relationship To Phases 22-26

Phase 22 provides the descriptor and `QgsInputPlan` used by this projection.
Private filesystem paths remain transport bindings only.

Phase 23 provides source handles, source revisions, active source, active range,
cue, prepared anchor, and `play_ready`.

Phase 24 provides rational frame/sample timing facts.

Phase 25 provides bounded prepared/playout buffer facts. Prepared does not mean
submitted, presented, audible, realtime, or verified.

Phase 26 provides source close/unload lifecycle and an internal runtime event
envelope. Phase 27 projects those internal envelopes into QNC-facing public
event envelopes.

## Command Envelope Model

`QgsQncCommandEnvelope` models the command shape a future QNC application can
send through an IPC/protocol layer later. The current phase does not implement
that IPC layer.

The modeled command variants are:

- `LoadPreparedInput`
- `PreloadSource`
- `SetActiveSource`
- `SetActiveRange`
- `Cue`
- `PrepareAnchor`
- `Play`
- `Pause`
- `Stop`
- `TickPrepare`
- `CloseActiveSource`
- `UnloadSource`

Each command envelope carries a command id, optional expected generation, public
source URI, command variant, and public payload summary. It must not contain raw
local paths.

## Projected Event Model

`QgsQncEventEnvelope` projects `QgsRuntimeEventEnvelope` into a QNC-compatible
event shape:

- sequence
- generation
- optional public source URI
- projected event kind
- public payload summary
- evidence status

Projected event kinds include source loaded/preloaded, active source/range,
cue, prepared anchor, playback readiness, transport state, tick preparation,
prepared buffer changes, source close/unload, command rejection, and runtime
generation changes.

The projection does not emit `FramePresented`, `AudioDeviceVerified`,
`RealtimeVerified`, or display-presented events.

## Passive View Model

`QgsQncPassiveView` is a snapshot of QGS backend facts. It includes:

- runtime generation
- current transport state
- current public source identity
- active range
- cue and prepared anchor facts
- `play_ready`
- loaded source count
- prepared buffer summary
- evidence summary
- private path exposure flag

This is not UI state, DB state, or workflow ownership.

## Timeline Projection

`QgsQncTimelineProjection` provides enough facts for a future passive QNC
timeline module:

- active range
- carrier frame
- cue frame
- prepared frame window
- frame rate
- original-audio sample rate
- presented-frame claim flag, currently false

It does not implement a timeline UI.

## Monitor Projection

`QgsQncMonitorProjection` provides a monitor placeholder:

- whether a prepared descriptor exists
- source frame number
- payload evidence status
- presented flag, currently false
- real display evidence, currently none

The monitor can observe prepared frame facts. It cannot claim real display
output in this phase.

## Evidence View

The QNC-facing evidence model keeps the important distinctions visible:

- `Prepared`
- `SubmittedToDevice`
- `Presented`
- `Verified`

For Phase 27, prepared buffer facts are `Prepared`; submitted, presented,
verified, realtime, frame-presented, and audio-device-verified facts remain not
implemented or false.

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-qnc-event-projection <original-mxf> <proxy-mp4>
```

The command:

1. builds the Phase 22 descriptor
2. builds `QgsInputPlan`
3. runs the Phase 23 load/preload/active/range/cue/anchor flow
4. runs one Phase 25 tick preparation
5. runs a Phase 26 unload lifecycle event
6. projects internal events to QNC-compatible event envelopes
7. prints command envelope examples
8. prints passive view snapshots before and after unload
9. prints timeline projection
10. prints monitor projection
11. prints evidence summary

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- public original URI: `qnc://local/media/original/Mironik-1560`
- public proxy URI: `qnc://local/media/proxy/Mironik-1560`
- private path exposed: no
- projected event sequence monotonic: yes
- passive view before unload: `Ready`, `play_ready=true`, prepared buffer count 6
- passive view after unload: `Empty`, no active source, `play_ready=false`
- timeline projection: active range `[0..50)`, prepared window `[0..6)`
- monitor projection: prepared descriptor present, presented false, real display evidence none
- evidence: prepared only; realtime, A/V sync, `FramePresented`, and
  `AudioDeviceVerified` remain no

For Mironik 2002:

- public original URI: `qnc://local/media/original/Mironik-2002`
- public proxy URI: `qnc://local/media/proxy/Mironik-2002`
- same projection behavior over the first 1000 ms active range
- private path exposed: no
- no realtime/device/presentation claim

## Non-Claims

Phase 27 does not implement:

- real QNC IPC
- process launcher
- QNC UI
- DB writes
- realtime scheduler
- video presenter
- audio output policy
- A/V sync
- export/render
- `FramePresented`
- `AudioDeviceVerified`
- `RealtimeVerified`

## Phase 28 Direction

Phase 28 should turn this projection into a stricter session/command execution
boundary: generation-checked command application, bounded command queue rules,
and a public transcript suitable for a future QNC protocol adapter while still
keeping QNC crates out of QGS.

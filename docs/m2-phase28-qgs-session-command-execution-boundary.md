# M2 Phase 28 - QGS Session Command Execution Boundary

Phase 28 adds a simple backend-neutral command execution boundary on top of the
Phase 27 QNC-compatible command/event projection.

This is not IPC, a process launcher, QNC UI, realtime playback, A/V sync, video
presenter work, audio output policy, DB work, or export/render.

## Relationship To Phase 27

Phase 27 defined QNC-shaped command envelopes, projected event envelopes,
passive views, timeline projections, monitor placeholders, and evidence views.
Those were projection types only.

Phase 28 adds an in-process `QgsQncSessionCommandExecutor` that applies those
command envelopes to the existing QGS transport/runtime skeleton. It still does
not define a wire protocol or import QNC crates.

## Command Execution Model

The executor owns:

- a Phase 22 `QgsInputPlan`
- a Phase 23 `QgsTransportEngine`
- a Phase 25 bounded prepared/playout buffer when `TickPrepare` is requested
- a Phase 26 runtime event log
- the current runtime generation

Supported command variants are:

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

Accepted commands call existing QGS transport and buffer methods. Rejected
commands return an outcome and current passive view without mutating runtime
state.

## Generation Check

Commands may carry an expected runtime generation. If present, it must match the
executor's current generation before any mutation occurs.

Generation mismatch returns:

- `accepted=false`
- reason `runtime generation mismatch`
- no projected events
- unchanged passive view

Source close/unload is a lifecycle boundary and increments the runtime
generation.

## Outcomes And Views

Every command result contains:

- command outcome
- newly projected public events
- updated passive view

The passive view reports transport state, active source, active range, cue,
prepared anchor, `play_ready`, loaded source count, prepared buffer summary, and
evidence state.

## Evidence Policy

`TickPrepare` can create prepared buffer state. That means `Prepared` only.

Phase 28 does not emit or claim:

- `FramePresented`
- `AudioDeviceVerified`
- `RealtimeVerified`
- real device output
- A/V sync

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-session-command-boundary <original-mxf> <proxy-mp4>
```

The command builds the Phase 22 descriptor and `QgsInputPlan`, then applies:

1. `LoadPreparedInput`
2. `PreloadSource`
3. `SetActiveSource`
4. `SetActiveRange [0..1000 ms)`
5. `Cue frame 0`
6. `PrepareAnchor`
7. `Play`
8. `TickPrepare`
9. `Pause`
10. `CloseActiveSource`
11. one wrong-generation command to prove rejection

## Acceptance Results

For Sony FX6 sample 002 and Mironik 2002, the command boundary reports:

- command count: 11
- accepted count: 10
- rejected count: 1
- wrong-generation command rejected: yes
- final generation: 1
- final passive view: no active source, no active range, `play_ready=false`
- private path exposed: no
- realtime: no
- A/V sync: no
- device output: no
- `FramePresented`: no
- `AudioDeviceVerified`: no

## Non-Claims

Phase 28 does not implement:

- network IPC
- process launching
- QNC UI or client protocol
- QNC DB writes
- realtime loop
- video presenter
- audio output policy
- A/V sync
- export/render

## Phase 29 Direction

The next useful step is a stricter public session transcript/queue model:
bounded command queue limits, duplicate command-id handling, and a stable
adapter boundary that a future QNC process protocol can wrap without taking
ownership of QGS backend timing or readiness.

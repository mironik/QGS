# M2 Integration Block A - QGS Session Runtime Control Surface

Integration Block A consolidates the Phase 22-28 runtime layers into a single
in-process QGS session control surface. It is the backend shape that a future
QNC protocol adapter can wrap, but it is not IPC, a process launcher, QNC UI,
database work, realtime playback, A/V sync, device output, or export/render.

## Relationship To Phases 22-28

This block follows the existing replacement map rather than creating a new
player side path.

It reuses:

- Phase 22 prepared input descriptors and `QgsInputPlan`
- Phase 23 source handles, load/preload/set-active behavior, and transport
  readiness
- Phase 24 active range, cue, prepared anchor, and rational frame timing
- Phase 25 bounded prepared buffer and `TickPrepare` accounting
- Phase 26 close/unload lifecycle and internal runtime event envelope
- Phase 27 QNC-compatible command, event, and passive-view projection
- Phase 28 generation-checked command executor

The new work is the control surface around that executor: a bounded FIFO command
queue, strict command-id duplicate rejection, deterministic command transcript,
and final session snapshot.

## Responsibilities Mirrored

The control surface mirrors the QNC TransportEngine responsibilities that are
already represented in the Step 22A audit and Phases 22-28:

- source handle identity
- `load_source` / `preload_source`
- `set_active_source`
- half-open active range
- cue and seek preparation
- prepared anchor
- `play_ready`
- rejection of Play before readiness
- no-work-on-Play accounting
- deterministic transport event transcript
- source close/unload lifecycle
- public passive view projection

It intentionally does not implement QNC-specific IPC, client protocol, UI
state, database persistence, or process launching. QGS owns the backend runtime
facts; QNC applications may later issue commands and observe projected events
through an adapter.

## Session Runtime Model

`QgsSessionRuntime` owns:

- one Phase 28 `QgsQncSessionCommandExecutor`
- one bounded command queue
- a strict duplicate command-id registry
- a public command transcript
- final passive-view snapshots

Configuration is carried by `QgsSessionRuntimeConfig`. The default command queue
limit is 16 queued commands.

The duplicate command policy is strict: a command id is rejected if it is
already queued, was already executed, or was already seen as a duplicate. There
is no replace, coalesce, or retry behavior in this block.

## Command Queue Rules

Commands are `QgsQncCommandEnvelope` values from Phase 27/28. Integration Block
A does not define a second command model.

Queue behavior:

- enqueue succeeds when capacity exists and the command id is new
- enqueue fails when the queue is full
- enqueue fails for duplicate command ids
- enqueue fails if the command envelope exposes a private local path
- rejected enqueues do not mutate transport/runtime state
- execution is FIFO
- expected generation is checked at execution time
- wrong-generation commands are rejected without mutating runtime state

Every executed command produces:

- command outcome
- projected event list
- updated passive view
- transcript entry

## Transcript And Snapshot

`QgsSessionCommandTranscript` records public command execution facts:

- command id
- command variant
- accepted/rejected status
- generation before and after execution
- projected event count
- resulting passive transport state
- rejection reason, when present

`QgsSessionRuntimeSnapshot` records:

- current runtime generation
- queued command count
- executed command count
- rejected enqueue count
- current passive view
- private path exposure status

Private filesystem paths must not appear in command, event, transcript, or
passive-view output.

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-session-runtime-control <original-mxf> <proxy-mp4>
```

The acceptance command builds a Phase 22 descriptor and `QgsInputPlan`, creates a
session runtime, enqueues the standard Phase 28 command sequence, executes it in
FIFO order, then probes:

- duplicate command-id rejection
- queue-full rejection with a small temporary queue
- stale-generation rejection at execution time

The main command sequence is:

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

## Acceptance Result Shape

For the Sony FX6 acceptance clips, the command should report:

- public original URI and proxy URI
- private path exposed: no
- queue limit: 16
- duplicate rejected: yes
- queue-full rejected: yes
- stale generation rejected: yes
- final active source: none after close
- final `play_ready`: false
- projected events are monotonic

The final passive view remains a public QNC-shaped observation surface. It is not
IPC and not QNC UI state.

Observed local acceptance:

| Clip | Enqueued | Executed | Accepted | Rejected | Events | Final generation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Mironik 1560 / sample-002 | 10 | 11 | 10 | 3 | 38 | 1 |
| Mironik 2002 | 10 | 11 | 10 | 3 | 38 | 1 |

The executed count includes the stale-generation probe. The rejected count
includes duplicate command-id rejection, queue-full rejection, and the
stale-generation command rejection. Both clips reported:

- duplicate rejected: yes
- queue-full rejected: yes
- stale generation rejected: yes
- event sequence monotonic: yes
- private path exposed: no
- final active source: none
- final `play_ready`: false

## Non-Claims

Integration Block A does not implement:

- network IPC
- process launcher behavior
- QNC UI or client protocol
- QNC database writes
- realtime playback
- A/V sync
- video presenter work
- production audio output
- `FramePresented`
- `AudioDeviceVerified`
- `RealtimeVerified`
- export/render

`TickPrepare` still means bounded prepared runtime state only. It does not mean
presented, audible, verified, realtime, or device-ready.

## Next Integration Block

The next useful integration block is a presenter/monitor device boundary around
the existing prepared payload and PipeWire diagnostics. That should keep QGS
media truth intact, preserve original MXF mono audio as authoritative, and avoid
claiming production output until device evidence actually supports it.

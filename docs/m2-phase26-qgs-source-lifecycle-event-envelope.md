# M2 Phase 26 - QGS Source Lifecycle / Runtime Event Envelope

Phase 26 adds source unload/close lifecycle and a stable internal runtime event
envelope for QGS transport/runtime facts.

This is a lifecycle and event-envelope phase. It is not playback, realtime
scheduling, A/V sync, device output, QNC IPC, UI, or export work.

## Relationship To Step 22A

Step 22A identified remaining QGS replacement gaps around source close/unload,
runtime command/event surfaces, stale handle handling, and QNC-compatible event
projection. Phase 26 addresses the backend lifecycle and internal event
envelope portion of that gap.

QGS owns backend runtime readiness, source lifecycle, and media timing facts.
QNC applications may later observe events and issue commands through a protocol,
but this phase does not implement that protocol.

## Relationship To Phase 22

The lifecycle model preserves the Phase 22 source identity rule:

- public runtime identity uses `qnc://` URI-like source IDs
- private local paths remain qgs-test transport bindings
- event envelopes do not expose private paths
- proxy picture remains separate from authoritative original MXF mono audio
- proxy AAC remains diagnostic-only

## Relationship To Phase 23

Phase 23 added `QgsTransportEngine`, source handles, source revisions, active
source, active range, cue, prepared anchor, and `play_ready`.

Phase 26 adds:

- `close_active_source`
- `clear_active_source`
- `unload_source`
- `invalidate_source_revision`
- stale handle rejection after unload
- lifecycle transport events

Unloading an active source clears active range, cue, prepared anchor, and
`play_ready`.

## Relationship To Phase 24

Phase 24 timing facts remain source-bound. Once the active source is unloaded or
closed, cue/active range/prepared anchor timing facts are cleared and commands
requiring them are rejected.

## Relationship To Phase 25

Phase 25 prepared/playout buffer state is explicitly discarded on source unload
or close. Discarding prepared state does not mean anything was submitted,
presented, audible, realtime, or verified.

## Source Lifecycle Semantics

`unload_source` with a valid inactive handle:

- removes the source
- invalidates the revision
- emits `SourceRevisionInvalidated`
- emits `SourceUnloaded`
- rejects future commands using the stale handle

`unload_source` with a valid active handle:

- clears active source
- clears active range
- clears cue
- clears prepared anchor
- sets `play_ready` false
- emits `ActiveSourceCleared`
- emits `PlayReadinessChanged(false)`
- invalidates the source revision
- emits `SourceUnloaded`

`close_active_source`:

- clears active source state
- sets `play_ready` false
- preserves the loaded source in the current skeleton
- allows the same valid handle to be set active again

Stale or missing source handles are rejected deterministically.

## Runtime Event Envelope

Phase 26 adds an internal envelope:

- sequence number
- generation number
- optional source handle summary
- event kind
- payload summary
- optional evidence level

The envelope wraps current transport and tick-preparation events into a stable
internal event transcript. It is not final QNC IPC, not a process protocol, and
not a UI/client contract.

The event sequence is deterministic and monotonic. The runtime generation starts
at 0. The lifecycle command path increments generation to 1 before source
discard/unload events.

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-runtime-lifecycle-events <original-mxf> <proxy-mp4>
```

The command:

1. builds the Phase 22 descriptor
2. builds `QgsInputPlan`
3. loads, preloads, and activates the source
4. sets active range `[0, 1000 ms)`
5. cues frame 0
6. prepares anchor
7. confirms `play_ready`
8. runs one Phase 25 tick
9. discards prepared buffer state
10. unloads the active source
11. attempts a stale command with the old handle
12. prints the event-envelope transcript and final snapshot

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- initial `play_ready`: true
- buffer frames before unload: 6
- unload result: unloaded, active, revision invalidated
- final active source: none
- final `play_ready`: false
- prepared buffer after unload: 0
- stale command rejected: yes
- event sequence monotonic: yes
- private path exposed in events: no
- realtime playback: no
- device output: no
- frame presented: no
- A/V sync: no

For Mironik 2002:

- same lifecycle behavior over the first 1000 ms active range
- public source URI remains `qnc://local/media/proxy/Mironik-2002`
- private path exposed in events: no
- stale command rejected: yes
- no realtime/device/presentation claim

## Non-Claims

Phase 26 does not implement:

- QNC client protocol
- network/process IPC
- realtime scheduler
- continuous realtime loop
- real device submission
- video presenter
- audio output policy
- A/V sync
- UI/export
- `FramePresented`
- `AudioDeviceVerified`
- `RealtimeVerified`

## Phase 27 Direction

Phase 27 should build on this event envelope toward a QNC-compatible command and
event projection while keeping QNC DB/UI/workflow ownership outside QGS.

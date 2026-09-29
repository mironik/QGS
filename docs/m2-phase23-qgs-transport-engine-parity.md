# M2 Phase 23 - QGS Transport Engine Parity Skeleton

Phase 23 adds the first QGS transport engine parity skeleton derived from the
Step 22A QNC Broadcast Engine / Broadcast Player replacement audit.

This is not a new player engine side path. It consumes the Phase 22
`QgsInputPlan`, preserves the QNC-style prepared input semantics, and implements
only the transport responsibilities needed before future frame clock, prepared
buffer, device, and client work.

## Implementation Note

Step 22A identified QNC `TransportEngine` as the owner of source
load/preload/set-active, active range, cue/seek, play readiness, prepared
anchor, bounded prepared/playout state, tick/preparation, event generation, and
the rule that `play()` must not open sources, decode, fill queues, or perform
deferred preroll.

Phase 23 mirrors this exact subset:

- source handle model with source revision
- `load_source`
- `preload_source`
- `set_active_source`
- half-open active range `[start_frame..end_frame)`
- cue/seek preparation through `cue_frame`
- prepared anchor through `prepare_anchor`
- `play_ready` evaluation
- rejection of Play before Ready
- no-work-on-Play counters
- deterministic transport event transcript

Phase 23 intentionally does not implement:

- realtime playback
- realtime scheduler
- continuous tick loop
- playout/prepared buffer implementation
- frame clock parity
- video presenter or display output
- audio device output
- A/V sync
- export/render
- QNC UI/client protocol
- new cutter-only model
- any path that bypasses the Phase 22 `QgsInputPlan`

This phase follows directly from Step 22A and Phase 22. Step 22A described the
missing QNC `TransportEngine` responsibilities. Phase 22 created the
QNC-compatible input descriptor and `QgsInputPlan` that preserve public
URI-style identity, source mode, proxy/original timing, and authoritative
original mono audio. Phase 23 uses that plan as its only input model and adds
transport state transitions over it.

## Runtime Types

Phase 23 adds backend-neutral transport types in `qgs-media-runtime`:

- `QgsTransportSourceHandle`
- `QgsTransportSourceRevision`
- `QgsTransportStatus`
- `QgsTransportActiveRange`
- `QgsTransportCuePoint`
- `QgsTransportPreparedAnchor`
- `QgsTransportNoWorkOnPlayCounters`
- `QgsTransportEvent`
- `QgsTransportSnapshot`
- `QgsTransportEngine`

The transport source handle exposes public `qnc://` URI-style identity from the
Phase 22 input plan. Raw filesystem paths remain private qgs-test bindings and
are not used as transport source identity.

## Transport Semantics

`load_source` validates a `QgsInputPlan`, creates a public source handle, and
assigns a revision. `preload_source` marks the source prepared enough for this
skeleton. `set_active_source` binds the active source and clears range, cue,
anchor, and readiness state.

The active range is a half-open frame interval. It is mapped to original-audio
sample boundaries using the source video timebase and authoritative original
audio sample rate.

`cue_frame` validates the cue inside the active range and records its
corresponding original-audio sample boundary. `prepare_anchor` requires a
preloaded active source and a cue. Play readiness becomes true only when the
active source, preload state, active range, cue, and prepared anchor all match
the same source revision.

`play()` rejects before Ready. When Ready is true, `play()` changes transport
status and emits events, but it does not open sources, decode frames, fill
queues, perform preroll, prepare the anchor, or touch device boundaries.

## Deterministic Events

The skeleton emits a stable event transcript:

- `TransportEngineCreated`
- `SourceLoaded`
- `SourcePreloaded`
- `ActiveSourceChanged`
- `ActiveRangeSet`
- `CueCompleted`
- `TransportPlayRejected`
- `PreparedAnchorReady`
- `PlayReadinessChanged`
- `TransportStarted`

Validation failures are reported with `TransportValidationFailed` where the
command path can continue observing state. This is still an internal QGS event
shape, not the final QNC client protocol.

## QGS-Test Command

The acceptance command is:

```bash
cargo run -q -p qgs-test -- \
  --qgs-transport-engine-parity <original-mxf> <proxy-mp4>
```

The command builds the Phase 22 descriptor and `QgsInputPlan`, then executes:

1. create transport engine
2. load source
3. preload source
4. set active source
5. set a 1000 ms active range
6. cue frame 0
7. reject Play before prepared anchor
8. prepare anchor
9. verify `play_ready`
10. Play
11. print no-work-on-Play counters and event transcript

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- public original URI: `qnc://local/media/original/Mironik-1560`
- public proxy URI: `qnc://local/media/proxy/Mironik-1560`
- source identity exposes private path: no
- active range: `[0..50)` frames
- sample range: `[0..48000)` original-audio samples
- cue point: frame 0, sample 0
- Play before Ready rejected: yes
- `play_ready` before anchor: no
- `play_ready` after anchor: yes
- Play result: ok
- final status: `Playing`
- no-work-on-Play counters: all zero
- event count: 11

For Mironik 2002:

- public original URI: `qnc://local/media/original/Mironik-2002`
- public proxy URI: `qnc://local/media/proxy/Mironik-2002`
- source identity exposes private path: no
- active range: `[0..50)` frames
- sample range: `[0..48000)` original-audio samples
- cue point: frame 0, sample 0
- Play before Ready rejected: yes
- `play_ready` before anchor: no
- `play_ready` after anchor: yes
- Play result: ok
- final status: `Playing`
- no-work-on-Play counters: all zero
- event count: 11

Both runs show:

- public `qnc://` source identity
- source identity does not expose private paths
- active range covers 1000 ms
- active range maps to 48000 original-audio samples
- Play before Ready is rejected
- `play_ready` is false before the anchor
- `play_ready` is true after the anchor
- Play succeeds after Ready
- final status is `Playing`
- no-work-on-Play counters are all zero
- realtime playback is not claimed
- device verification is not claimed
- export/render is not involved

## Current Non-Claims

Phase 23 does not claim:

- real playback
- realtime scheduling
- frame clock parity
- continuous tick loop
- playout buffer behavior
- display output
- audio device output
- A/V sync
- QNC UI/client protocol compatibility
- export/render

## Next Work

The next transport/runtime work should add the remaining Step 22A gaps without
bypassing the Phase 22 input model:

- rational frame clock parity
- cue/seek edge behavior
- bounded prepared/playout buffer ownership
- tick/preparation loop
- source unload/close semantics
- QNC-compatible client event envelope

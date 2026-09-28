# M2 Step 20H — QGS Broadcast Player Runtime Event Surface

Step 20H adds a backend-neutral event surface for the QGS Broadcast Player Runtime.

This is not a UI player, not speaker output, not display output, not export/render, and not final broadcast playout. QGS exposes runtime facts and events that future QNC applications can consume through their own session, transport, and UI layers.

QGS does not import QNC crates.

## Media Roles

The event surface preserves the Step 20F/20G source-mode model:

- `ProxyPreview`: proxy MP4 video plus authoritative original MXF audio
- `OriginalMedia`: original MXF video plus authoritative original MXF audio

Proxy MP4 AAC is not authoritative audio in either mode.

## Relationship To Steps 20D-20G

Step 20D defined the QGS Broadcast Player Runtime contract and clarified that QGS owns backend readiness, source-mode validation, original/proxy mapping, media timing facts, and bounded prepared state.

Step 20E added the Broadcast Player Runtime state machine.

Step 20F added bounded preroll planning.

Step 20G added prepared payload slot records.

Step 20H exposes those facts as deterministic runtime events so future QNC applications can observe the backend without reading mutable internals or importing QNC crates into QGS.

## Event Model

`BroadcastPlayerRuntimeEvent` exposes product-facing runtime events:

- `SessionCreated`
- `PrepareStarted`
- `PrerollPlanned`
- `PrerollReady`
- `PreparedSlotAvailable`
- `RuntimeReady`
- `TransportStarted`
- `TransportPaused`
- `SeekCompleted`
- `IntentionalProfileSkip`
- `SelectedFrameAccounted`
- `AudioRangeAccounted`
- `CapabilityMissing`
- `RuntimeCompleted`
- `RuntimeFailed`

The surface intentionally does not emit `FramePresented`. Step 20H has no real display presenter. Selected video frames and original-audio ranges are accounted as runtime facts, not as device presentation.

## Event Payloads

Events carry stable backend facts:

- session/source mode, preview profile, audio source role, and video source role
- preroll queue limits and planned selected frame/audio range counts
- prepared slot kind, slot index, source mode, readiness, and video slot status
- transport state and media time placeholders
- selected frame index, source frame index, presentation time, and duration
- original-audio start sample, sample count, track coverage count, and completeness
- capability and reason for capability-missing paths
- completion accounting for selected frames, audio ranges, intentional skips, and lateness drops

Events do not contain raw local filesystem paths.

## Prepared Slot Events

Prepared slots from Step 20G become `PreparedSlotAvailable` events for:

- video slots
- audio slots
- presentation slots

For Sony FX6 sample 002 in `ProxyPreview`, the bounded prepared working set has:

- video slots: 3
- audio slots: 3
- presentation slots: 3
- tracks covered: 4
- selected proxy source frames referenced: 0, 2, 4
- Broadcast Player ready: yes
- selected frames accounted: 53
- original-audio ranges accounted: 53
- intentional profile skips: 53
- lateness drops: 0
- runtime completed: yes

## OriginalMedia Capability Result

`OriginalMedia` remains contract-valid:

- video source: original MXF
- audio source: original MXF
- original video source present: yes

Because the original-video Broadcast Player runtime backend is not integrated in this milestone, the event surface emits `CapabilityMissing` for `OriginalMedia` instead of pretending the session is ready.

The `OriginalMedia` event sequence does not emit `RuntimeReady`.

## Why Events

QNC applications may later issue transport/session commands, observe runtime events, present UI state, and store/read media snapshots. They should not own QGS backend media readiness or inspect internal mutable state.

The event surface gives QNC applications stable observation points while QGS remains responsible for original/proxy mapping, source-mode validation, backend timing facts, and prepared runtime state.

## Limitations

Not implemented:

- real audio device output
- real display output
- realtime Broadcast Player loop
- QNC UI integration
- export/render
- sync correction or resampling
- original MXF realtime video preparation

## Next Steps

Future milestones can connect this event surface to a QNC-facing session/transport adapter, real decoded payload ownership, device output backends, and clock policy while preserving the current separation between backend facts and application UI.

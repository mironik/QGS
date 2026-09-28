# M2 Step 20H — QGS Broadcast Player Event Surface

Step 20H adds a backend-neutral event surface for the QGS Broadcast Player runtime.

This is not a UI player, not speaker output, not display output, not export/render, and not final broadcast playout. QGS exposes runtime facts and events that future QNC applications can consume through their own session, transport, and UI layers.

QGS does not import QNC crates.

## Media Roles

The event surface preserves the Step 20F/20G source-mode model:

- `ProxyPreview`: proxy MP4 video plus authoritative original MXF audio
- `OriginalMedia`: original MXF video plus authoritative original MXF audio

Proxy MP4 AAC is not authoritative audio in either mode.

## Event Surface

`BroadcastPlayerEvent` exposes product-facing runtime events:

- `SessionCreated`
- `PrepareStarted`
- `PrerollReady`
- `PreparedSlotAvailable`
- `BroadcastPlayerReady`
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

## OriginalMedia Capability Result

`OriginalMedia` remains contract-valid:

- video source: original MXF
- audio source: original MXF
- original video source present: yes

Because the original-video Broadcast Player runtime backend is not integrated in this milestone, the event surface emits `CapabilityMissing` for `OriginalMedia` instead of pretending the session is ready.

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

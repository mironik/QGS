# M2 Step 20G — QGS Broadcast Player Runtime Prepared Payload Slots

Step 20G turns the Step 20F preroll plan into a finite prepared working-set model.

This is still not real playback. QGS does not output audio, does not present to a display, does not run a realtime Broadcast Player loop, and does not import QNC crates. The slot model describes what a future realtime loop can consume once backend payload preparation is connected.

## Relationship To Step 20F

Step 20F answered whether a session can become `Ready` after bounded preroll requirements are satisfied.

Step 20G adds backend-neutral prepared slot records:

- prepared video slots
- prepared audio slots
- prepared presentation slots

The runtime remains source-mode aware:

- `ProxyPreview`: proxy MP4 video plus original MXF audio
- `OriginalMedia`: original MXF video plus original MXF audio

Proxy MP4 AAC is not authoritative audio in either mode.

## Prepared Slot Model

`BroadcastPreparedAudioSlot` records:

- finite slot index
- source mode
- authoritative original-audio role
- start time and duration
- start sample and sample count
- per-track/channel coverage summary
- completeness

`BroadcastPreparedVideoSlot` records:

- finite slot index
- source mode
- video source role
- source frame index
- selected preview frame index when applicable
- presentation time and duration
- status: `Prepared`, `IntentionalSkip`, `CapabilityMissing`, `NotSupported`, or `Missing`

`BroadcastPreparedPresentationSlot` links:

- presentation index
- selected source frame
- video slot index
- audio slot index
- presentation time and duration
- readiness

For this milestone these are lightweight runtime records. They do not yet own decoded video frames, GPU outputs, or PCM output buffers.

## ProxyPreview Result

For Sony FX6 sample 002 in `journalist-50i-preview`:

- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used
- selected preview frames planned: 53
- intentional source-frame skips planned: 53
- prepared video slots: 3
- prepared audio slots: 3
- prepared presentation slots: 3
- presentation slots ready: 3
- original audio tracks covered: 4
- `Ready`: yes
- `Play` from `Ready`: yes

Intentional source-frame skips are profile behavior. They are not presentation slots, missing frames, or lateness drops.

## OriginalMedia Result

The QGS Broadcast Player Runtime contract also represents `OriginalMedia`:

- video source: original MXF
- audio source: original MXF
- original video source present: yes for the Sony FX6 sample
- original-audio slots can be prepared
- original-video slots report `CapabilityMissing`
- presentation slots are not ready
- `Ready`: no
- reason: `CapabilityMissing`

This avoids fake original-video readiness while preserving the correct future mode for powerful systems and finishing workflows.

## Slot Bounds

The Step 20G proof uses the same small bounded capacities as Step 20F:

- video slot capacity: 6
- audio slot capacity: 8
- presentation slot capacity: 3
- prepared working set: 3 presentation periods

These values are proof configuration, not final QNC policy constants.

## Limitations

Not implemented:

- real decoded video payload slots
- real PCM output queues
- speaker output
- display output
- realtime scheduling
- QNC UI integration
- export/render
- audio resampling or drift correction
- realtime original MXF video playback

## Next Steps

Next runtime milestones should connect these prepared slots to real decoded video/GPU payload ownership, original-audio PCM payload scheduling, and a QGS session/transport event surface that QNC applications can consume without importing UI policy into QGS.

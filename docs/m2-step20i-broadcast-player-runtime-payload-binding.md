# M2 Step 20I — QGS Broadcast Player Runtime Payload Binding

Step 20I connects the prepared Broadcast Player Runtime slots from Step 20G to concrete backend payload references where QGS already owns those payloads.

This is still not real playback. There is no speaker output, display output, realtime playback loop, export/render, waveform UI, audio editing, QNC UI integration, or QNC crate dependency. The runtime does not emit `FramePresented`.

## Relationship To Steps 20G And 20H

Step 20G created lightweight prepared slot records for video, audio, and presentation periods.

Step 20H exposed those prepared/runtime facts through deterministic Broadcast Player Runtime events.

Step 20I binds prepared slots to payload references:

- prepared audio slots bind to original MXF PCM runtime blocks from Step 20C
- prepared video slots bind to explicit status records
- prepared presentation slots bind one video status record and one original-audio payload binding

## Media Roles

`ProxyPreview` remains the accepted preview path:

- video source: proxy MP4
- audio source: original MXF
- proxy MP4 AAC: not used

`OriginalMedia` remains a valid contract mode:

- video source: original MXF
- audio source: original MXF
- original-video payload readiness: `CapabilityMissing` in this milestone

Proxy MP4 AAC is not authoritative audio in either mode.

## Audio Payload Binding

`BroadcastAudioPayloadBinding` references existing `PcmAudioBlock` payloads without copying all audio bytes into presentation slots.

Each binding records:

- audio slot index and source mode
- start sample and sample count
- sample rate
- track count
- per-track/channel block coverage
- byte offset and byte count for full or partial block coverage
- total referenced payload bytes
- completeness

The binding preserves original PCM identity:

- 24-bit payload remains 24-bit
- no f32 conversion
- no s16 downconversion
- no generated silence
- track/channel identity is preserved

For Sony FX6 sample 002, each `journalist-50i-preview` presentation period is 40 ms. With 48 kHz original audio, that maps to 1920 samples per track. The original MXF audio runtime blocks are 20 ms mono blocks, so each presentation range references two blocks per track across four tracks:

- blocks per presentation audio range: 8
- bytes per mono block: 2880
- bytes per presentation audio range: 23040
- bytes across the three-slot preroll working set: 69120

## Video Payload Binding

`BroadcastVideoPayloadBinding` reports the video payload readiness status:

- `AccountedOnly`
- `PayloadReady`
- `CapabilityMissing`
- `NotSupported`
- `Missing`

For `ProxyPreview`, Step 20I deliberately reports prepared proxy-video slots as `AccountedOnly`. The Broadcast Player Runtime can account for the selected proxy frames and their timing, but real decoded/GPU video payload ownership is not yet connected to these runtime slots.

For `OriginalMedia`, original-video payload binding reports `CapabilityMissing`. QGS does not fake original MXF video payload readiness.

## Presentation Payload Binding

`BroadcastPresentationPayloadBinding` links:

- one video payload binding
- one audio payload binding
- presentation time
- duration
- readiness classification

Readiness is deliberately split:

- `RuntimeAccountingReady`: original-audio payload is bound and proxy-video is accounted, but no real device video payload is owned by the Broadcast Player Runtime yet
- `DevicePayloadReady`: both audio and video payloads are truly payload-ready
- `CapabilityMissing`: a required capability is missing
- `NotReady`: the binding is incomplete

For the current `ProxyPreview` proof, prepared presentation bindings are `RuntimeAccountingReady`, not `DevicePayloadReady`.

## Bounded Working Set

Step 20I uses the same finite proof capacities as Steps 20F/20G:

- video binding capacity: 6
- audio binding capacity: 8
- presentation binding capacity: 3
- prepared working set: 3 presentation periods

The proof does not construct an unbounded all-frame payload map.

## Sony FX6 Sample 002 Result

For `ProxyPreview` with `journalist-50i-preview`:

- prepared presentation payload bindings: 3
- audio source: original MXF
- proxy AAC: not used
- audio payload bindings: 3
- video payload bindings: 3
- presentation payload bindings: 3
- tracks covered: 4 original audio tracks
- blocks referenced per presentation range: 8
- audio bytes referenced per presentation range: 23040
- total referenced audio bytes: 69120
- proxy video binding status: `AccountedOnly`
- presentation readiness: `RuntimeAccountingReady`
- `FramePresented`: 0

For `OriginalMedia`:

- original audio binding is possible
- original video binding status: `CapabilityMissing`
- presentation readiness: `CapabilityMissing`
- no fake original-video payload is created

## Limitations

Not implemented:

- real decoded proxy-video payload ownership in Broadcast Player Runtime slots
- real GPU frame ownership in Broadcast Player Runtime slots
- real original MXF video runtime payload preparation
- real PCM device output queue
- speaker output
- display output
- realtime scheduler
- export/render
- UI integration
- audio resampling or drift correction

## Next Steps

Future milestones can bind real decoded proxy-video/GPU payload ownership to the Broadcast Player Runtime, add device-output adapters, and introduce clock policy while keeping original MXF audio authoritative and proxy AAC diagnostic-only.

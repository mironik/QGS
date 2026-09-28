# M2 Step 20C - PCM Audio Runtime Blocks

Step 20C converts original MXF PCM packets from Step 20B into backend-neutral runtime audio blocks. It keeps the QNC media model established in Step 20A:

- proxy MP4 video is the preview/edit-performance source
- original MXF audio is authoritative
- proxy MP4 AAC is diagnostic/fallback only and is not used by this path

This is still not real audio playback. No speaker output, audio-device clock, waveform UI, audio editing, mixdown/export, resampling, drift correction, or proxy-AAC primary path is implemented.

## Relationship To Previous Steps

Step 20A modeled original MXF audio tracks and timing without payload extraction.

Step 20B extracted real original MXF LPCM payload packets:

- four mono tracks
- 48 kHz
- 24-bit signed PCM payload bytes
- 106 packets per track
- 424 packets total
- 1,221,120 payload bytes total
- monotonic timestamps per track

Step 20C keeps those extraction semantics and adds a runtime-friendly block layer after packet extraction.

## Runtime Block Model

`qgs-media-runtime` now includes:

- `PcmAudioBlockLayout`
- `PcmAudioBlock`

For the Sony FX6 sample 002 original MXF, each extracted packet becomes one mono-track runtime block:

```text
PcmAudioPacket
  -> PcmAudioBlock
```

Each block carries:

- start time
- duration
- sample rate
- sample count
- PCM sample format
- layout
- owned payload bytes

The block layout used here is:

```text
MonoTrack { track_id, channel_index }
```

This is appropriate for the observed Sony FX6 MXF because the original audio is stored as one mono PCM essence stream per track/channel. Step 20C does not interleave or mix all four channels into a single buffer.

## Packet To Block Conversion

For this sample, packet-to-block conversion preserves:

- 960 samples per block
- 20 ms block duration
- 48 kHz sample rate
- 24-bit signed little-endian payload bytes
- 2880 bytes per mono block
- track id and channel index

The conversion validates payload size as:

```text
sample_count * channel_count * bytes_per_sample
```

For one mono 24-bit block:

```text
960 * 1 * 3 = 2880 bytes
```

Payload bytes are moved from the extracted packet into the runtime block. No silence is generated, no payload is synthesized, and no f32 or 16-bit conversion is performed.

## Bounded Runtime Queue

The focused qgs-test extraction command now includes runtime block accounting:

```sh
cargo run -q -p qgs-test -- --original-audio-extract <original-mxf>
```

Observed runtime block queue result:

```text
Runtime block queue: capacity=8 peak=8 backpressure=416
```

The queue capacity is intentionally small. The 416 backpressure events demonstrate that the command processes all blocks through a finite queue instead of accumulating the full audio payload stream unbounded.

## Acceptance Result

Observed privacy-safe result for Sony FX6 sample 002 original MXF:

```text
Audio source: original MXF
Proxy AAC: not used
Runtime block model: mono-track PCM blocks
Runtime blocks: blocks=424 bytes=1221120
track 3: blocks=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes gaps=0 overlaps=0
track 4: blocks=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes gaps=0 overlaps=0
track 5: blocks=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes gaps=0 overlaps=0
track 6: blocks=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes gaps=0 overlaps=0
Suitable for future audio clock: yes
```

The runtime block totals match Step 20B packet totals exactly:

```text
424 blocks == 424 extracted PCM packets
1,221,120 block payload bytes == 1,221,120 extracted payload bytes
```

## Clock-Readiness Checks

The qgs-test block sink/accounting verifies, per track:

- first block start time
- last block end time
- represented duration
- sample count
- monotonic block timing
- gap count
- overlap count

For the Sony FX6 sample 002 original MXF, all four tracks are continuous and monotonic with no gaps or overlaps. This makes the block stream suitable as input to a future audio-master clock layer.

Step 20C does not implement the audio master clock itself.

## Tests

Synthetic tests cover the runtime block model without requiring external camera media:

- packet-to-block conversion
- 24-bit payload length validation
- invalid payload rejection
- invalid layout rejection
- monotonic block validation
- gap detection
- overlap detection
- bounded block queue behavior

## Limitations

Not implemented in Step 20C:

- real speaker output
- audio-device clock
- waveform UI
- audio editing
- mixdown/export
- resampling or drift correction
- channel interleaving/mixing
- proxy AAC primary-audio path
- broad MXF audio support beyond the current Sony FX6 OP1a/Wave/LPCM subset

Recommended next steps are a real audio output/device-clock boundary, waveform-analysis input over the block stream, and later A/V sync policy using original MXF audio as the authoritative timeline.

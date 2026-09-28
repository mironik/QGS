# M2 Step 20B - Original MXF PCM Packet Extraction

Step 20B adds bounded extraction of original MXF LPCM audio essence into QGS-owned PCM packet structures. This milestone extends the Step 20A audio metadata/timeline foundation with real payload extraction while preserving the QNC media rule:

- proxy MP4 video is used for responsive preview/edit performance
- original MXF audio is authoritative audio
- proxy MP4 AAC is diagnostic/fallback only and is not used here

This is still not audio playback. There is no speaker output, no waveform UI, no audio editing, no mixdown/export, no audio-device clock, and no resampling or drift correction.

## Implemented Subset

The implementation is intentionally narrow and QGS-native. `qgs-mxf` now indexes the PCM audio essence elements observed in the Sony FX6 sample 002 original MXF and can extract an owned payload packet by index.

Supported in this milestone:

- OP1a-style MXF already handled by `qgs-mxf`
- Wave/LPCM-style audio descriptors already parsed by QGS metadata code
- one mono PCM essence element per audio track/channel
- PCM packet timing derived from descriptor sample rate and packet sample count
- 24-bit payload bytes preserved without downconversion
- bounded packet indexing and bounded owned-payload extraction

The extractor does not synthesize audio. If no indexed audio payload exists, the focused qgs-test command reports that boundary rather than generating silence.

## Sony FX6 Sample 002 Layout

For the local Sony FX6 sample 002 original MXF, QGS observes four original audio tracks:

| Track | Channels | Sample Rate | Bit Depth | Block Align |
| --- | ---: | ---: | ---: | ---: |
| 3 | 1 | 48000 Hz | 24-bit | 3 bytes |
| 4 | 1 | 48000 Hz | 24-bit | 3 bytes |
| 5 | 1 | 48000 Hz | 24-bit | 3 bytes |
| 6 | 1 | 48000 Hz | 24-bit | 3 bytes |

The essence layout for this sample is one PCM KLV packet per edit unit per mono track. Each packet is 2880 bytes:

```text
960 samples/edit-unit * 3 bytes/sample * 1 channel = 2880 bytes
```

At 48 kHz and 50 edit units/s, each audio packet represents 20 ms. With 106 edit units, each track represents 101,760 samples and 2.120 seconds.

## Packet Model

`qgs-mxf` adds audio index entries containing:

- track id
- channel index
- edit unit
- start sample
- sample count
- file offset and payload offset
- payload length
- index source

Extraction returns an owned packet payload. `qgs-media-runtime` adds a backend-neutral `PcmAudioPacket` model with:

- track id
- channel index
- start time
- duration
- sample count
- PCM sample format
- owned payload bytes

For this sample the runtime packet format is signed 24-bit PCM, little-endian, preserving the original bytes. No conversion to 16-bit or f32 is performed.

## Queue Bounds

The focused extraction command streams packets through a bounded queue before draining them into statistics. It does not collect all PCM payload packets into an unbounded runtime buffer.

Observed queue configuration/result:

```text
queue capacity: 8 PCM packets
queue peak: 8 PCM packets
backpressure events: 416
```

The high backpressure count is expected for a deliberately small bounded queue over 424 packets. It proves the path remains bounded while still extracting every packet.

## Payload Statistics

Acceptance command:

```sh
cargo run -q -p qgs-test -- --original-audio-extract <original-mxf>
```

Observed privacy-safe result for Sony FX6 sample 002 original MXF:

```text
Audio source: original MXF
Proxy AAC: not used
Audio tracks: 4
PCM payload: packets=424 bytes=1221120
track 3: packets=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes
track 4: packets=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes
track 5: packets=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes
track 6: packets=106 samples=101760 payload_bytes=305280 duration=2.120s monotonic=yes
```

The total payload size is exactly:

```text
4 tracks * 106 packets/track * 2880 bytes/packet = 1,221,120 bytes
```

## Timing Model

Packet timestamps are derived from `start_sample / sample_rate`. Packet duration is derived from `sample_count / sample_rate`.

For this sample:

- sample rate: 48,000 Hz
- samples per packet: 960
- packet duration: 20 ms
- packets per track: 106
- duration per track: 2.120 s
- timestamps are monotonic per track

This matches the existing original/proxy duration model from Step 20A and keeps original MXF audio suitable as a future master-clock source.

## Tests

Synthetic tests cover the new bounded logic without requiring external Sony media:

- PCM packet byte-size calculation
- 24-bit payload size handling
- invalid PCM payload geometry rejection
- Sony-style PCM essence key channel mapping
- existing bounded audio queue and monotonic timing behavior

## Limitations

Not implemented in Step 20B:

- speaker output
- audio waveform UI
- audio editing
- mixdown/export
- audio-device clock
- drift correction or resampling
- proxy AAC primary-audio path
- broad MXF PCM essence variants beyond the observed OP1a/Wave/LPCM subset

The next audio milestone can use these packets to feed a real audio decode/playback device boundary or waveform analysis. That should remain tied to original MXF audio for QNC editing, with proxy AAC kept as diagnostic/fallback material only.

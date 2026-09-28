# QGS M2 Step 20A Original MXF Audio Model

M2 Step 20A adds the first QGS audio timing foundation for the QNC Journalist
workflow. The important model correction is:

```text
original MXF audio
+
proxy MP4 video
```

The proxy MP4 remains the preview/edit-performance video source. The original
MXF is the authoritative audio source. Proxy MP4 AAC may be useful later as a
diagnostic or fallback reference, but it is not the primary journalist audio in
this model.

## Scope

Implemented in this step:

- backend-neutral audio timing types in `qgs-media-runtime`
- original-audio track modeling from `qgs-mxf` metadata
- bounded audio metadata/timing queue for the QNC Journalist demo path
- test audio sink for timing/accounting only
- original-audio duration comparison against proxy-video duration
- selected `journalist-50i-preview` video frame timestamps checked against the
  original-audio timeline

Not implemented:

- real speaker output
- PCM payload extraction from MXF essence
- waveform UI
- audio editing
- mixdown/export
- audio drift correction or resampling
- proxy AAC as authoritative audio

## Runtime Audio Types

`qgs-media-runtime` now defines:

- `AudioSampleFormat`
- `AudioFormat`
- `OriginalAudioTrack`
- `AudioTimeline`
- `AudioTimingPacket`
- `TestAudioSink`

`TestAudioSink` stores packets in a `Vec` because it is only a local test sink.
The bounded behavior is represented by `BoundedQueue<AudioTimingPacket>` before
packets are delivered to the sink.

`AudioTimeline::sample_rate()` and `AudioTimeline::bit_depth()` remain
convenience helpers that return the first track's value. The QNC demo path also
checks whether all original audio tracks agree on sample rate and bit depth and
reports a mixed value when they differ.

## Sony FX6 Sample 002 Result

Observed through QGS on Sony FX6 sample 002:

- audio source: original MXF
- preview video source: proxy MP4
- proxy audio: diagnostic/fallback only
- original audio tracks: 4
- total original audio channels: 4
- topology: four mono LPCM/PCM-style MXF audio descriptors
- sample rate: 48000 Hz
- bit depth: 24-bit
- original audio duration: 2.120 s
- proxy video duration: 2.120 s
- duration delta: 0.000 ms

The model currently derives the original-audio timeline from MXF audio metadata
and the clip edit duration. It does not yet extract PCM audio payload bytes from
the MXF essence.

## Bounded Queue

The Step 20A demo uses bounded metadata/timing packets:

- queue capacity: 4
- queue peak: 4
- backpressure events: 0
- packets recorded by the test sink: 4
- packet payloads: not present
- timestamps monotonic: yes

This proves the timeline/queue accounting without pretending decoded PCM data
exists.

## A/V Clock Foundation

For the QNC Journalist preview model:

- video timeline comes from proxy MP4 presentation frames
- authoritative audio timeline comes from original MXF metadata
- selected preview video frames use the `journalist-50i-preview` cadence

For Sony FX6 sample 002:

- source proxy frames: 106
- selected preview frames: 53
- intentional source-frame skips: 53
- selected video timestamps outside original-audio range: 0.000 ms
- original audio is usable as a future master-clock candidate for this clip

No audio-clock correction, resampling, or drift-management policy is implemented
yet.

## QNC Journalist Demo Output

The existing command:

```text
cargo run -q -p qgs-test -- --qnc-journalist-demo <original-mxf> <proxy-mp4>
```

now reports:

- `Audio source: original MXF`
- `Video source: proxy MP4`
- `Preview profile: journalist-50i-preview`
- original audio track/channel count
- sample rate and bit depth
- bounded audio timing queue peak
- original-audio/proxy-video duration delta
- whether selected preview timestamps fit inside the original-audio range

The export-plan stub also identifies original MXF as the audio source.

## Limitations

- Original MXF PCM payload extraction is not implemented yet.
- Audio packets are metadata/timing packets, not decoded PCM frames.
- No real audio output device is used.
- No audio mixing, editing, waveform, or export path exists.
- Proxy AAC remains diagnostic/fallback only.
- Original audio is identified as a future master-clock candidate, but QGS does
  not yet run a real audio-master playback clock.

## Next Steps

Recommended next milestones:

- implement bounded PCM essence extraction for MXF audio tracks
- add decoded PCM frame ownership and queueing
- introduce real audio-device output behind a testable sink boundary
- define audio-master clock behavior and video sync policy
- add waveform/index data for UI use
- preserve original-audio finishing while proxy video drives responsive editing


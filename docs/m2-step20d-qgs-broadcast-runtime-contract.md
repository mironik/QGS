# M2 Step 20D - QGS Broadcast Runtime Contract

Step 20D defines the first backend-neutral QGS Broadcast Runtime contract for future QNC OS applications. This is a QGS-owned contract, not a port, preservation effort, or adaptation of an existing QNC Broadcast Player.

Existing QNC Broadcast Player concepts may be treated as reference material only. QGS is free to define the best runtime contract for QGS/QNC OS without matching older player boundaries when those boundaries would compromise the media model.

QGS provides backend facts, bounded runtime contract types, and deterministic media-range mapping. Future QNC applications may consume this through media snapshots, resolver/media identity, session/transport contracts, and runtime events. QNC applications remain responsible for final transport commands, clock policy, presentation policy, UI, playout readiness, and user-facing sessions.

## Scope

Implemented in this milestone:

- media source role model
- preview profile model
- bounded queue-limit model
- capability flags for sample-clock readiness and source roles
- runtime state/event skeleton
- frame-to-original-audio sample range mapping
- PCM block coverage checks per original MXF audio track
- qgs-test acceptance summary for Sony FX6 sample 002

Not implemented:

- QNC Broadcast Player
- QNC UI integration
- real playback loop
- QGS-owned final playback clock
- real-time audio output
- speaker output
- display output
- waveform UI
- editing
- export/render
- resampling
- drift correction
- proxy AAC primary path
- full interlaced field rendering

No QNC crates are imported into QGS.

## Media Roles

The contract uses explicit source roles:

- `OriginalAuthoritativeAudio`
- `ProxyPreviewVideo`
- `OriginalFinishingMedia`
- `ProxyAudioDiagnosticOnly`

For the QNC Journalist workflow, original MXF audio is authoritative and proxy MP4 video is used for responsive preview/edit performance. Proxy MP4 AAC is not used as primary audio.

`journalist-50i-preview` is a preview profile. It is not full interlaced rendering, does not implement field cadence, and does not produce interlaced output.

## Broadcast Runtime Types

`qgs-media-runtime` now includes small backend-neutral contract types:

- `BroadcastMediaSourceRole`
- `BroadcastPreviewProfile`
- `BroadcastRuntimeState`
- `BroadcastRuntimeEvent`
- `BroadcastRuntimeQueueKind`
- `BroadcastRuntimeQueueLimits`
- `BroadcastRuntimeCapabilities`
- `BroadcastRuntimeSessionDescription`
- `AudioRangeCoverage`
- `AvFrameAudioRange`
- `BroadcastRuntimeContractSummary`

The session description validates that the contract is sample-clock aware, preserves original PCM format, preserves track/channel identity, uses proxy video for preview, does not treat proxy audio as primary, and is not UI-dependent.

## Frame To Sample Mapping

For each selected `journalist-50i-preview` frame, QGS computes:

- selected preview frame index
- proxy source presentation index
- normalized proxy frame timestamp
- original-audio start sample
- original-audio sample count
- original-audio start time and duration
- per-track PCM blocks covering the range
- completeness, gaps, and overlaps

The mapping is timestamp-based and normalizes the proxy video timeline to the media presentation origin. It does not assume future media is 50p.

For Sony FX6 sample 002:

- proxy source: 1080p50
- preview profile: `journalist-50i-preview`
- selected preview frames: 53
- selected preview period: 40 ms
- original audio sample rate: 48 kHz
- audio samples per selected preview period: 1,920

## Acceptance Result

Command:

```sh
cargo run -q -p qgs-test -- --broadcast-runtime-contract <original-mxf> <proxy-mp4>
```

Observed privacy-safe result for Sony FX6 sample 002:

```text
Audio source: original MXF
Video source: proxy MP4
Proxy AAC: not used
Preview profile: journalist-50i-preview
Clock owner: future QNC application/runtime policy, not QGS UI
Contract owner: QGS backend-neutral Broadcast Runtime
Proxy video: 1920x1080 H.264 High 8-bit Cs420 source_frames=106 selected_preview_frames=53
Original audio: tracks=4 sample_rate=48000Hz blocks=424 duration=2.120s
Frames checked: 53 complete=53 incomplete=0 outside_audio_range=0 max_av_delta_ms=0.000
Suitable for Broadcast Runtime contract: yes
```

Representative frame mappings:

```text
preview_frame=0  source_presentation=0   audio_samples=0..1920       complete=yes gaps=0 overlaps=0
preview_frame=26 source_presentation=52  audio_samples=49920..51840  complete=yes gaps=0 overlaps=0
preview_frame=52 source_presentation=104 audio_samples=99840..101760 complete=yes gaps=0 overlaps=0
```

Each representative range is covered by all four original MXF mono PCM tracks.

## Relationship To Steps 20A-20C

Step 20A established original MXF audio metadata and timeline modeling.

Step 20B extracted original MXF LPCM payload packets without using proxy AAC.

Step 20C converted extracted PCM packets into runtime mono-track PCM blocks with no gaps or overlaps.

Step 20D maps selected proxy-video preview frames to authoritative original-audio sample ranges over those runtime PCM blocks.

## Limitations

This is a contract and acceptance proof for QGS backend behavior. It does not decide final QNC application architecture, real transport commands, speaker/audio-device behavior, video display behavior, or sync correction policy.

The current Sony FX6 sample uses four mono 24-bit PCM tracks and a clean 1080p50 proxy. Broader media families may require additional source role metadata, edit-list handling, mixed-rate validation, or audio block layout variants.

## Next Steps

Recommended future milestones:

- QNC application-facing media snapshot/session contract
- transport command and runtime event API design
- real audio output/device-clock boundary
- audio/video sync policy over the sample-clock-aware contract
- waveform-analysis input from PCM blocks
- later display/presentation integration

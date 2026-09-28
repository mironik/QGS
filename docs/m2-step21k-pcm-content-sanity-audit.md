# M2 Step 21K — PCM Content Sanity Audit

Step 21K pauses new playback feature work and audits the QGS original-audio
content path after the manual PipeWire result was clarified as slight hum/buzz
rather than recognizable original audio content.

Current evidence remains conservative, with a later manual listening follow-up:

- native PipeWire stream creation works
- native PipeWire buffer submission works
- drain completion works
- voice-like original-audio content was later heard in both tested versions
- the second tested version sounded clearer and seemed present on both channels
- channel routing and gain are still not certified
- PCM content correctness is supported by audit data, but not fully certified by
  listening
- channel routing is not certified
- `AudioDeviceVerified` remains no

## Command

```bash
cargo run -q -p qgs-test -- --pipewire-audio-content-audit <original-mxf> <proxy-mp4>
```

Observed command:

```bash
cargo run -q -p qgs-test -- --pipewire-audio-content-audit "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF" "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560S03.MP4"
```

The command does not require PipeWire playback. It audits extracted original MXF
PCM, runtime PCM blocks, 24-bit interpretation, f32 conversion, interleaving,
runtime prepared payload construction, and PipeWire buffer geometry.

It also writes a diagnostic WAV:

```text
target/qgs-audio-audit/qgs-original-audio-audit.wav
```

The WAV is diagnostic only. It is derived from original MXF PCM, not proxy AAC.

## Manual Listening Follow-up

After the Step 21K audit, the diagnostic audio/output was manually listened to
again. The updated observation was:

- something like voices was audible
- voice-like content was heard in both tested versions
- the second version sounded clearer/better
- the second version seemed present on both channels
- the first version seemed mostly on one channel

This supports that the QGS decoded audio content is not merely hum/buzz. It also
fits the audit result that real nonzero original MXF PCM content flows through
the extraction, conversion, and buffer geometry path.

This does not yet certify channel routing, monitor folding, production output,
or full audio-device behavior. `AudioDeviceVerified` remains no. The result
instead points the next diagnostic focus toward monitor routing, gain, source
range selection, and explicit stereo monitor output.

Recommended follow-up:

```text
M2 Step 21L — Monitor Routing / Stereo Diagnostic Boundary
```

Planned focus for Step 21L:

- find or choose a more audible original-audio range
- produce/report explicit stereo monitor output
- compare 4-channel output vs stereo monitor output
- report channel mapping/routing clearly
- keep original MXF audio authoritative
- keep proxy AAC diagnostic-only or unused

## Source And Metadata

QGS observed:

- audio source: original MXF
- proxy AAC: not used
- tracks: 4 mono tracks
- sample rate: 48000 Hz
- bit depth: 24-bit
- block count: 424
- blocks per track: 106
- block size: 2880 bytes
- samples per block: 960

`ffprobe` also reports four original MXF audio streams as `pcm_s24le`, 48000 Hz,
mono, 24-bit. This supports the QGS selected interpretation:

- signed 24-bit little-endian
- f32 conversion denominator: 8388608.0
- no f32 values outside `[-1.0, 1.0]`

The qgs-mxf descriptor model does not currently expose a separate byte-order
field, so the command prints that the MXF byte order is not explicitly modeled
there. The external FFmpeg metadata is a diagnostic cross-check only.

## Track Statistics

The audit analyzed the first 48000 samples per track.

| Track | Channel | Min | Max | RMS f32 | Peak f32 | Zero Samples | Likely Silent |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 3 | 0 | -11576 | 12082 | 0.000348 | 0.001440 | 0.552% | no |
| 4 | 1 | -160833 | 155285 | 0.002573 | 0.019173 | 0.133% | no |
| 5 | 2 | -2746 | 2574 | 0.000077 | 0.000327 | 1.065% | yes |
| 6 | 3 | -378485 | 312892 | 0.007375 | 0.045119 | 0.000% | no |

The first 40 ms runtime payload is also low level:

- f32 min: -0.011710
- f32 max: 0.013849
- f32 RMS: 0.002411
- nonzero: yes

This makes the slight hum/buzz report plausible without requiring a runtime
payload regression. Several tracks are very low amplitude, and channel 2 is
near-silent in the analyzed range.

## PCM Interpretation

For track 3, channel 0, the first 12 bytes were:

```text
d9 0d 00 ce 1e 00 1b 10 00 53 09 00
```

Interpretations:

- signed 24-bit little-endian: `[3545, 7886, 4123, 2387]`
- signed 24-bit big-endian: `[-2552576, -3269120, 1773568, 5441792]`
- unsigned 24-bit little-endian: `[3545, 7886, 4123, 2387]`

For track 4, channel 1, the first samples include negative values:

- raw bytes: `89 79 ff 93 81 ff af 7a ff b9 82 ff`
- signed 24-bit little-endian: `[-34423, -32365, -34129, -32071]`
- unsigned 24-bit little-endian: large values near `0xFFxxxx`

This confirms why signed interpretation matters. The QGS sign extension path is
consistent with expected signed 24-bit little-endian PCM.

## Path Comparison

The audit compared:

1. Step 21H standalone segment path:
   - sequential original PCM segment
   - first 40 ms, start sample 0

2. Step 21I/21J runtime prepared payload path:
   - `ProxyPreview`
   - prepared audio slot 0
   - first 40 ms, start sample 0

Observed comparison:

- segment source blocks per track: 2
- runtime source blocks per track: 2
- segment output frames: 1920
- runtime output frames: 1920
- segment output bytes: 30720
- runtime output bytes: 30720
- same source range f32 bytes identical: yes

This is the key audit finding: the standalone segment path and runtime prepared
payload path produce byte-identical f32 output for the same original MXF sample
range. The latest runtime/prepared-payload path did not introduce a conversion
or interleaving divergence for this range.

## PipeWire Buffer Geometry

For f32 interleaved 4-channel audio:

- bytes per sample: 4
- channels: 4
- bytes per frame: 16
- 960 frames: 15360 bytes
- 1920 frames: 30720 bytes
- 48000 frames: 768000 bytes

The native PipeWire submission code writes:

- data plane: first plane
- chunk offset: 0
- chunk stride: `channels * 4`
- chunk size: copied byte count

For the audited buffers, the geometry matches the expected values.

## FFmpeg Comparison

`ffprobe` reports:

- four audio streams
- codec: `pcm_s24le`
- sample rate: 48000 Hz
- channels: mono
- duration: 2.120 s
- bits per sample: 24

An `ffmpeg` `astats` pass over the first second reports levels consistent with
QGS after accounting for FFmpeg's s32 representation of 24-bit PCM. This
supports the QGS signed little-endian interpretation, but it is still only a
diagnostic comparison.

## Likely Root Cause

No root cause was found in the QGS extraction, 24-bit sign extension, f32
scaling, interleaving, runtime prepared payload construction, or PipeWire byte
geometry for the audited first 40 ms.

The issue is narrowed:

- the original sample's first second is low amplitude on several tracks
- one track is near-silent
- standalone segment and runtime payload output match exactly for the same
  sample range
- PipeWire buffer geometry matches the configured f32 interleaved 4-channel
  format

The remaining likely causes are outside the proven byte path:

- the selected source range may be too quiet for clear recognition
- default output may fold or route 4-channel audio unexpectedly
- output level may be too low
- channel mapping may not match the physical speaker setup
- the diagnostic signal may need a louder monitored range or explicit
  track-pair monitor mode, while preserving original PCM as runtime truth

## Remaining Unknowns

- Whether the diagnostic WAV is recognizable when played through a known-good
  player.
- Whether a later/louder source range exists in the original MXF.
- Whether PipeWire/default sink channel routing is folding or discarding some
  channels.
- Whether QGS needs a diagnostic monitor path, such as tracks 1/2 stereo, for
  audibility checks. That would remain a device-boundary diagnostic, not a
  runtime truth change.

## Next Recommended Step

Before adding scheduling or A/V sync, continue with Step 21L: choose a more
audible original-audio range and add explicit monitor routing/stereo diagnostic
output. Do not upgrade `AudioDeviceVerified`, channel certification, realtime
playback, or production output claims until the device path and routing policy
are understood.

# M2 Step 21L — Audio Audit Corpus / Mironik 2002

Step 21L corrects the practical listening corpus for QGS audio-content and
monitor-routing diagnostics.

This is not a new playback feature. It does not implement full Broadcast Player
playback, realtime scheduling, A/V sync, video output, production routing
policy, QNC UI integration, or an `AudioDeviceVerified` upgrade.

## Correction

Step 21K was technically useful because it audited:

- original MXF PCM extraction
- signed 24-bit little-endian interpretation
- f32 device-boundary conversion
- runtime-prepared payload equality against the standalone segment path
- PipeWire f32 interleaved buffer geometry

However, Step 21K used the shortest practical sample:

- `Mironik 1560.MXF`
- `Mironik 1560S03.MP4`

That sample remains useful for boundary checks, but it is not the right
canonical listening source. It is short and contains low-level or sparse audio
in the audited ranges.

The preferred content-audibility and monitor-routing corpus is now:

- `Mironik 2002` original MXF
- matching `Mironik 2002` proxy MP4

The expectation is that Mironik 2002 contains more useful long-form original
audio material, likely on tracks 1 and 2, and possibly tracks 3 and 4.

## Command

The existing audit command now supports arbitrary original/proxy pairs and
bounded range selection:

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-content-audit <original-mxf> <proxy-mp4> \
  --start-ms 0 \
  --duration-ms 1000 \
  --output-dir target/qgs-audio-audit
```

For long-form MXF files such as Mironik 2002, use explicit bounded extraction:

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-content-audit \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002.MXF" \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002S03.MP4" \
  --start-ms 0 \
  --duration-ms 1000 \
  --output-dir "target/qgs-audio-audit/mironik-2002-start0"
```

This path uses a streaming MXF PCM audio index and reads only the PCM packets
overlapping the selected sample range. It does not load the full MXF into
memory, and it preserves the 64 MiB full-file guard used by older full-parse
paths.

To select the loudest contiguous diagnostic range by RMS:

```bash
cargo run -q -p qgs-test -- \
  --pipewire-audio-content-audit <original-mxf> <proxy-mp4> \
  --find-loudest-range-ms 1000 \
  --duration-ms 1000 \
  --output-dir target/qgs-audio-audit
```

The loudest-range scan is diagnostic only. It does not alter runtime behavior.

As of the large-MXF fix, `--find-loudest-range-ms` remains available for small
files but is intentionally not implemented for large MXF files. On Mironik 2002
it fails with:

```text
loudest-range scan for large MXF is not implemented yet; use --start-ms with --duration-ms for bounded extraction
```

This is deliberate. It avoids faking a loudest-range result and avoids loading a
large MXF or unbounded PCM into memory. A future streaming loudest-range scan can
upgrade this.

## Diagnostic WAVs

For the selected range, the command writes diagnostic WAVs under the selected
output directory using a sanitized source stem and explicit range label.

Expected output shape for Mironik 2002:

```text
target/qgs-audio-audit/Mironik-2002-original-4ch-f32-start000000ms-dur001000ms.wav
target/qgs-audio-audit/Mironik-2002-stereo-track12-f32-start000000ms-dur001000ms.wav
target/qgs-audio-audit/Mironik-2002-stereo-loudest-pair-f32-start000000ms-dur001000ms.wav
```

If tracks 3/4 contain useful non-silent audio, the command may also write:

```text
target/qgs-audio-audit/Mironik-2002-stereo-track34-f32-start000000ms-dur001000ms.wav
```

These WAVs are diagnostics. They do not replace the runtime truth:

- original MXF PCM remains authoritative
- proxy AAC is not used as runtime audio
- stereo monitor WAVs are diagnostic monitor views, not production routing

## Reported Data

The command reports:

- selected original path
- original MXF file size
- selected proxy path
- proxy AAC usage: no
- bounded extraction: yes
- full MXF loaded into memory: no
- selected start/duration
- source duration if known
- track count
- blocks per track
- samples per block
- analyzed sample range
- per-track min/max/RMS/peak/DC offset
- zero-sample and clipping ratios
- likely-silent status
- relative level in dBFS
- suggested diagnostic monitor pair
- diagnostic WAV paths

For the Mironik 2002 start-0 bounded audit, QGS observed:

- original MXF file size: 1,560,829,488 bytes
- source duration: 203.880 s
- audio packet index entries: 40,776
- selected range: 0..48,000 samples
- blocks extracted for selected range: 200
- bytes extracted for selected range: 576,000
- track 1 RMS: -26.71 dBFS
- track 2 RMS: -44.34 dBFS
- track 3 RMS: -85.04 dBFS, likely silent
- track 4 RMS: -17.45 dBFS
- loudest two tracks for monitoring: track 4 / track 1

Diagnostic WAVs were written to:

```text
target/qgs-audio-audit/mironik-2002-start0/Mironik-2002-original-4ch-f32-start000000ms-dur001000ms.wav
target/qgs-audio-audit/mironik-2002-start0/Mironik-2002-stereo-track12-f32-start000000ms-dur001000ms.wav
target/qgs-audio-audit/mironik-2002-start0/Mironik-2002-stereo-track34-f32-start000000ms-dur001000ms.wav
target/qgs-audio-audit/mironik-2002-start0/Mironik-2002-stereo-loudest-pair-f32-start000000ms-dur001000ms.wav
```

## Interpretation

For content-audibility work, use Mironik 2002 first. The shorter Mironik 1560
sample should be kept for fast technical regression checks, but it should not be
the only basis for diagnosing monitor routing, source audibility, or channel
selection.

The expected diagnostic flow is:

1. Run the loudest-range audit on Mironik 2002.
2. Listen to the 4-channel diagnostic WAV through a known-good player if
   possible.
3. Listen to the stereo track 1/2 monitor WAV.
4. Compare with the loudest-pair monitor WAV.
5. Decide which channels are useful for a future monitor-output diagnostic.

This still does not certify channel routing, speaker calibration, production
output, realtime playback, A/V sync, or full audio-device verification.

## Local Availability

Mironik 2002 is now present in the local `/home/miro/QGS-media-tests` corpus and
the explicit start/duration bounded audit has been run successfully without the
old `FileTooLarge` failure.

## Next Step

The next useful diagnostic remains:

```text
M2 Step 21L — Monitor Routing / Stereo Diagnostic Boundary
```

Using the corrected Mironik 2002 corpus, that follow-up should choose an audible
range, compare 4-channel output with explicit stereo monitor output, and report
the routing clearly while keeping original MXF audio authoritative and proxy AAC
unused or diagnostic-only.

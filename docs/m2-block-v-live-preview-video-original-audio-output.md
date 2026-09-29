# M2 Block V - Live Preview Video + Original Audio Output

Block V makes the existing QGS Broadcast Player live command produce practical
output while it runs:

- visible proxy-video preview artifacts
- original-MXF-derived PipeWire audio chunks

It does not add QNC UI, IPC, export/render, a Wayland/Vulkan presenter, a
second player state machine, production audio routing, realtime certification,
or A/V sync certification.

## Baseline From Blocks T/U

The live command remains:

```bash
--qgs-broadcast-player-live <original-mxf> <proxy-mp4>
```

It still drives:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

Block U stdin controls remain available:

- `pause`
- `play` / `resume`
- `seek <frame>`
- `status`
- `stop`
- `quit`
- `help`

## Video Output Mode

Implemented mode:

```text
--video-output preview-files
```

This mode uses the existing proxy MP4 decode/GPU/readback path and writes real
proxy frame pixels to:

```text
target/qgs-live-preview/frame_000000.ppm
target/qgs-live-preview/frame_000001.ppm
...
target/qgs-live-preview/latest.ppm
target/qgs-live-preview/latest.json
```

`latest.json` reports public-safe live state:

- current frame
- media time
- original-audio sample range
- prepared window
- source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- proxy AAC authoritative: false
- real display: no
- visual verified: false
- private path exposed: false

This is visible preview artifact output, not a real display presenter.

`--video-output preview-window` is reserved and currently reports unsupported.
QNC OS display direction remains Wayland + Vulkan, not X11.

## Audio Output Mode

Implemented modes:

```text
--audio-output pipewire-monitor
--audio-output pipewire-4mono
```

Both modes use original MXF PCM as the source. Proxy AAC is not used.

The live loop takes the current runtime audio sample range from the QNC control
surface snapshot, expands it to the configured preview cadence, extracts the
matching original MXF PCM range, converts only at the PipeWire device boundary,
and submits a bounded f32 interleaved buffer.

For current device-boundary stability, each PipeWire submission is capped to at
most five logical video frames of original-audio samples. `--preview-every`
still controls when the live loop emits output, but the submitted audio chunk
stays bounded even when preview files are written less frequently.

Source truth remains:

- original MXF audio is authoritative
- four discrete mono lanes remain addressable
- proxy AAC is non-authoritative

`pipewire-monitor` is a practical desktop monitor output label. It does not
certify production routing, physical channel mapping, or AudioDeviceVerified.

`pipewire-4mono` submits tracks 1..4 as channels 1..4 at the device boundary.
It still does not certify physical channel mapping or production routing.

## Command Options

New options:

| Option | Default | Meaning |
| --- | --- | --- |
| `--video-output none|preview-files|preview-window` | `preview-files` | Preview output mode. `preview-window` is not implemented yet. |
| `--audio-output none|pipewire-monitor|pipewire-4mono` | `none` | Original-MXF-derived audio output mode. |
| `--output-dir <path>` | `target/qgs-live-preview` | Preview artifact directory. |
| `--preview-every <n>` | `5` | Emit preview/audio output every `n` logical frames. |

Existing options remain:

- `--start-frame`
- `--status-every`
- `--max-frames`
- `--seek-frame`
- `--view compact|detailed`
- `--pace logical|wall`
- `--no-interactive`

## Playhead Coupling

Video output uses the live frame from the runtime snapshot.

Audio output uses the live original-audio sample range from the runtime
snapshot.

Pause stops further live-loop output submissions. Resume continues from the
current runtime position. Seek changes the next output frame/sample range.

The implementation is intentionally not a realtime scheduler. It submits
bounded chunks for observability and usefulness.

## Example

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <original-mxf> <proxy-mp4> \
  --max-frames 80 --status-every 10 \
  --video-output preview-files \
  --audio-output pipewire-monitor \
  --view compact
```

Compact output now includes output status:

```text
tick=000000 frame=0 t=00:00:00.000 audio=[0..960) window=[0..6) buffer=ready video_out=written audio_out=submitted-monitor
```

## Sample 002 Result

Command shape:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <sample-002-original-mxf> <sample-002-proxy-mp4> \
  --max-frames 80 --status-every 10 \
  --video-output preview-files \
  --audio-output pipewire-monitor \
  --view compact --no-interactive
```

Observed result:

- run result: completed
- frames processed: 80
- video output: `preview-files`
- preview artifact: `target/qgs-live-preview/latest.ppm`
- preview metadata: `target/qgs-live-preview/latest.json`
- audio output: `pipewire-monitor`
- audio submitted: yes
- audio submissions: 16
- audio bytes copied: 1228800
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real `FramePresented` claim: no

The observed `latest.json` carried public-safe state, including proxy MP4 video,
original MXF audio, `proxy_aac_authoritative=false`, `real_display=no`, and
`private_path_exposed=false`.

Detailed mode was also run with a short 10-frame bound. It preserved the richer
runtime sections and reported `video output: written` and
`audio output: submitted-monitor` on live ticks.

## Mironik 2002 Result

Command shape:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <mironik-2002-original-mxf> <mironik-2002-proxy-mp4> \
  --max-frames 50 --status-every 25 --preview-every 25 \
  --video-output preview-files \
  --audio-output pipewire-monitor \
  --view compact --no-interactive
```

Observed result:

- source frames: 10194 at 50 fps
- run result: completed
- frames processed: 50
- video output: `preview-files`
- audio output: `pipewire-monitor`
- audio submitted: yes
- audio submissions: 2
- audio bytes copied: 153600
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real `FramePresented` claim: no

The Mironik run uses the same bounded audio submission cap. With
`--preview-every 25`, output is emitted every 25 logical frames, while each
PipeWire audio chunk remains capped to five frames of original-audio samples.

## Interactive Behavior

A wall-paced interactive run accepted:

- `status`
- `pause`
- `seek 50`
- `play`
- `quit`

`pause` stopped live progression at the current frame, `seek 50` updated the
prepared window and original-audio sample range, `play` resumed from frame 50,
and `quit` stopped and unloaded the source. The run ended with private path
exposure still reported as `no`.

## Non-Claims

Even when preview files and PipeWire submissions succeed:

- real display: `NotImplemented`
- QNC OS display target: Wayland + Vulkan
- X11: legacy/non-target only
- visual verified: no
- realtime verified: no
- production audio verified: no
- A/V sync verified: no
- real `FramePresented`: no
- AudioDeviceVerified: no
- channel certification: no
- production routing: no
- proxy AAC used: no

## Limitations

- Preview output is file-based, not a window.
- Preview files are bounded by `--max-frames` and `--preview-every`.
- PipeWire audio submission is chunked and bounded, not continuous realtime
  playback.
- PipeWire chunks are capped for live diagnostics and are not a production
  audio scheduling policy.
- Pause/resume/seek affect future submissions, but no certified A/V sync policy
  exists yet.
- Physical speaker channel mapping is not certified.

## Recommended Next Block

Recommended next milestone:

```text
M2 Block W - Live Output Tuning / Usability
```

If the next priority is a real display path instead, use:

```text
M2 Block W - Minimal Wayland+Vulkan Preview Window
```
